//! Persistent, JamScript-agnostic MiniJAM transaction ingress.

use std::{
    future::Future,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
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
use jam_codec::{Decode as JamDecode, Encode as JamEncode};
use jp_core_primitives::{simple::ByteSequence, types::Preimage};
use minijam_chain_client::{FinalizedContext, MiniJamChainClient};
use minijam_protocol::{blake2_256, Hash, SystemReceiptV2};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair};
use thiserror::Error;
use tokio::sync::Semaphore;
use tower_http::limit::RequestBodyLimitLayer;

const MAX_RPC_BODY_BYTES: usize = 8 * 1_048_576;
const MAX_BATCH_ITEMS: usize = 4;
const MAX_WORK_PACKAGE_BYTES: usize = 1_048_576;
const MAX_RPC_CONCURRENCY: usize = 32;
const CONNECT_DEADLINE: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug)]
struct ConnectRetryPolicy {
    deadline: Duration,
    initial_delay: Duration,
    max_delay: Duration,
}
impl Default for ConnectRetryPolicy {
    fn default() -> Self {
        Self {
            deadline: CONNECT_DEADLINE,
            initial_delay: Duration::from_millis(250),
            max_delay: Duration::from_secs(2),
        }
    }
}
async fn retry_connection<T, F, Fut>(
    policy: ConnectRetryPolicy,
    mut connect: F,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let started = Instant::now();
    let mut delay = policy.initial_delay;
    let mut last_error = None;
    loop {
        let remaining = policy.deadline.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(last_error.unwrap_or_else(|| "connection deadline elapsed".into()));
        }
        match tokio::time::timeout(remaining, connect()).await {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => last_error = Some(error),
            Err(_) => {
                return Err(last_error.unwrap_or_else(|| "connection attempt timed out".into()))
            }
        }
        let remaining = policy.deadline.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(last_error.expect("failed attempt has an error"));
        }
        tokio::time::sleep(delay.min(remaining)).await;
        delay = delay.saturating_mul(2).min(policy.max_delay);
    }
}

async fn connect_chain_with_retry(
    rpc_url: &str,
    signer_uri: &str,
    timeout: Duration,
) -> Result<MiniJamChainClient, String> {
    let rpc_url = rpc_url.to_owned();
    let signer_uri = signer_uri.to_owned();
    retry_connection(ConnectRetryPolicy::default(), || {
        let rpc_url = rpc_url.clone();
        let signer_uri = signer_uri.clone();
        async move {
            let signer =
                sr25519::Pair::from_string(&signer_uri, None).map_err(|error| error.to_string())?;
            MiniJamChainClient::connect(rpc_url, signer, timeout)
                .await
                .map_err(|error| error.to_string())
        }
    })
    .await
    .map_err(|error| format!("failed to connect to MiniJAM node before startup deadline: {error}"))
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum TransactionState {
    Queued,
    Packaged,
    Refining,
    Reported,
    Imported,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct QueueEntry {
    id: Hash,
    service_id: u32,
    service_code_hash: Hash,
    payload: Vec<u8>,
    extrinsics: Vec<Vec<u8>>,
    state: TransactionState,
    package_hash: Option<Hash>,
    item_index: Option<u32>,
    receipt: Option<Hash>,
    error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActivePackage {
    package_hash: Hash,
    transaction_ids: Vec<Hash>,
    bundle_hash: Hash,
    context: ContextResult,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct QueueStore {
    entries: Vec<QueueEntry>,
    active: Option<ActivePackage>,
}

#[derive(Clone)]
pub struct FormalRpc {
    chain: Arc<MiniJamChainClient>,
    bundle_dir: PathBuf,
    queue_path: PathBuf,
    store: Arc<Mutex<QueueStore>>,
    admission: Arc<Semaphore>,
}

impl FormalRpc {
    pub fn new(chain: Arc<MiniJamChainClient>, bundle_dir: PathBuf) -> Result<Self, RpcError> {
        std::fs::create_dir_all(&bundle_dir)
            .map_err(|error| RpcError::Storage(error.to_string()))?;
        let queue_path = bundle_dir.join("transactions.json");
        let store = if queue_path.exists() {
            let bytes =
                std::fs::read(&queue_path).map_err(|error| RpcError::Storage(error.to_string()))?;
            serde_json::from_slice(&bytes)
                .map_err(|error| RpcError::Storage(format!("invalid durable queue: {error}")))?
        } else {
            QueueStore::default()
        };
        Ok(Self {
            chain,
            bundle_dir,
            queue_path,
            store: Arc::new(Mutex::new(store)),
            admission: Arc::new(Semaphore::new(MAX_RPC_CONCURRENCY)),
        })
    }
    pub fn router(self) -> Router {
        Router::new()
            .route("/", post(json_rpc))
            .route("/worker/v1/task", get(worker_task))
            .route("/worker/v1/report-submitted", post(report_submitted))
            .route("/ipfs/{cid}", get(get_bundle))
            .route("/health/ready", get(ready))
            .layer(RequestBodyLimitLayer::new(MAX_RPC_BODY_BYTES))
            .with_state(self)
    }

    fn persist_locked(&self, store: &QueueStore) -> Result<(), RpcError> {
        let bytes = serde_json::to_vec_pretty(store)
            .map_err(|error| RpcError::Storage(error.to_string()))?;
        let temporary = self.queue_path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes)
            .and_then(|_| std::fs::rename(&temporary, &self.queue_path))
            .map_err(|error| RpcError::Storage(error.to_string()))
    }
    async fn submit_transaction(
        &self,
        request: SubmitTransactionParams,
    ) -> Result<SubmitTransactionResult, RpcError> {
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
        let id = transaction_id(
            request.service_id,
            request.service_code_hash.0,
            &payload,
            &extrinsics,
        );
        let finalized = self.chain.finalized_context().await.map_err(chain_error)?;
        let service_info = self
            .chain
            .service_info_at(finalized.block_hash, request.service_id)
            .await
            .map_err(chain_error)?
            .ok_or(RpcError::ServiceNotFound)?;
        let service_info = decode_service_info(&service_info)?;
        if service_info.code_hash.0 != request.service_code_hash.0 {
            return Err(RpcError::CodeHashMismatch);
        }
        let mut store = self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?;
        if let Some(existing) = store.entries.iter().find(|entry| entry.id == id) {
            return Ok(SubmitTransactionResult {
                transaction_id: hex(&existing.id),
                status: existing.state,
                package_hash: existing.package_hash.map(|hash| hex(&hash)),
                item_index: existing.item_index,
            });
        }
        store.entries.push(QueueEntry {
            id,
            service_id: request.service_id,
            service_code_hash: request.service_code_hash.0,
            payload,
            extrinsics,
            state: TransactionState::Queued,
            package_hash: None,
            item_index: None,
            receipt: None,
            error: None,
        });
        let result = SubmitTransactionResult {
            transaction_id: hex(&id),
            status: TransactionState::Queued,
            package_hash: None,
            item_index: None,
        };
        self.persist_locked(&store)?;
        Ok(result)
    }

    async fn refresh_active(&self) -> Result<(), RpcError> {
        let active = self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?
            .active
            .clone();
        let Some(active) = active else {
            return Ok(());
        };
        let Some(status) = self
            .chain
            .package_status(active.package_hash)
            .await
            .map_err(chain_error)?
        else {
            return Ok(());
        };
        let receipt = if matches!(status, minijam_protocol::PackageStatus::Imported) {
            self.chain
                .execution_receipt_by_package_hash(active.package_hash)
                .await
                .map_err(chain_error)?
        } else {
            None
        };
        let failure = if matches!(status, minijam_protocol::PackageStatus::Failed) {
            self.chain
                .package_failure(active.package_hash)
                .await
                .map_err(chain_error)?
                .map(|bytes| hex(&bytes))
                .unwrap_or_else(|| "package failed".into())
        } else {
            String::new()
        };
        let mut store = self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?;
        match status {
            minijam_protocol::PackageStatus::Pending => {}
            minijam_protocol::PackageStatus::Imported => {
                for entry in &mut store.entries {
                    if active.transaction_ids.contains(&entry.id) {
                        entry.state = TransactionState::Imported;
                        entry.receipt = receipt;
                    }
                }
                store.active = None;
            }
            minijam_protocol::PackageStatus::Failed => {
                for entry in &mut store.entries {
                    if active.transaction_ids.contains(&entry.id) {
                        entry.state = TransactionState::Failed;
                        entry.error = Some(failure.clone());
                    }
                }
                store.active = None;
            }
        }
        self.persist_locked(&store)
    }

    async fn build_active_package(&self) -> Result<(), RpcError> {
        if self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?
            .active
            .is_some()
        {
            return Ok(());
        }
        let first = {
            let store = self
                .store
                .lock()
                .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?;
            store
                .entries
                .iter()
                .find(|entry| matches!(entry.state, TransactionState::Queued))
                .cloned()
        };
        let Some(first) = first else {
            return Ok(());
        };
        let finalized = self.chain.finalized_context().await.map_err(chain_error)?;
        let selected = {
            let store = self
                .store
                .lock()
                .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?;
            store
                .entries
                .iter()
                .filter(|entry| {
                    matches!(entry.state, TransactionState::Queued)
                        && entry.service_id == first.service_id
                        && entry.service_code_hash == first.service_code_hash
                })
                .take(MAX_BATCH_ITEMS)
                .cloned()
                .collect::<Vec<_>>()
        };
        let built = minijam_work_package_builder::build_work_batch(
            minijam_work_package_builder::BuildWorkBatchInput {
                service_id: first.service_id,
                service_code_hash: first.service_code_hash,
                transactions: selected
                    .iter()
                    .map(
                        |entry| minijam_work_package_builder::BuildTransactionInput {
                            payload: entry.payload.clone(),
                            extrinsics: entry.extrinsics.clone(),
                        },
                    )
                    .collect(),
                anchor_hash: finalized.block_hash,
                state_root: finalized.state_root,
                lookup_anchor_slot: finalized.slot,
            },
        )
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
        if built.canonical_work_package.len() > MAX_WORK_PACKAGE_BYTES {
            return Err(RpcError::InvalidParams("work package is too large".into()));
        }
        save_bundle_to_dir(
            &self.bundle_dir,
            &built.bundle_bytes,
            built.content_ref.content_hash,
        )?;
        let active = ActivePackage {
            package_hash: built.package_hash,
            transaction_ids: selected.iter().map(|entry| entry.id).collect(),
            bundle_hash: built.content_ref.content_hash,
            context: ContextResult::from(finalized),
        };
        let mut store = self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?;
        for (index, id) in active.transaction_ids.iter().enumerate() {
            if let Some(entry) = store.entries.iter_mut().find(|entry| entry.id == *id) {
                entry.state = TransactionState::Packaged;
                entry.package_hash = Some(active.package_hash);
                entry.item_index = Some(index as u32);
            }
        }
        store.active = Some(active);
        self.persist_locked(&store)
    }

    async fn worker_task(&self) -> Result<Option<WorkerTaskResponse>, RpcError> {
        self.refresh_active().await?;
        self.build_active_package().await?;
        let active = self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?
            .active
            .clone();
        let Some(active) = active else {
            return Ok(None);
        };
        let bytes = std::fs::read(
            self.bundle_dir
                .join(hex_without_prefix(&active.bundle_hash)),
        )
        .map_err(|error| RpcError::Storage(error.to_string()))?;
        let mut store = self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?;
        for entry in &mut store.entries {
            if active.transaction_ids.contains(&entry.id)
                && matches!(entry.state, TransactionState::Packaged)
            {
                entry.state = TransactionState::Refining;
            }
        }
        self.persist_locked(&store)?;
        Ok(Some(WorkerTaskResponse {
            package_hash: hex(&active.package_hash),
            bundle_base64: STANDARD.encode(bytes),
            context: active.context,
        }))
    }

    fn mark_reported(&self, package_hash: Hash) -> Result<(), RpcError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?;
        let transaction_ids = store
            .active
            .as_ref()
            .filter(|active| active.package_hash == package_hash)
            .ok_or(RpcError::TransactionNotFound)?
            .transaction_ids
            .clone();
        for entry in &mut store.entries {
            if transaction_ids.contains(&entry.id)
                && matches!(entry.state, TransactionState::Refining)
            {
                entry.state = TransactionState::Reported;
            }
        }
        self.persist_locked(&store)
    }

    async fn transaction_status(&self, id: Hash) -> Result<TransactionStatusResult, RpcError> {
        self.refresh_active().await?;
        let store = self
            .store
            .lock()
            .map_err(|_| RpcError::Storage("queue lock poisoned".into()))?;
        let entry = store
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or(RpcError::TransactionNotFound)?;
        Ok(TransactionStatusResult {
            transaction_id: hex(&entry.id),
            status: entry.state,
            package_hash: entry.package_hash.map(|hash| hex(&hash)),
            item_index: entry.item_index,
            execution_receipt: entry.receipt.map(|hash| hex(&hash)),
            error: entry.error.clone(),
        })
    }

    async fn create_service(
        &self,
        request: CreateServiceParams,
    ) -> Result<DeploymentResult, RpcError> {
        let blob = STANDARD
            .decode(request.blob_base64)
            .map_err(|error| RpcError::InvalidParams(format!("invalid blobBase64: {error}")))?;
        if blob.is_empty() {
            return Err(RpcError::InvalidParams(
                "service blob must not be empty".into(),
            ));
        }
        let code_hash = blake2_256(&blob);
        if request.code_hash.0 != code_hash {
            return Err(RpcError::CodeHashMismatch);
        }
        let submitted = self
            .chain
            .submit_create_service(
                code_hash,
                u32::try_from(blob.len())
                    .map_err(|_| RpcError::InvalidParams("service blob is too large".into()))?,
                request.min_item_gas,
                request.min_memo_gas,
            )
            .await
            .map_err(deployment_chain_error)?;
        let receipt = wait_for_system_receipt(&self.chain, submitted.correlation).await?;
        let service_id = match receipt {
            SystemReceiptV2::ServiceCreated { service_id } => service_id,
            SystemReceiptV2::Rejected { code } => return Err(RpcError::DeploymentRejected(code)),
        };
        let canonical = JamEncode::encode(&Preimage {
            requester: service_id,
            blob: ByteSequence::from(blob),
        });
        self.chain
            .submit_preimage_finalized(canonical)
            .await
            .map_err(deployment_chain_error)?;
        let context = wait_for_service_code_hash(&self.chain, service_id, code_hash).await?;
        Ok(DeploymentResult {
            operation_id: hex(&submitted.correlation),
            service_id,
            code_hash: hex(&code_hash),
            finalized: true,
            context: ContextResult::from(context),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Encode)]
struct TransactionForId {
    service_id: u32,
    service_code_hash: Hash,
    payload: Vec<u8>,
    extrinsics: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmitTransactionParams {
    service_id: u32,
    service_code_hash: HashParam,
    payload_base64: String,
    #[serde(default)]
    extrinsics_base64: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GetTransactionStatusParams {
    transaction_id: HashParam,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateServiceParams {
    code_hash: HashParam,
    blob_base64: String,
    min_item_gas: u64,
    min_memo_gas: u64,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "String")]
struct HashParam(Hash);
impl TryFrom<String> for HashParam {
    type Error = RpcError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        decode_hash(&value).map(Self)
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmitTransactionResult {
    transaction_id: String,
    status: TransactionState,
    package_hash: Option<String>,
    item_index: Option<u32>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransactionStatusResult {
    transaction_id: String,
    status: TransactionState,
    package_hash: Option<String>,
    item_index: Option<u32>,
    execution_receipt: Option<String>,
    error: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerTaskResponse {
    package_hash: String,
    bundle_base64: String,
    context: ContextResult,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentResult {
    operation_id: String,
    service_id: u32,
    code_hash: String,
    finalized: bool,
    context: ContextResult,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("service not found")]
    ServiceNotFound,
    #[error("service code hash does not match finalized ServiceInfo")]
    CodeHashMismatch,
    #[error("transaction not found")]
    TransactionNotFound,
    #[error("deployment was rejected with code {0}")]
    DeploymentRejected(u32),
    #[error("deployment system operation was quarantined: {0}")]
    DeploymentQuarantined(String),
    #[error("timed out waiting for deployment receipt; last operation state: {0}")]
    DeploymentReceiptTimeout(String),
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
            Self::ServiceNotFound => (-32011, "service not found".into(), None),
            Self::CodeHashMismatch => (-32012, "service code hash mismatch".into(), None),
            Self::TransactionNotFound => (-32013, "transaction not found".into(), None),
            Self::DeploymentRejected(code) => (
                -32014,
                format!("deployment rejected with code {code}"),
                None,
            ),
            Self::DeploymentQuarantined(reason) => (-32015, reason, None),
            Self::DeploymentReceiptTimeout(state) => (
                -32016,
                format!("deployment receipt timeout; state: {state}"),
                None,
            ),
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
        match rpc.admission.clone().try_acquire_owned() {
            Err(_) => Err(RpcError::Busy),
            Ok(_permit) => match request.method.as_str() {
                "minijam_submitTransactionV1" => {
                    match serde_json::from_value::<SubmitTransactionParams>(request.params) {
                        Ok(params) => rpc
                            .submit_transaction(params)
                            .await
                            .map(|value| serde_json::to_value(value).expect("result serializes")),
                        Err(error) => Err(RpcError::InvalidParams(error.to_string())),
                    }
                }
                "minijam_getTransactionStatusV1" => {
                    match serde_json::from_value::<GetTransactionStatusParams>(request.params) {
                        Ok(params) => rpc
                            .transaction_status(params.transaction_id.0)
                            .await
                            .map(|value| serde_json::to_value(value).expect("result serializes")),
                        Err(error) => Err(RpcError::InvalidParams(error.to_string())),
                    }
                }
                "minijam_createServiceV1" => {
                    match serde_json::from_value::<CreateServiceParams>(request.params) {
                        Ok(params) => rpc
                            .create_service(params)
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
        Ok(value) => JsonRpcResponse::ok(id, value),
        Err(error) => {
            let (code, message, data) = error.json_parts();
            JsonRpcResponse::<serde_json::Value>::error(id, code, message, data)
        }
    };
    Json(serde_json::to_value(response).expect("response serializes"))
}

async fn worker_task(State(rpc): State<FormalRpc>) -> Result<Response, RpcError> {
    let _permit = rpc
        .admission
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| RpcError::Busy)?;
    match rpc.worker_task().await? {
        Some(task) => Ok(Json(task).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReportSubmittedRequest {
    package_hash: HashParam,
}
async fn report_submitted(
    State(rpc): State<FormalRpc>,
    Json(request): Json<ReportSubmittedRequest>,
) -> Result<Response, RpcError> {
    let _permit = rpc
        .admission
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| RpcError::Busy)?;
    rpc.mark_reported(request.package_hash.0)?;
    Ok(StatusCode::NO_CONTENT.into_response())
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
async fn ready() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status":"ready"})))
}

async fn wait_for_system_receipt(
    chain: &MiniJamChainClient,
    request_id: Hash,
) -> Result<SystemReceiptV2, RpcError> {
    for _ in 0..120 {
        if let Some(receipt) = chain
            .system_receipt::<SystemReceiptV2>(request_id)
            .await
            .map_err(chain_error)?
        {
            return Ok(receipt);
        }
        let quarantined: Vec<pallet_minijam::QuarantinedSystemOp<minijam_runtime::Runtime>> =
            chain.quarantined_system_ops().await.map_err(chain_error)?;
        if let Some(operation) = quarantined
            .into_iter()
            .find(|operation| operation.op.request_id == request_id)
        {
            return Err(RpcError::DeploymentQuarantined(format!(
                "{:?}",
                operation.error_code
            )));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(RpcError::DeploymentReceiptTimeout("pending".into()))
}
async fn wait_for_service_code_hash(
    chain: &MiniJamChainClient,
    service_id: u32,
    expected: Hash,
) -> Result<FinalizedContext, RpcError> {
    for _ in 0..120 {
        let context = chain.finalized_context().await.map_err(chain_error)?;
        if chain
            .service_code_hash_at(context.block_hash, service_id)
            .await
            .map_err(chain_error)?
            == Some(expected)
        {
            return Ok(context);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(RpcError::Chain(
        "timed out waiting for finalized ServiceInfo/codeHash verification".into(),
    ))
}
fn decode_service_info(bytes: &[u8]) -> Result<jp_core_primitives::types::ServiceInfo, RpcError> {
    let mut raw = bytes;
    let value = minijam_protocol::StateValue::decode(&mut raw)
        .map_err(|error| RpcError::Chain(error.to_string()))?;
    JamDecode::decode(&mut value.as_slice())
        .map_err(|error| RpcError::Chain(format!("invalid finalized ServiceInfo: {error}")))
}
pub fn transaction_id(
    service_id: u32,
    service_code_hash: Hash,
    payload: &[u8],
    extrinsics: &[Vec<u8>],
) -> Hash {
    let transaction = TransactionForId {
        service_id,
        service_code_hash,
        payload: payload.to_vec(),
        extrinsics: extrinsics.to_vec(),
    };
    let mut preimage = b"minijam/transaction/v1".to_vec();
    preimage.extend_from_slice(&transaction.encode());
    blake2_256(&preimage)
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
fn chain_error(error: minijam_chain_client::ChainClientError) -> RpcError {
    RpcError::Chain(error.to_string())
}
fn deployment_chain_error(error: minijam_chain_client::ChainClientError) -> RpcError {
    match error {
        minijam_chain_client::ChainClientError::Dispatch(reason) => {
            RpcError::Chain(format!("deployment dispatch failed: {reason}"))
        }
        other => chain_error(other),
    }
}
fn decode_hash(value: &str) -> Result<Hash, RpcError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() != 64 {
        return Err(RpcError::InvalidParams("expected 32-byte hex".into()));
    }
    let mut hash = [0; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        hash[index] = (hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?;
    }
    Ok(hash)
}
fn hex_nibble(byte: u8) -> Result<u8, RpcError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(RpcError::InvalidParams("invalid hex".into())),
    }
}
fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(2 + bytes.len() * 2);
    value.push_str("0x");
    for byte in bytes {
        value.push_str(&format!("{byte:02x}"));
    }
    value
}
fn hex_without_prefix(bytes: &[u8]) -> String {
    hex(bytes).trim_start_matches("0x").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_id_is_stable_and_input_sensitive() {
        let first = transaction_id(7, [1; 32], b"payload", &[b"extrinsic".to_vec()]);
        assert_eq!(
            first,
            transaction_id(7, [1; 32], b"payload", &[b"extrinsic".to_vec()])
        );
        assert_ne!(
            first,
            transaction_id(7, [1; 32], b"other", &[b"extrinsic".to_vec()])
        );
        assert_ne!(
            first,
            transaction_id(8, [1; 32], b"payload", &[b"extrinsic".to_vec()])
        );
    }
}

pub async fn run_from_env() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind: SocketAddr = std::env::var("MINIJAM_FORMAL_RPC_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8090".into())
        .parse()?;
    let rpc_url = std::env::var("MINIJAM_RPC_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".into());
    let signer_uri = match std::env::var("MINIJAM_SIGNER_URI_FILE") {
        Ok(path) => std::fs::read_to_string(path)?.trim().to_owned(),
        Err(_) => std::env::var("MINIJAM_SIGNER_URI")
            .or_else(|_| {
                std::env::var("MINIJAM_WORKER_SEED_FILE").and_then(|path| {
                    std::fs::read_to_string(path).map_err(|_| std::env::VarError::NotPresent)
                })
            })
            .map(|value| value.trim().to_owned())?,
    };
    sr25519::Pair::from_string(&signer_uri, None).map_err(|error| error.to_string())?;
    let bundle_dir =
        PathBuf::from(std::env::var("MINIJAM_BUNDLE_DIR").unwrap_or_else(|_| "bundles".into()));
    let chain =
        Arc::new(connect_chain_with_retry(&rpc_url, &signer_uri, Duration::from_secs(15)).await?);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, FormalRpc::new(chain, bundle_dir)?.router()).await?;
    Ok(())
}
