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
const SYSTEM_SERVICE_BLOB: &[u8] = include_bytes!("../../artifacts/system-service.blob");
/// Deterministic local relayer identity derived from the seed `0x92` repeated
/// 32 times. The private seed is not part of the repository.
pub const LOCAL_INGRESS_RELAYER_ACCOUNT: [u8; 32] = [
    0x90, 0x15, 0x78, 0xa4, 0x17, 0x30, 0x0a, 0xa0, 0xae, 0x53, 0x3b, 0x5b, 0xd0, 0xe9, 0xaf, 0x48,
    0x9a, 0x4c, 0xc4, 0xa6, 0xf3, 0x89, 0x99, 0xb7, 0x62, 0x83, 0x86, 0x70, 0x87, 0x73, 0x82, 0x09,
];

pub const LOCAL_ALLOCATION_RELAYER_ACCOUNT: [u8; 32] = LOCAL_INGRESS_RELAYER_ACCOUNT;

/// The public relayer identity used by the currently deployed testnet. The
/// corresponding operator seed is intentionally kept outside the repository.
pub const TESTNET_INGRESS_RELAYER_ACCOUNT: [u8; 32] = LOCAL_INGRESS_RELAYER_ACCOUNT;
pub const TESTNET_ALLOCATION_RELAYER_ACCOUNT: [u8; 32] = LOCAL_ALLOCATION_RELAYER_ACCOUNT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorityIdentity {
    pub aura: [u8; 32],
    pub grandpa: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerIdentity {
    pub account: [u8; 32],
    pub session_key: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stage1GenesisConfig {
    pub authority: AuthorityIdentity,
    pub worker: WorkerIdentity,
    pub ingress_relayer: [u8; 32],
    pub allocation_relayer: [u8; 32],
    pub sudo: [u8; 32],
    pub faucet: [u8; 32],
}

const TESTNET_AUTHORITY_AURA: [[u8; 32]; 1] = [[
    0x66, 0xd0, 0x9c, 0xb4, 0xdf, 0xf3, 0x44, 0xd5, 0xa6, 0xb0, 0x7c, 0xa9, 0x90, 0x9d, 0xc0, 0x5f,
    0x46, 0xcc, 0xda, 0x66, 0x87, 0xc5, 0x2d, 0x7d, 0xad, 0x99, 0x83, 0xc7, 0xfe, 0x89, 0x16, 0x19,
]];
const TESTNET_AUTHORITY_GRANDPA: [[u8; 32]; 1] = [[
    0x4e, 0x0c, 0xa8, 0x04, 0x2d, 0x49, 0xc5, 0x95, 0xcf, 0x51, 0x10, 0x3a, 0x96, 0x31, 0x3b, 0xcf,
    0x72, 0xb3, 0x7c, 0xc1, 0x78, 0xb7, 0x61, 0x53, 0x82, 0xe9, 0x35, 0x8f, 0xd9, 0x5b, 0xee, 0x0d,
]];
const TESTNET_WORKER_ACCOUNT: [u8; 32] = [
    0x32, 0x6c, 0x5d, 0x73, 0x92, 0x0e, 0x92, 0x46, 0x43, 0x86, 0xba, 0x7a, 0x3a, 0xc1, 0x95, 0x22,
    0xb8, 0x55, 0xed, 0xc9, 0x50, 0x88, 0xb2, 0x93, 0x3f, 0xa5, 0x47, 0x0f, 0xca, 0x94, 0x1a, 0x0f,
];
const TESTNET_WORKER_SESSION_KEY: [u8; 32] = TESTNET_WORKER_ACCOUNT;
pub(crate) const TESTNET_FAUCET_ACCOUNT: [u8; 32] = [
    0x1a, 0x69, 0x04, 0x44, 0xd1, 0x60, 0xa1, 0xf6, 0x32, 0x81, 0x20, 0x3e, 0xde, 0x44, 0x9b, 0xa9,
    0x96, 0xc5, 0x60, 0xb7, 0x98, 0x0e, 0x40, 0x43, 0x75, 0x76, 0x5f, 0x2a, 0xea, 0xcd, 0x88, 0x6a,
];
const TESTNET_SUDO_ACCOUNT: [u8; 32] = [
    0x64, 0xda, 0x53, 0x90, 0x20, 0xcd, 0x74, 0x3f, 0xed, 0x81, 0xed, 0x5d, 0xe9, 0x22, 0xf0, 0xb3,
    0xe7, 0x76, 0x9b, 0xf3, 0xb7, 0x7a, 0x95, 0x3a, 0xf3, 0xc0, 0x77, 0x9e, 0xce, 0xfd, 0x7f, 0x23,
];

fn build_genesis(
    initial_authorities: Vec<(AuraId, GrandpaId)>,
    mut endowed_accounts: Vec<AccountId>,
    root: AccountId,
    workers: Vec<(AccountId, [u8; 32], Balance)>,
    ingress_relayer: AccountId,
    allocation_relayer: AccountId,
) -> Value {
    let reward_pool = AccountId::new([9; 32]);
    let fuel_escrow = AccountId::new([7; 32]);
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
        .any(|account| account == &ingress_relayer)
    {
        endowed_accounts.push(ingress_relayer.clone());
    }
    if !endowed_accounts
        .iter()
        .any(|account| account == &allocation_relayer)
    {
        endowed_accounts.push(allocation_relayer.clone());
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
                        1_000 * UNIT
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
            service_fuel: Vec::new(),
            ingress_relayer: Some(ingress_relayer.clone()),
            allocation_relayer: Some(allocation_relayer),
            _phantom: Default::default(),
        },
        mini_jam_workers: MiniJamWorkersConfig {
            workers,
            _phantom: Default::default(),
        },
    })
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

pub fn local_stage1_config() -> Stage1GenesisConfig {
    Stage1GenesisConfig {
        authority: AuthorityIdentity {
            aura: Sr25519Keyring::Alice.public().0,
            grandpa: sp_keyring::Ed25519Keyring::Alice.public().0,
        },
        worker: WorkerIdentity {
            account: Sr25519Keyring::Bob.public().0,
            session_key: Sr25519Keyring::Bob.public().0,
        },
        ingress_relayer: LOCAL_INGRESS_RELAYER_ACCOUNT,
        allocation_relayer: LOCAL_ALLOCATION_RELAYER_ACCOUNT,
        sudo: Sr25519Keyring::Charlie.public().0,
        faucet: Sr25519Keyring::Dave.public().0,
    }
}

pub fn testnet_stage1_config() -> Stage1GenesisConfig {
    Stage1GenesisConfig {
        authority: AuthorityIdentity {
            aura: TESTNET_AUTHORITY_AURA[0],
            grandpa: TESTNET_AUTHORITY_GRANDPA[0],
        },
        worker: WorkerIdentity {
            account: TESTNET_WORKER_ACCOUNT,
            session_key: TESTNET_WORKER_SESSION_KEY,
        },
        ingress_relayer: TESTNET_INGRESS_RELAYER_ACCOUNT,
        allocation_relayer: TESTNET_ALLOCATION_RELAYER_ACCOUNT,
        sudo: TESTNET_SUDO_ACCOUNT,
        faucet: TESTNET_FAUCET_ACCOUNT,
    }
}

/// The sole Stage-1 genesis builder. Network-specific differences are limited
/// to the identities supplied in `Stage1GenesisConfig`.
pub fn stage1_genesis(config: Stage1GenesisConfig) -> Value {
    let authority = AccountId::new(config.authority.aura);
    let worker = AccountId::new(config.worker.account);
    let sudo = AccountId::new(config.sudo);
    let faucet = AccountId::new(config.faucet);
    let ingress_relayer = AccountId::new(config.ingress_relayer);
    let allocation_relayer = AccountId::new(config.allocation_relayer);
    let mut endowed_accounts = Vec::new();
    for account in [
        authority.clone(),
        worker.clone(),
        sudo.clone(),
        faucet.clone(),
        ingress_relayer.clone(),
        allocation_relayer.clone(),
    ] {
        if !endowed_accounts.iter().any(|existing| existing == &account) {
            endowed_accounts.push(account);
        }
    }

    build_genesis(
        vec![(
            AuraId::from(sr25519::Public::from_raw(config.authority.aura)),
            GrandpaId::from(ed25519::Public::from_raw(config.authority.grandpa)),
        )],
        endowed_accounts,
        AccountId::new(config.sudo),
        vec![(worker, config.worker.session_key, 1_000 * UNIT)],
        ingress_relayer,
        allocation_relayer,
    )
}

pub fn local_genesis() -> Value {
    stage1_genesis(local_stage1_config())
}

pub fn testnet_genesis() -> Value {
    stage1_genesis(testnet_stage1_config())
}

pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
    let patch = match id.as_ref() {
        sp_genesis_builder::DEV_RUNTIME_PRESET
        | sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => local_genesis(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use jam_codec::Decode as JamDecode;
    use jambda_minijam_executive::MiniJamExecutive;
    use jp_core_primitives::{
        crypto::OpaqueHash, simple::ByteSequence, state::StoreKey, types::ServiceInfo,
    };
    use minijam_jamcore_api::{
        MiniJamExecutionInput, MiniJamExecutor, ProtocolStateReader, StateError,
    };
    use minijam_protocol::SystemReceiptV2;
    use minijam_protocol::{SystemCommandV2, SystemOpV2, PROTOCOL_VERSION_V1};
    use parity_scale_codec::Decode;
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
    fn local_genesis_seeds_stage1_service_and_one_worker() {
        let patch = local_genesis();
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
            service_fuel.is_empty(),
            "Stage-1 genesis does not seed Service Fuel"
        );

        let workers = field(mini_jam_workers, "workers", "workers")
            .as_array()
            .expect("workers must be a JSON array");
        assert_eq!(workers.len(), 1);
        assert!(workers.iter().all(|entry| {
            entry.as_array().is_some_and(|worker| {
                worker.len() == 3 && worker.get(2) == Some(&Value::from(1_000 * UNIT))
            })
        }));
    }

    #[test]
    fn stage1_genesis_uses_committed_service0_protocol_state() {
        let patch = testnet_genesis();
        let mini_jam = section(&patch, "mini_jam", "miniJam");
        let protocol_state = field(mini_jam, "protocol_state", "protocolState")
            .as_array()
            .expect("Stage-1 protocol state must be a JSON array");
        let committed_state = serde_json::to_value(system_service_zero_protocol_state())
            .expect("committed Service 0 state must serialize");
        assert_eq!(protocol_state, committed_state.as_array().unwrap());
    }

    #[test]
    fn stage1_genesis_registers_only_one_testnet_worker() {
        let config = testnet_stage1_config();
        assert_eq!(config.worker.account, TESTNET_WORKER_ACCOUNT);
        assert_eq!(config.worker.session_key, TESTNET_WORKER_SESSION_KEY);

        let patch = testnet_genesis();
        let registered = field(
            section(&patch, "mini_jam_workers", "miniJamWorkers"),
            "workers",
            "workers",
        )
        .as_array()
        .expect("workers must be a JSON array");
        assert_eq!(registered.len(), 1);
    }

    #[test]
    fn local_genesis_endows_reward_pool_and_fuel_escrow() {
        let patch = local_genesis();
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
        assert_eq!(manifest.get("stage"), Some(&Value::from(0)));
        assert_eq!(
            manifest.get("consumed_by_stages"),
            Some(&serde_json::json!([0, 1]))
        );
        assert!(!SYSTEM_SERVICE_BLOB.is_empty());
        jp_vm_predecode::to_af_and_c_blob(SYSTEM_SERVICE_BLOB)
            .expect("system service blob must be a valid PVM artifact");
    }

    #[test]
    fn system_ops_execute_through_real_jambda_executor() {
        let sender = [0x5a; 32];
        let command = SystemCommandV2::CreateService {
            code_hash: [0x9b; 32],
            code_len: 27,
            min_item_gas: 2,
            min_memo_gas: 3,
        };
        let op = SystemOpV2::new(sender, 0, command);
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
    fn allocation_executes_through_service_zero_and_accumulate() {
        let mut state = TestProtocolState::from_pairs(system_service_zero_protocol_state());
        let create = SystemOpV2::new(
            [0x5a; 32],
            0,
            SystemCommandV2::CreateService {
                code_hash: [0x9b; 32],
                code_len: 27,
                min_item_gas: 2,
                min_memo_gas: 3,
            },
        );
        let create_request_id = create.request_id;
        let create_input = MiniJamExecutionInput {
            protocol_version: PROTOCOL_VERSION_V1,
            slot: 10,
            parent_hash: [1u8; 32],
            parent_state_root: [2u8; 32],
            entropy: [3u8; 32],
            reports: Default::default(),
            preimages: Default::default(),
            system_ops: vec![create].try_into().unwrap(),
            max_gas: 20_000_000,
        };
        let create_output =
            <MiniJamExecutive as MiniJamExecutor>::execute(&MiniJamExecutive, create_input, &state)
                .expect("service creation must execute");

        let mut create_receipt_storage_key = b"system/receipt/".to_vec();
        create_receipt_storage_key.extend_from_slice(&create_request_id);
        let create_receipt_key =
            StoreKey::new_service_storage_key(&0, &ByteSequence::from(create_receipt_storage_key))
                .to_state_key();
        let create_receipt = create_output
            .ordered_changes
            .iter()
            .find(|change| change.key == create_receipt_key.0)
            .and_then(|change| change.value.as_ref())
            .expect("service creation must write a receipt");
        let receipt = SystemReceiptV2::decode(&mut create_receipt.as_slice())
            .expect("service creation receipt must decode");
        let target_service = match receipt {
            SystemReceiptV2::ServiceCreated { service_id } => service_id,
            other => panic!("unexpected service creation receipt: {other:?}"),
        };
        assert!(
            target_service > 0,
            "created service id was {target_service}"
        );
        state.apply(&create_output);
        let target_key = StoreKey::new_service_info_key(&target_service).to_state_key();
        let before = ServiceInfo::decode(
            &mut state
                .get(&target_key.0)
                .expect("created service must have service info")
                .expect("created service info must be present")
                .as_slice(),
        )
        .expect("created service info must decode");

        let allocation = SystemOpV2::new(
            [0xa1; 32],
            100,
            SystemCommandV2::ApplyAllocation {
                allocation_id: 100,
                target_service,
                amount: 500,
            },
        );
        let allocation_input = MiniJamExecutionInput {
            protocol_version: PROTOCOL_VERSION_V1,
            slot: 11,
            parent_hash: [4u8; 32],
            parent_state_root: [5u8; 32],
            entropy: [6u8; 32],
            reports: Default::default(),
            preimages: Default::default(),
            system_ops: vec![allocation].try_into().unwrap(),
            max_gas: 20_000_000,
        };
        let allocation_output = <MiniJamExecutive as MiniJamExecutor>::execute(
            &MiniJamExecutive,
            allocation_input,
            &state,
        )
        .expect("allocation transfer must execute");
        let mut allocation_receipt_storage_key = b"system/allocation/".to_vec();
        allocation_receipt_storage_key.extend_from_slice(&100u64.to_le_bytes());
        let allocation_receipt_key = StoreKey::new_service_storage_key(
            &0,
            &ByteSequence::from(allocation_receipt_storage_key),
        )
        .to_state_key();
        assert!(
            allocation_output
                .ordered_changes
                .iter()
                .any(|change| change.key == allocation_receipt_key.0),
            "successful or rejected allocation must write a receipt"
        );
        state.apply(&allocation_output);

        let after = ServiceInfo::decode(
            &mut state
                .get(&target_key.0)
                .expect("target service must remain present")
                .expect("target service info must remain present")
                .as_slice(),
        )
        .expect("target service info must decode after allocation");
        assert_eq!(after.balance, before.balance + 500);

        let mut receipt_storage_key = b"system/allocation/".to_vec();
        receipt_storage_key.extend_from_slice(&100u64.to_le_bytes());
        let receipt_key =
            StoreKey::new_service_storage_key(&0, &ByteSequence::from(receipt_storage_key))
                .to_state_key();
        let receipt = state
            .get(&receipt_key.0)
            .expect("allocation receipt lookup must succeed")
            .expect("successful allocation must write a receipt");
        assert_eq!(receipt.first(), Some(&0));
    }

    #[test]
    #[ignore = "long-running Jambda cross-epoch integration; run explicitly outside the release gate"]
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
    #[ignore = "long-running Jambda cross-epoch integration; run explicitly outside the release gate"]
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
        let op = SystemOpV2::new(
            sender,
            0,
            SystemCommandV2::CreateService {
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
    fn testnet_genesis_uses_fixed_network_identities() {
        let patch = testnet_genesis();
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
        assert_eq!(workers.len(), 1);
        assert_eq!(TESTNET_WORKER_ACCOUNT, TESTNET_WORKER_SESSION_KEY);

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
                "testnet worker accounts must not use development keyring accounts"
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
    fn local_and_testnet_genesis_use_fixed_relayer_identities() {
        let local = serde_json::to_value(AccountId::new(LOCAL_INGRESS_RELAYER_ACCOUNT)).unwrap();
        for patch in [local_genesis(), testnet_genesis()] {
            assert_eq!(
                field(
                    section(&patch, "mini_jam", "miniJam"),
                    "ingress_relayer",
                    "ingressRelayer"
                ),
                &local
            );
        }

        assert_eq!(
            testnet_stage1_config().ingress_relayer,
            TESTNET_INGRESS_RELAYER_ACCOUNT
        );
    }

    #[test]
    fn testnet_genesis_uses_fixed_faucet_and_sudo_accounts() {
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

        assert_eq!(TESTNET_FAUCET_ACCOUNT, EXPECTED_FAUCET);
        assert_eq!(TESTNET_SUDO_ACCOUNT, EXPECTED_SUDO);
        let patch = testnet_genesis();
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
    fn local_is_the_only_runtime_genesis_for_dev_and_local_presets() {
        assert!(preset_names()
            .iter()
            .any(|preset| preset.as_str() == sp_genesis_builder::DEV_RUNTIME_PRESET));
        assert_eq!(
            get_preset(&PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET)),
            get_preset(&PresetId::from(
                sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET
            ))
        );
    }
}
