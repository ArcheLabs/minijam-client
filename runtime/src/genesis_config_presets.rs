// SPDX-License-Identifier: Apache-2.0

use alloc::{vec, vec::Vec};

use frame_support::build_struct_json_patch;
use serde_json::Value;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_genesis_builder::{self, PresetId};
use sp_keyring::Sr25519Keyring;

use crate::{
    AccountId, AuraConfig, Balance, BalancesConfig, GrandpaConfig, RuntimeGenesisConfig,
    SudoConfig, UNIT,
};

const DEV_BALANCE: Balance = 1_000_000 * UNIT;
const REWARD_POOL_BALANCE: Balance = 1_000_000 * UNIT;

fn testnet_genesis(
    initial_authorities: Vec<(AuraId, GrandpaId)>,
    mut endowed_accounts: Vec<AccountId>,
    root: AccountId,
) -> Value {
    let reward_pool = AccountId::new([9; 32]);
    if !endowed_accounts
        .iter()
        .any(|account| account == &reward_pool)
    {
        endowed_accounts.push(reward_pool.clone());
    }

    build_struct_json_patch!(RuntimeGenesisConfig {
        balances: BalancesConfig {
            balances: endowed_accounts
                .iter()
                .cloned()
                .map(|account| {
                    let balance = if account == reward_pool {
                        REWARD_POOL_BALANCE
                    } else {
                        DEV_BALANCE
                    };
                    (account, balance)
                })
                .collect::<Vec<_>>(),
        },
        aura: AuraConfig {
            authorities: initial_authorities
                .iter()
                .map(|authority| authority.0.clone())
                .collect::<Vec<_>>(),
        },
        grandpa: GrandpaConfig {
            authorities: initial_authorities
                .iter()
                .map(|authority| (authority.1.clone(), 1))
                .collect::<Vec<_>>(),
        },
        sudo: SudoConfig { key: Some(root) },
    })
}

pub fn development_config_genesis() -> Value {
    testnet_genesis(
        vec![(
            Sr25519Keyring::Alice.public().into(),
            sp_keyring::Ed25519Keyring::Alice.public().into(),
        )],
        vec![
            Sr25519Keyring::Alice.to_account_id(),
            Sr25519Keyring::Bob.to_account_id(),
            Sr25519Keyring::Charlie.to_account_id(),
            Sr25519Keyring::Dave.to_account_id(),
            Sr25519Keyring::Eve.to_account_id(),
            Sr25519Keyring::Ferdie.to_account_id(),
            Sr25519Keyring::AliceStash.to_account_id(),
            Sr25519Keyring::BobStash.to_account_id(),
        ],
        Sr25519Keyring::Alice.to_account_id(),
    )
}

pub fn local_config_genesis() -> Value {
    testnet_genesis(
        vec![
            (
                Sr25519Keyring::Alice.public().into(),
                sp_keyring::Ed25519Keyring::Alice.public().into(),
            ),
            (
                Sr25519Keyring::Bob.public().into(),
                sp_keyring::Ed25519Keyring::Bob.public().into(),
            ),
        ],
        Sr25519Keyring::iter()
            .filter(|key| key != &Sr25519Keyring::One && key != &Sr25519Keyring::Two)
            .map(|key| key.to_account_id())
            .collect::<Vec<_>>(),
        Sr25519Keyring::Alice.to_account_id(),
    )
}

pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
    let patch = match id.as_ref() {
        sp_genesis_builder::DEV_RUNTIME_PRESET => development_config_genesis(),
        sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => local_config_genesis(),
        _ => return None,
    };

    Some(
        serde_json::to_string(&patch)
            .expect("genesis config JSON serialization must succeed")
            .into_bytes(),
    )
}

pub fn preset_names() -> Vec<PresetId> {
    vec![
        PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
        PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
    ]
}
