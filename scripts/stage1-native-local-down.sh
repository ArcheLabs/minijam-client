#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
RUNTIME="${MINIJAM_NATIVE_LOCAL_RUNTIME:-${ROOT}/target/stage1-native-local}"

stop_process() {
  local file="$1" pid
  [[ -s "${file}" ]] || return 0
  pid="$(<"${file}")"
  if [[ "${pid}" =~ ^[1-9][0-9]*$ ]] && kill -0 "${pid}" 2>/dev/null; then
    kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
    for _ in $(seq 1 50); do
      kill -0 "${pid}" 2>/dev/null || break
      sleep 0.1
    done
    if kill -0 "${pid}" 2>/dev/null; then
      kill -KILL -- "-${pid}" 2>/dev/null || kill -KILL "${pid}" 2>/dev/null || true
    fi
  fi
  rm -f -- "${file}"
}

stop_process "${RUNTIME}/formal-rpc.pid"
stop_process "${RUNTIME}/node.pid"
printf 'MINIJAM_LOCAL_FORMAL_RPC=STOPPED\n'
printf 'MINIJAM_LOCAL_NODE=STOPPED\n'

if [[ "${MINIJAM_LOCAL_PURGE:-0}" == "1" ]]; then
  rm -rf -- "${RUNTIME}"
  printf 'MINIJAM_LOCAL_NETWORK=PURGED\n'
else
  printf 'MINIJAM_LOCAL_NETWORK=STOPPED\n'
fi
