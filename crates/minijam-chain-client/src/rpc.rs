use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClient};
use minijam_protocol::Hash;
use parity_scale_codec::Decode;
use serde::Deserialize;

use crate::ChainClientError;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FinalizedContext {
    #[serde(deserialize_with = "deserialize_hash")]
    pub block_hash: Hash,
    pub block_number: u32,
    #[serde(deserialize_with = "deserialize_hash")]
    pub state_root: Hash,
    pub slot: u32,
}

pub async fn finalized_context(rpc: &WsClient) -> Result<FinalizedContext, ChainClientError> {
    rpc.request("minijam_getFinalizedContext", rpc_params![])
        .await
        .map_err(map_rpc)
}

pub async fn optional_hex(
    rpc: &WsClient,
    method: &str,
    params: serde_json::Value,
) -> Result<Option<Vec<u8>>, ChainClientError> {
    let values = params
        .as_array()
        .ok_or_else(|| ChainClientError::Decode("RPC params must be an array".into()))?;
    let mut params = jsonrpsee::core::params::ArrayParams::new();
    for value in values {
        params
            .insert(value)
            .map_err(|error| ChainClientError::Decode(error.to_string()))?;
    }
    let result: Option<String> = rpc.request(method, params).await.map_err(map_rpc)?;
    result.map(|value| decode_hex(&value)).transpose()
}

pub async fn genesis_hash(rpc: &WsClient) -> Result<Hash, ChainClientError> {
    let value: String = rpc
        .request("chain_getBlockHash", rpc_params![0])
        .await
        .map_err(map_rpc)?;
    decode_hash(&value)
}

pub async fn block_hash(rpc: &WsClient, number: u32) -> Result<Hash, ChainClientError> {
    let value: String = rpc
        .request("chain_getBlockHash", rpc_params![number])
        .await
        .map_err(map_rpc)?;
    decode_hash(&value)
}

pub async fn events_at(
    rpc: &WsClient,
    block_hash: Hash,
) -> Result<Vec<minijam_runtime::RuntimeEvent>, ChainClientError> {
    let mut key = sp_core::twox_128(b"System").to_vec();
    key.extend_from_slice(&sp_core::twox_128(b"Events"));
    let encoded: Option<String> = rpc
        .request("state_getStorage", rpc_params![hex(&key), hex(&block_hash)])
        .await
        .map_err(map_rpc)?;
    let Some(encoded) = encoded else {
        return Ok(Vec::new());
    };
    let bytes = decode_hex(&encoded)?;
    let records: Vec<frame_system::EventRecord<minijam_runtime::RuntimeEvent, sp_core::H256>> =
        Decode::decode(&mut bytes.as_slice())
            .map_err(|error| ChainClientError::Decode(error.to_string()))?;
    Ok(records.into_iter().map(|record| record.event).collect())
}

pub async fn account_nonce(rpc: &WsClient, account: [u8; 32]) -> Result<u32, ChainClientError> {
    rpc.request(
        "system_accountNextIndex",
        rpc_params![crate::account_id_rpc_param(account)],
    )
    .await
    .map_err(map_rpc)
}

pub async fn system_op_nonce(rpc: &WsClient, sender: [u8; 32]) -> Result<u64, ChainClientError> {
    rpc.request("minijam_getSystemOpNonce", rpc_params![hex(&sender)])
        .await
        .map_err(map_rpc)
}

pub async fn submit_extrinsic(rpc: &WsClient, encoded: &[u8]) -> Result<Hash, ChainClientError> {
    let result: String = rpc
        .request("author_submitExtrinsic", rpc_params![hex(encoded)])
        .await
        .map_err(map_rpc)?;
    decode_hash(&result)
}

pub fn hex(bytes: &[u8]) -> String {
    format!(
        "0x{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn decode_hash(value: &str) -> Result<Hash, ChainClientError> {
    decode_hex(value)?
        .try_into()
        .map_err(|_| ChainClientError::Decode("expected 32-byte hash".into()))
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ChainClientError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() % 2 != 0 {
        return Err(ChainClientError::Decode("odd-length hex".into()));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|error| ChainClientError::Decode(error.to_string()))
        })
        .collect()
}

fn deserialize_hash<'de, D>(deserializer: D) -> Result<Hash, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    decode_hash(&value).map_err(serde::de::Error::custom)
}

fn map_rpc(error: jsonrpsee::core::ClientError) -> ChainClientError {
    let message = error.to_string();
    if message.contains("1010") || message.contains("Invalid Transaction") {
        ChainClientError::Dispatch(message)
    } else {
        ChainClientError::Rpc(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalized_context_decodes_hex_hashes_and_stage0_slot() {
        let value = serde_json::json!({
            "blockHash": hex(&[1; 32]),
            "blockNumber": 12,
            "stateRoot": hex(&[2; 32]),
            "slot": 12
        });
        let context: FinalizedContext = serde_json::from_value(value).unwrap();

        assert_eq!(context.block_hash, [1; 32]);
        assert_eq!(context.state_root, [2; 32]);
        assert_eq!(context.block_number, context.slot);
    }

    #[test]
    fn hex_decoder_rejects_malformed_responses() {
        assert!(decode_hex("0x0").is_err());
        assert!(decode_hash("0x00").is_err());
    }

    #[test]
    fn standard_account_rpc_param_is_ss58() {
        use sp_core::crypto::Ss58Codec;

        let encoded = crate::account_id_rpc_param([0x61; 32]);
        assert_eq!(
            sp_core::crypto::AccountId32::from_ss58check(&encoded).unwrap(),
            [0x61; 32].into()
        );
        assert!(!encoded.starts_with("0x"));
    }
}
