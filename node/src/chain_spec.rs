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
