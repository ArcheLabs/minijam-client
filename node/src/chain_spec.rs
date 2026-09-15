use minijam_runtime::{
    genesis_config_presets::{
        stage0_config_genesis, stage1_config_genesis, stage1_direct_e2e_config_genesis,
    },
    AccountId, WASM_BINARY,
};
use sc_service::{ChainType, Properties};

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

fn chain_properties() -> Properties {
    let mut properties = Properties::new();
    properties.insert("tokenSymbol".into(), "MINI".into());
    properties.insert("tokenDecimals".into(), 12.into());
    properties
}

pub fn development_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Development")
    .with_id("dev")
    .with_chain_type(ChainType::Development)
    .with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
    .build())
}

pub fn local_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Local Testnet")
    .with_id("local_testnet")
    .with_chain_type(ChainType::Local)
    .with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
    .with_properties(chain_properties())
    .build())
}

pub fn stage0_chain_spec() -> Result<ChainSpec, String> {
    let value = std::env::var("MINIJAM_STAGE0_WORKER_PUBLIC_KEY").map_err(|_| {
        "MINIJAM_STAGE0_WORKER_PUBLIC_KEY is required when exporting the Stage 0 chain spec"
            .to_string()
    })?;
    stage0_chain_spec_with_worker(parse_worker_public_key(&value)?)
}

pub fn stage0_chain_spec_with_worker(worker: AccountId) -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Stage-0 wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Stage-0")
    .with_id("minijam_stage0")
    .with_chain_type(ChainType::Live)
    .with_genesis_config_patch(stage0_config_genesis(worker))
    .with_properties(chain_properties())
    .build())
}

pub fn stage1_chain_spec() -> Result<ChainSpec, String> {
    let (worker, allocation_relayer) = stage1_accounts_from_env()?;
    stage1_chain_spec_with_accounts(worker, allocation_relayer)
}

pub fn stage1_direct_e2e_chain_spec() -> Result<ChainSpec, String> {
    let (worker, allocation_relayer) = stage1_accounts_from_env()?;
    stage1_direct_e2e_chain_spec_with_accounts(worker, allocation_relayer)
}

fn stage1_accounts_from_env() -> Result<(AccountId, AccountId), String> {
    let worker = std::env::var("MINIJAM_STAGE1_WORKER_PUBLIC_KEY")
        .map_err(|_| "MINIJAM_STAGE1_WORKER_PUBLIC_KEY is required".to_string())?;
    let allocation = std::env::var("MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY")
        .map_err(|_| "MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY is required".to_string())?;
    Ok((
        parse_worker_public_key(&worker)?,
        parse_worker_public_key(&allocation)?,
    ))
}

pub fn stage1_chain_spec_with_accounts(
    worker: AccountId,
    allocation_relayer: AccountId,
) -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Stage-1 wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Stage-1")
    .with_id("minijam_stage1")
    .with_chain_type(ChainType::Live)
    .with_genesis_config_patch(stage1_config_genesis(worker, allocation_relayer))
    .with_properties(chain_properties())
    .build())
}

pub fn stage1_direct_e2e_chain_spec_with_accounts(
    worker: AccountId,
    allocation_relayer: AccountId,
) -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Stage-1 wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Stage-1 Direct E2E")
    .with_id("minijam_stage1_direct_e2e")
    .with_chain_type(ChainType::Development)
    .with_genesis_config_patch(stage1_direct_e2e_config_genesis(worker, allocation_relayer))
    .with_properties(chain_properties())
    .build())
}

fn parse_worker_public_key(value: &str) -> Result<AccountId, String> {
    let bytes = sp_core::bytes::from_hex(value)
        .map_err(|_| "worker public key must be 0x-prefixed 32-byte hex".to_string())?;
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "worker public key must be 0x-prefixed 32-byte hex".to_string())?;
    Ok(AccountId::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc_service::ChainSpec as _;

    #[test]
    fn stage0_chain_properties_keep_mini_units() {
        let spec = stage0_chain_spec_with_worker(AccountId::new([0x99; 32])).unwrap();
        assert_eq!(
            spec.properties().get("tokenSymbol"),
            Some(&serde_json::Value::from("MINI"))
        );
        assert_eq!(
            spec.properties().get("tokenDecimals"),
            Some(&serde_json::Value::from(12))
        );
    }

    #[test]
    fn worker_key_is_strictly_decoded() {
        assert!(parse_worker_public_key("not-a-key").is_err());
        assert!(parse_worker_public_key("0x11").is_err());
        assert_eq!(
            parse_worker_public_key(&format!("0x{}", "42".repeat(32))).unwrap(),
            AccountId::new([0x42; 32])
        );
    }

    #[test]
    fn stage0_specs_are_isolated_by_worker_account() {
        let first = stage0_chain_spec_with_worker(AccountId::new([0x42; 32])).unwrap();
        let second = stage0_chain_spec_with_worker(AccountId::new([0x43; 32])).unwrap();
        assert_ne!(
            first.as_json(false).unwrap(),
            second.as_json(false).unwrap()
        );
        assert_ne!(first.as_json(true).unwrap(), second.as_json(true).unwrap());
    }

    #[test]
    fn stage1_production_and_e2e_specs_are_explicitly_separated() {
        let production =
            stage1_chain_spec_with_accounts(AccountId::new([0x44; 32]), AccountId::new([0x55; 32]))
                .unwrap();
        assert_eq!(production.id(), "minijam_stage1");
        assert_eq!(production.chain_type(), ChainType::Live);
        let e2e = stage1_direct_e2e_chain_spec_with_accounts(
            AccountId::new([0x44; 32]),
            AccountId::new([0x55; 32]),
        )
        .unwrap();
        assert_eq!(e2e.id(), "minijam_stage1_direct_e2e");
        assert_eq!(e2e.chain_type(), ChainType::Development);
        assert_ne!(
            production.as_json(false).unwrap(),
            e2e.as_json(false).unwrap()
        );
    }
}
