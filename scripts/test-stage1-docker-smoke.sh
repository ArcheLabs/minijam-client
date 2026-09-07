#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
COMPOSE_FILE="${MINIJAM_STAGE1_COMPOSE_FILE:-${ROOT}/deploy/stage1/compose.compact.yml}"
PROJECT="${MINIJAM_STAGE1_SMOKE_PROJECT:-minijam-stage1-smoke}"
CHAIN_SPEC="${MINIJAM_STAGE1_CHAIN_SPEC_FILE:?set the Stage-1 chain-spec file}"
NODE_IMAGE="${MINIJAM_NODE_IMAGE:?set the exact Stage-1 node image reference}"
WORKER_IMAGE="${MINIJAM_WORKER_IMAGE:?set the exact Stage-1 worker image reference}"
FORMAL_RPC_IMAGE="${MINIJAM_FORMAL_RPC_IMAGE:?set the exact Stage-1 Formal RPC image reference}"
WORKER_KEY_FILE="${MINIJAM_WORKER_KEY_FILE:?set an ephemeral worker signing-key file}"
RELAYER_KEY_FILE="${MINIJAM_FORMAL_RPC_RELAYER_KEY_FILE:?set an ephemeral Work-ingress signing-key file}"

for file in "${CHAIN_SPEC}" "${WORKER_KEY_FILE}" "${RELAYER_KEY_FILE}"; do
  test -f "${file}" || { echo "missing smoke input: ${file}" >&2; exit 1; }
done
command -v curl >/dev/null 2>&1 || { echo 'curl is required for Stage-1 smoke' >&2; exit 127; }
docker info >/dev/null

CHAIN_SPEC="$(realpath "${CHAIN_SPEC}")"
WORKER_KEY_FILE="$(realpath "${WORKER_KEY_FILE}")"
RELAYER_KEY_FILE="$(realpath "${RELAYER_KEY_FILE}")"
compose=(docker compose --project-name "${PROJECT}" -f "${COMPOSE_FILE}")

cleanup() {
  if (( ${KEEP_STAGE1_SMOKE_STACK:-0} != 1 )); then
    "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
  fi
}
failure_diagnostics() {
  "${compose[@]}" ps --all >&2 || true
  "${compose[@]}" logs --no-color >&2 || true
}
trap cleanup EXIT
trap failure_diagnostics ERR

export MINIJAM_NODE_IMAGE="${NODE_IMAGE}"
export MINIJAM_WORKER_IMAGE="${WORKER_IMAGE}"
export MINIJAM_FORMAL_RPC_IMAGE="${FORMAL_RPC_IMAGE}"
export MINIJAM_STAGE1_CHAIN_SPEC_FILE="${CHAIN_SPEC}"
export MINIJAM_WORKER_KEY_FILE="${WORKER_KEY_FILE}"
export MINIJAM_FORMAL_RPC_RELAYER_KEY_FILE="${RELAYER_KEY_FILE}"

wait_for_node() {
  local deadline=$((SECONDS + ${MINIJAM_STAGE1_READY_TIMEOUT_SECONDS:-180}))
  until curl -fsS --max-time 3 \
      -H 'content-type: application/json' \
      --data '{"id":1,"jsonrpc":"2.0","method":"system_health","params":[]}' \
      http://127.0.0.1:9944 | grep -q '"result"'; do
    (( SECONDS < deadline )) || { echo 'Stage-1 node JSON-RPC did not become functional' >&2; return 1; }
    sleep 2
  done
}

wait_for_formal_rpc() {
  local deadline=$((SECONDS + ${MINIJAM_STAGE1_READY_TIMEOUT_SECONDS:-180}))
  until curl -fsS --max-time 3 http://127.0.0.1:8080/health/ready >/dev/null; do
    (( SECONDS < deadline )) || { echo 'Stage-1 Formal RPC did not become ready' >&2; return 1; }
    sleep 2
  done
}

assert_running() {
  local service="$1"
  local container
  container="$("${compose[@]}" ps -q "${service}")"
  test -n "${container}"
  test "$(docker inspect --format '{{.State.Running}}' "${container}")" = true
}

"${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
"${compose[@]}" up --detach --no-build --pull never
wait_for_node
wait_for_formal_rpc
assert_running node
assert_running worker
assert_running formal-rpc
"${compose[@]}" logs worker | grep -Fq 'rpc=http://node:9944'

"${compose[@]}" restart node
wait_for_node
wait_for_formal_rpc
assert_running node
assert_running worker
assert_running formal-rpc
"${compose[@]}" logs worker | grep -Fq 'rpc=http://node:9944'

printf 'STAGE1_DOCKER_SMOKE=PASS\n'
