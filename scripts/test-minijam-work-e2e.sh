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
CONTAINER="${MINIJAM_LOCAL_CONTAINER:-}"
TMP="$(mktemp -d)"

for command in curl jq base64 python3; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done
if [[ -n "${CONTAINER}" ]]; then
  command -v docker >/dev/null 2>&1 || { echo 'docker is required when MINIJAM_LOCAL_CONTAINER is set' >&2; exit 127; }
fi
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

provider_running() {
  [[ -z "${CONTAINER}" ]] ||
    [[ "$(docker inspect --format '{{.State.Running}}' "${CONTAINER}" 2>/dev/null || true)" == true ]]
}

provider_exit_diagnostic() {
  [[ -z "${CONTAINER}" ]] && return 0
  echo 'aggregate MiniJAM container exited during Work E2E' >&2
  echo '----- aggregate MiniJAM container logs (last 300 lines) -----' >&2
  docker logs --tail 300 "${CONTAINER}" >&2 || true
  echo '----- end aggregate MiniJAM container logs -----' >&2
}

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
context_hash="$(jq -er '.blockHash' <<<"${context}")"
printf 'CREATE_SERVICE_RETURNED=PASS\n'
service_info="$(rpc_call "${NODE_RPC}" minijam_getServiceInfoAt "$(jq -cn --arg hash "${context_hash}" --arg id "${service_id}" '[ $hash, ($id | tonumber) ]')")"
jq -e '.result | type == "string"' <<<"${service_info}" >/dev/null || {
  echo 'ServiceInfo is not present at the createService finalized context' >&2
  exit 1
}
printf '%s\n' "${service_info}" >"${TMP}/service-info.json"
python3 - "${TMP}/service-info.json" "${service_code_hash}" <<'PY'
import json
import pathlib
import sys

def take_compact(data, offset):
    first = data[offset]
    mode = first & 3
    if mode == 0:
        return first >> 2, offset + 1
    if mode == 1:
        return int.from_bytes(data[offset:offset + 2], "little") >> 2, offset + 2
    if mode == 2:
        return int.from_bytes(data[offset:offset + 4], "little") >> 2, offset + 4
    size = (first >> 2) + 4
    return int.from_bytes(data[offset + 1:offset + 1 + size], "little"), offset + 1 + size

response_path, expected_hash = sys.argv[1:]
encoded = bytes.fromhex(json.loads(pathlib.Path(response_path).read_text())["result"][2:])
value_len, offset = take_compact(encoded, 0)
value = encoded[offset:]
if len(value) != value_len or len(value) < 33:
    raise SystemExit("invalid finalized ServiceInfo StateValue")
actual_hash = "0x" + value[1:33].hex()
if actual_hash.lower() != expected_hash.lower():
    raise SystemExit("finalized ServiceInfo codeHash does not match requested blob")
PY
printf 'RETURNED_CONTEXT_SERVICE_INFO=PASS\n'
service_preimage="$(rpc_call "${NODE_RPC}" minijam_getServicePreimageAt "$(jq -cn --arg hash "${context_hash}" --arg id "${service_id}" --arg code_hash "${service_code_hash}" '[ $hash, ($id | tonumber), $code_hash ]')")"
jq -e '.result != null' <<<"${service_preimage}" >/dev/null || {
  echo 'Service preimage is not present at the createService finalized context' >&2
  exit 1
}
printf 'RETURNED_CONTEXT_SERVICE_PREIMAGE=PASS\n'
printf '%s\n' "${service_preimage}" >"${TMP}/service-preimage.json"
python3 - "${TMP}/service-preimage.json" "${SERVICE_BLOB}" "${service_id}" "${service_code_hash}" <<'PY'
import hashlib
import json
import pathlib
import sys

def take_compact(data, offset):
    first = data[offset]
    mode = first & 3
    if mode == 0:
        return first >> 2, offset + 1
    if mode == 1:
        return int.from_bytes(data[offset:offset + 2], "little") >> 2, offset + 2
    if mode == 2:
        return int.from_bytes(data[offset:offset + 4], "little") >> 2, offset + 4
    size = (first >> 2) + 4
    return int.from_bytes(data[offset + 1:offset + 1 + size], "little"), offset + 1 + size

response_path, blob_path, _service_id, expected_hash = sys.argv[1:]
encoded = bytes.fromhex(json.loads(pathlib.Path(response_path).read_text())["result"][2:])
blob_len, offset = take_compact(encoded, 0)
blob = encoded[offset:]
if len(blob) != blob_len:
    raise SystemExit("invalid finalized Service preimage StateValue")
expected_blob = pathlib.Path(blob_path).read_bytes()
actual_hash = "0x" + hashlib.blake2b(blob, digest_size=32).hexdigest()
if blob != expected_blob:
    raise SystemExit("finalized Service preimage blob does not match createService request")
if actual_hash.lower() != expected_hash.lower():
    raise SystemExit("finalized Service preimage hash does not match createService codeHash")
PY
printf 'RETURNED_CONTEXT_PREIMAGE_MATCH=PASS\n'

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
    continue
  fi
  cat "${TMP}/submit-work.json" >&2
  exit 1
done
(( submitted == 1 )) || { echo 'canonical local Work submission timed out' >&2; exit 1; }
package_hash="$(jq -er '.result.packageHash' "${TMP}/submit-work.json")"
printf 'MINIJAM_DEV_WORK_SUBMITTED=PASS\n'
printf 'IMMEDIATE_FIRST_WORK=PASS\n'

status_request="$(jq -cn --arg package_hash "${package_hash}" \
  '{id: 1, jsonrpc: "2.0", method: "minijam_getWorkStatusV1", params: {packageHash: $package_hash}}')"
deadline=$((SECONDS + TIMEOUT))
while (( SECONDS < deadline )); do
  if ! provider_running; then
    provider_exit_diagnostic
    exit 1
  fi
  if ! curl -fsS --max-time 10 -H 'content-type: application/json' \
    --data "${status_request}" "${FORMAL_URL}/" >"${TMP}/work-status.json"; then
    if ! provider_running; then
      provider_exit_diagnostic
      exit 1
    fi
  fi
  if jq -e '.error == null and .result.status == "imported"' "${TMP}/work-status.json" >/dev/null 2>&1; then
    imported_block="$(jq -er '.result.context.blockNumber' "${TMP}/work-status.json")"
    receipt="$(jq -er '.result.executionReceipt | strings | select(test("^0x[0-9a-fA-F]{64}$"))' "${TMP}/work-status.json")"
    finalized_head="$(rpc_call "${NODE_RPC}" chain_getFinalizedHead | jq -er '.result')"
    finalized_number="$(block_number "${finalized_head}")"
    if (( finalized_number >= imported_block )); then
      storage_context="$(jq -er '.result.context.blockHash' "${TMP}/work-status.json")"
      service_storage="$(rpc_call "${NODE_RPC}" minijam_getServiceStorageAt "$(jq -cn --arg hash "${storage_context}" --arg id "${service_id}" '[ $hash, ($id | tonumber), "0x636f756e746572" ]')")"
      jq -e '.result == "0x200100000000000000"' <<<"${service_storage}" >/dev/null || {
        echo 'first Counter action was imported without the expected state transition' >&2
        cat "${TMP}/work-status.json" >&2
        exit 1
      }
      printf 'MINIJAM_DEV_WORK_EXECUTION_RECEIPT=%s\n' "${receipt}"
      printf 'FIRST_WORK_IMPORTED=PASS\n'
      printf 'FIRST_EXECUTION_RECEIPT=PASS\n'
      printf 'FIRST_CANONICAL_STATE_TRANSITION=PASS\n'
      exit 0
    fi
  fi
  sleep 1
done

echo 'canonical local Work did not reach Imported at finalized state' >&2
cat "${TMP}/work-status.json" >&2 || true
exit 1
