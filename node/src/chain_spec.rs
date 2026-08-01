use minijam_runtime::{genesis_config_presets::stage0_config_genesis, AccountId, WASM_BINARY};
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
    .with_genesis_config(stage0_config_genesis(relayer))
    .with_properties(chain_properties())
    .build())
}

fn parse_relayer_public_key(value: &str) -> Result<AccountId, String> {
    let bytes = sp_core::bytes::from_hex(value).map_err(|_| {
        "MINIJAM_STAGE0_RELAYER_PUBLIC_KEY must be 0x-prefixed 32-byte hex".to_string()
    })?;
    let key: [u8; 32] = bytes.try_into().map_err(|_| {
        "MINIJAM_STAGE0_RELAYER_PUBLIC_KEY must be 0x-prefixed 32-byte hex".to_string()
    })?;
    Ok(AccountId::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc_service::ChainSpec as _;
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
        let stage0 = stage0_chain_spec_with_relayer(stage0_relayer.clone()).unwrap();
        let other = stage0_chain_spec_with_relayer(other_relayer).unwrap();
        let plain = stage0.as_json(false).unwrap();
        let raw = stage0.as_json(true).unwrap();
        let local_hex = format!(
            "{}",
            hex_bytes(minijam_runtime::genesis_config_presets::LOCAL_PLAYGROUND_RELAYER_ACCOUNT)
        );
        assert!(plain.contains(&stage0_relayer.to_ss58check()));
        assert!(!plain.contains(&local_hex));
        assert_ne!(plain, other.as_json(false).unwrap());
        assert_ne!(raw, other.as_json(true).unwrap());
    }

    fn hex_bytes(bytes: [u8; 32]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
