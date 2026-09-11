#!/usr/bin/env bash
set -euo pipefail

# Recovery acceptance for the persistent native provider. Work ingress remains
# a separate consumer operation and is exercised through the public APIs.
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
RUNTIME="${MINIJAM_NATIVE_LOCAL_RUNTIME:-${ROOT}/target/stage1-native-local}"
UP="${ROOT}/scripts/stage1-native-local-up.sh"
STATUS="${ROOT}/scripts/stage1-native-local-status.sh"
WORK_E2E="${ROOT}/scripts/test-stage1-native-work-e2e.sh"

stop_role() {
  local role="$1" pid_file="${RUNTIME}/${role}.pid" pid
  [[ -s "${pid_file}" ]] || { echo "missing ${role} pid file" >&2; return 1; }
  pid="$(<"${pid_file}")"
  [[ "${pid}" =~ ^[1-9][0-9]*$ ]] || { echo "invalid ${role} pid" >&2; return 1; }
  kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
  for _ in $(seq 1 50); do
    kill -0 "${pid}" 2>/dev/null || return 0
    sleep 0.1
  done
  echo "${role} did not stop after SIGTERM" >&2
  return 1
}

recover_role() {
  local role="$1" status_output up_output
  stop_role "${role}"
  status_output="$(MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${STATUS}" 2>&1 || true)"
  grep -Fxq 'MINIJAM_LOCAL_NETWORK=NOT_READY' <<<"${status_output}"
  printf 'MINIJAM_NATIVE_RECOVERY_%s_NOT_READY=PASS\n' "${role}"

  up_output="$(MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${UP}")"
  grep -Fxq 'MINIJAM_LOCAL_NETWORK=READY' <<<"${up_output}"
  printf 'MINIJAM_NATIVE_RECOVERY_%s_READY=PASS\n' "${role}"

  MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${WORK_E2E}"
  printf 'MINIJAM_NATIVE_RECOVERY_%s_WORK=PASS\n' "${role}"
}

initial_output="$(MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${UP}")"
if ! grep -Eq '^MINIJAM_LOCAL_NETWORK=(READY|ALREADY_RUNNING)$' <<<"${initial_output}"; then
  echo 'native provider did not become ready before recovery testing' >&2
  exit 1
fi
printf 'MINIJAM_NATIVE_RECOVERY_PROVIDER_READY=PASS\n'

recover_role worker-1
recover_role formal-rpc
recover_role node

test -s "${RUNTIME}/workers/1/state.toml"
printf 'MINIJAM_NATIVE_RECOVERY_WORKER_DB=PASS\n'
printf 'MINIJAM_NATIVE_WORK_RECOVERY=PASS\n'
