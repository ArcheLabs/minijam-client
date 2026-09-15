// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod stage0 {
    use super::Hash;

    pub const CORE_INDEX: u16 = 0;
    pub const AUTH_CODE_HOST: u32 = 0;
    pub const AUTH_CODE_HASH: [u8; 32] = [0; 32];
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct FinalizedContextV1 {
        pub block_hash: Hash,
        pub block_number: u32,
        pub state_root: Hash,
        pub slot: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct RefineContextV1 {
        pub anchor: Hash,
        pub state_root: Hash,
        pub lookup_anchor: Hash,
        pub lookup_anchor_slot: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ContextError {
        AnchorMismatch,
        LookupAnchorMismatch,
        StateRootMismatch,
        SlotMismatch,
    }

    pub fn validate_refine_context(
        context: RefineContextV1,
        finalized: FinalizedContextV1,
    ) -> Result<(), ContextError> {
        if context.anchor != context.lookup_anchor {
            return Err(ContextError::AnchorMismatch);
        }
        if context.lookup_anchor != finalized.block_hash {
            return Err(ContextError::LookupAnchorMismatch);
        }
        if context.state_root != finalized.state_root {
            return Err(ContextError::StateRootMismatch);
        }
        if context.lookup_anchor_slot != finalized.slot || finalized.slot != finalized.block_number
        {
            return Err(ContextError::SlotMismatch);
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn contexts() -> (RefineContextV1, FinalizedContextV1) {
            (
                RefineContextV1 {
                    anchor: [1; 32],
                    state_root: [2; 32],
                    lookup_anchor: [1; 32],
                    lookup_anchor_slot: 7,
                },
                FinalizedContextV1 {
                    block_hash: [1; 32],
                    block_number: 7,
                    state_root: [2; 32],
                    slot: 7,
                },
            )
        }

        #[test]
        fn validates_complete_stage0_refine_context() {
            let (context, finalized) = contexts();
            assert_eq!(validate_refine_context(context, finalized), Ok(()));
        }

        #[test]
        fn rejects_each_inconsistent_stage0_context_field() {
            let (context, finalized) = contexts();

            let mut invalid = context;
            invalid.anchor = [9; 32];
            assert_eq!(
                validate_refine_context(invalid, finalized),
                Err(ContextError::AnchorMismatch)
            );

            let mut invalid = finalized;
            invalid.block_hash = [9; 32];
            assert_eq!(
                validate_refine_context(context, invalid),
                Err(ContextError::LookupAnchorMismatch)
            );

            let mut invalid = finalized;
            invalid.state_root = [9; 32];
            assert_eq!(
                validate_refine_context(context, invalid),
                Err(ContextError::StateRootMismatch)
            );

            let mut invalid = finalized;
            invalid.slot = 8;
            assert_eq!(
                validate_refine_context(context, invalid),
                Err(ContextError::SlotMismatch)
            );
        }
    }
}

use alloc::vec::Vec;
use bounded_collections::{BoundedVec, ConstU32};
use parity_scale_codec::DecodeWithMemTracking;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub const PROTOCOL_VERSION_V1: u16 = 1;
pub const SYSTEM_SERVICE_ABI_VERSION: u16 = 2;
pub const SYSTEM_OP_REQUEST_DOMAIN_V2: &[u8] = b"minijam/system-op/v2";
pub const MAX_DELTA_BYTES: u32 = 4 * 1_048_576;

pub const NS_SYSTEM: u8 = 0x00;
pub const NS_SERVICE_INFO: u8 = 0x10;
pub const NS_SERVICE_STORAGE: u8 = 0x11;
pub const NS_SERVICE_LOOKUP: u8 = 0x12;
pub const NS_PREIMAGE: u8 = 0x13;
pub const NS_ADMIN_BRIDGE: u8 = 0x20;

pub type Hash = [u8; 32];
pub type BlockNumber = u32;

pub type CanonicalReportBytes = BoundedVec<u8, ConstU32<1_048_576>>;
pub type CanonicalPreimageBytes = BoundedVec<u8, ConstU32<1_048_576>>;
pub type BulletinProofBytes = BoundedVec<u8, ConstU32<65_536>>;
pub type WorkBundleBlob = BoundedVec<u8, ConstU32<1_048_576>>;
pub type WorkBundleItemExternalData = BoundedVec<WorkBundleBlob, ConstU32<128>>;
pub type WorkBundleExternalData = BoundedVec<WorkBundleItemExternalData, ConstU32<64>>;
pub type WorkBundleImportSegments = BoundedVec<WorkBundleBlob, ConstU32<1_024>>;
pub type WorkBundleImportProofs = BoundedVec<WorkBundleBlob, ConstU32<1_024>>;
pub type StateValue = BoundedVec<u8, ConstU32<1_048_576>>;
pub type StateChanges = BoundedVec<ProtocolStateChange, ConstU32<4_096>>;
pub type ReportBatch = BoundedVec<CanonicalReportBytes, ConstU32<4>>;
pub type PreimageBatch = BoundedVec<CanonicalPreimageBytes, ConstU32<64>>;
pub type ConsumedReports = BoundedVec<Hash, ConstU32<4>>;
pub type ConsumedPreimages = BoundedVec<Hash, ConstU32<64>>;
pub type SystemOpBatch = BoundedVec<SystemOpV2, ConstU32<64>>;
pub type ConsumedSystemOps = BoundedVec<Hash, ConstU32<64>>;

/// Ownerless system operation ABI used by the fresh MiniJAM genesis.
///
/// `submitter` identifies the account that submitted the request. It is not a
/// controller, manager, or owner of the target service. Authorization for
/// service-defined operations belongs to the service and is deliberately not
/// encoded as protocol ownership here.
#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct SystemOpV2 {
    pub request_id: Hash,
    pub submitter: [u8; 32],
    pub nonce: u64,
    pub command: SystemCommandV2,
}

impl DecodeWithMemTracking for SystemOpV2 {}

impl SystemOpV2 {
    pub fn new(submitter: [u8; 32], nonce: u64, command: SystemCommandV2) -> Self {
        let request_id = Self::compute_request_id(&submitter, nonce, &command);
        Self {
            request_id,
            submitter,
            nonce,
            command,
        }
    }

    pub fn compute_request_id(submitter: &[u8; 32], nonce: u64, command: &SystemCommandV2) -> Hash {
        let mut payload = Vec::new();
        payload.extend_from_slice(SYSTEM_OP_REQUEST_DOMAIN_V2);
        payload.extend_from_slice(submitter);
        payload.extend_from_slice(&nonce.to_le_bytes());
        payload.extend_from_slice(&command.encode());
        blake2_256(&payload)
    }

    pub fn request_id_matches(&self) -> bool {
        self.request_id == Self::compute_request_id(&self.submitter, self.nonce, &self.command)
    }
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum SystemCommandV2 {
    CreateService {
        code_hash: Hash,
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    },
    ApplyAllocation {
        allocation_id: u64,
        target_service: u32,
        amount: u64,
    },
}

impl DecodeWithMemTracking for SystemCommandV2 {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum SystemReceiptV2 {
    ServiceCreated { service_id: u32 },
    Rejected { code: u32 },
}

impl DecodeWithMemTracking for SystemReceiptV2 {}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum HashingAlgorithm {
    Blake2b256,
    Sha2_256,
    Keccak256,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct CidConfig {
    pub codec: u64,
    pub hashing: HashingAlgorithm,
}

impl Default for CidConfig {
    fn default() -> Self {
        Self {
            codec: 0x55,
            hashing: HashingAlgorithm::Blake2b256,
        }
    }
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct ContentRef {
    pub cid_v1: BoundedVec<u8, ConstU32<128>>,
    pub content_hash: Hash,
    pub size: u64,
}

impl DecodeWithMemTracking for ContentRef {}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct StorageLocation {
    pub block_number: BlockNumber,
    pub transaction_index: u32,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct StorageReceipt {
    pub content: ContentRef,
    pub location: StorageLocation,
    pub retention_until: BlockNumber,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum BulletinEvidence {
    NoExternalProofV1 {
        receipt: Option<StorageReceipt>,
    },
    ProofV1 {
        chain_id: Hash,
        head: Hash,
        location: StorageLocation,
        commitment: Hash,
        proof: BulletinProofBytes,
    },
}

/// Compact report projection retained as the executor's compatibility return
/// type. Consensus keys reports by `package_hash`.
#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct ReportMetadataV1 {
    pub package_hash: Hash,
    pub context_hash: Hash,
    pub exports_root: Hash,
    pub accumulate_gas: u64,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct PreimageMetadataV1 {
    pub requester: u32,
    pub blob_hash: Hash,
    pub blob_len: u32,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct WorkerTaskV1 {
    pub package_hash: Hash,
    pub bundle_ref: ContentRef,
}

impl DecodeWithMemTracking for WorkerTaskV1 {}

#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    TypeInfo,
)]
pub enum PackageStatus {
    Pending,
    Imported,
    Failed,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum StateOperation {
    Upsert,
    Update,
    Remove,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct ProtocolStateChange {
    pub key: [u8; 31],
    pub operation: StateOperation,
    pub value: Option<StateValue>,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum AssetId {
    Native,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum BridgeEffect {
    Inbound {
        nonce: u64,
        target_service: u32,
        asset: AssetId,
        amount: u128,
        account: [u8; 32],
    },
    Outbound {
        nonce: u64,
        source_service: u32,
        asset: AssetId,
        amount: u128,
        account: [u8; 32],
    },
}

pub fn blake2_256(bytes: &[u8]) -> Hash {
    let hash = blake2b_simd::Params::new().hash_length(32).hash(bytes);
    let mut output = [0u8; 32];
    output.copy_from_slice(hash.as_bytes());
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_op_request_id_commits_submitter_nonce_and_command() {
        let command = SystemCommandV2::CreateService {
            code_hash: [9u8; 32],
            code_len: 32,
            min_item_gas: 1,
            min_memo_gas: 2,
        };
        let op = SystemOpV2::new([1u8; 32], 7, command.clone());
        assert!(op.request_id_matches());

        let mut changed_nonce = op.clone();
        changed_nonce.nonce = 8;
        assert!(!changed_nonce.request_id_matches());

        assert_ne!(
            op.request_id,
            SystemOpV2::compute_request_id(&[2u8; 32], 7, &command)
        );
    }

    #[test]
    fn ownerless_v2_create_has_stable_shape_and_request_id() {
        let op = SystemOpV2::new(
            [1u8; 32],
            7,
            SystemCommandV2::CreateService {
                code_hash: [9u8; 32],
                code_len: 32,
                min_item_gas: 1,
                min_memo_gas: 2,
            },
        );
        assert!(op.request_id_matches());
        let encoded = op.encode();
        let mut input = encoded.as_slice();
        let decoded = SystemOpV2::decode(&mut input).expect("V2 op must decode");
        assert!(input.is_empty());
        assert_eq!(decoded, op);
        assert!(matches!(
            decoded.command,
            SystemCommandV2::CreateService { .. }
        ));
    }

    #[test]
    fn ownerless_v2_allocation_is_not_an_upgrade_command() {
        let op = SystemOpV2::new(
            [2u8; 32],
            3,
            SystemCommandV2::ApplyAllocation {
                allocation_id: 11,
                target_service: 19,
                amount: 500,
            },
        );
        assert!(op.request_id_matches());
        assert!(matches!(
            op.command,
            SystemCommandV2::ApplyAllocation { .. }
        ));
    }
}
