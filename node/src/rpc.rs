//! Node-specific RPC methods for the package-keyed MiniJAM boundary.

#![warn(missing_docs)]

use std::sync::Arc;

use jsonrpsee::{core::RpcResult, types::ErrorObjectOwned, RpcModule};
use minijam_protocol::PackageStatus;
use minijam_rpc_runtime_api::MiniJamRuntimeApi;
use minijam_runtime::{opaque::Block, AccountId, Balance, Nonce};
use sc_transaction_pool_api::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FinalizedContextV1 {
    block_hash: String,
    block_number: u32,
    state_root: String,
    slot: u32,
}

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

    let FullDeps { client, pool } = deps;
    let mut module = RpcModule::new(());
    module.merge(System::new(client.clone(), pool).into_rpc())?;
    module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
    register_minijam_rpc(&mut module, client)?;
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
    module.register_method("minijam_getPackageStatus", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<&'static str>> {
            let package_hash: sp_core::H256 = params.one()?;
            let status = client
                .runtime_api()
                .get_package_status(finalized_hash(&client), package_hash.to_fixed_bytes())
                .map_err(runtime_api_error)?;
            Ok(status.map(package_status_name))
        }
    })?;
    module.register_method("minijam_getPackageFailure", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let package_hash: sp_core::H256 = params.one()?;
            client
                .runtime_api()
                .get_package_failure(finalized_hash(&client), package_hash.to_fixed_bytes())
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getExecutionReceiptByPackageHash", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let package_hash: sp_core::H256 = params.one()?;
            client
                .runtime_api()
                .get_execution_receipt_by_package_hash(
                    finalized_hash(&client),
                    package_hash.to_fixed_bytes(),
                )
                .map(|value| value.map(|hash| hex_encode(&hash)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getLastExecutionReceipt", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<Option<String>> {
            client
                .runtime_api()
                .get_last_execution_receipt(finalized_hash(&client))
                .map(|value| value.map(|hash| hex_encode(&hash)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getAllocation", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let allocation_id: u64 = params.one()?;
            client
                .runtime_api()
                .get_allocation(finalized_hash(&client), allocation_id)
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_isAllocationProcessed", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<bool> {
            let allocation_id: u64 = params.one()?;
            client
                .runtime_api()
                .is_allocation_processed(finalized_hash(&client), allocation_id)
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getPendingAllocations", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            client
                .runtime_api()
                .get_pending_allocations(finalized_hash(&client))
                .map(|bytes| hex_encode(&bytes))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getPendingPreimages", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            client
                .runtime_api()
                .get_pending_preimages(finalized_hash(&client))
                .map(|bytes| hex_encode(&bytes))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getPreimageStatus", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<&'static str> {
            let (requester, blob_hash, blob_len): (u32, sp_core::H256, u32) = params.parse()?;
            let pending = client
                .runtime_api()
                .has_pending_preimage(
                    finalized_hash(&client),
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
            client
                .runtime_api()
                .get_quarantined_preimages(finalized_hash(&client))
                .map(|bytes| hex_encode(&bytes))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getPendingSystemOps", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            client
                .runtime_api()
                .get_pending_system_ops(finalized_hash(&client))
                .map(|bytes| hex_encode(&bytes))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getQuarantinedSystemOps", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<String> {
            client
                .runtime_api()
                .get_quarantined_system_ops(finalized_hash(&client))
                .map(|bytes| hex_encode(&bytes))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getSystemOp", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let request_id: sp_core::H256 = params.one()?;
            client
                .runtime_api()
                .get_system_op(finalized_hash(&client), request_id.to_fixed_bytes())
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getSystemReceipt", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let request_id: sp_core::H256 = params.one()?;
            client
                .runtime_api()
                .get_system_receipt(finalized_hash(&client), request_id.to_fixed_bytes())
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getSystemOpNonce", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<u64> {
            let sender: sp_core::H256 = params.one()?;
            client
                .runtime_api()
                .get_system_op_nonce(finalized_hash(&client), sender.to_fixed_bytes())
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getSystemServiceInfo", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<Option<String>> {
            client
                .runtime_api()
                .get_system_service_info(finalized_hash(&client))
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getFinalizedContext", {
        let client = client.clone();
        move |_, _, _| -> RpcResult<FinalizedContextV1> {
            let hash = finalized_hash(&client);
            let header = client
                .header(hash)
                .map_err(blockchain_error)?
                .ok_or_else(|| rpc_state_error("finalized header is unavailable"))?;
            let number = *header.number();
            Ok(FinalizedContextV1 {
                block_hash: hex_encode(hash.as_ref()),
                block_number: number,
                state_root: hex_encode(header.state_root().as_ref()),
                slot: number,
            })
        }
    })?;
    module.register_method("minijam_getServiceInfoAt", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let (block_hash, service_id): (sp_core::H256, u32) = params.parse()?;
            client
                .runtime_api()
                .get_service_info(block_hash, service_id)
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getServiceStorageAt", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let (block_hash, service_id, key): (sp_core::H256, u32, String) = params.parse()?;
            client
                .runtime_api()
                .get_service_storage(block_hash, service_id, parse_hex_vec(&key)?)
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getServicePreimageAt", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let (block_hash, service_id, code_hash): (sp_core::H256, u32, sp_core::H256) =
                params.parse()?;
            client
                .runtime_api()
                .get_service_preimage(block_hash, service_id, code_hash.to_fixed_bytes())
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getProtocolStateAt", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let (block_hash, key): (sp_core::H256, String) = params.parse()?;
            client
                .runtime_api()
                .get_protocol_state(block_hash, parse_hex_array::<31>(&key)?)
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    module.register_method("minijam_getProtocolState", {
        let client = client.clone();
        move |params, _, _| -> RpcResult<Option<String>> {
            let key: String = params.one()?;
            client
                .runtime_api()
                .get_protocol_state(finalized_hash(&client), parse_hex_array::<31>(&key)?)
                .map(|value| value.map(|bytes| hex_encode(&bytes)))
                .map_err(runtime_api_error)
        }
    })?;
    Ok(())
}

fn package_status_name(status: PackageStatus) -> &'static str {
    match status {
        PackageStatus::Pending => "pending",
        PackageStatus::Imported => "imported",
        PackageStatus::Failed => "failed",
    }
}
fn finalized_hash<C: HeaderBackend<Block>>(client: &Arc<C>) -> <Block as BlockT>::Hash {
    client.info().finalized_hash
}
fn runtime_api_error(error: sp_api::ApiError) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32001, "MiniJAM runtime API error", Some(error.to_string()))
}
fn blockchain_error(error: BlockChainError) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32002, "MiniJAM blockchain error", Some(error.to_string()))
}
fn rpc_state_error(message: &'static str) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32003, "MiniJAM state unavailable", Some(message))
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
        output[index] = (hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?;
    }
    Ok(output)
}
fn parse_hex_vec(input: &str) -> Result<Vec<u8>, ErrorObjectOwned> {
    let hex = input.strip_prefix("0x").unwrap_or(input);
    if !hex.len().is_multiple_of(2) {
        return Err(invalid_params("hex input must have an even length"));
    }
    hex.as_bytes()
        .chunks_exact(2)
        .map(|chunk| Ok((hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?))
        .collect()
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
