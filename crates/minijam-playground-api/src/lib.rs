// SPDX-License-Identifier: Apache-2.0

use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use parity_scale_codec::Encode;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair};
use thiserror::Error;

pub const ACTION_DOMAIN: &[u8] = b"minijam/playground-action/v1";
static ACTION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct PlaygroundConfig {
    pub genesis_hash: [u8; 32],
    pub compiler_url: String,
}

#[derive(Clone)]
pub struct Playground {
    config: PlaygroundConfig,
    db: Arc<Mutex<Connection>>,
    compiler: reqwest::Client,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareActionRequest {
    pub account: String,
    pub action: String,
    pub params_hash: String,
    pub expiry: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAction {
    pub action_id: String,
    pub account: String,
    pub action: String,
    pub params_hash: String,
    pub domain: String,
    pub genesis: String,
    pub expiry: u64,
    pub signing_payload: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionAuthorization {
    pub action_id: String,
    pub signature: String,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("signed action expired")]
    Expired,
    #[error("signed action was already used")]
    Replayed,
    #[error("signature does not match action account")]
    InvalidSignature,
    #[error("signed action parameters do not match request")]
    ParamsMismatch,
    #[error("storage error: {0}")]
    Storage(String),
    #[error("compiler unavailable: {0}")]
    Compiler(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Invalid(_) | Self::InvalidSignature | Self::ParamsMismatch => {
                StatusCode::BAD_REQUEST
            }
            Self::Expired | Self::Replayed => StatusCode::CONFLICT,
            Self::Storage(_) | Self::Compiler(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (
            status,
            Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}

impl Playground {
    pub fn open(path: &Path, config: PlaygroundConfig) -> Result<Self, ApiError> {
        let connection =
            Connection::open(path).map_err(|error| ApiError::Storage(error.to_string()))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS signed_actions (
                   action_id BLOB PRIMARY KEY,
                   account BLOB NOT NULL,
                   action TEXT NOT NULL,
                   params_hash BLOB NOT NULL,
                   genesis_hash BLOB NOT NULL,
                   expiry INTEGER NOT NULL,
                   used_at INTEGER
                 );",
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        Ok(Self {
            config,
            db: Arc::new(Mutex::new(connection)),
            compiler: reqwest::Client::new(),
        })
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/api/v1/build", post(build))
            .route("/api/v1/actions/prepare", post(prepare_action))
            .route("/health/live", get(|| async { StatusCode::NO_CONTENT }))
            .route("/health/ready", get(ready))
            .with_state(self)
    }

    fn prepare(&self, request: PrepareActionRequest) -> Result<PreparedAction, ApiError> {
        let account = decode_array::<32>(&request.account)?;
        let params_hash = decode_array::<32>(&request.params_hash)?;
        if request.action.is_empty() {
            return Err(ApiError::Invalid("action must not be empty".into()));
        }
        if request.expiry <= now() {
            return Err(ApiError::Expired);
        }
        let seed = (
            account,
            request.action.as_bytes(),
            params_hash,
            request.expiry,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            ACTION_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        )
            .encode();
        let action_id = minijam_protocol::blake2_256(&seed);
        let payload = action_payload(
            action_id,
            account,
            &request.action,
            params_hash,
            self.config.genesis_hash,
            request.expiry,
        );
        let signing_hash = minijam_protocol::blake2_256(&payload);
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .execute(
                "INSERT OR IGNORE INTO signed_actions
                 (action_id, account, action, params_hash, genesis_hash, expiry)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    action_id.as_slice(),
                    account.as_slice(),
                    request.action,
                    params_hash.as_slice(),
                    self.config.genesis_hash.as_slice(),
                    request.expiry as i64,
                ],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        Ok(PreparedAction {
            action_id: hex(&action_id),
            account: hex(&account),
            action: request.action,
            params_hash: hex(&params_hash),
            domain: String::from_utf8_lossy(ACTION_DOMAIN).into_owned(),
            genesis: hex(&self.config.genesis_hash),
            expiry: request.expiry,
            signing_payload: hex(&signing_hash),
        })
    }

    pub fn consume_action(
        &self,
        authorization: &ActionAuthorization,
        expected_action: &str,
        expected_params_hash: [u8; 32],
    ) -> Result<[u8; 32], ApiError> {
        let action_id = decode_array::<32>(&authorization.action_id)?;
        let signature = decode_array::<64>(&authorization.signature)?;
        let mut connection = self.db.lock().expect("playground db mutex poisoned");
        let transaction = connection
            .transaction()
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        let row = transaction
            .query_row(
                "SELECT account, action, params_hash, genesis_hash, expiry, used_at
                 FROM signed_actions WHERE action_id = ?1",
                params![action_id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)? as u64,
                        row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                    ))
                },
            )
            .optional()
            .map_err(|error| ApiError::Storage(error.to_string()))?
            .ok_or_else(|| ApiError::Invalid("unknown action id".into()))?;
        if row.5.is_some() {
            return Err(ApiError::Replayed);
        }
        if row.4 <= now() {
            return Err(ApiError::Expired);
        }
        let account: [u8; 32] = row
            .0
            .try_into()
            .map_err(|_| ApiError::Invalid("bad account".into()))?;
        let params_hash: [u8; 32] = row
            .2
            .try_into()
            .map_err(|_| ApiError::Invalid("bad params hash".into()))?;
        let genesis: [u8; 32] = row
            .3
            .try_into()
            .map_err(|_| ApiError::Invalid("bad genesis".into()))?;
        if row.1 != expected_action || params_hash != expected_params_hash {
            return Err(ApiError::ParamsMismatch);
        }
        let signing_hash = minijam_protocol::blake2_256(&action_payload(
            action_id,
            account,
            &row.1,
            params_hash,
            genesis,
            row.4,
        ));
        if !sr25519::Pair::verify(
            &sr25519::Signature::from_raw(signature),
            &signing_hash,
            &sr25519::Public::from_raw(account),
        ) {
            return Err(ApiError::InvalidSignature);
        }
        let changed = transaction
            .execute(
                "UPDATE signed_actions SET used_at = ?2
                 WHERE action_id = ?1 AND used_at IS NULL",
                params![action_id.as_slice(), now() as i64],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        if changed != 1 {
            return Err(ApiError::Replayed);
        }
        transaction
            .commit()
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        Ok(account)
    }
}

async fn prepare_action(
    State(playground): State<Playground>,
    Json(request): Json<PrepareActionRequest>,
) -> Result<Json<PreparedAction>, ApiError> {
    playground.prepare(request).map(Json)
}

async fn build(
    State(playground): State<Playground>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let response = playground
        .compiler
        .post(format!(
            "{}/internal/v1/compile",
            playground.config.compiler_url.trim_end_matches('/')
        ))
        .json(&request)
        .send()
        .await
        .map_err(|error| ApiError::Compiler(error.to_string()))?;
    let status = response.status();
    let body = response
        .json()
        .await
        .map_err(|error| ApiError::Compiler(error.to_string()))?;
    if !status.is_success() {
        return Err(ApiError::Compiler(format!("compiler returned {status}")));
    }
    Ok(Json(body))
}

async fn ready(State(playground): State<Playground>) -> StatusCode {
    match playground
        .db
        .lock()
        .expect("playground db mutex poisoned")
        .query_row("SELECT 1", [], |row| row.get::<_, u8>(0))
    {
        Ok(1) => StatusCode::NO_CONTENT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn action_payload(
    action_id: [u8; 32],
    account: [u8; 32],
    action: &str,
    params_hash: [u8; 32],
    genesis: [u8; 32],
    expiry: u64,
) -> Vec<u8> {
    (
        ACTION_DOMAIN,
        action_id,
        account,
        action.as_bytes(),
        params_hash,
        genesis,
        expiry,
    )
        .encode()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn hex(bytes: &[u8]) -> String {
    format!(
        "0x{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn decode_array<const N: usize>(value: &str) -> Result<[u8; N], ApiError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() != N * 2 {
        return Err(ApiError::Invalid(format!("expected {N}-byte hex")));
    }
    let mut output = [0; N];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|error| ApiError::Invalid(error.to_string()))?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playground(path: &Path) -> Playground {
        Playground::open(
            path,
            PlaygroundConfig {
                genesis_hash: [9; 32],
                compiler_url: "http://127.0.0.1:1".into(),
            },
        )
        .unwrap()
    }

    #[test]
    fn signed_action_is_bound_to_params_and_consumed_once() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));
        let pair = sr25519::Pair::from_seed(&[3; 32]);
        let params_hash = [7; 32];
        let prepared = playground
            .prepare(PrepareActionRequest {
                account: hex(&pair.public().0),
                action: "create".into(),
                params_hash: hex(&params_hash),
                expiry: now() + 60,
            })
            .unwrap();
        let signing_hash = decode_array::<32>(&prepared.signing_payload).unwrap();
        let authorization = ActionAuthorization {
            action_id: prepared.action_id,
            signature: hex(&pair.sign(&signing_hash).0),
        };

        assert_eq!(
            playground
                .consume_action(&authorization, "create", params_hash)
                .unwrap(),
            pair.public().0
        );
        assert!(matches!(
            playground.consume_action(&authorization, "create", params_hash),
            Err(ApiError::Replayed)
        ));
    }

    #[test]
    fn signed_action_rejects_substituted_params_without_consuming_it() {
        let temp = tempfile::tempdir().unwrap();
        let playground = playground(&temp.path().join("playground.sqlite"));
        let pair = sr25519::Pair::from_seed(&[4; 32]);
        let prepared = playground
            .prepare(PrepareActionRequest {
                account: hex(&pair.public().0),
                action: "work".into(),
                params_hash: hex(&[1; 32]),
                expiry: now() + 60,
            })
            .unwrap();
        let signing_hash = decode_array::<32>(&prepared.signing_payload).unwrap();
        let authorization = ActionAuthorization {
            action_id: prepared.action_id,
            signature: hex(&pair.sign(&signing_hash).0),
        };

        assert!(matches!(
            playground.consume_action(&authorization, "work", [2; 32]),
            Err(ApiError::ParamsMismatch)
        ));
        assert!(playground
            .consume_action(&authorization, "work", [1; 32])
            .is_ok());
    }
}
