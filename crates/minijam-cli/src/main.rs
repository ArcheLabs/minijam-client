// SPDX-License-Identifier: Apache-2.0

use std::{
    error::Error,
    fmt, fs,
    io::{Read, Write},
    net::TcpStream,
};

use clap::{Parser, Subcommand};
use minijam_protocol::{CanonicalReportBytes, Hash, SystemCommandV2};
use parity_scale_codec::Encode;
use serde_json::{json, Value};

const MINIJAM_PALLET_INDEX: u8 = 7;
const CALL_SUBMIT_REPORT: u8 = 0;
const CALL_PAUSE_EXECUTION: u8 = 1;
const CALL_SUBMIT_PREIMAGE: u8 = 2;
const CALL_SUBMIT_SYSTEM_OP: u8 = 3;
const CALL_SUBMIT_ALLOCATION: u8 = 7;

#[derive(Parser)]
#[command(
    name = "minijam-cli",
    about = "MiniJAM direct-report RPC and call-data utility"
)]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    rpc: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    SubmitReport {
        canonical_report: String,
    },
    SubmitPreimage {
        preimage: String,
    },
    SubmitCreateServiceSystemOp {
        code_hash: String,
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    },
    SubmitAllocation {
        allocation_id: u64,
        target_service: u32,
        amount: u128,
    },
    PauseExecution,
    SubmitRawExtrinsic {
        extrinsic: String,
    },
    GetPackageStatus {
        package_hash: String,
    },
    GetPackageFailure {
        package_hash: String,
    },
    GetExecutionReceipt {
        package_hash: String,
    },
    GetLastExecutionReceipt,
    GetPendingPreimages,
    GetQuarantinedPreimages,
    GetPendingSystemOps,
    GetQuarantinedSystemOps,
    GetSystemServiceInfo,
    GetProtocolState {
        key: String,
    },
}

#[derive(Debug)]
struct CliError(String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Error for CliError {}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::SubmitReport { canonical_report } => {
            let bytes = fs::read(canonical_report)?;
            let report: CanonicalReportBytes = bytes
                .try_into()
                .map_err(|_| CliError("canonical report exceeds 1 MiB".into()))?;
            print_call_data(
                "MiniJam",
                "submit_report",
                call_data(CALL_SUBMIT_REPORT, &report),
            );
        }
        Command::SubmitPreimage { preimage } => {
            let bytes = fs::read(preimage)?;
            print_call_data(
                "MiniJam",
                "submit_preimage",
                call_data(CALL_SUBMIT_PREIMAGE, &bytes),
            );
        }
        Command::SubmitCreateServiceSystemOp {
            code_hash,
            code_len,
            min_item_gas,
            min_memo_gas,
        } => {
            let command = SystemCommandV2::CreateService {
                code_hash: parse_hash(&code_hash)?,
                code_len,
                min_item_gas,
                min_memo_gas,
            };
            print_call_data(
                "MiniJam",
                "submit_system_op",
                call_data(CALL_SUBMIT_SYSTEM_OP, &Box::new(command)),
            );
        }
        Command::SubmitAllocation {
            allocation_id,
            target_service,
            amount,
        } => {
            let allocation = pallet_minijam::AllocationV1 {
                allocation_id,
                target_service,
                amount,
            };
            print_call_data(
                "MiniJam",
                "submit_allocation",
                call_data(CALL_SUBMIT_ALLOCATION, &allocation),
            );
        }
        Command::PauseExecution => print_call_data(
            "MiniJam",
            "pause_execution",
            call_data(CALL_PAUSE_EXECUTION, &()),
        ),
        Command::SubmitRawExtrinsic { extrinsic } => {
            let result = rpc_call(
                &cli.rpc,
                "author_submitExtrinsic",
                json!([normalize_hex(&extrinsic)?]),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::GetPackageStatus { package_hash } => print_rpc_result(
            &cli.rpc,
            "minijam_getPackageStatus",
            json!([normalize_hash_hex(&package_hash)?]),
        )?,
        Command::GetPackageFailure { package_hash } => print_rpc_result(
            &cli.rpc,
            "minijam_getPackageFailure",
            json!([normalize_hash_hex(&package_hash)?]),
        )?,
        Command::GetExecutionReceipt { package_hash } => print_rpc_result(
            &cli.rpc,
            "minijam_getExecutionReceiptByPackageHash",
            json!([normalize_hash_hex(&package_hash)?]),
        )?,
        Command::GetLastExecutionReceipt => {
            print_rpc_result(&cli.rpc, "minijam_getLastExecutionReceipt", json!([]))?
        }
        Command::GetPendingPreimages => {
            print_rpc_result(&cli.rpc, "minijam_getPendingPreimages", json!([]))?
        }
        Command::GetQuarantinedPreimages => {
            print_rpc_result(&cli.rpc, "minijam_getQuarantinedPreimages", json!([]))?
        }
        Command::GetPendingSystemOps => {
            print_rpc_result(&cli.rpc, "minijam_getPendingSystemOps", json!([]))?
        }
        Command::GetQuarantinedSystemOps => {
            print_rpc_result(&cli.rpc, "minijam_getQuarantinedSystemOps", json!([]))?
        }
        Command::GetSystemServiceInfo => {
            print_rpc_result(&cli.rpc, "minijam_getSystemServiceInfo", json!([]))?
        }
        Command::GetProtocolState { key } => {
            let key = parse_hex_array::<31>(&key)?;
            print_rpc_result(
                &cli.rpc,
                "minijam_getProtocolState",
                json!([hex_encode(&key)]),
            )?;
        }
    }
    Ok(())
}

fn call_data<T: Encode>(call_index: u8, args: &T) -> Vec<u8> {
    let mut encoded = vec![MINIJAM_PALLET_INDEX, call_index];
    args.encode_to(&mut encoded);
    encoded
}

fn print_call_data(pallet: &str, call: &str, bytes: Vec<u8>) {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "pallet": pallet,
            "call": call,
            "call_data": hex_encode(&bytes),
            "submit": "sign this call data and submit the signed extrinsic"
        }))
        .expect("JSON serialization cannot fail")
    );
}

fn print_rpc_result(rpc: &str, method: &str, params: Value) -> Result<(), Box<dyn Error>> {
    println!(
        "{}",
        serde_json::to_string_pretty(&rpc_call(rpc, method, params)?)?
    );
    Ok(())
}

fn rpc_call(rpc: &str, method: &str, params: Value) -> Result<Value, Box<dyn Error>> {
    let request = json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params});
    let value: Value = serde_json::from_str(&http_post_json(rpc, &request.to_string())?)?;
    if let Some(error) = value.get("error") {
        return Err(Box::new(CliError(format!("RPC error: {error}"))));
    }
    Ok(value.get("result").cloned().unwrap_or(Value::Null))
}

fn http_post_json(url: &str, body: &str) -> Result<String, Box<dyn Error>> {
    let endpoint = HttpEndpoint::parse(url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))?;
    write!(stream, "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", endpoint.path, endpoint.host, body.len(), body)?;
    read_http_body(stream)
}

fn read_http_body(mut stream: TcpStream) -> Result<String, Box<dyn Error>> {
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| CliError("HTTP response did not contain a header/body separator".into()))?;
    if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
        return Err(Box::new(CliError(format!(
            "HTTP request failed: {}",
            headers.lines().next().unwrap_or(headers)
        ))));
    }
    Ok(body.to_string())
}

struct HttpEndpoint {
    host: String,
    port: u16,
    path: String,
}
impl HttpEndpoint {
    fn parse(url: &str) -> Result<Self, Box<dyn Error>> {
        let stripped = url
            .strip_prefix("http://")
            .ok_or_else(|| CliError("only http:// URLs are supported".into()))?;
        let (authority, path) = stripped
            .split_once('/')
            .map(|(a, p)| (a, format!("/{p}")))
            .unwrap_or((stripped, "/".into()));
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_owned(), port.parse::<u16>()?),
            None => (authority.to_owned(), 80),
        };
        Ok(Self { host, port, path })
    }
}

fn normalize_hash_hex(input: &str) -> Result<String, Box<dyn Error>> {
    Ok(hex_encode(&parse_hash(input)?))
}
fn normalize_hex(input: &str) -> Result<String, Box<dyn Error>> {
    Ok(hex_encode(&parse_hex_bytes(input)?))
}
fn parse_hash(input: &str) -> Result<Hash, Box<dyn Error>> {
    parse_hex_array::<32>(input)
}
fn parse_hex_array<const N: usize>(input: &str) -> Result<[u8; N], Box<dyn Error>> {
    let bytes = parse_hex_bytes(input)?;
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        Box::new(CliError(format!(
            "expected {N} bytes of hex input, got {}",
            bytes.len()
        ))) as Box<dyn Error>
    })
}
fn parse_hex_bytes(input: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let hex = input.strip_prefix("0x").unwrap_or(input);
    if !hex.len().is_multiple_of(2) {
        return Err(Box::new(CliError("hex input must have even length".into())));
    }
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(hex_nibble(pair[0])? << 4 | hex_nibble(pair[1])?))
        .collect()
}
fn hex_nibble(byte: u8) -> Result<u8, Box<dyn Error>> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(Box::new(CliError(
            "hex input contains a non-hex character".into(),
        ))),
    }
}
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(2 + bytes.len() * 2);
    output.push_str("0x");
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn call_data_uses_the_runtime_minijam_index() {
        let encoded = call_data(CALL_PAUSE_EXECUTION, &());
        assert_eq!(encoded, vec![MINIJAM_PALLET_INDEX, CALL_PAUSE_EXECUTION]);
    }
    #[test]
    fn parse_hex_array_accepts_prefixed_hash() {
        assert_eq!(
            parse_hex_array::<4>("0x0102aAff").unwrap(),
            [1, 2, 170, 255]
        );
    }
}
