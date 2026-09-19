#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
IMAGE="${MINIJAM_IMAGE:?set the exact aggregate MiniJAM image reference}"
CONTAINER="${MINIJAM_LOCAL_CONTAINER:-minijam-local-e2e}"
TIMEOUT="${MINIJAM_LOCAL_READY_TIMEOUT_SECONDS:-180}"
ARTIFACT_DIR="${MINIJAM_LOCAL_ARTIFACT_DIR:-${ROOT}/target/.minijam-local-e2e}"

for command in curl jq docker; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done
docker info >/dev/null

mkdir -p "${ARTIFACT_DIR}"
cleanup() {
  local status=$?
  if (( status != 0 )); then
    docker logs "${CONTAINER}" >"${ARTIFACT_DIR}/container.log" 2>&1 || true
  fi
  if (( ${KEEP_MINIJAM_LOCAL_CONTAINER:-0} != 1 )); then
    docker rm --force "${CONTAINER}" >/dev/null 2>&1 || true
  fi
  exit "${status}"
}
trap cleanup EXIT

docker rm --force "${CONTAINER}" >/dev/null 2>&1 || true
docker run --detach --name "${CONTAINER}" \
  --publish 127.0.0.1:9944:9944 \
  --publish 127.0.0.1:8080:8080 \
  --publish 127.0.0.1:8082:8082 \
  "${IMAGE}" --dev >/dev/null

node_rpc() {
  local method="$1" params="${2:-[]}"
  curl -fsS --max-time 5 -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" \
    http://127.0.0.1:9944
}

wait_for_node() {
  local deadline=$((SECONDS + TIMEOUT))
  while (( SECONDS < deadline )); do
    if [[ "$(docker inspect --format '{{.State.Running}}' "${CONTAINER}" 2>/dev/null || true)" != true ]]; then
      echo 'aggregate MiniJAM container exited before node readiness' >&2
      return 1
    fi
    if node_rpc system_health | jq -e '.result != null and .error == null' >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  echo 'aggregate MiniJAM node did not become ready' >&2
  return 1
}

wait_for_endpoint() {
  local url="$1" expected="$2" deadline=$((SECONDS + TIMEOUT))
  while (( SECONDS < deadline )); do
    if curl -fsS --max-time 5 "${url}" | jq -e ".status == \"${expected}\"" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  echo "endpoint did not become ready: ${url}" >&2
  return 1
}

wait_for_worker_endpoint() {
  local url="$1" deadline=$((SECONDS + TIMEOUT))
  while (( SECONDS < deadline )); do
    if curl -fsS --max-time 5 "${url}" | grep -Fxq 'ready'; then
      return 0
    fi
    sleep 2
  done
  echo "worker endpoint did not become ready: ${url}" >&2
  return 1
}

block_number() {
  local hash="${1:-}" params='[]' value
  if [[ -n "${hash}" ]]; then
    params="$(jq -cn --arg hash "${hash}" '[ $hash ]')"
  fi
  value="$(node_rpc chain_getHeader "${params}" | jq -er '.result.number')"
  case "${value}" in
    0x*|0X*) printf '%d\n' "$((16#${value:2}))" ;;
    *) printf '%d\n' "${value}" ;;
  esac
}

finalized_block_number() {
  local hash
  hash="$(node_rpc chain_getFinalizedHead | jq -er '.result')"
  block_number "${hash}"
}

wait_for_block_progress() {
  local initial="$1" deadline=$((SECONDS + TIMEOUT)) current
  while (( SECONDS < deadline )); do
    current="$(block_number 2>/dev/null || true)"
    if [[ "${current}" =~ ^[0-9]+$ ]] && (( current > initial )); then
      return 0
    fi
    sleep 2
  done
  return 1
}

wait_for_finalized_progress() {
  local initial="$1" deadline=$((SECONDS + TIMEOUT)) current
  while (( SECONDS < deadline )); do
    current="$(finalized_block_number 2>/dev/null || true)"
    if [[ "${current}" =~ ^[0-9]+$ ]] && (( current > initial )); then
      return 0
    fi
    sleep 2
  done
  return 1
}

wait_for_node
printf 'MINIJAM_DEV_NODE_READY=PASS\n'
wait_for_endpoint http://127.0.0.1:8080/health/ready ready
printf 'MINIJAM_DEV_FORMAL_RPC_READY=PASS\n'
wait_for_worker_endpoint http://127.0.0.1:8082/health/ready
printf 'MINIJAM_DEV_WORKER_0_READY=PASS\n'
initial_block="$(block_number)"
initial_finalized="$(finalized_block_number)"
wait_for_block_progress "${initial_block}"
printf 'MINIJAM_DEV_BLOCK_PROGRESS=PASS\n'
wait_for_finalized_progress "${initial_finalized}"
printf 'MINIJAM_DEV_FINALITY=PASS\n'

MINIJAM_NODE_RPC=http://127.0.0.1:9944 \
MINIJAM_FORMAL_RPC_URL=http://127.0.0.1:8080 \
MINIJAM_WORK_E2E_TIMEOUT_SECONDS="${TIMEOUT}" \
  "${ROOT}/scripts/test-minijam-work-e2e.sh"

printf 'MINIJAM_CANONICAL_LOCAL_E2E=PASS\n'
