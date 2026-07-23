// SPDX-License-Identifier: Apache-2.0

use std::{thread, time::Duration};

use clap::Parser;
use minijam_worker::WorkerConfig;

#[derive(Debug, Parser)]
#[command(name = "minijam-worker")]
#[command(about = "MiniJAM stage-0 worker daemon")]
struct Cli {
    #[arg(long, default_value = "ws://127.0.0.1:9944")]
    rpc_url: String,

    #[arg(long)]
    key: Option<String>,

    #[arg(long, default_value_t = 1_000)]
    poll_interval_ms: u64,

    #[arg(long, default_value = "http://127.0.0.1:8080")]
    ipfs_gateway: String,

    #[arg(long, default_value_t = 30)]
    request_timeout_secs: u64,

    #[arg(long, default_value_t = 16_777_216)]
    max_bundle_bytes: u64,

    #[arg(long)]
    once: bool,
}

impl From<Cli> for WorkerConfig {
    fn from(cli: Cli) -> Self {
        Self {
            rpc_url: cli.rpc_url,
            key: cli.key,
            poll_interval: Duration::from_millis(cli.poll_interval_ms),
            ipfs_gateway: cli.ipfs_gateway,
            request_timeout: Duration::from_secs(cli.request_timeout_secs),
            max_bundle_bytes: cli.max_bundle_bytes,
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let once = cli.once;
    let config = WorkerConfig::from(cli);
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
