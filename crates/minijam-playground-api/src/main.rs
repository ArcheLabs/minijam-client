use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use minijam_playground_api::{Playground, PlaygroundConfig};
use sp_core::{sr25519, Pair};

#[tokio::main]
async fn main() {
    let bind: SocketAddr = std::env::var("MINIJAM_PLAYGROUND_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()
        .expect("valid bind");
    let database = PathBuf::from(
        std::env::var("MINIJAM_PLAYGROUND_DB").unwrap_or_else(|_| "playground.sqlite".into()),
    );
    let compiler_url =
        std::env::var("MINIJAM_COMPILER_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into());
    let bundle_dir =
        PathBuf::from(std::env::var("MINIJAM_BUNDLE_DIR").unwrap_or_else(|_| "bundles".into()));
    let rpc_url = std::env::var("MINIJAM_RPC_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".into());
    let signer_uri = std::env::var("MINIJAM_RELAYER_URI").expect("MINIJAM_RELAYER_URI");
    let signer = sr25519::Pair::from_string(&signer_uri, None).expect("valid relayer URI");
    let chain = Arc::new(
        minijam_chain_client::MiniJamChainClient::connect(rpc_url, signer, Duration::from_secs(15))
            .await
            .expect("connect MiniJAM chain"),
    );
    let allocation_signer_uri =
        std::env::var("MINIJAM_ALLOCATION_RELAYER_URI").unwrap_or_else(|_| signer_uri.clone());
    let allocation_signer = sr25519::Pair::from_string(&allocation_signer_uri, None)
        .expect("valid allocation relayer URI");
    let allocation_chain = Arc::new(
        minijam_chain_client::MiniJamChainClient::connect(
            std::env::var("MINIJAM_RPC_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".into()),
            allocation_signer,
            Duration::from_secs(15),
        )
        .await
        .expect("connect MiniJAM allocation chain"),
    );
    let genesis_hash = match std::env::var("MINIJAM_GENESIS_HASH") {
        Ok(value) if !value.eq_ignore_ascii_case("rpc") => decode_hash(&value),
        _ => chain
            .genesis_hash()
            .await
            .expect("read genesis hash from MiniJAM chain"),
    };
    let playground = Playground::open(
        &database,
        PlaygroundConfig {
            genesis_hash,
            compiler_url,
            bundle_dir,
        },
    )
    .expect("open playground")
    .with_chain(chain)
    .with_allocation_chain(allocation_chain);
    playground.start_recovery();
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .expect("bind playground");
    axum::serve(listener, playground.router())
        .await
        .expect("serve playground");
}

fn decode_hash(value: &str) -> [u8; 32] {
    let value = value.strip_prefix("0x").unwrap_or(value);
    assert_eq!(value.len(), 64, "genesis hash must be 32-byte hex");
    let mut output = [0; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("valid genesis hash");
    }
    output
}
