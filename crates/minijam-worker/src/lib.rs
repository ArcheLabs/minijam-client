// SPDX-License-Identifier: Apache-2.0

#![feature(generic_const_exprs)]
#![allow(incomplete_features)]
#![recursion_limit = "4096"]

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use base64::Engine;
use jam_codec::{Decode as JamDecode, Encode as JamEncode};
use jambda_minijam_spec::MiniJamSpec;
use jambda_state_backend::StateBackend;
use jp_core_primitives::{
    error::DataBaseError,
    state::{column, ColumnFamily, StateKey, StoreChange, StoreOp},
    traits::DataBase,
};
use jp_vm_interp::InterpBackend;
use minijam_chain_client::MiniJamChainClient;
use minijam_protocol::{stage0, Hash};
use parity_scale_codec::Decode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sp_core::{sr25519, Pair};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerConfig {
    pub rpc_url: String,
    pub formal_rpc_url: String,
    pub key: Option<String>,
    pub core_index: u16,
    pub poll_interval: Duration,
    pub recovery_db_path: Option<PathBuf>,
    pub request_timeout: Duration,
    pub max_bundle_bytes: u64,
}
impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "ws://127.0.0.1:9944".into(),
            formal_rpc_url: "http://127.0.0.1:8090".into(),
            key: None,
            core_index: 0,
            poll_interval: Duration::from_secs(1),
            recovery_db_path: Some("worker-recovery.json".into()),
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
        if self.formal_rpc_url.trim().is_empty() {
            return Err(ConfigError::MissingFormalRpcUrl);
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
        let file: WorkerConfigFile = toml::from_str(input).map_err(ConfigFileError::Toml)?;
        let mut config = Self::default();
        if let Some(node) = file.node {
            if let Some(url) = node.rpc_url {
                config.rpc_url = url;
            }
        }
        if let Some(formal) = file.formal {
            if let Some(url) = formal.rpc_url {
                config.formal_rpc_url = url;
            }
        }
        if let Some(worker) = file.worker {
            if let Some(key) = worker.key {
                config.key = Some(key);
            }
            if let Some(core) = worker.core_index {
                config.core_index = core;
            }
            if let Some(ms) = worker.poll_interval_ms {
                config.poll_interval = Duration::from_millis(ms);
            }
            if let Some(path) = worker.recovery_db_path {
                config.recovery_db_path = Some(path);
            }
        }
        if let Some(content) = file.content {
            if let Some(timeout) = content.request_timeout_secs {
                config.request_timeout = Duration::from_secs(timeout);
            }
            if let Some(max) = content.max_bundle_bytes {
                config.max_bundle_bytes = max;
            }
        }
        Ok(config)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    MissingRpcUrl,
    MissingFormalRpcUrl,
    ZeroPollInterval,
    ZeroRequestTimeout,
    ZeroMaxBundleBytes,
}
#[derive(Debug)]
pub enum ConfigFileError {
    Toml(toml::de::Error),
}
impl std::fmt::Display for ConfigFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Toml(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ConfigFileError {}
#[derive(Clone, Debug, Default, Deserialize)]
struct WorkerConfigFile {
    node: Option<NodeConfigFile>,
    formal: Option<FormalConfigFile>,
    worker: Option<WorkerSectionConfigFile>,
    content: Option<ContentConfigFile>,
}
#[derive(Clone, Debug, Default, Deserialize)]
struct NodeConfigFile {
    rpc_url: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize)]
struct FormalConfigFile {
    rpc_url: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize)]
struct WorkerSectionConfigFile {
    key: Option<String>,
    core_index: Option<u16>,
    poll_interval_ms: Option<u64>,
    recovery_db_path: Option<PathBuf>,
}
#[derive(Clone, Debug, Default, Deserialize)]
struct ContentConfigFile {
    request_timeout_secs: Option<u64>,
    max_bundle_bytes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerTaskResponse {
    package_hash: String,
    bundle_base64: String,
    #[serde(rename = "context")]
    _context: Value,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerTask {
    pub package_hash: Hash,
    pub bundle_bytes: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("worker chain error: {0}")]
    Chain(String),
    #[error("worker HTTP error: {0}")]
    Http(String),
    #[error("worker refine error: {0}")]
    Refine(String),
    #[error("worker signing error: {0}")]
    Signing(String),
    #[error("worker recovery error: {0}")]
    Recovery(String),
}

#[derive(Default)]
pub struct WorkerMetrics {
    polls: AtomicU64,
    reports: AtomicU64,
    failures: AtomicU64,
}
impl WorkerMetrics {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn record_poll(&self) {
        self.polls.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_report(&self) {
        self.reports.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
    pub fn render_prometheus(&self) -> String {
        format!("minijam_worker_polls_total {}\nminijam_worker_reports_submitted_total {}\nminijam_worker_failures_total {}\n", self.polls.load(Ordering::Relaxed), self.reports.load(Ordering::Relaxed), self.failures.load(Ordering::Relaxed))
    }
}

#[derive(Default)]
pub struct WorkerHealth {
    ready: std::sync::atomic::AtomicBool,
}
impl WorkerHealth {
    pub fn set_ready(&self, value: bool) {
        self.ready.store(value, Ordering::Release);
    }
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

pub struct WorkerRunner {
    config: WorkerConfig,
    chain: Arc<MiniJamChainClient>,
    state_source: BlockingHttpStateSource,
    recovery: Option<WorkerRecoveryDb>,
    metrics: Arc<WorkerMetrics>,
}
impl WorkerRunner {
    pub async fn connect(
        config: WorkerConfig,
        metrics: Arc<WorkerMetrics>,
    ) -> Result<Self, WorkerError> {
        config
            .validate()
            .map_err(|error| WorkerError::Chain(format!("invalid worker config: {error:?}")))?;
        let key = config
            .key
            .clone()
            .ok_or_else(|| WorkerError::Signing("worker.key is required".into()))?;
        let signer = sr25519::Pair::from_string(&key, None)
            .map_err(|error| WorkerError::Signing(error.to_string()))?;
        let chain = Arc::new(
            MiniJamChainClient::connect(config.rpc_url.clone(), signer, config.request_timeout)
                .await
                .map_err(|error| WorkerError::Chain(error.to_string()))?,
        );
        let state_source = BlockingHttpStateSource::new(node_http_url(&config.rpc_url));
        let recovery = config.recovery_db_path.clone().map(WorkerRecoveryDb::new);
        Ok(Self {
            config,
            chain,
            state_source,
            recovery,
            metrics,
        })
    }
    pub async fn poll_once(&mut self) -> Result<bool, WorkerError> {
        self.metrics.record_poll();
        let Some(task) =
            fetch_task(&self.config.formal_rpc_url, self.config.request_timeout).await?
        else {
            return Ok(false);
        };
        if let Some(recovery) = &self.recovery {
            if recovery.contains(task.package_hash)? {
                return Ok(false);
            }
        }
        let package_hash = task.package_hash;
        let source = self.state_source.clone();
        let max_bundle = self.config.max_bundle_bytes;
        let bundle = task.bundle_bytes.clone();
        let report = tokio::task::spawn_blocking(move || {
            refine_bundle(&source, package_hash, bundle, max_bundle)
        })
        .await
        .map_err(|error| WorkerError::Refine(error.to_string()))??;
        self.chain
            .submit_report(report, package_hash)
            .await
            .map_err(|error| WorkerError::Chain(error.to_string()))?;
        if let Some(recovery) = &self.recovery {
            recovery.mark(package_hash)?;
        }
        notify_report_submitted(
            &self.config.formal_rpc_url,
            package_hash,
            self.config.request_timeout,
        )
        .await?;
        self.metrics.record_report();
        Ok(true)
    }
}

async fn fetch_task(base: &str, timeout: Duration) -> Result<Option<WorkerTask>, WorkerError> {
    let url = format!("{}/worker/v1/task", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    if response.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(WorkerError::Http(format!(
            "task endpoint returned {}",
            response.status()
        )));
    }
    let response: WorkerTaskResponse = response
        .json()
        .await
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    let package_hash = decode_hash(&response.package_hash)?;
    let bundle_bytes = base64::engine::general_purpose::STANDARD
        .decode(response.bundle_base64)
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    Ok(Some(WorkerTask {
        package_hash,
        bundle_bytes,
    }))
}

async fn notify_report_submitted(
    base: &str,
    package_hash: Hash,
    timeout: Duration,
) -> Result<(), WorkerError> {
    let url = format!("{}/worker/v1/report-submitted", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    let response = client
        .post(url)
        .json(&json!({"packageHash": hex(&package_hash)}))
        .send()
        .await
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(WorkerError::Http(format!(
            "report callback returned {}",
            response.status()
        )))
    }
}

fn refine_bundle(
    source: &BlockingHttpStateSource,
    package_hash: Hash,
    bundle_bytes: Vec<u8>,
    max_bundle: u64,
) -> Result<Vec<u8>, WorkerError> {
    if bundle_bytes.len() as u64 > max_bundle {
        return Err(WorkerError::Refine(
            "bundle exceeds configured limit".into(),
        ));
    }
    let mut raw = bundle_bytes.as_slice();
    let bundle = jambda_refine::MiniJamWorkBundleV1::decode(&mut raw)
        .map_err(|error| WorkerError::Refine(format!("invalid work bundle: {error}")))?;
    if !raw.is_empty() || !bundle.package_hash_matches() || bundle.package_hash.0 != package_hash {
        return Err(WorkerError::Refine("bundle package hash mismatch".into()));
    }
    if bundle.work_package.auth_code_host != stage0::AUTH_CODE_HOST
        || bundle.work_package.auth_code_hash.0 != stage0::AUTH_CODE_HASH
        || !bundle.work_package.authorization.is_empty()
        || !bundle.work_package.authorizer_config.is_empty()
    {
        return Err(WorkerError::Refine(
            "bundle does not use fixed Stage 0 authorization".into(),
        ));
    }
    source.validate_finalized_context(&bundle.work_package.context)?;
    let lookup_anchor = bundle.work_package.context.lookup_anchor.0;
    let input = bundle.into_work_report_input(stage0::CORE_INDEX);
    let db = ProtocolStateDb::new(source, lookup_anchor);
    let mut backend = StateBackend::<MiniJamSpec, _>::new(db);
    backend
        .load_from_db()
        .map_err(|error| WorkerError::Refine(format!("failed to load Jambda state: {error:?}")))?;
    let output = jambda_refine::compute_work_report::<
        MiniJamSpec,
        ProtocolStateDb<'_, BlockingHttpStateSource>,
        StateBackend<MiniJamSpec, ProtocolStateDb<'_, BlockingHttpStateSource>>,
        InterpBackend,
        jp_vm_engine::InnerEngine<InterpBackend>,
    >(&backend, input, InterpBackend)
    .map_err(|error| WorkerError::Refine(format!("Jambda refine failed: {error:?}")))?;
    let report = output.report.encode();
    let projection = jambda_minijam_executive::MiniJamExecutive::project_report(&report)
        .map_err(|error| WorkerError::Refine(format!("generated report is invalid: {error:?}")))?;
    if projection.package_hash != package_hash {
        return Err(WorkerError::Refine(
            "generated report package hash mismatch".into(),
        ));
    }
    Ok(report)
}

#[derive(Clone, Debug)]
pub struct BlockingHttpStateSource {
    rpc_url: String,
}
impl BlockingHttpStateSource {
    pub fn new(rpc_url: String) -> Self {
        Self { rpc_url }
    }
}
impl BlockingHttpStateSource {
    fn rpc(&self, method: &str, params: Value) -> Result<Value, WorkerError> {
        let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let raw = http_post_json(&self.rpc_url, &body.to_string())?;
        let value: Value =
            serde_json::from_str(&raw).map_err(|error| WorkerError::Http(error.to_string()))?;
        if let Some(error) = value.get("error") {
            return Err(WorkerError::Chain(error.to_string()));
        }
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    }
    fn protocol_state(
        &self,
        block_hash: Hash,
        key: [u8; 31],
    ) -> Result<Option<Vec<u8>>, WorkerError> {
        let value = self.rpc(
            "minijam_getProtocolStateAt",
            json!([hex(&block_hash), hex(&key)]),
        )?;
        let Some(encoded) = value.as_str() else {
            return Ok(None);
        };
        let bytes = decode_hex(encoded)?;
        let state = minijam_protocol::StateValue::decode(&mut bytes.as_slice())
            .map_err(|error| WorkerError::Chain(error.to_string()))?;
        Ok(Some(state.into_inner()))
    }
    fn validate_finalized_context(
        &self,
        context: &jp_core_primitives::work::RefineContext,
    ) -> Result<(), WorkerError> {
        // Formal RPC pins this context when it creates the active package. It
        // may be older than the node's current finalized head by the time the
        // Worker polls, so validate the invariant fields here and read state
        // at the pinned anchor below instead of racing the moving head.
        if context.anchor.0 != context.lookup_anchor.0 || context.lookup_anchor_slot.0 == 0 {
            return Err(WorkerError::Refine(
                "work bundle context is not a valid fixed finalized context".into(),
            ));
        }
        Ok(())
    }
}

type PendingStateWrites = BTreeMap<(ColumnFamily, Vec<u8>), Vec<u8>>;
struct ProtocolStateDb<'a, S> {
    source: &'a S,
    block_hash: Hash,
    writes: Mutex<PendingStateWrites>,
    deletes: Mutex<BTreeMap<(ColumnFamily, Vec<u8>), ()>>,
}
impl<'a, S> ProtocolStateDb<'a, S> {
    fn new(source: &'a S, block_hash: Hash) -> Self {
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
impl DataBase for ProtocolStateDb<'_, BlockingHttpStateSource> {
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
        let mut key31 = [0u8; 31];
        key31.copy_from_slice(key);
        self.source
            .protocol_state(self.block_hash, key31)
            .map_err(|error| DataBaseError::Other(error.to_string()))
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
                StoreOp::Upsert | StoreOp::Update => self.put(
                    change.col(),
                    &key,
                    change.value.clone().ok_or(DataBaseError::NotFound)?,
                )?,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct RecoveryFile {
    packages: Vec<Hash>,
}
#[derive(Clone, Debug)]
pub struct WorkerRecoveryDb {
    path: PathBuf,
}
impl WorkerRecoveryDb {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn contains(&self, package: Hash) -> Result<bool, WorkerError> {
        Ok(self.load()?.packages.contains(&package))
    }
    pub fn mark(&self, package: Hash) -> Result<(), WorkerError> {
        let mut file = self.load()?;
        if !file.packages.contains(&package) {
            file.packages.push(package);
        }
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| WorkerError::Recovery(error.to_string()))?;
        }
        let temp = self.path.with_extension("tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(&file)
                .map_err(|error| WorkerError::Recovery(error.to_string()))?,
        )
        .and_then(|_| std::fs::rename(&temp, &self.path))
        .map_err(|error| WorkerError::Recovery(error.to_string()))
    }
    fn load(&self) -> Result<RecoveryFile, WorkerError> {
        if !self.path.exists() {
            return Ok(RecoveryFile::default());
        }
        let bytes =
            std::fs::read(&self.path).map_err(|error| WorkerError::Recovery(error.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|error| WorkerError::Recovery(error.to_string()))
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn node_http_url(url: &str) -> String {
    url.replace("ws://", "http://")
        .replace("wss://", "https://")
}
fn http_post_json(url: &str, body: &str) -> Result<String, WorkerError> {
    let endpoint = HttpEndpoint::parse(url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    write!(stream, "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", endpoint.path, endpoint.host, body.len(), body).map_err(|error| WorkerError::Http(error.to_string()))?;
    read_http_body(stream)
}
fn read_http_body(mut stream: TcpStream) -> Result<String, WorkerError> {
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    let separator = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| WorkerError::Http("HTTP response missing body separator".into()))?;
    let headers = String::from_utf8_lossy(&bytes[..separator]);
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("500");
    if !status.starts_with('2') {
        return Err(WorkerError::Http(format!("HTTP request returned {status}")));
    }
    String::from_utf8(bytes[separator + 4..].to_vec())
        .map_err(|error| WorkerError::Http(error.to_string()))
}
#[derive(Clone, Debug)]
struct HttpEndpoint {
    host: String,
    port: u16,
    path: String,
}
impl HttpEndpoint {
    fn parse(url: &str) -> Result<Self, WorkerError> {
        let stripped = url
            .strip_prefix("http://")
            .ok_or_else(|| WorkerError::Http("worker HTTP endpoint must use http://".into()))?;
        let (authority, path) = stripped
            .split_once('/')
            .map(|(host, path)| (host, format!("/{path}")))
            .unwrap_or((stripped, "/".into()));
        let (host, port) = authority
            .rsplit_once(':')
            .map(|(host, port)| (host.to_string(), port.parse().unwrap_or(80)))
            .unwrap_or((authority.into(), 80));
        Ok(Self { host, port, path })
    }
}
fn decode_hash(input: &str) -> Result<Hash, WorkerError> {
    let bytes = decode_hex(input)?;
    bytes
        .try_into()
        .map_err(|_| WorkerError::Chain("expected 32-byte hash".into()))
}
fn decode_hex(input: &str) -> Result<Vec<u8>, WorkerError> {
    let input = input.strip_prefix("0x").unwrap_or(input);
    if !input.len().is_multiple_of(2) {
        return Err(WorkerError::Chain("odd-length hex".into()));
    }
    (0..input.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&input[index..index + 2], 16)
                .map_err(|error| WorkerError::Chain(error.to_string()))
        })
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    let mut output = String::from("0x");
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
