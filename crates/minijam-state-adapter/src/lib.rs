// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};
use minijam_jamcore_api::{
    InvariantError, MiniJamExecutionInputV1, MiniJamExecutionOutputV1, ProtocolStateReader,
    StateError,
};
use minijam_protocol::{ProtocolNamespace, ProtocolStateChange, StateOperation, MAX_DELTA_BYTES};
use parity_scale_codec::Encode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedDelta {
    changes: Vec<ProtocolStateChange>,
}

impl ValidatedDelta {
    pub fn changes(&self) -> &[ProtocolStateChange] {
        &self.changes
    }

    pub fn into_changes(self) -> Vec<ProtocolStateChange> {
        self.changes
    }
}

/// Validate an executive result without mutating the underlying state.
///
/// FRAME integration must validate first and apply the returned changes inside
/// its storage transaction.
pub fn validate_execution_output<R: ProtocolStateReader>(
    input: &MiniJamExecutionInputV1,
    output: &MiniJamExecutionOutputV1,
    state: &R,
) -> Result<ValidatedDelta, ValidationError> {
    if output.gas_used > input.max_gas {
        return Err(ValidationError::GasExceeded);
    }
    if output.compute_receipt_hash() != output.receipt_hash {
        return Err(ValidationError::Invariant(InvariantError::ReceiptMismatch));
    }
    if output.ordered_changes.encoded_size() > MAX_DELTA_BYTES as usize {
        return Err(ValidationError::DeltaTooLarge);
    }

    let changes = output.ordered_changes.as_slice();
    if changes.windows(2).any(|pair| pair[0].key >= pair[1].key) {
        let error = if changes.windows(2).any(|pair| pair[0].key == pair[1].key) {
            InvariantError::DuplicateKey
        } else {
            InvariantError::UnsortedChanges
        };
        return Err(ValidationError::Invariant(error));
    }

    for change in changes {
        if ProtocolNamespace::from_key(&change.key).is_none() {
            return Err(ValidationError::Invariant(InvariantError::ForbiddenChange));
        }

        let exists = state.get(&change.key)?.is_some();
        match (change.operation, change.value.is_some(), exists) {
            (StateOperation::Upsert, true, false)
            | (StateOperation::Update, true, true)
            | (StateOperation::Remove, false, true) => {}
            _ => return Err(ValidationError::State(StateError::InvalidOperation)),
        }
    }

    Ok(ValidatedDelta {
        changes: changes.to_vec(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    GasExceeded,
    DeltaTooLarge,
    State(StateError),
    Invariant(InvariantError),
}

impl From<StateError> for ValidationError {
    fn from(value: StateError) -> Self {
        Self::State(value)
    }
}

/// Deterministic runtime-independent state used by adapter tests.
///
/// Applying a delta stages all writes in a clone and swaps it into place only
/// after every operation succeeds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryProtocolState {
    entries: BTreeMap<[u8; 31], Vec<u8>>,
}

impl MemoryProtocolState {
    pub fn insert(&mut self, key: [u8; 31], value: Vec<u8>) {
        self.entries.insert(key, value);
    }

    pub fn value(&self, key: &[u8; 31]) -> Option<&[u8]> {
        self.entries.get(key).map(Vec::as_slice)
    }

    pub fn apply_validated(&mut self, delta: ValidatedDelta) {
        let mut staged = self.entries.clone();
        for change in delta.changes {
            match change.operation {
                StateOperation::Upsert | StateOperation::Update => {
                    let value = change
                        .value
                        .expect("validated writes always contain a value");
                    staged.insert(change.key, value.into_inner());
                }
                StateOperation::Remove => {
                    staged.remove(&change.key);
                }
            }
        }
        self.entries = staged;
    }
}

impl ProtocolStateReader for MemoryProtocolState {
    fn get(&self, key: &[u8; 31]) -> Result<Option<Vec<u8>>, StateError> {
        Ok(self.value(key).map(ToOwned::to_owned))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijam_jamcore_api::MiniJamExecutionOutputV1;
    use minijam_protocol::{
        CanonicalReportBytes, ProtocolStateChange, ReportBatch, StateChanges, StateValue,
        NS_ADMIN_BRIDGE, NS_SERVICE_STORAGE, PROTOCOL_VERSION_V1,
    };

    fn key(namespace: u8, discriminator: u8) -> [u8; 31] {
        let mut key = [0u8; 31];
        key[0] = namespace;
        key[30] = discriminator;
        key
    }

    fn input(max_gas: u64) -> MiniJamExecutionInputV1 {
        MiniJamExecutionInputV1 {
            slot: 1,
            epoch: 0,
            reports: ReportBatch::try_from(Vec::<CanonicalReportBytes>::new()).unwrap(),
            max_gas,
            protocol_version: PROTOCOL_VERSION_V1,
        }
    }

    fn output(changes: Vec<ProtocolStateChange>) -> MiniJamExecutionOutputV1 {
        let mut output = MiniJamExecutionOutputV1::empty();
        output.ordered_changes = StateChanges::try_from(changes).unwrap();
        output.gas_used = 10;
        output.receipt_hash = output.compute_receipt_hash();
        output
    }

    fn value(byte: u8) -> StateValue {
        StateValue::try_from(vec![byte]).unwrap()
    }

    #[test]
    fn validates_and_applies_allowed_changes_atomically() {
        let existing = key(NS_SERVICE_STORAGE, 1);
        let inserted = key(NS_ADMIN_BRIDGE, 2);
        let mut state = MemoryProtocolState::default();
        state.insert(existing, vec![1]);

        let delta = validate_execution_output(
            &input(100),
            &output(vec![
                ProtocolStateChange {
                    key: existing,
                    operation: StateOperation::Update,
                    value: Some(value(2)),
                },
                ProtocolStateChange {
                    key: inserted,
                    operation: StateOperation::Upsert,
                    value: Some(value(3)),
                },
            ]),
            &state,
        )
        .unwrap();

        state.apply_validated(delta);
        assert_eq!(state.value(&existing), Some([2].as_slice()));
        assert_eq!(state.value(&inserted), Some([3].as_slice()));
    }

    #[test]
    fn rejects_forbidden_namespace_without_writes() {
        let forbidden = key(0x80, 1);
        let state = MemoryProtocolState::default();
        let before = state.clone();
        let result = validate_execution_output(
            &input(100),
            &output(vec![ProtocolStateChange {
                key: forbidden,
                operation: StateOperation::Upsert,
                value: Some(value(1)),
            }]),
            &state,
        );

        assert_eq!(
            result,
            Err(ValidationError::Invariant(InvariantError::ForbiddenChange))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn rejects_duplicate_unsorted_and_invalid_operations() {
        let first = key(NS_SERVICE_STORAGE, 1);
        let second = key(NS_SERVICE_STORAGE, 2);
        let state = MemoryProtocolState::default();

        let duplicate = output(vec![
            ProtocolStateChange {
                key: first,
                operation: StateOperation::Upsert,
                value: Some(value(1)),
            },
            ProtocolStateChange {
                key: first,
                operation: StateOperation::Upsert,
                value: Some(value(2)),
            },
        ]);
        assert_eq!(
            validate_execution_output(&input(100), &duplicate, &state),
            Err(ValidationError::Invariant(InvariantError::DuplicateKey))
        );

        let unsorted = output(vec![
            ProtocolStateChange {
                key: second,
                operation: StateOperation::Upsert,
                value: Some(value(1)),
            },
            ProtocolStateChange {
                key: first,
                operation: StateOperation::Upsert,
                value: Some(value(2)),
            },
        ]);
        assert_eq!(
            validate_execution_output(&input(100), &unsorted, &state),
            Err(ValidationError::Invariant(InvariantError::UnsortedChanges))
        );

        let remove_missing = output(vec![ProtocolStateChange {
            key: first,
            operation: StateOperation::Remove,
            value: None,
        }]);
        assert_eq!(
            validate_execution_output(&input(100), &remove_missing, &state),
            Err(ValidationError::State(StateError::InvalidOperation))
        );
    }

    #[test]
    fn rejects_receipt_and_gas_mismatch() {
        let state = MemoryProtocolState::default();
        let mut bad_receipt = output(Vec::new());
        bad_receipt.receipt_hash = [9u8; 32];
        assert_eq!(
            validate_execution_output(&input(100), &bad_receipt, &state),
            Err(ValidationError::Invariant(InvariantError::ReceiptMismatch))
        );

        let over_gas = output(Vec::new());
        assert_eq!(
            validate_execution_output(&input(9), &over_gas, &state),
            Err(ValidationError::GasExceeded)
        );
    }
}
