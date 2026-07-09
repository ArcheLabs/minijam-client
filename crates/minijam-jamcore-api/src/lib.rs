// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

use minijam_protocol::{
    blake2_256, BridgeEffects, ConsumedReports, EpochIndex, Hash, ProtocolStateChange, ReportBatch,
    ServiceOutputs, StateChanges,
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub const INTERFACE_VERSION: u16 = 1;

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct MiniJamExecutionInputV1 {
    pub slot: u32,
    pub epoch: EpochIndex,
    pub reports: ReportBatch,
    pub max_gas: u64,
    pub protocol_version: u16,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct MiniJamExecutionOutputV1 {
    pub ordered_changes: StateChanges,
    pub consumed_reports: ConsumedReports,
    pub service_outputs: ServiceOutputs,
    pub bridge_effects: BridgeEffects,
    pub gas_used: u64,
    pub receipt_hash: Hash,
}

impl MiniJamExecutionOutputV1 {
    pub fn empty() -> Self {
        let mut output = Self {
            ordered_changes: Default::default(),
            consumed_reports: Default::default(),
            service_outputs: Default::default(),
            bridge_effects: Default::default(),
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
                &self.service_outputs,
                &self.bridge_effects,
                self.gas_used,
            )
                .encode(),
        )
    }
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum MiniJamError {
    Input(InputError),
    Execution(ExecutionOutcome),
    State(StateError),
    Invariant(InvariantError),
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum InputError {
    UnsupportedVersion,
    InvalidReportEncoding,
    ReportHashMismatch,
    MetadataMismatch,
    LimitExceeded,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum ExecutionOutcome {
    OutOfGas,
    Trap,
    ServiceFailure,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum StateError {
    MissingState,
    Decode,
    InvalidKey,
    InvalidOperation,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum InvariantError {
    DuplicateKey,
    UnsortedChanges,
    ForbiddenChange,
    ReceiptMismatch,
}

pub trait ProtocolStateReader {
    fn get(&self, key: &[u8; 31]) -> Result<Option<&[u8]>, StateError>;
}

pub trait MiniJamExecutor {
    fn execute<R: ProtocolStateReader>(
        &self,
        input: MiniJamExecutionInputV1,
        state: &R,
    ) -> Result<MiniJamExecutionOutputV1, MiniJamError>;
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
