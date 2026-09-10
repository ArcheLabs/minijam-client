#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
COMPOSE_FILE="${MINIJAM_STAGE1_COMPOSE_FILE:-${ROOT}/deploy/stage1/compose.compact.yml}"
PROJECT="${MINIJAM_STAGE1_CREATE_SERVICE_PROJECT:-minijam-stage1-create-service}"
CHAIN_SPEC="${MINIJAM_STAGE1_CHAIN_SPEC_FILE:?set the Stage-1 chain-spec file}"
NODE_IMAGE="${MINIJAM_NODE_IMAGE:?set the exact Stage-1 node image reference}"
WORKER_IMAGE="${MINIJAM_WORKER_IMAGE:?set the exact Stage-1 worker image reference}"
FORMAL_RPC_IMAGE="${MINIJAM_FORMAL_RPC_IMAGE:?set the exact Stage-1 Formal RPC image reference}"
NODE_NETWORK_KEY="${MINIJAM_NODE_NETWORK_KEY:?set the ephemeral node network key}"
WORKER_SEED="${MINIJAM_WORKER_SEED:?set the ephemeral worker seed}"
RELAYER_URI="${MINIJAM_FORMAL_RPC_RELAYER_URI:?set the ephemeral Work-ingress relayer URI}"
SERVICE_BLOB="${MINIJAM_NATIVE_SERVICE_BLOB:-${ROOT}/examples/services/counter/artifacts/counter-c.blob}"
SERVICE_CODE_HASH="${MINIJAM_NATIVE_SERVICE_CODE_HASH:?set the BLAKE2-256 hash of the service blob}"
TIMEOUT="${MINIJAM_DOCKER_CREATE_SERVICE_TIMEOUT_SECONDS:-180}"
ARTIFACT_DIR="${MINIJAM_DOCKER_ARTIFACT_DIR:-}"

command -v curl >/dev/null 2>&1 || { echo 'curl is required for Docker CreateService E2E' >&2; exit 127; }
command -v jq >/dev/null 2>&1 || { echo 'jq is required for Docker CreateService E2E' >&2; exit 127; }
command -v base64 >/dev/null 2>&1 || { echo 'base64 is required for Docker CreateService E2E' >&2; exit 127; }
command -v docker >/dev/null 2>&1 || { echo 'docker is required for Docker CreateService E2E' >&2; exit 127; }
test -f "${CHAIN_SPEC}" || { echo "missing chain spec: ${CHAIN_SPEC}" >&2; exit 1; }
test -s "${SERVICE_BLOB}" || { echo "service blob is missing or empty: ${SERVICE_BLOB}" >&2; exit 1; }
[[ "${SERVICE_CODE_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] || {
  echo 'MINIJAM_NATIVE_SERVICE_CODE_HASH must be a 0x-prefixed 32-byte hex value' >&2
  exit 1
}

compose=(docker compose --project-name "${PROJECT}" -f "${COMPOSE_FILE}")
response_file="${ARTIFACT_DIR:+${ARTIFACT_DIR}/}create-service-response.json"
compose_log_file="${ARTIFACT_DIR:+${ARTIFACT_DIR}/}compose.log"

preserve_artifacts() {
  [[ -n "${ARTIFACT_DIR}" ]] || return 0
  mkdir -p "${ARTIFACT_DIR}"
  "${compose[@]}" logs --no-color >"${compose_log_file}" 2>&1 || true
  [[ -f "${response_file}" ]] || true
}

cleanup() {
  local status=$?
  if (( status != 0 )); then
    preserve_artifacts
  fi
  "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
  exit "${status}"
}
trap cleanup EXIT

export MINIJAM_NODE_IMAGE="${NODE_IMAGE}"
export MINIJAM_WORKER_IMAGE="${WORKER_IMAGE}"
export MINIJAM_FORMAL_RPC_IMAGE="${FORMAL_RPC_IMAGE}"
export MINIJAM_STAGE1_CHAIN_SPEC_FILE="$(realpath "${CHAIN_SPEC}")"
export MINIJAM_NODE_NETWORK_KEY="${NODE_NETWORK_KEY}"
export MINIJAM_WORKER_SEED="${WORKER_SEED}"
export MINIJAM_FORMAL_RPC_RELAYER_URI="${RELAYER_URI}"

node_rpc() {
  local method="$1"
  local params="${2:-[]}"
  curl -fsS --max-time 5 \
    -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" \
    http://127.0.0.1:9944
}

wait_for_node() {
  local deadline=$((SECONDS + ${MINIJAM_DOCKER_READY_TIMEOUT_SECONDS:-180}))
  until health="$(node_rpc system_health 2>/dev/null)" \
    && jq -e '.result != null and .error == null' <<<"${health}" >/dev/null; do
    (( SECONDS < deadline )) || { echo 'Docker node RPC did not become ready' >&2; return 1; }
    sleep 2
  done
}

wait_for_formal_rpc() {
  local deadline=$((SECONDS + ${MINIJAM_DOCKER_READY_TIMEOUT_SECONDS:-180}))
  until curl -fsS --max-time 5 http://127.0.0.1:8080/health/ready \
    | jq -e '.status == "ready"' >/dev/null; do
    (( SECONDS < deadline )) || { echo 'Docker Formal RPC did not become ready' >&2; return 1; }
    sleep 2
  done
}

"${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
"${compose[@]}" up --detach --no-build --pull never
wait_for_node
printf 'DOCKER_NODE_RPC=PASS\n'
wait_for_formal_rpc
printf 'DOCKER_FORMAL_RPC_READY=PASS\n'

blob_base64="$(base64 <"${SERVICE_BLOB}" | tr -d '\r\n')"
request="$(jq -cn \
  --arg codeHash "${SERVICE_CODE_HASH}" \
  --arg blobBase64 "${blob_base64}" \
  '{id: 1, jsonrpc: "2.0", method: "minijam_createServiceV1", params: {
    codeHash: $codeHash,
    blobBase64: $blobBase64,
    minItemGas: 1,
    minMemoGas: 1
  }}')"

mkdir -p "${ARTIFACT_DIR:-${ROOT}/target/.stage1-docker-create-service}"
if [[ -z "${ARTIFACT_DIR}" ]]; then
  response_file="${ROOT}/target/.stage1-docker-create-service/create-service-response.json"
  compose_log_file="${ROOT}/target/.stage1-docker-create-service/compose.log"
fi
curl -fsS --max-time "${TIMEOUT}" \
  -H 'content-type: application/json' \
  --data "${request}" \
  http://127.0.0.1:8080/ >"${response_file}"

jq -e --arg expected "${SERVICE_CODE_HASH}" '
  .error == null
  and .result.finalized == true
  and (.result.serviceId | type == "number")
  and .result.codeHash == $expected
  and (.result.context.blockHash | strings | test("^0x[0-9a-fA-F]{64}$"))
' "${response_file}" >/dev/null || {
  echo 'Docker minijam_createServiceV1 returned an invalid or unsuccessful result' >&2
  cat "${response_file}" >&2
  exit 1
}
printf 'DOCKER_CREATE_SERVICE=PASS\n'

logs="$("${compose[@]}" logs --no-color formal-rpc 2>/dev/null || true)"
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
  grep -Fq "minijam_createServiceV1 phase=${phase}" <<<"${logs}" || {
    echo "missing Docker CreateService phase: ${phase}" >&2
    exit 1
  }
done
printf 'DOCKER_CREATE_SERVICE_PHASES=PASS\n'
printf 'MINIJAM_DOCKER_CREATE_SERVICE_E2E=PASS\n'
