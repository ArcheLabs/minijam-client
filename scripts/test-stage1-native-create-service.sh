#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
NODE_BIN="${MINIJAM_NATIVE_NODE_BIN:?set the built MiniJAM node binary}"
FORMAL_RPC_BIN="${MINIJAM_NATIVE_FORMAL_RPC_BIN:?set the built Formal RPC binary}"
RELAYER_URI="${MINIJAM_NATIVE_RELAYER_URI:?set the ingress relayer signing URI}"
RELAYER_PUBLIC_KEY="${MINIJAM_NATIVE_INGRESS_RELAYER_PUBLIC_KEY:?set the ingress relayer AccountId32 public key}"
ALLOCATION_PUBLIC_KEY="${MINIJAM_NATIVE_ALLOCATION_RELAYER_PUBLIC_KEY:-${RELAYER_PUBLIC_KEY}}"
SERVICE_BLOB="${MINIJAM_NATIVE_SERVICE_BLOB:-${ROOT}/examples/services/counter/artifacts/counter-c.blob}"
SERVICE_CODE_HASH="${MINIJAM_NATIVE_SERVICE_CODE_HASH:?set the BLAKE2-256 hash of the service blob}"
NODE_RPC_PORT="${MINIJAM_NATIVE_NODE_RPC_PORT:-9944}"
FORMAL_RPC_PORT="${MINIJAM_NATIVE_FORMAL_RPC_PORT:-8090}"
READY_TIMEOUT="${MINIJAM_NATIVE_READY_TIMEOUT_SECONDS:-180}"
E2E_TIMEOUT="${MINIJAM_NATIVE_CREATE_SERVICE_TIMEOUT_SECONDS:-180}"
ARTIFACT_DIR="${MINIJAM_NATIVE_ARTIFACT_DIR:-}"

command -v curl >/dev/null 2>&1 || { echo 'curl is required for native Stage-1 E2E' >&2; exit 127; }
command -v jq >/dev/null 2>&1 || { echo 'jq is required for native Stage-1 E2E' >&2; exit 127; }
command -v base64 >/dev/null 2>&1 || { echo 'base64 is required for native Stage-1 E2E' >&2; exit 127; }
command -v python3 >/dev/null 2>&1 || { echo 'python3 is required for native Stage-1 E2E' >&2; exit 127; }
test -x "${NODE_BIN}" || { echo "node binary is not executable: ${NODE_BIN}" >&2; exit 1; }
test -x "${FORMAL_RPC_BIN}" || { echo "Formal RPC binary is not executable: ${FORMAL_RPC_BIN}" >&2; exit 1; }
test -s "${SERVICE_BLOB}" || { echo "service blob is missing or empty: ${SERVICE_BLOB}" >&2; exit 1; }
[[ "${SERVICE_CODE_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] || {
  echo 'MINIJAM_NATIVE_SERVICE_CODE_HASH must be a 0x-prefixed 32-byte hex value' >&2
  exit 1
}
[[ "${RELAYER_PUBLIC_KEY}" =~ ^0x[0-9a-fA-F]{64}$ ]] || {
  echo 'MINIJAM_NATIVE_INGRESS_RELAYER_PUBLIC_KEY must be a 0x-prefixed 32-byte hex value' >&2
  exit 1
}
[[ "${ALLOCATION_PUBLIC_KEY}" =~ ^0x[0-9a-fA-F]{64}$ ]] || {
  echo 'MINIJAM_NATIVE_ALLOCATION_RELAYER_PUBLIC_KEY must be a 0x-prefixed 32-byte hex value' >&2
  exit 1
}

computed_service_code_hash="$(python3 - "${SERVICE_BLOB}" <<'PY'
import hashlib
import pathlib
import sys

print("0x" + hashlib.blake2b(pathlib.Path(sys.argv[1]).read_bytes(), digest_size=32).hexdigest())
PY
)"
test "${computed_service_code_hash,,}" = "${SERVICE_CODE_HASH,,}" || {
  echo 'MINIJAM_NATIVE_SERVICE_CODE_HASH does not match the service blob' >&2
  exit 1
}

TMP="$(mktemp -d)"
NODE_BASE_PATH="${TMP}/node-data"
CHAIN_SPEC="${MINIJAM_NATIVE_CHAIN_SPEC_FILE:-${TMP}/stage1.json}"
NODE_LOG="${TMP}/node.log"
FORMAL_RPC_LOG="${TMP}/formal-rpc.log"
mkdir -p "${NODE_BASE_PATH}" "${TMP}/bundles"

node_pid=''
formal_rpc_pid=''
cleanup() {
  local status=$?
  if (( status != 0 )) && [[ -n "${ARTIFACT_DIR}" ]]; then
    mkdir -p "${ARTIFACT_DIR}"
    cp -f "${NODE_LOG}" "${ARTIFACT_DIR}/node.log" 2>/dev/null || true
    cp -f "${FORMAL_RPC_LOG}" "${ARTIFACT_DIR}/formal-rpc.log" 2>/dev/null || true
    cp -f "${CHAIN_SPEC}" "${ARTIFACT_DIR}/stage1.json" 2>/dev/null || true
    cp -f "${TMP}/create-service-response.json" \
      "${ARTIFACT_DIR}/create-service-response.json" 2>/dev/null || true
    cp -f "${TMP}/finalized-head-samples.log" \
      "${ARTIFACT_DIR}/finalized-head-samples.log" 2>/dev/null || true
  fi
  if [[ -n "${formal_rpc_pid}" ]]; then
    kill "${formal_rpc_pid}" 2>/dev/null || true
    wait "${formal_rpc_pid}" 2>/dev/null || true
  fi
  if [[ -n "${node_pid}" ]]; then
    kill "${node_pid}" 2>/dev/null || true
    wait "${node_pid}" 2>/dev/null || true
  fi
  if (( ${KEEP_MINIJAM_NATIVE_E2E_ARTIFACTS:-0} == 1 )); then
    printf 'native E2E artifacts: %s\n' "${TMP}" >&2
  else
    rm -rf -- "${TMP}"
  fi
  return "${status}"
}
failure_diagnostics() {
  if [[ -f "${NODE_LOG}" ]]; then
    printf '%s\n' '--- native node log ---' >&2
    tail -200 "${NODE_LOG}" >&2 || true
  fi
  if [[ -f "${FORMAL_RPC_LOG}" ]]; then
    printf '%s\n' '--- native Formal RPC log ---' >&2
    tail -200 "${FORMAL_RPC_LOG}" >&2 || true
  fi
}
trap cleanup EXIT
trap failure_diagnostics ERR

if [[ -n "${MINIJAM_NATIVE_CHAIN_SPEC_FILE:-}" ]]; then
  test -s "${CHAIN_SPEC}" || { echo "chain spec is missing or empty: ${CHAIN_SPEC}" >&2; exit 1; }
else
  MINIJAM_STAGE1_INGRESS_RELAYER_PUBLIC_KEY="${RELAYER_PUBLIC_KEY}" \
    MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY="${ALLOCATION_PUBLIC_KEY}" \
    "${NODE_BIN}" build-spec --chain stage1 > "${CHAIN_SPEC}"
fi

rpc_call() {
  local method="$1"
  local params="${2:-[]}"
  curl -fsS --max-time 5 \
    -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" \
    "http://127.0.0.1:${NODE_RPC_PORT}"
}

wait_for_node() {
  local deadline=$((SECONDS + READY_TIMEOUT))
  until health="$(rpc_call system_health 2>/dev/null)" \
    && jq -e '.result != null and .error == null' <<<"${health}" >/dev/null \
    && finalized_head="$(rpc_call chain_getFinalizedHead 2>/dev/null)" \
    && jq -e '.result | strings | length == 66' <<<"${finalized_head}" >/dev/null; do
    (( SECONDS < deadline )) || { echo 'native node RPC/finality did not become ready' >&2; return 1; }
    sleep 2
  done
}

finalized_number() {
  local hash="$1"
  local number_hex
  number_hex="$(rpc_call chain_getHeader "[\"${hash}\"]" | jq -er '.result.number')"
  case "${number_hex}" in
    0x*) printf '%d\n' "$((16#${number_hex#0x}))" ;;
    0X*) printf '%d\n' "$((16#${number_hex#0X}))" ;;
    *) printf '%d\n' "${number_hex}" ;;
  esac
}

wait_for_finality_progress() {
  local initial_head="$1"
  local initial_number
  initial_number="$(finalized_number "${initial_head}")"
  : > "${TMP}/finalized-head-samples.log"
  local deadline=$((SECONDS + READY_TIMEOUT))
  while :; do
    local current_head current_number
    current_head="$(rpc_call chain_getFinalizedHead 2>/dev/null || true)"
    current_head="$(jq -er '.result | strings' <<<"${current_head}" 2>/dev/null || true)"
    if [[ -n "${current_head}" ]]; then
      current_number="$(finalized_number "${current_head}" 2>/dev/null || true)"
      printf '%s %s\n' "${current_head}" "${current_number:-unknown}" \
        >> "${TMP}/finalized-head-samples.log"
      if [[ "${current_number}" =~ ^[0-9]+$ ]] && (( current_number > initial_number )); then
        printf 'NATIVE_FINALITY=PASS (block %s -> %s)\n' "${initial_number}" "${current_number}"
        return 0
      fi
    fi
    (( SECONDS < deadline )) || {
      echo "native finalized head did not advance beyond block ${initial_number}" >&2
      return 1
    }
    sleep 2
  done
}

wait_for_formal_rpc() {
  local deadline=$((SECONDS + READY_TIMEOUT))
  until curl -fsS --max-time 5 "http://127.0.0.1:${FORMAL_RPC_PORT}/health/ready" \
    | jq -e '.status == "ready"' >/dev/null; do
    (( SECONDS < deadline )) || { echo 'native Formal RPC did not become ready' >&2; return 1; }
    sleep 2
  done
}

"${NODE_BIN}" \
  --chain "${CHAIN_SPEC}" \
  --base-path "${NODE_BASE_PATH}" \
  --validator \
  --force-authoring \
  --unsafe-rpc-external \
  --rpc-methods=safe \
  --rpc-cors=all \
  --rpc-port "${NODE_RPC_PORT}" \
  --ws-port "${NODE_RPC_PORT}" \
  --name minijam-native-e2e \
  >"${NODE_LOG}" 2>&1 &
node_pid="$!"

wait_for_node
initial_finalized_head="$(jq -er '.result' <<<"${finalized_head}")"
printf 'NATIVE_NODE_RPC=PASS\n'
wait_for_finality_progress "${initial_finalized_head}"

MINIJAM_RPC_URL="ws://127.0.0.1:${NODE_RPC_PORT}" \
  MINIJAM_FORMAL_RPC_BIND="127.0.0.1:${FORMAL_RPC_PORT}" \
  MINIJAM_RELAYER_URI="${RELAYER_URI}" \
  MINIJAM_BUNDLE_DIR="${TMP}/bundles" \
  "${FORMAL_RPC_BIN}" >"${FORMAL_RPC_LOG}" 2>&1 &
formal_rpc_pid="$!"
wait_for_formal_rpc
printf 'NATIVE_FORMAL_RPC_READY=PASS\n'

blob_base64="$(base64 < "${SERVICE_BLOB}" | tr -d '\r\n')"
request="$(jq -cn \
  --arg codeHash "${SERVICE_CODE_HASH}" \
  --arg blobBase64 "${blob_base64}" \
  '{id: 1, jsonrpc: "2.0", method: "minijam_createServiceV1", params: {
    codeHash: $codeHash,
    blobBase64: $blobBase64,
    minItemGas: 1,
    minMemoGas: 1
  }}')"

response_file="${TMP}/create-service-response.json"
curl -fsS --max-time "${E2E_TIMEOUT}" \
  -H 'content-type: application/json' \
  --data "${request}" \
  "http://127.0.0.1:${FORMAL_RPC_PORT}/" > "${response_file}"

jq -e --arg expected "${SERVICE_CODE_HASH}" '
  .error == null
  and .result.finalized == true
  and (.result.serviceId | type == "number")
  and .result.codeHash == $expected
  and (.result.context.blockHash | strings | test("^0x[0-9a-fA-F]{64}$"))
' "${response_file}" >/dev/null || {
  echo 'native minijam_createServiceV1 returned an invalid or unsuccessful result' >&2
  cat "${response_file}" >&2
  exit 1
}
printf 'NATIVE_CREATE_SERVICE=PASS\n'

for phase in \
  CREATE_SERVICE_REQUEST_RECEIVED \
  CREATE_SERVICE_SYSTEM_OP_PREPARED \
  CREATE_SERVICE_EXTRINSIC_SUBMITTED \
  CREATE_SERVICE_FINALIZED \
  CREATE_SERVICE_WAITING_RECEIPT \
  CREATE_SERVICE_RECEIPT_RECEIVED \
  CREATE_SERVICE_PREIMAGE_SUBMITTED \
  CREATE_SERVICE_PREIMAGE_FINALIZED \
  CREATE_SERVICE_CODE_HASH_CONFIRMED \
  CREATE_SERVICE_COMPLETE; do
  grep -Fq "minijam_createServiceV1 phase=${phase}" "${FORMAL_RPC_LOG}" || {
    echo "missing native CreateService phase: ${phase}" >&2
    exit 1
  }
done
printf 'NATIVE_CREATE_SERVICE_PHASES=PASS\n'
printf 'MINIJAM_NATIVE_CREATE_SERVICE_E2E=PASS\n'
