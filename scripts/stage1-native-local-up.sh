#!/usr/bin/env bash
set -euo pipefail

# Start the local direct-refine stack: one node, one Formal RPC, one Worker.
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
RUNTIME="${MINIJAM_NATIVE_LOCAL_RUNTIME:-${ROOT}/target/stage1-native-local}"
CHAIN_SPEC="${RUNTIME}/stage1-direct-e2e.json"
NODE_DATA="${RUNTIME}/node-data"
BUNDLES="${RUNTIME}/bundles"
LOGS="${RUNTIME}/logs"
NODE_LOG="${LOGS}/node.log"
FORMAL_LOG="${LOGS}/formal-rpc.log"
WORKER_LOG="${LOGS}/worker.log"
NODE_PID_FILE="${RUNTIME}/node.pid"
FORMAL_PID_FILE="${RUNTIME}/formal-rpc.pid"
WORKER_PID_FILE="${RUNTIME}/worker.pid"
CONNECTION_ENV="${RUNTIME}/connection.env"
PRIVATE_ENV="${RUNTIME}/private.env"
NODE_BIN="${MINIJAM_NATIVE_NODE_BIN:-${ROOT}/target/debug/minijam-node}"
FORMAL_BIN="${MINIJAM_NATIVE_FORMAL_RPC_BIN:-${ROOT}/target/debug/minijam-formal-rpc}"
WORKER_BIN="${MINIJAM_NATIVE_WORKER_BIN:-${ROOT}/target/debug/minijam-worker}"
NODE_PORT="${MINIJAM_NATIVE_NODE_RPC_PORT:-9944}"
FORMAL_PORT="${MINIJAM_NATIVE_FORMAL_RPC_PORT:-8090}"
NODE_P2P_PORT="${MINIJAM_NATIVE_NODE_P2P_PORT:-30333}"
READY_TIMEOUT="${MINIJAM_NATIVE_LOCAL_READY_TIMEOUT_SECONDS:-180}"
WORKER_KEY="${MINIJAM_NATIVE_WORKER_KEY:-//Alice}"
NODE_RPC="http://127.0.0.1:${NODE_PORT}"
FORMAL_URL="http://127.0.0.1:${FORMAL_PORT}"

for command in cargo curl jq openssl setsid; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done

pid_from() { [[ -s "$1" ]] || return 1; local pid; pid="$(<"$1")"; [[ "${pid}" =~ ^[1-9][0-9]*$ ]] || return 1; printf '%s\n' "${pid}"; }
alive() { local pid; pid="$(pid_from "$1")" || return 1; kill -0 "${pid}" 2>/dev/null; }
stop_pid_file() { local pid; pid="$(pid_from "$1")" || return 0; kill "${pid}" 2>/dev/null || true; wait "${pid}" 2>/dev/null || true; rm -f -- "$1"; }
rpc_call() { local method="$1" params="${2:-[]}"; curl -fsS --max-time 5 -H 'content-type: application/json' --data "$(jq -cn --arg method "${method}" --argjson params "${params}" '{id:1,jsonrpc:"2.0",method:$method,params:$params}')" "${NODE_RPC}"; }
wait_for_node() { local deadline=$((SECONDS + READY_TIMEOUT)); while (( SECONDS < deadline )); do alive "${NODE_PID_FILE}" && rpc_call system_health >/dev/null 2>&1 && return 0; sleep 2; done; tail -n 100 "${NODE_LOG}" >&2 || true; return 1; }
wait_for_formal() { local deadline=$((SECONDS + READY_TIMEOUT)); while (( SECONDS < deadline )); do alive "${FORMAL_PID_FILE}" && curl -fsS --max-time 5 "${FORMAL_URL}/health/ready" | jq -e '.status == "ready"' >/dev/null 2>&1 && return 0; sleep 2; done; tail -n 100 "${FORMAL_LOG}" >&2 || true; return 1; }
cleanup_on_error() { local status=$?; if (( status != 0 )); then stop_pid_file "${WORKER_PID_FILE}"; stop_pid_file "${FORMAL_PID_FILE}"; stop_pid_file "${NODE_PID_FILE}"; fi; exit "${status}"; }
trap cleanup_on_error EXIT

mkdir -p "${RUNTIME}" "${NODE_DATA}" "${BUNDLES}" "${LOGS}"
if alive "${NODE_PID_FILE}" && alive "${FORMAL_PID_FILE}" && alive "${WORKER_PID_FILE}"; then
  printf 'MINIJAM_LOCAL_NETWORK=ALREADY_RUNNING\nMINIJAM_NODE_RPC=%s\nMINIJAM_FORMAL_RPC_URL=%s\n' "${NODE_RPC}" "${FORMAL_URL}"
  exit 0
fi
stop_pid_file "${WORKER_PID_FILE}"
stop_pid_file "${FORMAL_PID_FILE}"
stop_pid_file "${NODE_PID_FILE}"

if [[ "${MINIJAM_SKIP_BUILD:-0}" == 1 ]]; then
  printf 'LOCAL_INCREMENTAL_BUILD=SKIPPED\n'
else
  cargo build --locked --manifest-path "${ROOT}/Cargo.toml" \
    -p minijam-node -p minijam-formal-rpc -p minijam-worker
  printf 'LOCAL_INCREMENTAL_BUILD=PASS\n'
fi
for binary in "${NODE_BIN}" "${FORMAL_BIN}" "${WORKER_BIN}"; do
  test -x "${binary}" || { echo "binary is not executable: ${binary}" >&2; exit 1; }
done

if [[ ! -s "${CHAIN_SPEC}" ]]; then
  worker_info="$("${NODE_BIN}" key inspect "${WORKER_KEY}")"
  worker_public_key="$(sed -nE 's/^[[:space:]]*Public key \(hex\):[[:space:]]*(0x[0-9a-fA-F]{64})[[:space:]]*$/\1/p' <<<"${worker_info}" | head -n1)"
  [[ "${worker_public_key}" =~ ^0x[0-9a-fA-F]{64}$ ]] || { echo 'unable to derive Worker AccountId32' >&2; exit 1; }
  MINIJAM_STAGE1_WORKER_PUBLIC_KEY="${worker_public_key}" MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY="${worker_public_key}" "${NODE_BIN}" build-spec --chain stage1-direct-e2e >"${CHAIN_SPEC}"
fi
[[ "$(jq -er '.id | strings' "${CHAIN_SPEC}")" == minijam_stage1_direct_e2e ]] || { echo "${CHAIN_SPEC} is not a direct E2E chain spec" >&2; exit 1; }

if [[ ! -s "${PRIVATE_ENV}" ]]; then
  node_network_key="$("${NODE_BIN}" key generate-node-key 2>/dev/null | tr -d '\r\n')"
  umask 077
  {
    printf 'MINIJAM_WORKER_KEY=%s\n' "${WORKER_KEY}"
    printf 'MINIJAM_NODE_NETWORK_KEY=%s\n' "${node_network_key}"
  } >"${PRIVATE_ENV}"
  chmod 600 "${PRIVATE_ENV}"
else
  node_network_key="$(sed -nE 's/^MINIJAM_NODE_NETWORK_KEY=([^[:space:]]+)$/\1/p' "${PRIVATE_ENV}" | head -n1)"
fi
network_dir="${NODE_DATA}/chains/minijam_stage1_direct_e2e/network"
mkdir -p "${network_dir}"
printf '%s' "${node_network_key#0x}" >"${network_dir}/secret_ed25519"
chmod 600 "${network_dir}/secret_ed25519"

setsid "${NODE_BIN}" --chain "${CHAIN_SPEC}" --base-path "${NODE_DATA}" --node-key-file "${network_dir}/secret_ed25519" --alice --force-authoring --unsafe-rpc-external --rpc-methods=safe --rpc-cors=all --port "${NODE_P2P_PORT}" --rpc-port "${NODE_PORT}" >>"${NODE_LOG}" 2>&1 &
printf '%s\n' "$!" >"${NODE_PID_FILE}"
wait_for_node
printf 'MINIJAM_LOCAL_NODE_RPC=PASS\n'

MINIJAM_RPC_URL="ws://127.0.0.1:${NODE_PORT}" MINIJAM_FORMAL_RPC_BIND="127.0.0.1:${FORMAL_PORT}" MINIJAM_SIGNER_URI="${WORKER_KEY}" MINIJAM_BUNDLE_DIR="${BUNDLES}" setsid "${FORMAL_BIN}" >>"${FORMAL_LOG}" 2>&1 &
printf '%s\n' "$!" >"${FORMAL_PID_FILE}"
wait_for_formal
printf 'MINIJAM_LOCAL_FORMAL_RPC_READY=PASS\n'

setsid "${WORKER_BIN}" --rpc-url="ws://127.0.0.1:${NODE_PORT}" --formal-rpc-url="${FORMAL_URL}" --key="${WORKER_KEY}" --recovery-db="${RUNTIME}/worker-recovery.json" >>"${WORKER_LOG}" 2>&1 &
printf '%s\n' "$!" >"${WORKER_PID_FILE}"
sleep 2
alive "${WORKER_PID_FILE}"
printf 'MINIJAM_LOCAL_WORKER_READY=PASS\n'

umask 022
{
  printf 'MINIJAM_NODE_RPC=%s\n' "${NODE_RPC}"
  printf 'MINIJAM_FORMAL_RPC_URL=%s\n' "${FORMAL_URL}"
  printf 'MINIJAM_CHAIN_SPEC=%s\n' "${CHAIN_SPEC}"
  printf 'MINIJAM_CHAIN_ID=minijam_stage1_direct_e2e\n'
  printf 'MINIJAM_WORKER_COUNT=1\n'
} >"${CONNECTION_ENV}"
printf 'MINIJAM_LOCAL_NETWORK=READY\nMINIJAM_NODE_RPC=%s\nMINIJAM_FORMAL_RPC_URL=%s\n' "${NODE_RPC}" "${FORMAL_URL}"
trap - EXIT
