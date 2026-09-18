use minijam_runtime::{
    genesis_config_presets::{local_genesis, testnet_genesis},
    WASM_BINARY,
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

/// Canonical Stage-1 local network. `--dev` resolves to this exact spec.
pub fn local_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Local Stage-1 wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Local")
    .with_id("minijam_local")
    .with_chain_type(ChainType::Local)
    .with_genesis_config_patch(local_genesis())
    .with_properties(chain_properties())
    .build())
}

/// Canonical Stage-1 testnet network.
pub fn testnet_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Stage-1 testnet wasm not available".to_string())?,
        None,
    )
    .with_name("MiniJAM Testnet")
    .with_id("minijam_testnet")
    .with_chain_type(ChainType::Live)
    .with_genesis_config_patch(testnet_genesis())
    .with_properties(chain_properties())
    .build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijam_runtime::genesis_config_presets::{local_stage1_config, testnet_stage1_config};
    use sc_service::ChainSpec as _;
    use serde_json::{json, Value};

    fn runtime_genesis_patch(spec: &mut Value) -> &mut serde_json::Map<String, Value> {
        spec.get_mut("genesis")
            .and_then(Value::as_object_mut)
            .and_then(|genesis| genesis.get_mut("runtimeGenesis"))
            .and_then(Value::as_object_mut)
            .and_then(|runtime| runtime.get_mut("patch"))
            .and_then(Value::as_object_mut)
            .expect("chain spec must contain runtime genesis")
    }

    fn normalize_identity_fields(spec: &mut Value) {
        let runtime = runtime_genesis_patch(spec);
        let identity = || json!("<identity>");

        if let Some(authorities) = runtime
            .get_mut("aura")
            .and_then(Value::as_object_mut)
            .and_then(|aura| aura.get_mut("authorities"))
            .and_then(Value::as_array_mut)
        {
            for authority in authorities {
                *authority = identity();
            }
        }

        if let Some(authorities) = runtime
            .get_mut("grandpa")
            .and_then(Value::as_object_mut)
            .and_then(|grandpa| grandpa.get_mut("authorities"))
            .and_then(Value::as_array_mut)
        {
            for authority in authorities {
                authority
                    .as_array_mut()
                    .expect("grandpa authority must be a tuple")[0] = identity();
            }
        }

        if let Some(balances) = runtime
            .get_mut("balances")
            .and_then(Value::as_object_mut)
            .and_then(|balances| balances.get_mut("balances"))
            .and_then(Value::as_array_mut)
        {
            for balance in balances {
                balance
                    .as_array_mut()
                    .expect("balance entry must be a tuple")[0] = identity();
            }
        }

        if let Some(key) = runtime
            .get_mut("sudo")
            .and_then(Value::as_object_mut)
            .and_then(|sudo| sudo.get_mut("key"))
        {
            *key = identity();
        }

        if let Some(mini_jam) = runtime.get_mut("miniJam").and_then(Value::as_object_mut) {
            for field in ["ingressRelayer", "allocationRelayer"] {
                if let Some(value) = mini_jam.get_mut(field) {
                    *value = identity();
                }
            }
        }

        if let Some(workers) = runtime
            .get_mut("miniJamWorkers")
            .and_then(Value::as_object_mut)
            .and_then(|workers| workers.get_mut("workers"))
            .and_then(Value::as_array_mut)
        {
            for worker in workers {
                let worker = worker.as_array_mut().expect("worker entry must be a tuple");
                worker[0] = identity();
                worker[1] = identity();
            }
        }
    }

    #[test]
    fn local_and_testnet_specs_have_the_canonical_network_ids() {
        let local = local_chain_spec().expect("local chain spec must build");
        let testnet = testnet_chain_spec().expect("testnet chain spec must build");
        assert_eq!(local.id(), "minijam_local");
        assert_eq!(local.chain_type(), ChainType::Local);
        assert_eq!(testnet.id(), "minijam_testnet");
        assert_eq!(testnet.chain_type(), ChainType::Live);
    }

    #[test]
    fn local_and_testnet_use_one_authority_and_one_worker() {
        for spec in [local_chain_spec().unwrap(), testnet_chain_spec().unwrap()] {
            let json = spec.as_json(false).unwrap();
            let json: Value = serde_json::from_str(&json).unwrap();
            let runtime = json["genesis"]["runtimeGenesis"]["patch"]
                .as_object()
                .expect("chain spec must contain runtime genesis");
            assert_eq!(runtime["aura"]["authorities"].as_array().unwrap().len(), 1);
            assert_eq!(
                runtime["grandpa"]["authorities"].as_array().unwrap().len(),
                1
            );
            assert_eq!(
                runtime["miniJamWorkers"]["workers"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        }

        assert_eq!(local_stage1_config().worker.account.len(), 32);
        assert_eq!(testnet_stage1_config().worker.account.len(), 32);
    }

    #[test]
    fn local_and_testnet_genesis_have_the_same_protocol_shape() {
        let local = local_chain_spec().unwrap().as_json(false).unwrap();
        let testnet = testnet_chain_spec().unwrap().as_json(false).unwrap();
        let mut local: Value = serde_json::from_str(&local).unwrap();
        let mut testnet: Value = serde_json::from_str(&testnet).unwrap();

        for spec in [&mut local, &mut testnet] {
            let object = spec.as_object_mut().unwrap();
            object.remove("name");
            object.remove("id");
            object.remove("chainType");
            object.remove("bootNodes");
            object.remove("protocolId");
            object.remove("properties");
        }

        normalize_identity_fields(&mut local);
        normalize_identity_fields(&mut testnet);

        assert_eq!(local, testnet);
    }

    #[test]
    fn local_and_testnet_specs_are_reproducible() {
        let local_a = local_chain_spec().unwrap();
        let local_b = local_chain_spec().unwrap();
        let testnet_a = testnet_chain_spec().unwrap();
        let testnet_b = testnet_chain_spec().unwrap();

        assert_eq!(
            local_a.as_json(false).unwrap(),
            local_b.as_json(false).unwrap()
        );
        assert_eq!(
            local_a.as_json(true).unwrap(),
            local_b.as_json(true).unwrap()
        );
        assert_eq!(
            testnet_a.as_json(false).unwrap(),
            testnet_b.as_json(false).unwrap()
        );
        assert_eq!(
            testnet_a.as_json(true).unwrap(),
            testnet_b.as_json(true).unwrap()
        );
    }
}
