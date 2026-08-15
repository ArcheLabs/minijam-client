// SPDX-License-Identifier: Apache-2.0

use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use jp_core_primitives::{simple::ByteSequence, types::Preimage};
use minijam_chain_client::{FinalizedContext, PreparedSystemOperation, Submission};
use minijam_protocol::{StateValue, SystemReceiptV1};
use parity_scale_codec::{Decode, Encode};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair};
use thiserror::Error;
use tower_http::cors::{AllowHeaders, Any, CorsLayer};

pub const ACTION_DOMAIN: &[u8] = b"minijam/playground-action/v1";
static ACTION_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct PlaygroundConfig {
    pub genesis_hash: [u8; 32],
    pub compiler_url: String,
    pub bundle_dir: std::path::PathBuf,
}

#[derive(Clone)]
pub struct Playground {
    config: PlaygroundConfig,
    db: Arc<Mutex<Connection>>,
    compiler: reqwest::Client,
    chain: Option<Arc<dyn ChainGateway>>,
    allocation_chain: Option<Arc<dyn ChainGateway>>,
}

#[async_trait]
pub trait ChainGateway: Send + Sync {
    async fn finalized_context(&self) -> Result<FinalizedContext, ApiError>;
    async fn controller_at(
        &self,
        block: [u8; 32],
        service_id: u32,
    ) -> Result<Option<[u8; 32]>, ApiError>;
    async fn service_info_at(
        &self,
        block: [u8; 32],
        service_id: u32,
    ) -> Result<Option<Vec<u8>>, ApiError>;
    async fn service_storage_at(
        &self,
        block: [u8; 32],
        service_id: u32,
        key: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, ApiError>;
    async fn service_preimage_at(
        &self,
        block: [u8; 32],
        service_id: u32,
        code_hash: [u8; 32],
    ) -> Result<Option<Vec<u8>>, ApiError>;
    async fn prepare_create(
        &self,
        controller: [u8; 32],
        code_hash: [u8; 32],
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    ) -> Result<PreparedSystemOperation, ApiError>;
    async fn prepare_upgrade(
        &self,
        controller: [u8; 32],
        service_id: u32,
        code_hash: [u8; 32],
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    ) -> Result<PreparedSystemOperation, ApiError>;
    async fn submit_prepared_system_op(
        &self,
        prepared: PreparedSystemOperation,
    ) -> Result<Submission, ApiError>;
    async fn system_receipt(
        &self,
        request_id: [u8; 32],
    ) -> Result<Option<SystemReceiptV1>, ApiError>;
    async fn submit_preimage(&self, canonical: Vec<u8>) -> Result<Submission, ApiError>;
    async fn submit_work(
        &self,
        canonical: Vec<u8>,
        bundle_ref: minijam_protocol::ContentRef,
        package_hash: [u8; 32],
    ) -> Result<Submission, ApiError>;
    async fn submit_allocation(
        &self,
        allocation_id: u64,
        target_service: u32,
        amount: u128,
    ) -> Result<Submission, ApiError>;
    async fn work_id_by_package_hash(
        &self,
        package_hash: [u8; 32],
    ) -> Result<Option<u64>, ApiError>;
    async fn work_terminal(&self, work_id: u64) -> Result<Option<Result<[u8; 32], ()>>, ApiError>;
}

#[async_trait]
impl ChainGateway for minijam_chain_client::MiniJamChainClient {
    async fn finalized_context(&self) -> Result<FinalizedContext, ApiError> {
        self.finalized_context()
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn controller_at(
        &self,
        block: [u8; 32],
        service_id: u32,
    ) -> Result<Option<[u8; 32]>, ApiError> {
        let Some(encoded) = self
            .service_controller_at(block, service_id)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))?
        else {
            return Ok(None);
        };
        let value = StateValue::decode(&mut encoded.as_slice())
            .map_err(|error| ApiError::Chain(error.to_string()))?;
        value
            .into_inner()
            .try_into()
            .map(Some)
            .map_err(|_| ApiError::Chain("controller is not 32 bytes".into()))
    }

    async fn service_info_at(
        &self,
        block: [u8; 32],
        service_id: u32,
    ) -> Result<Option<Vec<u8>>, ApiError> {
        self.service_info_at(block, service_id)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn service_storage_at(
        &self,
        block: [u8; 32],
        service_id: u32,
        key: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, ApiError> {
        self.service_storage_at(block, service_id, &key)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn service_preimage_at(
        &self,
        block: [u8; 32],
        service_id: u32,
        code_hash: [u8; 32],
    ) -> Result<Option<Vec<u8>>, ApiError> {
        self.service_preimage_at(block, service_id, code_hash)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn prepare_create(
        &self,
        controller: [u8; 32],
        code_hash: [u8; 32],
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    ) -> Result<PreparedSystemOperation, ApiError> {
        self.prepare_create_service(controller, code_hash, code_len, min_item_gas, min_memo_gas)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn prepare_upgrade(
        &self,
        controller: [u8; 32],
        service_id: u32,
        code_hash: [u8; 32],
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    ) -> Result<PreparedSystemOperation, ApiError> {
        self.prepare_upgrade_service(
            controller,
            service_id,
            code_hash,
            code_len,
            min_item_gas,
            min_memo_gas,
        )
        .await
        .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn submit_prepared_system_op(
        &self,
        prepared: PreparedSystemOperation,
    ) -> Result<Submission, ApiError> {
        self.submit_prepared_extrinsic(prepared)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn system_receipt(
        &self,
        request_id: [u8; 32],
    ) -> Result<Option<SystemReceiptV1>, ApiError> {
        let Some(encoded) = self
            .system_receipt::<StateValue>(request_id)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))?
        else {
            return Ok(None);
        };
        SystemReceiptV1::decode(&mut encoded.into_inner().as_slice())
            .map(Some)
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn submit_preimage(&self, canonical: Vec<u8>) -> Result<Submission, ApiError> {
        self.submit_preimage(canonical)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn submit_work(
        &self,
        canonical: Vec<u8>,
        bundle_ref: minijam_protocol::ContentRef,
        package_hash: [u8; 32],
    ) -> Result<Submission, ApiError> {
        self.submit_work(canonical, bundle_ref, package_hash)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn submit_allocation(
        &self,
        allocation_id: u64,
        target_service: u32,
        amount: u128,
    ) -> Result<Submission, ApiError> {
        self.submit_allocation(allocation_id, target_service, amount)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn work_id_by_package_hash(
        &self,
        package_hash: [u8; 32],
    ) -> Result<Option<u64>, ApiError> {
        self.work_id_by_package_hash(package_hash)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))
    }

    async fn work_terminal(&self, work_id: u64) -> Result<Option<Result<[u8; 32], ()>>, ApiError> {
        let Some(work) = self
            .work_status::<pallet_minijam::WorkRecord<minijam_runtime::Runtime>>(work_id)
            .await
            .map_err(|error| ApiError::Chain(error.to_string()))?
        else {
            return Ok(None);
        };
        match work.status {
            pallet_minijam::WorkStatus::Imported => self
                .execution_receipt(work_id)
                .await
                .map(|receipt| receipt.map(Ok))
                .map_err(|error| ApiError::Chain(error.to_string())),
            pallet_minijam::WorkStatus::Failed => Ok(Some(Err(()))),
            _ => Ok(None),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareActionRequest {
    pub account: String,
    pub action: String,
    pub params_hash: String,
    pub expiry: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAction {
    pub action_id: String,
    pub account: String,
    pub action: String,
    pub params_hash: String,
    pub domain: String,
    pub genesis: String,
    pub expiry: u64,
    pub signing_payload: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionAuthorization {
    pub action_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateServiceRequest {
    pub authorization: ActionAuthorization,
    pub blob_base64: String,
    pub code_hash: String,
    pub min_item_gas: u64,
    pub min_memo_gas: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeServiceRequest {
    pub authorization: ActionAuthorization,
    pub service_id: u32,
    pub blob_base64: String,
    pub code_hash: String,
    pub min_item_gas: u64,
    pub min_memo_gas: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitWorkRequest {
    pub authorization: ActionAuthorization,
    pub service_id: u32,
    pub service_code_hash: String,
    pub payload_base64: String,
    #[serde(default)]
    pub extrinsics_base64: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitAllocationRequest {
    pub allocation_id: u64,
    pub target_service: u32,
    pub amount: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSubmission {
    pub extrinsic_hash: String,
    pub submitted_nonce: u32,
    pub correlation: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    pub service_id: u32,
    pub controller: String,
    pub code_hash: String,
    pub code_length: u64,
    pub balance: u64,
    pub preimage_ready: bool,
    pub finalized_block: String,
    pub finalized_block_number: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StorageQuery {
    pub key: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageView {
    pub service_id: u32,
    pub key: String,
    pub value: Option<String>,
    pub finalized_block: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Create,
    Upgrade,
    Work,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Prepared,
    Submitted,
    WaitingReceipt,
    SubmittingPreimage,
    WaitingPreimage,
    TrackingWork,
    Succeeded,
    Failed,
}

impl OperationStatus {
    fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub operation_id: String,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub account: String,
    pub action_id: String,
    pub request: serde_json::Value,
    pub correlation: Option<String>,
    pub extrinsic_hash: Option<String>,
    pub submitted_nonce: Option<u32>,
    #[serde(skip_serializing)]
    pub encoded_extrinsic: Option<Vec<u8>>,
    #[serde(skip_serializing)]
    pub system_op_nonce: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("signed action expired")]
    Expired,
    #[error("signed action was already used")]
    Replayed,
    #[error("signature does not match action account")]
    InvalidSignature,
    #[error("signed action parameters do not match request")]
    ParamsMismatch,
    #[error("storage error: {0}")]
    Storage(String),
    #[error("compiler unavailable: {0}")]
    Compiler(String),
    #[error("chain unavailable: {0}")]
    Chain(String),
    #[error("account is not the finalized Service Controller")]
    Forbidden,
    #[error("resource not found")]
    NotFound,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Invalid(_) | Self::InvalidSignature | Self::ParamsMismatch => {
                StatusCode::BAD_REQUEST
            }
            Self::Expired | Self::Replayed => StatusCode::CONFLICT,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Storage(_) | Self::Compiler(_) | Self::Chain(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
        };
        (
            status,
            Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}

impl Playground {
    pub fn open(path: &Path, config: PlaygroundConfig) -> Result<Self, ApiError> {
        std::fs::create_dir_all(&config.bundle_dir)
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        let connection =
            Connection::open(path).map_err(|error| ApiError::Storage(error.to_string()))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS signed_actions (
                   action_id BLOB PRIMARY KEY,
                   account BLOB NOT NULL,
                   action TEXT NOT NULL,
                   params_hash BLOB NOT NULL,
                   genesis_hash BLOB NOT NULL,
                   expiry INTEGER NOT NULL,
                   used_at INTEGER
                 );
                 CREATE TABLE IF NOT EXISTS operations (
                   operation_id TEXT PRIMARY KEY,
                   kind TEXT NOT NULL,
                   status TEXT NOT NULL,
                   account BLOB NOT NULL,
                   action_id BLOB NOT NULL UNIQUE,
                   request_json TEXT NOT NULL,
                   correlation BLOB,
                   extrinsic_hash BLOB,
                   submitted_nonce INTEGER,
                   encoded_extrinsic BLOB,
                   system_op_nonce INTEGER,
                   result_json TEXT,
                   error TEXT,
                   created_at INTEGER NOT NULL,
                   updated_at INTEGER NOT NULL
                 );",
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        ensure_operation_column(&connection, "encoded_extrinsic", "BLOB")?;
        ensure_operation_column(&connection, "system_op_nonce", "INTEGER")?;
        Ok(Self {
            config,
            db: Arc::new(Mutex::new(connection)),
            compiler: reqwest::Client::new(),
            chain: None,
            allocation_chain: None,
        })
    }

    pub fn with_chain(mut self, chain: Arc<dyn ChainGateway>) -> Self {
        self.chain = Some(chain);
        self
    }

    pub fn with_allocation_chain(mut self, chain: Arc<dyn ChainGateway>) -> Self {
        self.allocation_chain = Some(chain);
        self
    }

    pub fn start_recovery(&self) {
        let playground = self.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(operations) = playground.recoverable_operations() {
                    for operation in operations {
                        let _ = playground.process_service_operation(operation).await;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    fn chain(&self) -> Result<&Arc<dyn ChainGateway>, ApiError> {
        self.chain
            .as_ref()
            .ok_or_else(|| ApiError::Chain("chain client is not configured".into()))
    }

    fn allocation_chain(&self) -> Result<&Arc<dyn ChainGateway>, ApiError> {
        self.allocation_chain
            .as_ref()
            .or(self.chain.as_ref())
            .ok_or_else(|| ApiError::Chain("allocation chain client is not configured".into()))
    }

    async fn process_service_operation(&self, operation: Operation) -> Result<(), ApiError> {
        if operation.kind == OperationKind::Work {
            return self.process_work_operation(operation).await;
        }
        let account = decode_array::<32>(&operation.account)?;
        let blob = STANDARD
            .decode(json_string(&operation.request, "blobBase64")?)
            .map_err(|error| ApiError::Invalid(error.to_string()))?;
        let code_hash = decode_array::<32>(json_string(&operation.request, "codeHash")?)?;
        let min_item_gas = json_u64(&operation.request, "minItemGas")?;
        let min_memo_gas = json_u64(&operation.request, "minMemoGas")?;

        if operation.status == OperationStatus::Prepared {
            let prepared = match operation.encoded_extrinsic.clone() {
                Some(encoded_extrinsic) => PreparedSystemOperation {
                    encoded_extrinsic,
                    submitted_nonce: operation.submitted_nonce.ok_or_else(|| {
                        ApiError::Storage("prepared operation has no transaction nonce".into())
                    })?,
                    system_op_nonce: operation.system_op_nonce.ok_or_else(|| {
                        ApiError::Storage("prepared operation has no system operation nonce".into())
                    })?,
                    correlation: decode_array::<32>(operation.correlation.as_deref().ok_or_else(
                        || ApiError::Storage("prepared operation has no request id".into()),
                    )?)?,
                },
                None => {
                    let prepared = match operation.kind {
                        OperationKind::Create => {
                            self.chain()?
                                .prepare_create(
                                    account,
                                    code_hash,
                                    blob.len() as u32,
                                    min_item_gas,
                                    min_memo_gas,
                                )
                                .await?
                        }
                        OperationKind::Upgrade => {
                            self.chain()?
                                .prepare_upgrade(
                                    account,
                                    json_u64(&operation.request, "serviceId")? as u32,
                                    code_hash,
                                    blob.len() as u32,
                                    min_item_gas,
                                    min_memo_gas,
                                )
                                .await?
                        }
                        _ => unreachable!(),
                    };
                    self.persist_prepared_system_op(&operation.operation_id, &prepared)?;
                    prepared
                }
            };
            let submission = self.chain()?.submit_prepared_system_op(prepared).await?;
            self.update_operation(
                &operation.operation_id,
                OperationStatus::WaitingReceipt,
                Some(submission.correlation),
                Some(submission.extrinsic_hash),
                Some(submission.submitted_nonce),
                None,
                None,
            )?;
            return Ok(());
        }

        if operation.status == OperationStatus::WaitingReceipt {
            let request_id = decode_array::<32>(
                operation
                    .correlation
                    .as_deref()
                    .ok_or_else(|| ApiError::Storage("operation has no request id".into()))?,
            )?;
            let Some(receipt) = self.chain()?.system_receipt(request_id).await? else {
                return Ok(());
            };
            let service_id = match receipt {
                SystemReceiptV1::ServiceCreated {
                    service_id,
                    controller,
                }
                | SystemReceiptV1::ServiceUpgraded {
                    service_id,
                    controller,
                    ..
                } if controller == account => service_id,
                SystemReceiptV1::Rejected { code } => {
                    self.update_operation(
                        &operation.operation_id,
                        OperationStatus::Failed,
                        None,
                        None,
                        None,
                        None,
                        Some(&format!("system operation rejected with code {code}")),
                    )?;
                    return Ok(());
                }
                _ => return Err(ApiError::Chain("receipt controller mismatch".into())),
            };
            let canonical = jam_codec::Encode::encode(&Preimage {
                requester: service_id,
                blob: ByteSequence::from(blob),
            });
            let preimage = self.chain()?.submit_preimage(canonical).await?;
            self.update_operation(
                &operation.operation_id,
                OperationStatus::WaitingPreimage,
                None,
                Some(preimage.extrinsic_hash),
                Some(preimage.submitted_nonce),
                Some(&serde_json::json!({
                    "serviceId": service_id,
                    "preimageHash": hex(&preimage.correlation),
                })),
                None,
            )?;
            return Ok(());
        }

        if operation.status == OperationStatus::WaitingPreimage {
            let service_id = operation
                .result
                .as_ref()
                .and_then(|result| result.get("serviceId"))
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| ApiError::Storage("operation has no service id".into()))?
                as u32;
            let finalized = self.chain()?.finalized_context().await?;
            let Some(encoded) = self
                .chain()?
                .service_preimage_at(finalized.block_hash, service_id, code_hash)
                .await?
            else {
                return Ok(());
            };
            let finalized_blob = decode_state_value(&encoded)?;
            if finalized_blob.len() != blob.len()
                || minijam_protocol::blake2_256(&finalized_blob) != code_hash
            {
                return Err(ApiError::Chain(
                    "finalized Service preimage does not match requested code".into(),
                ));
            }
            self.update_operation(
                &operation.operation_id,
                OperationStatus::Succeeded,
                None,
                None,
                None,
                None,
                None,
            )?;
        }
        Ok(())
    }

    async fn process_work_operation(&self, operation: Operation) -> Result<(), ApiError> {
        let package_hash = decode_array::<32>(json_string(&operation.request, "packageHash")?)?;
        if operation.status == OperationStatus::Prepared {
            if let Some(work_id) = self.chain()?.work_id_by_package_hash(package_hash).await? {
                self.update_operation(
                    &operation.operation_id,
                    OperationStatus::TrackingWork,
                    Some(package_hash),
                    None,
                    None,
                    Some(&serde_json::json!({"workId": work_id})),
                    None,
                )?;
                return Ok(());
            }
            let canonical = STANDARD
                .decode(json_string(&operation.request, "canonicalWorkPackage")?)
                .map_err(|error| ApiError::Invalid(error.to_string()))?;
            let cid = cid::Cid::try_from(json_string(&operation.request, "bundleCid")?)
                .map_err(|error| ApiError::Invalid(error.to_string()))?;
            let bundle_ref = minijam_protocol::ContentRef {
                cid_v1: cid
                    .to_bytes()
                    .try_into()
                    .map_err(|_| ApiError::Invalid("CID exceeds protocol bounds".into()))?,
                content_hash: decode_array::<32>(json_string(&operation.request, "bundleHash")?)?,
                size: json_u64(&operation.request, "bundleSize")?,
            };
            let submission = self
                .chain()?
                .submit_work(canonical, bundle_ref, package_hash)
                .await?;
            self.update_operation(
                &operation.operation_id,
                OperationStatus::TrackingWork,
                Some(package_hash),
                Some(submission.extrinsic_hash),
                Some(submission.submitted_nonce),
                None,
                None,
            )?;
            return Ok(());
        }
        if operation.status == OperationStatus::TrackingWork {
            let Some(work_id) = self.chain()?.work_id_by_package_hash(package_hash).await? else {
                return Ok(());
            };
            match self.chain()?.work_terminal(work_id).await? {
                Some(Ok(receipt)) => self.update_operation(
                    &operation.operation_id,
                    OperationStatus::Succeeded,
                    None,
                    None,
                    None,
                    Some(&serde_json::json!({
                        "workId": work_id,
                        "executionReceipt": hex(&receipt),
                    })),
                    None,
                )?,
                Some(Err(())) => self.update_operation(
                    &operation.operation_id,
                    OperationStatus::Failed,
                    None,
                    None,
                    None,
                    Some(&serde_json::json!({"workId": work_id})),
                    Some("work failed"),
                )?,
                None => {}
            }
        }
        Ok(())
    }

    fn save_bundle(&self, bytes: &[u8], expected_hash: [u8; 32]) -> Result<(), ApiError> {
        if minijam_protocol::blake2_256(bytes) != expected_hash {
            return Err(ApiError::Invalid("bundle hash mismatch".into()));
        }
        let path = self
            .config
            .bundle_dir
            .join(hex_without_prefix(&expected_hash));
        if path.exists() {
            let existing =
                std::fs::read(path).map_err(|error| ApiError::Storage(error.to_string()))?;
            if existing != bytes {
                return Err(ApiError::Storage("bundle hash collision".into()));
            }
            return Ok(());
        }
        let temporary = self
            .config
            .bundle_dir
            .join(format!(".{}.tmp", hex_without_prefix(&expected_hash)));
        std::fs::write(&temporary, bytes)
            .and_then(|_| std::fs::rename(&temporary, &path))
            .map_err(|error| ApiError::Storage(error.to_string()))
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/api/v1/build", post(build))
            .route("/api/v1/config", get(get_config))
            .route("/api/v1/actions/prepare", post(prepare_action))
            .route("/api/v1/services", post(create_service))
            .route("/api/v1/services/{id}", get(get_service))
            .route("/api/v1/services/{id}/storage", get(get_service_storage))
            .route("/api/v1/services/{id}/upgrade", post(upgrade_service))
            .route("/api/v1/work", post(submit_work))
            .route("/api/v1/allocations", post(submit_allocation))
            .route("/api/v1/operations/{id}", get(get_operation))
            .route("/ipfs/{cid}", get(get_bundle))
            .route("/health/live", get(|| async { StatusCode::NO_CONTENT }))
            .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
            .route("/readyz", get(ready))
            .route("/health/ready", get(ready))
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                    .allow_headers(AllowHeaders::mirror_request()),
            )
            .with_state(self)
    }

    fn prepare(&self, request: PrepareActionRequest) -> Result<PreparedAction, ApiError> {
        let account = decode_array::<32>(&request.account)?;
        let params_hash = decode_array::<32>(&request.params_hash)?;
        if request.action.is_empty() {
            return Err(ApiError::Invalid("action must not be empty".into()));
        }
        if request.expiry <= now() {
            return Err(ApiError::Expired);
        }
        let seed = (
            account,
            request.action.as_bytes(),
            params_hash,
            request.expiry,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            ACTION_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        )
            .encode();
        let action_id = minijam_protocol::blake2_256(&seed);
        let payload = action_payload(
            action_id,
            account,
            &request.action,
            params_hash,
            self.config.genesis_hash,
            request.expiry,
        );
        let signing_hash = minijam_protocol::blake2_256(&payload);
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .execute(
                "INSERT OR IGNORE INTO signed_actions
                 (action_id, account, action, params_hash, genesis_hash, expiry)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    action_id.as_slice(),
                    account.as_slice(),
                    request.action,
                    params_hash.as_slice(),
                    self.config.genesis_hash.as_slice(),
                    request.expiry as i64,
                ],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        Ok(PreparedAction {
            action_id: hex(&action_id),
            account: hex(&account),
            action: request.action,
            params_hash: hex(&params_hash),
            domain: String::from_utf8_lossy(ACTION_DOMAIN).into_owned(),
            genesis: hex(&self.config.genesis_hash),
            expiry: request.expiry,
            signing_payload: hex(&signing_hash),
        })
    }

    pub fn consume_action(
        &self,
        authorization: &ActionAuthorization,
        expected_action: &str,
        expected_params_hash: [u8; 32],
    ) -> Result<[u8; 32], ApiError> {
        let action_id = decode_array::<32>(&authorization.action_id)?;
        let signature = decode_array::<64>(&authorization.signature)?;
        let mut connection = self.db.lock().expect("playground db mutex poisoned");
        let transaction = connection
            .transaction()
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        let row = transaction
            .query_row(
                "SELECT account, action, params_hash, genesis_hash, expiry, used_at
                 FROM signed_actions WHERE action_id = ?1",
                params![action_id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)? as u64,
                        row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                    ))
                },
            )
            .optional()
            .map_err(|error| ApiError::Storage(error.to_string()))?
            .ok_or_else(|| ApiError::Invalid("unknown action id".into()))?;
        if row.5.is_some() {
            return Err(ApiError::Replayed);
        }
        if row.4 <= now() {
            return Err(ApiError::Expired);
        }
        let account: [u8; 32] = row
            .0
            .try_into()
            .map_err(|_| ApiError::Invalid("bad account".into()))?;
        let params_hash: [u8; 32] = row
            .2
            .try_into()
            .map_err(|_| ApiError::Invalid("bad params hash".into()))?;
        let genesis: [u8; 32] = row
            .3
            .try_into()
            .map_err(|_| ApiError::Invalid("bad genesis".into()))?;
        if row.1 != expected_action || params_hash != expected_params_hash {
            return Err(ApiError::ParamsMismatch);
        }
        let signing_hash = minijam_protocol::blake2_256(&action_payload(
            action_id,
            account,
            &row.1,
            params_hash,
            genesis,
            row.4,
        ));
        let signing_message = wrap_signing_bytes(&signing_hash);
        if !sr25519::Pair::verify(
            &sr25519::Signature::from_raw(signature),
            &signing_message,
            &sr25519::Public::from_raw(account),
        ) {
            return Err(ApiError::InvalidSignature);
        }
        let changed = transaction
            .execute(
                "UPDATE signed_actions SET used_at = ?2
                 WHERE action_id = ?1 AND used_at IS NULL",
                params![action_id.as_slice(), now() as i64],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        if changed != 1 {
            return Err(ApiError::Replayed);
        }
        transaction
            .commit()
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        Ok(account)
    }

    pub fn insert_operation(
        &self,
        kind: OperationKind,
        account: [u8; 32],
        action_id: [u8; 32],
        request: &serde_json::Value,
    ) -> Result<Operation, ApiError> {
        let created_at = now();
        let operation_id = hex(&minijam_protocol::blake2_256(
            &(
                b"minijam/operation/v1".as_slice(),
                account,
                action_id,
                created_at,
                OPERATION_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            )
                .encode(),
        ));
        let request_json =
            serde_json::to_string(request).map_err(|error| ApiError::Invalid(error.to_string()))?;
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .execute(
                "INSERT INTO operations
                 (operation_id, kind, status, account, action_id, request_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    operation_id,
                    enum_json(kind)?,
                    enum_json(OperationStatus::Prepared)?,
                    account.as_slice(),
                    action_id.as_slice(),
                    request_json,
                    created_at as i64,
                ],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        self.operation(&operation_id)?
            .ok_or_else(|| ApiError::Storage("inserted operation disappeared".into()))
    }

    pub fn operation(&self, operation_id: &str) -> Result<Option<Operation>, ApiError> {
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .query_row(
                "SELECT operation_id, kind, status, account, action_id, request_json,
                        correlation, extrinsic_hash, submitted_nonce, encoded_extrinsic,
                        system_op_nonce, result_json, error,
                        created_at, updated_at
                 FROM operations WHERE operation_id = ?1",
                params![operation_id],
                decode_operation,
            )
            .optional()
            .map_err(|error| ApiError::Storage(error.to_string()))
    }

    pub fn recoverable_operations(&self) -> Result<Vec<Operation>, ApiError> {
        let connection = self.db.lock().expect("playground db mutex poisoned");
        let mut statement = connection
            .prepare(
                "SELECT operation_id, kind, status, account, action_id, request_json,
                        correlation, extrinsic_hash, submitted_nonce, encoded_extrinsic,
                        system_op_nonce, result_json, error,
                        created_at, updated_at
                 FROM operations ORDER BY created_at, operation_id",
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([], decode_operation)
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        let mut operations = Vec::new();
        for row in rows {
            let operation = row.map_err(|error| ApiError::Storage(error.to_string()))?;
            if !operation.status.terminal() {
                operations.push(operation);
            }
        }
        Ok(operations)
    }

    pub fn update_operation(
        &self,
        operation_id: &str,
        status: OperationStatus,
        correlation: Option<[u8; 32]>,
        extrinsic_hash: Option<[u8; 32]>,
        submitted_nonce: Option<u32>,
        result: Option<&serde_json::Value>,
        error: Option<&str>,
    ) -> Result<(), ApiError> {
        let result = result
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .execute(
                "UPDATE operations SET status = ?2, correlation = COALESCE(?3, correlation),
                 extrinsic_hash = COALESCE(?4, extrinsic_hash),
                 submitted_nonce = COALESCE(?5, submitted_nonce),
                 result_json = COALESCE(?6, result_json), error = ?7, updated_at = ?8
                 WHERE operation_id = ?1",
                params![
                    operation_id,
                    enum_json(status)?,
                    correlation.as_ref().map(|value| value.as_slice()),
                    extrinsic_hash.as_ref().map(|value| value.as_slice()),
                    submitted_nonce,
                    result,
                    error,
                    now() as i64,
                ],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        Ok(())
    }

    fn persist_prepared_system_op(
        &self,
        operation_id: &str,
        prepared: &PreparedSystemOperation,
    ) -> Result<(), ApiError> {
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .execute(
                "UPDATE operations SET correlation = ?2, submitted_nonce = ?3,
                 encoded_extrinsic = ?4, system_op_nonce = ?5, updated_at = ?6
                 WHERE operation_id = ?1 AND status = 'prepared'",
                params![
                    operation_id,
                    prepared.correlation.as_slice(),
                    prepared.submitted_nonce,
                    prepared.encoded_extrinsic,
                    prepared.system_op_nonce as i64,
                    now() as i64,
                ],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        Ok(())
    }
}

async fn create_service(
    State(playground): State<Playground>,
    Json(request): Json<CreateServiceRequest>,
) -> Result<(StatusCode, Json<Operation>), ApiError> {
    decode_service_blob(&request.blob_base64, &request.code_hash)?;
    let params = serde_json::json!({
        "blobBase64": request.blob_base64,
        "codeHash": request.code_hash,
        "minItemGas": request.min_item_gas,
        "minMemoGas": request.min_memo_gas,
    });
    let params_hash = hash_json(&params)?;
    let account =
        playground.consume_action(&request.authorization, "create_service", params_hash)?;
    let action_id = decode_array::<32>(&request.authorization.action_id)?;
    let operation =
        playground.insert_operation(OperationKind::Create, account, action_id, &params)?;
    let runner = playground.clone();
    let queued = operation.clone();
    tokio::spawn(async move {
        let _ = runner.process_service_operation(queued).await;
    });
    Ok((StatusCode::ACCEPTED, Json(operation)))
}

async fn upgrade_service(
    State(playground): State<Playground>,
    axum::extract::Path(service_id): axum::extract::Path<u32>,
    Json(request): Json<UpgradeServiceRequest>,
) -> Result<(StatusCode, Json<Operation>), ApiError> {
    if service_id != request.service_id {
        return Err(ApiError::Invalid(
            "path service id does not match body".into(),
        ));
    }
    decode_service_blob(&request.blob_base64, &request.code_hash)?;
    let params = serde_json::json!({
        "serviceId": service_id,
        "blobBase64": request.blob_base64,
        "codeHash": request.code_hash,
        "minItemGas": request.min_item_gas,
        "minMemoGas": request.min_memo_gas,
    });
    let params_hash = hash_json(&params)?;
    let account =
        playground.consume_action(&request.authorization, "upgrade_service", params_hash)?;
    let finalized = playground.chain()?.finalized_context().await?;
    if playground
        .chain()?
        .controller_at(finalized.block_hash, service_id)
        .await?
        != Some(account)
    {
        return Err(ApiError::Forbidden);
    }
    let action_id = decode_array::<32>(&request.authorization.action_id)?;
    let operation =
        playground.insert_operation(OperationKind::Upgrade, account, action_id, &params)?;
    let runner = playground.clone();
    let queued = operation.clone();
    tokio::spawn(async move {
        let _ = runner.process_service_operation(queued).await;
    });
    Ok((StatusCode::ACCEPTED, Json(operation)))
}

async fn submit_work(
    State(playground): State<Playground>,
    Json(request): Json<SubmitWorkRequest>,
) -> Result<(StatusCode, Json<Operation>), ApiError> {
    let service_code_hash = decode_array::<32>(&request.service_code_hash)?;
    let payload = STANDARD
        .decode(&request.payload_base64)
        .map_err(|error| ApiError::Invalid(error.to_string()))?;
    let extrinsics = request
        .extrinsics_base64
        .iter()
        .map(|value| {
            STANDARD
                .decode(value)
                .map_err(|error| ApiError::Invalid(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let signed_params = serde_json::json!({
        "serviceId": request.service_id,
        "serviceCodeHash": request.service_code_hash,
        "payloadBase64": request.payload_base64,
        "extrinsicsBase64": request.extrinsics_base64,
    });
    let params_hash = hash_json(&signed_params)?;
    let account = playground.consume_action(&request.authorization, "work", params_hash)?;
    let finalized = playground.chain()?.finalized_context().await?;
    let built = minijam_work_package_builder::build_work_package(
        minijam_work_package_builder::BuildWorkInput {
            service_id: request.service_id,
            service_code_hash,
            payload,
            extrinsics,
            anchor_hash: finalized.block_hash,
            state_root: finalized.state_root,
            lookup_anchor_slot: finalized.slot,
        },
    )
    .map_err(|error| ApiError::Invalid(error.to_string()))?;
    playground.save_bundle(&built.bundle_bytes, built.content_ref.content_hash)?;
    let bundle_cid = cid::Cid::read_bytes(built.content_ref.cid_v1.as_slice())
        .map_err(|error| ApiError::Invalid(error.to_string()))?;
    let operation_request = serde_json::json!({
        "serviceId": request.service_id,
        "serviceCodeHash": request.service_code_hash,
        "payloadBase64": request.payload_base64,
        "extrinsicsBase64": request.extrinsics_base64,
        "packageHash": hex(&built.package_hash),
        "canonicalWorkPackage": STANDARD.encode(&built.canonical_work_package),
        "bundleCid": bundle_cid.to_string(),
        "bundleHash": hex(&built.content_ref.content_hash),
        "bundleSize": built.content_ref.size,
    });
    let action_id = decode_array::<32>(&request.authorization.action_id)?;
    let operation =
        playground.insert_operation(OperationKind::Work, account, action_id, &operation_request)?;
    let runner = playground.clone();
    let queued = operation.clone();
    tokio::spawn(async move {
        let _ = runner.process_work_operation(queued).await;
    });
    Ok((StatusCode::ACCEPTED, Json(operation)))
}

async fn submit_allocation(
    State(playground): State<Playground>,
    Json(request): Json<SubmitAllocationRequest>,
) -> Result<Json<AllocationSubmission>, ApiError> {
    if request.allocation_id == 0 {
        return Err(ApiError::Invalid("allocationId must be non-zero".into()));
    }
    if request.amount == 0 {
        return Err(ApiError::Invalid("amount must be non-zero".into()));
    }
    let submission = playground
        .allocation_chain()?
        .submit_allocation(
            request.allocation_id,
            request.target_service,
            request.amount,
        )
        .await?;
    Ok(Json(AllocationSubmission {
        extrinsic_hash: hex(&submission.extrinsic_hash),
        submitted_nonce: submission.submitted_nonce,
        correlation: hex(&submission.correlation),
    }))
}

async fn get_service(
    State(playground): State<Playground>,
    AxumPath(service_id): AxumPath<u32>,
) -> Result<Json<ServiceView>, ApiError> {
    let finalized = playground.chain()?.finalized_context().await?;
    let controller = playground
        .chain()?
        .controller_at(finalized.block_hash, service_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let encoded = playground
        .chain()?
        .service_info_at(finalized.block_hash, service_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let value = StateValue::decode(&mut encoded.as_slice())
        .map_err(|error| ApiError::Chain(error.to_string()))?;
    let info = jam_codec::Decode::decode(&mut value.into_inner().as_slice())
        .map_err(|error| ApiError::Chain(format!("invalid finalized ServiceInfo: {error}")))?;
    let info: jp_core_primitives::types::ServiceInfo = info;
    let preimage = playground
        .chain()?
        .service_preimage_at(finalized.block_hash, service_id, info.code_hash.0)
        .await?;
    let code_length = preimage
        .as_deref()
        .map(decode_state_value)
        .transpose()?
        .map_or(0, |blob| blob.len() as u64);
    Ok(Json(ServiceView {
        service_id,
        controller: hex(&controller),
        code_hash: hex(&info.code_hash.0),
        code_length,
        balance: info.balance,
        preimage_ready: preimage.is_some(),
        finalized_block: hex(&finalized.block_hash),
        finalized_block_number: finalized.block_number,
    }))
}

async fn get_service_storage(
    State(playground): State<Playground>,
    AxumPath(service_id): AxumPath<u32>,
    Query(query): Query<StorageQuery>,
) -> Result<Json<StorageView>, ApiError> {
    let key = decode_hex_bytes(&query.key)?;
    let finalized = playground.chain()?.finalized_context().await?;
    if playground
        .chain()?
        .controller_at(finalized.block_hash, service_id)
        .await?
        .is_none()
    {
        return Err(ApiError::NotFound);
    }
    let value = playground
        .chain()?
        .service_storage_at(finalized.block_hash, service_id, key.clone())
        .await?
        .map(|encoded| decode_state_value(&encoded))
        .transpose()?;
    Ok(Json(StorageView {
        service_id,
        key: hex(&key),
        value: value.as_deref().map(hex),
        finalized_block: hex(&finalized.block_hash),
    }))
}

async fn get_operation(
    State(playground): State<Playground>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Operation>, ApiError> {
    playground
        .operation(&id)?
        .map(Json)
        .ok_or_else(|| ApiError::Invalid("operation not found".into()))
}

async fn get_bundle(
    State(playground): State<Playground>,
    AxumPath(value): AxumPath<String>,
) -> Result<Response, ApiError> {
    let cid =
        cid::Cid::try_from(value.as_str()).map_err(|error| ApiError::Invalid(error.to_string()))?;
    if cid.version() != cid::Version::V1
        || cid.codec() != 0x55
        || cid.hash().code() != 0xb220
        || cid.hash().digest().len() != 32
    {
        return Err(ApiError::Invalid(
            "expected CIDv1 raw Blake2b-256 content identifier".into(),
        ));
    }
    let content_hash: [u8; 32] = cid
        .hash()
        .digest()
        .try_into()
        .map_err(|_| ApiError::Invalid("CID digest is not 32 bytes".into()))?;
    let bytes = std::fs::read(
        playground
            .config
            .bundle_dir
            .join(hex_without_prefix(&content_hash)),
    )
    .map_err(|error| ApiError::Storage(error.to_string()))?;
    if minijam_protocol::blake2_256(&bytes) != content_hash {
        return Err(ApiError::Storage(
            "stored bundle does not match requested CID".into(),
        ));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .body(Body::from(bytes))
        .map_err(|error| ApiError::Storage(error.to_string()))
}

async fn prepare_action(
    State(playground): State<Playground>,
    Json(request): Json<PrepareActionRequest>,
) -> Result<Json<PreparedAction>, ApiError> {
    playground.prepare(request).map(Json)
}

async fn build(
    State(playground): State<Playground>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let response = playground
        .compiler
        .post(format!(
            "{}/internal/v1/compile",
            playground.config.compiler_url.trim_end_matches('/')
        ))
        .json(&request)
        .send()
        .await
        .map_err(|error| ApiError::Compiler(error.to_string()))?;
    let status = response.status();
    let body = response
        .json()
        .await
        .map_err(|error| ApiError::Compiler(error.to_string()))?;
    if !status.is_success() {
        return Err(ApiError::Compiler(format!("compiler returned {status}")));
    }
    Ok(Json(body))
}

async fn get_config(State(playground): State<Playground>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "genesisHash": hex(&playground.config.genesis_hash),
        "actionDomain": String::from_utf8_lossy(ACTION_DOMAIN),
    }))
}

async fn ready(State(playground): State<Playground>) -> StatusCode {
    let database_ready = playground
        .db
        .lock()
        .expect("playground db mutex poisoned")
        .query_row("SELECT 1", [], |row| row.get::<_, u8>(0))
        .is_ok_and(|value| value == 1);
    if !database_ready {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    let compiler_ready = playground
        .compiler
        .get(format!(
            "{}/health/ready",
            playground.config.compiler_url.trim_end_matches('/')
        ))
        .send()
        .await
        .is_ok_and(|response| response.status().is_success());
    let chain_ready = match playground.chain() {
        Ok(chain) => chain.finalized_context().await.is_ok(),
        Err(_) => false,
    };
    if compiler_ready && chain_ready {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

fn action_payload(
    action_id: [u8; 32],
    account: [u8; 32],
    action: &str,
    params_hash: [u8; 32],
    genesis: [u8; 32],
    expiry: u64,
) -> Vec<u8> {
    (
        ACTION_DOMAIN,
        action_id,
        account,
        action.as_bytes(),
        params_hash,
        genesis,
        expiry,
    )
        .encode()
}

fn wrap_signing_bytes(payload: &[u8]) -> Vec<u8> {
    let mut wrapped = Vec::with_capacity(b"<Bytes>".len() + payload.len() + b"</Bytes>".len());
    wrapped.extend_from_slice(b"<Bytes>");
    wrapped.extend_from_slice(payload);
    wrapped.extend_from_slice(b"</Bytes>");
    wrapped
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn hex(bytes: &[u8]) -> String {
    format!(
        "0x{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn hex_without_prefix(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn decode_array<const N: usize>(value: &str) -> Result<[u8; N], ApiError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() != N * 2 {
        return Err(ApiError::Invalid(format!("expected {N}-byte hex")));
    }
    let mut output = [0; N];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|error| ApiError::Invalid(error.to_string()))?;
    }
    Ok(output)
}

fn decode_hex_bytes(value: &str) -> Result<Vec<u8>, ApiError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if !value.len().is_multiple_of(2) {
        return Err(ApiError::Invalid("hex value has odd length".into()));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|error| ApiError::Invalid(error.to_string()))
        })
        .collect()
}

fn decode_state_value(encoded: &[u8]) -> Result<Vec<u8>, ApiError> {
    StateValue::decode(&mut encoded.as_ref())
        .map(StateValue::into_inner)
        .map_err(|error| ApiError::Chain(error.to_string()))
}

fn decode_service_blob(encoded: &str, expected_hash: &str) -> Result<Vec<u8>, ApiError> {
    let blob = STANDARD
        .decode(encoded)
        .map_err(|error| ApiError::Invalid(error.to_string()))?;
    if minijam_protocol::blake2_256(&blob) != decode_array::<32>(expected_hash)? {
        return Err(ApiError::Invalid("code hash does not match Blob".into()));
    }
    Ok(blob)
}

fn hash_json(value: &serde_json::Value) -> Result<[u8; 32], ApiError> {
    serde_json::to_vec(value)
        .map(|bytes| minijam_protocol::blake2_256(&bytes))
        .map_err(|error| ApiError::Invalid(error.to_string()))
}

fn json_string<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str, ApiError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ApiError::Invalid(format!("operation has no {key}")))
}

fn json_u64(value: &serde_json::Value, key: &str) -> Result<u64, ApiError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| ApiError::Invalid(format!("operation has no {key}")))
}

fn enum_json<T: Serialize>(value: T) -> Result<String, ApiError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| ApiError::Storage("invalid enum value".into()))
}

fn ensure_operation_column(
    connection: &Connection,
    name: &str,
    sql_type: &str,
) -> Result<(), ApiError> {
    let mut statement = connection
        .prepare("PRAGMA table_info(operations)")
        .map_err(|error| ApiError::Storage(error.to_string()))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| ApiError::Storage(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ApiError::Storage(error.to_string()))?;
    drop(statement);
    if !columns.iter().any(|column| column == name) {
        connection
            .execute(
                &format!("ALTER TABLE operations ADD COLUMN {name} {sql_type}"),
                [],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
    }
    Ok(())
}

fn decode_operation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Operation> {
    let kind: String = row.get(1)?;
    let status: String = row.get(2)?;
    let account: Vec<u8> = row.get(3)?;
    let action_id: Vec<u8> = row.get(4)?;
    let request: String = row.get(5)?;
    let correlation: Option<Vec<u8>> = row.get(6)?;
    let extrinsic_hash: Option<Vec<u8>> = row.get(7)?;
    let result: Option<String> = row.get(11)?;
    Ok(Operation {
        operation_id: row.get(0)?,
        kind: serde_json::from_value(serde_json::Value::String(kind)).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        status: serde_json::from_value(serde_json::Value::String(status)).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        account: hex(&account),
        action_id: hex(&action_id),
        request: serde_json::from_str(&request).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        correlation: correlation.map(|value| hex(&value)),
        extrinsic_hash: extrinsic_hash.map(|value| hex(&value)),
        submitted_nonce: row.get(8)?,
        encoded_extrinsic: row.get(9)?,
        system_op_nonce: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
        result: result
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
        error: row.get(12)?,
        created_at: row.get::<_, i64>(13)? as u64,
        updated_at: row.get::<_, i64>(14)? as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jam_codec::Decode as _;
    use std::sync::atomic::AtomicUsize;
    use tower::ServiceExt;

    fn sign_prepared(pair: &sr25519::Pair, prepared: PreparedAction) -> ActionAuthorization {
        let signing_hash = decode_array::<32>(&prepared.signing_payload).unwrap();
        ActionAuthorization {
            action_id: prepared.action_id,
            signature: hex(&pair.sign(&wrap_signing_bytes(&signing_hash)).0),
        }
    }

    #[test]
    fn signing_hash_is_wrapped_like_polkadot_bytes() {
        let signing_hash = [0x5a; 32];
        let wrapped = wrap_signing_bytes(&signing_hash);

        assert_eq!(&wrapped[..7], b"<Bytes>");
        assert_eq!(&wrapped[7..39], &signing_hash);
        assert_eq!(&wrapped[39..], b"</Bytes>");
    }

    #[test]
    fn rust_params_hash_matches_shared_playground_vectors() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../../test-vectors/playground-actions.json"
        ))
        .unwrap();
        for vector in vectors.as_array().unwrap() {
            assert_eq!(
                hex(&hash_json(&vector["params"]).unwrap()),
                vector["paramsHash"].as_str().unwrap(),
                "{}",
                vector["name"].as_str().unwrap()
            );
        }
    }

    fn playground(path: &Path) -> Playground {
        Playground::open(
            path,
            PlaygroundConfig {
                genesis_hash: [9; 32],
                compiler_url: "http://127.0.0.1:1".into(),
                bundle_dir: path.with_extension("bundles"),
            },
        )
        .unwrap()
    }

    struct MockChain {
        controller: Option<[u8; 32]>,
        create_calls: AtomicUsize,
        upgrade_calls: AtomicUsize,
        preimage_calls: AtomicUsize,
        work_calls: AtomicUsize,
        receipt: Mutex<Option<SystemReceiptV1>>,
        work_id: Mutex<Option<u64>>,
        work_terminal: Mutex<Option<Result<[u8; 32], ()>>>,
        service_info: Mutex<Option<Vec<u8>>>,
        service_storage: Mutex<Option<Vec<u8>>>,
        service_preimage: Mutex<Option<Vec<u8>>>,
        submitted_system_extrinsics: Mutex<Vec<Vec<u8>>>,
    }

    impl MockChain {
        fn new(controller: Option<[u8; 32]>) -> Self {
            Self {
                controller,
                create_calls: AtomicUsize::new(0),
                upgrade_calls: AtomicUsize::new(0),
                preimage_calls: AtomicUsize::new(0),
                work_calls: AtomicUsize::new(0),
                receipt: Mutex::new(None),
                work_id: Mutex::new(None),
                work_terminal: Mutex::new(None),
                service_info: Mutex::new(None),
                service_storage: Mutex::new(None),
                service_preimage: Mutex::new(None),
                submitted_system_extrinsics: Mutex::new(Vec::new()),
            }
        }

        fn submission(seed: u8) -> Submission {
            Submission {
                extrinsic_hash: [seed; 32],
                submitted_nonce: seed as u32,
                correlation: [seed.wrapping_add(1); 32],
            }
        }
    }

    #[async_trait]
    impl ChainGateway for MockChain {
        async fn finalized_context(&self) -> Result<FinalizedContext, ApiError> {
            Ok(FinalizedContext {
                block_hash: [8; 32],
                block_number: 8,
                state_root: [9; 32],
                slot: 8,
            })
        }

        async fn controller_at(
            &self,
            _block: [u8; 32],
            _service_id: u32,
        ) -> Result<Option<[u8; 32]>, ApiError> {
            Ok(self.controller)
        }

        async fn service_info_at(
            &self,
            _block: [u8; 32],
            _service_id: u32,
        ) -> Result<Option<Vec<u8>>, ApiError> {
            Ok(self.service_info.lock().unwrap().clone())
        }

        async fn service_storage_at(
            &self,
            _block: [u8; 32],
            _service_id: u32,
            _key: Vec<u8>,
        ) -> Result<Option<Vec<u8>>, ApiError> {
            Ok(self.service_storage.lock().unwrap().clone())
        }

        async fn service_preimage_at(
            &self,
            _block: [u8; 32],
            _service_id: u32,
            _code_hash: [u8; 32],
        ) -> Result<Option<Vec<u8>>, ApiError> {
            Ok(self.service_preimage.lock().unwrap().clone())
        }

        async fn prepare_create(
            &self,
            _controller: [u8; 32],
            _code_hash: [u8; 32],
            _code_len: u32,
            _min_item_gas: u64,
            _min_memo_gas: u64,
        ) -> Result<PreparedSystemOperation, ApiError> {
            Ok(PreparedSystemOperation {
                encoded_extrinsic: vec![1, 1, 1],
                submitted_nonce: 1,
                system_op_nonce: 11,
                correlation: [2; 32],
            })
        }

        async fn prepare_upgrade(
            &self,
            _controller: [u8; 32],
            _service_id: u32,
            _code_hash: [u8; 32],
            _code_len: u32,
            _min_item_gas: u64,
            _min_memo_gas: u64,
        ) -> Result<PreparedSystemOperation, ApiError> {
            Ok(PreparedSystemOperation {
                encoded_extrinsic: vec![3, 3, 3],
                submitted_nonce: 3,
                system_op_nonce: 13,
                correlation: [4; 32],
            })
        }

        async fn submit_prepared_system_op(
            &self,
            prepared: PreparedSystemOperation,
        ) -> Result<Submission, ApiError> {
            match prepared.encoded_extrinsic.first() {
                Some(1) => self.create_calls.fetch_add(1, Ordering::Relaxed),
                Some(3) => self.upgrade_calls.fetch_add(1, Ordering::Relaxed),
                _ => panic!("unexpected prepared mock extrinsic"),
            };
            self.submitted_system_extrinsics
                .lock()
                .unwrap()
                .push(prepared.encoded_extrinsic);
            Ok(Submission {
                extrinsic_hash: [prepared.submitted_nonce as u8; 32],
                submitted_nonce: prepared.submitted_nonce,
                correlation: prepared.correlation,
            })
        }

        async fn system_receipt(
            &self,
            _request_id: [u8; 32],
        ) -> Result<Option<SystemReceiptV1>, ApiError> {
            Ok(self.receipt.lock().unwrap().clone())
        }

        async fn submit_preimage(&self, _canonical: Vec<u8>) -> Result<Submission, ApiError> {
            self.preimage_calls.fetch_add(1, Ordering::Relaxed);
            Ok(Self::submission(5))
        }

        async fn submit_work(
            &self,
            _canonical: Vec<u8>,
            _bundle_ref: minijam_protocol::ContentRef,
            _package_hash: [u8; 32],
        ) -> Result<Submission, ApiError> {
            self.work_calls.fetch_add(1, Ordering::Relaxed);
            Ok(Self::submission(7))
        }

        async fn submit_allocation(
            &self,
            _allocation_id: u64,
            _target_service: u32,
            _amount: u128,
        ) -> Result<Submission, ApiError> {
            Ok(Self::submission(9))
        }

        async fn work_id_by_package_hash(
            &self,
            _package_hash: [u8; 32],
        ) -> Result<Option<u64>, ApiError> {
            Ok(*self.work_id.lock().unwrap())
        }

        async fn work_terminal(
            &self,
            _work_id: u64,
        ) -> Result<Option<Result<[u8; 32], ()>>, ApiError> {
            Ok(*self.work_terminal.lock().unwrap())
        }
    }

    #[tokio::test]
    async fn bundle_gateway_returns_verified_decodable_bundle() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));
        let built = minijam_work_package_builder::build_work_package(
            minijam_work_package_builder::BuildWorkInput {
                service_id: 7,
                service_code_hash: [1; 32],
                payload: b"payload".to_vec(),
                extrinsics: vec![b"extrinsic".to_vec()],
                anchor_hash: [2; 32],
                state_root: [3; 32],
                lookup_anchor_slot: 4,
            },
        )
        .unwrap();
        playground
            .save_bundle(&built.bundle_bytes, built.content_ref.content_hash)
            .unwrap();
        let cid = cid::Cid::read_bytes(built.content_ref.cid_v1.as_slice()).unwrap();
        let response = playground
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/ipfs/{cid}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            built.bundle_bytes.len().to_string()
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), built.bundle_bytes);
        let mut encoded = bytes.as_ref();
        let decoded = jambda_refine::MiniJamWorkBundleV1::decode(&mut encoded).unwrap();
        assert!(encoded.is_empty());
        assert!(decoded.package_hash_matches());
    }

    #[tokio::test]
    async fn public_api_allows_arbitrary_origins_and_preflight() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));

        for origin in ["http://127.0.0.1:5173", "https://example.com"] {
            let response = playground
                .clone()
                .router()
                .oneshot(
                    axum::http::Request::builder()
                        .method("GET")
                        .uri("/api/v1/config")
                        .header(header::ORIGIN, origin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
            assert!(!response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS));
        }

        let response = playground
            .clone()
            .router()
            .oneshot(
                axum::http::Request::builder()
                    .method("OPTIONS")
                    .uri("/api/v1/actions/prepare")
                    .header(header::ORIGIN, "https://example.com")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert!(response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
            .to_str()
            .unwrap()
            .contains("POST"));
        assert!(response.headers()[header::ACCESS_CONTROL_ALLOW_HEADERS]
            .to_str()
            .unwrap()
            .to_ascii_lowercase()
            .contains("content-type"));
    }

    #[tokio::test]
    async fn public_api_routes_return_cors_headers_for_developer_requests() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));
        let requests = [
            ("/api/v1/build", "{}"),
            ("/api/v1/actions/prepare", "{}"),
            ("/api/v1/services", "{}"),
            ("/api/v1/work", "{}"),
        ];

        for (path, body) in requests {
            let response = playground
                .clone()
                .router()
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri(path)
                        .header(header::ORIGIN, "https://community.example")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert!(
                response
                    .headers()
                    .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
                "missing CORS header for {path}"
            );
        }
    }

    #[tokio::test]
    async fn service_reads_are_bound_to_one_finalized_context() {
        let temp = tempfile::tempdir().unwrap();
        let controller = [21; 32];
        let chain = Arc::new(MockChain::new(Some(controller)));
        let mut info = jp_core_primitives::types::ServiceInfo::default();
        info.code_hash = jp_core_primitives::crypto::OpaqueHash([22; 32]);
        let info_value = StateValue::try_from(jam_codec::Encode::encode(&info)).unwrap();
        *chain.service_info.lock().unwrap() = Some(parity_scale_codec::Encode::encode(&info_value));
        let preimage_value = StateValue::try_from(b"service-code".to_vec()).unwrap();
        *chain.service_preimage.lock().unwrap() =
            Some(parity_scale_codec::Encode::encode(&preimage_value));
        let storage_value = StateValue::try_from(vec![1, 2, 3]).unwrap();
        *chain.service_storage.lock().unwrap() =
            Some(parity_scale_codec::Encode::encode(&storage_value));
        let playground =
            playground(&temp.path().join("playground.sqlite")).with_chain(chain.clone());

        let Json(service) = get_service(State(playground.clone()), AxumPath(7))
            .await
            .unwrap();
        let Json(storage) = get_service_storage(
            State(playground),
            AxumPath(7),
            Query(StorageQuery { key: "0x01".into() }),
        )
        .await
        .unwrap();

        assert_eq!(service.controller, hex(&controller));
        assert_eq!(service.code_hash, hex(&[22; 32]));
        assert_eq!(service.code_length, 12);
        assert!(service.preimage_ready);
        assert_eq!(service.finalized_block, hex(&[8; 32]));
        assert_eq!(storage.value, Some("0x010203".into()));
        assert_eq!(storage.finalized_block, service.finalized_block);
    }

    #[test]
    fn signed_action_is_bound_to_params_and_consumed_once() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));
        let pair = sr25519::Pair::from_seed(&[3; 32]);
        let params_hash = [7; 32];
        let prepared = playground
            .prepare(PrepareActionRequest {
                account: hex(&pair.public().0),
                action: "create_service".into(),
                params_hash: hex(&params_hash),
                expiry: now() + 60,
            })
            .unwrap();
        let authorization = sign_prepared(&pair, prepared);

        assert_eq!(
            playground
                .consume_action(&authorization, "create_service", params_hash)
                .unwrap(),
            pair.public().0
        );
        assert!(matches!(
            playground.consume_action(&authorization, "create_service", params_hash),
            Err(ApiError::Replayed)
        ));
    }

    #[test]
    fn signed_action_rejects_substituted_params_without_consuming_it() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));
        let pair = sr25519::Pair::from_seed(&[4; 32]);
        let prepared = playground
            .prepare(PrepareActionRequest {
                account: hex(&pair.public().0),
                action: "work".into(),
                params_hash: hex(&[1; 32]),
                expiry: now() + 60,
            })
            .unwrap();
        let authorization = sign_prepared(&pair, prepared);

        assert!(matches!(
            playground.consume_action(&authorization, "work", [2; 32]),
            Err(ApiError::ParamsMismatch)
        ));
        assert!(playground
            .consume_action(&authorization, "work", [1; 32])
            .is_ok());
    }

    #[test]
    fn signed_action_rejects_raw_payload_signature() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));
        let pair = sr25519::Pair::from_seed(&[14; 32]);
        let params_hash = [15; 32];
        let prepared = playground
            .prepare(PrepareActionRequest {
                account: hex(&pair.public().0),
                action: "work".into(),
                params_hash: hex(&params_hash),
                expiry: now() + 60,
            })
            .unwrap();
        let signing_hash = decode_array::<32>(&prepared.signing_payload).unwrap();
        let authorization = ActionAuthorization {
            action_id: prepared.action_id,
            signature: hex(&pair.sign(&signing_hash).0),
        };

        assert!(matches!(
            playground.consume_action(&authorization, "work", params_hash),
            Err(ApiError::InvalidSignature)
        ));
    }

    #[test]
    fn signed_action_rejects_signature_from_another_account() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));
        let account_pair = sr25519::Pair::from_seed(&[16; 32]);
        let wrong_pair = sr25519::Pair::from_seed(&[17; 32]);
        let params_hash = [18; 32];
        let prepared = playground
            .prepare(PrepareActionRequest {
                account: hex(&account_pair.public().0),
                action: "upgrade_service".into(),
                params_hash: hex(&params_hash),
                expiry: now() + 60,
            })
            .unwrap();
        let authorization = sign_prepared(&wrong_pair, prepared);

        assert!(matches!(
            playground.consume_action(&authorization, "upgrade_service", params_hash),
            Err(ApiError::InvalidSignature)
        ));
    }

    #[test]
    fn signed_action_rejects_substituted_action_genesis_and_expiry() {
        for field in ["action", "genesis_hash", "expiry"] {
            let temp = tempfile::tempdir().unwrap();
            let playground = playground(&temp.path().join("playground.sqlite"));
            let pair = sr25519::Pair::from_seed(&[19; 32]);
            let params_hash = [20; 32];
            let prepared = playground
                .prepare(PrepareActionRequest {
                    account: hex(&pair.public().0),
                    action: "work".into(),
                    params_hash: hex(&params_hash),
                    expiry: now() + 60,
                })
                .unwrap();
            let action_id = decode_array::<32>(&prepared.action_id).unwrap();
            let authorization = sign_prepared(&pair, prepared);
            let expected_action = if field == "action" {
                playground
                    .db
                    .lock()
                    .unwrap()
                    .execute(
                        "UPDATE signed_actions SET action = 'upgrade_service' WHERE action_id = ?1",
                        params![action_id.as_slice()],
                    )
                    .unwrap();
                "upgrade_service"
            } else {
                if field == "genesis_hash" {
                    playground
                        .db
                        .lock()
                        .unwrap()
                        .execute(
                            "UPDATE signed_actions SET genesis_hash = ?2 WHERE action_id = ?1",
                            params![action_id.as_slice(), [21_u8; 32].as_slice()],
                        )
                        .unwrap();
                } else {
                    playground
                        .db
                        .lock()
                        .unwrap()
                        .execute(
                            "UPDATE signed_actions SET expiry = ?2 WHERE action_id = ?1",
                            params![action_id.as_slice(), (now() + 120) as i64],
                        )
                        .unwrap();
                }
                "work"
            };

            assert!(
                matches!(
                    playground.consume_action(&authorization, expected_action, params_hash),
                    Err(ApiError::InvalidSignature)
                ),
                "{field} substitution must invalidate the signature"
            );
        }
    }

    #[test]
    fn operations_survive_restart_and_only_non_terminal_rows_recover() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("playground.sqlite");
        let operation_id = {
            let playground = playground(&path);
            playground
                .insert_operation(
                    OperationKind::Create,
                    [1; 32],
                    [2; 32],
                    &serde_json::json!({"codeHash": hex(&[3; 32])}),
                )
                .unwrap()
                .operation_id
        };
        let restarted = playground(&path);
        assert_eq!(
            restarted.recoverable_operations().unwrap()[0].operation_id,
            operation_id
        );
        restarted
            .update_operation(
                &operation_id,
                OperationStatus::Succeeded,
                Some([4; 32]),
                Some([5; 32]),
                Some(7),
                Some(&serde_json::json!({"serviceId": 10})),
                None,
            )
            .unwrap();
        assert!(restarted.recoverable_operations().unwrap().is_empty());
        assert_eq!(
            restarted.operation(&operation_id).unwrap().unwrap().status,
            OperationStatus::Succeeded
        );
    }

    #[tokio::test]
    async fn create_recovery_does_not_submit_create_twice() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("playground.sqlite");
        let chain = Arc::new(MockChain::new(None));
        let blob = b"service".to_vec();
        let request = serde_json::json!({
            "blobBase64": STANDARD.encode(&blob),
            "codeHash": hex(&minijam_protocol::blake2_256(&blob)),
            "minItemGas": 1,
            "minMemoGas": 2,
        });
        let first = playground(&path).with_chain(chain.clone());
        let operation = first
            .insert_operation(OperationKind::Create, [3; 32], [4; 32], &request)
            .unwrap();
        let operation_id = operation.operation_id.clone();
        first.process_service_operation(operation).await.unwrap();
        assert_eq!(chain.create_calls.load(Ordering::Relaxed), 1);

        let restarted = playground(&path).with_chain(chain.clone());
        let waiting = restarted.recoverable_operations().unwrap().remove(0);
        restarted
            .process_service_operation(waiting.clone())
            .await
            .unwrap();
        assert_eq!(chain.create_calls.load(Ordering::Relaxed), 1);

        *chain.receipt.lock().unwrap() = Some(SystemReceiptV1::ServiceCreated {
            service_id: 17,
            controller: [3; 32],
        });
        restarted.process_service_operation(waiting).await.unwrap();
        assert_eq!(chain.preimage_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            restarted.operation(&operation_id).unwrap().unwrap().status,
            OperationStatus::WaitingPreimage
        );

        let waiting = restarted.operation(&operation_id).unwrap().unwrap();
        restarted.process_service_operation(waiting).await.unwrap();
        assert_eq!(chain.preimage_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            restarted.operation(&operation_id).unwrap().unwrap().status,
            OperationStatus::WaitingPreimage
        );

        let value = StateValue::try_from(blob).unwrap();
        *chain.service_preimage.lock().unwrap() = Some(Encode::encode(&value));
        let waiting = restarted.operation(&operation_id).unwrap().unwrap();
        restarted.process_service_operation(waiting).await.unwrap();
        assert_eq!(
            restarted.operation(&operation_id).unwrap().unwrap().status,
            OperationStatus::Succeeded
        );
    }

    #[tokio::test]
    async fn prepared_system_operation_is_persisted_before_submission_and_reused() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("playground.sqlite");
        let chain = Arc::new(MockChain::new(None));
        let blob = b"write-ahead service".to_vec();
        let code_hash = minijam_protocol::blake2_256(&blob);
        let request = serde_json::json!({
            "blobBase64": STANDARD.encode(&blob),
            "codeHash": hex(&code_hash),
            "minItemGas": 1,
            "minMemoGas": 2,
        });
        let first = playground(&path).with_chain(chain.clone());
        let operation = first
            .insert_operation(OperationKind::Create, [31; 32], [32; 32], &request)
            .unwrap();
        let prepared = chain
            .prepare_create([31; 32], code_hash, blob.len() as u32, 1, 2)
            .await
            .unwrap();
        first
            .persist_prepared_system_op(&operation.operation_id, &prepared)
            .unwrap();

        let persisted = first.operation(&operation.operation_id).unwrap().unwrap();
        assert_eq!(persisted.status, OperationStatus::Prepared);
        assert_eq!(
            persisted.encoded_extrinsic.as_deref(),
            Some(prepared.encoded_extrinsic.as_slice())
        );
        assert_eq!(persisted.submitted_nonce, Some(prepared.submitted_nonce));
        assert_eq!(persisted.system_op_nonce, Some(prepared.system_op_nonce));
        drop(first);

        let restarted = playground(&path).with_chain(chain.clone());
        let recovered = restarted.recoverable_operations().unwrap().remove(0);
        restarted
            .process_service_operation(recovered)
            .await
            .unwrap();

        let submissions = chain.submitted_system_extrinsics.lock().unwrap();
        assert_eq!(submissions.as_slice(), &[prepared.encoded_extrinsic]);
        assert_eq!(
            restarted
                .operation(&operation.operation_id)
                .unwrap()
                .unwrap()
                .status,
            OperationStatus::WaitingReceipt
        );
    }

    #[tokio::test]
    async fn crash_after_system_submission_replays_the_identical_extrinsic() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("playground.sqlite");
        let chain = Arc::new(MockChain::new(None));
        let blob = b"submitted before crash".to_vec();
        let code_hash = minijam_protocol::blake2_256(&blob);
        let request = serde_json::json!({
            "blobBase64": STANDARD.encode(&blob),
            "codeHash": hex(&code_hash),
            "minItemGas": 3,
            "minMemoGas": 4,
        });
        let first = playground(&path).with_chain(chain.clone());
        let operation = first
            .insert_operation(OperationKind::Create, [33; 32], [34; 32], &request)
            .unwrap();
        let prepared = chain
            .prepare_create([33; 32], code_hash, blob.len() as u32, 3, 4)
            .await
            .unwrap();
        first
            .persist_prepared_system_op(&operation.operation_id, &prepared)
            .unwrap();
        chain
            .submit_prepared_system_op(prepared.clone())
            .await
            .unwrap();
        drop(first);

        let restarted = playground(&path).with_chain(chain.clone());
        let recovered = restarted.recoverable_operations().unwrap().remove(0);
        restarted
            .process_service_operation(recovered)
            .await
            .unwrap();

        let submissions = chain.submitted_system_extrinsics.lock().unwrap();
        assert_eq!(submissions.len(), 2);
        assert_eq!(submissions[0], submissions[1]);
        assert_eq!(submissions[0], prepared.encoded_extrinsic);
    }

    #[tokio::test]
    async fn upgrade_reaches_receipt_and_submits_new_preimage() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("playground.sqlite");
        let controller = [12; 32];
        let chain = Arc::new(MockChain::new(Some(controller)));
        let instance = playground(&path).with_chain(chain.clone());
        let blob = b"new service code".to_vec();
        let request = serde_json::json!({
            "serviceId": 27,
            "blobBase64": STANDARD.encode(&blob),
            "codeHash": hex(&minijam_protocol::blake2_256(&blob)),
            "minItemGas": 30,
            "minMemoGas": 31,
        });
        let operation = instance
            .insert_operation(OperationKind::Upgrade, controller, [13; 32], &request)
            .unwrap();
        let operation_id = operation.operation_id.clone();

        instance.process_service_operation(operation).await.unwrap();
        assert_eq!(chain.upgrade_calls.load(Ordering::Relaxed), 1);
        *chain.receipt.lock().unwrap() = Some(SystemReceiptV1::ServiceUpgraded {
            service_id: 27,
            controller,
            code_hash: minijam_protocol::blake2_256(&blob),
        });
        let waiting = instance.operation(&operation_id).unwrap().unwrap();
        instance.process_service_operation(waiting).await.unwrap();

        assert_eq!(chain.preimage_calls.load(Ordering::Relaxed), 1);
        let waiting = instance.operation(&operation_id).unwrap().unwrap();
        assert_eq!(waiting.status, OperationStatus::WaitingPreimage);
        let value = StateValue::try_from(blob).unwrap();
        *chain.service_preimage.lock().unwrap() = Some(Encode::encode(&value));
        instance.process_service_operation(waiting).await.unwrap();
        let completed = instance.operation(&operation_id).unwrap().unwrap();
        assert_eq!(completed.status, OperationStatus::Succeeded);
        assert_eq!(completed.result.unwrap()["serviceId"], 27);
    }

    #[tokio::test]
    async fn work_recovery_tracks_package_hash_without_duplicate_submission() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("playground.sqlite");
        let chain = Arc::new(MockChain::new(Some([14; 32])));
        let instance = playground(&path).with_chain(chain.clone());
        let built = minijam_work_package_builder::build_work_package(
            minijam_work_package_builder::BuildWorkInput {
                service_id: 28,
                service_code_hash: [15; 32],
                payload: b"work".to_vec(),
                extrinsics: Vec::new(),
                anchor_hash: [16; 32],
                state_root: [17; 32],
                lookup_anchor_slot: 18,
            },
        )
        .unwrap();
        instance
            .save_bundle(&built.bundle_bytes, built.content_ref.content_hash)
            .unwrap();
        let cid = cid::Cid::read_bytes(built.content_ref.cid_v1.as_slice()).unwrap();
        let request = serde_json::json!({
            "packageHash": hex(&built.package_hash),
            "canonicalWorkPackage": STANDARD.encode(&built.canonical_work_package),
            "bundleCid": cid.to_string(),
            "bundleHash": hex(&built.content_ref.content_hash),
            "bundleSize": built.content_ref.size,
        });
        let operation = instance
            .insert_operation(OperationKind::Work, [14; 32], [19; 32], &request)
            .unwrap();
        let operation_id = operation.operation_id.clone();
        instance.process_work_operation(operation).await.unwrap();
        assert_eq!(chain.work_calls.load(Ordering::Relaxed), 1);

        *chain.work_id.lock().unwrap() = Some(29);
        let restarted = playground(&path).with_chain(chain.clone());
        let tracking = restarted.operation(&operation_id).unwrap().unwrap();
        restarted.process_work_operation(tracking).await.unwrap();
        assert_eq!(chain.work_calls.load(Ordering::Relaxed), 1);
        *chain.work_terminal.lock().unwrap() = Some(Ok([20; 32]));
        let tracking = restarted.operation(&operation_id).unwrap().unwrap();
        restarted.process_work_operation(tracking).await.unwrap();

        let completed = restarted.operation(&operation_id).unwrap().unwrap();
        assert_eq!(completed.status, OperationStatus::Succeeded);
        assert_eq!(completed.result.unwrap()["workId"], 29);
        assert_eq!(chain.work_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn non_controller_upgrade_is_forbidden_before_chain_submission() {
        let temp = tempfile::tempdir().unwrap();
        let chain = Arc::new(MockChain::new(Some([99; 32])));
        let playground =
            playground(&temp.path().join("playground.sqlite")).with_chain(chain.clone());
        let pair = sr25519::Pair::from_seed(&[6; 32]);
        let blob = b"upgrade".to_vec();
        let params = serde_json::json!({
            "serviceId": 9,
            "blobBase64": STANDARD.encode(&blob),
            "codeHash": hex(&minijam_protocol::blake2_256(&blob)),
            "minItemGas": 10,
            "minMemoGas": 11,
        });
        let prepared = playground
            .prepare(PrepareActionRequest {
                account: hex(&pair.public().0),
                action: "upgrade_service".into(),
                params_hash: hex(&hash_json(&params).unwrap()),
                expiry: now() + 60,
            })
            .unwrap();
        let request = UpgradeServiceRequest {
            authorization: sign_prepared(&pair, prepared),
            service_id: 9,
            blob_base64: params["blobBase64"].as_str().unwrap().into(),
            code_hash: params["codeHash"].as_str().unwrap().into(),
            min_item_gas: 10,
            min_memo_gas: 11,
        };

        assert!(matches!(
            upgrade_service(State(playground), axum::extract::Path(9), Json(request)).await,
            Err(ApiError::Forbidden)
        ));
        assert_eq!(chain.upgrade_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn non_controller_work_is_accepted_by_experience_ingress() {
        let temp = tempfile::tempdir().unwrap();
        let chain = Arc::new(MockChain::new(Some([99; 32])));
        let playground =
            playground(&temp.path().join("playground.sqlite")).with_chain(chain.clone());
        let pair = sr25519::Pair::from_seed(&[6; 32]);
        let params = serde_json::json!({
            "serviceId": 9,
            "serviceCodeHash": hex(&[7; 32]),
            "payloadBase64": STANDARD.encode(b"work"),
            "extrinsicsBase64": Vec::<String>::new(),
        });
        let prepared = playground
            .prepare(PrepareActionRequest {
                account: hex(&pair.public().0),
                action: "work".into(),
                params_hash: hex(&hash_json(&params).unwrap()),
                expiry: now() + 60,
            })
            .unwrap();
        let request = SubmitWorkRequest {
            authorization: sign_prepared(&pair, prepared),
            service_id: 9,
            service_code_hash: params["serviceCodeHash"].as_str().unwrap().into(),
            payload_base64: params["payloadBase64"].as_str().unwrap().into(),
            extrinsics_base64: Vec::new(),
        };

        let result = submit_work(State(playground), Json(request)).await;
        assert!(
            result.is_ok(),
            "non-controller Work must enter the API queue"
        );
        assert_eq!(chain.work_calls.load(Ordering::Relaxed), 0);
    }
}
