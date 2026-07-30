use minijam_runtime::{genesis_config_presets::STAGE0_RUNTIME_PRESET, WASM_BINARY};
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
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Stage-0 wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Stage-0")
    .with_id("minijam_stage0")
    .with_chain_type(ChainType::Live)
    .with_genesis_config_preset_name(STAGE0_RUNTIME_PRESET)
    .with_properties(chain_properties())
    .build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_core::crypto::{AccountId32, Ss58Codec};

    #[test]
    fn stage0_chain_properties_keep_mini_units() {
        let spec = stage0_chain_spec().expect("Stage 0 chain spec must build");
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
}
