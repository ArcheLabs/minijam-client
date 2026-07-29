// SPDX-License-Identifier: Apache-2.0

#![feature(generic_const_exprs)]
#![allow(incomplete_features)]
#![recursion_limit = "4096"]

use core::time::Duration;
use std::{
    collections::BTreeMap,
    fmt,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
};

use jam_codec::{Decode as JamDecode, Encode as JamEncode};
use jambda_state_backend::StateBackend;
use jp_core_primitives::{
    error::DataBaseError,
    spec::TinySpec,
    state::{column, ColumnFamily, StateKey, StoreChange, StoreOp},
    traits::DataBase,
};
use jp_vm_interp::InterpBackend;
use minijam_protocol::{
    stage0, BulletinEvidence, ContentRef, Hash, OpposeReason, ReportEnvelopeV1, Verdict, WorkId,
    WorkerId, WorkerVoteV1, PROTOCOL_VERSION_V1,
};
use minijam_worker_engine::{
    fetch::{fetch_verified_content, ContentFetcher, FetchError, HttpBytesClient},
    verify_work_bundle, MiniJamWorkBundleDecoder, WorkBundleDecoder, WorkBundleVerificationError,
};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sp_core::{sr25519, Pair};
use std::sync::Mutex;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerConfig {
    pub rpc_url: String,
    pub key: Option<String>,
    pub worker_id: Option<WorkerId>,
    pub chain_id: Hash,
    pub expected_genesis_hash: Option<Hash>,
    pub core_index: u16,
    pub submit_candidates: bool,
    pub submit_support_votes: bool,
    pub poll_interval: Duration,
    pub recovery_db_path: Option<PathBuf>,
    pub metrics_bind: Option<String>,
    pub health_bind: Option<String>,
    pub ipfs_gateway: String,
    pub request_timeout: Duration,
    pub max_bundle_bytes: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://127.0.0.1:9944".into(),
            key: None,
            worker_id: None,
            chain_id: [77; 32],
            expected_genesis_hash: None,
            core_index: 0,
            submit_candidates: false,
            submit_support_votes: false,
            poll_interval: Duration::from_millis(1_000),
            recovery_db_path: None,
            metrics_bind: None,
            health_bind: Some("127.0.0.1:8082".into()),
            ipfs_gateway: "http://127.0.0.1:8080".into(),
            request_timeout: Duration::from_secs(30),
            max_bundle_bytes: 16_777_216,
        }
    }
}

impl WorkerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.rpc_url.trim().is_empty() {
            return Err(ConfigError::MissingRpcUrl);
        }
        if self.ipfs_gateway.trim().is_empty() {
            return Err(ConfigError::MissingIpfsGateway);
        }
        if self.poll_interval.is_zero() {
            return Err(ConfigError::ZeroPollInterval);
        }
        if self.request_timeout.is_zero() {
            return Err(ConfigError::ZeroRequestTimeout);
        }
        if self.max_bundle_bytes == 0 {
            return Err(ConfigError::ZeroMaxBundleBytes);
        }
        Ok(())
    }

    pub fn from_toml_str(input: &str) -> Result<Self, ConfigFileError> {
        let file = WorkerConfigFile::parse(input)?;
        let mut config = Self::default();
        file.apply_to(&mut config)?;
        Ok(config)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    MissingRpcUrl,
    MissingIpfsGateway,
    ZeroPollInterval,
    ZeroRequestTimeout,
    ZeroMaxBundleBytes,
}

#[derive(Debug)]
pub enum ConfigFileError {
    Toml(toml::de::Error),
    InvalidGenesisHash,
}

impl core::fmt::Display for ConfigFileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Toml(error) => write!(f, "{error}"),
            Self::InvalidGenesisHash => write!(f, "node.genesis_hash must be 32-byte hex"),
        }
    }
}

impl std::error::Error for ConfigFileError {}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct WorkerConfigFile {
    node: Option<NodeConfigFile>,
    worker: Option<WorkerSectionConfigFile>,
    metrics: Option<MetricsConfigFile>,
    health: Option<HealthConfigFile>,
    content: Option<ContentConfigFile>,
}

impl WorkerConfigFile {
    fn parse(input: &str) -> Result<Self, ConfigFileError> {
        toml::from_str(input).map_err(ConfigFileError::Toml)
    }

    fn apply_to(self, config: &mut WorkerConfig) -> Result<(), ConfigFileError> {
        if let Some(node) = self.node {
            if let Some(rpc_url) = node.rpc_url {
                config.rpc_url = rpc_url;
            }
            if let Some(genesis_hash) = node.genesis_hash {
                let bytes =
                    decode_hex(&genesis_hash).map_err(|_| ConfigFileError::InvalidGenesisHash)?;
                config.expected_genesis_hash = Some(
                    bytes
                        .try_into()
                        .map_err(|_| ConfigFileError::InvalidGenesisHash)?,
                );
            }
        }
        if let Some(worker) = self.worker {
            if let Some(key) = worker.key {
                config.key = Some(key);
            }
            if let Some(worker_id) = worker.worker_id {
                config.worker_id = Some(worker_id);
            }
            if let Some(core_index) = worker.core_index {
                config.core_index = core_index;
            }
            if let Some(submit_candidates) = worker.submit_candidates {
                config.submit_candidates = submit_candidates;
            }
            if let Some(submit_support_votes) = worker.submit_support_votes {
                config.submit_support_votes = submit_support_votes;
            }
            if let Some(poll_interval_ms) = worker.poll_interval_ms {
                config.poll_interval = Duration::from_millis(poll_interval_ms);
            }
            if let Some(recovery_db_path) = worker.recovery_db_path {
                config.recovery_db_path = Some(recovery_db_path);
            }
        }
        if let Some(metrics) = self.metrics {
            if let Some(bind) = metrics.bind {
                config.metrics_bind = Some(bind);
            }
        }
        if let Some(health) = self.health {
            if let Some(bind) = health.bind {
                config.health_bind = Some(bind);
            }
        }
        if let Some(content) = self.content {
            if let Some(ipfs_gateway) = content.ipfs_gateway {
                config.ipfs_gateway = ipfs_gateway;
            }
            if let Some(request_timeout_secs) = content.request_timeout_secs {
                config.request_timeout = Duration::from_secs(request_timeout_secs);
            }
            if let Some(max_bundle_bytes) = content.max_bundle_bytes {
                config.max_bundle_bytes = max_bundle_bytes;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct NodeConfigFile {
    rpc_url: Option<String>,
    genesis_hash: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct WorkerSectionConfigFile {
    key: Option<String>,
    worker_id: Option<WorkerId>,
    core_index: Option<u16>,
    submit_candidates: Option<bool>,
    submit_support_votes: Option<bool>,
    poll_interval_ms: Option<u64>,
    recovery_db_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct MetricsConfigFile {
    bind: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct HealthConfigFile {
    bind: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct ContentConfigFile {
    ipfs_gateway: Option<String>,
    request_timeout_secs: Option<u64>,
    max_bundle_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkTask {
    pub work_id: WorkId,
    pub round: u8,
    pub assignment_epoch: u32,
    pub assigned_workers: Vec<WorkerId>,
    pub candidate_producer: WorkerId,
    pub package_hash: Hash,
    pub canonical_work_package: Vec<u8>,
    pub bundle_ref: ContentRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoteTask {
    pub work_id: WorkId,
    pub round: u8,
    pub assignment_epoch: u32,
    pub candidate_report_hash: Hash,
    pub candidate_report: Vec<u8>,
    pub deadline: u32,
    pub assigned_workers: Vec<WorkerId>,
    pub submitted_votes: Vec<WorkerId>,
    pub package_hash: Hash,
    pub canonical_work_package: Vec<u8>,
    pub bundle_ref: ContentRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedVoteSubmission {
    pub worker_id: WorkerId,
    pub vote: WorkerVoteV1,
    pub signature: [u8; 64],
    pub extrinsic_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedCandidateSubmission {
    pub envelope: ReportEnvelopeV1,
    pub exported_segment_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSignedCandidateSubmission {
    pub envelope: ReportEnvelopeV1,
    pub nonce: minijam_runtime::Nonce,
    pub extrinsic_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerTaskStatus {
    BundleReady { bundle_len: usize },
    BundleRejected { reason: WorkerError },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerError {
    Chain(String),
    Fetch(FetchError),
    Bundle(WorkBundleVerificationError),
    Refine(String),
    Signing(String),
}

#[derive(Debug, Default)]
pub struct WorkerMetrics {
    polls_total: AtomicU64,
    tasks_processed_total: AtomicU64,
    vote_tasks_seen_total: AtomicU64,
    bundle_ready_total: AtomicU64,
    bundle_rejected_total: AtomicU64,
}

impl WorkerMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_poll(&self, processed: usize) {
        self.polls_total.fetch_add(1, Ordering::Relaxed);
        self.tasks_processed_total
            .fetch_add(processed as u64, Ordering::Relaxed);
    }

    pub fn record_bundle_ready(&self) {
        self.bundle_ready_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_vote_tasks_seen(&self, count: usize) {
        self.vote_tasks_seen_total
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn record_bundle_rejected(&self) {
        self.bundle_rejected_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn render_prometheus(&self) -> String {
        format!(
            concat!(
                "# HELP minijam_worker_polls_total Worker poll iterations completed.\n",
                "# TYPE minijam_worker_polls_total counter\n",
                "minijam_worker_polls_total {}\n",
                "# HELP minijam_worker_tasks_processed_total Worker tasks processed by polling.\n",
                "# TYPE minijam_worker_tasks_processed_total counter\n",
                "minijam_worker_tasks_processed_total {}\n",
                "# HELP minijam_worker_vote_tasks_seen_total Open vote tasks observed by polling.\n",
                "# TYPE minijam_worker_vote_tasks_seen_total counter\n",
                "minijam_worker_vote_tasks_seen_total {}\n",
                "# HELP minijam_worker_bundle_ready_total Bundles fetched and verified successfully.\n",
                "# TYPE minijam_worker_bundle_ready_total counter\n",
                "minijam_worker_bundle_ready_total {}\n",
                "# HELP minijam_worker_bundle_rejected_total Bundles rejected during fetch or verification.\n",
                "# TYPE minijam_worker_bundle_rejected_total counter\n",
                "minijam_worker_bundle_rejected_total {}\n"
            ),
            self.polls_total.load(Ordering::Relaxed),
            self.tasks_processed_total.load(Ordering::Relaxed),
            self.vote_tasks_seen_total.load(Ordering::Relaxed),
            self.bundle_ready_total.load(Ordering::Relaxed),
            self.bundle_rejected_total.load(Ordering::Relaxed)
        )
    }
}

pub fn spawn_prometheus_metrics_server(
    bind: &str,
    metrics: Arc<WorkerMetrics>,
) -> std::io::Result<thread::JoinHandle<()>> {
    let listener = TcpListener::bind(bind)?;
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = metrics.render_prometheus();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain; version=0.0.4\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    Ok(handle)
}

#[derive(Default)]
pub struct WorkerHealth {
    ready: AtomicBool,
}

impl WorkerHealth {
    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Release);
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

pub fn spawn_worker_health_server(
    bind: &str,
    health: Arc<WorkerHealth>,
) -> std::io::Result<thread::JoinHandle<()>> {
    let listener = TcpListener::bind(bind)?;
    Ok(thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let mut request = [0u8; 1024];
            let count = stream.read(&mut request).unwrap_or_default();
            let request = String::from_utf8_lossy(&request[..count]);
            let (status, body) = if request.starts_with("GET /health/live ") {
                ("200 OK", "live\n")
            } else if request.starts_with("GET /health/ready ") && health.is_ready() {
                ("200 OK", "ready\n")
            } else if request.starts_with("GET /health/ready ") {
                ("503 Service Unavailable", "not ready\n")
            } else {
                ("404 Not Found", "not found\n")
            };
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    }))
}

pub fn check_bundle_gateway_ready(gateway: &str) -> Result<(), WorkerError> {
    let base = gateway.trim_end_matches('/');
    let base = base.strip_suffix("/ipfs").unwrap_or(base);
    http_get_bytes(&format!("{base}/health/ready"))
        .map(|_| ())
        .map_err(|error| WorkerError::Fetch(FetchError::Transport(error.to_string())))
}

#[derive(Debug)]
pub struct WsWorkerChainSource {
    client: jsonrpsee::ws_client::WsClient,
}

impl WsWorkerChainSource {
    pub async fn connect(url: &str) -> Result<Self, WorkerError> {
        let client = jsonrpsee::ws_client::WsClientBuilder::default()
            .build(url)
            .await
            .map_err(|error| WorkerError::Chain(error.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl WorkerChainSource for WsWorkerChainSource {
    async fn pending_work_tasks(&self) -> Result<Vec<WorkTask>, WorkerError> {
        use jsonrpsee::core::client::ClientT;

        let encoded: String = self
            .client
            .request("minijam_getPendingWorkTasks", jsonrpsee::rpc_params![])
            .await
            .map_err(|error| WorkerError::Chain(error.to_string()))?;
        decode_pending_work_tasks_response(&encoded)
    }

    async fn open_vote_tasks(&self) -> Result<Vec<VoteTask>, WorkerError> {
        use jsonrpsee::core::client::ClientT;

        let encoded: String = self
            .client
            .request("minijam_getOpenVoteTasks", jsonrpsee::rpc_params![])
            .await
            .map_err(|error| WorkerError::Chain(error.to_string()))?;
        decode_open_vote_tasks_response(&encoded)
    }
}

#[derive(Clone, Debug)]
pub struct BlockingHttpWorkerChainSource {
    rpc_url: String,
}

impl BlockingHttpWorkerChainSource {
    pub fn new(rpc_url: impl Into<String>) -> Result<Self, WorkerError> {
        let rpc_url = rpc_url.into();
        HttpEndpoint::parse(&rpc_url).map_err(|error| WorkerError::Chain(error.to_string()))?;
        Ok(Self { rpc_url })
    }

    pub async fn submit_raw_extrinsic(&self, extrinsic_hex: &str) -> Result<Hash, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "author_submitExtrinsic",
                "params": [extrinsic_hex],
            })
            .to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        let encoded = json_rpc_string_result(&response)?;
        let bytes = decode_hex(&encoded)?;
        let hash: [u8; 32] = bytes.try_into().map_err(|_| {
            WorkerError::Chain("author_submitExtrinsic returned a non-32-byte hash".into())
        })?;
        Ok(hash)
    }
}

pub trait ProtocolStateSource {
    fn validate_finalized_context(
        &self,
        _context: stage0::RefineContextV1,
    ) -> Result<(), WorkerError> {
        Ok(())
    }

    fn protocol_state_value_at(
        &self,
        block_hash: [u8; 32],
        key: [u8; 31],
    ) -> Result<Option<Vec<u8>>, WorkerError>;
}

pub trait WorkerSignedTxContext {
    fn account_nonce(&self, account: [u8; 32]) -> Result<minijam_runtime::Nonce, WorkerError>;

    fn genesis_hash(&self) -> Result<Hash, WorkerError>;
}

impl ProtocolStateSource for BlockingHttpWorkerChainSource {
    fn validate_finalized_context(
        &self,
        context: stage0::RefineContextV1,
    ) -> Result<(), WorkerError> {
        let finalized = self.rpc_string("chain_getFinalizedHead", json!([]))?;
        let finalized_context = self.header_context(&finalized)?;
        let anchor = hex_encode(&context.lookup_anchor);
        let anchor_context = self.header_context(&anchor)?;
        if anchor_context.block_number > finalized_context.block_number {
            return Err(WorkerError::Refine(
                "work package lookup anchor is not finalized".into(),
            ));
        }
        let canonical =
            self.rpc_string("chain_getBlockHash", json!([anchor_context.block_number]))?;
        if canonical != anchor {
            return Err(WorkerError::Refine(
                "work package lookup anchor is not on the finalized chain".into(),
            ));
        }
        stage0::validate_refine_context(context, anchor_context).map_err(|error| {
            WorkerError::Refine(format!(
                "invalid finalized Stage 0 refine context: {error:?}"
            ))
        })
    }

    fn protocol_state_value_at(
        &self,
        block_hash: [u8; 32],
        key: [u8; 31],
    ) -> Result<Option<Vec<u8>>, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "minijam_getProtocolStateAt",
                "params": [hex_encode(&block_hash), hex_encode(&key)],
            })
            .to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        let Some(encoded) = json_rpc_optional_string_result(&response)? else {
            return Ok(None);
        };
        let bytes = decode_hex(&encoded)?;
        let value =
            minijam_protocol::StateValue::decode(&mut bytes.as_slice()).map_err(|error| {
                WorkerError::Chain(format!("invalid protocol state value: {error}"))
            })?;
        Ok(Some(value.into_inner()))
    }
}

impl BlockingHttpWorkerChainSource {
    pub fn registered_session_key(
        &self,
        worker_id: WorkerId,
    ) -> Result<Option<[u8; 32]>, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "minijam_getWorker",
                "params": [worker_id],
            })
            .to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        let Some(encoded) = json_rpc_optional_string_result(&response)? else {
            return Ok(None);
        };
        let bytes = decode_hex(&encoded)?;
        let worker = pallet_minijam_workers::WorkerRecord::<minijam_runtime::Runtime>::decode(
            &mut bytes.as_slice(),
        )
        .map_err(|error| WorkerError::Chain(format!("invalid Worker record: {error}")))?;
        Ok(Some(worker.session_key))
    }

    fn rpc_string(&self, method: &str, params: Value) -> Result<String, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        json_rpc_string_result(&response)
    }

    fn header_context(&self, block_hash: &str) -> Result<stage0::FinalizedContextV1, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "chain_getHeader",
                "params": [block_hash],
            })
            .to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        let response: Value = serde_json::from_str(&response)
            .map_err(|error| WorkerError::Chain(format!("invalid JSON-RPC response: {error}")))?;
        let result = response
            .get("result")
            .and_then(Value::as_object)
            .ok_or_else(|| WorkerError::Chain("chain_getHeader returned no header".into()))?;
        let number = result
            .get("number")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkerError::Chain("chain_getHeader returned no number".into()))?;
        let block_number = u32::from_str_radix(number.strip_prefix("0x").unwrap_or(number), 16)
            .map_err(|error| WorkerError::Chain(format!("invalid header number: {error}")))?;
        let state_root = result
            .get("stateRoot")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkerError::Chain("chain_getHeader returned no state root".into()))?;
        let state_root: [u8; 32] = decode_hex(state_root)?
            .try_into()
            .map_err(|_| WorkerError::Chain("header state root is not 32 bytes".into()))?;
        let block_hash: [u8; 32] = decode_hex(block_hash)?
            .try_into()
            .map_err(|_| WorkerError::Chain("header block hash is not 32 bytes".into()))?;
        Ok(stage0::FinalizedContextV1 {
            block_hash,
            block_number,
            state_root,
            slot: block_number,
        })
    }
}

impl WorkerSignedTxContext for BlockingHttpWorkerChainSource {
    fn account_nonce(&self, account: [u8; 32]) -> Result<minijam_runtime::Nonce, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "system_accountNextIndex",
                "params": [minijam_chain_client::account_id_rpc_param(account)],
            })
            .to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        json_rpc_u32_result(&response)
    }

    fn genesis_hash(&self) -> Result<Hash, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "chain_getBlockHash",
                "params": [0],
            })
            .to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        let encoded = json_rpc_string_result(&response)?;
        let bytes = decode_hex(&encoded)?;
        bytes.try_into().map_err(|_| {
            WorkerError::Chain("chain_getBlockHash(0) returned a non-32-byte hash".into())
        })
    }
}

struct ProtocolStateDb<'a, S> {
    source: &'a S,
    block_hash: [u8; 32],
    writes: Mutex<BTreeMap<(ColumnFamily, Vec<u8>), Vec<u8>>>,
    deletes: Mutex<BTreeMap<(ColumnFamily, Vec<u8>), ()>>,
}

impl<'a, S> ProtocolStateDb<'a, S> {
    fn new(source: &'a S, block_hash: [u8; 32]) -> Self {
        Self {
            source,
            block_hash,
            writes: Mutex::new(BTreeMap::new()),
            deletes: Mutex::new(BTreeMap::new()),
        }
    }

    fn key(col: ColumnFamily, key: &[u8]) -> (ColumnFamily, Vec<u8>) {
        (col, key.to_vec())
    }
}

unsafe impl<S> Sync for ProtocolStateDb<'_, S> {}

impl<S> DataBase for ProtocolStateDb<'_, S>
where
    S: ProtocolStateSource,
{
    fn key_may_exist<K: AsRef<[u8]>>(&self, col: ColumnFamily, key: &K) -> bool {
        self.get(col, key).ok().flatten().is_some()
    }

    fn get<K: AsRef<[u8]>>(
        &self,
        col: ColumnFamily,
        key: &K,
    ) -> Result<Option<Vec<u8>>, DataBaseError> {
        let key = key.as_ref();
        let db_key = Self::key(col, key);
        if self.deletes.lock().unwrap().contains_key(&db_key) {
            return Ok(None);
        }
        if let Some(value) = self.writes.lock().unwrap().get(&db_key).cloned() {
            return Ok(Some(value));
        }
        if col != column::COL_STATE || key.len() != 31 {
            return Ok(None);
        }
        let mut state_key = [0u8; 31];
        state_key.copy_from_slice(key);
        self.source
            .protocol_state_value_at(self.block_hash, state_key)
            .map_err(|error| DataBaseError::Other(format!("{error:?}")))
    }

    fn del<K: AsRef<[u8]>>(&self, col: ColumnFamily, key: &K) -> Result<(), DataBaseError> {
        let db_key = Self::key(col, key.as_ref());
        self.writes.lock().unwrap().remove(&db_key);
        self.deletes.lock().unwrap().insert(db_key, ());
        Ok(())
    }

    fn multi_get<K: AsRef<[u8]>>(
        &self,
        keys: &[K],
        col: ColumnFamily,
    ) -> Result<Vec<Option<Vec<u8>>>, DataBaseError> {
        keys.iter().map(|key| self.get(col, key)).collect()
    }

    fn put<K: AsRef<[u8]>>(
        &self,
        col: ColumnFamily,
        key: &K,
        value: Box<[u8]>,
    ) -> Result<(), DataBaseError> {
        let db_key = Self::key(col, key.as_ref());
        self.deletes.lock().unwrap().remove(&db_key);
        self.writes.lock().unwrap().insert(db_key, value.into_vec());
        Ok(())
    }

    fn batch_write(&self, changes: &[StoreChange]) -> Result<(), DataBaseError> {
        for change in changes {
            let key = change.key.to_db_key();
            match change.op {
                StoreOp::Remove => self.del(change.col(), &key)?,
                StoreOp::Upsert | StoreOp::Update => {
                    let value = change.value.clone().ok_or(DataBaseError::NotFound)?;
                    self.put(change.col(), &key, value)?;
                }
            }
        }
        Ok(())
    }

    fn batch_write_cf<K: AsRef<[u8]>>(
        &self,
        col: ColumnFamily,
        entries: &[(K, Vec<u8>)],
    ) -> Result<(), DataBaseError> {
        for (key, value) in entries {
            self.put(col, key, value.clone().into_boxed_slice())?;
        }
        Ok(())
    }

    fn multi_seek_for_prev<F>(
        &self,
        _col: ColumnFamily,
        keys: &[&StateKey],
        mut callback: F,
    ) -> Result<(), DataBaseError>
    where
        F: FnMut(usize, Option<(&[u8], &[u8])>),
    {
        for index in 0..keys.len() {
            callback(index, None);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkerChainSource for BlockingHttpWorkerChainSource {
    async fn pending_work_tasks(&self) -> Result<Vec<WorkTask>, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "minijam_getPendingWorkTasks",
                "params": [],
            })
            .to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        let encoded = json_rpc_string_result(&response)?;
        decode_pending_work_tasks_response(&encoded)
    }

    async fn open_vote_tasks(&self) -> Result<Vec<VoteTask>, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "minijam_getOpenVoteTasks",
                "params": [],
            })
            .to_string(),
        )
        .map_err(|error| WorkerError::Chain(error.to_string()))?;
        let encoded = json_rpc_string_result(&response)?;
        decode_open_vote_tasks_response(&encoded)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BlockingHttpBytesClient;

#[async_trait::async_trait]
impl HttpBytesClient for BlockingHttpBytesClient {
    async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        http_get_bytes(url).map_err(|error| FetchError::Transport(error.to_string()))
    }
}

pub fn decode_pending_work_tasks_response(encoded: &str) -> Result<Vec<WorkTask>, WorkerError> {
    let bytes = decode_hex(encoded)?;
    let tasks = Vec::<minijam_protocol::WorkerTaskV1>::decode(&mut bytes.as_slice())
        .map_err(|error| WorkerError::Chain(format!("invalid pending work task batch: {error}")))?;
    Ok(tasks
        .into_iter()
        .map(|task| WorkTask {
            work_id: task.work_id,
            round: task.round,
            assignment_epoch: task.assignment_epoch,
            assigned_workers: task.assigned_workers.into_inner(),
            candidate_producer: task.candidate_producer,
            package_hash: task.package_hash,
            canonical_work_package: task.canonical_work_package.into_inner(),
            bundle_ref: task.bundle_ref,
        })
        .collect())
}

pub fn decode_open_vote_tasks_response(encoded: &str) -> Result<Vec<VoteTask>, WorkerError> {
    let bytes = decode_hex(encoded)?;
    let tasks = Vec::<minijam_protocol::WorkerVerificationTaskV1>::decode(&mut bytes.as_slice())
        .map_err(|error| WorkerError::Chain(format!("invalid open vote task batch: {error}")))?;
    Ok(tasks
        .into_iter()
        .map(|task| VoteTask {
            work_id: task.work_id,
            round: task.round,
            assignment_epoch: task.assignment_epoch,
            candidate_report_hash: task.candidate_report_hash,
            candidate_report: task.candidate_report.into_inner(),
            deadline: task.deadline,
            assigned_workers: task.assigned_workers.into_inner(),
            submitted_votes: task.submitted_votes.into_inner(),
            package_hash: task.package_hash,
            canonical_work_package: task.canonical_work_package.into_inner(),
            bundle_ref: task.bundle_ref,
        })
        .collect())
}

pub fn sr25519_pair_from_uri(uri: &str) -> Result<sr25519::Pair, WorkerError> {
    sr25519::Pair::from_string(uri, None)
        .map_err(|error| WorkerError::Signing(format!("invalid sr25519 key URI: {error}")))
}

pub fn prepare_refine_backed_vote<S>(
    state: &S,
    worker_id: WorkerId,
    pair: &sr25519::Pair,
    chain_id: Hash,
    core_index: u16,
    task: &VoteTask,
    bundle: &[u8],
) -> Result<Option<PreparedVoteSubmission>, WorkerError>
where
    S: ProtocolStateSource,
{
    if !task.assigned_workers.contains(&worker_id) || task.submitted_votes.contains(&worker_id) {
        return Ok(None);
    }
    if minijam_protocol::blake2_256(&task.candidate_report) != task.candidate_report_hash {
        return Err(WorkerError::Refine(
            "candidate canonical report does not match its on-chain hash".into(),
        ));
    }
    let work_task = WorkTask {
        work_id: task.work_id,
        round: task.round,
        assignment_epoch: task.assignment_epoch,
        assigned_workers: task.assigned_workers.clone(),
        candidate_producer: task
            .assigned_workers
            .iter()
            .copied()
            .min()
            .unwrap_or_default(),
        package_hash: task.package_hash,
        canonical_work_package: task.canonical_work_package.clone(),
        bundle_ref: task.bundle_ref.clone(),
    };
    let local = prepare_candidate_envelope(state, chain_id, core_index, &work_task, bundle)?;
    let candidate_metadata =
        jambda_minijam_executive::MiniJamExecutive::project_report(&task.candidate_report)
            .map_err(|error| {
                WorkerError::Refine(format!("invalid candidate report projection: {error:?}"))
            })?;
    let verdict = if local.envelope.canonical_report_hash == task.candidate_report_hash
        && local.envelope.projected_metadata == candidate_metadata
    {
        Verdict::Support
    } else {
        Verdict::Oppose(OpposeReason::InvalidRefine)
    };
    let vote = WorkerVoteV1 {
        work_id: task.work_id,
        round: task.round,
        assignment_epoch: task.assignment_epoch,
        candidate_report_hash: task.candidate_report_hash,
        verdict,
        deadline: task.deadline,
        chain_id,
        protocol_version: PROTOCOL_VERSION_V1,
    };
    Ok(Some(prepare_vote_submission(worker_id, pair, vote)))
}

pub fn prepare_vote_submission(
    worker_id: WorkerId,
    pair: &sr25519::Pair,
    vote: WorkerVoteV1,
) -> PreparedVoteSubmission {
    let signature = pair.sign(&vote.signing_hash()).0;
    let call =
        minijam_runtime::RuntimeCall::MiniJamWorkers(pallet_minijam_workers::Call::submit_vote {
            worker_id,
            vote: vote.clone(),
            signature,
        });
    let extrinsic = minijam_runtime::UncheckedExtrinsic::new_bare(call);
    PreparedVoteSubmission {
        worker_id,
        vote,
        signature,
        extrinsic_hex: hex_encode(&extrinsic.encode()),
    }
}

pub fn prepare_candidate_envelope<S>(
    state: &S,
    chain_id: Hash,
    core_index: u16,
    task: &WorkTask,
    bundle_bytes: &[u8],
) -> Result<PreparedCandidateSubmission, WorkerError>
where
    S: ProtocolStateSource,
{
    let mut raw = bundle_bytes;
    let bundle = jambda_refine::MiniJamWorkBundleV1::decode(&mut raw)
        .map_err(|error| WorkerError::Refine(format!("invalid Jambda work bundle: {error}")))?;
    if !raw.is_empty() {
        return Err(WorkerError::Refine(
            "Jambda work bundle contains trailing bytes".into(),
        ));
    }
    if bundle.version != jambda_refine::MINIJAM_WORK_BUNDLE_VERSION_V1 {
        return Err(WorkerError::Refine(format!(
            "unsupported Jambda work bundle version {}",
            bundle.version
        )));
    }
    if !bundle.package_hash_matches() || bundle.package_hash.0 != task.package_hash {
        return Err(WorkerError::Refine(
            "Jambda work bundle package hash does not match task".into(),
        ));
    }
    let canonical_work_package = bundle.work_package.encode();
    if canonical_work_package != task.canonical_work_package {
        return Err(WorkerError::Refine(
            "Jambda work bundle package does not match task canonical work package".into(),
        ));
    }
    if core_index != stage0::CORE_INDEX
        || bundle.work_package.auth_code_host != stage0::AUTH_CODE_HOST
        || bundle.work_package.auth_code_hash.0 != stage0::AUTH_CODE_HASH
        || !bundle.work_package.authorization.is_empty()
        || !bundle.work_package.authorizer_config.is_empty()
    {
        return Err(WorkerError::Refine(
            "work package does not use the fixed Stage 0 allow-all authorization".into(),
        ));
    }

    let lookup_anchor = bundle.work_package.context.lookup_anchor.0;
    state.validate_finalized_context(stage0::RefineContextV1 {
        anchor: bundle.work_package.context.anchor.0,
        state_root: bundle.work_package.context.state_root.0,
        lookup_anchor,
        lookup_anchor_slot: bundle.work_package.context.lookup_anchor_slot.0,
    })?;
    let input = bundle.into_work_report_input(core_index);
    let db = ProtocolStateDb::new(state, lookup_anchor);
    let mut backend = StateBackend::<TinySpec, _>::new_tiny(db);
    backend
        .load_tiny_from_db()
        .map_err(|error| WorkerError::Refine(format!("failed to load Jambda state: {error:?}")))?;
    let output = jambda_refine::compute_work_report::<
        TinySpec,
        ProtocolStateDb<'_, S>,
        StateBackend<TinySpec, ProtocolStateDb<'_, S>>,
        InterpBackend,
        jp_vm_engine::InnerEngine<InterpBackend>,
    >(&backend, input, InterpBackend::default())
    .map_err(|error| WorkerError::Refine(format!("Jambda refine failed: {error:?}")))?;
    let canonical_report = output.report.encode();
    let projected_metadata =
        jambda_minijam_executive::MiniJamExecutive::project_report(&canonical_report).map_err(
            |error| WorkerError::Refine(format!("invalid generated report projection: {error:?}")),
        )?;
    let canonical_report_hash = minijam_protocol::blake2_256(&canonical_report);
    let envelope = ReportEnvelopeV1 {
        protocol_version: PROTOCOL_VERSION_V1,
        chain_id,
        work_id: task.work_id,
        assignment_round: task.round,
        canonical_report: canonical_report
            .try_into()
            .map_err(|_| WorkerError::Refine("generated report exceeds envelope limit".into()))?,
        canonical_report_hash,
        projected_metadata,
        bulletin_evidence: BulletinEvidence::NoExternalProofV1 { receipt: None },
        signatures: Default::default(),
    };
    Ok(PreparedCandidateSubmission {
        envelope,
        exported_segment_count: output.exported_segments.len(),
    })
}

pub fn prepare_signed_candidate_submission(
    pair: &sr25519::Pair,
    nonce: minijam_runtime::Nonce,
    genesis_hash: Hash,
    envelope: ReportEnvelopeV1,
) -> PreparedSignedCandidateSubmission {
    let call = minijam_runtime::RuntimeCall::MiniJam(pallet_minijam::Call::submit_candidate {
        envelope: Box::new(envelope.clone()),
    });
    let extrinsic = minijam_chain_client::sign_runtime_call(pair, nonce, genesis_hash, call);
    PreparedSignedCandidateSubmission {
        envelope,
        nonce,
        extrinsic_hex: hex_encode(&extrinsic),
    }
}

fn json_rpc_string_result(response: &str) -> Result<String, WorkerError> {
    let value: Value = serde_json::from_str(response)
        .map_err(|error| WorkerError::Chain(format!("invalid JSON-RPC response: {error}")))?;
    if let Some(error) = value.get("error") {
        return Err(WorkerError::Chain(format!("JSON-RPC error: {error}")));
    }
    value
        .get("result")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            WorkerError::Chain("JSON-RPC response did not contain a string result".into())
        })
}

fn json_rpc_u32_result(response: &str) -> Result<u32, WorkerError> {
    let value: Value = serde_json::from_str(response)
        .map_err(|error| WorkerError::Chain(format!("invalid JSON-RPC response: {error}")))?;
    if let Some(error) = value.get("error") {
        return Err(WorkerError::Chain(format!("JSON-RPC error: {error}")));
    }
    if let Some(number) = value.get("result").and_then(Value::as_u64) {
        return number
            .try_into()
            .map_err(|_| WorkerError::Chain("JSON-RPC u32 result is out of range".into()));
    }
    if let Some(text) = value.get("result").and_then(Value::as_str) {
        return text
            .parse::<u32>()
            .map_err(|error| WorkerError::Chain(format!("invalid JSON-RPC u32 result: {error}")));
    }
    Err(WorkerError::Chain(
        "JSON-RPC response did not contain a u32 result".into(),
    ))
}

fn json_rpc_optional_string_result(response: &str) -> Result<Option<String>, WorkerError> {
    let value: Value = serde_json::from_str(response)
        .map_err(|error| WorkerError::Chain(format!("invalid JSON-RPC response: {error}")))?;
    if let Some(error) = value.get("error") {
        return Err(WorkerError::Chain(format!("JSON-RPC error: {error}")));
    }
    match value.get("result") {
        Some(Value::Null) => Ok(None),
        Some(Value::String(result)) => Ok(Some(result.clone())),
        _ => Err(WorkerError::Chain(
            "JSON-RPC response did not contain an optional string result".into(),
        )),
    }
}

fn http_post_json(url: &str, body: &str) -> Result<String, HttpError> {
    let endpoint = HttpEndpoint::parse(url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .map_err(|error| HttpError(error.to_string()))?;
    write!(
        stream,
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        endpoint.path,
        endpoint.host,
        body.len(),
        body
    )
    .map_err(|error| HttpError(error.to_string()))?;
    let body = read_http_body(stream)?;
    String::from_utf8(body).map_err(|error| HttpError(error.to_string()))
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>, HttpError> {
    let endpoint = HttpEndpoint::parse(url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .map_err(|error| HttpError(error.to_string()))?;
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        endpoint.path, endpoint.host
    )
    .map_err(|error| HttpError(error.to_string()))?;
    read_http_body(stream)
}

fn read_http_body(mut stream: TcpStream) -> Result<Vec<u8>, HttpError> {
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| HttpError(error.to_string()))?;
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| HttpError("HTTP response did not contain a header/body separator".into()))?;
    let headers = String::from_utf8_lossy(&response[..separator]);
    if !http_response_is_success(&headers) {
        return Err(HttpError(format!(
            "HTTP request failed: {}",
            headers.lines().next().unwrap_or_else(|| headers.as_ref())
        )));
    }
    Ok(response[(separator + 4)..].to_vec())
}

fn http_response_is_success(headers: &str) -> bool {
    headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .is_some_and(|status| (200..300).contains(&status))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HttpEndpoint {
    host: String,
    port: u16,
    path: String,
}

impl HttpEndpoint {
    fn parse(url: &str) -> Result<Self, HttpError> {
        let stripped = url
            .strip_prefix("http://")
            .ok_or_else(|| HttpError("worker active polling requires an http:// RPC URL".into()))?;
        let (authority, path) = match stripped.split_once('/') {
            Some((authority, path)) => (authority, format!("/{path}")),
            None => (stripped, "/".to_string()),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (
                host.to_string(),
                port.parse::<u16>()
                    .map_err(|error| HttpError(error.to_string()))?,
            ),
            None => (authority.to_string(), 80),
        };
        Ok(Self { host, port, path })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HttpError(String);

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for HttpError {}

fn decode_hex(input: &str) -> Result<Vec<u8>, WorkerError> {
    let hex = input.strip_prefix("0x").unwrap_or(input);
    if hex.len() % 2 != 0 {
        return Err(WorkerError::Chain("hex input has odd length".into()));
    }
    let mut output = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks_exact(2) {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        output.push((high << 4) | low);
    }
    Ok(output)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::from("0x");
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_nibble(byte: u8) -> Result<u8, WorkerError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(WorkerError::Chain(
            "hex input contains a non-hex character".into(),
        )),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryDbError {
    Io(String),
    Decode(String),
    Encode(String),
}

impl core::fmt::Display for RecoveryDbError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "worker recovery db I/O error: {error}"),
            Self::Decode(error) => write!(f, "worker recovery db decode error: {error}"),
            Self::Encode(error) => write!(f, "worker recovery db encode error: {error}"),
        }
    }
}

impl std::error::Error for RecoveryDbError {}

#[derive(Clone, Debug)]
pub struct WorkerRecoveryDb {
    path: PathBuf,
}

impl WorkerRecoveryDb {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_statuses(
        &self,
    ) -> Result<BTreeMap<(WorkId, u8), WorkerTaskStatus>, RecoveryDbError> {
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }
        let contents = std::fs::read_to_string(&self.path)
            .map_err(|error| RecoveryDbError::Io(error.to_string()))?;
        let file: WorkerRecoveryFile = toml::from_str(&contents)
            .map_err(|error| RecoveryDbError::Decode(error.to_string()))?;
        Ok(file.into_statuses())
    }

    pub fn save_statuses(
        &self,
        statuses: &BTreeMap<(WorkId, u8), WorkerTaskStatus>,
    ) -> Result<(), RecoveryDbError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| RecoveryDbError::Io(error.to_string()))?;
        }
        let file = WorkerRecoveryFile::from_statuses(statuses);
        let contents = toml::to_string_pretty(&file)
            .map_err(|error| RecoveryDbError::Encode(error.to_string()))?;
        std::fs::write(&self.path, contents).map_err(|error| RecoveryDbError::Io(error.to_string()))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct WorkerRecoveryFile {
    tasks: Vec<PersistedWorkerTaskStatus>,
}

impl WorkerRecoveryFile {
    fn from_statuses(statuses: &BTreeMap<(WorkId, u8), WorkerTaskStatus>) -> Self {
        Self {
            tasks: statuses
                .iter()
                .map(|(&(work_id, round), status)| {
                    PersistedWorkerTaskStatus::from_status(work_id, round, status)
                })
                .collect(),
        }
    }

    fn into_statuses(self) -> BTreeMap<(WorkId, u8), WorkerTaskStatus> {
        self.tasks
            .into_iter()
            .map(|status| {
                let key = (status.work_id, status.round);
                (key, status.into_status())
            })
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PersistedWorkerTaskStatus {
    work_id: WorkId,
    round: u8,
    status: String,
    bundle_len: Option<usize>,
    reason: Option<String>,
}

impl PersistedWorkerTaskStatus {
    fn from_status(work_id: WorkId, round: u8, status: &WorkerTaskStatus) -> Self {
        match status {
            WorkerTaskStatus::BundleReady { bundle_len } => Self {
                work_id,
                round,
                status: "bundle_ready".into(),
                bundle_len: Some(*bundle_len),
                reason: None,
            },
            WorkerTaskStatus::BundleRejected { reason } => Self {
                work_id,
                round,
                status: "bundle_rejected".into(),
                bundle_len: None,
                reason: Some(format!("{reason:?}")),
            },
        }
    }

    fn into_status(self) -> WorkerTaskStatus {
        if self.status == "bundle_ready" {
            WorkerTaskStatus::BundleReady {
                bundle_len: self.bundle_len.unwrap_or_default(),
            }
        } else {
            WorkerTaskStatus::BundleRejected {
                reason: WorkerError::Chain(
                    self.reason
                        .unwrap_or_else(|| "persisted bundle rejection".into()),
                ),
            }
        }
    }
}

#[async_trait::async_trait]
pub trait WorkerChainSource: Send + Sync {
    async fn pending_work_tasks(&self) -> Result<Vec<WorkTask>, WorkerError>;

    async fn open_vote_tasks(&self) -> Result<Vec<VoteTask>, WorkerError> {
        Ok(Vec::new())
    }
}

#[async_trait::async_trait]
pub trait WorkerTxSubmitter: Send + Sync {
    async fn submit_raw_extrinsic(&self, extrinsic_hex: &str) -> Result<Hash, WorkerError>;
}

#[async_trait::async_trait]
impl WorkerTxSubmitter for BlockingHttpWorkerChainSource {
    async fn submit_raw_extrinsic(&self, extrinsic_hex: &str) -> Result<Hash, WorkerError> {
        BlockingHttpWorkerChainSource::submit_raw_extrinsic(self, extrinsic_hex).await
    }
}

pub struct WorkerRunner<C, F, D> {
    chain: C,
    fetcher: F,
    decoder: D,
    max_bundle_bytes: u64,
    statuses: BTreeMap<(WorkId, u8), WorkerTaskStatus>,
}

pub type Stage0WorkerRunner<C, F> = WorkerRunner<C, F, MiniJamWorkBundleDecoder>;

impl<C, F, D> WorkerRunner<C, F, D> {
    pub fn new(chain: C, fetcher: F, decoder: D, max_bundle_bytes: u64) -> Self {
        Self {
            chain,
            fetcher,
            decoder,
            max_bundle_bytes,
            statuses: BTreeMap::new(),
        }
    }

    pub fn with_statuses(
        chain: C,
        fetcher: F,
        decoder: D,
        max_bundle_bytes: u64,
        statuses: BTreeMap<(WorkId, u8), WorkerTaskStatus>,
    ) -> Self {
        Self {
            chain,
            fetcher,
            decoder,
            max_bundle_bytes,
            statuses,
        }
    }

    pub fn status(&self, work_id: WorkId, round: u8) -> Option<&WorkerTaskStatus> {
        self.statuses.get(&(work_id, round))
    }

    pub fn statuses(&self) -> &BTreeMap<(WorkId, u8), WorkerTaskStatus> {
        &self.statuses
    }
}

impl<C, F> WorkerRunner<C, F, MiniJamWorkBundleDecoder> {
    pub fn stage0(chain: C, fetcher: F, max_bundle_bytes: u64) -> Self {
        Self::new(chain, fetcher, MiniJamWorkBundleDecoder, max_bundle_bytes)
    }
}

impl<C, F, D> WorkerRunner<C, F, D>
where
    C: WorkerChainSource,
    F: ContentFetcher,
    D: WorkBundleDecoder,
{
    pub async fn poll_once(&mut self) -> Result<usize, WorkerError> {
        self.poll_once_inner(None).await
    }

    pub async fn poll_once_with_metrics(
        &mut self,
        metrics: &WorkerMetrics,
    ) -> Result<usize, WorkerError> {
        self.poll_once_inner(Some(metrics)).await
    }

    async fn poll_once_inner(
        &mut self,
        metrics: Option<&WorkerMetrics>,
    ) -> Result<usize, WorkerError> {
        let tasks = self.chain.pending_work_tasks().await?;
        let mut processed = 0usize;
        for task in tasks {
            let key = (task.work_id, task.round);
            if matches!(
                self.statuses.get(&key),
                Some(WorkerTaskStatus::BundleReady { .. })
            ) {
                continue;
            }

            let status = match self.prepare_bundle(&task).await {
                Ok(bundle_len) => {
                    if let Some(metrics) = metrics {
                        metrics.record_bundle_ready();
                    }
                    WorkerTaskStatus::BundleReady { bundle_len }
                }
                Err(error) => {
                    if let Some(metrics) = metrics {
                        metrics.record_bundle_rejected();
                    }
                    WorkerTaskStatus::BundleRejected { reason: error }
                }
            };
            self.statuses.insert(key, status);
            processed = processed.saturating_add(1);
        }
        if let Some(metrics) = metrics {
            metrics.record_poll(processed);
        }
        Ok(processed)
    }

    pub async fn poll_once_with_recovery(
        &mut self,
        recovery: &WorkerRecoveryDb,
    ) -> Result<usize, WorkerError> {
        let processed = self.poll_once().await?;
        recovery
            .save_statuses(&self.statuses)
            .map_err(|error| WorkerError::Chain(error.to_string()))?;
        Ok(processed)
    }

    pub async fn poll_open_vote_tasks_with_metrics(
        &self,
        metrics: &WorkerMetrics,
    ) -> Result<Vec<VoteTask>, WorkerError> {
        let tasks = self.chain.open_vote_tasks().await?;
        metrics.record_vote_tasks_seen(tasks.len());
        Ok(tasks)
    }

    async fn prepare_bundle(&self, task: &WorkTask) -> Result<usize, WorkerError> {
        let bundle = fetch_verified_content(&self.fetcher, &task.bundle_ref, self.max_bundle_bytes)
            .await
            .map_err(WorkerError::Fetch)?;
        verify_work_bundle(
            &task.bundle_ref,
            &bundle,
            self.max_bundle_bytes,
            task.package_hash,
            &self.decoder,
        )
        .map_err(WorkerError::Bundle)?;
        Ok(bundle.len())
    }
}

impl<C, F, D> WorkerRunner<C, F, D>
where
    C: WorkerChainSource + WorkerTxSubmitter + ProtocolStateSource,
    F: ContentFetcher,
    D: WorkBundleDecoder,
{
    pub async fn submit_refine_votes(
        &self,
        worker_id: WorkerId,
        pair: &sr25519::Pair,
        chain_id: Hash,
        core_index: u16,
        metrics: Option<&WorkerMetrics>,
    ) -> Result<Vec<Hash>, WorkerError> {
        let tasks = self.chain.open_vote_tasks().await?;
        if let Some(metrics) = metrics {
            metrics.record_vote_tasks_seen(tasks.len());
        }
        let mut tx_hashes = Vec::new();
        for task in tasks {
            if !task.assigned_workers.contains(&worker_id)
                || task.submitted_votes.contains(&worker_id)
            {
                continue;
            }
            let bundle =
                fetch_verified_content(&self.fetcher, &task.bundle_ref, self.max_bundle_bytes)
                    .await
                    .map_err(WorkerError::Fetch)?;
            verify_work_bundle(
                &task.bundle_ref,
                &bundle,
                self.max_bundle_bytes,
                task.package_hash,
                &self.decoder,
            )
            .map_err(WorkerError::Bundle)?;
            let Some(submission) = prepare_refine_backed_vote(
                &self.chain,
                worker_id,
                pair,
                chain_id,
                core_index,
                &task,
                &bundle,
            )?
            else {
                continue;
            };
            tx_hashes.push(
                self.chain
                    .submit_raw_extrinsic(&submission.extrinsic_hex)
                    .await?,
            );
        }
        Ok(tx_hashes)
    }
}

impl<C, F, D> WorkerRunner<C, F, D>
where
    C: WorkerChainSource + WorkerTxSubmitter + ProtocolStateSource + WorkerSignedTxContext,
    F: ContentFetcher,
    D: WorkBundleDecoder,
{
    pub async fn submit_candidate_reports(
        &self,
        worker_id: WorkerId,
        pair: &sr25519::Pair,
        chain_id: Hash,
        core_index: u16,
        metrics: Option<&WorkerMetrics>,
    ) -> Result<Vec<Hash>, WorkerError> {
        let tasks = self.chain.pending_work_tasks().await?;
        let mut nonce = self.chain.account_nonce(pair.public().0)?;
        let genesis_hash = self.chain.genesis_hash()?;
        let mut tx_hashes = Vec::new();
        for task in tasks {
            if !task.assigned_workers.contains(&worker_id) || task.candidate_producer != worker_id {
                continue;
            }
            let bundle =
                fetch_verified_content(&self.fetcher, &task.bundle_ref, self.max_bundle_bytes)
                    .await
                    .map_err(WorkerError::Fetch)?;
            verify_work_bundle(
                &task.bundle_ref,
                &bundle,
                self.max_bundle_bytes,
                task.package_hash,
                &self.decoder,
            )
            .map_err(WorkerError::Bundle)?;
            let candidate =
                prepare_candidate_envelope(&self.chain, chain_id, core_index, &task, &bundle)?;
            let submission =
                prepare_signed_candidate_submission(pair, nonce, genesis_hash, candidate.envelope);
            tx_hashes.push(
                self.chain
                    .submit_raw_extrinsic(&submission.extrinsic_hex)
                    .await?,
            );
            if let Some(metrics) = metrics {
                metrics.record_bundle_ready();
            }
            nonce = nonce.saturating_add(1);
        }
        Ok(tx_hashes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use jam_codec::Encode as JamEncode;
    use jp_core_primitives::{
        crypto::OpaqueHash,
        simple::{ByteSequence, TimeSlot},
        traits::JamHash,
        work::{RefineContext, WorkPackage},
    };
    use minijam_protocol::blake2_256;
    use minijam_worker_engine::{fetch::MemoryContentFetcher, MiniJamWorkBundleDecoder};
    use parity_scale_codec::Encode;
    use std::sync::Mutex;

    #[test]
    fn default_worker_config_is_valid() {
        WorkerConfig::default().validate().unwrap();
    }

    #[test]
    fn http_success_accepts_no_content_readiness_response() {
        assert!(http_response_is_success("HTTP/1.1 200 OK\r\n"));
        assert!(http_response_is_success("HTTP/1.1 204 No Content\r\n"));
        assert!(!http_response_is_success(
            "HTTP/1.1 503 Service Unavailable\r\n"
        ));
    }

    #[test]
    fn config_rejects_zero_limits() {
        let mut config = WorkerConfig::default();
        config.max_bundle_bytes = 0;
        assert_eq!(config.validate(), Err(ConfigError::ZeroMaxBundleBytes));

        let mut config = WorkerConfig::default();
        config.poll_interval = Duration::ZERO;
        assert_eq!(config.validate(), Err(ConfigError::ZeroPollInterval));
    }

    #[test]
    fn config_loads_documented_toml_shape() {
        let config = WorkerConfig::from_toml_str(
            r#"
            [node]
            rpc_url = "ws://node.example:9944"

            [worker]
            key = "//Alice"
            worker_id = 7
            core_index = 2
            submit_candidates = true
            submit_support_votes = true
            poll_interval_ms = 250
            recovery_db_path = "/var/lib/minijam/worker-state.toml"

            [metrics]
            bind = "127.0.0.1:9616"

            [content]
            ipfs_gateway = "http://127.0.0.1:8080/ipfs"
            request_timeout_secs = 15
            max_bundle_bytes = 4096
            "#,
        )
        .unwrap();

        assert_eq!(config.rpc_url, "ws://node.example:9944");
        assert_eq!(config.key, Some("//Alice".into()));
        assert_eq!(config.worker_id, Some(7));
        assert_eq!(config.core_index, 2);
        assert!(config.submit_candidates);
        assert!(config.submit_support_votes);
        assert_eq!(config.poll_interval, Duration::from_millis(250));
        assert_eq!(
            config.recovery_db_path,
            Some(PathBuf::from("/var/lib/minijam/worker-state.toml"))
        );
        assert_eq!(config.metrics_bind, Some("127.0.0.1:9616".into()));
        assert_eq!(config.ipfs_gateway, "http://127.0.0.1:8080/ipfs");
        assert_eq!(config.request_timeout, Duration::from_secs(15));
        assert_eq!(config.max_bundle_bytes, 4096);
    }

    #[test]
    fn config_file_uses_defaults_for_missing_sections() {
        let config = WorkerConfig::from_toml_str(
            r#"
            [node]
            rpc_url = "ws://node.example:9944"
            "#,
        )
        .unwrap();

        assert_eq!(config.rpc_url, "ws://node.example:9944");
        assert_eq!(config.ipfs_gateway, WorkerConfig::default().ipfs_gateway);
        assert_eq!(
            config.request_timeout,
            WorkerConfig::default().request_timeout
        );
    }

    #[derive(Clone)]
    struct TestChainSource {
        tasks: Vec<WorkTask>,
    }

    struct EmptyProtocolStateSource;

    impl ProtocolStateSource for EmptyProtocolStateSource {
        fn protocol_state_value_at(
            &self,
            _block_hash: [u8; 32],
            _key: [u8; 31],
        ) -> Result<Option<Vec<u8>>, WorkerError> {
            Ok(None)
        }
    }

    struct FailingProtocolStateSource;

    impl ProtocolStateSource for FailingProtocolStateSource {
        fn validate_finalized_context(
            &self,
            _context: stage0::RefineContextV1,
        ) -> Result<(), WorkerError> {
            Err(WorkerError::Chain("historical state unavailable".into()))
        }

        fn protocol_state_value_at(
            &self,
            _block_hash: [u8; 32],
            _key: [u8; 31],
        ) -> Result<Option<Vec<u8>>, WorkerError> {
            Err(WorkerError::Chain("historical state unavailable".into()))
        }
    }

    #[derive(Default)]
    struct TrackingProtocolStateSource {
        anchors: Mutex<Vec<[u8; 32]>>,
        reject_anchor: bool,
        finalized_context: Option<stage0::FinalizedContextV1>,
    }

    impl ProtocolStateSource for TrackingProtocolStateSource {
        fn validate_finalized_context(
            &self,
            context: stage0::RefineContextV1,
        ) -> Result<(), WorkerError> {
            if self.reject_anchor {
                return Err(WorkerError::Refine(format!(
                    "anchor {} is not finalized",
                    hex_encode(&context.lookup_anchor)
                )));
            }
            if let Some(finalized) = self.finalized_context {
                stage0::validate_refine_context(context, finalized).map_err(|error| {
                    WorkerError::Refine(format!(
                        "invalid finalized Stage 0 refine context: {error:?}"
                    ))
                })?;
            }
            Ok(())
        }

        fn protocol_state_value_at(
            &self,
            block_hash: [u8; 32],
            _key: [u8; 31],
        ) -> Result<Option<Vec<u8>>, WorkerError> {
            self.anchors.lock().unwrap().push(block_hash);
            Ok(None)
        }
    }

    #[async_trait::async_trait]
    impl WorkerChainSource for TestChainSource {
        async fn pending_work_tasks(&self) -> Result<Vec<WorkTask>, WorkerError> {
            Ok(self.tasks.clone())
        }
    }

    struct TestVoteChainSource {
        vote_tasks: Vec<VoteTask>,
    }

    #[async_trait::async_trait]
    impl WorkerChainSource for TestVoteChainSource {
        async fn pending_work_tasks(&self) -> Result<Vec<WorkTask>, WorkerError> {
            Ok(Vec::new())
        }

        async fn open_vote_tasks(&self) -> Result<Vec<VoteTask>, WorkerError> {
            Ok(self.vote_tasks.clone())
        }
    }

    #[derive(Clone)]
    struct TestVoteSubmitChainSource {
        vote_tasks: Vec<VoteTask>,
        submitted: Arc<Mutex<Vec<String>>>,
    }

    #[derive(Clone)]
    struct TestCandidateSubmitChainSource {
        tasks: Vec<WorkTask>,
        submitted: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl WorkerChainSource for TestVoteSubmitChainSource {
        async fn pending_work_tasks(&self) -> Result<Vec<WorkTask>, WorkerError> {
            Ok(Vec::new())
        }

        async fn open_vote_tasks(&self) -> Result<Vec<VoteTask>, WorkerError> {
            Ok(self.vote_tasks.clone())
        }
    }

    #[async_trait::async_trait]
    impl WorkerTxSubmitter for TestVoteSubmitChainSource {
        async fn submit_raw_extrinsic(&self, extrinsic_hex: &str) -> Result<Hash, WorkerError> {
            self.submitted
                .lock()
                .unwrap()
                .push(extrinsic_hex.to_string());
            Ok([7u8; 32])
        }
    }

    impl ProtocolStateSource for TestVoteSubmitChainSource {
        fn protocol_state_value_at(
            &self,
            _block_hash: [u8; 32],
            _key: [u8; 31],
        ) -> Result<Option<Vec<u8>>, WorkerError> {
            Ok(None)
        }
    }

    #[async_trait::async_trait]
    impl WorkerChainSource for TestCandidateSubmitChainSource {
        async fn pending_work_tasks(&self) -> Result<Vec<WorkTask>, WorkerError> {
            Ok(self.tasks.clone())
        }
    }

    #[async_trait::async_trait]
    impl WorkerTxSubmitter for TestCandidateSubmitChainSource {
        async fn submit_raw_extrinsic(&self, extrinsic_hex: &str) -> Result<Hash, WorkerError> {
            self.submitted
                .lock()
                .unwrap()
                .push(extrinsic_hex.to_string());
            Ok([8u8; 32])
        }
    }

    impl ProtocolStateSource for TestCandidateSubmitChainSource {
        fn protocol_state_value_at(
            &self,
            _block_hash: [u8; 32],
            _key: [u8; 31],
        ) -> Result<Option<Vec<u8>>, WorkerError> {
            Ok(None)
        }
    }

    impl WorkerSignedTxContext for TestCandidateSubmitChainSource {
        fn account_nonce(&self, _account: [u8; 32]) -> Result<minijam_runtime::Nonce, WorkerError> {
            Ok(3)
        }

        fn genesis_hash(&self) -> Result<Hash, WorkerError> {
            Ok([9u8; 32])
        }
    }

    fn task(work_id: WorkId, round: u8, package_hash: Hash, bundle: &[u8]) -> WorkTask {
        WorkTask {
            work_id,
            round,
            assignment_epoch: 1,
            assigned_workers: vec![0, 1, 2],
            candidate_producer: 0,
            package_hash,
            canonical_work_package: Vec::from([1, 2, 3]),
            bundle_ref: ContentRef {
                cid_v1: format!("cid-{work_id}-{round}")
                    .into_bytes()
                    .try_into()
                    .unwrap(),
                content_hash: blake2_256(bundle),
                size: bundle.len() as u64,
            },
        }
    }

    fn refine_package(seed: u8) -> WorkPackage {
        WorkPackage {
            auth_code_host: 0,
            auth_code_hash: OpaqueHash(stage0::AUTH_CODE_HASH),
            context: RefineContext {
                anchor: OpaqueHash([5u8; 32]),
                state_root: OpaqueHash([seed; 32]),
                beefy_root: OpaqueHash([4u8; 32]),
                lookup_anchor: OpaqueHash([5u8; 32]),
                lookup_anchor_slot: TimeSlot(0),
                prerequisites: Vec::new(),
            },
            authorization: ByteSequence::from(Vec::new()),
            authorizer_config: ByteSequence::from(Vec::new()),
            items: Vec::new(),
        }
    }

    fn refine_bundle(seed: u8) -> (Vec<u8>, Hash) {
        let package = refine_package(seed);
        let package_hash = package.jam_hash().0;
        let input = jambda_refine::WorkReportInput {
            core_index: 0,
            work_package: Arc::new(package),
            external_data: Arc::new(Vec::new()),
            import_segments: Arc::new(Vec::new()),
            import_proofs: Default::default(),
        };
        (
            jambda_refine::MiniJamWorkBundleV1::new(&input).encode(),
            package_hash,
        )
    }

    fn refine_task(work_id: WorkId, round: u8, package_hash: Hash, bundle: &[u8]) -> WorkTask {
        let mut raw = bundle;
        let bundle = jambda_refine::MiniJamWorkBundleV1::decode(&mut raw).unwrap();
        let encoded_bundle = bundle.encode();
        WorkTask {
            work_id,
            round,
            assignment_epoch: 1,
            assigned_workers: vec![0, 1, 2],
            candidate_producer: 0,
            package_hash,
            canonical_work_package: bundle.work_package.encode(),
            bundle_ref: ContentRef {
                cid_v1: format!("cid-{work_id}-{round}")
                    .into_bytes()
                    .try_into()
                    .unwrap(),
                content_hash: blake2_256(&encoded_bundle),
                size: encoded_bundle.len() as u64,
            },
        }
    }

    fn refine_vote_task(
        work_id: WorkId,
        assigned_workers: Vec<WorkerId>,
        submitted_votes: Vec<WorkerId>,
    ) -> (VoteTask, Vec<u8>) {
        let (bundle, package_hash) = refine_bundle(work_id as u8);
        let work = refine_task(work_id, 1, package_hash, &bundle);
        let candidate =
            prepare_candidate_envelope(&EmptyProtocolStateSource, [42; 32], 0, &work, &bundle)
                .unwrap()
                .envelope;
        (
            VoteTask {
                work_id,
                round: 1,
                assignment_epoch: 7,
                candidate_report_hash: candidate.canonical_report_hash,
                candidate_report: candidate.canonical_report.into_inner(),
                deadline: 100,
                assigned_workers,
                submitted_votes,
                package_hash,
                canonical_work_package: work.canonical_work_package,
                bundle_ref: work.bundle_ref,
            },
            bundle,
        )
    }

    #[test]
    fn runner_fetches_and_verifies_pending_work_bundles() {
        let (bundle, package_hash) = refine_bundle(7);
        let task = task(1, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle.clone());
        let mut runner = WorkerRunner::stage0(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            1024,
        );

        assert_eq!(block_on(runner.poll_once()).unwrap(), 1);
        assert_eq!(
            runner.status(task.work_id, task.round),
            Some(&WorkerTaskStatus::BundleReady {
                bundle_len: bundle.len()
            })
        );
        assert_eq!(block_on(runner.poll_once()).unwrap(), 0);
    }

    #[test]
    fn metrics_render_prometheus_counters() {
        let metrics = WorkerMetrics::new();
        metrics.record_poll(2);
        metrics.record_bundle_ready();
        metrics.record_bundle_rejected();

        let rendered = metrics.render_prometheus();

        assert!(rendered.contains("minijam_worker_polls_total 1"));
        assert!(rendered.contains("minijam_worker_tasks_processed_total 2"));
        assert!(rendered.contains("minijam_worker_bundle_ready_total 1"));
        assert!(rendered.contains("minijam_worker_bundle_rejected_total 1"));
    }

    #[test]
    fn runner_records_metrics_for_bundle_outcomes() {
        let (good_bundle, package_hash) = refine_bundle(7);
        let (bad_bundle, _) = refine_bundle(8);
        let good_task = task(10, 0, package_hash, &good_bundle);
        let bad_task = task(11, 0, package_hash, &bad_bundle);
        let fetcher = MemoryContentFetcher::new()
            .with_content(&good_task.bundle_ref, good_bundle)
            .with_content(&bad_task.bundle_ref, bad_bundle);
        let mut runner = WorkerRunner::stage0(
            TestChainSource {
                tasks: vec![good_task, bad_task],
            },
            fetcher,
            1024,
        );
        let metrics = WorkerMetrics::new();

        assert_eq!(
            block_on(runner.poll_once_with_metrics(&metrics)).unwrap(),
            2
        );

        let rendered = metrics.render_prometheus();
        assert!(rendered.contains("minijam_worker_polls_total 1"));
        assert!(rendered.contains("minijam_worker_tasks_processed_total 2"));
        assert!(rendered.contains("minijam_worker_bundle_ready_total 1"));
        assert!(rendered.contains("minijam_worker_bundle_rejected_total 1"));
    }

    #[test]
    fn runner_records_open_vote_task_metrics() {
        let (vote_task, _) = refine_vote_task(42, vec![0, 1, 2], vec![1]);
        let runner = WorkerRunner::stage0(
            TestVoteChainSource {
                vote_tasks: vec![vote_task],
            },
            MemoryContentFetcher::new(),
            64,
        );
        let metrics = WorkerMetrics::new();

        let tasks = block_on(runner.poll_open_vote_tasks_with_metrics(&metrics)).unwrap();

        assert_eq!(tasks.len(), 1);
        assert!(metrics
            .render_prometheus()
            .contains("minijam_worker_vote_tasks_seen_total 1"));
    }

    #[test]
    fn decodes_pending_work_tasks_rpc_response() {
        let package_hash = [7u8; 32];
        let bundle_ref = ContentRef {
            cid_v1: b"cid-1".to_vec().try_into().unwrap(),
            content_hash: [8u8; 32],
            size: 32,
        };
        let tasks = vec![minijam_protocol::WorkerTaskV1 {
            work_id: 42,
            round: 1,
            assignment_epoch: 3,
            assigned_workers: vec![0, 1, 2].try_into().unwrap(),
            candidate_producer: 0,
            package_hash,
            canonical_work_package: vec![1, 2, 3].try_into().unwrap(),
            bundle_ref: bundle_ref.clone(),
        }];
        let encoded = hex_encode_for_test(&tasks.encode());

        let decoded = decode_pending_work_tasks_response(&encoded).unwrap();

        assert_eq!(
            decoded,
            vec![WorkTask {
                work_id: 42,
                round: 1,
                assignment_epoch: 3,
                assigned_workers: vec![0, 1, 2],
                candidate_producer: 0,
                package_hash,
                canonical_work_package: vec![1, 2, 3],
                bundle_ref,
            }]
        );
    }

    #[test]
    fn decodes_open_vote_tasks_rpc_response() {
        let bundle_ref = ContentRef {
            cid_v1: b"cid-vote".to_vec().try_into().unwrap(),
            content_hash: [6; 32],
            size: 8,
        };
        let tasks = vec![minijam_protocol::WorkerVerificationTaskV1 {
            work_id: 42,
            round: 1,
            assignment_epoch: 7,
            candidate_report_hash: [9u8; 32],
            candidate_report: vec![1, 2].try_into().unwrap(),
            deadline: 100,
            assigned_workers: vec![0, 2, 4].try_into().unwrap(),
            submitted_votes: vec![2].try_into().unwrap(),
            package_hash: [8; 32],
            canonical_work_package: vec![3, 4].try_into().unwrap(),
            bundle_ref: bundle_ref.clone(),
        }];
        let encoded = hex_encode_for_test(&tasks.encode());

        let decoded = decode_open_vote_tasks_response(&encoded).unwrap();

        assert_eq!(
            decoded,
            vec![VoteTask {
                work_id: 42,
                round: 1,
                assignment_epoch: 7,
                candidate_report_hash: [9u8; 32],
                candidate_report: vec![1, 2],
                deadline: 100,
                assigned_workers: vec![0, 2, 4],
                submitted_votes: vec![2],
                package_hash: [8; 32],
                canonical_work_package: vec![3, 4],
                bundle_ref,
            }]
        );
    }

    #[test]
    fn prepares_support_only_after_independent_refine() {
        let pair = sr25519::Pair::from_seed(&[1u8; 32]);
        let (task, bundle) = refine_vote_task(42, vec![3, 4, 5], vec![4]);
        let submission = prepare_refine_backed_vote(
            &EmptyProtocolStateSource,
            3,
            &pair,
            [42u8; 32],
            0,
            &task,
            &bundle,
        )
        .unwrap()
        .unwrap();

        assert_eq!(submission.worker_id, 3);
        assert_eq!(submission.vote.work_id, 42);
        assert_eq!(submission.vote.verdict, Verdict::Support);
        assert!(sr25519::Pair::verify(
            &sr25519::Signature::from_raw(submission.signature),
            &submission.vote.signing_hash(),
            &pair.public(),
        ));
        assert!(submission.extrinsic_hex.starts_with("0x"));
        assert!(submission.extrinsic_hex.len() > 16);
        assert!(prepare_refine_backed_vote(
            &EmptyProtocolStateSource,
            4,
            &pair,
            [42; 32],
            0,
            &task,
            &bundle
        )
        .unwrap()
        .is_none());
        assert!(prepare_refine_backed_vote(
            &EmptyProtocolStateSource,
            9,
            &pair,
            [42; 32],
            0,
            &task,
            &bundle
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn independently_refined_mismatched_candidate_is_opposed() {
        let pair = sr25519::Pair::from_seed(&[2; 32]);
        let (mut task, bundle) = refine_vote_task(44, vec![1, 2, 3], Vec::new());
        let (other_bundle, other_hash) = refine_bundle(45);
        let other_work = refine_task(44, 1, other_hash, &other_bundle);
        let other_candidate = prepare_candidate_envelope(
            &EmptyProtocolStateSource,
            [42; 32],
            0,
            &other_work,
            &other_bundle,
        )
        .unwrap()
        .envelope;
        task.candidate_report = other_candidate.canonical_report.into_inner();
        task.candidate_report_hash = other_candidate.canonical_report_hash;

        let submission = prepare_refine_backed_vote(
            &EmptyProtocolStateSource,
            2,
            &pair,
            [42; 32],
            0,
            &task,
            &bundle,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            submission.vote.verdict,
            Verdict::Oppose(OpposeReason::InvalidRefine)
        );
    }

    #[test]
    fn failed_independent_refine_cannot_create_support_vote() {
        let pair = sr25519::Pair::from_seed(&[3; 32]);
        let (task, bundle) = refine_vote_task(46, vec![1, 2, 3], Vec::new());

        let result = prepare_refine_backed_vote(
            &FailingProtocolStateSource,
            2,
            &pair,
            [42; 32],
            0,
            &task,
            &bundle,
        );

        assert!(matches!(result, Err(WorkerError::Chain(_))));
    }

    #[test]
    fn three_distinct_worker_identities_produce_and_verify_independently() {
        let producer = sr25519::Pair::from_seed(&[10; 32]);
        let verifier_one = sr25519::Pair::from_seed(&[11; 32]);
        let verifier_two = sr25519::Pair::from_seed(&[12; 32]);
        let (bundle, package_hash) = refine_bundle(50);
        let work = refine_task(50, 1, package_hash, &bundle);
        let candidate =
            prepare_candidate_envelope(&EmptyProtocolStateSource, [42; 32], 0, &work, &bundle)
                .unwrap();
        let signed_candidate =
            prepare_signed_candidate_submission(&producer, 0, [9; 32], candidate.envelope.clone());
        let task = VoteTask {
            work_id: 50,
            round: 1,
            assignment_epoch: 7,
            candidate_report_hash: candidate.envelope.canonical_report_hash,
            candidate_report: candidate.envelope.canonical_report.into_inner(),
            deadline: 100,
            assigned_workers: vec![0, 1, 2],
            submitted_votes: Vec::new(),
            package_hash,
            canonical_work_package: work.canonical_work_package,
            bundle_ref: work.bundle_ref,
        };

        let first = prepare_refine_backed_vote(
            &EmptyProtocolStateSource,
            1,
            &verifier_one,
            [42; 32],
            0,
            &task,
            &bundle,
        )
        .unwrap()
        .unwrap();
        let second = prepare_refine_backed_vote(
            &EmptyProtocolStateSource,
            2,
            &verifier_two,
            [42; 32],
            0,
            &task,
            &bundle,
        )
        .unwrap()
        .unwrap();

        assert_eq!(first.vote.verdict, Verdict::Support);
        assert_eq!(second.vote.verdict, Verdict::Support);
        assert_ne!(producer.public(), verifier_one.public());
        assert_ne!(verifier_one.public(), verifier_two.public());
        assert_ne!(first.signature, second.signature);
        assert_eq!(
            signed_candidate.envelope.canonical_report_hash,
            first.vote.candidate_report_hash
        );
    }

    #[test]
    fn runner_submits_refine_backed_vote_for_assigned_unsubmitted_worker() {
        let pair = sr25519::Pair::from_seed(&[1u8; 32]);
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let (assigned, bundle) = refine_vote_task(42, vec![3, 4, 5], vec![4]);
        let (unassigned, _) = refine_vote_task(43, vec![6, 7, 8], Vec::new());
        let fetcher = MemoryContentFetcher::new().with_content(&assigned.bundle_ref, bundle);
        let chain = TestVoteSubmitChainSource {
            vote_tasks: vec![assigned, unassigned],
            submitted: Arc::clone(&submitted),
        };
        let runner = WorkerRunner::stage0(chain, fetcher, 4096);
        let metrics = WorkerMetrics::new();

        let hashes =
            block_on(runner.submit_refine_votes(3, &pair, [42u8; 32], 0, Some(&metrics))).unwrap();

        assert_eq!(hashes, vec![[7u8; 32]]);
        assert_eq!(submitted.lock().unwrap().len(), 1);
        assert!(metrics
            .render_prometheus()
            .contains("minijam_worker_vote_tasks_seen_total 2"));
    }

    #[test]
    fn vote_restart_skips_worker_already_recorded_on_chain() {
        let pair = sr25519::Pair::from_seed(&[4; 32]);
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let (task, _) = refine_vote_task(47, vec![1, 2, 3], vec![2]);
        let chain = TestVoteSubmitChainSource {
            vote_tasks: vec![task],
            submitted: Arc::clone(&submitted),
        };
        let runner = WorkerRunner::stage0(chain, MemoryContentFetcher::new(), 4096);

        let hashes = block_on(runner.submit_refine_votes(2, &pair, [42; 32], 0, None)).unwrap();

        assert!(hashes.is_empty());
        assert!(submitted.lock().unwrap().is_empty());
    }

    #[test]
    fn missing_bundle_never_submits_support() {
        let pair = sr25519::Pair::from_seed(&[5; 32]);
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let (task, _) = refine_vote_task(48, vec![1, 2, 3], Vec::new());
        let chain = TestVoteSubmitChainSource {
            vote_tasks: vec![task],
            submitted: Arc::clone(&submitted),
        };
        let runner = WorkerRunner::stage0(chain, MemoryContentFetcher::new(), 4096);

        let result = block_on(runner.submit_refine_votes(2, &pair, [42; 32], 0, None));

        assert!(matches!(result, Err(WorkerError::Fetch(_))));
        assert!(submitted.lock().unwrap().is_empty());
    }

    #[test]
    fn runner_records_bundle_rejection_without_stopping_poll() {
        let (_, package_hash) = refine_bundle(7);
        let (bundle, _) = refine_bundle(8);
        let task = task(2, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle);
        let mut runner = WorkerRunner::new(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            MiniJamWorkBundleDecoder,
            1024,
        );

        assert_eq!(block_on(runner.poll_once()).unwrap(), 1);
        assert!(matches!(
            runner.status(task.work_id, task.round),
            Some(WorkerTaskStatus::BundleRejected {
                reason: WorkerError::Bundle(WorkBundleVerificationError::PackageHashMismatch)
            })
        ));
    }

    #[test]
    fn prepares_candidate_envelope_with_real_jambda_refine_report() {
        let (bundle, package_hash) = refine_bundle(9);
        let task = refine_task(77, 2, package_hash, &bundle);

        let prepared =
            prepare_candidate_envelope(&EmptyProtocolStateSource, [42u8; 32], 0, &task, &bundle)
                .unwrap();

        assert_eq!(prepared.envelope.work_id, 77);
        assert_eq!(prepared.envelope.assignment_round, 2);
        assert_eq!(prepared.envelope.chain_id, [42u8; 32]);
        assert_eq!(
            prepared.envelope.computed_report_hash(),
            prepared.envelope.canonical_report_hash
        );
        assert_eq!(
            prepared.envelope.projected_metadata.package_hash,
            package_hash
        );
        assert_eq!(prepared.exported_segment_count, 0);
    }

    #[test]
    fn candidate_refine_reads_state_at_package_lookup_anchor() {
        let (bundle, package_hash) = refine_bundle(9);
        let task = refine_task(77, 2, package_hash, &bundle);
        let source = TrackingProtocolStateSource::default();

        prepare_candidate_envelope(&source, [42u8; 32], 0, &task, &bundle).unwrap();
        let db = ProtocolStateDb::new(&source, [5u8; 32]);
        db.get(column::COL_STATE, &[7u8; 31]).unwrap();

        let anchors = source.anchors.lock().unwrap();
        assert!(!anchors.is_empty());
        assert!(anchors.iter().all(|anchor| *anchor == [5u8; 32]));
    }

    #[test]
    fn candidate_refine_rejects_non_finalized_lookup_anchor() {
        let (bundle, package_hash) = refine_bundle(9);
        let task = refine_task(77, 2, package_hash, &bundle);
        let source = TrackingProtocolStateSource {
            reject_anchor: true,
            ..Default::default()
        };

        let error = prepare_candidate_envelope(&source, [42u8; 32], 0, &task, &bundle).unwrap_err();

        assert!(matches!(error, WorkerError::Refine(message) if message.contains("not finalized")));
        assert!(source.anchors.lock().unwrap().is_empty());
    }

    #[test]
    fn candidate_refine_rejects_context_state_root_mismatch() {
        let (bundle, package_hash) = refine_bundle(9);
        let task = refine_task(77, 2, package_hash, &bundle);
        let source = TrackingProtocolStateSource {
            finalized_context: Some(stage0::FinalizedContextV1 {
                block_hash: [5u8; 32],
                block_number: 0,
                state_root: [8u8; 32],
                slot: 0,
            }),
            ..Default::default()
        };

        let error = prepare_candidate_envelope(&source, [42u8; 32], 0, &task, &bundle).unwrap_err();

        assert!(
            matches!(error, WorkerError::Refine(message) if message.contains("StateRootMismatch"))
        );
        assert!(source.anchors.lock().unwrap().is_empty());
    }

    #[test]
    fn candidate_refine_rejects_user_selected_authorization() {
        let (encoded, _) = refine_bundle(9);
        let mut raw = encoded.as_slice();
        let mut bundle = jambda_refine::MiniJamWorkBundleV1::decode(&mut raw).unwrap();
        bundle.work_package.authorization = ByteSequence::from(vec![1]);
        bundle.package_hash = bundle.work_package.jam_hash();
        let package_hash = bundle.package_hash.0;
        let encoded = bundle.encode();
        let task = refine_task(77, 2, package_hash, &encoded);

        let error = prepare_candidate_envelope(
            &EmptyProtocolStateSource,
            [42u8; 32],
            stage0::CORE_INDEX,
            &task,
            &encoded,
        )
        .unwrap_err();

        assert!(
            matches!(error, WorkerError::Refine(message) if message.contains("fixed Stage 0 allow-all"))
        );
    }

    #[test]
    fn prepares_signed_candidate_submission() {
        let (bundle, package_hash) = refine_bundle(9);
        let task = refine_task(77, 2, package_hash, &bundle);
        let prepared =
            prepare_candidate_envelope(&EmptyProtocolStateSource, [42u8; 32], 0, &task, &bundle)
                .unwrap();
        let pair = sr25519::Pair::from_seed(&[1u8; 32]);

        let signed =
            prepare_signed_candidate_submission(&pair, 3, [9u8; 32], prepared.envelope.clone());

        assert_eq!(signed.envelope, prepared.envelope);
        assert_eq!(signed.nonce, 3);
        assert!(signed.extrinsic_hex.starts_with("0x"));
        assert!(signed.extrinsic_hex.len() > prepared.envelope.canonical_report.len() * 2);
    }

    #[test]
    fn runner_submits_candidate_reports_from_jambda_refine() {
        let (bundle, package_hash) = refine_bundle(10);
        let task = refine_task(88, 0, package_hash, &bundle);
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let chain = TestCandidateSubmitChainSource {
            tasks: vec![task.clone()],
            submitted: Arc::clone(&submitted),
        };
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle);
        let runner = WorkerRunner::stage0(chain, fetcher, 1024);
        let metrics = WorkerMetrics::new();
        let pair = sr25519::Pair::from_seed(&[1u8; 32]);

        let skipped =
            block_on(runner.submit_candidate_reports(1, &pair, [42u8; 32], 0, Some(&metrics)))
                .unwrap();
        assert!(skipped.is_empty());
        assert!(submitted.lock().unwrap().is_empty());

        let hashes =
            block_on(runner.submit_candidate_reports(0, &pair, [42u8; 32], 0, Some(&metrics)))
                .unwrap();

        assert_eq!(hashes, vec![[8u8; 32]]);
        assert_eq!(submitted.lock().unwrap().len(), 1);
        assert!(metrics
            .render_prometheus()
            .contains("minijam_worker_bundle_ready_total 1"));
    }

    fn hex_encode_for_test(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::from("0x");
        for byte in bytes {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }

    #[test]
    fn recovery_db_round_trips_worker_statuses() {
        let tempdir = tempfile::tempdir().unwrap();
        let db = WorkerRecoveryDb::new(tempdir.path().join("worker-state.toml"));
        let mut statuses = BTreeMap::new();
        statuses.insert((1, 0), WorkerTaskStatus::BundleReady { bundle_len: 48 });
        statuses.insert(
            (2, 1),
            WorkerTaskStatus::BundleRejected {
                reason: WorkerError::Chain("rpc unavailable".into()),
            },
        );

        db.save_statuses(&statuses).unwrap();
        let loaded = db.load_statuses().unwrap();

        assert_eq!(
            loaded.get(&(1, 0)),
            Some(&WorkerTaskStatus::BundleReady { bundle_len: 48 })
        );
        assert!(matches!(
            loaded.get(&(2, 1)),
            Some(WorkerTaskStatus::BundleRejected {
                reason: WorkerError::Chain(reason)
            }) if reason.contains("rpc unavailable")
        ));
    }

    #[test]
    fn runner_can_resume_ready_bundle_statuses() {
        let (bundle, package_hash) = refine_bundle(7);
        let task = task(3, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle.clone());
        let mut statuses = BTreeMap::new();
        statuses.insert(
            (task.work_id, task.round),
            WorkerTaskStatus::BundleReady {
                bundle_len: bundle.len(),
            },
        );
        let mut runner = WorkerRunner::with_statuses(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            MiniJamWorkBundleDecoder,
            1024,
            statuses,
        );

        assert_eq!(block_on(runner.poll_once()).unwrap(), 0);
        assert_eq!(
            runner.status(task.work_id, task.round),
            Some(&WorkerTaskStatus::BundleReady {
                bundle_len: bundle.len()
            })
        );
    }

    #[test]
    fn runner_persists_statuses_after_poll() {
        let tempdir = tempfile::tempdir().unwrap();
        let db = WorkerRecoveryDb::new(tempdir.path().join("worker-state.toml"));
        let (bundle, package_hash) = refine_bundle(7);
        let task = task(4, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle.clone());
        let mut runner = WorkerRunner::new(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            MiniJamWorkBundleDecoder,
            1024,
        );

        assert_eq!(block_on(runner.poll_once_with_recovery(&db)).unwrap(), 1);
        assert_eq!(
            db.load_statuses().unwrap().get(&(task.work_id, task.round)),
            Some(&WorkerTaskStatus::BundleReady {
                bundle_len: bundle.len()
            })
        );
    }
}
