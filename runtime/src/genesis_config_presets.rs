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
    MiniJamWorkersConfig, RuntimeGenesisConfig, SudoConfig, UNIT,
};

const DEV_BALANCE: Balance = 1_000_000 * UNIT;
const REWARD_POOL_BALANCE: Balance = 1_000_000 * UNIT;
const SYSTEM_SERVICE_FUEL: Balance = 1_000 * UNIT;
const SYSTEM_SERVICE_BLOB: &[u8] = include_bytes!("../../artifacts/system-service.blob");
pub const STAGE0_RUNTIME_PRESET: &str = "stage0";

/// Known deterministic local-development identity derived from the seed `0x92` repeated 32 times.
/// Never use as a public Stage 0 Relayer.
pub const LOCAL_PLAYGROUND_RELAYER_ACCOUNT: [u8; 32] = [
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
const STAGE0_WORKER_ACCOUNTS: [[u8; 32]; 3] = [
    [
        0x32, 0x6c, 0x5d, 0x73, 0x92, 0x0e, 0x92, 0x46, 0x43, 0x86, 0xba, 0x7a, 0x3a, 0xc1, 0x95,
        0x22, 0xb8, 0x55, 0xed, 0xc9, 0x50, 0x88, 0xb2, 0x93, 0x3f, 0xa5, 0x47, 0x0f, 0xca, 0x94,
        0x1a, 0x0f,
    ],
    [
        0xdc, 0xf2, 0x8e, 0x11, 0x5d, 0x43, 0x96, 0x63, 0xa1, 0x2e, 0x8c, 0xba, 0x10, 0xe7, 0x6b,
        0xc6, 0xe9, 0x50, 0x34, 0x69, 0xad, 0x27, 0xab, 0x03, 0xe1, 0x14, 0x0d, 0xda, 0xc9, 0x31,
        0xcc, 0x53,
    ],
    [
        0xce, 0x5c, 0x3f, 0x32, 0x90, 0xa1, 0xac, 0x97, 0xf3, 0x14, 0x5d, 0x96, 0xc7, 0xa4, 0x9d,
        0x14, 0xa8, 0xb6, 0x70, 0xf4, 0x65, 0xa1, 0xde, 0x88, 0xd7, 0xc5, 0x20, 0xcb, 0x6d, 0x11,
        0x27, 0x3d,
    ],
];
const STAGE0_WORKER_SESSION_KEYS: [[u8; 32]; 3] = STAGE0_WORKER_ACCOUNTS;
pub(crate) const STAGE0_FAUCET_ACCOUNT: [u8; 32] = [
    0x1a, 0x69, 0x04, 0x44, 0xd1, 0x60, 0xa1, 0xf6, 0x32, 0x81, 0x20, 0x3e, 0xde, 0x44, 0x9b, 0xa9,
    0x96, 0xc5, 0x60, 0xb7, 0x98, 0x0e, 0x40, 0x43, 0x75, 0x76, 0x5f, 0x2a, 0xea, 0xcd, 0x88, 0x6a,
];
const STAGE0_SUDO_ACCOUNT: [u8; 32] = [
    0x64, 0xda, 0x53, 0x90, 0x20, 0xcd, 0x74, 0x3f, 0xed, 0x81, 0xed, 0x5d, 0xe9, 0x22, 0xf0, 0xb3,
    0xe7, 0x76, 0x9b, 0xf3, 0xb7, 0x7a, 0x95, 0x3a, 0xf3, 0xc0, 0x77, 0x9e, 0xce, 0xfd, 0x7f, 0x23,
];

fn testnet_genesis(
    initial_authorities: Vec<(AuraId, GrandpaId)>,
    mut endowed_accounts: Vec<AccountId>,
    root: AccountId,
    workers: Vec<(AccountId, [u8; 32], Balance)>,
    ingress_relayer: AccountId,
) -> Value {
    let reward_pool = AccountId::new([9; 32]);
    let fuel_escrow = AccountId::new([7; 32]);
    let playground_relayer = ingress_relayer;
    if !endowed_accounts
        .iter()
        .any(|account| account == &reward_pool)
    {
        endowed_accounts.push(reward_pool.clone());
    }
    if !endowed_accounts
        .iter()
        .any(|account| account == &fuel_escrow)
    {
        endowed_accounts.push(fuel_escrow.clone());
    }
    if !endowed_accounts
        .iter()
        .any(|account| account == &playground_relayer)
    {
        endowed_accounts.push(playground_relayer.clone());
    }

    build_struct_json_patch!(RuntimeGenesisConfig {
        balances: BalancesConfig {
            balances: endowed_accounts
                .iter()
                .cloned()
                .map(|account| {
                    let balance = if account == reward_pool {
                        REWARD_POOL_BALANCE
                    } else if account == fuel_escrow {
                        SYSTEM_SERVICE_FUEL
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
        mini_jam: MiniJamConfig {
            protocol_state: system_service_zero_protocol_state(),
            service_fuel: vec![(0, SYSTEM_SERVICE_FUEL)],
            ingress_relayer: Some(playground_relayer),
            _phantom: Default::default(),
        },
        mini_jam_workers: MiniJamWorkersConfig {
            workers,
            _phantom: Default::default(),
        },
    })
}

fn development_workers() -> Vec<(AccountId, [u8; 32], Balance)> {
    vec![
        (
            Sr25519Keyring::Alice.to_account_id(),
            Sr25519Keyring::Alice.public().0,
            1_000 * UNIT,
        ),
        (
            Sr25519Keyring::Bob.to_account_id(),
            Sr25519Keyring::Bob.public().0,
            1_000 * UNIT,
        ),
        (
            Sr25519Keyring::Charlie.to_account_id(),
            Sr25519Keyring::Charlie.public().0,
            1_000 * UNIT,
        ),
    ]
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

fn stage0_workers() -> Vec<(AccountId, [u8; 32], Balance)> {
    STAGE0_WORKER_ACCOUNTS
        .iter()
        .copied()
        .zip(STAGE0_WORKER_SESSION_KEYS.iter().copied())
        .map(|(account, session_key)| (AccountId::new(account), session_key, 1_000 * UNIT))
        .collect()
}

fn stage0_endowed_accounts() -> Vec<AccountId> {
    STAGE0_WORKER_ACCOUNTS
        .iter()
        .copied()
        .chain([STAGE0_SUDO_ACCOUNT, STAGE0_FAUCET_ACCOUNT])
        .map(AccountId::new)
        .collect()
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
            Sr25519Keyring::AliceStash.to_account_id(),
            Sr25519Keyring::BobStash.to_account_id(),
        ],
        Sr25519Keyring::Alice.to_account_id(),
        development_workers(),
        AccountId::new(LOCAL_PLAYGROUND_RELAYER_ACCOUNT),
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
        development_workers(),
        AccountId::new(LOCAL_PLAYGROUND_RELAYER_ACCOUNT),
    )
}

pub fn stage0_config_genesis(ingress_relayer: AccountId) -> Value {
    testnet_genesis(
        stage0_authorities(),
        stage0_endowed_accounts(),
        AccountId::new(STAGE0_SUDO_ACCOUNT),
        stage0_workers(),
        ingress_relayer,
    )
}

pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
    let patch = match id.as_ref() {
        sp_genesis_builder::DEV_RUNTIME_PRESET => development_config_genesis(),
        sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => local_config_genesis(),
        // Stage 0 must be constructed by the node with an explicit public Relayer key.
        // A runtime preset has no safe channel for that required input.
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
        PresetId::from(STAGE0_RUNTIME_PRESET),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use jam_codec::Decode as JamDecode;
    use jambda_minijam_executive::MiniJamExecutive;
    use jp_core_primitives::{crypto::OpaqueHash, types::ServiceInfo};
    use minijam_jamcore_api::{
        MiniJamExecutionInput, MiniJamExecutor, ProtocolStateReader, StateError,
    };
    use minijam_protocol::{SystemCommandV1, SystemOpV1, PROTOCOL_VERSION_V1};
    use std::collections::BTreeMap;

    struct TestProtocolState(BTreeMap<[u8; 31], Vec<u8>>);

    impl TestProtocolState {
        fn from_pairs(pairs: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
            Self(
                pairs
                    .into_iter()
                    .map(|(key, value)| {
                        let key: [u8; 31] =
                            key.try_into().expect("protocol state key must be 31 bytes");
                        (key, value)
                    })
                    .collect(),
            )
        }

        fn apply(&mut self, output: &minijam_jamcore_api::MiniJamExecutionOutput) {
            for change in &output.ordered_changes {
                match change.operation {
                    minijam_protocol::StateOperation::Upsert
                    | minijam_protocol::StateOperation::Update => {
                        self.0.insert(
                            change.key,
                            change
                                .value
                                .as_ref()
                                .expect("validated state write has a value")
                                .clone()
                                .into_inner(),
                        );
                    }
                    minijam_protocol::StateOperation::Remove => {
                        self.0.remove(&change.key);
                    }
                }
            }
        }
    }

    impl ProtocolStateReader for TestProtocolState {
        fn get(&self, key: &[u8; 31]) -> Result<Option<Vec<u8>>, StateError> {
            Ok(self.0.get(key).cloned())
        }
    }

    fn section<'a>(patch: &'a Value, snake: &str, camel: &str) -> &'a Value {
        patch
            .get(snake)
            .or_else(|| patch.get(camel))
            .unwrap_or_else(|| panic!("missing genesis section {snake}/{camel}"))
    }

    fn field<'a>(section: &'a Value, snake: &str, camel: &str) -> &'a Value {
        section
            .get(snake)
            .or_else(|| section.get(camel))
            .unwrap_or_else(|| panic!("missing genesis field {snake}/{camel}"))
    }

    fn contains_value(value: &Value, expected: &Value) -> bool {
        value == expected
            || match value {
                Value::Array(values) => values.iter().any(|value| contains_value(value, expected)),
                Value::Object(values) => {
                    values.values().any(|value| contains_value(value, expected))
                }
                _ => false,
            }
    }

    #[test]
    fn development_genesis_seeds_stage0_service_and_workers() {
        let patch = development_config_genesis();
        let mini_jam = section(&patch, "mini_jam", "miniJam");
        let mini_jam_workers = section(&patch, "mini_jam_workers", "miniJamWorkers");

        let protocol_state = field(mini_jam, "protocol_state", "protocolState")
            .as_array()
            .expect("protocol state must be a JSON array");
        assert!(
            !protocol_state.is_empty(),
            "service 0 protocol state must be present"
        );

        let service_fuel = field(mini_jam, "service_fuel", "serviceFuel")
            .as_array()
            .expect("service fuel must be a JSON array");
        assert!(
            service_fuel.iter().any(|entry| {
                entry
                    .as_array()
                    .is_some_and(|pair| pair.first() == Some(&Value::from(0)))
            }),
            "service 0 fuel must be present"
        );

        let workers = field(mini_jam_workers, "workers", "workers")
            .as_array()
            .expect("workers must be a JSON array");
        assert_eq!(workers.len(), 3);
        assert!(workers.iter().all(|entry| {
            entry.as_array().is_some_and(|worker| {
                worker.len() == 3 && worker.get(2) == Some(&Value::from(1_000 * UNIT))
            })
        }));
    }

    #[test]
    fn development_genesis_endows_reward_pool_and_fuel_escrow() {
        let patch = development_config_genesis();
        let balances = field(
            section(&patch, "balances", "balances"),
            "balances",
            "balances",
        )
        .as_array()
        .expect("balances must be a JSON array");

        let reward_pool = serde_json::to_value(AccountId::new([9; 32])).unwrap();
        let fuel_escrow = serde_json::to_value(AccountId::new([7; 32])).unwrap();
        assert!(
            balances.iter().any(|entry| entry
                .as_array()
                .is_some_and(|pair| pair.first() == Some(&reward_pool))),
            "reward pool account must be endowed"
        );
        assert!(
            balances.iter().any(|entry| entry
                .as_array()
                .is_some_and(|pair| pair.first() == Some(&fuel_escrow))),
            "fuel escrow account must be endowed"
        );
    }

    #[test]
    fn system_service_manifest_matches_embedded_blob() {
        let manifest: Value =
            serde_json::from_str(include_str!("../../artifacts/system-service.manifest.json"))
                .expect("system service manifest must be valid JSON");
        assert_eq!(
            manifest.get("artifact"),
            Some(&Value::from("system-service.blob"))
        );
        assert_eq!(
            manifest.get("byte_len"),
            Some(&Value::from(SYSTEM_SERVICE_BLOB.len() as u64))
        );
        assert!(!SYSTEM_SERVICE_BLOB.is_empty());
        jp_vm_predecode::to_af_and_c_blob(SYSTEM_SERVICE_BLOB)
            .expect("system service blob must be a valid PVM artifact");
    }

    #[test]
    fn system_ops_execute_through_real_jambda_executor() {
        let sender = [0x5a; 32];
        let command = SystemCommandV1::CreateService {
            controller: sender,
            code_hash: [0x9b; 32],
            code_len: 27,
            min_item_gas: 2,
            min_memo_gas: 3,
        };
        let op = SystemOpV1::new(sender, 0, command);
        let input = MiniJamExecutionInput {
            protocol_version: PROTOCOL_VERSION_V1,
            slot: 10,
            parent_hash: [1u8; 32],
            parent_state_root: [2u8; 32],
            entropy: [3u8; 32],
            reports: Default::default(),
            preimages: Default::default(),
            system_ops: vec![op.clone()]
                .try_into()
                .expect("single system op fits batch"),
            max_gas: 20_000_000,
        };
        let state = TestProtocolState::from_pairs(system_service_zero_protocol_state());

        let output = <MiniJamExecutive as MiniJamExecutor>::execute(
            &MiniJamExecutive,
            input.clone(),
            &state,
        )
        .expect("CreateService must execute through the MiniJamExecutor trait");

        assert_eq!(output.consumed_system_ops.as_slice(), &[op.request_id]);
        assert!(output.consumed_reports.is_empty());
        assert_eq!(output.input_hash, input.compute_input_hash());
        assert_eq!(output.receipt_hash, output.compute_receipt_hash());
        assert!(output.ordered_changes.iter().any(|change| {
            let Some(value) = change.value.as_ref() else {
                return false;
            };
            ServiceInfo::decode(&mut value.as_slice()).is_ok_and(|info| {
                info.code_hash == OpaqueHash([0x9b; 32])
                    && info.parent_service == 0
                    && info.balance > 0
            })
        }));
        assert!(output.ordered_changes.iter().any(|change| {
            change
                .value
                .as_ref()
                .is_some_and(|value| value.as_slice() == sender)
        }));

        let mut invalid_op = op.clone();
        invalid_op.request_id[0] ^= 0xff;
        let invalid_input = MiniJamExecutionInput {
            system_ops: vec![invalid_op]
                .try_into()
                .expect("single system op fits batch"),
            ..input.clone()
        };
        let invalid_result = <MiniJamExecutive as MiniJamExecutor>::execute(
            &MiniJamExecutive,
            invalid_input,
            &TestProtocolState::from_pairs(system_service_zero_protocol_state()),
        );
        assert!(
            invalid_result.is_err(),
            "invalid request id must be rejected"
        );
    }

    #[test]
    fn empty_blocks_cross_epoch_through_real_jambda_executor() {
        let mut state = TestProtocolState::from_pairs(system_service_zero_protocol_state());

        for slot in 1..=13 {
            let input = MiniJamExecutionInput {
                protocol_version: PROTOCOL_VERSION_V1,
                slot,
                parent_hash: [slot as u8; 32],
                parent_state_root: [(slot + 1) as u8; 32],
                entropy: [(slot + 2) as u8; 32],
                reports: Default::default(),
                preimages: Default::default(),
                system_ops: Default::default(),
                max_gas: 20_000_000,
            };

            let output =
                <MiniJamExecutive as MiniJamExecutor>::execute(&MiniJamExecutive, input, &state)
                    .unwrap_or_else(|error| {
                        panic!("empty block at slot {slot} must execute: {error:?}")
                    });
            for change in &output.ordered_changes {
                let exists = state.get(&change.key).unwrap().is_some();
                assert!(
                    matches!(
                        (change.operation, exists),
                        (minijam_protocol::StateOperation::Upsert, false)
                            | (minijam_protocol::StateOperation::Update, true)
                            | (minijam_protocol::StateOperation::Remove, true)
                    ),
                    "empty block change operation must match persisted state at slot {slot}"
                );
            }
            state.apply(&output);
        }
    }

    #[test]
    fn create_service_executes_after_epoch_transitions() {
        let mut state = TestProtocolState::from_pairs(system_service_zero_protocol_state());
        for slot in 1..=121 {
            let input = MiniJamExecutionInput {
                protocol_version: PROTOCOL_VERSION_V1,
                slot,
                parent_hash: [slot as u8; 32],
                parent_state_root: [(slot + 1) as u8; 32],
                entropy: [(slot + 2) as u8; 32],
                reports: Default::default(),
                preimages: Default::default(),
                system_ops: Default::default(),
                max_gas: 20_000_000,
            };
            let output =
                <MiniJamExecutive as MiniJamExecutor>::execute(&MiniJamExecutive, input, &state)
                    .unwrap_or_else(|error| panic!("empty block at slot {slot} failed: {error:?}"));
            state.apply(&output);
        }

        let sender = [0x5a; 32];
        let op = SystemOpV1::new(
            sender,
            0,
            SystemCommandV1::CreateService {
                controller: sender,
                code_hash: [0x9b; 32],
                code_len: 27,
                min_item_gas: 2,
                min_memo_gas: 3,
            },
        );
        let input = MiniJamExecutionInput {
            protocol_version: PROTOCOL_VERSION_V1,
            slot: 122,
            parent_hash: [122; 32],
            parent_state_root: [123; 32],
            entropy: [124; 32],
            reports: Default::default(),
            preimages: Default::default(),
            system_ops: vec![op].try_into().unwrap(),
            max_gas: 20_000_000,
        };
        <MiniJamExecutive as MiniJamExecutor>::execute(&MiniJamExecutive, input, &state)
            .expect("CreateService must execute after epoch transitions");
    }

    #[test]
    fn stage0_genesis_uses_release_network_identities() {
        let patch = stage0_config_genesis(AccountId::new([0x99; 32]));
        let aura = field(
            section(&patch, "aura", "aura"),
            "authorities",
            "authorities",
        )
        .as_array()
        .expect("aura authorities must be a JSON array");
        let grandpa = field(
            section(&patch, "grandpa", "grandpa"),
            "authorities",
            "authorities",
        )
        .as_array()
        .expect("grandpa authorities must be a JSON array");
        let workers = field(
            section(&patch, "mini_jam_workers", "miniJamWorkers"),
            "workers",
            "workers",
        )
        .as_array()
        .expect("workers must be a JSON array");

        assert_eq!(aura.len(), 1);
        assert_eq!(grandpa.len(), 1);
        assert_eq!(workers.len(), 3);
        assert_eq!(STAGE0_WORKER_ACCOUNTS, STAGE0_WORKER_SESSION_KEYS);

        let development_accounts = [
            Sr25519Keyring::Alice.to_account_id(),
            Sr25519Keyring::Bob.to_account_id(),
            Sr25519Keyring::Charlie.to_account_id(),
        ]
        .into_iter()
        .map(|account| serde_json::to_value(account).unwrap())
        .collect::<Vec<_>>();

        for worker in workers {
            let account = worker
                .as_array()
                .and_then(|entry| entry.first())
                .expect("worker account must be present");
            assert!(
                !development_accounts.contains(account),
                "stage0 worker accounts must not use development keyring accounts"
            );
        }

        for placeholder in [
            [0x41; 32], [0x42; 32], [0x43; 32], [0x51; 32], [0x52; 32], [0x53; 32], [0x61; 32],
            [0x62; 32], [0x63; 32], [0x64; 32], [0x65; 32], [0x71; 32], [0x72; 32], [0x73; 32],
            [0x74; 32], [0x75; 32],
        ] {
            assert!(!contains_value(
                &patch,
                &serde_json::to_value(AccountId::new(placeholder)).unwrap()
            ));
        }
    }

    #[test]
    fn development_and_local_genesis_use_only_the_known_local_relayer() {
        let local = serde_json::to_value(AccountId::new(LOCAL_PLAYGROUND_RELAYER_ACCOUNT)).unwrap();
        for patch in [development_config_genesis(), local_config_genesis()] {
            assert_eq!(
                field(
                    section(&patch, "mini_jam", "miniJam"),
                    "ingress_relayer",
                    "ingressRelayer"
                ),
                &local
            );
        }

        let stage0 = stage0_config_genesis(AccountId::new([0x42; 32]));
        assert_ne!(
            field(
                section(&stage0, "mini_jam", "miniJam"),
                "ingress_relayer",
                "ingressRelayer"
            ),
            &local
        );
    }

    #[test]
    fn stage0_genesis_uses_release_faucet_and_sudo_accounts() {
        const EXPECTED_FAUCET: [u8; 32] = [
            0x1a, 0x69, 0x04, 0x44, 0xd1, 0x60, 0xa1, 0xf6, 0x32, 0x81, 0x20, 0x3e, 0xde, 0x44,
            0x9b, 0xa9, 0x96, 0xc5, 0x60, 0xb7, 0x98, 0x0e, 0x40, 0x43, 0x75, 0x76, 0x5f, 0x2a,
            0xea, 0xcd, 0x88, 0x6a,
        ];
        const EXPECTED_SUDO: [u8; 32] = [
            0x64, 0xda, 0x53, 0x90, 0x20, 0xcd, 0x74, 0x3f, 0xed, 0x81, 0xed, 0x5d, 0xe9, 0x22,
            0xf0, 0xb3, 0xe7, 0x76, 0x9b, 0xf3, 0xb7, 0x7a, 0x95, 0x3a, 0xf3, 0xc0, 0x77, 0x9e,
            0xce, 0xfd, 0x7f, 0x23,
        ];

        assert_eq!(STAGE0_FAUCET_ACCOUNT, EXPECTED_FAUCET);
        assert_eq!(STAGE0_SUDO_ACCOUNT, EXPECTED_SUDO);
        assert_eq!(crate::FaucetAccount::get(), AccountId::new(EXPECTED_FAUCET));

        let patch = stage0_config_genesis(AccountId::new([0x99; 32]));
        let sudo = section(&patch, "sudo", "sudo");
        assert_eq!(
            field(sudo, "key", "key"),
            &serde_json::to_value(AccountId::new(EXPECTED_SUDO)).unwrap()
        );

        let balances = field(
            section(&patch, "balances", "balances"),
            "balances",
            "balances",
        )
        .as_array()
        .expect("balances must be a JSON array");
        for expected in [EXPECTED_FAUCET, EXPECTED_SUDO] {
            let account = serde_json::to_value(AccountId::new(expected)).unwrap();
            assert!(balances.iter().any(|entry| {
                entry.as_array().is_some_and(|pair| {
                    pair.first() == Some(&account)
                        && pair.get(1) == Some(&Value::from(1_000_000 * UNIT))
                })
            }));
        }

        for old in [[0x81; 32], [0x91; 32]] {
            assert!(!contains_value(
                &patch,
                &serde_json::to_value(AccountId::new(old)).unwrap()
            ));
        }
    }

    #[test]
    fn stage0_preset_is_registered() {
        assert!(preset_names()
            .iter()
            .any(|preset| preset.as_str() == STAGE0_RUNTIME_PRESET));
        assert!(get_preset(&PresetId::from(STAGE0_RUNTIME_PRESET)).is_some());
    }
}
