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
        atomic::{AtomicU64, Ordering},
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
    BulletinEvidence, ContentRef, Hash, ReportEnvelopeV1, Verdict, WorkId, WorkerId, WorkerVoteV1,
    PROTOCOL_VERSION_V1,
};
use minijam_worker_engine::{
    fetch::{fetch_verified_content, ContentFetcher, FetchError, HttpBytesClient},
    verify_work_bundle, MiniJamWorkBundleDecoder, WorkBundleDecoder, WorkBundleVerificationError,
};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sp_core::{sr25519, Pair, H256};
use sp_runtime::{generic::Era, traits::IdentifyAccount, MultiSigner};
use std::sync::Mutex;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerConfig {
    pub rpc_url: String,
    pub key: Option<String>,
    pub worker_id: Option<WorkerId>,
    pub chain_id: Hash,
    pub core_index: u16,
    pub submit_candidates: bool,
    pub submit_support_votes: bool,
    pub poll_interval: Duration,
    pub recovery_db_path: Option<PathBuf>,
    pub metrics_bind: Option<String>,
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
            core_index: 0,
            submit_candidates: false,
            submit_support_votes: false,
            poll_interval: Duration::from_millis(1_000),
            recovery_db_path: None,
            metrics_bind: None,
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
        file.apply_to(&mut config);
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
}

impl core::fmt::Display for ConfigFileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Toml(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ConfigFileError {}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct WorkerConfigFile {
    node: Option<NodeConfigFile>,
    worker: Option<WorkerSectionConfigFile>,
    metrics: Option<MetricsConfigFile>,
    content: Option<ContentConfigFile>,
}

impl WorkerConfigFile {
    fn parse(input: &str) -> Result<Self, ConfigFileError> {
        toml::from_str(input).map_err(ConfigFileError::Toml)
    }

    fn apply_to(self, config: &mut WorkerConfig) {
        if let Some(node) = self.node {
            if let Some(rpc_url) = node.rpc_url {
                config.rpc_url = rpc_url;
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
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct NodeConfigFile {
    rpc_url: Option<String>,
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
struct ContentConfigFile {
    ipfs_gateway: Option<String>,
    request_timeout_secs: Option<u64>,
    max_bundle_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkTask {
    pub work_id: WorkId,
    pub round: u8,
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
    pub deadline: u32,
    pub assigned_workers: Vec<WorkerId>,
    pub submitted_votes: Vec<WorkerId>,
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
    fn protocol_state_value(&self, key: [u8; 31]) -> Result<Option<Vec<u8>>, WorkerError>;
}

pub trait WorkerSignedTxContext {
    fn account_nonce(&self, account: [u8; 32]) -> Result<minijam_runtime::Nonce, WorkerError>;

    fn genesis_hash(&self) -> Result<Hash, WorkerError>;
}

impl ProtocolStateSource for BlockingHttpWorkerChainSource {
    fn protocol_state_value(&self, key: [u8; 31]) -> Result<Option<Vec<u8>>, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "minijam_getProtocolState",
                "params": [hex_encode(&key)],
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

impl WorkerSignedTxContext for BlockingHttpWorkerChainSource {
    fn account_nonce(&self, account: [u8; 32]) -> Result<minijam_runtime::Nonce, WorkerError> {
        let response = http_post_json(
            &self.rpc_url,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "system_accountNextIndex",
                "params": [hex_encode(&account)],
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
    writes: Mutex<BTreeMap<(ColumnFamily, Vec<u8>), Vec<u8>>>,
    deletes: Mutex<BTreeMap<(ColumnFamily, Vec<u8>), ()>>,
}

impl<'a, S> ProtocolStateDb<'a, S> {
    fn new(source: &'a S) -> Self {
        Self {
            source,
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
            .protocol_state_value(state_key)
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
            package_hash: task.package_hash,
            canonical_work_package: task.canonical_work_package.into_inner(),
            bundle_ref: task.bundle_ref,
        })
        .collect())
}

pub fn decode_open_vote_tasks_response(encoded: &str) -> Result<Vec<VoteTask>, WorkerError> {
    let bytes = decode_hex(encoded)?;
    let tasks = Vec::<minijam_protocol::WorkerVoteTaskV1>::decode(&mut bytes.as_slice())
        .map_err(|error| WorkerError::Chain(format!("invalid open vote task batch: {error}")))?;
    Ok(tasks
        .into_iter()
        .map(|task| VoteTask {
            work_id: task.work_id,
            round: task.round,
            assignment_epoch: task.assignment_epoch,
            candidate_report_hash: task.candidate_report_hash,
            deadline: task.deadline,
            assigned_workers: task.assigned_workers.into_inner(),
            submitted_votes: task.submitted_votes.into_inner(),
        })
        .collect())
}

pub fn sr25519_pair_from_uri(uri: &str) -> Result<sr25519::Pair, WorkerError> {
    sr25519::Pair::from_string(uri, None)
        .map_err(|error| WorkerError::Signing(format!("invalid sr25519 key URI: {error}")))
}

pub fn prepare_support_vote_submission(
    worker_id: WorkerId,
    pair: &sr25519::Pair,
    chain_id: Hash,
    task: &VoteTask,
) -> Option<PreparedVoteSubmission> {
    if !task.assigned_workers.contains(&worker_id) || task.submitted_votes.contains(&worker_id) {
        return None;
    }
    let vote = WorkerVoteV1 {
        work_id: task.work_id,
        round: task.round,
        assignment_epoch: task.assignment_epoch,
        candidate_report_hash: task.candidate_report_hash,
        verdict: Verdict::Support,
        deadline: task.deadline,
        chain_id,
        protocol_version: PROTOCOL_VERSION_V1,
    };
    Some(prepare_vote_submission(worker_id, pair, vote))
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

    let input = bundle.into_work_report_input(core_index);
    let db = ProtocolStateDb::new(state);
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
    let tx_ext = signed_tx_extension(nonce);
    let genesis_hash = H256::from(genesis_hash);
    let raw_payload = minijam_runtime::SignedPayload::from_raw(
        call.clone(),
        tx_ext.clone(),
        (
            (),
            (),
            minijam_runtime::VERSION.spec_version,
            minijam_runtime::VERSION.transaction_version,
            genesis_hash,
            genesis_hash,
            (),
            (),
            (),
            (),
        ),
    );
    let signature = raw_payload.using_encoded(|payload| pair.sign(payload));
    let signer = MultiSigner::Sr25519(pair.public()).into_account();
    let extrinsic = minijam_runtime::UncheckedExtrinsic::new_signed(
        call,
        minijam_runtime::Address::Id(signer),
        minijam_runtime::Signature::Sr25519(signature),
        tx_ext,
    );
    PreparedSignedCandidateSubmission {
        envelope,
        nonce,
        extrinsic_hex: hex_encode(&extrinsic.encode()),
    }
}

fn signed_tx_extension(nonce: minijam_runtime::Nonce) -> minijam_runtime::TxExtension {
    (
        frame_system::AuthorizeCall::<minijam_runtime::Runtime>::new(),
        frame_system::CheckNonZeroSender::<minijam_runtime::Runtime>::new(),
        frame_system::CheckSpecVersion::<minijam_runtime::Runtime>::new(),
        frame_system::CheckTxVersion::<minijam_runtime::Runtime>::new(),
        frame_system::CheckGenesis::<minijam_runtime::Runtime>::new(),
        frame_system::CheckEra::<minijam_runtime::Runtime>::from(Era::Immortal),
        frame_system::CheckNonce::<minijam_runtime::Runtime>::from(nonce),
        frame_system::CheckWeight::<minijam_runtime::Runtime>::new(),
        pallet_transaction_payment::ChargeTransactionPayment::<minijam_runtime::Runtime>::from(0),
        frame_system::WeightReclaim::<minijam_runtime::Runtime>::new(),
    )
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
    if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
        return Err(HttpError(format!(
            "HTTP request failed: {}",
            headers.lines().next().unwrap_or_else(|| headers.as_ref())
        )));
    }
    Ok(response[(separator + 4)..].to_vec())
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
    C: WorkerChainSource + WorkerTxSubmitter,
{
    pub async fn submit_support_votes(
        &self,
        worker_id: WorkerId,
        pair: &sr25519::Pair,
        chain_id: Hash,
        metrics: Option<&WorkerMetrics>,
    ) -> Result<Vec<Hash>, WorkerError> {
        let tasks = self.chain.open_vote_tasks().await?;
        if let Some(metrics) = metrics {
            metrics.record_vote_tasks_seen(tasks.len());
        }
        let mut tx_hashes = Vec::new();
        for task in tasks {
            let Some(submission) =
                prepare_support_vote_submission(worker_id, pair, chain_id, &task)
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
    use minijam_protocol::WorkerVoteTaskV1;
    use minijam_worker_engine::{fetch::MemoryContentFetcher, MiniJamWorkBundleDecoder};
    use parity_scale_codec::Encode;
    use std::sync::Mutex;

    #[test]
    fn default_worker_config_is_valid() {
        WorkerConfig::default().validate().unwrap();
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
        fn protocol_state_value(&self, _key: [u8; 31]) -> Result<Option<Vec<u8>>, WorkerError> {
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
        fn protocol_state_value(&self, _key: [u8; 31]) -> Result<Option<Vec<u8>>, WorkerError> {
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
            auth_code_hash: OpaqueHash([seed; 32]),
            context: RefineContext {
                anchor: OpaqueHash([2u8; 32]),
                state_root: OpaqueHash([3u8; 32]),
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
        let runner = WorkerRunner::stage0(
            TestVoteChainSource {
                vote_tasks: vec![VoteTask {
                    work_id: 42,
                    round: 1,
                    assignment_epoch: 7,
                    candidate_report_hash: [9u8; 32],
                    deadline: 100,
                    assigned_workers: vec![0, 1, 2],
                    submitted_votes: vec![1],
                }],
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
                package_hash,
                canonical_work_package: vec![1, 2, 3],
                bundle_ref,
            }]
        );
    }

    #[test]
    fn decodes_open_vote_tasks_rpc_response() {
        let tasks = vec![WorkerVoteTaskV1 {
            work_id: 42,
            round: 1,
            assignment_epoch: 7,
            candidate_report_hash: [9u8; 32],
            deadline: 100,
            assigned_workers: vec![0, 2, 4].try_into().unwrap(),
            submitted_votes: vec![2].try_into().unwrap(),
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
                deadline: 100,
                assigned_workers: vec![0, 2, 4],
                submitted_votes: vec![2],
            }]
        );
    }

    #[test]
    fn prepares_support_vote_submission_with_session_signature_and_unsigned_extrinsic() {
        let pair = sr25519::Pair::from_seed(&[1u8; 32]);
        let task = VoteTask {
            work_id: 42,
            round: 1,
            assignment_epoch: 7,
            candidate_report_hash: [9u8; 32],
            deadline: 100,
            assigned_workers: vec![3, 4, 5],
            submitted_votes: vec![4],
        };

        let submission = prepare_support_vote_submission(3, &pair, [42u8; 32], &task).unwrap();

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
        assert!(prepare_support_vote_submission(4, &pair, [42u8; 32], &task).is_none());
        assert!(prepare_support_vote_submission(9, &pair, [42u8; 32], &task).is_none());
    }

    #[test]
    fn runner_submits_support_votes_for_assigned_unsubmitted_worker() {
        let pair = sr25519::Pair::from_seed(&[1u8; 32]);
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let chain = TestVoteSubmitChainSource {
            vote_tasks: vec![
                VoteTask {
                    work_id: 42,
                    round: 1,
                    assignment_epoch: 7,
                    candidate_report_hash: [9u8; 32],
                    deadline: 100,
                    assigned_workers: vec![3, 4, 5],
                    submitted_votes: vec![4],
                },
                VoteTask {
                    work_id: 43,
                    round: 1,
                    assignment_epoch: 7,
                    candidate_report_hash: [8u8; 32],
                    deadline: 100,
                    assigned_workers: vec![6, 7, 8],
                    submitted_votes: Vec::new(),
                },
            ],
            submitted: Arc::clone(&submitted),
        };
        let runner = WorkerRunner::stage0(chain, MemoryContentFetcher::new(), 64);
        let metrics = WorkerMetrics::new();

        let hashes =
            block_on(runner.submit_support_votes(3, &pair, [42u8; 32], Some(&metrics))).unwrap();

        assert_eq!(hashes, vec![[7u8; 32]]);
        assert_eq!(submitted.lock().unwrap().len(), 1);
        assert!(metrics
            .render_prometheus()
            .contains("minijam_worker_vote_tasks_seen_total 2"));
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

        let hashes =
            block_on(runner.submit_candidate_reports(&pair, [42u8; 32], 0, Some(&metrics)))
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
