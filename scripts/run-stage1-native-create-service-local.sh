#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
NODE_BIN="${MINIJAM_NATIVE_NODE_BIN:-${ROOT}/target/debug/minijam-node}"
FORMAL_RPC_BIN="${MINIJAM_NATIVE_FORMAL_RPC_BIN:-${ROOT}/target/debug/minijam-formal-rpc}"
SERVICE_BLOB="${MINIJAM_NATIVE_SERVICE_BLOB:-${ROOT}/examples/services/counter/artifacts/counter-c.blob}"

command -v cargo >/dev/null 2>&1 || { echo 'cargo is required for local Native E2E' >&2; exit 127; }
command -v openssl >/dev/null 2>&1 || { echo 'openssl is required for local Native E2E' >&2; exit 127; }
command -v python3 >/dev/null 2>&1 || { echo 'python3 is required for local Native E2E' >&2; exit 127; }

build_started=$SECONDS
if [[ "${MINIJAM_SKIP_BUILD:-0}" == "1" ]]; then
  printf 'LOCAL_INCREMENTAL_BUILD=SKIPPED\n'
else
  cargo build --locked -p minijam-node -p minijam-formal-rpc
  printf 'LOCAL_INCREMENTAL_BUILD=PASS\n'
fi
printf 'LOCAL_BUILD_SECONDS=%s\n' "$((SECONDS - build_started))"

test -x "${NODE_BIN}" || { echo "node binary is not executable: ${NODE_BIN}" >&2; exit 1; }
test -x "${FORMAL_RPC_BIN}" || { echo "Formal RPC binary is not executable: ${FORMAL_RPC_BIN}" >&2; exit 1; }
test -s "${SERVICE_BLOB}" || { echo "service blob is missing or empty: ${SERVICE_BLOB}" >&2; exit 1; }

extract_sr25519_account_id() {
  sed -nE \
    's/^[[:space:]]*Public key \(hex\):[[:space:]]*(0x[0-9a-fA-F]{64})[[:space:]]*$/\1/p' \
    | head -n1
}

relayer_uri="0x$(openssl rand -hex 32)"
if ! relayer_info="$("${NODE_BIN}" key inspect "${relayer_uri}" 2>&1)"; then
  echo 'node failed to inspect the ephemeral sr25519 relayer URI' >&2
  exit 1
fi
relayer_public_key="$(extract_sr25519_account_id <<<"${relayer_info}")"
[[ "${relayer_public_key}" =~ ^0x[0-9a-fA-F]{64}$ ]] || {
  echo 'unable to derive the ephemeral relayer AccountId32' >&2
  exit 1
}

if ! node_network_key="$("${NODE_BIN}" key generate-node-key 2>/dev/null)"; then
  echo 'node failed to generate an ephemeral network key' >&2
  exit 1
fi
node_network_key="$(printf '%s' "${node_network_key}" | tr -d '\r\n')"
[[ "${node_network_key}" =~ ^(0x)?[0-9a-fA-F]{64}$ ]] || {
  echo 'node returned an invalid network key' >&2
  exit 1
}

service_code_hash="$(python3 - "${SERVICE_BLOB}" <<'PY'
import hashlib
import pathlib
import sys

print("0x" + hashlib.blake2b(pathlib.Path(sys.argv[1]).read_bytes(), digest_size=32).hexdigest())
PY
)"

printf 'LOCAL_RELAYER_ACCOUNT=%s\n' "${relayer_public_key}"
printf 'LOCAL_SERVICE_CODE_HASH=%s\n' "${service_code_hash}"

MINIJAM_NATIVE_NODE_BIN="${NODE_BIN}" \
MINIJAM_NATIVE_FORMAL_RPC_BIN="${FORMAL_RPC_BIN}" \
MINIJAM_NATIVE_RELAYER_URI="${relayer_uri}" \
MINIJAM_NATIVE_INGRESS_RELAYER_PUBLIC_KEY="${relayer_public_key}" \
MINIJAM_NATIVE_ALLOCATION_RELAYER_PUBLIC_KEY="${relayer_public_key}" \
MINIJAM_NODE_NETWORK_KEY="${node_network_key}" \
MINIJAM_NATIVE_SERVICE_BLOB="${SERVICE_BLOB}" \
MINIJAM_NATIVE_SERVICE_CODE_HASH="${service_code_hash}" \
  "${ROOT}/scripts/test-stage1-native-create-service.sh"
