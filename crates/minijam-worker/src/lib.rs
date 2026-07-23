// SPDX-License-Identifier: Apache-2.0

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

use minijam_protocol::{ContentRef, Hash, WorkId};
use minijam_worker_engine::{
    fetch::{fetch_verified_content, ContentFetcher, FetchError, HttpBytesClient},
    verify_work_bundle, MiniJamWorkBundleDecoder, WorkBundleDecoder, WorkBundleVerificationError,
};
use parity_scale_codec::Decode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerConfig {
    pub rpc_url: String,
    pub key: Option<String>,
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
pub enum WorkerTaskStatus {
    BundleReady { bundle_len: usize },
    BundleRejected { reason: WorkerError },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerError {
    Chain(String),
    Fetch(FetchError),
    Bundle(WorkBundleVerificationError),
}

#[derive(Debug, Default)]
pub struct WorkerMetrics {
    polls_total: AtomicU64,
    tasks_processed_total: AtomicU64,
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
                "# HELP minijam_worker_bundle_ready_total Bundles fetched and verified successfully.\n",
                "# TYPE minijam_worker_bundle_ready_total counter\n",
                "minijam_worker_bundle_ready_total {}\n",
                "# HELP minijam_worker_bundle_rejected_total Bundles rejected during fetch or verification.\n",
                "# TYPE minijam_worker_bundle_rejected_total counter\n",
                "minijam_worker_bundle_rejected_total {}\n"
            ),
            self.polls_total.load(Ordering::Relaxed),
            self.tasks_processed_total.load(Ordering::Relaxed),
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use minijam_protocol::blake2_256;
    use minijam_protocol::MiniJamWorkBundleV1;
    use minijam_worker_engine::{fetch::MemoryContentFetcher, MiniJamWorkBundleDecoder};
    use parity_scale_codec::Encode;

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

    #[async_trait::async_trait]
    impl WorkerChainSource for TestChainSource {
        async fn pending_work_tasks(&self) -> Result<Vec<WorkTask>, WorkerError> {
            Ok(self.tasks.clone())
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

    #[test]
    fn runner_fetches_and_verifies_pending_work_bundles() {
        let package_hash = [7u8; 32];
        let bundle = MiniJamWorkBundleV1::new(package_hash).encode();
        let task = task(1, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle.clone());
        let mut runner = WorkerRunner::stage0(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            64,
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
        let package_hash = [7u8; 32];
        let good_bundle = MiniJamWorkBundleV1::new(package_hash).encode();
        let bad_bundle = MiniJamWorkBundleV1::new([8u8; 32]).encode();
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
            64,
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
    fn runner_records_bundle_rejection_without_stopping_poll() {
        let package_hash = [7u8; 32];
        let bundle = MiniJamWorkBundleV1::new([8u8; 32]).encode();
        let task = task(2, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle);
        let mut runner = WorkerRunner::new(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            MiniJamWorkBundleDecoder,
            64,
        );

        assert_eq!(block_on(runner.poll_once()).unwrap(), 1);
        assert!(matches!(
            runner.status(task.work_id, task.round),
            Some(WorkerTaskStatus::BundleRejected {
                reason: WorkerError::Bundle(WorkBundleVerificationError::PackageHashMismatch)
            })
        ));
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
        let package_hash = [7u8; 32];
        let bundle = MiniJamWorkBundleV1::new(package_hash).encode();
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
            64,
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
        let package_hash = [7u8; 32];
        let bundle = MiniJamWorkBundleV1::new(package_hash).encode();
        let task = task(4, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle.clone());
        let mut runner = WorkerRunner::new(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            MiniJamWorkBundleDecoder,
            64,
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
