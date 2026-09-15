#!/usr/bin/env bash
set -euo pipefail

# Consumer-side direct-refine acceptance test. The native node, Formal RPC,
# and one Worker must already be running.
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
NODE_RPC="${MINIJAM_NODE_RPC:-http://127.0.0.1:${MINIJAM_NATIVE_NODE_RPC_PORT:-9944}}"
FORMAL_URL="${MINIJAM_FORMAL_RPC_URL:-http://127.0.0.1:${MINIJAM_NATIVE_FORMAL_RPC_PORT:-8090}}"
SERVICE_BLOB="${MINIJAM_NATIVE_SERVICE_BLOB:-${ROOT}/examples/services/counter/artifacts/counter-c.blob}"
TIMEOUT="${MINIJAM_NATIVE_DIRECT_E2E_TIMEOUT_SECONDS:-240}"
ARTIFACT_DIR="${MINIJAM_NATIVE_DIRECT_E2E_ARTIFACT_DIR:-}"
TMP="$(mktemp -d)"

for command in curl jq base64 python3; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done
test -s "${SERVICE_BLOB}" || { echo "service blob is missing or empty: ${SERVICE_BLOB}" >&2; exit 1; }

cleanup() {
  local status=$?
  if (( status != 0 )) && [[ -n "${ARTIFACT_DIR}" ]]; then
    mkdir -p "${ARTIFACT_DIR}"
    cp -f "${TMP}"/*.json "${ARTIFACT_DIR}/" 2>/dev/null || true
  fi
  if (( status == 0 )) || (( ${KEEP_MINIJAM_NATIVE_E2E_ARTIFACTS:-0} != 1 )); then
    rm -rf -- "${TMP}"
  else
    printf 'native direct E2E artifacts: %s\n' "${TMP}" >&2
  fi
  return "${status}"
}
trap cleanup EXIT

rpc_call() {
  local endpoint="$1" method="$2" params="${3:-[]}"
  curl -fsS --max-time 10 -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" "${endpoint}"
}

curl -fsS --max-time 10 "${FORMAL_URL}/health/ready" | jq -e '.status == "ready"' >/dev/null
rpc_call "${NODE_RPC}" system_health | jq -e '.result != null and .error == null' >/dev/null
printf 'MINIJAM_NATIVE_DIRECT_PROVIDER_READY=PASS\n'

service_code_hash="$(python3 - "${SERVICE_BLOB}" <<'PY'
import hashlib
import pathlib
import sys
print("0x" + hashlib.blake2b(pathlib.Path(sys.argv[1]).read_bytes(), digest_size=32).hexdigest())
PY
)"
blob_base64="$(base64 <"${SERVICE_BLOB}" | tr -d '\r\n')"
create_request="$(jq -cn --arg code_hash "${service_code_hash}" --arg blob_base64 "${blob_base64}" \
  '{id: 1, jsonrpc: "2.0", method: "minijam_createServiceV1", params: {
    codeHash: $code_hash, blobBase64: $blob_base64, minItemGas: 1, minMemoGas: 1
  }}')"
curl -fsS --max-time "${TIMEOUT}" -H 'content-type: application/json' \
  --data "${create_request}" "${FORMAL_URL}/" >"${TMP}/create-service.json"
jq -e --arg expected "${service_code_hash}" \
  '.error == null and .result.finalized == true and (.result.serviceId | type == "number") and .result.codeHash == $expected' \
  "${TMP}/create-service.json" >/dev/null
service_id="$(jq -er '.result.serviceId' "${TMP}/create-service.json")"
context="$(jq -c '.result.context | {blockHash, stateRoot, slot}' "${TMP}/create-service.json")"
printf 'MINIJAM_NATIVE_DIRECT_SERVICE_READY=PASS\n'

submit_transaction() {
  local payload_base64="$1" output="$2"
  jq -cn --arg service_id "${service_id}" --arg code_hash "${service_code_hash}" --arg payload "${payload_base64}" \
    '{id: 1, jsonrpc: "2.0", method: "minijam_submitTransactionV1", params: {
      serviceId: ($service_id | tonumber), serviceCodeHash: $code_hash,
      payloadBase64: $payload, extrinsicsBase64: []
    }}' \
    | curl -fsS --max-time 30 -H 'content-type: application/json' --data @- "${FORMAL_URL}/" >"${output}"
  jq -e '.error == null and (.result.transactionId | strings | test("^0x[0-9a-fA-F]{64}$"))' "${output}" >/dev/null
}

payload_one="$(printf '\001\000\000\000\000\000\000\000' | base64 | tr -d '\r\n')"
payload_two="$(printf '\002\000\000\000\000\000\000\000' | base64 | tr -d '\r\n')"
submit_transaction "${payload_one}" "${TMP}/submit-one.json"
submit_transaction "${payload_two}" "${TMP}/submit-two.json"
transaction_one="$(jq -er '.result.transactionId' "${TMP}/submit-one.json")"
transaction_two="$(jq -er '.result.transactionId' "${TMP}/submit-two.json")"
printf 'MINIJAM_NATIVE_DIRECT_TRANSACTION_INGRESS=PASS\n'

deadline=$((SECONDS + TIMEOUT))
last_one='{}'
last_two='{}'
while (( SECONDS < deadline )); do
  rpc_call "${FORMAL_URL}" minijam_getTransactionStatusV1 "$(jq -cn --arg id "${transaction_one}" '{transactionId: $id}')" >"${TMP}/status-one.json"
  rpc_call "${FORMAL_URL}" minijam_getTransactionStatusV1 "$(jq -cn --arg id "${transaction_two}" '{transactionId: $id}')" >"${TMP}/status-two.json"
  last_one="$(jq -c '.result // .error' "${TMP}/status-one.json")"
  last_two="$(jq -c '.result // .error' "${TMP}/status-two.json")"
  state_one="$(jq -er '.result.status // "unknown"' "${TMP}/status-one.json")"
  state_two="$(jq -er '.result.status // "unknown"' "${TMP}/status-two.json")"
  if [[ "${state_one}" == failed || "${state_two}" == failed ]]; then
    echo "direct transaction failed: ${last_one} / ${last_two}" >&2
    exit 1
  fi
  if [[ "${state_one}" == imported && "${state_two}" == imported ]]; then
    break
  fi
  sleep 1
done
[[ "${state_one:-}" == imported && "${state_two:-}" == imported ]] || {
  echo "direct transactions did not import before timeout: ${last_one} / ${last_two}" >&2
  exit 1
}

package_one="$(jq -er '.result.packageHash' "${TMP}/status-one.json")"
package_two="$(jq -er '.result.packageHash' "${TMP}/status-two.json")"
index_one="$(jq -er '.result.itemIndex' "${TMP}/status-one.json")"
index_two="$(jq -er '.result.itemIndex' "${TMP}/status-two.json")"
[[ "${package_one}" == "${package_two}" ]] || { echo 'batched transactions used different packages' >&2; exit 1; }
[[ "${index_one}" == 0 && "${index_two}" == 1 ]] || { echo "unexpected batch item indexes: ${index_one}, ${index_two}" >&2; exit 1; }
receipt_one="$(jq -er '.result.executionReceipt' "${TMP}/status-one.json")"
receipt_two="$(jq -er '.result.executionReceipt' "${TMP}/status-two.json")"
[[ "${receipt_one}" == "${receipt_two}" ]] || { echo 'batched transactions did not share a package receipt' >&2; exit 1; }
printf 'MINIJAM_NATIVE_DIRECT_PACKAGE_IMPORTED=PASS\n'
printf 'MINIJAM_NATIVE_DIRECT_PACKAGE_HASH=%s\n' "${package_one}"
printf 'MINIJAM_NATIVE_DIRECT_RECEIPT=%s\n' "${receipt_one}"
