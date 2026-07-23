// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, thread, time::Duration};

use clap::Parser;
use minijam_worker::WorkerConfig;

#[derive(Debug, Parser)]
#[command(name = "minijam-worker")]
#[command(about = "MiniJAM stage-0 worker daemon")]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(long)]
    rpc_url: Option<String>,

    #[arg(long)]
    key: Option<String>,

    #[arg(long)]
    poll_interval_ms: Option<u64>,

    #[arg(long)]
    ipfs_gateway: Option<String>,

    #[arg(long)]
    request_timeout_secs: Option<u64>,

    #[arg(long)]
    max_bundle_bytes: Option<u64>,

    #[arg(long)]
    once: bool,
}

fn build_config(cli: &Cli) -> Result<WorkerConfig, String> {
    let mut config = if let Some(path) = &cli.config {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        WorkerConfig::from_toml_str(&contents)
            .map_err(|error| format!("failed to parse {}: {error}", path.display()))?
    } else {
        WorkerConfig::default()
    };

    if let Some(rpc_url) = &cli.rpc_url {
        config.rpc_url = rpc_url.clone();
    }
    if let Some(key) = &cli.key {
        config.key = Some(key.clone());
    }
    if let Some(poll_interval_ms) = cli.poll_interval_ms {
        config.poll_interval = Duration::from_millis(poll_interval_ms);
    }
    if let Some(ipfs_gateway) = &cli.ipfs_gateway {
        config.ipfs_gateway = ipfs_gateway.clone();
    }
    if let Some(request_timeout_secs) = cli.request_timeout_secs {
        config.request_timeout = Duration::from_secs(request_timeout_secs);
    }
    if let Some(max_bundle_bytes) = cli.max_bundle_bytes {
        config.max_bundle_bytes = max_bundle_bytes;
    }
    Ok(config)
}

fn main() {
    let cli = Cli::parse();
    let once = cli.once;
    let config = match build_config(&cli) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = config.validate() {
        eprintln!("invalid worker config: {error:?}");
        std::process::exit(2);
    }

    eprintln!(
        "minijam worker configured rpc={} ipfs_gateway={} poll_ms={} max_bundle_bytes={}",
        config.rpc_url,
        config.ipfs_gateway,
        config.poll_interval.as_millis(),
        config.max_bundle_bytes
    );

    if once {
        return;
    }

    loop {
        thread::sleep(config.poll_interval);
        eprintln!("minijam worker polling is ready; chain RPC integration is pending");
    }
}
