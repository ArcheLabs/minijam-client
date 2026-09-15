// SPDX-License-Identifier: Apache-2.0

use alloc::{vec, vec::Vec};

use frame_support::build_struct_json_patch;
use jambda_minijam_executive::{system_service_genesis_state, SystemServiceGenesisConfig};
use serde_json::Value;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{ed25519, sr25519};
use sp_genesis_builder::{self, PresetId};
use sp_keyring::Sr25519Keyring;

use crate::{
    AccountId, AuraConfig, Balance, BalancesConfig, GrandpaConfig, MiniJamConfig,
    RuntimeGenesisConfig, SudoConfig, UNIT,
};

const DEV_BALANCE: Balance = 1_000_000 * UNIT;
const SYSTEM_SERVICE_BLOB: &[u8] = include_bytes!("../../artifacts/system-service.blob");
pub const STAGE0_RUNTIME_PRESET: &str = "stage0";

/// Deterministic local-development account used as the single Worker account.
pub const LOCAL_WORKER_ACCOUNT: [u8; 32] = [
    0x90, 0x15, 0x78, 0xa4, 0x17, 0x30, 0x0a, 0xa0, 0xae, 0x53, 0x3b, 0x5b, 0xd0, 0xe9, 0xaf, 0x48,
    0x9a, 0x4c, 0xc4, 0xa6, 0xf3, 0x89, 0x99, 0xb7, 0x62, 0x83, 0x86, 0x70, 0x87, 0x73, 0x82, 0x09,
];

const STAGE0_AURA_AUTHORITIES: [[u8; 32]; 1] = [[
    0x66, 0xd0, 0x9c, 0xb4, 0xdf, 0xf3, 0x44, 0xd5, 0xa6, 0xb0, 0x7c, 0xa9, 0x90, 0x9d, 0xc0, 0x5f,
    0x46, 0xcc, 0xda, 0x66, 0x87, 0xc5, 0x2d, 0x7d, 0xad, 0x99, 0x83, 0xc7, 0xfe, 0x89, 0x16, 0x19,
]];
const STAGE0_GRANDPA_AUTHORITIES: [[u8; 32]; 1] = [[
    0x4e, 0x0c, 0xa8, 0x04, 0x2d, 0x49, 0xc5, 0x95, 0xcf, 0x51, 0x10, 0x3a, 0x96, 0x31, 0x3b, 0xcf,
    0x72, 0xb3, 0x7c, 0xc1, 0x78, 0xb7, 0x61, 0x53, 0x82, 0xe9, 0x35, 0x8f, 0xd9, 0x5b, 0xee, 0x0d,
]];
const STAGE0_SUDO_ACCOUNT: [u8; 32] = [
    0x64, 0xda, 0x53, 0x90, 0x20, 0xcd, 0x74, 0x3f, 0xed, 0x81, 0xed, 0x5d, 0xe9, 0x22, 0xf0, 0xb3,
    0xe7, 0x76, 0x9b, 0xf3, 0xb7, 0x7a, 0x95, 0x3a, 0xf3, 0xc0, 0x77, 0x9e, 0xce, 0xfd, 0x7f, 0x23,
];
pub(crate) const STAGE0_FAUCET_ACCOUNT: [u8; 32] = [
    0x1a, 0x69, 0x04, 0x44, 0xd1, 0x60, 0xa1, 0xf6, 0x32, 0x81, 0x20, 0x3e, 0xde, 0x44, 0x9b, 0xa9,
    0x96, 0xc5, 0x60, 0xb7, 0x98, 0x0e, 0x40, 0x43, 0x75, 0x76, 0x5f, 0x2a, 0xea, 0xcd, 0x88, 0x6a,
];

fn testnet_genesis(
    initial_authorities: Vec<(AuraId, GrandpaId)>,
    mut endowed_accounts: Vec<AccountId>,
    root: AccountId,
    worker_account: AccountId,
    allocation_relayer: AccountId,
) -> Value {
    for account in [&worker_account, &allocation_relayer] {
        if !endowed_accounts.iter().any(|endowed| endowed == account) {
            endowed_accounts.push((*account).clone());
        }
    }
    build_struct_json_patch!(RuntimeGenesisConfig {
        balances: BalancesConfig {
            balances: endowed_accounts
                .into_iter()
                .map(|account| (account, DEV_BALANCE))
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
        mini_jam: MiniJamConfig {
            protocol_state: system_service_zero_protocol_state(),
            worker_account: Some(worker_account),
            allocation_relayer: Some(allocation_relayer),
            _phantom: Default::default(),
        },
    })
}

fn stage0_authorities() -> Vec<(AuraId, GrandpaId)> {
    STAGE0_AURA_AUTHORITIES
        .iter()
        .copied()
        .zip(STAGE0_GRANDPA_AUTHORITIES.iter().copied())
        .map(|(aura, grandpa)| {
            (
                AuraId::from(sr25519::Public::from_raw(aura)),
                GrandpaId::from(ed25519::Public::from_raw(grandpa)),
            )
        })
        .collect()
}

fn stage0_endowed_accounts() -> Vec<AccountId> {
    vec![
        AccountId::new(LOCAL_WORKER_ACCOUNT),
        AccountId::new(STAGE0_SUDO_ACCOUNT),
        AccountId::new(STAGE0_FAUCET_ACCOUNT),
    ]
}

pub(crate) fn system_service_zero_protocol_state() -> Vec<(Vec<u8>, Vec<u8>)> {
    system_service_genesis_state(SystemServiceGenesisConfig {
        code_blob: SYSTEM_SERVICE_BLOB.to_vec(),
        initial_balance: 1_000_000_000_000,
        min_item_gas: 1,
        min_memo_gas: 1,
        deposit_offset: 0,
        genesis_slot: 0,
        parent_service: 0,
    })
    .expect("system service 0 genesis state must be valid")
    .into_iter()
    .map(|(key, value)| (key.0.to_vec(), value))
    .collect()
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
        ],
        Sr25519Keyring::Alice.to_account_id(),
        AccountId::new(LOCAL_WORKER_ACCOUNT),
        AccountId::new(LOCAL_WORKER_ACCOUNT),
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
            .collect(),
        Sr25519Keyring::Alice.to_account_id(),
        AccountId::new(LOCAL_WORKER_ACCOUNT),
        AccountId::new(LOCAL_WORKER_ACCOUNT),
    )
}

pub fn stage0_config_genesis(worker_account: AccountId) -> Value {
    testnet_genesis(
        stage0_authorities(),
        stage0_endowed_accounts(),
        AccountId::new(STAGE0_SUDO_ACCOUNT),
        worker_account.clone(),
        worker_account,
    )
}

pub fn stage1_config_genesis(worker_account: AccountId, allocation_relayer: AccountId) -> Value {
    testnet_genesis(
        stage0_authorities(),
        stage0_endowed_accounts(),
        AccountId::new(STAGE0_SUDO_ACCOUNT),
        worker_account,
        allocation_relayer,
    )
}

pub fn stage1_direct_e2e_config_genesis(
    worker_account: AccountId,
    allocation_relayer: AccountId,
) -> Value {
    let mut accounts = stage0_endowed_accounts();
    accounts.extend([
        Sr25519Keyring::Alice.to_account_id(),
        Sr25519Keyring::Bob.to_account_id(),
        Sr25519Keyring::Charlie.to_account_id(),
    ]);
    testnet_genesis(
        vec![(
            Sr25519Keyring::Alice.public().into(),
            sp_keyring::Ed25519Keyring::Alice.public().into(),
        )],
        accounts,
        AccountId::new(STAGE0_SUDO_ACCOUNT),
        worker_account,
        allocation_relayer,
    )
}

pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
    let patch = match id.as_ref() {
        sp_genesis_builder::DEV_RUNTIME_PRESET => development_config_genesis(),
        sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => local_config_genesis(),
        STAGE0_RUNTIME_PRESET => return None,
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
