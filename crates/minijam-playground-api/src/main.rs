use std::{net::SocketAddr, path::PathBuf};

use minijam_playground_api::{Playground, PlaygroundConfig};

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
    let genesis_hex = std::env::var("MINIJAM_GENESIS_HASH").expect("MINIJAM_GENESIS_HASH");
    let genesis_hash = decode_hash(&genesis_hex);
    let playground = Playground::open(
        &database,
        PlaygroundConfig {
            genesis_hash,
            compiler_url,
        },
    )
    .expect("open playground");
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
