// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use minijam_protocol::{
    blake2_256, ConsumedPreimages, ConsumedReports, ConsumedSystemOps, Hash, PreimageBatch,
    PreimageMetadataV1, ProtocolStateChange, ReportBatch, StateChanges, StateOperation,
    SystemOpBatch,
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub const INTERFACE_VERSION: u16 = 1;
pub const EXECUTION_RECEIPT_DOMAIN: &[u8] = b"minijam/execution-receipt/v1";
pub const BLOCK_INPUT_DOMAIN: &[u8] = b"minijam/block-input/v1";

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
pub struct MiniJamExecutionInput {
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

impl MiniJamExecutionInput {
    pub fn compute_input_hash(&self) -> Hash {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(BLOCK_INPUT_DOMAIN);
        encoded.extend_from_slice(&self.protocol_version.to_le_bytes());
        encoded.extend_from_slice(&self.slot.to_le_bytes());
        encoded.extend_from_slice(&self.parent_hash);
        encoded.extend_from_slice(&self.parent_state_root);
        encoded.extend_from_slice(&self.entropy);
        encoded.extend_from_slice(&(self.reports.len() as u32).to_le_bytes());
        for report in &self.reports {
            append_len_prefixed_bytes(&mut encoded, report);
        }
        encoded.extend_from_slice(&(self.preimages.len() as u32).to_le_bytes());
        for preimage in &self.preimages {
            append_len_prefixed_bytes(&mut encoded, preimage);
        }
        encoded.extend_from_slice(&(self.system_ops.len() as u32).to_le_bytes());
        for op in &self.system_ops {
            append_len_prefixed_bytes(&mut encoded, &op.encode());
        }
        encoded.extend_from_slice(&self.max_gas.to_le_bytes());
        blake2_256(&encoded)
    }
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct MiniJamExecutionOutput {
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

impl MiniJamExecutionOutput {
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
        encoded.extend_from_slice(EXECUTION_RECEIPT_DOMAIN);
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

fn append_hashes(out: &mut Vec<u8>, hashes: &[Hash]) {
    out.extend_from_slice(&(hashes.len() as u32).to_le_bytes());
    for hash in hashes {
        out.extend_from_slice(hash);
    }
}

fn append_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
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
        input: MiniJamExecutionInput,
        state: &R,
    ) -> Result<MiniJamExecutionOutput, MiniJamError>;

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
        _input: MiniJamExecutionInput,
        _state: &R,
    ) -> Result<MiniJamExecutionOutput, MiniJamError> {
        Ok(MiniJamExecutionOutput::empty())
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
    fn receipt_commits_input_hash_and_system_ops() {
        let mut a = MiniJamExecutionOutput::empty();
        let mut b = MiniJamExecutionOutput::empty();
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
