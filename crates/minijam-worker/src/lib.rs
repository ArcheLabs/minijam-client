// SPDX-License-Identifier: Apache-2.0

use core::time::Duration;
use std::collections::BTreeMap;

use minijam_protocol::{ContentRef, Hash, WorkId};
use minijam_worker_engine::{
    fetch::{fetch_verified_content, ContentFetcher, FetchError},
    verify_work_bundle, WorkBundleDecoder, WorkBundleVerificationError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerConfig {
    pub rpc_url: String,
    pub key: Option<String>,
    pub poll_interval: Duration,
    pub ipfs_gateway: String,
    pub request_timeout: Duration,
    pub max_bundle_bytes: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "ws://127.0.0.1:9944".into(),
            key: None,
            poll_interval: Duration::from_millis(1_000),
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    MissingRpcUrl,
    MissingIpfsGateway,
    ZeroPollInterval,
    ZeroRequestTimeout,
    ZeroMaxBundleBytes,
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

    pub fn status(&self, work_id: WorkId, round: u8) -> Option<&WorkerTaskStatus> {
        self.statuses.get(&(work_id, round))
    }

    pub fn statuses(&self) -> &BTreeMap<(WorkId, u8), WorkerTaskStatus> {
        &self.statuses
    }
}

impl<C, F, D> WorkerRunner<C, F, D>
where
    C: WorkerChainSource,
    F: ContentFetcher,
    D: WorkBundleDecoder,
{
    pub async fn poll_once(&mut self) -> Result<usize, WorkerError> {
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
                Ok(bundle_len) => WorkerTaskStatus::BundleReady { bundle_len },
                Err(error) => WorkerTaskStatus::BundleRejected { reason: error },
            };
            self.statuses.insert(key, status);
            processed = processed.saturating_add(1);
        }
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
    use minijam_worker_engine::{fetch::MemoryContentFetcher, WorkBundleDecodeError};

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

    struct TestBundleDecoder;

    impl WorkBundleDecoder for TestBundleDecoder {
        fn package_hash(&self, bytes: &[u8]) -> Result<Hash, WorkBundleDecodeError> {
            bytes
                .get(..32)
                .ok_or(WorkBundleDecodeError::InvalidEncoding)?
                .try_into()
                .map_err(|_| WorkBundleDecodeError::InvalidEncoding)
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
        let mut bundle = package_hash.to_vec();
        bundle.extend_from_slice(b"bundle");
        let task = task(1, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle.clone());
        let mut runner = WorkerRunner::new(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            TestBundleDecoder,
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
    fn runner_records_bundle_rejection_without_stopping_poll() {
        let package_hash = [7u8; 32];
        let mut bundle = [8u8; 32].to_vec();
        bundle.extend_from_slice(b"bundle");
        let task = task(2, 0, package_hash, &bundle);
        let fetcher = MemoryContentFetcher::new().with_content(&task.bundle_ref, bundle);
        let mut runner = WorkerRunner::new(
            TestChainSource {
                tasks: vec![task.clone()],
            },
            fetcher,
            TestBundleDecoder,
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
}
