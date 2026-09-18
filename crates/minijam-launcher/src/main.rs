// SPDX-License-Identifier: Apache-2.0

use std::{
    io,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use clap::Parser;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::{Child, Command},
    time::{sleep, Instant},
};

const NODE_RPC_PORT: u16 = 9944;
const FORMAL_RPC_PORT: u16 = 8080;
const WORKER_HEALTH_PORT: u16 = 8082;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const LOCAL_RELAYER_SEED: &str =
    "0x9292929292929292929292929292929292929292929292929292929292929292";

#[derive(Debug, Parser)]
#[command(name = "minijam")]
#[command(about = "Run the canonical MiniJAM Stage-1 local network")]
struct Cli {
    /// Start the deterministic Stage-1 local network.
    #[arg(long)]
    dev: bool,
}

struct ManagedChild {
    name: &'static str,
    child: Child,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("minijam: {error}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    if !cli.dev {
        return Err("only `--dev` is currently published; mainnet is not published".into());
    }

    let data_dir = data_dir()?;
    std::fs::create_dir_all(data_dir.join("node"))
        .map_err(|error| format!("failed to create node data directory: {error}"))?;
    std::fs::create_dir_all(data_dir.join("worker"))
        .map_err(|error| format!("failed to create worker data directory: {error}"))?;
    std::fs::create_dir_all(data_dir.join("bundles"))
        .map_err(|error| format!("failed to create bundle data directory: {error}"))?;

    let mut children = Vec::new();
    children.push(spawn_node(&data_dir)?);
    if let Err(error) = wait_for_node(&mut children[0]).await {
        shutdown_children(&mut children).await;
        return Err(error);
    }

    children.push(spawn_formal_rpc(&data_dir)?);
    if let Err(error) = wait_for_formal_rpc(&mut children[1]).await {
        shutdown_children(&mut children).await;
        return Err(error);
    }

    children.push(spawn_worker(&data_dir)?);
    if let Err(error) = wait_for_worker(&mut children[2]).await {
        shutdown_children(&mut children).await;
        return Err(error);
    }

    eprintln!(
        "MiniJAM local ready: node=:{NODE_RPC_PORT} formal-rpc=:{FORMAL_RPC_PORT} worker-health=:{WORKER_HEALTH_PORT}"
    );

    let outcome = tokio::select! {
        signal = shutdown_signal() => {
            signal.map(|_| ()).map_err(|error| error.to_string())
        }
        result = monitor_children(&mut children) => result,
    };
    shutdown_children(&mut children).await;
    outcome
}

fn data_dir() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("MINIJAM_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    let standard = PathBuf::from("/data");
    if std::fs::create_dir_all(&standard).is_ok() {
        return Ok(standard);
    }

    let fallback = std::env::current_dir()
        .map_err(|error| format!("failed to resolve current directory: {error}"))?
        .join(".minijam-data");
    std::fs::create_dir_all(&fallback)
        .map_err(|error| format!("failed to create fallback data directory: {error}"))?;
    Ok(fallback)
}

fn binary(name: &str) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("MINIJAM_BIN_DIR") {
        candidates.push(PathBuf::from(path).join(name));
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            candidates.push(parent.join(name));
        }
    }
    candidates.push(PathBuf::from("/usr/local/bin").join(name));
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug")
            .join(name),
    );
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release")
            .join(name),
    );

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("could not find {name}; install the MiniJAM component binaries together with minijam"))
}

fn spawn_node(data_dir: &Path) -> Result<ManagedChild, String> {
    let node = binary("minijam-node")?;
    let child = Command::new(node)
        .args([
            "--dev",
            "--validator",
            "--force-authoring",
            "--rpc-port=9944",
            "--unsafe-rpc-external",
            "--rpc-methods=safe",
            "--rpc-cors=all",
        ])
        .arg(format!("--base-path={}", data_dir.join("node").display()))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("failed to start node: {error}"))?;
    Ok(ManagedChild {
        name: "node",
        child,
    })
}

fn spawn_formal_rpc(data_dir: &Path) -> Result<ManagedChild, String> {
    let formal_rpc = binary("minijam-formal-rpc")?;
    let child = Command::new(formal_rpc)
        .env("MINIJAM_RPC_URL", "ws://127.0.0.1:9944")
        .env("MINIJAM_FORMAL_RPC_BIND", "0.0.0.0:8080")
        .env("MINIJAM_BUNDLE_DIR", data_dir.join("bundles"))
        .env("MINIJAM_RELAYER_URI", LOCAL_RELAYER_SEED)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("failed to start formal RPC: {error}"))?;
    Ok(ManagedChild {
        name: "formal-rpc",
        child,
    })
}

fn spawn_worker(data_dir: &Path) -> Result<ManagedChild, String> {
    let worker = binary("minijam-worker")?;
    let child = Command::new(worker)
        .args([
            "--rpc-url=http://127.0.0.1:9944",
            "--worker-id=0",
            "--key=//Bob",
            "--ipfs-gateway=http://127.0.0.1:8080",
            "--health-bind=0.0.0.0:8082",
            "--submit-candidates",
            "--submit-support-votes",
        ])
        .arg(format!(
            "--state-db={}",
            data_dir.join("worker/state.toml").display()
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("failed to start worker: {error}"))?;
    Ok(ManagedChild {
        name: "worker",
        child,
    })
}

async fn wait_for_node(child: &mut ManagedChild) -> Result<(), String> {
    wait_for_http(child, NODE_RPC_PORT, node_rpc_request()).await
}

async fn wait_for_formal_rpc(child: &mut ManagedChild) -> Result<(), String> {
    wait_for_http(child, FORMAL_RPC_PORT, http_get_request("/health/ready")).await
}

async fn wait_for_worker(child: &mut ManagedChild) -> Result<(), String> {
    wait_for_http(child, WORKER_HEALTH_PORT, http_get_request("/health/ready")).await
}

async fn wait_for_http(child: &mut ManagedChild, port: u16, request: String) -> Result<(), String> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        if let Some(status) = child
            .child
            .try_wait()
            .map_err(|error| format!("failed to inspect {}: {error}", child.name))?
        {
            return Err(format!(
                "{} exited before port {port} became ready ({status})",
                child.name
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for {} on port {port}",
                child.name
            ));
        }
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)).await {
            if stream.write_all(request.as_bytes()).await.is_ok() && stream.flush().await.is_ok() {
                let mut response = [0_u8; 8192];
                if let Ok(length) = stream.read(&mut response).await {
                    let response = String::from_utf8_lossy(&response[..length]);
                    if response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200")
                    {
                        return Ok(());
                    }
                }
            }
        }
        sleep(Duration::from_millis(250)).await;
    }
}

fn http_get_request(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
}

fn node_rpc_request() -> String {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"system_health","params":[]}"#;
    format!(
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

async fn monitor_children(children: &mut [ManagedChild]) -> Result<(), String> {
    loop {
        for child in children.iter_mut() {
            if let Some(status) = child
                .child
                .try_wait()
                .map_err(|error| format!("failed to inspect {}: {error}", child.name))?
            {
                return Err(format!("{} exited unexpectedly ({status})", child.name));
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
}

async fn shutdown_children(children: &mut [ManagedChild]) {
    for child in children.iter_mut() {
        terminate(&mut child.child);
    }

    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    loop {
        let mut all_stopped = true;
        for child in children.iter_mut() {
            match child.child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => all_stopped = false,
                Err(_) => {}
            }
        }
        if all_stopped || Instant::now() >= deadline {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    for child in children.iter_mut() {
        if child.child.try_wait().ok().flatten().is_none() {
            let _ = child.child.kill().await;
        }
    }
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            // SAFETY: pid is returned by the live child process we spawned.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
    }
    #[cfg(not(unix))]
    let _ = child;
}

async fn shutdown_signal() -> io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}
