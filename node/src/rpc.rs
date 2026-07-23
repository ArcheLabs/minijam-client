//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

use jsonrpsee::{core::RpcResult, types::ErrorObjectOwned, RpcModule};
use minijam_protocol::WorkId;
use minijam_rpc_runtime_api::MiniJamRuntimeApi;
use minijam_runtime::{opaque::Block, AccountId, Balance, Nonce};
use sc_transaction_pool_api::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_runtime::traits::Block as BlockT;

/// Full client dependencies.
pub struct FullDeps<C, P> {
    /// The client instance to use.
    pub client: Arc<C>,
    /// Transaction pool instance.
    pub pool: Arc<P>,
}

/// Instantiate all full RPC extensions.
pub fn create_full<C, P>(
    deps: FullDeps<C, P>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
    C: ProvideRuntimeApi<Block>,
    C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
    C: Send + Sync + 'static,
    C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
    C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
    C::Api: MiniJamRuntimeApi<Block>,
    C::Api: BlockBuilder<Block>,
    P: TransactionPool + 'static,
{
    use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
    use substrate_frame_rpc_system::{System, SystemApiServer};

    let mut module = RpcModule::new(());
    let FullDeps { client, pool } = deps;

    module.merge(System::new(client.clone(), pool).into_rpc())?;
    module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
    register_minijam_rpc::<C>(&mut module, client)?;

    // Extend this RPC with a custom API by using the following syntax.
    // `YourRpcStruct` should have a reference to a client, which is needed
    // to call into the runtime.
    // `module.merge(YourRpcTrait::into_rpc(YourRpcStruct::new(ReferenceToClient, ...)))?;`

    // You probably want to enable the `rpc v2 chainSpec` API as well
    //
    // let chain_name = chain_spec.name().to_string();
    // let genesis_hash = client.block_hash(0).ok().flatten().expect("Genesis block exists; qed");
    // let properties = chain_spec.properties();
    // module.merge(ChainSpec::new(chain_name, genesis_hash, properties).into_rpc())?;

    Ok(module)
}

fn register_minijam_rpc<C>(
    module: &mut RpcModule<()>,
    client: Arc<C>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    C: ProvideRuntimeApi<Block>,
    C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
    C: Send + Sync + 'static,
    C::Api: MiniJamRuntimeApi<Block>,
{
    module.register_method("minijam_getWork", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let work_id: WorkId = params.one()?;
            let encoded = client
                .runtime_api()
                .get_work(best_hash(&client), work_id)
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getPendingWorkTasks", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            let tasks = client
                .runtime_api()
                .get_pending_work_tasks(finalized_hash(&client))
                .map_err(runtime_api_error)?;
            Ok(hex_encode(&parity_scale_codec::Encode::encode(&tasks)))
        }
    })?;

    module.register_method("minijam_getOpenVoteTasks", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            let tasks = client
                .runtime_api()
                .get_open_vote_tasks(finalized_hash(&client))
                .map_err(runtime_api_error)?;
            Ok(hex_encode(&parity_scale_codec::Encode::encode(&tasks)))
        }
    })?;

    module.register_method("minijam_getWorkByPackageHash", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let package_hash: sp_core::H256 = params.one()?;
            let encoded = client
                .runtime_api()
                .get_work_by_package_hash(best_hash(&client), package_hash.to_fixed_bytes())
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getWorkBundleRef", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let work_id: WorkId = params.one()?;
            let encoded = client
                .runtime_api()
                .get_work_bundle_ref(best_hash(&client), work_id)
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getCandidate", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let (work_id, round): (WorkId, u8) = params.parse()?;
            let encoded = client
                .runtime_api()
                .get_candidate(best_hash(&client), work_id, round)
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getExecutionReceipt", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let work_id: WorkId = params.one()?;
            let receipt = client
                .runtime_api()
                .get_execution_receipt(best_hash(&client), work_id)
                .map_err(runtime_api_error)?;
            Ok(receipt.map(|hash| hex_encode(&hash)))
        }
    })?;

    module.register_method("minijam_getLastExecutionReceipt", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<Option<String>> {
            let receipt = client
                .runtime_api()
                .get_last_execution_receipt(best_hash(&client))
                .map_err(runtime_api_error)?;
            Ok(receipt.map(|hash| hex_encode(&hash)))
        }
    })?;

    module.register_method("minijam_getServiceFuel", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<String> {
            let service_id: u32 = params.one()?;
            let encoded = client
                .runtime_api()
                .get_service_fuel(best_hash(&client), service_id)
                .map_err(runtime_api_error)?;
            Ok(hex_encode(&encoded))
        }
    })?;

    module.register_method("minijam_getWorkFuelReservation", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let work_id: WorkId = params.one()?;
            let encoded = client
                .runtime_api()
                .get_work_fuel_reservation(best_hash(&client), work_id)
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getWorkFuelSettlement", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let work_id: WorkId = params.one()?;
            let encoded = client
                .runtime_api()
                .get_work_fuel_settlement(best_hash(&client), work_id)
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getPendingPreimages", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            let encoded = client
                .runtime_api()
                .get_pending_preimages(best_hash(&client))
                .map_err(runtime_api_error)?;
            Ok(hex_encode(&encoded))
        }
    })?;

    module.register_method("minijam_getPreimageStatus", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<&'static str> {
            let (requester, blob_hash, blob_len): (u32, sp_core::H256, u32) = params.parse()?;
            let pending = client
                .runtime_api()
                .has_pending_preimage(
                    best_hash(&client),
                    requester,
                    blob_hash.to_fixed_bytes(),
                    blob_len,
                )
                .map_err(runtime_api_error)?;
            Ok(if pending { "pending" } else { "unknown" })
        }
    })?;

    module.register_method("minijam_getQuarantinedPreimages", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            let encoded = client
                .runtime_api()
                .get_quarantined_preimages(best_hash(&client))
                .map_err(runtime_api_error)?;
            Ok(hex_encode(&encoded))
        }
    })?;

    module.register_method("minijam_getPendingSystemOps", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            let encoded = client
                .runtime_api()
                .get_pending_system_ops(best_hash(&client))
                .map_err(runtime_api_error)?;
            Ok(hex_encode(&encoded))
        }
    })?;

    module.register_method("minijam_getQuarantinedSystemOps", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            let encoded = client
                .runtime_api()
                .get_quarantined_system_ops(best_hash(&client))
                .map_err(runtime_api_error)?;
            Ok(hex_encode(&encoded))
        }
    })?;

    module.register_method("minijam_getSystemOp", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let request_id: sp_core::H256 = params.one()?;
            let encoded = client
                .runtime_api()
                .get_system_op(best_hash(&client), request_id.to_fixed_bytes())
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getSystemReceipt", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let request_id: sp_core::H256 = params.one()?;
            let encoded = client
                .runtime_api()
                .get_system_receipt(best_hash(&client), request_id.to_fixed_bytes())
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getSystemServiceInfo", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<Option<String>> {
            let encoded = client
                .runtime_api()
                .get_system_service_info(best_hash(&client))
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    module.register_method("minijam_getProtocolState", {
        move |params, _, _| -> RpcResult<Option<String>> {
            let key_hex: String = params.one()?;
            let key = parse_hex_array::<31>(&key_hex)?;
            let encoded = client
                .runtime_api()
                .get_protocol_state(best_hash(&client), key)
                .map_err(runtime_api_error)?;
            Ok(encoded.map(|bytes| hex_encode(&bytes)))
        }
    })?;

    Ok(())
}

fn best_hash<C>(client: &Arc<C>) -> <Block as BlockT>::Hash
where
    C: HeaderBackend<Block>,
{
    client.info().best_hash
}

fn finalized_hash<C>(client: &Arc<C>) -> <Block as BlockT>::Hash
where
    C: HeaderBackend<Block>,
{
    client.info().finalized_hash
}

fn runtime_api_error(error: sp_api::ApiError) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32001, "MiniJAM runtime API error", Some(error.to_string()))
}

fn invalid_params(message: &'static str) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32602, "Invalid MiniJAM RPC params", Some(message))
}

fn parse_hex_array<const N: usize>(input: &str) -> Result<[u8; N], ErrorObjectOwned> {
    let hex = input.strip_prefix("0x").unwrap_or(input);
    if hex.len() != N * 2 {
        return Err(invalid_params(
            "hex length does not match expected byte width",
        ));
    }

    let mut output = [0u8; N];
    for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn hex_nibble(byte: u8) -> Result<u8, ErrorObjectOwned> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(invalid_params("hex input contains a non-hex character")),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(2 + bytes.len() * 2);
    output.push_str("0x");
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
