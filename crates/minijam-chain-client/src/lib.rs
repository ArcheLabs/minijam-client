// SPDX-License-Identifier: Apache-2.0

mod events;
mod extrinsic;
mod rpc;

pub use events::{FinalityObservation, FinalizedEvent};
pub use extrinsic::sign_call as sign_runtime_call;
pub use rpc::{DispatchOutcome, FinalizedContext};

use std::time::Duration;

use jam_codec::Decode as JamDecode;
use jp_core_primitives::types::ServiceInfo;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use minijam_protocol::{
    CanonicalPreimageBytes, ContentRef, Hash, StateValue, SystemCommandV2, WorkId,
};
use minijam_runtime::RuntimeCall;
use parity_scale_codec::{Decode, Encode};
use sp_core::{
    crypto::{AccountId32, Ss58Codec},
    sr25519, Pair,
};
use sp_runtime::traits::IdentifyAccount;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChainClientError {
    #[error("RPC unavailable: {0}")]
    Rpc(String),
    #[error("chain dispatch rejected: {0}")]
    Dispatch(String),
    #[error("invalid chain response: {0}")]
    Decode(String),
    #[error("transaction terminal failure: {0}")]
    TransactionFailed(String),
    #[error("input exceeds runtime bounds")]
    InputTooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Submission {
    pub extrinsic_hash: Hash,
    pub submitted_nonce: u32,
    pub correlation: Hash,
    pub lifecycle: Option<TransactionLifecycle>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionLifecycle {
    pub statuses: Vec<serde_json::Value>,
    pub included_block: Option<Hash>,
    pub included_extrinsic_index: Option<u32>,
    pub dispatch_outcome: Option<DispatchOutcome>,
    /// Compatibility projection; `dispatch_outcome` is authoritative.
    pub dispatch_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSystemOperation {
    pub encoded_extrinsic: Vec<u8>,
    pub submitted_nonce: u32,
    pub system_op_nonce: u64,
    pub correlation: Hash,
}

pub fn account_id_rpc_param(account: [u8; 32]) -> String {
    AccountId32::new(account).to_ss58check()
}

pub struct MiniJamChainClient {
    rpc_url: String,
    request_timeout: Duration,
    rpc: futures::lock::Mutex<WsClient>,
    signer: sr25519::Pair,
    next_nonce: futures::lock::Mutex<NonceCursor>,
    submit_lock: futures::lock::Mutex<()>,
    system_op_lock: futures::lock::Mutex<()>,
    next_system_op_nonce: futures::lock::Mutex<Option<u64>>,
}

impl MiniJamChainClient {
    pub async fn connect(
        rpc_url: impl Into<String>,
        signer: sr25519::Pair,
        request_timeout: Duration,
    ) -> Result<Self, ChainClientError> {
        let rpc_url = rpc_url.into();
        let rpc = Self::connect_rpc(&rpc_url, request_timeout).await?;
        Ok(Self {
            rpc_url,
            request_timeout,
            rpc: futures::lock::Mutex::new(rpc),
            signer,
            next_nonce: futures::lock::Mutex::new(NonceCursor::default()),
            submit_lock: futures::lock::Mutex::new(()),
            system_op_lock: futures::lock::Mutex::new(()),
            next_system_op_nonce: futures::lock::Mutex::new(None),
        })
    }

    async fn connect_rpc(url: &str, timeout: Duration) -> Result<WsClient, ChainClientError> {
        WsClientBuilder::default()
            .request_timeout(timeout)
            .build(url)
            .await
            .map_err(|error| ChainClientError::Rpc(error.to_string()))
    }

    async fn reconnect(&self) -> Result<(), ChainClientError> {
        let replacement = Self::connect_rpc(&self.rpc_url, self.request_timeout).await?;
        *self.rpc.lock().await = replacement;
        Ok(())
    }

    pub fn signer_account(&self) -> [u8; 32] {
        let account: AccountId32 =
            sp_runtime::MultiSigner::Sr25519(self.signer.public()).into_account();
        account.into()
    }

    pub async fn finalized_context(&self) -> Result<FinalizedContext, ChainClientError> {
        rpc::finalized_context(&*self.rpc.lock().await).await
    }

    pub async fn genesis_hash(&self) -> Result<Hash, ChainClientError> {
        rpc::genesis_hash(&*self.rpc.lock().await).await
    }

    pub async fn observe_finality(&self) -> Result<FinalityObservation, ChainClientError> {
        let context = self.finalized_context().await?;
        Ok(FinalityObservation {
            finalized_block: context.block_hash,
            finalized_number: context.block_number,
        })
    }

    pub async fn wait_for_finalized_event<F>(
        &self,
        from_block: u32,
        wait: Duration,
        mut matches: F,
    ) -> Result<FinalizedEvent, ChainClientError>
    where
        F: FnMut(&minijam_runtime::RuntimeEvent) -> bool,
    {
        let started = std::time::Instant::now();
        let mut next = from_block;
        loop {
            let finalized = self.finalized_context().await?;
            while next <= finalized.block_number {
                let block_hash = rpc::block_hash(&*self.rpc.lock().await, next).await?;
                for event in rpc::events_at(&*self.rpc.lock().await, block_hash).await? {
                    if matches(&event) {
                        return Ok(FinalizedEvent {
                            block_hash,
                            block_number: next,
                            event,
                        });
                    }
                }
                next = next.saturating_add(1);
            }
            if started.elapsed() >= wait {
                return Err(ChainClientError::Rpc(
                    "timed out waiting for finalized event".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub async fn service_info_at(
        &self,
        block: Hash,
        service_id: u32,
    ) -> Result<Option<Vec<u8>>, ChainClientError> {
        rpc::optional_hex(
            &*self.rpc.lock().await,
            "minijam_getServiceInfoAt",
            serde_json::json!([rpc::hex(&block), service_id]),
        )
        .await
    }

    /// Return the canonical code hash without exposing Jambda's ServiceInfo
    /// representation to workload repositories.
    pub async fn service_code_hash_at(
        &self,
        block: Hash,
        service_id: u32,
    ) -> Result<Option<Hash>, ChainClientError> {
        let Some(bytes) = self.service_info_at(block, service_id).await? else {
            return Ok(None);
        };
        let value = StateValue::decode(&mut bytes.as_slice())
            .map_err(|error| ChainClientError::Decode(error.to_string()))?;
        let value = value.into_inner();
        let info = ServiceInfo::decode(&mut value.as_slice())
            .map_err(|error| ChainClientError::Decode(error.to_string()))?;
        Ok(Some(info.code_hash.0))
    }

    pub async fn service_storage_at(
        &self,
        block: Hash,
        service_id: u32,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, ChainClientError> {
        rpc::optional_hex(
            &*self.rpc.lock().await,
            "minijam_getServiceStorageAt",
            serde_json::json!([rpc::hex(&block), service_id, rpc::hex(key)]),
        )
        .await
    }

    pub async fn service_preimage_at(
        &self,
        block: Hash,
        service_id: u32,
        code_hash: Hash,
    ) -> Result<Option<Vec<u8>>, ChainClientError> {
        rpc::optional_hex(
            &*self.rpc.lock().await,
            "minijam_getServicePreimageAt",
            serde_json::json!([rpc::hex(&block), service_id, rpc::hex(&code_hash)]),
        )
        .await
    }

    pub async fn submit_create_service(
        &self,
        code_hash: Hash,
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    ) -> Result<Submission, ChainClientError> {
        self.submit_system_command(SystemCommandV2::CreateService {
            code_hash,
            code_len,
            min_item_gas,
            min_memo_gas,
        })
        .await
    }

    pub async fn prepare_create_service(
        &self,
        code_hash: Hash,
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    ) -> Result<PreparedSystemOperation, ChainClientError> {
        self.prepare_system_command(SystemCommandV2::CreateService {
            code_hash,
            code_len,
            min_item_gas,
            min_memo_gas,
        })
        .await
    }

    async fn submit_system_command(
        &self,
        command: SystemCommandV2,
    ) -> Result<Submission, ChainClientError> {
        let _system_op = self.system_op_lock.lock().await;
        // Keep the account nonce reservation and the complete watcher
        // lifecycle in the same submission critical section. Otherwise a
        // normal call could claim the same account nonce between prepare and
        // submit.
        let _submission = self.submit_lock.lock().await;
        let prepared = self.prepare_system_command_inner(command).await?;
        // Drop the RPC mutex guard before dispatch attribution. Keeping the
        // guard alive as the `match` scrutinee deadlocks the success arm when
        // `complete_prepared_submission` reads the finalized block/events
        // through the same client.
        let watched = {
            let rpc = self.rpc.lock().await;
            rpc::submit_and_watch_extrinsic(&rpc, &prepared.encoded_extrinsic, self.request_timeout)
                .await
        };
        match watched {
            Ok((extrinsic_hash, statuses)) => {
                self.complete_prepared_submission(prepared, extrinsic_hash, statuses)
                    .await
            }
            Err(error) => {
                self.next_nonce.lock().await.invalidate();
                self.next_system_op_nonce.lock().await.take();
                if matches!(error, ChainClientError::Rpc(_)) {
                    let _ = self.reconnect().await;
                }
                Err(error)
            }
        }
    }

    async fn prepare_system_command(
        &self,
        command: SystemCommandV2,
    ) -> Result<PreparedSystemOperation, ChainClientError> {
        let _system_op = self.system_op_lock.lock().await;
        self.prepare_system_command_inner(command).await
    }

    async fn prepare_system_command_inner(
        &self,
        command: SystemCommandV2,
    ) -> Result<PreparedSystemOperation, ChainClientError> {
        let account = sp_runtime::MultiSigner::Sr25519(self.signer.public()).into_account();
        let sender = system_op_sender(&account);
        let next_system_nonce = self.next_system_op_nonce.lock().await;
        let system_nonce = match *next_system_nonce {
            Some(nonce) => nonce,
            None => rpc::system_op_nonce(&*self.rpc.lock().await, sender).await?,
        };
        let correlation =
            minijam_protocol::SystemOpV2::compute_request_id(&sender, system_nonce, &command);
        let submitted_nonce = {
            let current = self.next_nonce.lock().await;
            match current.peek() {
                Some(nonce) => nonce,
                None => {
                    let account =
                        sp_runtime::MultiSigner::Sr25519(self.signer.public()).into_account();
                    rpc::account_nonce(&*self.rpc.lock().await, account.into()).await?
                }
            }
        };
        let genesis = rpc::genesis_hash(&*self.rpc.lock().await).await?;
        let encoded_extrinsic = extrinsic::sign_call(
            &self.signer,
            submitted_nonce,
            genesis,
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_system_op {
                command: Box::new(command),
            }),
        );
        Ok(PreparedSystemOperation {
            encoded_extrinsic,
            submitted_nonce,
            system_op_nonce: system_nonce,
            correlation,
        })
    }

    pub async fn submit_prepared_extrinsic(
        &self,
        prepared: PreparedSystemOperation,
    ) -> Result<Submission, ChainClientError> {
        let _system_op = self.system_op_lock.lock().await;
        let _submission = self.submit_lock.lock().await;
        match rpc::submit_and_watch_extrinsic(
            &*self.rpc.lock().await,
            &prepared.encoded_extrinsic,
            self.request_timeout,
        )
        .await
        {
            Ok((extrinsic_hash, statuses)) => {
                self.complete_prepared_submission(prepared, extrinsic_hash, statuses)
                    .await
            }
            Err(error) => {
                self.next_nonce.lock().await.invalidate();
                self.next_system_op_nonce.lock().await.take();
                if matches!(error, ChainClientError::Rpc(_)) {
                    let _ = self.reconnect().await;
                }
                Err(error)
            }
        }
    }

    async fn complete_prepared_submission(
        &self,
        prepared: PreparedSystemOperation,
        extrinsic_hash: Hash,
        statuses: Vec<serde_json::Value>,
    ) -> Result<Submission, ChainClientError> {
        let included_block = statuses.iter().rev().find_map(|status| {
            let value = status.get("inBlock").or_else(|| status.get("finalized"))?;
            let value = value
                .as_str()?
                .strip_prefix("0x")
                .unwrap_or(value.as_str()?);
            if value.len() != 64 {
                return None;
            }
            let mut hash = [0u8; 32];
            for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
                hash[index] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
            }
            Some(hash)
        });
        let block = included_block.ok_or_else(|| {
            ChainClientError::Decode("finalized transaction has no included block".into())
        });

        // Finalized means the account nonce was consumed even if dispatch
        // failed. The system-op nonce remains unknown until exact attribution.
        self.commit_account_nonce(&prepared).await;
        let (included_extrinsic_index, dispatch_outcome) = match block {
            Ok(block) => {
                match rpc::dispatch_outcome_at(&*self.rpc.lock().await, block, extrinsic_hash).await
                {
                    Ok(value) => value,
                    Err(error) => {
                        self.invalidate_system_op_nonce().await;
                        return Err(error);
                    }
                }
            }
            Err(error) => {
                self.invalidate_system_op_nonce().await;
                return Err(error);
            }
        };
        let dispatch_error = match &dispatch_outcome {
            DispatchOutcome::Success => None,
            DispatchOutcome::Failed(error) => Some(error.clone()),
        };
        match dispatch_outcome {
            DispatchOutcome::Success => self.commit_system_op_nonce(&prepared).await,
            DispatchOutcome::Failed(_) => self.invalidate_system_op_nonce().await,
        }
        Ok(Submission {
            extrinsic_hash,
            submitted_nonce: prepared.submitted_nonce,
            correlation: prepared.correlation,
            lifecycle: Some(TransactionLifecycle {
                statuses,
                included_block,
                included_extrinsic_index: Some(included_extrinsic_index),
                dispatch_outcome: Some(dispatch_outcome),
                dispatch_error,
            }),
        })
    }

    async fn commit_account_nonce(&self, prepared: &PreparedSystemOperation) {
        self.next_nonce
            .lock()
            .await
            .commit(prepared.submitted_nonce);
    }

    async fn commit_system_op_nonce(&self, prepared: &PreparedSystemOperation) {
        *self.next_system_op_nonce.lock().await = Some(prepared.system_op_nonce.saturating_add(1));
    }

    async fn invalidate_system_op_nonce(&self) {
        self.next_system_op_nonce.lock().await.take();
    }

    pub async fn submit_preimage(&self, bytes: Vec<u8>) -> Result<Submission, ChainClientError> {
        let correlation = minijam_protocol::blake2_256(&bytes);
        let canonical_preimage: CanonicalPreimageBytes = bytes
            .try_into()
            .map_err(|_| ChainClientError::InputTooLarge)?;
        self.submit_call(
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_preimage { canonical_preimage }),
            correlation,
        )
        .await
    }

    pub async fn submit_work(
        &self,
        canonical: Vec<u8>,
        bundle_ref: ContentRef,
        package_hash: Hash,
    ) -> Result<Submission, ChainClientError> {
        let canonical_work_package = canonical
            .try_into()
            .map_err(|_| ChainClientError::InputTooLarge)?;
        self.submit_call(
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_work {
                canonical_work_package,
                bundle_ref,
            }),
            package_hash,
        )
        .await
    }

    pub async fn submit_allocation(
        &self,
        allocation_id: u64,
        target_service: u32,
        amount: u128,
    ) -> Result<Submission, ChainClientError> {
        let allocation = pallet_minijam::AllocationV1 {
            allocation_id,
            target_service,
            amount,
        };
        let correlation = minijam_protocol::blake2_256(&allocation.encode());
        self.submit_call(
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_allocation { allocation }),
            correlation,
        )
        .await
    }

    async fn submit_call(
        &self,
        call: RuntimeCall,
        correlation: Hash,
    ) -> Result<Submission, ChainClientError> {
        // Preserve nonce submission order as well as uniqueness: a later nonce must never race ahead.
        let _submission = self.submit_lock.lock().await;
        let nonce = self.allocate_nonce().await?;
        let genesis = rpc::genesis_hash(&*self.rpc.lock().await).await?;
        let encoded = extrinsic::sign_call(&self.signer, nonce, genesis, call);
        match rpc::submit_extrinsic(&*self.rpc.lock().await, &encoded).await {
            Ok(extrinsic_hash) => Ok(Submission {
                extrinsic_hash,
                submitted_nonce: nonce,
                correlation,
                lifecycle: None,
            }),
            Err(error) => {
                self.next_nonce.lock().await.invalidate();
                if matches!(error, ChainClientError::Rpc(_)) {
                    let _ = self.reconnect().await;
                }
                Err(error)
            }
        }
    }

    async fn allocate_nonce(&self) -> Result<u32, ChainClientError> {
        let mut current = self.next_nonce.lock().await;
        if let Some(nonce) = current.take() {
            return Ok(nonce);
        }
        let account = sp_runtime::MultiSigner::Sr25519(self.signer.public()).into_account();
        let nonce = rpc::account_nonce(&*self.rpc.lock().await, account.into()).await?;
        Ok(current.initialize(nonce))
    }

    pub async fn system_receipt<T: Decode>(
        &self,
        request_id: Hash,
    ) -> Result<Option<T>, ChainClientError> {
        self.decode_query(
            "minijam_getSystemReceipt",
            serde_json::json!([rpc::hex(&request_id)]),
        )
        .await
    }

    pub async fn work_status<T: Decode>(
        &self,
        work_id: WorkId,
    ) -> Result<Option<T>, ChainClientError> {
        self.decode_query("minijam_getWork", serde_json::json!([work_id]))
            .await
    }

    pub async fn work_id_by_package_hash(
        &self,
        package_hash: Hash,
    ) -> Result<Option<WorkId>, ChainClientError> {
        use jsonrpsee::core::client::ClientT;
        self.rpc
            .lock()
            .await
            .request(
                "minijam_getWorkIdByPackageHash",
                jsonrpsee::rpc_params![rpc::hex(&package_hash)],
            )
            .await
            .map_err(|error| ChainClientError::Rpc(error.to_string()))
    }

    pub async fn candidate<T: Decode>(
        &self,
        work_id: WorkId,
        round: u8,
    ) -> Result<Option<T>, ChainClientError> {
        self.decode_query("minijam_getCandidate", serde_json::json!([work_id, round]))
            .await
    }

    pub async fn execution_receipt(
        &self,
        work_id: WorkId,
    ) -> Result<Option<Hash>, ChainClientError> {
        let value = rpc::optional_hex(
            &*self.rpc.lock().await,
            "minijam_getExecutionReceipt",
            serde_json::json!([work_id]),
        )
        .await?;
        value
            .map(|bytes| {
                bytes
                    .try_into()
                    .map_err(|_| ChainClientError::Decode("receipt hash is not 32 bytes".into()))
            })
            .transpose()
    }

    async fn decode_query<T: Decode>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<Option<T>, ChainClientError> {
        rpc::optional_hex(&*self.rpc.lock().await, method, params)
            .await?
            .map(|bytes| {
                T::decode(&mut bytes.as_slice())
                    .map_err(|error| ChainClientError::Decode(error.to_string()))
            })
            .transpose()
    }
}

fn system_op_sender(account: &AccountId32) -> Hash {
    minijam_protocol::blake2_256(&account.encode())
}

#[derive(Default)]
struct NonceCursor {
    next: Option<u32>,
}

impl NonceCursor {
    fn peek(&self) -> Option<u32> {
        self.next
    }

    fn commit(&mut self, nonce: u32) {
        self.next = Some(nonce.saturating_add(1));
    }

    fn take(&mut self) -> Option<u32> {
        let nonce = self.next?;
        self.next = Some(nonce.saturating_add(1));
        Some(nonce)
    }

    fn initialize(&mut self, nonce: u32) -> u32 {
        self.next = Some(nonce.saturating_add(1));
        nonce
    }

    fn invalidate(&mut self) {
        self.next = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{system_op_sender, AccountId32, NonceCursor};

    #[test]
    fn nonce_cursor_allocates_once_and_resynchronizes_after_failure() {
        let mut cursor = NonceCursor::default();
        assert_eq!(cursor.take(), None);
        assert_eq!(cursor.initialize(10), 10);
        assert_eq!(cursor.take(), Some(11));
        assert_eq!(cursor.take(), Some(12));
        cursor.invalidate();
        assert_eq!(cursor.take(), None);
        assert_eq!(cursor.initialize(20), 20);
        cursor.commit(20);
        assert_eq!(cursor.peek(), Some(21));
    }

    #[test]
    fn system_op_sender_matches_runtime_account_derivation() {
        let account = AccountId32::new([0x92; 32]);
        assert_eq!(
            system_op_sender(&account),
            minijam_protocol::blake2_256(account.as_ref())
        );
        assert_ne!(system_op_sender(&account), <[u8; 32]>::from(account));
    }
}
