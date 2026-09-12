#!/usr/bin/env bash
set -euo pipefail

# This is a consumer-side Work acceptance test. The native provider must
# already be running; this script only talks to its node and Formal RPC APIs.
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
NODE_PORT="${MINIJAM_NATIVE_NODE_RPC_PORT:-9944}"
FORMAL_PORT="${MINIJAM_NATIVE_FORMAL_RPC_PORT:-8090}"
NODE_RPC="${MINIJAM_NODE_RPC:-http://127.0.0.1:${NODE_PORT}}"
FORMAL_URL="${MINIJAM_FORMAL_RPC_URL:-http://127.0.0.1:${FORMAL_PORT}}"
SERVICE_BLOB="${MINIJAM_NATIVE_SERVICE_BLOB:-${ROOT}/examples/services/counter/artifacts/counter-c.blob}"
TIMEOUT="${MINIJAM_NATIVE_WORK_E2E_TIMEOUT_SECONDS:-240}"
ARTIFACT_DIR="${MINIJAM_NATIVE_WORK_E2E_ARTIFACT_DIR:-}"
TMP="$(mktemp -d)"

for command in curl jq base64 python3; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done
test -s "${SERVICE_BLOB}" || { echo "service blob is missing or empty: ${SERVICE_BLOB}" >&2; exit 1; }

copy_artifacts() {
  local status=$?
  if [[ -n "${ARTIFACT_DIR}" ]]; then
    mkdir -p "${ARTIFACT_DIR}"
    cp -f "${TMP}"/*.json "${ARTIFACT_DIR}/" 2>/dev/null || true
    cp -f "${TMP}"/*.log "${ARTIFACT_DIR}/" 2>/dev/null || true
  fi
  if (( status != 0 )); then
    echo "native Work E2E artifacts: ${ARTIFACT_DIR:-${TMP}}" >&2
  else
    rm -rf -- "${TMP}"
  fi
  return "${status}"
}
trap copy_artifacts EXIT

rpc_call() {
  local endpoint="$1" method="$2" params="${3:-[]}"
  curl -fsS --max-time 10 -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" "${endpoint}"
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

formal_health="$(curl -fsS --max-time 10 "${FORMAL_URL}/health/ready")"
jq -e '.status == "ready"' <<<"${formal_health}" >/dev/null
node_health="$(rpc_call "${NODE_RPC}" system_health)"
jq -e '.result != null and .error == null' <<<"${node_health}" >/dev/null
printf 'MINIJAM_NATIVE_WORK_PROVIDER_READY=PASS\n'

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
    codeHash: $code_hash,
    blobBase64: $blob_base64,
    minItemGas: 1,
    minMemoGas: 1
  }}')"
curl -fsS --max-time "${TIMEOUT}" -H 'content-type: application/json' \
  --data "${create_request}" "${FORMAL_URL}/" >"${TMP}/create-service.json"
jq -e --arg expected "${service_code_hash}" '
  .error == null
  and .result.finalized == true
  and (.result.serviceId | type == "number")
  and .result.codeHash == $expected
  and (.result.context.blockHash | strings | test("^0x[0-9a-fA-F]{64}$"))
' "${TMP}/create-service.json" >/dev/null
service_id="$(jq -er '.result.serviceId' "${TMP}/create-service.json")"
printf 'MINIJAM_NATIVE_WORK_SERVICE_READY=PASS\n'

# The create-service response includes the operator-facing blockNumber field;
# submitWorkV1 accepts the protocol ContextResult fields only.
context="$(jq -c '.result.context | {blockHash, stateRoot, slot}' "${TMP}/create-service.json")"
payload_base64="$(printf '\001\000\000\000\000\000\000\000' | base64 | tr -d '\r\n')"
work_submitted=0
for _ in $(seq 1 12); do
  work_request="$(jq -cn \
    --argjson context "${context}" \
    --arg service_id "${service_id}" \
    --arg code_hash "${service_code_hash}" \
    --arg payload "${payload_base64}" \
    '{id: 1, jsonrpc: "2.0", method: "minijam_submitWorkV1", params: {
      context: $context,
      serviceId: ($service_id | tonumber),
      serviceCodeHash: $code_hash,
      payloadBase64: $payload,
      extrinsicsBase64: []
    }}')"
  curl -fsS --max-time 30 -H 'content-type: application/json' \
    --data "${work_request}" "${FORMAL_URL}/" >"${TMP}/submit-work.json"
  if jq -e '.error == null and .result.packageHash != null' "${TMP}/submit-work.json" >/dev/null; then
    work_submitted=1
    break
  fi
  if jq -e '.error.code == -32010 and .error.data.blockHash != null' "${TMP}/submit-work.json" >/dev/null; then
    context="$(jq -c '.error.data' "${TMP}/submit-work.json")"
    sleep 1
    continue
  fi
  echo 'native minijam_submitWorkV1 failed' >&2
  cat "${TMP}/submit-work.json" >&2
  exit 1
done
(( work_submitted == 1 )) || { echo 'native Work submission did not succeed before the retry limit' >&2; exit 1; }
package_hash="$(jq -er '.result.packageHash' "${TMP}/submit-work.json")"
printf 'MINIJAM_NATIVE_WORK_INGRESS=PASS\n'

status_request="$(jq -cn --arg package_hash "${package_hash}" \
  '{id: 1, jsonrpc: "2.0", method: "minijam_getWorkStatusV1", params: {packageHash: $package_hash}}')"
status_history="${TMP}/work-status-history.log"
: >"${status_history}"
work_id=''
for _ in $(seq 1 120); do
  work_id_response="$(rpc_call "${NODE_RPC}" minijam_getWorkIdByPackageHash "[\"${package_hash}\"]" 2>/dev/null || true)"
  work_id="$(jq -er '.result | numbers' <<<"${work_id_response}" 2>/dev/null || true)"
  if [[ -z "${work_id}" ]]; then
    status_probe="$(curl -fsS --max-time 10 -H 'content-type: application/json' \
      --data "${status_request}" "${FORMAL_URL}/" 2>/dev/null || true)"
    work_id="$(jq -er '.result.workId | numbers' <<<"${status_probe}" 2>/dev/null || true)"
  fi
  [[ -n "${work_id}" ]] && break
  sleep 0.5
done
[[ -n "${work_id}" ]] || { echo 'native node did not expose the submitted Work id' >&2; exit 1; }
assigned_worker=''
candidate_seen=0
vote_seen=0
imported=0
finalized=0
deadline=$((SECONDS + TIMEOUT))
sample=0
while (( SECONDS < deadline )); do
  sample=$((sample + 1))

  summary="$(rpc_call "${NODE_RPC}" minijam_getPendingWorkTaskSummaryV1 '[]' 2>/dev/null || true)"
  if [[ -n "${work_id}" && -n "${summary}" ]]; then
    assignment="$(jq -c --argjson id "${work_id}" --arg package_hash "${package_hash}" \
      '.result[]? | select(.workId == $id and .packageHash == $package_hash)' <<<"${summary}" | head -n1 || true)"
    if [[ -n "${assignment}" && -z "${assigned_worker}" ]]; then
      assigned_worker="$(jq -er '.candidateProducer' <<<"${assignment}")"
      assigned_workers="$(jq -c '.assignedWorkers' <<<"${assignment}")"
      printf 'MINIJAM_NATIVE_WORK_ASSIGNED_WORKER=%s\n' "${assigned_worker}"
      printf 'MINIJAM_NATIVE_WORK_ASSIGNED_WORKERS=%s\n' "${assigned_workers}"
      printf 'MINIJAM_NATIVE_WORK_ASSIGNMENT=PASS\n'
    fi
  fi

  if [[ -n "${work_id}" ]]; then
    candidate_response="$(rpc_call "${NODE_RPC}" minijam_getCandidate "[${work_id},0]" 2>/dev/null || true)"
    if jq -e '.error == null and .result != null' <<<"${candidate_response}" >/dev/null 2>&1; then
      candidate_seen=1
    fi
  fi

  curl -fsS --max-time 10 -H 'content-type: application/json' \
    --data "${status_request}" "${FORMAL_URL}/" >"${TMP}/work-status-${sample}.json"
  if jq -e '.error == null and .result != null' "${TMP}/work-status-${sample}.json" >/dev/null; then
    status="$(jq -er '.result.status' "${TMP}/work-status-${sample}.json")"
    printf '%s %s\n' "${sample}" "${status}" >>"${status_history}"
    if [[ "${status}" == "voting" || "${status}" == "accepted" || "${status}" == "imported" ]]; then
      candidate_seen=1
    fi
    if [[ "${status}" == "accepted" || "${status}" == "imported" ]]; then
      vote_seen=1
    fi
    if [[ "${status}" == "failed" ]]; then
      echo 'native Work reached failed status' >&2
      cat "${TMP}/work-status-${sample}.json" >&2
      exit 1
    fi
    if [[ "${status}" == "imported" ]]; then
      imported=1
      receipt="$(jq -er '.result.executionReceipt | strings | select(test("^0x[0-9a-fA-F]{64}$"))' "${TMP}/work-status-${sample}.json")"
      imported_block="$(jq -er '.result.context.blockNumber' "${TMP}/work-status-${sample}.json")"
      finalized_head="$(rpc_call "${NODE_RPC}" chain_getFinalizedHead | jq -er '.result')"
      finalized_number="$(block_number "${finalized_head}")"
      if (( finalized_number >= imported_block )); then
        finalized=1
        printf 'MINIJAM_NATIVE_WORK_EXECUTION_RECEIPT=%s\n' "${receipt}"
        printf 'MINIJAM_NATIVE_WORK_IMPORTED_BLOCK=%s\n' "${imported_block}"
        printf 'MINIJAM_NATIVE_WORK_FINALIZED_BLOCK=%s\n' "${finalized_number}"
        break
      fi
    fi
  else
    code="$(jq -r '.error.code // empty' "${TMP}/work-status-${sample}.json")"
    [[ "${code}" == "-32013" || -z "${code}" ]] || {
      echo 'native minijam_getWorkStatusV1 failed' >&2
      cat "${TMP}/work-status-${sample}.json" >&2
      exit 1
    }
  fi
  sleep 0.5
done

(( imported == 1 )) || { echo 'native Work did not reach Imported before the timeout' >&2; exit 1; }
(( finalized == 1 )) || { echo 'native Work was not observed at a finalized context' >&2; exit 1; }
[[ -n "${work_id}" ]] || { echo 'native Work response did not expose a work id' >&2; exit 1; }

if [[ -z "${assigned_worker}" ]]; then
  summary="$(rpc_call "${NODE_RPC}" minijam_getPendingWorkTaskSummaryV1 '[]' 2>/dev/null || true)"
  assigned_worker="$(jq -er --argjson id "${work_id}" --arg package_hash "${package_hash}" \
    '.result[]? | select(.workId == $id and .packageHash == $package_hash) | .candidateProducer' <<<"${summary}" | head -n1 || true)"
fi
[[ -n "${assigned_worker}" ]] || {
  echo 'native Work completed but no finalized pending-task observation identified its worker' >&2
  exit 1
}
(( candidate_seen == 1 )) || { echo 'native Work did not expose a candidate-bearing status' >&2; exit 1; }
(( vote_seen == 1 )) || { echo 'native Work did not expose an accepted vote decision' >&2; exit 1; }

printf 'MINIJAM_NATIVE_WORK_ID=%s\n' "${work_id}"
printf 'MINIJAM_NATIVE_WORK_CANDIDATE=PASS\n'
printf 'MINIJAM_NATIVE_WORK_VOTE=ACCEPTED\n'
printf 'MINIJAM_NATIVE_WORK_IMPORTED=PASS\n'
printf 'MINIJAM_NATIVE_WORK_FINALIZED=PASS\n'
printf 'MINIJAM_NATIVE_WORK_E2E=PASS\n'
