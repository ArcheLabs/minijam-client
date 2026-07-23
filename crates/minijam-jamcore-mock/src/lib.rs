// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

use bounded_collections::BoundedVec;
use minijam_jamcore_api::{
    InvariantError, MiniJamError, MiniJamExecutionInput, MiniJamExecutionOutput, MiniJamExecutor,
    ProtocolStateReader, StateError,
};
use minijam_protocol::{ProtocolStateChange, StateOperation};

#[derive(Clone, Debug)]
pub enum MockMode {
    Empty,
    Upsert {
        key: [u8; 31],
        value: BoundedVec<u8, bounded_collections::ConstU32<1_048_576>>,
    },
    StateError,
    InvariantError,
}

#[derive(Clone, Debug)]
pub struct MockExecutor {
    pub mode: MockMode,
}

impl Default for MockExecutor {
    fn default() -> Self {
        Self {
            mode: MockMode::Empty,
        }
    }
}

impl MiniJamExecutor for MockExecutor {
    fn execute<R: ProtocolStateReader>(
        &self,
        _input: MiniJamExecutionInput,
        _state: &R,
    ) -> Result<MiniJamExecutionOutput, MiniJamError> {
        match &self.mode {
            MockMode::Empty => Ok(MiniJamExecutionOutput::empty()),
            MockMode::Upsert { key, value } => {
                let mut output = MiniJamExecutionOutput::empty();
                output
                    .ordered_changes
                    .try_push(ProtocolStateChange {
                        key: *key,
                        operation: StateOperation::Upsert,
                        value: Some(value.clone()),
                    })
                    .map_err(|_| MiniJamError::Invariant(InvariantError::DuplicateKey))?;
                output.receipt_hash = output.compute_receipt_hash();
                Ok(output)
            }
            MockMode::StateError => Err(MiniJamError::State(StateError::MissingState)),
            MockMode::InvariantError => Err(MiniJamError::Invariant(InvariantError::DuplicateKey)),
        }
    }
}
