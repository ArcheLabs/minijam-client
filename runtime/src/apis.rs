use alloc::vec::Vec;
use frame_support::{
    genesis_builder_helper::{build_state, get_preset},
    weights::Weight,
};
use pallet_grandpa::AuthorityId as GrandpaId;
use parity_scale_codec::Encode;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::{
    traits::{Block as BlockT, NumberFor},
    transaction_validity::{TransactionSource, TransactionValidity},
    ApplyExtrinsicResult,
};
use sp_session::OpaqueGeneratedSessionKeys;
use sp_version::RuntimeVersion;

use super::{
    AccountId, Aura, Balance, Block, Executive, Grandpa, InherentDataExt, Nonce, Runtime,
    RuntimeCall, RuntimeGenesisConfig, SessionKeys, System, TransactionPayment, VERSION,
};

impl_runtime_apis! {
    impl sp_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion { VERSION }
        fn execute_block(block: <Block as BlockT>::LazyBlock) {
            Executive::execute_block(block);
        }
        fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
            Executive::initialize_block(header)
        }
    }

    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }
        fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
            Runtime::metadata_at_version(version)
        }
        fn metadata_versions() -> Vec<u32> {
            Runtime::metadata_versions()
        }
    }

    impl frame_support::view_functions::runtime_api::RuntimeViewFunction<Block> for Runtime {
        fn execute_view_function(
            id: frame_support::view_functions::ViewFunctionId,
            input: Vec<u8>,
        ) -> Result<Vec<u8>, frame_support::view_functions::ViewFunctionDispatchError> {
            Runtime::execute_view_function(id, input)
        }
    }

    impl minijam_rpc_runtime_api::MiniJamRuntimeApi<Block> for Runtime {
        fn get_work(work_id: minijam_protocol::WorkId) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_work(work_id).map(|value| value.encode())
        }

        fn get_work_by_package_hash(package_hash: minijam_protocol::Hash) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_work_by_package_hash(package_hash)
                .map(|value| value.encode())
        }

        fn get_work_bundle_ref(work_id: minijam_protocol::WorkId) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_work_bundle_ref(work_id)
                .map(|value| value.encode())
        }

        fn get_candidate(work_id: minijam_protocol::WorkId, round: u8) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_candidate(work_id, round)
                .map(|value| value.encode())
        }

        fn get_execution_receipt(work_id: minijam_protocol::WorkId) -> Option<minijam_protocol::Hash> {
            pallet_minijam::Pallet::<Runtime>::get_execution_receipt(work_id)
        }

        fn get_last_execution_receipt() -> Option<minijam_protocol::Hash> {
            pallet_minijam::Pallet::<Runtime>::get_last_execution_receipt()
        }

        fn get_service_fuel(service_id: u32) -> Vec<u8> {
            pallet_minijam::Pallet::<Runtime>::get_service_fuel(service_id).encode()
        }

        fn get_work_fuel_reservation(work_id: minijam_protocol::WorkId) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_work_fuel_reservation(work_id)
                .map(|value| value.encode())
        }

        fn get_work_fuel_settlement(work_id: minijam_protocol::WorkId) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_work_fuel_settlement(work_id)
                .map(|value| value.encode())
        }

        fn get_pending_preimages() -> Vec<u8> {
            pallet_minijam::Pallet::<Runtime>::get_pending_preimages().encode()
        }

        fn has_pending_preimage(
            requester: u32,
            blob_hash: minijam_protocol::Hash,
            blob_len: u32,
        ) -> bool {
            pallet_minijam::Pallet::<Runtime>::has_pending_preimage(
                requester,
                blob_hash,
                blob_len,
            )
        }

        fn get_pending_system_ops() -> Vec<u8> {
            pallet_minijam::Pallet::<Runtime>::get_pending_system_ops().encode()
        }

        fn get_quarantined_system_ops() -> Vec<u8> {
            pallet_minijam::Pallet::<Runtime>::get_quarantined_system_ops().encode()
        }

        fn get_system_op(request_id: minijam_protocol::Hash) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_system_op(request_id)
                .map(|value| value.encode())
        }

        fn get_system_receipt(request_id: minijam_protocol::Hash) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_system_receipt(request_id)
                .map(|value| value.encode())
        }

        fn get_system_service_info() -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_system_service_info()
                .map(|value| value.encode())
        }

        fn get_protocol_state(key: [u8; 31]) -> Option<Vec<u8>> {
            pallet_minijam::Pallet::<Runtime>::get_protocol_state(key)
                .map(|value| value.encode())
        }
    }

    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }
        fn finalize_block() -> <Block as BlockT>::Header {
            Executive::finalize_block()
        }
        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }
        fn check_inherents(
            block: <Block as BlockT>::LazyBlock,
            data: sp_inherents::InherentData,
        ) -> sp_inherents::CheckInherentsResult {
            data.check_extrinsics(&block)
        }
    }

    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
        fn offchain_worker(header: &<Block as BlockT>::Header) {
            Executive::offchain_worker(header)
        }
    }

    impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
        fn slot_duration() -> sp_consensus_aura::SlotDuration {
            sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
        }
        fn authorities() -> Vec<AuraId> {
            pallet_aura::Authorities::<Runtime>::get().into_inner()
        }
    }

    impl sp_session::SessionKeys<Block> for Runtime {
        fn generate_session_keys(owner: Vec<u8>, seed: Option<Vec<u8>>) -> OpaqueGeneratedSessionKeys {
            SessionKeys::generate(&owner, seed).into()
        }
        fn decode_session_keys(encoded: Vec<u8>) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
            SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
        fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
            Grandpa::grandpa_authorities()
        }
        fn current_set_id() -> sp_consensus_grandpa::SetId {
            Grandpa::current_set_id()
        }
        fn submit_report_equivocation_unsigned_extrinsic(
            _equivocation_proof: sp_consensus_grandpa::EquivocationProof<
                <Block as BlockT>::Hash,
                NumberFor<Block>,
            >,
            _key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            None
        }
        fn generate_key_ownership_proof(
            _set_id: sp_consensus_grandpa::SetId,
            _authority_id: GrandpaId,
        ) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
            None
        }
    }

    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
        fn account_nonce(account: AccountId) -> Nonce {
            System::account_nonce(account)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
        fn query_info(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_info(uxt, len)
        }
        fn query_fee_details(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_fee_details(uxt, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            TransactionPayment::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall> for Runtime {
        fn query_call_info(
            call: RuntimeCall,
            len: u32,
        ) -> pallet_transaction_payment::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_call_info(call, len)
        }
        fn query_call_fee_details(
            call: RuntimeCall,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_call_fee_details(call, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            TransactionPayment::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
        fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
            build_state::<RuntimeGenesisConfig>(config)
        }
        fn get_preset(_id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
            get_preset::<RuntimeGenesisConfig>(_id, crate::genesis_config_presets::get_preset)
        }
        fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
            crate::genesis_config_presets::preset_names()
        }
    }
}
