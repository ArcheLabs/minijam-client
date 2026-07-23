// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use minijam_protocol::{
    blake2_256, ConsumedPreimages, ConsumedReports, ConsumedSystemOps, Hash, PreimageBatch,
    PreimageMetadataV1, ProtocolStateChange, ReportBatch, StateChanges, StateOperation,
    SystemOpBatch, PROTOCOL_VERSION_V1,
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub const INTERFACE_VERSION: u16 = 1;
pub const EXECUTION_RECEIPT_DOMAIN_V2: &[u8] = b"minijam/execution-receipt/v2";

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq, TypeInfo)]
pub struct ServiceResultProjection {
    pub service_id: u32,
    pub code_hash: Hash,
    pub refine_gas_used: u64,
    pub accumulate_gas: u64,
}

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq, TypeInfo)]
pub struct ReportProjectionV1 {
    pub package_hash: Hash,
    pub context_hash: Hash,
    pub exports_root: Hash,
    pub result_count: u32,
    pub services: Vec<ServiceResultProjection>,
    pub total_refine_gas: u64,
    pub total_accumulate_gas: u64,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct MiniJamExecutionInputV1 {
    pub protocol_version: u16,
    pub slot: u32,
    pub parent_hash: Hash,
    pub parent_state_root: Hash,
    pub entropy: Hash,
    pub reports: ReportBatch,
    pub preimages: PreimageBatch,
    pub max_gas: u64,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct MiniJamExecutionInputV2 {
    pub protocol_version: u16,
    pub slot: u32,
    pub parent_hash: Hash,
    pub parent_state_root: Hash,
    pub entropy: Hash,
    pub reports: ReportBatch,
    pub preimages: PreimageBatch,
    pub system_ops: SystemOpBatch,
    pub max_gas: u64,
}

impl From<MiniJamExecutionInputV1> for MiniJamExecutionInputV2 {
    fn from(input: MiniJamExecutionInputV1) -> Self {
        Self {
            protocol_version: input.protocol_version,
            slot: input.slot,
            parent_hash: input.parent_hash,
            parent_state_root: input.parent_state_root,
            entropy: input.entropy,
            reports: input.reports,
            preimages: input.preimages,
            system_ops: Default::default(),
            max_gas: input.max_gas,
        }
    }
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct MiniJamExecutionOutputV1 {
    pub ordered_changes: StateChanges,
    pub consumed_reports: ConsumedReports,
    pub consumed_preimages: ConsumedPreimages,
    pub header_hash: Hash,
    pub accumulate_root: Hash,
    pub gas_used: u64,
    pub receipt_hash: Hash,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct MiniJamExecutionOutputV2 {
    pub ordered_changes: StateChanges,
    pub consumed_reports: ConsumedReports,
    pub consumed_preimages: ConsumedPreimages,
    pub consumed_system_ops: ConsumedSystemOps,
    pub input_hash: Hash,
    pub header_hash: Hash,
    pub accumulate_root: Hash,
    pub gas_used: u64,
    pub receipt_hash: Hash,
}

impl MiniJamExecutionOutputV1 {
    pub fn empty() -> Self {
        let mut output = Self {
            ordered_changes: Default::default(),
            consumed_reports: Default::default(),
            consumed_preimages: Default::default(),
            header_hash: [0u8; 32],
            accumulate_root: [0u8; 32],
            gas_used: 0,
            receipt_hash: [0u8; 32],
        };
        output.receipt_hash = output.compute_receipt_hash();
        output
    }

    pub fn compute_receipt_hash(&self) -> Hash {
        blake2_256(
            &(
                &self.ordered_changes,
                &self.consumed_reports,
                &self.consumed_preimages,
                self.header_hash,
                self.accumulate_root,
                self.gas_used,
            )
                .encode(),
        )
    }
}

impl MiniJamExecutionOutputV2 {
    pub fn empty() -> Self {
        let mut output = Self {
            ordered_changes: Default::default(),
            consumed_reports: Default::default(),
            consumed_preimages: Default::default(),
            consumed_system_ops: Default::default(),
            input_hash: [0u8; 32],
            header_hash: [0u8; 32],
            accumulate_root: [0u8; 32],
            gas_used: 0,
            receipt_hash: [0u8; 32],
        };
        output.receipt_hash = output.compute_receipt_hash();
        output
    }

    pub fn compute_receipt_hash(&self) -> Hash {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(EXECUTION_RECEIPT_DOMAIN_V2);
        encoded.extend_from_slice(&self.input_hash);
        append_state_changes(&mut encoded, &self.ordered_changes);
        append_hashes(&mut encoded, &self.consumed_reports);
        append_hashes(&mut encoded, &self.consumed_preimages);
        append_hashes(&mut encoded, &self.consumed_system_ops);
        encoded.extend_from_slice(&self.header_hash);
        encoded.extend_from_slice(&self.accumulate_root);
        encoded.extend_from_slice(&self.gas_used.to_le_bytes());
        blake2_256(&encoded)
    }
}

impl TryFrom<MiniJamExecutionOutputV2> for MiniJamExecutionOutputV1 {
    type Error = MiniJamError;

    fn try_from(output: MiniJamExecutionOutputV2) -> Result<Self, Self::Error> {
        if !output.consumed_system_ops.is_empty() {
            return Err(MiniJamError::Input(InputError::UnsupportedVersion));
        }
        let mut v1 = Self {
            ordered_changes: output.ordered_changes,
            consumed_reports: output.consumed_reports,
            consumed_preimages: output.consumed_preimages,
            header_hash: output.header_hash,
            accumulate_root: output.accumulate_root,
            gas_used: output.gas_used,
            receipt_hash: [0u8; 32],
        };
        v1.receipt_hash = v1.compute_receipt_hash();
        Ok(v1)
    }
}

impl From<MiniJamExecutionOutputV1> for MiniJamExecutionOutputV2 {
    fn from(output: MiniJamExecutionOutputV1) -> Self {
        let mut v2 = Self {
            ordered_changes: output.ordered_changes,
            consumed_reports: output.consumed_reports,
            consumed_preimages: output.consumed_preimages,
            consumed_system_ops: Default::default(),
            input_hash: [0u8; 32],
            header_hash: output.header_hash,
            accumulate_root: output.accumulate_root,
            gas_used: output.gas_used,
            receipt_hash: [0u8; 32],
        };
        v2.receipt_hash = v2.compute_receipt_hash();
        v2
    }
}

fn append_hashes(out: &mut Vec<u8>, hashes: &[Hash]) {
    out.extend_from_slice(&(hashes.len() as u32).to_le_bytes());
    for hash in hashes {
        out.extend_from_slice(hash);
    }
}

fn append_state_changes(out: &mut Vec<u8>, changes: &[ProtocolStateChange]) {
    out.extend_from_slice(&(changes.len() as u32).to_le_bytes());
    for change in changes {
        out.extend_from_slice(&change.key);
        out.push(match change.operation {
            StateOperation::Upsert => 0,
            StateOperation::Update => 1,
            StateOperation::Remove => 2,
        });
        match &change.value {
            Some(value) => {
                out.push(1);
                out.extend_from_slice(&(value.len() as u32).to_le_bytes());
                out.extend_from_slice(value);
            }
            None => out.push(0),
        }
    }
}

#[derive(
    Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
)]
pub enum MiniJamError {
    Input(InputError),
    Execution(ExecutionOutcome),
    State(StateError),
    Invariant(InvariantError),
}

#[derive(
    Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
)]
pub enum InputError {
    UnsupportedVersion,
    InvalidReportEncoding,
    InvalidPreimageEncoding,
    ReportHashMismatch,
    PreimageHashMismatch,
    MetadataMismatch,
    LimitExceeded,
}

#[derive(
    Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
)]
pub enum ExecutionOutcome {
    OutOfGas,
    Trap,
    ServiceFailure,
}

#[derive(
    Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
)]
pub enum StateError {
    MissingState,
    Decode,
    InvalidKey,
    InvalidOperation,
}

#[derive(
    Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
)]
pub enum InvariantError {
    DuplicateKey,
    UnsortedChanges,
    ReceiptMismatch,
}

pub trait ProtocolStateReader {
    fn get(&self, key: &[u8; 31]) -> Result<Option<Vec<u8>>, StateError>;
}

pub trait MiniJamExecutor {
    fn execute<R: ProtocolStateReader>(
        &self,
        input: MiniJamExecutionInputV1,
        state: &R,
    ) -> Result<MiniJamExecutionOutputV1, MiniJamError>;

    fn execute_v2<R: ProtocolStateReader>(
        &self,
        input: MiniJamExecutionInputV2,
        state: &R,
    ) -> Result<MiniJamExecutionOutputV2, MiniJamError> {
        if !input.system_ops.is_empty() {
            return Err(MiniJamError::Input(InputError::UnsupportedVersion));
        }
        let v1 = MiniJamExecutionInputV1 {
            protocol_version: PROTOCOL_VERSION_V1,
            slot: input.slot,
            parent_hash: input.parent_hash,
            parent_state_root: input.parent_state_root,
            entropy: input.entropy,
            reports: input.reports,
            preimages: input.preimages,
            max_gas: input.max_gas,
        };
        self.execute(v1, state).map(Into::into)
    }

    fn validate_preimage_submission<R: ProtocolStateReader>(
        &self,
        _bytes: &[u8],
        _state: &R,
    ) -> Result<PreimageMetadataV1, MiniJamError> {
        Err(MiniJamError::Input(InputError::InvalidPreimageEncoding))
    }

    fn project_report(&self, _bytes: &[u8]) -> Result<ReportProjectionV1, MiniJamError> {
        Err(MiniJamError::Input(InputError::InvalidReportEncoding))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopMiniJamExecutor;

impl MiniJamExecutor for NoopMiniJamExecutor {
    fn execute<R: ProtocolStateReader>(
        &self,
        _input: MiniJamExecutionInputV1,
        _state: &R,
    ) -> Result<MiniJamExecutionOutputV1, MiniJamError> {
        Ok(MiniJamExecutionOutputV1::empty())
    }
}

pub fn normalize_changes(changes: &mut [ProtocolStateChange]) -> Result<(), InvariantError> {
    changes.sort_by_key(|change| change.key);
    if changes.windows(2).any(|pair| pair[0].key == pair[1].key) {
        return Err(InvariantError::DuplicateKey);
    }
    Ok(())
}

pub fn interface_hash() -> Hash {
    blake2_256(b"minijam-jamcore-api/v1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijam_protocol::SystemOpV1;

    #[test]
    fn v2_receipt_commits_input_hash_and_system_ops() {
        let mut a = MiniJamExecutionOutputV2::empty();
        let mut b = MiniJamExecutionOutputV2::empty();
        a.input_hash = [1u8; 32];
        b.input_hash = [2u8; 32];
        assert_ne!(a.compute_receipt_hash(), b.compute_receipt_hash());

        a.input_hash = [0u8; 32];
        b.input_hash = [0u8; 32];
        a.consumed_system_ops
            .try_push(
                SystemOpV1::new(
                    [1u8; 32],
                    0,
                    minijam_protocol::SystemCommandV1::CreateService {
                        code_hash: [9u8; 32],
                        code_len: 1,
                        min_item_gas: 1,
                        min_memo_gas: 1,
                        initial_balance: 1,
                    },
                )
                .request_id,
            )
            .unwrap();
        assert_ne!(a.compute_receipt_hash(), b.compute_receipt_hash());
    }
}
