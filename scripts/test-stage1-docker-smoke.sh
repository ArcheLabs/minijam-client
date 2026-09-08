#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
COMPOSE_FILE="${MINIJAM_STAGE1_COMPOSE_FILE:-${ROOT}/deploy/stage1/compose.compact.yml}"
PROJECT="${MINIJAM_STAGE1_SMOKE_PROJECT:-minijam-stage1-smoke}"
CHAIN_SPEC="${MINIJAM_STAGE1_CHAIN_SPEC_FILE:?set the Stage-1 chain-spec file}"
NODE_IMAGE="${MINIJAM_NODE_IMAGE:?set the exact Stage-1 node image reference}"
WORKER_IMAGE="${MINIJAM_WORKER_IMAGE:?set the exact Stage-1 worker image reference}"
FORMAL_RPC_IMAGE="${MINIJAM_FORMAL_RPC_IMAGE:?set the exact Stage-1 Formal RPC image reference}"
NODE_NETWORK_KEY="${MINIJAM_NODE_NETWORK_KEY:?set the ephemeral node network key}"
WORKER_SEED="${MINIJAM_WORKER_SEED:?set the ephemeral worker seed}"
RELAYER_URI="${MINIJAM_FORMAL_RPC_RELAYER_URI:?set the ephemeral Work-ingress relayer URI}"

test -f "${CHAIN_SPEC}" || { echo "missing smoke input: ${CHAIN_SPEC}" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || { echo 'curl is required for Stage-1 smoke' >&2; exit 127; }
command -v jq >/dev/null 2>&1 || { echo 'jq is required for Stage-1 smoke' >&2; exit 127; }
docker info >/dev/null

CHAIN_SPEC="$(realpath "${CHAIN_SPEC}")"
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
export MINIJAM_NODE_NETWORK_KEY="${NODE_NETWORK_KEY}"
export MINIJAM_WORKER_SEED="${WORKER_SEED}"
export MINIJAM_FORMAL_RPC_RELAYER_URI="${RELAYER_URI}"

wait_for_node() {
  local deadline=$((SECONDS + ${MINIJAM_STAGE1_READY_TIMEOUT_SECONDS:-180}))
  until curl -fsS --max-time 3 \
      -H 'content-type: application/json' \
      --data '{"id":1,"jsonrpc":"2.0","method":"system_health","params":[]}' \
      http://127.0.0.1:9944 | jq -e '.result != null' >/dev/null; do
    (( SECONDS < deadline )) || { echo 'Stage-1 node JSON-RPC did not become functional' >&2; return 1; }
    sleep 2
  done
}

node_peer_id() {
  curl -fsS --max-time 3 \
    -H 'content-type: application/json' \
    --data '{"id":1,"jsonrpc":"2.0","method":"system_localPeerId","params":[]}' \
    http://127.0.0.1:9944 | jq -er '.result | strings | select(length > 0)'
}

wait_for_node_network_identity() {
  local deadline=$((SECONDS + ${MINIJAM_STAGE1_READY_TIMEOUT_SECONDS:-180}))
  until NODE_PEER_ID="$(node_peer_id)"; do
    (( SECONDS < deadline )) || { echo 'Stage-1 node network identity did not become available' >&2; return 1; }
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

wait_for_secret_readable() {
  local service="$1"
  local path="$2"
  local label="$3"
  local deadline=$((SECONDS + ${MINIJAM_STAGE1_READY_TIMEOUT_SECONDS:-180}))
  until "${compose[@]}" exec -T "${service}" sh -c 'test -r "$1"' sh "${path}" >/dev/null 2>&1; do
    (( SECONDS < deadline )) || { echo "${label} is not readable by the container user" >&2; return 1; }
    sleep 2
  done
  printf '%s PASS\n' "${label} readable"
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
printf 'node started PASS\n'
wait_for_node_network_identity
printf 'node network identity available PASS\n'
wait_for_secret_readable node /run/secrets/node_network_key 'node secret'
wait_for_secret_readable worker /run/secrets/worker_signing_key 'worker secret'
wait_for_secret_readable formal-rpc /run/secrets/work_ingress_key 'formal-rpc secret'
wait_for_formal_rpc
assert_running node
assert_running worker
assert_running formal-rpc
printf 'worker running PASS\n'
printf 'formal-rpc readiness PASS\n'
"${compose[@]}" logs worker | grep -Fq 'rpc=http://node:9944'
printf 'node JSON-RPC PASS\n'

"${compose[@]}" restart node
wait_for_node
restarted_peer_id="$(node_peer_id)"
test "${restarted_peer_id}" = "${NODE_PEER_ID}"
wait_for_formal_rpc
assert_running node
assert_running worker
assert_running formal-rpc
"${compose[@]}" logs worker | grep -Fq 'rpc=http://node:9944'
printf 'node restart PASS\n'
printf 'post-restart recovery PASS\n'

printf 'STAGE1_DOCKER_SMOKE=PASS\n'
