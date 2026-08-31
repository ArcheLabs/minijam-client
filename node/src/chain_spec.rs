use minijam_runtime::{
    genesis_config_presets::{
        season2_config_genesis, stage0_config_genesis, stage1_config_genesis,
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
    let value = std::env::var("MINIJAM_STAGE0_RELAYER_PUBLIC_KEY").map_err(|_| {
        "MINIJAM_STAGE0_RELAYER_PUBLIC_KEY is required when exporting the Stage 0 chain spec"
            .to_string()
    })?;
    stage0_chain_spec_with_relayer(parse_relayer_public_key(&value)?)
}

pub fn stage0_chain_spec_with_relayer(relayer: AccountId) -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Stage-0 wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Stage-0")
    .with_id("minijam_stage0")
    .with_chain_type(ChainType::Live)
    .with_genesis_config_patch(stage0_config_genesis(relayer))
    .with_properties(chain_properties())
    .build())
}

pub fn season2_chain_spec() -> Result<ChainSpec, String> {
    let ingress = std::env::var("MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY")
        .map_err(|_| "MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY is required".to_string())?;
    let allocation = std::env::var("MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY")
        .map_err(|_| "MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY is required".to_string())?;
    season2_chain_spec_with_relayers(
        parse_relayer_public_key(&ingress)?,
        parse_relayer_public_key(&allocation)?,
    )
}

pub fn stage1_chain_spec() -> Result<ChainSpec, String> {
    let ingress = std::env::var("MINIJAM_STAGE1_INGRESS_RELAYER_PUBLIC_KEY")
        .map_err(|_| "MINIJAM_STAGE1_INGRESS_RELAYER_PUBLIC_KEY is required".to_string())?;
    let allocation = std::env::var("MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY")
        .map_err(|_| "MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY is required".to_string())?;
    stage1_chain_spec_with_relayers(
        parse_relayer_public_key(&ingress)?,
        parse_relayer_public_key(&allocation)?,
    )
}

pub fn stage1_chain_spec_with_relayers(
    ingress_relayer: AccountId,
    allocation_relayer: AccountId,
) -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Stage-1 wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Stage-1")
    .with_id("minijam_stage1")
    .with_chain_type(ChainType::Live)
    .with_genesis_config_patch(stage1_config_genesis(ingress_relayer, allocation_relayer))
    .with_properties(chain_properties())
    .build())
}

pub fn season2_chain_spec_with_relayers(
    ingress_relayer: AccountId,
    allocation_relayer: AccountId,
) -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Season 2 wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Season 2")
    .with_id("minijam_season2")
    .with_chain_type(ChainType::Live)
    .with_genesis_config_patch(season2_config_genesis(ingress_relayer, allocation_relayer))
    .with_properties(chain_properties())
    .build())
}

fn parse_relayer_public_key(value: &str) -> Result<AccountId, String> {
    let bytes = sp_core::bytes::from_hex(value)
        .map_err(|_| "relayer public key must be 0x-prefixed 32-byte hex".to_string())?;
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "relayer public key must be 0x-prefixed 32-byte hex".to_string())?;
    Ok(AccountId::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_core::crypto::{AccountId32, Ss58Codec};

    #[test]
    fn stage0_chain_properties_keep_mini_units() {
        let spec = stage0_chain_spec_with_relayer(AccountId::new([0x99; 32]))
            .expect("Stage 0 chain spec must build");
        let properties = spec.properties();
        assert_eq!(
            properties.get("tokenSymbol"),
            Some(&serde_json::Value::from("MINI"))
        );
        assert_eq!(
            properties.get("tokenDecimals"),
            Some(&serde_json::Value::from(12))
        );
    }

    #[test]
    fn stage0_release_accounts_have_documented_ss58_addresses() {
        let faucet = AccountId32::new([
            0x1a, 0x69, 0x04, 0x44, 0xd1, 0x60, 0xa1, 0xf6, 0x32, 0x81, 0x20, 0x3e, 0xde, 0x44,
            0x9b, 0xa9, 0x96, 0xc5, 0x60, 0xb7, 0x98, 0x0e, 0x40, 0x43, 0x75, 0x76, 0x5f, 0x2a,
            0xea, 0xcd, 0x88, 0x6a,
        ]);
        let sudo = AccountId32::new([
            0x64, 0xda, 0x53, 0x90, 0x20, 0xcd, 0x74, 0x3f, 0xed, 0x81, 0xed, 0x5d, 0xe9, 0x22,
            0xf0, 0xb3, 0xe7, 0x76, 0x9b, 0xf3, 0xb7, 0x7a, 0x95, 0x3a, 0xf3, 0xc0, 0x77, 0x9e,
            0xce, 0xfd, 0x7f, 0x23,
        ]);

        assert_eq!(
            faucet.to_ss58check(),
            "5CfLJGrEfAnDLbNGQuSa5CUwGgU13gt7rsWXJLCsNCMFjDUr"
        );
        assert_eq!(
            sudo.to_ss58check(),
            "5ELwW5Q5vLgPKqBpRxuQwGcaGwUhYUVzEd9MhfVUzWWdhLTr"
        );
    }

    #[test]
    fn stage0_relayer_key_is_required_and_strictly_decoded() {
        assert!(parse_relayer_public_key("not-a-key").is_err());
        assert!(parse_relayer_public_key("0x11").is_err());
        assert_eq!(
            parse_relayer_public_key(&format!("0x{}", "42".repeat(32))).unwrap(),
            AccountId::new([0x42; 32])
        );
    }

    #[test]
    fn stage0_plain_and_raw_specs_are_isolated_by_relayer() {
        let stage0_relayer = AccountId::new([0x42; 32]);
        let other_relayer = AccountId::new([0x43; 32]);
        let local_relayer = AccountId::new(
            minijam_runtime::genesis_config_presets::LOCAL_PLAYGROUND_RELAYER_ACCOUNT,
        );

        let patch = stage0_config_genesis(stage0_relayer.clone());
        let expected = serde_json::to_value(stage0_relayer.clone()).unwrap();
        let actual = patch
            .pointer("/mini_jam/ingress_relayer")
            .or_else(|| patch.pointer("/miniJam/ingressRelayer"))
            .expect("Stage 0 genesis patch must contain ingress Relayer");
        assert_eq!(actual, &expected);
        assert_ne!(actual, &serde_json::to_value(local_relayer).unwrap());

        let stage0 = stage0_chain_spec_with_relayer(stage0_relayer.clone()).unwrap();
        let other = stage0_chain_spec_with_relayer(other_relayer).unwrap();
        let plain = stage0.as_json(false).unwrap();
        let raw = stage0.as_json(true).unwrap();
        let other_plain = other.as_json(false).unwrap();
        let other_raw = other.as_json(true).unwrap();
        assert_ne!(plain, other_plain);
        assert_ne!(raw, other_raw);
    }

    #[test]
    fn season2_spec_has_one_worker_and_separate_relayers() {
        let ingress = AccountId::new([0x42; 32]);
        let allocation = AccountId::new([0x43; 32]);
        let spec = season2_chain_spec_with_relayers(ingress.clone(), allocation.clone())
            .expect("Season 2 chain spec must build");
        let patch = spec
            .as_json(false)
            .expect("Season 2 plain chain spec must serialize");
        let value: serde_json::Value = serde_json::from_str(&patch).unwrap();
        let genesis = value.pointer("/genesis/runtimeGenesis/patch").unwrap();
        let mini_jam = genesis.get("miniJam").unwrap();
        assert_eq!(
            mini_jam.get("ingressRelayer").unwrap(),
            &serde_json::to_value(ingress).unwrap()
        );
        assert_eq!(
            mini_jam.get("allocationRelayer").unwrap(),
            &serde_json::to_value(allocation).unwrap()
        );
        assert_eq!(
            genesis.get("miniJamWorkers").unwrap()["workers"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
