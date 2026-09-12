#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
RUNTIME="${MINIJAM_NATIVE_LOCAL_RUNTIME:-${ROOT}/target/stage1-native-local}"
CHAIN_SPEC="${RUNTIME}/chain-spec.json"
NODE_DATA="${RUNTIME}/node-data"
BUNDLES="${RUNTIME}/bundles"
WORKERS="${RUNTIME}/workers"
LOGS="${RUNTIME}/logs"
NODE_LOG="${LOGS}/node.log"
FORMAL_LOG="${LOGS}/formal-rpc.log"
NODE_PID_FILE="${RUNTIME}/node.pid"
FORMAL_PID_FILE="${RUNTIME}/formal-rpc.pid"
CONNECTION_ENV="${RUNTIME}/connection.env"
PRIVATE_ENV="${RUNTIME}/private.env"
NODE_BIN="${MINIJAM_NATIVE_NODE_BIN:-${ROOT}/target/debug/minijam-node}"
FORMAL_BIN="${MINIJAM_NATIVE_FORMAL_RPC_BIN:-${ROOT}/target/debug/minijam-formal-rpc}"
WORKER_BIN="${MINIJAM_NATIVE_WORKER_BIN:-${ROOT}/target/debug/minijam-worker}"
NODE_PORT="${MINIJAM_NATIVE_NODE_RPC_PORT:-9944}"
NODE_P2P_PORT="${MINIJAM_NATIVE_NODE_P2P_PORT:-30333}"
NODE_PROMETHEUS_PORT="${MINIJAM_NATIVE_NODE_PROMETHEUS_PORT:-9615}"
FORMAL_PORT="${MINIJAM_NATIVE_FORMAL_RPC_PORT:-8090}"
NODE_RPC="${MINIJAM_NODE_RPC:-http://127.0.0.1:${NODE_PORT}}"
FORMAL_URL="${MINIJAM_FORMAL_RPC_URL:-http://127.0.0.1:${FORMAL_PORT}}"
WORKER_HEALTH_BASE_PORT="${MINIJAM_NATIVE_WORKER_HEALTH_BASE_PORT:-8082}"
READY_TIMEOUT="${MINIJAM_NATIVE_LOCAL_READY_TIMEOUT_SECONDS:-180}"
WORKER_IDS=(0 1 2)
declare -A WORKER_KEYS=([0]=//Alice [1]=//Bob [2]=//Charlie)
declare -A WORKER_LOG_OFFSETS

for command in cargo curl jq openssl setsid; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done

pid_value() {
  local file="$1" pid
  [[ -s "${file}" ]] || return 1
  pid="$(<"${file}")"
  [[ "${pid}" =~ ^[1-9][0-9]*$ ]] || return 1
  printf '%s\n' "${pid}"
}

process_alive() {
  local file="$1" pid
  pid="$(pid_value "${file}")" || return 1
  kill -0 "${pid}" 2>/dev/null
}

worker_health_url() {
  local worker_id="$1"
  printf 'http://127.0.0.1:%s/health/ready\n' "$((WORKER_HEALTH_BASE_PORT + worker_id))"
}

rpc_call() {
  local method="$1" params="${2:-[]}" endpoint="${3:-${NODE_RPC}}"
  curl -fsS --max-time 5 -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" \
    "${endpoint}"
}

block_number() {
  local hash="${1:-}" params='[]' value
  [[ -z "${hash}" ]] || params="[\"${hash}\"]"
  value="$(rpc_call chain_getHeader "${params}" | jq -er '.result.number')"
  case "${value}" in
    0x*) printf '%d\n' "$((16#${value#0x}))" ;;
    0X*) printf '%d\n' "$((16#${value#0X}))" ;;
    *) printf '%d\n' "${value}" ;;
  esac
}

node_rpc_ready() {
  local health finalized
  health="$(rpc_call system_health 2>/dev/null)" \
    && jq -e '.result != null and .error == null' <<<"${health}" >/dev/null \
    && finalized="$(rpc_call chain_getFinalizedHead 2>/dev/null)" \
    && jq -e '.result | strings | test("^0x[0-9a-fA-F]{64}$")' <<<"${finalized}" >/dev/null
}

formal_ready() {
  local response
  response="$(curl -fsS --max-time 5 "${FORMAL_URL}/health/ready" 2>/dev/null)" \
    && jq -e '.status == "ready"' <<<"${response}" >/dev/null
}

worker_ready() {
  local worker_id="$1" response log_file
  log_file="${LOGS}/worker-${worker_id}.log"
  process_alive "${RUNTIME}/worker-${worker_id}.pid" || return 1
  response="$(curl -fsS --max-time 5 "$(worker_health_url "${worker_id}")" 2>/dev/null)" || return 1
  [[ "${response}" == *ready* ]] || return 1
  grep -q 'minijam worker poll completed' "${log_file}" 2>/dev/null
}

all_stack_ready() {
  node_rpc_ready || return 1
  formal_ready || return 1
  for worker_id in "${WORKER_IDS[@]}"; do
    worker_ready "${worker_id}" || return 1
  done
}

wait_for_node() {
  local deadline=$((SECONDS + READY_TIMEOUT))
  while (( SECONDS < deadline )); do
    if ! process_alive "${NODE_PID_FILE}"; then
      echo 'native MiniJAM node exited before becoming ready' >&2
      return 1
    fi
    if node_rpc_ready; then
      return 0
    fi
    sleep 2
  done
  echo 'native MiniJAM node RPC did not become ready' >&2
  tail -n 100 "${NODE_LOG}" >&2 || true
  return 1
}

wait_for_progress() {
  local initial_best="$1" initial_finalized="$2" deadline=$((SECONDS + READY_TIMEOUT))
  local current_best current_head current_finalized best_pass=0
  while (( SECONDS < deadline )); do
    current_best="$(block_number 2>/dev/null || true)"
    current_head="$(rpc_call chain_getFinalizedHead 2>/dev/null | jq -er '.result | strings' 2>/dev/null || true)"
    if [[ "${current_best}" =~ ^[0-9]+$ ]] && (( current_best > initial_best )) && (( best_pass == 0 )); then
      best_pass=1
      printf 'MINIJAM_LOCAL_BEST_BLOCK_ADVANCES=PASS\n'
    elif (( best_pass == 0 )); then
      sleep 2
      continue
    fi
    if [[ -n "${current_head}" ]]; then
      current_finalized="$(block_number "${current_head}" 2>/dev/null || true)"
      if [[ "${current_finalized}" =~ ^[0-9]+$ ]] && (( current_finalized > initial_finalized )); then
        printf 'MINIJAM_LOCAL_FINALITY=PASS\n'
        return 0
      fi
    fi
    sleep 2
  done
  echo "best/finalized block did not advance beyond ${initial_best}/${initial_finalized}" >&2
  return 1
}

wait_for_formal() {
  local deadline=$((SECONDS + READY_TIMEOUT))
  while (( SECONDS < deadline )); do
    if ! process_alive "${FORMAL_PID_FILE}"; then
      echo 'native Formal RPC exited before becoming ready' >&2
      return 1
    fi
    if formal_ready; then
      printf 'MINIJAM_LOCAL_FORMAL_RPC_READY=PASS\n'
      return 0
    fi
    sleep 2
  done
  echo 'native Formal RPC did not become ready' >&2
  tail -n 100 "${FORMAL_LOG}" >&2 || true
  return 1
}

wait_for_worker() {
  local worker_id="$1" log_offset="$2" deadline=$((SECONDS + READY_TIMEOUT))
  local log_file="${LOGS}/worker-${worker_id}.log" response
  while (( SECONDS < deadline )); do
    if ! process_alive "${RUNTIME}/worker-${worker_id}.pid"; then
      echo "native worker ${worker_id} exited before becoming ready" >&2
      tail -n 100 "${log_file}" >&2 || true
      return 1
    fi
    response="$(curl -fsS --max-time 5 "$(worker_health_url "${worker_id}")" 2>/dev/null || true)"
    if [[ "${response}" == *ready* ]] \
      && [[ -s "${log_file}" ]] \
      && tail -c +$((log_offset + 1)) "${log_file}" 2>/dev/null \
        | grep -q 'minijam worker poll completed'; then
      printf 'MINIJAM_LOCAL_WORKER_%s_READY=PASS\n' "${worker_id}"
      return 0
    fi
    sleep 2
  done
  echo "native worker ${worker_id} did not become ready" >&2
  tail -n 100 "${log_file}" >&2 || true
  return 1
}

read_private_value() {
  local name="$1"
  sed -nE "s/^${name}=([^[:space:]]+)$/\1/p" "${PRIVATE_ENV}" | head -n1
}

cleanup_on_error() {
  local status=$?
  if (( status != 0 )); then
    MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" \
      "${ROOT}/scripts/stage1-native-local-down.sh" >/dev/null 2>&1 || true
  fi
  exit "${status}"
}

mkdir -p "${RUNTIME}" "${NODE_DATA}" "${BUNDLES}" "${WORKERS}" "${LOGS}"

if all_stack_ready; then
  printf 'MINIJAM_LOCAL_NETWORK=ALREADY_RUNNING\n'
  printf 'MINIJAM_NODE_RPC=%s\n' "${NODE_RPC}"
  printf 'MINIJAM_FORMAL_RPC_URL=%s\n' "${FORMAL_URL}"
  exit 0
fi

if [[ -e "${NODE_PID_FILE}" || -e "${FORMAL_PID_FILE}" ]]; then
  MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${ROOT}/scripts/stage1-native-local-down.sh"
fi

if [[ "${MINIJAM_SKIP_BUILD:-0}" == "1" ]]; then
  printf 'LOCAL_INCREMENTAL_BUILD=SKIPPED\n'
else
  cargo build --locked -p minijam-node -p minijam-formal-rpc -p minijam-worker
  printf 'LOCAL_INCREMENTAL_BUILD=PASS\n'
fi
[[ -x "${NODE_BIN}" ]] || { echo "node binary is not executable: ${NODE_BIN}" >&2; exit 1; }
[[ -x "${FORMAL_BIN}" ]] || { echo "Formal RPC binary is not executable: ${FORMAL_BIN}" >&2; exit 1; }
[[ -x "${WORKER_BIN}" ]] || { echo "worker binary is not executable: ${WORKER_BIN}" >&2; exit 1; }

if [[ -s "${CHAIN_SPEC}" ]]; then
  chain_id="$(jq -er '.id | strings' "${CHAIN_SPEC}")"
  [[ "${chain_id}" == "minijam_stage1_work_e2e" ]] || {
    echo "existing chain spec is not minijam_stage1_work_e2e: ${chain_id}" >&2
    echo 'set MINIJAM_LOCAL_PURGE=1 before starting a different local profile' >&2
    exit 1
  }
  [[ -s "${PRIVATE_ENV}" ]] || {
    echo "private local credentials are missing: ${PRIVATE_ENV}" >&2
    exit 1
  }
  relayer_uri="$(read_private_value MINIJAM_RELAYER_URI)"
  node_network_key="$(read_private_value MINIJAM_NODE_NETWORK_KEY)"
else
  if [[ -e "${PRIVATE_ENV}" ]]; then
    [[ -s "${PRIVATE_ENV}" ]] || { echo 'private local credentials are empty' >&2; exit 1; }
    relayer_uri="$(read_private_value MINIJAM_RELAYER_URI)"
    node_network_key="$(read_private_value MINIJAM_NODE_NETWORK_KEY)"
  else
    relayer_uri="0x$(openssl rand -hex 32)"
    node_network_key="$("${NODE_BIN}" key generate-node-key 2>/dev/null | tr -d '\r\n')"
    umask 077
    {
      printf 'MINIJAM_RELAYER_URI=%s\n' "${relayer_uri}"
      printf 'MINIJAM_NODE_NETWORK_KEY=%s\n' "${node_network_key}"
    } >"${PRIVATE_ENV}"
    chmod 600 "${PRIVATE_ENV}"
  fi
  relayer_info="$("${NODE_BIN}" key inspect "${relayer_uri}")"
  relayer_public_key="$(sed -nE 's/^[[:space:]]*Public key \(hex\):[[:space:]]*(0x[0-9a-fA-F]{64})[[:space:]]*$/\1/p' <<<"${relayer_info}" | head -n1)"
  [[ "${relayer_public_key}" =~ ^0x[0-9a-fA-F]{64}$ ]] || { echo 'unable to derive relayer AccountId32' >&2; exit 1; }
  [[ "${node_network_key}" =~ ^(0x)?[0-9a-fA-F]{64}$ ]] || { echo 'invalid ephemeral node network key' >&2; exit 1; }

  MINIJAM_STAGE1_INGRESS_RELAYER_PUBLIC_KEY="${relayer_public_key}" \
  MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY="${relayer_public_key}" \
    "${NODE_BIN}" build-spec --chain stage1-work-e2e >"${CHAIN_SPEC}"
  chain_id="$(jq -er '.id | strings' "${CHAIN_SPEC}")"
  [[ "${chain_id}" == "minijam_stage1_work_e2e" ]] || { echo "unexpected chain id: ${chain_id}" >&2; exit 1; }
  printf 'MINIJAM_LOCAL_WORK_E2E_CHAIN_SPEC=PASS\n'
fi

[[ "${relayer_uri}" =~ ^0x[0-9a-fA-F]{64}$ ]] || { echo 'invalid local relayer URI' >&2; exit 1; }
[[ "${node_network_key}" =~ ^(0x)?[0-9a-fA-F]{64}$ ]] || { echo 'invalid local node network key' >&2; exit 1; }

network_dir="${NODE_DATA}/chains/${chain_id}/network"
mkdir -p "${network_dir}"
printf '%s' "${node_network_key#0x}" >"${network_dir}/secret_ed25519"
chmod 600 "${network_dir}/secret_ed25519"
printf 'MINIJAM_LOCAL_NETWORK_KEY_MATERIALIZED=PASS\n'

trap cleanup_on_error EXIT
setsid "${NODE_BIN}" \
  --chain "${CHAIN_SPEC}" \
  --base-path "${NODE_DATA}" \
  --alice \
  --force-authoring \
  --unsafe-rpc-external \
    --rpc-methods=safe \
    --rpc-cors=all \
    --port "${NODE_P2P_PORT}" \
    --prometheus-port "${NODE_PROMETHEUS_PORT}" \
    --rpc-port "${NODE_PORT}" \
  --name minijam-native-stage1-work-e2e \
  >>"${NODE_LOG}" 2>&1 &
printf '%s\n' "$!" >"${NODE_PID_FILE}"

wait_for_node
printf 'MINIJAM_LOCAL_NODE_RPC=PASS\n'
initial_best="$(block_number)"
initial_finalized="$(rpc_call chain_getFinalizedHead | jq -er '.result')"
initial_finalized_number="$(block_number "${initial_finalized}")"
wait_for_progress "${initial_best}" "${initial_finalized_number}"

MINIJAM_RPC_URL="ws://127.0.0.1:${NODE_PORT}" \
MINIJAM_FORMAL_RPC_BIND="127.0.0.1:${FORMAL_PORT}" \
MINIJAM_RELAYER_URI="${relayer_uri}" \
MINIJAM_BUNDLE_DIR="${BUNDLES}" \
  setsid "${FORMAL_BIN}" >>"${FORMAL_LOG}" 2>&1 &
printf '%s\n' "$!" >"${FORMAL_PID_FILE}"
wait_for_formal

for worker_id in "${WORKER_IDS[@]}"; do
  worker_log="${LOGS}/worker-${worker_id}.log"
  if [[ -f "${worker_log}" ]]; then
    WORKER_LOG_OFFSETS["${worker_id}"]="$(wc -c <"${worker_log}")"
  else
    WORKER_LOG_OFFSETS["${worker_id}"]=0
  fi
  mkdir -p "${WORKERS}/${worker_id}"
  setsid "${WORKER_BIN}" \
    --rpc-url="http://127.0.0.1:${NODE_PORT}" \
    --worker-id="${worker_id}" \
    --key="${WORKER_KEYS[${worker_id}]}" \
    --submit-candidates \
    --submit-support-votes \
    --poll-interval-ms=500 \
    --state-db="${WORKERS}/${worker_id}/state.toml" \
    --ipfs-gateway="${FORMAL_URL}" \
    --health-bind="127.0.0.1:$((WORKER_HEALTH_BASE_PORT + worker_id))" \
    >>"${worker_log}" 2>&1 &
  printf '%s\n' "$!" >"${RUNTIME}/worker-${worker_id}.pid"
done

for worker_id in "${WORKER_IDS[@]}"; do
  wait_for_worker "${worker_id}" "${WORKER_LOG_OFFSETS[${worker_id}]}"
done

umask 022
{
  printf 'MINIJAM_NODE_RPC=%s\n' "${NODE_RPC}"
  printf 'MINIJAM_FORMAL_RPC_URL=%s\n' "${FORMAL_URL}"
  printf 'MINIJAM_CHAIN_SPEC=%s\n' "${CHAIN_SPEC}"
  printf 'MINIJAM_CHAIN_ID=%s\n' "${chain_id}"
  printf 'MINIJAM_WORKER_COUNT=%s\n' "${#WORKER_IDS[@]}"
} >"${CONNECTION_ENV}"
chmod 644 "${CONNECTION_ENV}"

printf 'MINIJAM_LOCAL_NETWORK=READY\n'
printf 'MINIJAM_NODE_RPC=%s\n' "${NODE_RPC}"
printf 'MINIJAM_FORMAL_RPC_URL=%s\n' "${FORMAL_URL}"
