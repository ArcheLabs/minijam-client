// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use minijam_protocol::{
    blake2_256, ConsumedPreimages, ConsumedReports, Hash, PreimageBatch, PreimageMetadataV1,
    ProtocolStateChange, ReportBatch, StateChanges,
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub const INTERFACE_VERSION: u16 = 1;

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
pub struct MiniJamExecutionOutputV1 {
    pub ordered_changes: StateChanges,
    pub consumed_reports: ConsumedReports,
    pub consumed_preimages: ConsumedPreimages,
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

    fn validate_preimage_submission<R: ProtocolStateReader>(
        &self,
        _bytes: &[u8],
        _state: &R,
    ) -> Result<PreimageMetadataV1, MiniJamError> {
        Err(MiniJamError::Input(InputError::InvalidPreimageEncoding))
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
