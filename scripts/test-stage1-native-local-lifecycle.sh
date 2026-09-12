#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
RUNTIME="${MINIJAM_NATIVE_LOCAL_RUNTIME:-${ROOT}/target/stage1-native-local}"
UP="${ROOT}/scripts/stage1-native-local-up.sh"
STATUS="${ROOT}/scripts/stage1-native-local-status.sh"
DOWN="${ROOT}/scripts/stage1-native-local-down.sh"
TMP="$(mktemp -d)"

cleanup() {
  local status=$?
  MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${DOWN}" >/dev/null 2>&1 || true
  rm -rf -- "${TMP}"
  return "${status}"
}
trap cleanup EXIT

for command in grep sha256sum awk; do
  command -v "${command}" >/dev/null 2>&1 || { echo "${command} is required" >&2; exit 127; }
done

MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${UP}" >"${TMP}/first-up.log"
grep -Fxq 'MINIJAM_LOCAL_NETWORK=READY' "${TMP}/first-up.log"
if grep -Fxq 'MINIJAM_LOCAL_NETWORK=ALREADY_RUNNING' "${TMP}/first-up.log"; then
  echo 'native lifecycle test requires a stopped, isolated runtime' >&2
  exit 1
fi
printf 'MINIJAM_NATIVE_LOCAL_FIRST_UP=PASS\n'

spec_checksum="$(sha256sum "${RUNTIME}/chain-spec.json" | awk '{print $1}')"
MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${UP}" >"${TMP}/second-up.log"
grep -Fxq 'MINIJAM_LOCAL_NETWORK=ALREADY_RUNNING' "${TMP}/second-up.log"
printf 'MINIJAM_NATIVE_LOCAL_SECOND_UP=PASS\n'

MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${STATUS}" >"${TMP}/status.log"
grep -Fxq 'MINIJAM_LOCAL_NETWORK=READY' "${TMP}/status.log"
for marker in \
  NODE_PROCESS=RUNNING NODE_RPC=PASS FORMAL_RPC_PROCESS=RUNNING FORMAL_RPC_READY=PASS \
  WORKER_0_PROCESS=RUNNING WORKER_0_HEALTH=PASS WORKER_0_NODE_POLL=PASS \
  WORKER_1_PROCESS=RUNNING WORKER_1_HEALTH=PASS WORKER_1_NODE_POLL=PASS \
  WORKER_2_PROCESS=RUNNING WORKER_2_HEALTH=PASS WORKER_2_NODE_POLL=PASS; do
  grep -Fxq "${marker}" "${TMP}/status.log"
done
printf 'MINIJAM_NATIVE_LOCAL_STATUS=PASS\n'

MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${DOWN}" >"${TMP}/down.log"
grep -Fxq 'MINIJAM_LOCAL_NETWORK=STOPPED' "${TMP}/down.log"
test -d "${RUNTIME}/node-data"
test -d "${RUNTIME}/workers/0"
test -d "${RUNTIME}/workers/1"
test -d "${RUNTIME}/workers/2"
printf 'MINIJAM_NATIVE_LOCAL_DOWN_PRESERVES_DATA=PASS\n'

MINIJAM_NATIVE_LOCAL_RUNTIME="${RUNTIME}" "${UP}" >"${TMP}/restart-up.log"
grep -Fxq 'MINIJAM_LOCAL_NETWORK=READY' "${TMP}/restart-up.log"
test "${spec_checksum}" = "$(sha256sum "${RUNTIME}/chain-spec.json" | awk '{print $1}')"
printf 'MINIJAM_NATIVE_LOCAL_RESTART_REUSES_DATA=PASS\n'
printf 'MINIJAM_NATIVE_LOCAL_LIFECYCLE=PASS\n'
