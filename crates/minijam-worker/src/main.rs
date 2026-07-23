// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, sync::Arc, thread, time::Duration};

use clap::Parser;
use futures::executor::block_on;
use minijam_worker::{
    spawn_prometheus_metrics_server, BlockingHttpBytesClient, BlockingHttpWorkerChainSource,
    WorkerConfig, WorkerMetrics, WorkerRecoveryDb, WorkerRunner,
};
use minijam_worker_engine::{fetch::IpfsGatewayFetcher, MiniJamWorkBundleDecoder};

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
    state_db: Option<PathBuf>,

    #[arg(long)]
    metrics_bind: Option<String>,

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
    if let Some(state_db) = &cli.state_db {
        config.recovery_db_path = Some(state_db.clone());
    }
    if let Some(metrics_bind) = &cli.metrics_bind {
        config.metrics_bind = Some(metrics_bind.clone());
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
        "minijam worker configured rpc={} ipfs_gateway={} poll_ms={} max_bundle_bytes={} state_db={} metrics={}",
        config.rpc_url,
        config.ipfs_gateway,
        config.poll_interval.as_millis(),
        config.max_bundle_bytes,
        config
            .recovery_db_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "disabled".into()),
        config.metrics_bind.as_deref().unwrap_or("disabled")
    );
    let metrics = Arc::new(WorkerMetrics::new());
    if let Some(bind) = &config.metrics_bind {
        if let Err(error) = spawn_prometheus_metrics_server(bind, Arc::clone(&metrics)) {
            eprintln!("failed to start worker metrics endpoint at {bind}: {error}");
            std::process::exit(2);
        }
        eprintln!("minijam worker metrics listening on {bind}");
    }
    let recovery_db = config.recovery_db_path.as_ref().map(WorkerRecoveryDb::new);
    let statuses = if let Some(db) = &recovery_db {
        match db.load_statuses() {
            Ok(statuses) => {
                eprintln!(
                    "minijam worker recovery db loaded path={} statuses={}",
                    db.path().display(),
                    statuses.len()
                );
                statuses
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    } else {
        Default::default()
    };

    let chain = match BlockingHttpWorkerChainSource::new(config.rpc_url.clone()) {
        Ok(chain) => chain,
        Err(error) => {
            eprintln!("{error:?}");
            std::process::exit(2);
        }
    };
    let fetcher = IpfsGatewayFetcher::new(BlockingHttpBytesClient, config.ipfs_gateway.clone());
    let mut runner = WorkerRunner::with_statuses(
        chain,
        fetcher,
        MiniJamWorkBundleDecoder,
        config.max_bundle_bytes,
        statuses,
    );

    if once {
        if let Err(error) = poll_and_persist(&mut runner, &metrics, recovery_db.as_ref()) {
            eprintln!("minijam worker poll failed: {error:?}");
            std::process::exit(1);
        }
        return;
    }

    loop {
        thread::sleep(config.poll_interval);
        if let Err(error) = poll_and_persist(&mut runner, &metrics, recovery_db.as_ref()) {
            eprintln!("minijam worker poll failed: {error:?}");
        }
    }
}

fn poll_and_persist<C, F, D>(
    runner: &mut WorkerRunner<C, F, D>,
    metrics: &WorkerMetrics,
    recovery_db: Option<&WorkerRecoveryDb>,
) -> Result<(), minijam_worker::WorkerError>
where
    C: minijam_worker::WorkerChainSource,
    F: minijam_worker_engine::fetch::ContentFetcher,
    D: minijam_worker_engine::WorkBundleDecoder,
{
    let processed = block_on(runner.poll_once_with_metrics(metrics))?;
    let vote_tasks = block_on(runner.poll_open_vote_tasks_with_metrics(metrics))?;
    if let Some(db) = recovery_db {
        db.save_statuses(runner.statuses())
            .map_err(|error| minijam_worker::WorkerError::Chain(error.to_string()))?;
    }
    eprintln!(
        "minijam worker poll completed processed={} open_vote_tasks={}",
        processed,
        vote_tasks.len()
    );
    Ok(())
}
