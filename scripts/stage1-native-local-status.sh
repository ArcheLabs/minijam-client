#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
RUNTIME="${MINIJAM_NATIVE_LOCAL_RUNTIME:-${ROOT}/target/stage1-native-local}"
NODE_RPC="${MINIJAM_NODE_RPC:-http://127.0.0.1:9944}"
FORMAL_URL="${MINIJAM_FORMAL_RPC_URL:-http://127.0.0.1:8090}"

rpc_call() {
  local method="$1" params="${2:-[]}"
  curl -fsS --max-time 5 -H 'content-type: application/json' \
    --data "$(jq -cn --arg method "${method}" --argjson params "${params}" \
      '{id: 1, jsonrpc: "2.0", method: $method, params: $params}')" "${NODE_RPC}"
}

block_number() {
  local value="$1"
  case "${value}" in
    0x*) printf '%d\n' "$((16#${value#0x}))" ;;
    0X*) printf '%d\n' "$((16#${value#0X}))" ;;
    *) printf '%d\n' "${value}" ;;
  esac
}

node_ok=0
best=unknown
finalized=unknown
if [[ -s "${RUNTIME}/node.pid" ]] && kill -0 "$(<"${RUNTIME}/node.pid")" 2>/dev/null; then
  printf 'NODE_PROCESS=RUNNING\n'
  if health="$(rpc_call system_health 2>/dev/null)" && jq -e '.result != null and .error == null' <<<"${health}" >/dev/null; then
    node_ok=1
    printf 'NODE_RPC=PASS\n'
    best_hex="$(rpc_call chain_getHeader 2>/dev/null | jq -er '.result.number' 2>/dev/null || true)"
    finalized_hex="$(rpc_call chain_getFinalizedHead 2>/dev/null | jq -er '.result' 2>/dev/null || true)"
    if [[ -n "${best_hex}" ]]; then best="$(block_number "${best_hex}")"; fi
    if [[ -n "${finalized_hex}" ]]; then
      final_hex="$(rpc_call chain_getHeader "[\"${finalized_hex}\"]" 2>/dev/null | jq -er '.result.number' 2>/dev/null || true)"
      [[ -n "${final_hex}" ]] && finalized="$(block_number "${final_hex}")"
    fi
  else
    printf 'NODE_RPC=FAIL\n'
  fi
else
  printf 'NODE_PROCESS=STOPPED\n'
  printf 'NODE_RPC=FAIL\n'
fi
printf 'BEST_BLOCK=%s\n' "${best}"
printf 'FINALIZED_BLOCK=%s\n' "${finalized}"

formal_ok=0
if [[ -s "${RUNTIME}/formal-rpc.pid" ]] && kill -0 "$(<"${RUNTIME}/formal-rpc.pid")" 2>/dev/null; then
  printf 'FORMAL_RPC_PROCESS=RUNNING\n'
  if response="$(curl -fsS --max-time 5 "${FORMAL_URL}/health/ready" 2>/dev/null)" \
    && jq -e '.status == "ready"' <<<"${response}" >/dev/null; then
    formal_ok=1
    printf 'FORMAL_RPC_READY=PASS\n'
  else
    printf 'FORMAL_RPC_READY=FAIL\n'
  fi
else
  printf 'FORMAL_RPC_PROCESS=STOPPED\n'
  printf 'FORMAL_RPC_READY=FAIL\n'
fi

if (( node_ok == 1 && formal_ok == 1 )); then
  printf 'MINIJAM_LOCAL_NETWORK=READY\n'
else
  printf 'MINIJAM_LOCAL_NETWORK=NOT_READY\n'
  exit 1
fi
