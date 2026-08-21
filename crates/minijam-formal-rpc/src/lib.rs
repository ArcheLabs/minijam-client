//! JamScript-agnostic MiniJAM Work ingress.

use std::{
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use jam_codec::Decode as JamDecode;
use jp_core_primitives::types::ServiceInfo;
use minijam_chain_client::{FinalizedContext, MiniJamChainClient};
use minijam_protocol::{blake2_256, Hash};
use parity_scale_codec::Decode;
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair};
use thiserror::Error;
use tokio::sync::Semaphore;
use tower_http::limit::RequestBodyLimitLayer;

const MAX_WORK_BYTES: usize = 1_048_576;
const MAX_RPC_BODY_BYTES: usize = 8 * 1_048_576;
const MAX_RPC_CONCURRENCY: usize = 32;

#[derive(Clone)]
pub struct FormalRpc {
    chain: Arc<MiniJamChainClient>,
    bundle_dir: PathBuf,
    admission: Arc<Semaphore>,
}

impl FormalRpc {
    pub fn new(chain: Arc<MiniJamChainClient>, bundle_dir: PathBuf) -> Result<Self, RpcError> {
        std::fs::create_dir_all(&bundle_dir)
            .map_err(|error| RpcError::Storage(error.to_string()))?;
        Ok(Self {
            chain,
            bundle_dir,
            admission: Arc::new(Semaphore::new(MAX_RPC_CONCURRENCY)),
        })
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/", post(json_rpc))
            .route("/ipfs/{cid}", get(get_bundle))
            .layer(RequestBodyLimitLayer::new(MAX_RPC_BODY_BYTES))
            .with_state(self)
    }

    async fn submit_work(&self, request: SubmitWorkParams) -> Result<SubmitWorkResult, RpcError> {
        let finalized = self.chain.finalized_context().await.map_err(chain_error)?;
        request.context.matches(&finalized)?;

        let service_info = self
            .chain
            .service_info_at(finalized.block_hash, request.service_id)
            .await
            .map_err(chain_error)?
            .ok_or(RpcError::ServiceNotFound)?;
        let service_info = decode_service_info(&service_info)?;
        validate_code_hash(&service_info, request.service_code_hash.0)?;

        let payload = STANDARD
            .decode(request.payload_base64)
            .map_err(|error| RpcError::InvalidParams(format!("invalid payloadBase64: {error}")))?;
        let extrinsics = request
            .extrinsics_base64
            .into_iter()
            .map(|value| {
                STANDARD.decode(value).map_err(|error| {
                    RpcError::InvalidParams(format!("invalid extrinsicsBase64: {error}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let built = minijam_work_package_builder::build_work_package(
            minijam_work_package_builder::BuildWorkInput {
                service_id: request.service_id,
                service_code_hash: request.service_code_hash.0,
                payload,
                extrinsics,
                anchor_hash: finalized.block_hash,
                state_root: finalized.state_root,
                lookup_anchor_slot: finalized.slot,
            },
        )
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
        if built.canonical_work_package.len() > MAX_WORK_BYTES {
            return Err(RpcError::InvalidParams("work package is too large".into()));
        }

        self.save_bundle(&built.bundle_bytes, built.content_ref.content_hash)?;
        let submission = self
            .chain
            .submit_work(
                built.canonical_work_package,
                built.content_ref,
                built.package_hash,
            )
            .await
            .map_err(chain_error)?;

        Ok(SubmitWorkResult {
            package_hash: hex(&built.package_hash),
            submission_hash: hex(&submission.extrinsic_hash),
            context: ContextResult::from(finalized),
        })
    }

    async fn work_status(&self, package_hash: Hash) -> Result<WorkStatusResult, RpcError> {
        let context = self.chain.finalized_context().await.map_err(chain_error)?;
        let work_id = self
            .chain
            .work_id_by_package_hash(package_hash)
            .await
            .map_err(chain_error)?;
        let Some(work_id) = work_id else {
            return Err(RpcError::WorkNotFound);
        };
        let work = self
            .chain
            .work_status::<pallet_minijam::WorkRecord<minijam_runtime::Runtime>>(work_id)
            .await
            .map_err(chain_error)?
            .ok_or(RpcError::WorkNotFound)?;
        let execution_receipt = if matches!(work.status, pallet_minijam::WorkStatus::Imported) {
            self.chain
                .execution_receipt(work_id)
                .await
                .map_err(chain_error)?
                .map(|hash| hex(&hash))
        } else {
            None
        };
        Ok(WorkStatusResult {
            package_hash: hex(&package_hash),
            work_id: Some(work_id),
            status: WorkStatus::from(work.status),
            execution_receipt,
            context: ContextResult::from(context),
        })
    }

    fn save_bundle(&self, bytes: &[u8], expected_hash: Hash) -> Result<(), RpcError> {
        save_bundle_to_dir(&self.bundle_dir, bytes, expected_hash)
    }
}

fn save_bundle_to_dir(
    bundle_dir: &FsPath,
    bytes: &[u8],
    expected_hash: Hash,
) -> Result<(), RpcError> {
    if blake2_256(bytes) != expected_hash {
        return Err(RpcError::Storage("bundle hash mismatch".into()));
    }
    let path = bundle_dir.join(hex_without_prefix(&expected_hash));
    if path.exists() {
        let existing =
            std::fs::read(&path).map_err(|error| RpcError::Storage(error.to_string()))?;
        if existing != bytes {
            return Err(RpcError::Storage("bundle hash collision".into()));
        }
        return Ok(());
    }
    let temporary = bundle_dir.join(format!(".{}.tmp", hex_without_prefix(&expected_hash)));
    std::fs::write(&temporary, bytes)
        .and_then(|_| std::fs::rename(&temporary, &path))
        .map_err(|error| RpcError::Storage(error.to_string()))
}

fn validate_code_hash(service_info: &ServiceInfo, expected: Hash) -> Result<(), RpcError> {
    if service_info.code_hash.0 != expected {
        return Err(RpcError::CodeHashMismatch);
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmitWorkParams {
    context: ContextParams,
    service_id: u32,
    service_code_hash: HashParam,
    payload_base64: String,
    #[serde(default)]
    extrinsics_base64: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContextParams {
    block_hash: HashParam,
    state_root: HashParam,
    slot: u32,
}

impl ContextParams {
    fn matches(&self, finalized: &FinalizedContext) -> Result<(), RpcError> {
        if self.block_hash.0 != finalized.block_hash
            || self.state_root.0 != finalized.state_root
            || self.slot != finalized.slot
        {
            return Err(RpcError::StaleContext {
                finalized: ContextResult::from(finalized.clone()),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "String")]
struct HashParam(Hash);

impl TryFrom<String> for HashParam {
    type Error = RpcError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let value = value.strip_prefix("0x").unwrap_or(&value);
        if value.len() != 64 {
            return Err(RpcError::InvalidParams("expected 32-byte hex".into()));
        }
        let mut output = [0; 32];
        for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
            output[index] = (hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?;
        }
        Ok(Self(output))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GetWorkStatusParams {
    package_hash: HashParam,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextResult {
    pub block_hash: String,
    pub block_number: u32,
    pub state_root: String,
    pub slot: u32,
}

impl From<FinalizedContext> for ContextResult {
    fn from(value: FinalizedContext) -> Self {
        Self {
            block_hash: hex(&value.block_hash),
            block_number: value.block_number,
            state_root: hex(&value.state_root),
            slot: value.slot,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitWorkResult {
    pub package_hash: String,
    pub submission_hash: String,
    pub context: ContextResult,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkStatus {
    InsufficientWorkers,
    AwaitingCandidate,
    Voting,
    Accepted,
    Imported,
    Failed,
}

impl From<pallet_minijam::WorkStatus> for WorkStatus {
    fn from(value: pallet_minijam::WorkStatus) -> Self {
        match value {
            pallet_minijam::WorkStatus::InsufficientWorkers => Self::InsufficientWorkers,
            pallet_minijam::WorkStatus::AwaitingCandidate => Self::AwaitingCandidate,
            pallet_minijam::WorkStatus::Voting => Self::Voting,
            pallet_minijam::WorkStatus::Accepted => Self::Accepted,
            pallet_minijam::WorkStatus::Imported => Self::Imported,
            pallet_minijam::WorkStatus::Failed => Self::Failed,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkStatusResult {
    pub package_hash: String,
    pub work_id: Option<u64>,
    pub status: WorkStatus,
    pub execution_receipt: Option<String>,
    pub context: ContextResult,
}

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("stale finalized context")]
    StaleContext { finalized: ContextResult },
    #[error("service not found")]
    ServiceNotFound,
    #[error("service code hash does not match finalized ServiceInfo")]
    CodeHashMismatch,
    #[error("work not found")]
    WorkNotFound,
    #[error("storage error: {0}")]
    Storage(String),
    #[error("chain error: {0}")]
    Chain(String),
    #[error("formal RPC is busy")]
    Busy,
}

impl RpcError {
    fn json_parts(self) -> (i32, String, Option<serde_json::Value>) {
        match self {
            Self::InvalidRequest(message) => (-32600, message, None),
            Self::InvalidParams(message) => (-32602, message, None),
            Self::MethodNotFound(method) => (-32601, format!("method not found: {method}"), None),
            Self::StaleContext { finalized } => (
                -32010,
                "stale finalized context".into(),
                Some(serde_json::to_value(finalized).expect("context serializes")),
            ),
            Self::ServiceNotFound => (-32011, "service not found".into(), None),
            Self::CodeHashMismatch => (-32012, "service code hash mismatch".into(), None),
            Self::WorkNotFound => (-32013, "work not found".into(), None),
            Self::Storage(message) => (-32020, message, None),
            Self::Chain(message) => (-32021, message, None),
            Self::Busy => (-32029, "formal RPC is busy".into(), None),
        }
    }
}

impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        let (code, message, data) = self.json_parts();
        Json(JsonRpcResponse::<serde_json::Value>::error(
            serde_json::Value::Null,
            code,
            message,
            data,
        ))
        .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: serde_json::Value,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse<T> {
    jsonrpc: &'static str,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

impl<T> JsonRpcResponse<T> {
    fn ok(id: serde_json::Value, result: T) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(
        id: serde_json::Value,
        code: i32,
        message: String,
        data: Option<serde_json::Value>,
    ) -> JsonRpcResponse<serde_json::Value> {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data,
            }),
        }
    }
}

async fn json_rpc(
    State(rpc): State<FormalRpc>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<serde_json::Value> {
    let id = request.id.clone();
    let result = if request.jsonrpc != "2.0" {
        Err(RpcError::InvalidRequest("jsonrpc must be 2.0".into()))
    } else {
        let permit = rpc.admission.clone().try_acquire_owned();
        match permit {
            Err(_) => Err(RpcError::Busy),
            Ok(_permit) => match request.method.as_str() {
                "minijam_submitWorkV1" => {
                    match serde_json::from_value::<SubmitWorkParams>(request.params) {
                        Ok(params) => rpc
                            .submit_work(params)
                            .await
                            .map(|value| serde_json::to_value(value).expect("result serializes")),
                        Err(error) => Err(RpcError::InvalidParams(error.to_string())),
                    }
                }
                "minijam_getWorkStatusV1" => {
                    match serde_json::from_value::<GetWorkStatusParams>(request.params) {
                        Ok(params) => rpc
                            .work_status(params.package_hash.0)
                            .await
                            .map(|value| serde_json::to_value(value).expect("result serializes")),
                        Err(error) => Err(RpcError::InvalidParams(error.to_string())),
                    }
                }
                method => Err(RpcError::MethodNotFound(method.into())),
            },
        }
    };
    let response = match result {
        Ok(result) => JsonRpcResponse::ok(id, result),
        Err(error) => {
            let (code, message, data) = error.json_parts();
            JsonRpcResponse::<serde_json::Value>::error(id, code, message, data)
        }
    };
    Json(serde_json::to_value(response).expect("response serializes"))
}

async fn get_bundle(
    State(rpc): State<FormalRpc>,
    Path(cid): Path<String>,
) -> Result<Response, RpcError> {
    let cid = cid::Cid::try_from(cid.as_str())
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    if cid.version() != cid::Version::V1
        || cid.codec() != 0x55
        || cid.hash().code() != 0xb220
        || cid.hash().digest().len() != 32
    {
        return Err(RpcError::InvalidParams("invalid bundle CID".into()));
    }
    let hash: Hash = cid
        .hash()
        .digest()
        .try_into()
        .map_err(|_| RpcError::InvalidParams("invalid bundle hash".into()))?;
    let bytes = std::fs::read(rpc.bundle_dir.join(hex_without_prefix(&hash)))
        .map_err(|error| RpcError::Storage(error.to_string()))?;
    if blake2_256(&bytes) != hash {
        return Err(RpcError::Storage("stored bundle hash mismatch".into()));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .body(Body::from(bytes))
        .map_err(|error| RpcError::Storage(error.to_string()))
}

fn decode_service_info(bytes: &[u8]) -> Result<ServiceInfo, RpcError> {
    let value = minijam_protocol::StateValue::decode(&mut bytes.as_ref())
        .map_err(|error| RpcError::Chain(error.to_string()))?;
    let bytes = value.into_inner();
    ServiceInfo::decode(&mut bytes.as_slice())
        .map_err(|error| RpcError::Chain(format!("invalid finalized ServiceInfo: {error}")))
}

fn chain_error(error: minijam_chain_client::ChainClientError) -> RpcError {
    RpcError::Chain(error.to_string())
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(2 + bytes.len() * 2);
    output.push_str("0x");
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn hex_without_prefix(bytes: &[u8]) -> String {
    hex(bytes).trim_start_matches("0x").to_owned()
}

fn hex_nibble(byte: u8) -> Result<u8, RpcError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(RpcError::InvalidParams("invalid hex".into())),
    }
}

pub async fn run_from_env() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind: SocketAddr = std::env::var("MINIJAM_FORMAL_RPC_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8090".into())
        .parse()?;
    let rpc_url = std::env::var("MINIJAM_RPC_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".into());
    let signer_uri = std::env::var("MINIJAM_RELAYER_URI")?;
    let signer =
        sr25519::Pair::from_string(&signer_uri, None).map_err(|error| error.to_string())?;
    let chain = Arc::new(
        MiniJamChainClient::connect(rpc_url, signer, std::time::Duration::from_secs(15)).await?,
    );
    let bundle_dir =
        PathBuf::from(std::env::var("MINIJAM_BUNDLE_DIR").unwrap_or_else(|_| "bundles".into()));
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, FormalRpc::new(chain, bundle_dir)?.router()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_work_params_reject_application_gas_fields() {
        let value = serde_json::json!({
            "context": {
                "blockHash": format!("0x{}", "11".repeat(32)),
                "stateRoot": format!("0x{}", "22".repeat(32)),
                "slot": 7
            },
            "serviceId": 1000,
            "serviceCodeHash": format!("0x{}", "33".repeat(32)),
            "payloadBase64": "",
            "extrinsicsBase64": [],
            "gas": 1
        });
        assert!(serde_json::from_value::<SubmitWorkParams>(value).is_err());
    }

    #[test]
    fn work_status_uses_formal_snake_case_names() {
        assert_eq!(
            serde_json::to_value(WorkStatus::InsufficientWorkers).unwrap(),
            serde_json::json!("insufficient_workers")
        );
        assert_eq!(
            serde_json::to_value(WorkStatus::Imported).unwrap(),
            serde_json::json!("imported")
        );
    }

    #[test]
    fn json_rpc_errors_preserve_request_id_and_standard_codes() {
        let response = JsonRpcResponse::<serde_json::Value>::error(
            serde_json::json!(17),
            RpcError::MethodNotFound("unknown".into()).json_parts().0,
            "method not found".into(),
            None,
        );
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["id"], serde_json::json!(17));
        assert_eq!(value["error"]["code"], -32601);
        assert_eq!(
            RpcError::InvalidRequest("bad version".into())
                .json_parts()
                .0,
            -32600
        );
        assert_eq!(
            RpcError::InvalidParams("bad params".into()).json_parts().0,
            -32602
        );
    }

    #[test]
    fn stale_context_and_code_hash_validation_are_explicit() {
        let finalized = FinalizedContext {
            block_hash: [1; 32],
            block_number: 7,
            state_root: [2; 32],
            slot: 7,
        };
        let stale = ContextParams {
            block_hash: HashParam([9; 32]),
            state_root: HashParam([2; 32]),
            slot: 7,
        };
        assert!(matches!(
            stale.matches(&finalized),
            Err(RpcError::StaleContext { .. })
        ));

        let mut info = ServiceInfo::default();
        info.code_hash.0 = [3; 32];
        assert!(validate_code_hash(&info, [3; 32]).is_ok());
        assert!(matches!(
            validate_code_hash(&info, [4; 32]),
            Err(RpcError::CodeHashMismatch)
        ));
    }

    #[test]
    fn bundle_store_writes_and_verifies_content() {
        let directory = tempfile::tempdir().unwrap();
        let bytes = b"verified bundle";
        let hash = blake2_256(bytes);
        save_bundle_to_dir(directory.path(), bytes, hash).unwrap();
        let stored = std::fs::read(directory.path().join(hex_without_prefix(&hash))).unwrap();
        assert_eq!(stored, bytes);
        assert!(save_bundle_to_dir(directory.path(), b"tampered", hash).is_err());
    }
}
