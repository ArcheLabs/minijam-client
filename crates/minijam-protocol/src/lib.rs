// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod stage0 {
    use super::Hash;

    pub const CORE_INDEX: u16 = 0;
    pub const AUTH_CODE_HOST: u32 = 0;
    pub const AUTH_CODE_HASH: [u8; 32] = [0; 32];
    // Local MINI Cells compatibility probe: match JAM 0.7.2's Refine ceiling.
    // MINI Cells' measured candidate is ~42.4M gas; keep the protocol default
    // at the JAM-compatible 1B ceiling rather than carrying an exploratory 5B.
    pub const REFINE_GAS_LIMIT: u64 = 1_000_000_000;
    pub const ACCUMULATE_GAS_LIMIT: u64 = 1_000_000_000;

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
pub const SYSTEM_OP_REQUEST_DOMAIN_V1: &[u8] = b"minijam/system-op/v1";
pub const UNIT: u128 = 1_000_000_000_000;

pub const TOP_WORKERS: u32 = 8;
pub const MAX_WORKS_PER_ROUND: u32 = 4;
pub const WORKERS_PER_WORK: u32 = 3;
pub const SUPPORT_THRESHOLD: u32 = 2;
pub const OPPOSE_THRESHOLD: u32 = 2;
pub const EPOCH_LENGTH: u32 = 100;
pub const ASSIGNMENT_SEED_DELAY: u32 = 10;
pub const REPORT_SUBMISSION_DEADLINE: u32 = 20;
pub const VOTE_WINDOW: u32 = 10;
pub const UNBONDING_EPOCHS: u32 = 2;
pub const MAX_CANDIDATE_ROUNDS: u8 = 3;
pub const MAX_DUTIES_PER_ROUND: u32 = 2;

pub const MINIMUM_WORKER_STAKE: u128 = 1_000 * UNIT;
pub const WORK_DEPOSIT: u128 = 0;
pub const CANDIDATE_BOND: u128 = 0;
pub const TIMELY_VOTE_REWARD: u128 = 0;
pub const ACCEPTED_SUBMITTER_REWARD: u128 = 0;
pub const MINIMUM_ABSENCE_SLASH: u128 = UNIT;
pub const REWARD_POOL_ENDOWMENT: u128 = 1_000_000 * UNIT;
pub const MAX_DELTA_BYTES: u32 = 4 * 1_048_576;

pub const NS_SYSTEM: u8 = 0x00;
pub const NS_SERVICE_INFO: u8 = 0x10;
pub const NS_SERVICE_STORAGE: u8 = 0x11;
pub const NS_SERVICE_LOOKUP: u8 = 0x12;
pub const NS_PREIMAGE: u8 = 0x13;
pub const NS_ADMIN_BRIDGE: u8 = 0x20;

pub type Hash = [u8; 32];
pub type ChainId = Hash;
pub type WorkId = u64;
pub type WorkerId = u64;
pub type AssignmentRound = u8;
pub type BlockNumber = u32;
pub type EpochIndex = u32;

pub type CanonicalReportBytes = BoundedVec<u8, ConstU32<1_048_576>>;
pub type CanonicalPreimageBytes = BoundedVec<u8, ConstU32<1_048_576>>;
pub type CanonicalWorkPackageBytes = BoundedVec<u8, ConstU32<1_048_576>>;
pub type WorkBundleBlob = BoundedVec<u8, ConstU32<1_048_576>>;
pub type WorkBundleItemExternalData = BoundedVec<WorkBundleBlob, ConstU32<128>>;
pub type WorkBundleExternalData = BoundedVec<WorkBundleItemExternalData, ConstU32<64>>;
pub type WorkBundleImportSegments = BoundedVec<WorkBundleBlob, ConstU32<1_024>>;
pub type WorkBundleImportProofs = BoundedVec<WorkBundleBlob, ConstU32<1_024>>;
pub type WorkerVoteAssignments = BoundedVec<WorkerId, ConstU32<8>>;
pub type WorkerVoteSubmissions = BoundedVec<WorkerId, ConstU32<8>>;
pub type BulletinProofBytes = BoundedVec<u8, ConstU32<65_536>>;
pub type ReportSignatures = BoundedVec<WorkerSignature, ConstU32<8>>;
pub type StateValue = BoundedVec<u8, ConstU32<1_048_576>>;
pub type StateChanges = BoundedVec<ProtocolStateChange, ConstU32<4_096>>;
pub type ReportBatch = BoundedVec<CanonicalReportBytes, ConstU32<4>>;
pub type PreimageBatch = BoundedVec<CanonicalPreimageBytes, ConstU32<64>>;
pub type ConsumedReports = BoundedVec<Hash, ConstU32<4>>;
pub type ConsumedPreimages = BoundedVec<Hash, ConstU32<64>>;
pub type SystemOpBatch = BoundedVec<SystemOpV1, ConstU32<64>>;
pub type ConsumedSystemOps = BoundedVec<Hash, ConstU32<64>>;

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct SystemOpV1 {
    pub request_id: Hash,
    pub sender: [u8; 32],
    pub nonce: u64,
    pub command: SystemCommandV1,
}

impl DecodeWithMemTracking for SystemOpV1 {}

impl SystemOpV1 {
    pub fn new(sender: [u8; 32], nonce: u64, command: SystemCommandV1) -> Self {
        let request_id = Self::compute_request_id(&sender, nonce, &command);
        Self {
            request_id,
            sender,
            nonce,
            command,
        }
    }

    pub fn compute_request_id(sender: &[u8; 32], nonce: u64, command: &SystemCommandV1) -> Hash {
        let mut payload = Vec::new();
        payload.extend_from_slice(SYSTEM_OP_REQUEST_DOMAIN_V1);
        payload.extend_from_slice(sender);
        payload.extend_from_slice(&nonce.to_le_bytes());
        payload.extend_from_slice(&command.encode());
        blake2_256(&payload)
    }

    pub fn request_id_matches(&self) -> bool {
        self.request_id == Self::compute_request_id(&self.sender, self.nonce, &self.command)
    }
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum SystemCommandV1 {
    CreateService {
        controller: [u8; 32],
        code_hash: Hash,
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    },
    UpgradeService {
        controller: [u8; 32],
        service_id: u32,
        code_hash: Hash,
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    },
}

impl DecodeWithMemTracking for SystemCommandV1 {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum SystemReceiptV1 {
    ServiceCreated {
        service_id: u32,
        controller: [u8; 32],
    },
    ServiceUpgraded {
        service_id: u32,
        controller: [u8; 32],
        code_hash: Hash,
    },
    Rejected {
        code: u32,
    },
}

impl DecodeWithMemTracking for SystemReceiptV1 {}

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
pub struct WorkerSignature {
    pub worker_id: WorkerId,
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct ReportEnvelopeV1 {
    pub protocol_version: u16,
    pub chain_id: ChainId,
    pub work_id: WorkId,
    pub assignment_round: AssignmentRound,
    pub canonical_report: CanonicalReportBytes,
    pub canonical_report_hash: Hash,
    pub projected_metadata: ReportMetadataV1,
    pub bulletin_evidence: BulletinEvidence,
    pub signatures: ReportSignatures,
}

impl ReportEnvelopeV1 {
    pub fn computed_report_hash(&self) -> Hash {
        blake2_256(&self.canonical_report)
    }

    pub fn signing_hash(&self, assignment_epoch: EpochIndex, deadline: BlockNumber) -> Hash {
        let payload = (
            b"minijam/report-v1".as_slice(),
            self.protocol_version,
            self.chain_id,
            self.work_id,
            self.assignment_round,
            assignment_epoch,
            deadline,
            self.canonical_report_hash,
            self.projected_metadata,
            &self.bulletin_evidence,
        )
            .encode();
        blake2_256(&payload)
    }
}

impl DecodeWithMemTracking for ReportEnvelopeV1 {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct WorkerTaskV1 {
    pub work_id: WorkId,
    pub round: AssignmentRound,
    pub assignment_epoch: EpochIndex,
    pub assigned_workers: WorkerVoteAssignments,
    pub candidate_producer: WorkerId,
    pub package_hash: Hash,
    pub canonical_work_package: CanonicalWorkPackageBytes,
    pub bundle_ref: ContentRef,
}

impl DecodeWithMemTracking for WorkerTaskV1 {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct WorkerVoteTaskV1 {
    pub work_id: WorkId,
    pub round: AssignmentRound,
    pub assignment_epoch: EpochIndex,
    pub candidate_report_hash: Hash,
    pub deadline: BlockNumber,
    pub assigned_workers: WorkerVoteAssignments,
    pub submitted_votes: WorkerVoteSubmissions,
}

impl DecodeWithMemTracking for WorkerVoteTaskV1 {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct WorkerVerificationTaskV1 {
    pub work_id: WorkId,
    pub round: AssignmentRound,
    pub assignment_epoch: EpochIndex,
    pub candidate_report_hash: Hash,
    pub candidate_report: CanonicalReportBytes,
    pub deadline: BlockNumber,
    pub assigned_workers: WorkerVoteAssignments,
    pub submitted_votes: WorkerVoteSubmissions,
    pub package_hash: Hash,
    pub canonical_work_package: CanonicalWorkPackageBytes,
    pub bundle_ref: ContentRef,
}

impl DecodeWithMemTracking for WorkerVerificationTaskV1 {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct MiniJamWorkBundleV1 {
    pub protocol_version: u16,
    pub package_hash: Hash,
    pub external_data: WorkBundleExternalData,
    pub import_segments: WorkBundleImportSegments,
    pub import_proofs: WorkBundleImportProofs,
}

impl MiniJamWorkBundleV1 {
    pub fn new(package_hash: Hash) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_V1,
            package_hash,
            external_data: Default::default(),
            import_segments: Default::default(),
            import_proofs: Default::default(),
        }
    }
}

impl DecodeWithMemTracking for MiniJamWorkBundleV1 {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum OpposeReason {
    InvalidRefine,
    MissingData,
    ContextMismatch,
    MalformedOutput,
    Other(Hash),
}

impl DecodeWithMemTracking for OpposeReason {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum Verdict {
    Support,
    Oppose(OpposeReason),
}

impl DecodeWithMemTracking for Verdict {}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct WorkerVoteV1 {
    pub work_id: WorkId,
    pub round: AssignmentRound,
    pub assignment_epoch: EpochIndex,
    pub candidate_report_hash: Hash,
    pub verdict: Verdict,
    pub deadline: BlockNumber,
    pub chain_id: ChainId,
    pub protocol_version: u16,
}

impl DecodeWithMemTracking for WorkerVoteV1 {}

impl WorkerVoteV1 {
    pub fn signing_hash(&self) -> Hash {
        let mut payload = b"minijam/worker-vote-v1".to_vec();
        payload.extend(self.encode());
        blake2_256(&payload)
    }
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
    fn system_op_request_id_commits_sender_nonce_and_command() {
        let command = SystemCommandV1::CreateService {
            controller: [8u8; 32],
            code_hash: [9u8; 32],
            code_len: 32,
            min_item_gas: 1,
            min_memo_gas: 2,
        };
        let op = SystemOpV1::new([1u8; 32], 7, command.clone());
        assert!(op.request_id_matches());

        let mut changed_nonce = op.clone();
        changed_nonce.nonce = 8;
        assert!(!changed_nonce.request_id_matches());

        assert_ne!(
            op.request_id,
            SystemOpV1::compute_request_id(&[2u8; 32], 7, &command)
        );
    }
}
