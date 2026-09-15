use std::{path::PathBuf, sync::Arc, time::Duration};

use clap::Parser;
use minijam_worker::{WorkerConfig, WorkerError, WorkerMetrics, WorkerRunner};

#[derive(Debug, Parser)]
#[command(name = "minijam-worker")]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    rpc_url: Option<String>,
    #[arg(long)]
    formal_rpc_url: Option<String>,
    #[arg(long)]
    key: Option<String>,
    #[arg(long)]
    poll_ms: Option<u64>,
    #[arg(long)]
    recovery_db: Option<PathBuf>,
}

fn load_config(cli: &Cli) -> Result<WorkerConfig, String> {
    let mut config = if let Some(path) = &cli.config {
        let contents = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        WorkerConfig::from_toml_str(&contents).map_err(|error| error.to_string())?
    } else {
        WorkerConfig::default()
    };
    if let Some(value) = &cli.rpc_url {
        config.rpc_url = value.clone();
    }
    if let Some(value) = &cli.formal_rpc_url {
        config.formal_rpc_url = value.clone();
    }
    if let Some(value) = &cli.key {
        config.key = Some(value.clone());
    }
    if config.key.is_none() {
        config.key = std::env::var("MINIJAM_WORKER_SEED_FILE")
            .ok()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|value| value.trim().to_owned())
            .or_else(|| std::env::var("MINIJAM_WORKER_KEY").ok());
    }
    if let Some(value) = cli.poll_ms {
        config.poll_interval = Duration::from_millis(value);
    }
    if let Some(value) = &cli.recovery_db {
        config.recovery_db_path = Some(value.clone());
    }
    config
        .validate()
        .map_err(|error| format!("invalid worker configuration: {error:?}"))?;
    Ok(config)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let config = match load_config(&cli) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("minijam worker configuration failed: {error}");
            std::process::exit(2);
        }
    };
    let metrics = Arc::new(WorkerMetrics::new());
    let mut runner = match WorkerRunner::connect(config.clone(), metrics).await {
        Ok(runner) => runner,
        Err(error) => {
            eprintln!("minijam worker startup failed: {error}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "minijam worker started; formal_rpc_url={} poll_ms={}",
        config.formal_rpc_url,
        config.poll_interval.as_millis()
    );
    loop {
        match runner.poll_once().await {
            Ok(true) => eprintln!("minijam worker refined and submitted one package report"),
            Ok(false) => {}
            Err(WorkerError::Http(error)) => eprintln!("minijam worker poll failed: {error}"),
            Err(error) => eprintln!("minijam worker iteration failed: {error}"),
        }
        tokio::select! {
            _ = tokio::time::sleep(config.poll_interval) => {},
            _ = tokio::signal::ctrl_c() => break,
        }
    }
}
