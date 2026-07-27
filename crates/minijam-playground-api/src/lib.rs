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
static OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Create,
    Upgrade,
    Work,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Prepared,
    Submitted,
    WaitingReceipt,
    SubmittingPreimage,
    TrackingWork,
    Succeeded,
    Failed,
}

impl OperationStatus {
    fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub operation_id: String,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub account: String,
    pub action_id: String,
    pub request: serde_json::Value,
    pub correlation: Option<String>,
    pub extrinsic_hash: Option<String>,
    pub submitted_nonce: Option<u32>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
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
                 );
                 CREATE TABLE IF NOT EXISTS operations (
                   operation_id TEXT PRIMARY KEY,
                   kind TEXT NOT NULL,
                   status TEXT NOT NULL,
                   account BLOB NOT NULL,
                   action_id BLOB NOT NULL UNIQUE,
                   request_json TEXT NOT NULL,
                   correlation BLOB,
                   extrinsic_hash BLOB,
                   submitted_nonce INTEGER,
                   result_json TEXT,
                   error TEXT,
                   created_at INTEGER NOT NULL,
                   updated_at INTEGER NOT NULL
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
            .route("/api/v1/operations/{id}", get(get_operation))
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

    pub fn insert_operation(
        &self,
        kind: OperationKind,
        account: [u8; 32],
        action_id: [u8; 32],
        request: &serde_json::Value,
    ) -> Result<Operation, ApiError> {
        let created_at = now();
        let operation_id = hex(&minijam_protocol::blake2_256(
            &(
                b"minijam/operation/v1".as_slice(),
                account,
                action_id,
                created_at,
                OPERATION_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            )
                .encode(),
        ));
        let request_json =
            serde_json::to_string(request).map_err(|error| ApiError::Invalid(error.to_string()))?;
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .execute(
                "INSERT INTO operations
                 (operation_id, kind, status, account, action_id, request_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    operation_id,
                    enum_json(kind)?,
                    enum_json(OperationStatus::Prepared)?,
                    account.as_slice(),
                    action_id.as_slice(),
                    request_json,
                    created_at as i64,
                ],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        self.operation(&operation_id)?
            .ok_or_else(|| ApiError::Storage("inserted operation disappeared".into()))
    }

    pub fn operation(&self, operation_id: &str) -> Result<Option<Operation>, ApiError> {
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .query_row(
                "SELECT operation_id, kind, status, account, action_id, request_json,
                        correlation, extrinsic_hash, submitted_nonce, result_json, error,
                        created_at, updated_at
                 FROM operations WHERE operation_id = ?1",
                params![operation_id],
                decode_operation,
            )
            .optional()
            .map_err(|error| ApiError::Storage(error.to_string()))
    }

    pub fn recoverable_operations(&self) -> Result<Vec<Operation>, ApiError> {
        let connection = self.db.lock().expect("playground db mutex poisoned");
        let mut statement = connection
            .prepare(
                "SELECT operation_id, kind, status, account, action_id, request_json,
                        correlation, extrinsic_hash, submitted_nonce, result_json, error,
                        created_at, updated_at
                 FROM operations ORDER BY created_at, operation_id",
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([], decode_operation)
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        let mut operations = Vec::new();
        for row in rows {
            let operation = row.map_err(|error| ApiError::Storage(error.to_string()))?;
            if !operation.status.terminal() {
                operations.push(operation);
            }
        }
        Ok(operations)
    }

    pub fn update_operation(
        &self,
        operation_id: &str,
        status: OperationStatus,
        correlation: Option<[u8; 32]>,
        extrinsic_hash: Option<[u8; 32]>,
        submitted_nonce: Option<u32>,
        result: Option<&serde_json::Value>,
        error: Option<&str>,
    ) -> Result<(), ApiError> {
        let result = result
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        self.db
            .lock()
            .expect("playground db mutex poisoned")
            .execute(
                "UPDATE operations SET status = ?2, correlation = COALESCE(?3, correlation),
                 extrinsic_hash = COALESCE(?4, extrinsic_hash),
                 submitted_nonce = COALESCE(?5, submitted_nonce),
                 result_json = COALESCE(?6, result_json), error = ?7, updated_at = ?8
                 WHERE operation_id = ?1",
                params![
                    operation_id,
                    enum_json(status)?,
                    correlation.as_ref().map(|value| value.as_slice()),
                    extrinsic_hash.as_ref().map(|value| value.as_slice()),
                    submitted_nonce,
                    result,
                    error,
                    now() as i64,
                ],
            )
            .map_err(|error| ApiError::Storage(error.to_string()))?;
        Ok(())
    }
}

async fn get_operation(
    State(playground): State<Playground>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Operation>, ApiError> {
    playground
        .operation(&id)?
        .map(Json)
        .ok_or_else(|| ApiError::Invalid("operation not found".into()))
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

fn enum_json<T: Serialize>(value: T) -> Result<String, ApiError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| ApiError::Storage("invalid enum value".into()))
}

fn decode_operation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Operation> {
    let kind: String = row.get(1)?;
    let status: String = row.get(2)?;
    let account: Vec<u8> = row.get(3)?;
    let action_id: Vec<u8> = row.get(4)?;
    let request: String = row.get(5)?;
    let correlation: Option<Vec<u8>> = row.get(6)?;
    let extrinsic_hash: Option<Vec<u8>> = row.get(7)?;
    let result: Option<String> = row.get(9)?;
    Ok(Operation {
        operation_id: row.get(0)?,
        kind: serde_json::from_value(serde_json::Value::String(kind)).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        status: serde_json::from_value(serde_json::Value::String(status)).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        account: hex(&account),
        action_id: hex(&action_id),
        request: serde_json::from_str(&request).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        correlation: correlation.map(|value| hex(&value)),
        extrinsic_hash: extrinsic_hash.map(|value| hex(&value)),
        submitted_nonce: row.get(8)?,
        result: result
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
        error: row.get(10)?,
        created_at: row.get::<_, i64>(11)? as u64,
        updated_at: row.get::<_, i64>(12)? as u64,
    })
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

    #[test]
    fn operations_survive_restart_and_only_non_terminal_rows_recover() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("playground.sqlite");
        let operation_id = {
            let playground = playground(&path);
            playground
                .insert_operation(
                    OperationKind::Create,
                    [1; 32],
                    [2; 32],
                    &serde_json::json!({"codeHash": hex(&[3; 32])}),
                )
                .unwrap()
                .operation_id
        };
        let restarted = playground(&path);
        assert_eq!(
            restarted.recoverable_operations().unwrap()[0].operation_id,
            operation_id
        );
        restarted
            .update_operation(
                &operation_id,
                OperationStatus::Succeeded,
                Some([4; 32]),
                Some([5; 32]),
                Some(7),
                Some(&serde_json::json!({"serviceId": 10})),
                None,
            )
            .unwrap();
        assert!(restarted.recoverable_operations().unwrap().is_empty());
        assert_eq!(
            restarted.operation(&operation_id).unwrap().unwrap().status,
            OperationStatus::Succeeded
        );
    }
}
