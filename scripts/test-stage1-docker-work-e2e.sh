#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
COMPOSE_FILE="${MINIJAM_STAGE1_WORK_E2E_COMPOSE_FILE:-${ROOT}/deploy/stage1/compose.work-e2e.yml}"
PROJECT="${MINIJAM_STAGE1_WORK_E2E_PROJECT:-minijam-stage1-work-e2e}"
CHAIN_SPEC="${MINIJAM_STAGE1_WORK_E2E_CHAIN_SPEC_FILE:?set the stage1-work-e2e chain spec file}"
NODE_IMAGE="${MINIJAM_NODE_IMAGE:?set the exact MiniJAM node image reference}"
WORKER_IMAGE="${MINIJAM_WORKER_IMAGE:?set the exact MiniJAM worker image reference}"
FORMAL_RPC_IMAGE="${MINIJAM_FORMAL_RPC_IMAGE:?set the exact MiniJAM Formal RPC image reference}"
NODE_NETWORK_KEY="${MINIJAM_NODE_NETWORK_KEY:?set an ephemeral local node network key}"
RELAYER_URI="${MINIJAM_FORMAL_RPC_RELAYER_URI:?set an ephemeral local Work-ingress relayer URI}"
TIMEOUT="${MINIJAM_DOCKER_WORK_E2E_TIMEOUT_SECONDS:-240}"
ARTIFACT_DIR="${MINIJAM_DOCKER_WORK_E2E_ARTIFACT_DIR:-${ROOT}/target/.stage1-docker-work-e2e}"

for command in curl jq docker realpath; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done
test -s "${CHAIN_SPEC}" || { echo "chain spec is missing or empty: ${CHAIN_SPEC}" >&2; exit 1; }
test -f "${COMPOSE_FILE}" || { echo "missing Docker Work E2E compose file: ${COMPOSE_FILE}" >&2; exit 1; }
[[ "$(jq -er '.id | strings' "${CHAIN_SPEC}")" == "minijam_stage1_work_e2e" ]] || {
  echo 'Docker Work E2E requires chain id minijam_stage1_work_e2e' >&2
  exit 1
}
[[ "${NODE_NETWORK_KEY}" =~ ^(0x)?[0-9a-fA-F]{64}$ ]] || {
  echo 'MINIJAM_NODE_NETWORK_KEY must be a 32-byte hex value' >&2
  exit 1
}
[[ "${RELAYER_URI}" =~ ^0x[0-9a-fA-F]{64}$ ]] || {
  echo 'MINIJAM_FORMAL_RPC_RELAYER_URI must be a 0x-prefixed 32-byte value' >&2
  exit 1
}

docker info >/dev/null
CHAIN_SPEC="$(realpath "${CHAIN_SPEC}")"
compose=(docker compose --project-name "${PROJECT}" -f "${COMPOSE_FILE}")

export MINIJAM_NODE_IMAGE="${NODE_IMAGE}"
export MINIJAM_WORKER_IMAGE="${WORKER_IMAGE}"
export MINIJAM_FORMAL_RPC_IMAGE="${FORMAL_RPC_IMAGE}"
export MINIJAM_STAGE1_WORK_E2E_CHAIN_SPEC_FILE="${CHAIN_SPEC}"
export MINIJAM_NODE_NETWORK_KEY="${NODE_NETWORK_KEY}"
export MINIJAM_FORMAL_RPC_RELAYER_URI="${RELAYER_URI}"

mkdir -p "${ARTIFACT_DIR}"
preserve_artifacts() {
  "${compose[@]}" ps --all >"${ARTIFACT_DIR}/compose-ps.log" 2>&1 || true
  "${compose[@]}" logs --no-color >"${ARTIFACT_DIR}/compose.log" 2>&1 || true
}
cleanup() {
  local status=$?
  if (( status != 0 )); then
    preserve_artifacts
  fi
  if (( ${KEEP_STAGE1_DOCKER_WORK_E2E_STACK:-0} != 1 )); then
    "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
  fi
  exit "${status}"
}
trap cleanup EXIT

node_rpc() {
  local method="$1" params="${2:-[]}"
  curl -fsS --max-time 10 -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" \
    http://127.0.0.1:9944
}

wait_for_node() {
  local deadline=$((SECONDS + TIMEOUT)) response
  while (( SECONDS < deadline )); do
    response="$(node_rpc system_health 2>/dev/null || true)"
    if jq -e '.result != null and .error == null' <<<"${response}" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  echo 'Docker Work E2E node RPC did not become ready' >&2
  return 1
}

wait_for_http() {
  local url="$1" expected="$2" deadline=$((SECONDS + TIMEOUT)) response
  while (( SECONDS < deadline )); do
    response="$(curl -fsS --max-time 5 "${url}" 2>/dev/null || true)"
    if [[ "${response}" == *"${expected}"* ]]; then
      return 0
    fi
    sleep 2
  done
  echo "Docker Work E2E endpoint did not become ready: ${url}" >&2
  return 1
}

"${compose[@]}" config >/dev/null
"${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
"${compose[@]}" up --detach --no-build --pull never
wait_for_node
printf 'MINIJAM_DOCKER_WORK_NODE_RPC=PASS\n'
wait_for_http http://127.0.0.1:8080/health/ready ready
printf 'MINIJAM_DOCKER_WORK_FORMAL_RPC=PASS\n'
wait_for_http http://127.0.0.1:8082/health/ready ready
wait_for_http http://127.0.0.1:8083/health/ready ready
wait_for_http http://127.0.0.1:8084/health/ready ready
printf 'MINIJAM_DOCKER_WORK_WORKERS=PASS\n'

MINIJAM_NODE_RPC=http://127.0.0.1:9944 \
MINIJAM_FORMAL_RPC_URL=http://127.0.0.1:8080 \
MINIJAM_NATIVE_WORK_E2E_TIMEOUT_SECONDS="${TIMEOUT}" \
MINIJAM_NATIVE_WORK_E2E_ARTIFACT_DIR="${ARTIFACT_DIR}" \
  "${ROOT}/scripts/test-stage1-native-work-e2e.sh" \
  | tee "${ARTIFACT_DIR}/work-e2e.log"
grep -Fxq 'MINIJAM_NATIVE_WORK_IMPORTED=PASS' "${ARTIFACT_DIR}/work-e2e.log"
grep -Fxq 'MINIJAM_NATIVE_WORK_FINALIZED=PASS' "${ARTIFACT_DIR}/work-e2e.log"
printf 'MINIJAM_DOCKER_WORK_IMPORTED=PASS\n'
printf 'MINIJAM_DOCKER_WORK_FINALIZED=PASS\n'
printf 'MINIJAM_DOCKER_WORK_E2E=PASS\n'
