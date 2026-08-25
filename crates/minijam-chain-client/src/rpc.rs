use jsonrpsee::{
    core::client::{ClientT, SubscriptionClientT},
    rpc_params,
    ws_client::WsClient,
};
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
    Ok(event_records_at(rpc, block_hash)
        .await?
        .into_iter()
        .map(|record| record.event)
        .collect())
}

type EventRecord = frame_system::EventRecord<minijam_runtime::RuntimeEvent, sp_core::H256>;

pub async fn event_records_at(
    rpc: &WsClient,
    block_hash: Hash,
) -> Result<Vec<EventRecord>, ChainClientError> {
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
    let records: Vec<EventRecord> = Decode::decode(&mut bytes.as_slice())
        .map_err(|error| ChainClientError::Decode(error.to_string()))?;
    Ok(records)
}

pub async fn dispatch_error_at(
    rpc: &WsClient,
    block_hash: Hash,
    extrinsic_hash: Hash,
) -> Result<Option<String>, ChainClientError> {
    let index = block_extrinsic_index(rpc, block_hash, extrinsic_hash).await?;
    let Some(index) = index else {
        return Err(ChainClientError::Decode(
            "included extrinsic was not found in block".into(),
        ));
    };
    let records = event_records_at(rpc, block_hash).await?;
    Ok(dispatch_error_from_records(&records, index))
}

fn dispatch_error_from_records(records: &[EventRecord], index: u32) -> Option<String> {
    records.iter().find_map(|record| {
        if record.phase != frame_system::Phase::ApplyExtrinsic(index) {
            return None;
        }
        match &record.event {
            minijam_runtime::RuntimeEvent::System(frame_system::Event::ExtrinsicFailed {
                dispatch_error,
                ..
            }) => Some(format!("{dispatch_error:?}")),
            _ => None,
        }
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WatchStatus {
    Continue,
    Final,
    Failed,
}

fn classify_watch_status(status: &serde_json::Value) -> WatchStatus {
    if status.get("dropped").is_some()
        || status.get("invalid").is_some()
        || status.get("usurped").is_some()
        || status.get("finalityTimeout").is_some()
    {
        WatchStatus::Failed
    } else if status.get("finalized").is_some() {
        WatchStatus::Final
    } else {
        // InBlock is deliberately non-final. Retracted also keeps the stream
        // alive because a later canonical InBlock may follow it.
        WatchStatus::Continue
    }
}

#[derive(serde::Deserialize)]
struct BlockResponse {
    block: BlockBody,
}

#[derive(serde::Deserialize)]
struct BlockBody {
    extrinsics: Vec<String>,
}

async fn block_extrinsic_index(
    rpc: &WsClient,
    block_hash: Hash,
    extrinsic_hash: Hash,
) -> Result<Option<u32>, ChainClientError> {
    let block: BlockResponse = rpc
        .request("chain_getBlock", rpc_params![hex(&block_hash)])
        .await
        .map_err(map_rpc)?;
    for (index, encoded) in block.block.extrinsics.iter().enumerate() {
        let bytes = decode_hex(encoded)?;
        if minijam_protocol::blake2_256(&bytes) == extrinsic_hash {
            return Ok(Some(index as u32));
        }
    }
    Ok(None)
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

/// Submit through the transaction watcher. A hash returned by
/// `author_submitExtrinsic` only proves pool acceptance; the watcher gives us
/// the subsequent ready/in-block/finalized/dropped status.
pub async fn submit_and_watch_extrinsic(
    rpc: &WsClient,
    encoded: &[u8],
    timeout: std::time::Duration,
) -> Result<(Hash, Vec<serde_json::Value>), ChainClientError> {
    let mut subscription = rpc
        .subscribe::<serde_json::Value, _>(
            "author_submitAndWatchExtrinsic",
            rpc_params![hex(encoded)],
            "author_unwatchExtrinsic",
        )
        .await
        .map_err(map_rpc)?;
    let started = std::time::Instant::now();
    let mut statuses = Vec::new();
    let extrinsic_hash = minijam_protocol::blake2_256(encoded);
    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(ChainClientError::Rpc(
                "timed out waiting for transaction status".into(),
            ));
        }
        let status = tokio::time::timeout(remaining, subscription.next())
            .await
            .map_err(|_| ChainClientError::Rpc("timed out waiting for transaction status".into()))?
            .ok_or_else(|| ChainClientError::Rpc("transaction status subscription ended".into()))?;
        let status = status.map_err(|error| ChainClientError::Rpc(error.to_string()))?;
        match classify_watch_status(&status) {
            WatchStatus::Failed => {
                return Err(ChainClientError::TransactionFailed(status.to_string()));
            }
            WatchStatus::Final => {
                statuses.push(status);
                return Ok((extrinsic_hash, statuses));
            }
            WatchStatus::Continue => {}
        }
        statuses.push(status);
    }
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

    #[test]
    fn watcher_keeps_non_final_states_and_retries_terminal_failures() {
        assert_eq!(
            classify_watch_status(&serde_json::json!({"inBlock": hex(&[1; 32])})),
            WatchStatus::Continue
        );
        assert_eq!(
            classify_watch_status(&serde_json::json!({"retracted": hex(&[1; 32])})),
            WatchStatus::Continue
        );
        assert_eq!(
            classify_watch_status(&serde_json::json!({"finalized": hex(&[2; 32])})),
            WatchStatus::Final
        );
        for status in [
            serde_json::json!({"dropped": null}),
            serde_json::json!({"invalid": null}),
            serde_json::json!({"usurped": hex(&[3; 32])}),
            serde_json::json!({"finalityTimeout": null}),
        ] {
            assert_eq!(classify_watch_status(&status), WatchStatus::Failed);
        }
    }

    #[test]
    fn dispatch_failure_is_scoped_to_the_matching_extrinsic_phase() {
        let records = vec![
            frame_system::EventRecord {
                phase: frame_system::Phase::ApplyExtrinsic(0),
                event: minijam_runtime::RuntimeEvent::System(
                    frame_system::Event::ExtrinsicSuccess {
                        dispatch_info: Default::default(),
                    },
                ),
                topics: Vec::new(),
            },
            frame_system::EventRecord {
                phase: frame_system::Phase::ApplyExtrinsic(1),
                event: minijam_runtime::RuntimeEvent::System(
                    frame_system::Event::ExtrinsicFailed {
                        dispatch_error: sp_runtime::DispatchError::Other("unrelated"),
                        dispatch_info: Default::default(),
                    },
                ),
                topics: Vec::new(),
            },
        ];
        assert!(dispatch_error_from_records(&records, 0).is_none());
        assert!(dispatch_error_from_records(&records, 1)
            .expect("matching failure")
            .contains("unrelated"));
    }
}
