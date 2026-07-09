// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

use bounded_collections::BoundedVec;
use minijam_jamcore_api::{
    InvariantError, MiniJamError, MiniJamExecutionInputV1, MiniJamExecutionOutputV1,
    MiniJamExecutor, ProtocolStateReader, StateError,
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
        _input: MiniJamExecutionInputV1,
        _state: &R,
    ) -> Result<MiniJamExecutionOutputV1, MiniJamError> {
        match &self.mode {
            MockMode::Empty => Ok(MiniJamExecutionOutputV1::empty()),
            MockMode::Upsert { key, value } => {
                let mut output = MiniJamExecutionOutputV1::empty();
                output
                    .ordered_changes
                    .try_push(ProtocolStateChange {
                        key: *key,
                        operation: StateOperation::Upsert,
                        value: Some(value.clone()),
                    })
                    .map_err(|_| MiniJamError::Invariant(InvariantError::ForbiddenChange))?;
                output.receipt_hash = output.compute_receipt_hash();
                Ok(output)
            }
            MockMode::StateError => Err(MiniJamError::State(StateError::MissingState)),
            MockMode::InvariantError => {
                Err(MiniJamError::Invariant(InvariantError::ForbiddenChange))
            }
        }
    }
}
