// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use bounded_collections::{BoundedVec, ConstU32};
use parity_scale_codec::DecodeWithMemTracking;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub const PROTOCOL_VERSION_V1: u16 = 1;
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
pub const WORK_DEPOSIT: u128 = 10 * UNIT;
pub const CANDIDATE_BOND: u128 = 10 * UNIT;
pub const TIMELY_VOTE_REWARD: u128 = UNIT;
pub const ACCEPTED_SUBMITTER_REWARD: u128 = UNIT;
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
pub type BulletinProofBytes = BoundedVec<u8, ConstU32<65_536>>;
pub type ReportSignatures = BoundedVec<WorkerSignature, ConstU32<8>>;
pub type StateValue = BoundedVec<u8, ConstU32<1_048_576>>;
pub type StateChanges = BoundedVec<ProtocolStateChange, ConstU32<4_096>>;
pub type ReportBatch = BoundedVec<CanonicalReportBytes, ConstU32<4>>;
pub type ConsumedReports = BoundedVec<Hash, ConstU32<4>>;
pub type ServiceOutputs = BoundedVec<ServiceOutput, ConstU32<1_024>>;
pub type BridgeEffects = BoundedVec<BridgeEffect, ConstU32<1_024>>;

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

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum ProtocolNamespace {
    System,
    ServiceInfo,
    ServiceStorage,
    ServiceLookup,
    Preimage,
    AdminBridge,
}

impl ProtocolNamespace {
    pub fn from_key(key: &[u8; 31]) -> Option<Self> {
        match key[0] {
            NS_SYSTEM => Some(Self::System),
            NS_SERVICE_INFO => Some(Self::ServiceInfo),
            NS_SERVICE_STORAGE => Some(Self::ServiceStorage),
            NS_SERVICE_LOOKUP => Some(Self::ServiceLookup),
            NS_PREIMAGE => Some(Self::Preimage),
            NS_ADMIN_BRIDGE => Some(Self::AdminBridge),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct ProtocolStateChange {
    pub key: [u8; 31],
    pub operation: StateOperation,
    pub value: Option<StateValue>,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct ServiceOutput {
    pub service_id: u32,
    pub output_hash: Hash,
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
