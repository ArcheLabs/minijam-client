#!/usr/bin/env bash
set -euo pipefail

# Consumer-side Work gate for the canonical local launcher. The provider is
# started by test-minijam-local-e2e.sh; this script only consumes its public
# node and Formal RPC APIs.
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
NODE_RPC="${MINIJAM_NODE_RPC:-http://127.0.0.1:9944}"
FORMAL_URL="${MINIJAM_FORMAL_RPC_URL:-http://127.0.0.1:8080}"
SERVICE_BLOB="${MINIJAM_SERVICE_BLOB:-${ROOT}/examples/services/counter/artifacts/counter-c.blob}"
TIMEOUT="${MINIJAM_WORK_E2E_TIMEOUT_SECONDS:-240}"
TMP="$(mktemp -d)"

for command in curl jq base64 python3; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done
test -s "${SERVICE_BLOB}" || { echo "service blob is missing: ${SERVICE_BLOB}" >&2; exit 1; }

cleanup() {
  local status=$?
  if (( status != 0 )); then
    echo "canonical local Work artifacts: ${TMP}" >&2
  else
    rm -rf -- "${TMP}"
  fi
  return "${status}"
}
trap cleanup EXIT

rpc_call() {
  local endpoint="$1" method="$2" params="${3:-[]}"
  curl -fsS --max-time 10 -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" \
    "${endpoint}"
}

block_number() {
  local hash="$1" value
  value="$(rpc_call "${NODE_RPC}" chain_getHeader "[\"${hash}\"]" | jq -er '.result.number')"
  case "${value}" in
    0x*) printf '%d\n' "$((16#${value#0x}))" ;;
    0X*) printf '%d\n' "$((16#${value#0X}))" ;;
    *) printf '%d\n' "${value}" ;;
  esac
}

node_health="$(rpc_call "${NODE_RPC}" system_health)"
jq -e '.result != null and .error == null' <<<"${node_health}" >/dev/null
jq -e '.status == "ready"' < <(curl -fsS --max-time 10 "${FORMAL_URL}/health/ready") >/dev/null
printf 'MINIJAM_DEV_WORK_PROVIDER_READY=PASS\n'

service_code_hash="$(python3 - "${SERVICE_BLOB}" <<'PY'
import hashlib
import pathlib
import sys

print("0x" + hashlib.blake2b(pathlib.Path(sys.argv[1]).read_bytes(), digest_size=32).hexdigest())
PY
)"
blob_base64="$(base64 <"${SERVICE_BLOB}" | tr -d '\r\n')"
create_request="$(jq -cn \
  --arg code_hash "${service_code_hash}" \
  --arg blob_base64 "${blob_base64}" \
  '{id: 1, jsonrpc: "2.0", method: "minijam_createServiceV1", params: {
    codeHash: $code_hash, blobBase64: $blob_base64, minItemGas: 1, minMemoGas: 1
  }}')"
curl -fsS --max-time "${TIMEOUT}" -H 'content-type: application/json' \
  --data "${create_request}" "${FORMAL_URL}/" >"${TMP}/create-service.json"
jq -e --arg expected "${service_code_hash}" '
  .error == null and .result.finalized == true
  and (.result.serviceId | type == "number") and .result.codeHash == $expected
' "${TMP}/create-service.json" >/dev/null
service_id="$(jq -er '.result.serviceId' "${TMP}/create-service.json")"
context="$(jq -c '.result.context | {blockHash, stateRoot, slot}' "${TMP}/create-service.json")"
printf 'MINIJAM_DEV_SERVICE_FINALIZED=PASS\n'

payload_base64="$(printf '\001\000\000\000\000\000\000\000' | base64 | tr -d '\r\n')"
submitted=0
for _ in $(seq 1 12); do
  work_request="$(jq -cn \
    --argjson context "${context}" \
    --arg service_id "${service_id}" \
    --arg code_hash "${service_code_hash}" \
    --arg payload "${payload_base64}" \
    '{id: 1, jsonrpc: "2.0", method: "minijam_submitWorkV1", params: {
      context: $context, serviceId: ($service_id | tonumber), serviceCodeHash: $code_hash,
      payloadBase64: $payload, extrinsicsBase64: []
    }}')"
  curl -fsS --max-time 30 -H 'content-type: application/json' \
    --data "${work_request}" "${FORMAL_URL}/" >"${TMP}/submit-work.json"
  if jq -e '.error == null and .result.packageHash != null' "${TMP}/submit-work.json" >/dev/null; then
    submitted=1
    break
  fi
  if jq -e '.error.code == -32010 and .error.data.blockHash != null' "${TMP}/submit-work.json" >/dev/null; then
    context="$(jq -c '.error.data | {blockHash, stateRoot, slot}' "${TMP}/submit-work.json")"
    sleep 1
    continue
  fi
  cat "${TMP}/submit-work.json" >&2
  exit 1
done
(( submitted == 1 )) || { echo 'canonical local Work submission timed out' >&2; exit 1; }
package_hash="$(jq -er '.result.packageHash' "${TMP}/submit-work.json")"
printf 'MINIJAM_DEV_WORK_SUBMITTED=PASS\n'

status_request="$(jq -cn --arg package_hash "${package_hash}" \
  '{id: 1, jsonrpc: "2.0", method: "minijam_getWorkStatusV1", params: {packageHash: $package_hash}}')"
deadline=$((SECONDS + TIMEOUT))
while (( SECONDS < deadline )); do
  curl -fsS --max-time 10 -H 'content-type: application/json' \
    --data "${status_request}" "${FORMAL_URL}/" >"${TMP}/work-status.json" || true
  if jq -e '.error == null and .result.status == "imported"' "${TMP}/work-status.json" >/dev/null 2>&1; then
    imported_block="$(jq -er '.result.context.blockNumber' "${TMP}/work-status.json")"
    receipt="$(jq -er '.result.executionReceipt | strings | select(test("^0x[0-9a-fA-F]{64}$"))' "${TMP}/work-status.json")"
    finalized_head="$(rpc_call "${NODE_RPC}" chain_getFinalizedHead | jq -er '.result')"
    finalized_number="$(block_number "${finalized_head}")"
    if (( finalized_number >= imported_block )); then
      printf 'MINIJAM_DEV_WORK_EXECUTION_RECEIPT=%s\n' "${receipt}"
      printf 'MINIJAM_DEV_WORK_IMPORTED=PASS\n'
      printf 'MINIJAM_DEV_WORK_FINALIZED=PASS\n'
      exit 0
    fi
  fi
  sleep 1
done

echo 'canonical local Work did not reach Imported at finalized state' >&2
cat "${TMP}/work-status.json" >&2 || true
exit 1
