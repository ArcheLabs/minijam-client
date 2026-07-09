// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use minijam_protocol::{BlockNumber, CidConfig, ContentRef, Hash, StorageLocation, StorageReceipt};

pub type AccountId = [u8; 32];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Authorization {
    pub transactions_left: u32,
    pub bytes_left: u64,
    pub expires_at: BlockNumber,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenewalRef {
    ByLocation(StorageLocation),
    ByContentHash(Hash),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentStatus {
    Available { retention_until: BlockNumber },
    Expired,
    Missing,
}

#[derive(Debug, thiserror::Error)]
pub enum BulletinError {
    #[error("account is not authorized")]
    Unauthorized,
    #[error("authorization quota is exhausted")]
    QuotaExhausted,
    #[error("authorization has expired")]
    AuthorizationExpired,
    #[error("content is missing")]
    Missing,
    #[error("content has expired")]
    Expired,
    #[error("content failed its hash or length check")]
    Corrupt,
    #[error("unsupported CID configuration")]
    UnsupportedCid,
    #[error("simulated request timed out")]
    Timeout,
    #[error("storage I/O failed: {0}")]
    Io(String),
    #[error("invalid CID: {0}")]
    InvalidCid(String),
}

#[async_trait]
pub trait BulletinStore: Send + Sync {
    async fn authorization(
        &self,
        account: &AccountId,
    ) -> Result<Option<Authorization>, BulletinError>;

    async fn store(
        &self,
        account: &AccountId,
        data: &[u8],
    ) -> Result<StorageReceipt, BulletinError> {
        self.store_with_cid_config(account, CidConfig::default(), data)
            .await
    }

    async fn store_with_cid_config(
        &self,
        account: &AccountId,
        config: CidConfig,
        data: &[u8],
    ) -> Result<StorageReceipt, BulletinError>;

    async fn fetch(&self, content: &ContentRef) -> Result<Vec<u8>, BulletinError>;

    async fn renew(
        &self,
        account: &AccountId,
        reference: RenewalRef,
    ) -> Result<StorageReceipt, BulletinError>;

    async fn status(&self, content: &ContentRef) -> Result<ContentStatus, BulletinError>;
}
