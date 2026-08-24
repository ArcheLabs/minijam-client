// SPDX-License-Identifier: Apache-2.0

use std::{
    error::Error,
    fmt, fs,
    io::{Read, Write},
    net::TcpStream,
};

use clap::{Parser, Subcommand, ValueEnum};
use minijam_protocol::{
    ContentRef, Hash, OpposeReason, ReportEnvelopeV1, SystemCommandV2, Verdict, WorkerVoteV1,
    PROTOCOL_VERSION_V1,
};
use parity_scale_codec::{Decode, Encode};
use serde_json::{json, Value};

const MINIJAM_WORKERS_PALLET_INDEX: u8 = 7;
const MINIJAM_PALLET_INDEX: u8 = 8;
const CALL_SUBMIT_WORK: u8 = 0;
const CALL_SUBMIT_CANDIDATE: u8 = 1;
const CALL_SUBMIT_PREIMAGE: u8 = 4;
const CALL_SUBMIT_SYSTEM_OP: u8 = 5;
const CALL_FUND_SERVICE: u8 = 6;
const CALL_CLAIM_FAUCET: u8 = 10;
const CALL_SUBMIT_VOTE: u8 = 2;

#[derive(Parser)]
#[command(name = "minijam-cli")]
#[command(about = "MiniJAM stage-0 RPC and call-data utility")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    rpc: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    SubmitWork {
        #[arg(long)]
        work_package: String,
        #[arg(long)]
        bundle_cid: Option<String>,
        #[arg(long)]
        bundle_cid_hex: Option<String>,
        #[arg(long)]
        bundle_hash: String,
        #[arg(long)]
        bundle_size: u64,
    },
    FundService {
        #[arg(long)]
        service_id: u32,
        #[arg(long)]
        amount: u128,
    },
    SubmitCandidate {
        #[arg(long)]
        envelope: String,
    },
    SubmitVote {
        #[arg(long)]
        worker_id: u64,
        #[arg(long)]
        work_id: u64,
        #[arg(long)]
        round: u8,
        #[arg(long)]
        assignment_epoch: u32,
        #[arg(long)]
        candidate_report_hash: String,
        #[arg(long)]
        verdict: VoteVerdictArg,
        #[arg(long)]
        oppose_hash: Option<String>,
        #[arg(long)]
        deadline: u32,
        #[arg(long)]
        chain_id: String,
        #[arg(long, default_value_t = PROTOCOL_VERSION_V1)]
        protocol_version: u16,
        #[arg(long)]
        signature: String,
    },
    SubmitPreimage {
        #[arg(long)]
        preimage: String,
    },
    SubmitCreateServiceSystemOp {
        #[arg(long)]
        code_hash: String,
        #[arg(long)]
        code_len: u32,
        #[arg(long)]
        min_item_gas: u64,
        #[arg(long)]
        min_memo_gas: u64,
    },
    ClaimFaucet,
    SubmitRawExtrinsic {
        #[arg(long)]
        extrinsic: String,
    },
    WorkerStatus {
        #[arg(long, default_value = "http://127.0.0.1:9616/metrics")]
        metrics: String,
    },
    GetWork {
        work_id: u64,
    },
    GetPendingWorkTasks,
    GetOpenVoteTasks,
    GetWorkByPackageHash {
        package_hash: String,
    },
    GetWorkBundleRef {
        work_id: u64,
    },
    GetCandidate {
        work_id: u64,
        round: u8,
    },
    GetExecutionReceipt {
        work_id: u64,
    },
    GetLastExecutionReceipt,
    GetServiceFuel {
        service_id: u32,
    },
    GetWorkFuelReservation {
        work_id: u64,
    },
    GetWorkFuelSettlement {
        work_id: u64,
    },
    GetPendingPreimages,
    GetQuarantinedPreimages,
    GetPreimageStatus {
        requester: u32,
        blob_hash: String,
        blob_len: u32,
    },
    GetPendingSystemOps,
    GetQuarantinedSystemOps,
    GetSystemOp {
        request_id: String,
    },
    GetSystemReceipt {
        request_id: String,
    },
    GetSystemServiceInfo,
    GetProtocolState {
        key: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum VoteVerdictArg {
    Support,
    InvalidRefine,
    MissingData,
    ContextMismatch,
    MalformedOutput,
    Other,
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
        Command::SubmitWork {
            work_package,
            bundle_cid,
            bundle_cid_hex,
            bundle_hash,
            bundle_size,
        } => {
            let work_package = fs::read(work_package)?;
            let bundle_ref = ContentRef {
                cid_v1: parse_cid(bundle_cid, bundle_cid_hex)?
                    .try_into()
                    .map_err(|_| {
                        CliError("bundle CID is longer than the 128 byte ContentRef limit".into())
                    })?,
                content_hash: parse_hash(&bundle_hash)?,
                size: bundle_size,
            };
            print_call_data(
                "MiniJam",
                "submit_work",
                call_data(CALL_SUBMIT_WORK, &(work_package, bundle_ref)),
            );
        }
        Command::FundService { service_id, amount } => {
            print_call_data(
                "MiniJam",
                "fund_service",
                call_data(CALL_FUND_SERVICE, &(service_id, amount)),
            );
        }
        Command::SubmitCandidate { envelope } => {
            let envelope = decode_envelope_file(&envelope)?;
            print_call_data(
                "MiniJam",
                "submit_candidate",
                call_data(CALL_SUBMIT_CANDIDATE, &envelope),
            );
        }
        Command::SubmitVote {
            worker_id,
            work_id,
            round,
            assignment_epoch,
            candidate_report_hash,
            verdict,
            oppose_hash,
            deadline,
            chain_id,
            protocol_version,
            signature,
        } => {
            let vote = WorkerVoteV1 {
                work_id,
                round,
                assignment_epoch,
                candidate_report_hash: parse_hash(&candidate_report_hash)?,
                verdict: build_verdict(verdict, oppose_hash.as_deref())?,
                deadline,
                chain_id: parse_hash(&chain_id)?,
                protocol_version,
            };
            print_call_data(
                "MiniJamWorkers",
                "submit_vote",
                call_data_for_pallet(
                    MINIJAM_WORKERS_PALLET_INDEX,
                    CALL_SUBMIT_VOTE,
                    &(worker_id, vote, parse_hex_array::<64>(&signature)?),
                ),
            );
        }
        Command::SubmitPreimage { preimage } => {
            print_call_data(
                "MiniJam",
                "submit_preimage",
                call_data(CALL_SUBMIT_PREIMAGE, &fs::read(preimage)?),
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
                call_data(CALL_SUBMIT_SYSTEM_OP, &command),
            );
        }
        Command::ClaimFaucet => {
            print_call_data("MiniJam", "claim_faucet", call_data(CALL_CLAIM_FAUCET, &()));
        }
        Command::SubmitRawExtrinsic { extrinsic } => {
            let result = rpc_call(
                &cli.rpc,
                "author_submitExtrinsic",
                json!([normalize_hex(&extrinsic)?]),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::WorkerStatus { metrics } => {
            print_worker_status(&metrics)?;
        }
        Command::GetWork { work_id } => {
            print_rpc_result(&cli.rpc, "minijam_getWork", json!([work_id]))?
        }
        Command::GetPendingWorkTasks => {
            print_rpc_result(&cli.rpc, "minijam_getPendingWorkTasks", json!([]))?
        }
        Command::GetOpenVoteTasks => {
            print_rpc_result(&cli.rpc, "minijam_getOpenVoteTasks", json!([]))?
        }
        Command::GetWorkByPackageHash { package_hash } => print_rpc_result(
            &cli.rpc,
            "minijam_getWorkByPackageHash",
            json!([normalize_hash_hex(&package_hash)?]),
        )?,
        Command::GetWorkBundleRef { work_id } => {
            print_rpc_result(&cli.rpc, "minijam_getWorkBundleRef", json!([work_id]))?
        }
        Command::GetCandidate { work_id, round } => {
            print_rpc_result(&cli.rpc, "minijam_getCandidate", json!([work_id, round]))?
        }
        Command::GetExecutionReceipt { work_id } => {
            print_rpc_result(&cli.rpc, "minijam_getExecutionReceipt", json!([work_id]))?
        }
        Command::GetLastExecutionReceipt => {
            print_rpc_result(&cli.rpc, "minijam_getLastExecutionReceipt", json!([]))?
        }
        Command::GetServiceFuel { service_id } => {
            print_rpc_result(&cli.rpc, "minijam_getServiceFuel", json!([service_id]))?
        }
        Command::GetWorkFuelReservation { work_id } => {
            print_rpc_result(&cli.rpc, "minijam_getWorkFuelReservation", json!([work_id]))?
        }
        Command::GetWorkFuelSettlement { work_id } => {
            print_rpc_result(&cli.rpc, "minijam_getWorkFuelSettlement", json!([work_id]))?
        }
        Command::GetPendingPreimages => {
            print_rpc_result(&cli.rpc, "minijam_getPendingPreimages", json!([]))?
        }
        Command::GetQuarantinedPreimages => {
            print_rpc_result(&cli.rpc, "minijam_getQuarantinedPreimages", json!([]))?
        }
        Command::GetPreimageStatus {
            requester,
            blob_hash,
            blob_len,
        } => print_rpc_result(
            &cli.rpc,
            "minijam_getPreimageStatus",
            json!([requester, normalize_hash_hex(&blob_hash)?, blob_len]),
        )?,
        Command::GetPendingSystemOps => {
            print_rpc_result(&cli.rpc, "minijam_getPendingSystemOps", json!([]))?
        }
        Command::GetQuarantinedSystemOps => {
            print_rpc_result(&cli.rpc, "minijam_getQuarantinedSystemOps", json!([]))?
        }
        Command::GetSystemOp { request_id } => print_rpc_result(
            &cli.rpc,
            "minijam_getSystemOp",
            json!([normalize_hash_hex(&request_id)?]),
        )?,
        Command::GetSystemReceipt { request_id } => print_rpc_result(
            &cli.rpc,
            "minijam_getSystemReceipt",
            json!([normalize_hash_hex(&request_id)?]),
        )?,
        Command::GetSystemServiceInfo => {
            print_rpc_result(&cli.rpc, "minijam_getSystemServiceInfo", json!([]))?
        }
        Command::GetProtocolState { key } => {
            let key = parse_hex_array::<31>(&key)?;
            print_rpc_result(
                &cli.rpc,
                "minijam_getProtocolState",
                json!([hex_encode(&key)]),
            )?
        }
    }
    Ok(())
}

fn call_data<T: Encode>(call_index: u8, args: &T) -> Vec<u8> {
    call_data_for_pallet(MINIJAM_PALLET_INDEX, call_index, args)
}

fn call_data_for_pallet<T: Encode>(pallet_index: u8, call_index: u8, args: &T) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(2);
    encoded.push(pallet_index);
    encoded.push(call_index);
    args.encode_to(&mut encoded);
    encoded
}

fn decode_envelope_file(path: &str) -> Result<ReportEnvelopeV1, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let mut input = bytes.as_slice();
    let envelope = ReportEnvelopeV1::decode(&mut input)
        .map_err(|error| CliError(format!("invalid ReportEnvelopeV1 SCALE file: {error}")))?;
    if !input.is_empty() {
        return Err(Box::new(CliError(
            "ReportEnvelopeV1 SCALE file contains trailing bytes".into(),
        )));
    }
    Ok(envelope)
}

fn build_verdict(
    verdict: VoteVerdictArg,
    oppose_hash: Option<&str>,
) -> Result<Verdict, Box<dyn Error>> {
    let verdict = match verdict {
        VoteVerdictArg::Support => Verdict::Support,
        VoteVerdictArg::InvalidRefine => Verdict::Oppose(OpposeReason::InvalidRefine),
        VoteVerdictArg::MissingData => Verdict::Oppose(OpposeReason::MissingData),
        VoteVerdictArg::ContextMismatch => Verdict::Oppose(OpposeReason::ContextMismatch),
        VoteVerdictArg::MalformedOutput => Verdict::Oppose(OpposeReason::MalformedOutput),
        VoteVerdictArg::Other => Verdict::Oppose(OpposeReason::Other(parse_hash(
            oppose_hash.ok_or_else(|| CliError("--oppose-hash is required for Other".into()))?,
        )?)),
    };
    Ok(verdict)
}

fn print_call_data(pallet: &str, call: &str, call_data: Vec<u8>) {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "pallet": pallet,
            "call": call,
            "call_data": hex_encode(&call_data),
            "submit": "sign this call data as a MiniJAM runtime call, then submit the signed extrinsic with submit-raw-extrinsic"
        }))
        .expect("serializing JSON output cannot fail")
    );
}

fn print_rpc_result(rpc: &str, method: &str, params: Value) -> Result<(), Box<dyn Error>> {
    let result = rpc_call(rpc, method, params)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn rpc_call(rpc: &str, method: &str, params: Value) -> Result<Value, Box<dyn Error>> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let response = http_post_json(rpc, &request.to_string())?;
    let value: Value = serde_json::from_str(&response)?;
    if let Some(error) = value.get("error") {
        return Err(Box::new(CliError(format!("RPC error: {error}"))));
    }
    Ok(value.get("result").cloned().unwrap_or(Value::Null))
}

fn print_worker_status(metrics_url: &str) -> Result<(), Box<dyn Error>> {
    let metrics = http_get(metrics_url)?;
    let mut status = serde_json::Map::new();
    for line in metrics.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(' ') {
            if name.starts_with("minijam_worker_") {
                status.insert(name.to_string(), Value::String(value.trim().to_string()));
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(status))?);
    Ok(())
}

fn http_post_json(url: &str, body: &str) -> Result<String, Box<dyn Error>> {
    let endpoint = HttpEndpoint::parse(url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))?;
    write!(
        stream,
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        endpoint.path,
        endpoint.host,
        body.len(),
        body
    )?;
    read_http_body(stream)
}

fn http_get(url: &str) -> Result<String, Box<dyn Error>> {
    let endpoint = HttpEndpoint::parse(url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))?;
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        endpoint.path, endpoint.host
    )?;
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
        let (authority, path) = match stripped.split_once('/') {
            Some((authority, path)) => (authority, format!("/{path}")),
            None => (stripped, "/".to_string()),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), port.parse()?),
            None => (authority.to_string(), 80),
        };
        Ok(Self { host, port, path })
    }
}

fn parse_cid(cid: Option<String>, cid_hex: Option<String>) -> Result<Vec<u8>, Box<dyn Error>> {
    match (cid, cid_hex) {
        (Some(cid), None) => Ok(cid.into_bytes()),
        (None, Some(cid_hex)) => parse_hex_bytes(&cid_hex),
        (Some(_), Some(_)) => Err(Box::new(CliError(
            "use either --bundle-cid or --bundle-cid-hex, not both".into(),
        ))),
        (None, None) => Err(Box::new(CliError(
            "one of --bundle-cid or --bundle-cid-hex is required".into(),
        ))),
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
    if hex.len() % 2 != 0 {
        return Err(Box::new(CliError("hex input must have even length".into())));
    }
    let mut output = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks_exact(2) {
        output.push(hex_nibble(pair[0])? << 4 | hex_nibble(pair[1])?);
    }
    Ok(output)
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
    fn parse_hex_array_accepts_prefixed_hash() {
        let parsed = parse_hex_array::<4>("0x0102aAff").unwrap();
        assert_eq!(parsed, [1, 2, 170, 255]);
    }

    #[test]
    fn claim_faucet_call_data_uses_minijam_pallet_and_call_index() {
        assert_eq!(hex_encode(&call_data(CALL_CLAIM_FAUCET, &())), "0x080a");
    }

    #[test]
    fn fund_service_call_data_encodes_args_after_prefix() {
        let encoded = call_data(CALL_FUND_SERVICE, &(7u32, 25u128));
        assert_eq!(&encoded[..2], &[MINIJAM_PALLET_INDEX, CALL_FUND_SERVICE]);
        assert_eq!(&encoded[2..6], &7u32.to_le_bytes());
        assert_eq!(&encoded[6..22], &25u128.to_le_bytes());
    }

    #[test]
    fn submit_vote_call_data_uses_workers_pallet_prefix() {
        let vote = WorkerVoteV1 {
            work_id: 42,
            round: 1,
            assignment_epoch: 7,
            candidate_report_hash: [9u8; 32],
            verdict: Verdict::Support,
            deadline: 100,
            chain_id: [42u8; 32],
            protocol_version: PROTOCOL_VERSION_V1,
        };
        let encoded = call_data_for_pallet(
            MINIJAM_WORKERS_PALLET_INDEX,
            CALL_SUBMIT_VOTE,
            &(3u64, vote, [5u8; 64]),
        );
        assert_eq!(
            &encoded[..2],
            &[MINIJAM_WORKERS_PALLET_INDEX, CALL_SUBMIT_VOTE]
        );
        assert_eq!(&encoded[2..10], &3u64.to_le_bytes());
    }
}
