#[tokio::main]
async fn main() {
    if let Err(error) = minijam_formal_rpc::run_from_env().await {
        eprintln!("minijam formal RPC failed: {error}");
        std::process::exit(1);
    }
}
