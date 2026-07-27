use std::{net::SocketAddr, path::PathBuf, time::Duration};

use minijam_compiler_api::{CompilerConfig, CompilerService};

#[tokio::main]
async fn main() {
    let bind: SocketAddr = std::env::var("MINIJAM_COMPILER_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8081".into())
        .parse()
        .expect("valid bind address");
    let repository =
        PathBuf::from(std::env::var("MINIJAM_REPOSITORY").unwrap_or_else(|_| ".".into()));
    let image = std::env::var("MINIJAM_COMPILER_IMAGE")
        .unwrap_or_else(|_| "minijam-compiler:stage0".into());
    let service = CompilerService::new(CompilerConfig {
        repository,
        image,
        timeout: Duration::from_secs(30),
        concurrency: 2,
    });
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .expect("bind compiler API");
    axum::serve(listener, service.router())
        .await
        .expect("serve compiler API");
}
