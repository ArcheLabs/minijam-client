// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bounded_collections::BoundedVec;
use cid::Cid;
use minijam_bulletin_api::{
    AccountId, Authorization, BulletinError, BulletinStore, ContentStatus, RenewalRef,
};
use minijam_protocol::{
    blake2_256, BlockNumber, CidConfig, ContentRef, Hash, HashingAlgorithm, StorageLocation,
    StorageReceipt,
};
use multihash::Multihash;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

const BLAKE2B_256_MULTIHASH: u64 = 0xb220;
const RAW_CODEC: u64 = 0x55;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fault {
    Missing,
    Corrupt,
    Timeout,
}

#[derive(Clone, Debug)]
struct StoredEntry {
    receipt: StorageReceipt,
}

#[derive(Default)]
struct State {
    block_number: BlockNumber,
    next_index: u32,
    authorizations: BTreeMap<AccountId, Authorization>,
    entries: BTreeMap<Hash, StoredEntry>,
    locations: BTreeMap<(BlockNumber, u32), Hash>,
    faults: BTreeMap<Hash, Fault>,
}

pub struct SimulatedBulletinStore {
    root: PathBuf,
    retention_period: BlockNumber,
    state: Mutex<State>,
}

impl SimulatedBulletinStore {
    pub fn new(
        root: impl Into<PathBuf>,
        retention_period: BlockNumber,
    ) -> Result<Self, BulletinError> {
        let root = root.into();
        fs::create_dir_all(root.join("blobs"))
            .map_err(|error| BulletinError::Io(error.to_string()))?;
        Ok(Self {
            root,
            retention_period,
            state: Mutex::new(State::default()),
        })
    }

    pub fn block_number(&self) -> BlockNumber {
        self.state
            .lock()
            .expect("bulletin mutex poisoned")
            .block_number
    }

    pub fn set_block_number(&self, block_number: BlockNumber) {
        self.state
            .lock()
            .expect("bulletin mutex poisoned")
            .block_number = block_number;
    }

    pub fn advance_blocks(&self, blocks: BlockNumber) {
        let mut state = self.state.lock().expect("bulletin mutex poisoned");
        state.block_number = state.block_number.saturating_add(blocks);
        state.next_index = 0;
    }

    pub fn authorize(
        &self,
        account: AccountId,
        transactions: u32,
        bytes: u64,
        expires_at: BlockNumber,
    ) {
        self.state
            .lock()
            .expect("bulletin mutex poisoned")
            .authorizations
            .insert(
                account,
                Authorization {
                    transactions_left: transactions,
                    bytes_left: bytes,
                    expires_at,
                },
            );
    }

    pub fn inject_fault(&self, content_hash: Hash, fault: Fault) {
        self.state
            .lock()
            .expect("bulletin mutex poisoned")
            .faults
            .insert(content_hash, fault);
    }

    pub fn clear_fault(&self, content_hash: &Hash) {
        self.state
            .lock()
            .expect("bulletin mutex poisoned")
            .faults
            .remove(content_hash);
    }

    fn blob_path(&self, hash: &Hash) -> PathBuf {
        let mut name = String::with_capacity(64);
        for byte in hash {
            use core::fmt::Write;
            let _ = write!(name, "{byte:02x}");
        }
        self.root.join("blobs").join(name)
    }

    fn write_blob_atomic(&self, hash: &Hash, data: &[u8]) -> Result<(), BulletinError> {
        let target = self.blob_path(hash);
        if target.exists() {
            return Ok(());
        }
        let temporary = target.with_extension("tmp");
        fs::write(&temporary, data).map_err(|error| BulletinError::Io(error.to_string()))?;
        fs::rename(&temporary, &target).map_err(|error| BulletinError::Io(error.to_string()))
    }

    fn read_blob(&self, hash: &Hash) -> Result<Vec<u8>, BulletinError> {
        fs::read(self.blob_path(hash)).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                BulletinError::Missing
            } else {
                BulletinError::Io(error.to_string())
            }
        })
    }

    fn cid_for(config: CidConfig, data: &[u8]) -> Result<(Hash, Vec<u8>), BulletinError> {
        if config.codec != RAW_CODEC || config.hashing != HashingAlgorithm::Blake2b256 {
            return Err(BulletinError::UnsupportedCid);
        }
        let content_hash = blake2_256(data);
        let multihash = Multihash::<64>::wrap(BLAKE2B_256_MULTIHASH, &content_hash)
            .map_err(|error| BulletinError::InvalidCid(error.to_string()))?;
        Ok((
            content_hash,
            Cid::new_v1(config.codec, multihash).to_bytes(),
        ))
    }

    fn locate_hash(state: &State, reference: RenewalRef) -> Option<Hash> {
        match reference {
            RenewalRef::ByContentHash(hash) => Some(hash),
            RenewalRef::ByLocation(location) => state
                .locations
                .get(&(location.block_number, location.transaction_index))
                .copied(),
        }
    }

    fn check_authorization(
        state: &mut State,
        account: &AccountId,
        bytes: u64,
    ) -> Result<(), BulletinError> {
        let authorization = state
            .authorizations
            .get_mut(account)
            .ok_or(BulletinError::Unauthorized)?;
        if state.block_number >= authorization.expires_at {
            return Err(BulletinError::AuthorizationExpired);
        }
        if authorization.transactions_left == 0 || authorization.bytes_left < bytes {
            return Err(BulletinError::QuotaExhausted);
        }
        authorization.transactions_left -= 1;
        authorization.bytes_left -= bytes;
        Ok(())
    }

    fn check_fault(state: &State, hash: &Hash) -> Result<(), BulletinError> {
        match state.faults.get(hash) {
            Some(Fault::Missing) => Err(BulletinError::Missing),
            Some(Fault::Corrupt) => Err(BulletinError::Corrupt),
            Some(Fault::Timeout) => Err(BulletinError::Timeout),
            None => Ok(()),
        }
    }
}

#[async_trait]
impl BulletinStore for SimulatedBulletinStore {
    async fn authorization(
        &self,
        account: &AccountId,
    ) -> Result<Option<Authorization>, BulletinError> {
        Ok(self
            .state
            .lock()
            .expect("bulletin mutex poisoned")
            .authorizations
            .get(account)
            .copied())
    }

    async fn store_with_cid_config(
        &self,
        account: &AccountId,
        config: CidConfig,
        data: &[u8],
    ) -> Result<StorageReceipt, BulletinError> {
        let (content_hash, cid) = Self::cid_for(config, data)?;
        let cid_v1 = BoundedVec::try_from(cid)
            .map_err(|_| BulletinError::InvalidCid("CID exceeds the protocol bound".into()))?;

        let (receipt, should_write) = {
            let mut state = self.state.lock().expect("bulletin mutex poisoned");
            Self::check_authorization(&mut state, account, data.len() as u64)?;
            let location = StorageLocation {
                block_number: state.block_number,
                transaction_index: state.next_index,
            };
            state.next_index = state.next_index.saturating_add(1);
            let receipt = StorageReceipt {
                content: ContentRef {
                    cid_v1,
                    content_hash,
                    size: data.len() as u64,
                },
                location,
                retention_until: state.block_number.saturating_add(self.retention_period),
            };
            state.locations.insert(
                (location.block_number, location.transaction_index),
                content_hash,
            );
            let should_write = !state.entries.contains_key(&content_hash);
            state.entries.insert(
                content_hash,
                StoredEntry {
                    receipt: receipt.clone(),
                },
            );
            (receipt, should_write)
        };

        if should_write {
            if let Err(error) = self.write_blob_atomic(&content_hash, data) {
                let mut state = self.state.lock().expect("bulletin mutex poisoned");
                state.entries.remove(&content_hash);
                state.locations.remove(&(
                    receipt.location.block_number,
                    receipt.location.transaction_index,
                ));
                return Err(error);
            }
        }
        Ok(receipt)
    }

    async fn fetch(&self, content: &ContentRef) -> Result<Vec<u8>, BulletinError> {
        {
            let state = self.state.lock().expect("bulletin mutex poisoned");
            Self::check_fault(&state, &content.content_hash)?;
            let entry = state
                .entries
                .get(&content.content_hash)
                .ok_or(BulletinError::Missing)?;
            if state.block_number >= entry.receipt.retention_until {
                return Err(BulletinError::Expired);
            }
        }
        let data = self.read_blob(&content.content_hash)?;
        let (hash, cid) = Self::cid_for(CidConfig::default(), &data)?;
        if hash != content.content_hash
            || data.len() as u64 != content.size
            || cid.as_slice() != content.cid_v1.as_slice()
        {
            return Err(BulletinError::Corrupt);
        }
        Ok(data)
    }

    async fn renew(
        &self,
        account: &AccountId,
        reference: RenewalRef,
    ) -> Result<StorageReceipt, BulletinError> {
        let mut state = self.state.lock().expect("bulletin mutex poisoned");
        Self::check_authorization(&mut state, account, 0)?;
        let hash = Self::locate_hash(&state, reference).ok_or(BulletinError::Missing)?;
        let block_number = state.block_number;
        let index = state.next_index;
        state.next_index = state.next_index.saturating_add(1);
        let entry = state.entries.get_mut(&hash).ok_or(BulletinError::Missing)?;
        entry.receipt.location = StorageLocation {
            block_number,
            transaction_index: index,
        };
        entry.receipt.retention_until = block_number.saturating_add(self.retention_period);
        let receipt = entry.receipt.clone();
        state.locations.insert((block_number, index), hash);
        Ok(receipt)
    }

    async fn status(&self, content: &ContentRef) -> Result<ContentStatus, BulletinError> {
        let state = self.state.lock().expect("bulletin mutex poisoned");
        if let Some(Fault::Missing) = state.faults.get(&content.content_hash) {
            return Ok(ContentStatus::Missing);
        }
        match state.entries.get(&content.content_hash) {
            None => Ok(ContentStatus::Missing),
            Some(entry) if state.block_number >= entry.receipt.retention_until => {
                Ok(ContentStatus::Expired)
            }
            Some(entry) => Ok(ContentStatus::Available {
                retention_until: entry.receipt.retention_until,
            }),
        }
    }
}

pub fn simulator_root_is_valid(path: &Path) -> bool {
    path.join("blobs").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use minijam_bulletin_api::BulletinStore;

    #[test]
    fn store_fetch_expire_and_renew() {
        block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let store = SimulatedBulletinStore::new(directory.path(), 10).unwrap();
            let account = [1u8; 32];
            store.authorize(account, 3, 1024, 100);

            let receipt = store.store(&account, b"mini-jam").await.unwrap();
            assert_eq!(store.fetch(&receipt.content).await.unwrap(), b"mini-jam");

            store.advance_blocks(10);
            assert!(matches!(
                store.fetch(&receipt.content).await,
                Err(BulletinError::Expired)
            ));

            let renewed = store
                .renew(
                    &account,
                    RenewalRef::ByContentHash(receipt.content.content_hash),
                )
                .await
                .unwrap();
            assert_eq!(store.fetch(&renewed.content).await.unwrap(), b"mini-jam");
        });
    }

    #[test]
    fn injected_corruption_is_observable() {
        block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let store = SimulatedBulletinStore::new(directory.path(), 10).unwrap();
            let account = [2u8; 32];
            store.authorize(account, 1, 1024, 100);
            let receipt = store.store(&account, b"payload").await.unwrap();
            store.inject_fault(receipt.content.content_hash, Fault::Corrupt);
            assert!(matches!(
                store.fetch(&receipt.content).await,
                Err(BulletinError::Corrupt)
            ));
        });
    }
}
