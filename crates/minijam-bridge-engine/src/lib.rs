// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use minijam_protocol::{AssetId, BridgeEffect, StateValue, NS_ADMIN_BRIDGE};
use parity_scale_codec::Encode;

pub type AccountId = [u8; 32];
pub const BRIDGE_SCHEMA_VERSION: u16 = 1;
pub const BRIDGE_DIRECTION_INBOUND: u8 = 0;
pub const BRIDGE_DIRECTION_OUTBOUND: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BridgeError {
    ZeroAmount,
    InsufficientEscrow,
    NonceConsumed,
    UnexpectedNonce,
    Overflow,
    ValueTooLarge,
}

#[derive(Clone, Debug, Default)]
pub struct BridgeLedger {
    next_inbound_nonce: u64,
    escrow: BTreeMap<AccountId, u128>,
    consumed_outbound: BTreeSet<u64>,
}

pub fn admin_bridge_key(effect: &BridgeEffect) -> [u8; 31] {
    let (direction, nonce) = match effect {
        BridgeEffect::Inbound { nonce, .. } => (BRIDGE_DIRECTION_INBOUND, *nonce),
        BridgeEffect::Outbound { nonce, .. } => (BRIDGE_DIRECTION_OUTBOUND, *nonce),
    };
    let mut key = [0u8; 31];
    key[0] = NS_ADMIN_BRIDGE;
    key[1] = direction;
    key[23..31].copy_from_slice(&nonce.to_be_bytes());
    key
}

pub fn admin_bridge_value(effect: &BridgeEffect) -> Result<StateValue, BridgeError> {
    StateValue::try_from((BRIDGE_SCHEMA_VERSION, effect).encode())
        .map_err(|_| BridgeError::ValueTooLarge)
}

pub trait BridgeAdminRecordSource {
    fn drain_admin_records(limit: u32) -> Result<Vec<([u8; 31], StateValue)>, BridgeError>;
}

impl BridgeAdminRecordSource for () {
    fn drain_admin_records(_: u32) -> Result<Vec<([u8; 31], StateValue)>, BridgeError> {
        Ok(Vec::new())
    }
}

impl BridgeLedger {
    pub fn next_inbound_nonce(&self) -> u64 {
        self.next_inbound_nonce
    }

    pub fn escrowed(&self, account: &AccountId) -> u128 {
        self.escrow.get(account).copied().unwrap_or(0)
    }

    pub fn prepare_inbound(
        &mut self,
        account: AccountId,
        target_service: u32,
        amount: u128,
    ) -> Result<BridgeEffect, BridgeError> {
        if amount == 0 {
            return Err(BridgeError::ZeroAmount);
        }
        let nonce = self.next_inbound_nonce;
        let next_nonce = self
            .next_inbound_nonce
            .checked_add(1)
            .ok_or(BridgeError::UnexpectedNonce)?;
        let new_balance = self
            .escrowed(&account)
            .checked_add(amount)
            .ok_or(BridgeError::Overflow)?;
        self.next_inbound_nonce = next_nonce;
        self.escrow.insert(account, new_balance);
        Ok(BridgeEffect::Inbound {
            nonce,
            target_service,
            asset: AssetId::Native,
            amount,
            account,
        })
    }

    pub fn apply_outbound(
        &mut self,
        nonce: u64,
        account: AccountId,
        source_service: u32,
        amount: u128,
    ) -> Result<BridgeEffect, BridgeError> {
        if amount == 0 {
            return Err(BridgeError::ZeroAmount);
        }
        if self.consumed_outbound.contains(&nonce) {
            return Err(BridgeError::NonceConsumed);
        }
        let escrow = self
            .escrow
            .get_mut(&account)
            .ok_or(BridgeError::InsufficientEscrow)?;
        if *escrow < amount {
            return Err(BridgeError::InsufficientEscrow);
        }
        *escrow -= amount;
        self.consumed_outbound.insert(nonce);
        Ok(BridgeEffect::Outbound {
            nonce,
            source_service,
            asset: AssetId::Native,
            amount,
            account,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_is_exactly_once_and_limited_by_escrow() {
        let account = [9u8; 32];
        let mut ledger = BridgeLedger::default();
        ledger.prepare_inbound(account, 7, 100).unwrap();
        ledger.apply_outbound(42, account, 7, 60).unwrap();
        assert_eq!(ledger.escrowed(&account), 40);
        assert_eq!(
            ledger.apply_outbound(42, account, 7, 1),
            Err(BridgeError::NonceConsumed)
        );
        assert_eq!(
            ledger.apply_outbound(43, account, 7, 41),
            Err(BridgeError::InsufficientEscrow)
        );
    }

    #[test]
    fn admin_bridge_keys_are_namespaced_and_ordered_by_nonce() {
        let first = BridgeEffect::Inbound {
            nonce: 1,
            target_service: 7,
            asset: AssetId::Native,
            amount: 100,
            account: [1; 32],
        };
        let second = BridgeEffect::Inbound {
            nonce: 2,
            target_service: 7,
            asset: AssetId::Native,
            amount: 100,
            account: [1; 32],
        };
        let first_key = admin_bridge_key(&first);
        let second_key = admin_bridge_key(&second);

        assert_eq!(first_key[0], NS_ADMIN_BRIDGE);
        assert_eq!(first_key[1], BRIDGE_DIRECTION_INBOUND);
        assert!(first_key < second_key);
        assert!(admin_bridge_value(&first).is_ok());
    }
}
