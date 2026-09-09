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
  "${compose[@]}" port node 9944 >&2 || true
  local node_container
  node_container="$("${compose[@]}" ps -q node 2>/dev/null || true)"
  if [[ -n "${node_container}" ]]; then
    docker inspect "${node_container}" \
      --format '{{json .NetworkSettings.Ports}}' >&2 || true
  fi
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

assert_node_host_port_published() {
  local mapping
  mapping="$("${compose[@]}" port node 9944 2>/dev/null || true)"
  if ! grep -Eq '(^|[[:space:]])127\.0\.0\.1:9944([[:space:]]|$)' <<<"${mapping}"; then
    echo 'Stage-1 node host RPC port was not published on 127.0.0.1:9944' >&2
    return 1
  fi
  printf 'NODE_HOST_PORT_PUBLISHED=PASS\n'
}

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

worker_success_count() {
  "${compose[@]}" logs --no-color worker 2>/dev/null \
    | grep -Fc 'minijam worker poll completed' || true
}

wait_for_worker_node_rpc() {
  local previous_successes="${1:-0}"
  local deadline=$((SECONDS + ${MINIJAM_STAGE1_READY_TIMEOUT_SECONDS:-180}))
  while :; do
    if ! assert_running worker; then
      echo 'Stage-1 worker exited before proving node RPC access' >&2
      return 1
    fi
    local logs
    logs="$("${compose[@]}" logs --no-color worker 2>/dev/null || true)"
    if grep -Eiq '403 Forbidden|HTTP/1\.1 403|HTTP request failed:.*403' <<<"${logs}"; then
      echo 'Stage-1 worker observed a persistent node RPC HTTP 403 rejection' >&2
      return 1
    fi
    local successes
    successes="$(worker_success_count)"
    if (( successes > previous_successes )); then
      printf 'WORKER_NODE_RPC=PASS\n'
      return 0
    fi
    (( SECONDS < deadline )) || {
      echo 'Stage-1 worker did not complete a successful node RPC poll' >&2
      return 1
    }
    sleep 2
  done
}

"${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
"${compose[@]}" up --detach --no-build --pull never
assert_node_host_port_published
wait_for_node
printf 'node started PASS\n'
wait_for_node_network_identity
printf 'node network identity available PASS\n'
printf 'NODE_NETWORK_IDENTITY=PASS\n'
wait_for_secret_readable node /run/secrets/node_network_key 'node secret'
wait_for_secret_readable worker /run/secrets/worker_signing_key 'worker secret'
wait_for_secret_readable formal-rpc /run/secrets/work_ingress_key 'formal-rpc secret'
wait_for_formal_rpc
assert_running node
assert_running worker
assert_running formal-rpc
printf 'worker running PASS\n'
printf 'WORKER_RUNNING=PASS\n'
printf 'formal-rpc readiness PASS\n'
printf 'FORMAL_RPC_READY=PASS\n'
worker_successes_before_cold_start="$(worker_success_count)"
wait_for_worker_node_rpc "${worker_successes_before_cold_start}"
printf 'node JSON-RPC PASS\n'
printf 'NODE_JSON_RPC=PASS\n'

"${compose[@]}" stop node
"${compose[@]}" up --detach --no-deps --force-recreate formal-rpc
sleep "${MINIJAM_STAGE1_COLD_START_GRACE_SECONDS:-2}"
assert_running formal-rpc
"${compose[@]}" start node
wait_for_node
wait_for_formal_rpc
wait_for_worker_node_rpc "${worker_successes_before_cold_start}"
assert_running node
assert_running worker
assert_running formal-rpc
printf 'FORMAL_RPC_COLD_START_RECOVERY=PASS\n'

worker_successes_before_restart="$(worker_success_count)"
"${compose[@]}" restart node
wait_for_node
restarted_peer_id="$(node_peer_id)"
test "${restarted_peer_id}" = "${NODE_PEER_ID}"
wait_for_formal_rpc
wait_for_worker_node_rpc "${worker_successes_before_restart}"
assert_running node
assert_running worker
assert_running formal-rpc
printf 'NODE_RESTART_IDENTITY=PASS\n'
printf 'node restart PASS\n'
printf 'POST_RESTART_RECOVERY=PASS\n'

printf 'STAGE1_DOCKER_SMOKE=PASS\n'
