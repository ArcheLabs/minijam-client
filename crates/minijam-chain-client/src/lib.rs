// SPDX-License-Identifier: Apache-2.0

mod events;
mod extrinsic;
mod rpc;

pub use events::{FinalityObservation, FinalizedEvent};
pub use rpc::{DispatchOutcome, FinalizedContext};

use std::time::Duration;

use jam_codec::Decode as JamDecode;
use jp_core_primitives::types::ServiceInfo;
use jsonrpsee::{core::client::ClientT, rpc_params};
use minijam_protocol::{CanonicalPreimageBytes, CanonicalReportBytes, Hash, SystemCommandV2};
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
    pub dispatch_error: Option<String>,
}

pub fn account_id_rpc_param(account: [u8; 32]) -> String {
    AccountId32::new(account).to_ss58check()
}

pub struct MiniJamChainClient {
    rpc_url: String,
    request_timeout: Duration,
    rpc: futures::lock::Mutex<jsonrpsee::ws_client::WsClient>,
    signer: sr25519::Pair,
    next_nonce: futures::lock::Mutex<NonceCursor>,
    submit_lock: futures::lock::Mutex<()>,
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
        })
    }
    async fn connect_rpc(
        url: &str,
        timeout: Duration,
    ) -> Result<jsonrpsee::ws_client::WsClient, ChainClientError> {
        jsonrpsee::ws_client::WsClientBuilder::default()
            .request_timeout(timeout)
            .build(url)
            .await
            .map_err(|error| ChainClientError::Rpc(error.to_string()))
    }
    async fn reconnect(&self) -> Result<(), ChainClientError> {
        *self.rpc.lock().await = Self::connect_rpc(&self.rpc_url, self.request_timeout).await?;
        Ok(())
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
    pub async fn service_code_hash_at(
        &self,
        block: Hash,
        service_id: u32,
    ) -> Result<Option<Hash>, ChainClientError> {
        let Some(bytes) = self.service_info_at(block, service_id).await? else {
            return Ok(None);
        };
        let value = minijam_protocol::StateValue::decode(&mut bytes.as_slice())
            .map_err(|error| ChainClientError::Decode(error.to_string()))?;
        let info: ServiceInfo = JamDecode::decode(&mut value.as_slice())
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
    async fn submit_system_command(
        &self,
        command: SystemCommandV2,
    ) -> Result<Submission, ChainClientError> {
        let account = sp_runtime::MultiSigner::Sr25519(self.signer.public()).into_account();
        let sender = minijam_protocol::blake2_256(&account.encode());
        let nonce: u64 = self
            .rpc
            .lock()
            .await
            .request("minijam_getSystemOpNonce", rpc_params![rpc::hex(&sender)])
            .await
            .map_err(|error| ChainClientError::Rpc(error.to_string()))?;
        let correlation =
            minijam_protocol::SystemOpV2::compute_request_id(&sender, nonce, &command);
        self.submit_call(
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_system_op {
                command: Box::new(command),
            }),
            correlation,
        )
        .await
    }

    pub async fn submit_preimage(&self, bytes: Vec<u8>) -> Result<Submission, ChainClientError> {
        let canonical_preimage: CanonicalPreimageBytes = bytes
            .clone()
            .try_into()
            .map_err(|_| ChainClientError::InputTooLarge)?;
        self.submit_call(
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_preimage { canonical_preimage }),
            minijam_protocol::blake2_256(&bytes),
        )
        .await
    }
    pub async fn submit_preimage_finalized(
        &self,
        bytes: Vec<u8>,
    ) -> Result<Submission, ChainClientError> {
        let canonical_preimage: CanonicalPreimageBytes = bytes
            .clone()
            .try_into()
            .map_err(|_| ChainClientError::InputTooLarge)?;
        self.submit_call_and_watch(
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_preimage { canonical_preimage }),
            minijam_protocol::blake2_256(&bytes),
        )
        .await
    }
    pub async fn submit_report(
        &self,
        bytes: Vec<u8>,
        package_hash: Hash,
    ) -> Result<Submission, ChainClientError> {
        let canonical_report: CanonicalReportBytes = bytes
            .try_into()
            .map_err(|_| ChainClientError::InputTooLarge)?;
        self.submit_call(
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_report { canonical_report }),
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
        self.submit_call(
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_allocation {
                allocation: allocation.clone(),
            }),
            minijam_protocol::blake2_256(&allocation.encode()),
        )
        .await
    }

    async fn submit_call(
        &self,
        call: RuntimeCall,
        correlation: Hash,
    ) -> Result<Submission, ChainClientError> {
        let _guard = self.submit_lock.lock().await;
        let nonce = self.allocate_nonce().await?;
        let genesis = rpc::genesis_hash(&*self.rpc.lock().await).await?;
        let encoded = extrinsic::sign_call(&self.signer, nonce, genesis, call);
        let submitted = {
            let rpc = self.rpc.lock().await;
            rpc::submit_extrinsic(&rpc, &encoded).await
        };
        match submitted {
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
    async fn submit_call_and_watch(
        &self,
        call: RuntimeCall,
        correlation: Hash,
    ) -> Result<Submission, ChainClientError> {
        let _guard = self.submit_lock.lock().await;
        let nonce = self.allocate_nonce().await?;
        let genesis = rpc::genesis_hash(&*self.rpc.lock().await).await?;
        let encoded = extrinsic::sign_call(&self.signer, nonce, genesis, call);
        let watched = {
            let rpc = self.rpc.lock().await;
            rpc::submit_and_watch_extrinsic(&rpc, &encoded, self.request_timeout).await
        };
        match watched {
            Ok((hash, statuses)) => {
                let block = included_block_from_statuses(&statuses).ok_or_else(|| {
                    ChainClientError::Decode("finalized transaction has no included block".into())
                })?;
                let (index, outcome) = {
                    let rpc = self.rpc.lock().await;
                    rpc::dispatch_outcome_at(&rpc, block, hash).await
                }?;
                self.next_nonce.lock().await.commit(nonce);
                match &outcome {
                    DispatchOutcome::Success => Ok(Submission {
                        extrinsic_hash: hash,
                        submitted_nonce: nonce,
                        correlation,
                        lifecycle: Some(TransactionLifecycle {
                            statuses,
                            included_block: Some(block),
                            included_extrinsic_index: Some(index),
                            dispatch_outcome: Some(outcome),
                            dispatch_error: None,
                        }),
                    }),
                    DispatchOutcome::Failed(error) => {
                        Err(ChainClientError::Dispatch(error.clone()))
                    }
                }
            }
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
        let mut cursor = self.next_nonce.lock().await;
        if let Some(nonce) = cursor.take() {
            return Ok(nonce);
        }
        let account = sp_runtime::MultiSigner::Sr25519(self.signer.public()).into_account();
        let nonce = rpc::account_nonce(&*self.rpc.lock().await, account.into()).await?;
        Ok(cursor.initialize(nonce))
    }

    pub async fn package_status(
        &self,
        package_hash: Hash,
    ) -> Result<Option<minijam_protocol::PackageStatus>, ChainClientError> {
        let status: Option<String> = self
            .rpc
            .lock()
            .await
            .request(
                "minijam_getPackageStatus",
                rpc_params![rpc::hex(&package_hash)],
            )
            .await
            .map_err(|error| ChainClientError::Rpc(error.to_string()))?;
        Ok(status.map(|value| match value.as_str() {
            "pending" => minijam_protocol::PackageStatus::Pending,
            "imported" => minijam_protocol::PackageStatus::Imported,
            "failed" => minijam_protocol::PackageStatus::Failed,
            _ => minijam_protocol::PackageStatus::Failed,
        }))
    }
    pub async fn package_failure(
        &self,
        package_hash: Hash,
    ) -> Result<Option<Vec<u8>>, ChainClientError> {
        rpc::optional_hex(
            &*self.rpc.lock().await,
            "minijam_getPackageFailure",
            serde_json::json!([rpc::hex(&package_hash)]),
        )
        .await
    }
    pub async fn execution_receipt_by_package_hash(
        &self,
        package_hash: Hash,
    ) -> Result<Option<Hash>, ChainClientError> {
        rpc::optional_hex(
            &*self.rpc.lock().await,
            "minijam_getExecutionReceiptByPackageHash",
            serde_json::json!([rpc::hex(&package_hash)]),
        )
        .await?
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| ChainClientError::Decode("receipt hash is not 32 bytes".into()))
        })
        .transpose()
    }
    pub async fn system_receipt<T: Decode>(
        &self,
        request_id: Hash,
    ) -> Result<Option<T>, ChainClientError> {
        self.decode_query::<minijam_protocol::StateValue>(
            "minijam_getSystemReceipt",
            serde_json::json!([rpc::hex(&request_id)]),
        )
        .await?
        .map(decode_state_value)
        .transpose()
    }
    pub async fn system_op<T: Decode>(
        &self,
        request_id: Hash,
    ) -> Result<Option<T>, ChainClientError> {
        self.decode_query(
            "minijam_getSystemOp",
            serde_json::json!([rpc::hex(&request_id)]),
        )
        .await
    }
    pub async fn pending_system_ops<T: Decode>(&self) -> Result<T, ChainClientError> {
        self.required_query("minijam_getPendingSystemOps", serde_json::json!([]))
            .await
    }
    pub async fn quarantined_system_ops<T: Decode>(&self) -> Result<T, ChainClientError> {
        self.required_query("minijam_getQuarantinedSystemOps", serde_json::json!([]))
            .await
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
    async fn required_query<T: Decode>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, ChainClientError> {
        self.decode_query(method, params)
            .await?
            .ok_or_else(|| ChainClientError::Decode(format!("{method} returned no value")))
    }
}

fn included_block_from_statuses(statuses: &[serde_json::Value]) -> Option<Hash> {
    statuses.iter().rev().find_map(|status| {
        let value = status
            .get("inBlock")
            .or_else(|| status.get("finalized"))?
            .as_str()?;
        let value = value.strip_prefix("0x").unwrap_or(value);
        if value.len() != 64 {
            return None;
        }
        let mut hash = [0u8; 32];
        for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
            hash[index] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
        }
        Some(hash)
    })
}
fn decode_state_value<T: Decode>(
    value: minijam_protocol::StateValue,
) -> Result<T, ChainClientError> {
    T::decode(&mut value.as_slice()).map_err(|error| ChainClientError::Decode(error.to_string()))
}

#[derive(Default)]
struct NonceCursor {
    next: Option<u32>,
}
impl NonceCursor {
    fn initialize(&mut self, nonce: u32) -> u32 {
        self.next = Some(nonce.saturating_add(1));
        nonce
    }
    fn take(&mut self) -> Option<u32> {
        let nonce = self.next?;
        self.next = Some(nonce.saturating_add(1));
        Some(nonce)
    }
    fn commit(&mut self, nonce: u32) {
        self.next = Some(nonce.saturating_add(1));
    }
    fn invalidate(&mut self) {
        self.next = None;
    }
}
