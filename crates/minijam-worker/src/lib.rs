// SPDX-License-Identifier: Apache-2.0

use core::time::Duration;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
