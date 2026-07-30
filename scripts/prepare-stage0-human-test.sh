#!/usr/bin/env bash
set -euo pipefail

repository="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
env_file="${repository}/deploy/stage0/.env"
compose_file="${repository}/compose.stage0.yml"
expected_commit="bc0f0f20ee6d8086f5c1041c1294759b89f2b4fc"
expected_genesis="0x49a0833903d204f2658e1f19185d935500e1c16286ae16ffbc67eefad07f6d53"
expected_aura="0x66d09cb4dff344d5a6b07ca9909dc05f46ccda6687c52d7dad9983c7fe891619"
expected_grandpa="0x4e0ca8042d49c595cf51103a96313bcf72b37cc178b7615382e9358fd95bee0d"
expected_workers=(
  "0x326c5d73920e92464386ba7a3ac19522b855edc95088b2933fa5470fca941a0f"
  "0xdcf28e115d439663a12e8cba10e76bc6e9503469ad27ab03e1140ddac931cc53"
  "0xce5c3f3290a1ac97f3145d96c7a49d14a8b670f465a1de88d7c520cb6d11273d"
)
expected_relayer="0x901578a417300aa0ae533b5bd0e9af489a4cc4a6f38999b76283867087738209"
expected_faucet="0x1a690444d160a1f63281203ede449ba996c560b7980e404375765f2aeacd886a"

failures=0

pass() {
  printf 'PASS %s\n' "$1"
}

fail() {
  printf 'FAIL %s\n' "$1"
  failures=$((failures + 1))
}

require_command() {
  if command -v "$1" >/dev/null 2>&1; then
    pass "command ${1}"
  else
    fail "command ${1} not found"
  fi
}

resolve_path() {
  local value="$1"
  if [[ "${value}" = /* ]]; then
    printf '%s\n' "${value}"
  else
    printf '%s\n' "${repository}/${value#./}"
  fi
}

require_command docker
require_command jq
require_command sha256sum
if command -v docker >/dev/null 2>&1; then
  docker version >/dev/null 2>&1 && pass "docker daemon is reachable" || fail "docker daemon is reachable"
fi

if [[ -f "${env_file}" ]]; then
  pass "deploy/stage0/.env exists"
  set -a
  # shellcheck disable=SC1090
  source "${env_file}"
  set +a
else
  fail "deploy/stage0/.env exists"
fi

image_variables=(
  MINIJAM_NODE_IMAGE
  MINIJAM_WORKER_IMAGE
  MINIJAM_COMPILER_IMAGE
  MINIJAM_PLAYGROUND_API_IMAGE
  MINIJAM_PLAYGROUND_WEB_IMAGE
)
for name in "${image_variables[@]}"; do
  value="${!name:-}"
  if [[ "${value}" =~ ^ghcr\.io/archelabs/minijam-[a-z-]+@sha256:[0-9a-f]{64}$ ]]; then
    pass "${name} uses immutable GHCR digest"
  else
    fail "${name} uses immutable GHCR digest"
  fi
done

manifest="$(resolve_path "${MINIJAM_RELEASE_MANIFEST_PATH:-./chain-specs/release-manifest.json}")"
raw="$(resolve_path "${MINIJAM_CHAIN_SPEC_PATH:-./chain-specs/stage0-raw.json}")"
sums="$(resolve_path "${MINIJAM_RELEASE_SHA256SUMS_PATH:-./chain-specs/SHA256SUMS}")"

if [[ -f "${manifest}" ]]; then
  pass "release-manifest.json exists"
else
  fail "release-manifest.json exists"
fi
if [[ -f "${raw}" ]]; then
  pass "stage0-raw.json exists"
else
  fail "stage0-raw.json exists"
fi
if [[ -f "${sums}" ]]; then
  (
    cd "$(dirname -- "${sums}")"
    sha256sum -c "$(basename -- "${sums}")" >/dev/null
  ) && pass "SHA256SUMS verify" || fail "SHA256SUMS verify"
else
  fail "SHA256SUMS exists"
fi

if [[ -f "${manifest}" ]]; then
  [[ "$(jq -er '.release' "${manifest}")" == "v0.1.0-stage0.1" ]] \
    && pass "release tag matches v0.1.0-stage0.1" || fail "release tag matches v0.1.0-stage0.1"
  [[ "$(jq -er '.minijam_client_commit' "${manifest}")" == "${expected_commit}" ]] \
    && pass "MiniJAM commit matches release baseline" || fail "MiniJAM commit matches release baseline"
  [[ "$(jq -er '.genesis_hash' "${manifest}")" == "${expected_genesis}" ]] \
    && pass "genesis hash matches release baseline" || fail "genesis hash matches release baseline"
  [[ "$(jq -er '.faucet.public_key' "${manifest}")" == "${expected_faucet}" ]] \
    && pass "faucet public key matches genesis" || fail "faucet public key matches genesis"
  for key in node worker compiler playground_api playground_web; do
    jq -er ".images.${key}" "${manifest}" | grep -Eq '^ghcr\.io/archelabs/minijam-[a-z-]+@sha256:[0-9a-f]{64}$' \
      && pass "manifest image ${key} uses digest" || fail "manifest image ${key} uses digest"
  done
fi

if [[ "${MINIJAM_GENESIS_HASH:-}" == "${expected_genesis}" ]]; then
  pass "MINIJAM_GENESIS_HASH matches release baseline"
else
  fail "MINIJAM_GENESIS_HASH matches release baseline"
fi

node_bin="${repository}/target/release/minijam-node"
if [[ -x "${node_bin}" ]]; then
  public_key() {
    local scheme="$1"
    local secret="$2"
    "${node_bin}" key inspect --scheme "${scheme}" "${secret}" \
      | awk '/Public key \(hex\):/ { print $4; found = 1 } END { exit !found }'
  }

  for index in 1 2 3; do
    var="WORKER_${index}_SEED_PATH"
    worker_path="$(resolve_path "${!var:-}")"
    if [[ -f "${worker_path}" ]]; then
      worker_public="$(public_key sr25519 "${worker_path}")"
      [[ "${worker_public}" == "${expected_workers[$((index - 1))]}" ]] \
        && pass "worker-${index} public key matches worker-id $((index - 1))" \
        || fail "worker-${index} public key matches worker-id $((index - 1))"
    else
      fail "worker-${index} seed file exists"
    fi
  done

  if [[ -n "${MINIJAM_RELAYER_URI:-}" ]]; then
    relayer_public="$(public_key sr25519 "${MINIJAM_RELAYER_URI}")"
    [[ "${relayer_public}" == "${expected_relayer}" ]] \
      && pass "relayer public key matches runtime relayer" || fail "relayer public key matches runtime relayer"
  else
    fail "MINIJAM_RELAYER_URI is set"
  fi

  if [[ -n "${FAUCET_ACCOUNT_MNEMONIC:-}" ]]; then
    faucet_public="$(public_key sr25519 "${FAUCET_ACCOUNT_MNEMONIC}")"
    [[ "${faucet_public}" == "${expected_faucet}" ]] \
      && pass "faucet account matches genesis faucet" || fail "faucet account matches genesis faucet"
  else
    fail "FAUCET_ACCOUNT_MNEMONIC is set"
  fi
else
  fail "target/release/minijam-node is executable for key checks"
fi

keystore_path="$(resolve_path "${NODE_KEY_OR_SEED_PATH:-}")"
if [[ -f "${keystore_path}" ]]; then
  tar -tzf "${keystore_path}" | grep -Fq "61757261${expected_aura#0x}" \
    && pass "node keystore contains expected Aura key" || fail "node keystore contains expected Aura key"
  tar -tzf "${keystore_path}" | grep -Fq "6772616e${expected_grandpa#0x}" \
    && pass "node keystore contains expected GRANDPA key" || fail "node keystore contains expected GRANDPA key"
else
  fail "node keystore archive exists"
fi

secret_paths=("${keystore_path}")
for index in 1 2 3; do
  var="WORKER_${index}_SEED_PATH"
  secret_paths+=("$(resolve_path "${!var:-}")")
done
for path in "${secret_paths[@]}"; do
  if [[ -f "${path}" ]]; then
    mode="$(stat -c '%a' "${path}")"
    owner="$(stat -c '%u:%g' "${path}")"
    [[ "${mode}" == "400" ]] && pass "secret $(basename -- "${path}") mode 0400" || fail "secret $(basename -- "${path}") mode 0400"
    [[ "${owner}" == "10001:10001" ]] && pass "secret $(basename -- "${path}") owner 10001:10001" || fail "secret $(basename -- "${path}") owner 10001:10001"
  fi
done

if command -v docker >/dev/null 2>&1; then
  config="$(docker compose --env-file "${env_file}" -f "${compose_file}" config --format json 2>/dev/null || true)"
  if [[ -n "${config}" ]] && jq empty >/dev/null 2>&1 <<<"${config}"; then
    jq -e 'all(.services[]; has("build") | not)' >/dev/null <<<"${config}" \
      && pass "compose has no build sections" || fail "compose has no build sections"
    jq -e '(.services | keys | length) == 9' >/dev/null <<<"${config}" \
      && pass "compose defines 9 services" || fail "compose defines 9 services"
  else
    fail "compose config renders"
  fi
fi

for port in "${MINIJAM_WEB_PORT:-4173}" "${FAUCET_PORT:-5555}"; do
  if ss -ltn "( sport = :${port} )" | awk 'NR > 1 { found = 1 } END { exit found ? 0 : 1 }'; then
    fail "port ${port} is free"
  else
    pass "port ${port} is free"
  fi
done

if ((failures == 0)); then
  printf 'PASS Stage 0 human Playground preflight\n'
else
  printf 'FAIL Stage 0 human Playground preflight (%d failures)\n' "${failures}"
  exit 1
fi
