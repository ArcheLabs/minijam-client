#!/usr/bin/env bash
set -euo pipefail

repository="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
artifact_input="${1:?usage: $0 RELEASE_ARTIFACT_DIR}"
artifact="$(cd -- "${artifact_input}" && pwd -P)"
manifest="${artifact}/release-manifest.json"
sums="${artifact}/SHA256SUMS"
raw="${artifact}/stage0-raw.json"
release_dir="${repository}/chain-specs/stage0-release"
env_file="${repository}/deploy/stage0/.env"

test -f "${manifest}"
test -f "${sums}"
test -f "${raw}"

expected_release="v0.1.0-stage0.1"
expected_commit="bc0f0f20ee6d8086f5c1041c1294759b89f2b4fc"
expected_genesis="0x49a0833903d204f2658e1f19185d935500e1c16286ae16ffbc67eefad07f6d53"

(
  cd "${artifact}"
  sha256sum -c SHA256SUMS >/dev/null
)

[[ "$(jq -er '.release' "${manifest}")" == "${expected_release}" ]]
[[ "$(jq -er '.minijam_client_commit' "${manifest}")" == "${expected_commit}" ]]
[[ "$(jq -er '.genesis_hash' "${manifest}")" == "${expected_genesis}" ]]

install -d -m 0755 "${release_dir}" "${repository}/deploy/stage0"
for name in \
  SHA256SUMS \
  release-manifest.json \
  stage0-plain.json \
  stage0-raw.json \
  minijam_runtime.compact.compressed.wasm; do
  install -m 0644 "${artifact}/${name}" "${release_dir}/${name}"
done
install -m 0644 "${raw}" "${repository}/chain-specs/stage0-raw.json"

{
  printf 'MINIJAM_NODE_IMAGE=%s\n' "$(jq -er '.images.node' "${manifest}")"
  printf 'MINIJAM_WORKER_IMAGE=%s\n' "$(jq -er '.images.worker' "${manifest}")"
  printf 'MINIJAM_COMPILER_IMAGE=%s\n' "$(jq -er '.images.compiler' "${manifest}")"
  printf 'MINIJAM_PLAYGROUND_API_IMAGE=%s\n' "$(jq -er '.images.playground_api' "${manifest}")"
  printf 'MINIJAM_PLAYGROUND_WEB_IMAGE=%s\n' "$(jq -er '.images.playground_web' "${manifest}")"
  printf 'FAUCET_API_IMAGE=%s\n' "${FAUCET_API_IMAGE:-polkadot-testnet-faucet:minijam-local}"
  printf '\n'
  printf 'MINIJAM_CHAIN_SPEC_PATH=./chain-specs/stage0-raw.json\n'
  printf 'MINIJAM_RELEASE_MANIFEST_PATH=./chain-specs/stage0-release/release-manifest.json\n'
  printf 'MINIJAM_RELEASE_SHA256SUMS_PATH=./chain-specs/stage0-release/SHA256SUMS\n'
  printf 'MINIJAM_GENESIS_HASH=%s\n' "${expected_genesis}"
  printf 'MINIJAM_RELAYER_URI=\n'
  printf '\n'
  printf 'NODE_KEY_OR_SEED_PATH=./deploy/stage0/secrets/node-keystore.tar.gz\n'
  printf 'WORKER_1_SEED_PATH=./deploy/stage0/secrets/worker-1.seed\n'
  printf 'WORKER_2_SEED_PATH=./deploy/stage0/secrets/worker-2.seed\n'
  printf 'WORKER_3_SEED_PATH=./deploy/stage0/secrets/worker-3.seed\n'
  printf '\n'
  printf 'MINIJAM_WEB_BIND=127.0.0.1\n'
  printf 'MINIJAM_WEB_PORT=4173\n'
  printf 'FAUCET_BIND=127.0.0.1\n'
  printf 'FAUCET_PORT=5555\n'
  printf 'FAUCET_ACCOUNT_MNEMONIC=\n'
  printf 'FAUCET_DB_USERNAME=postgres\n'
  printf 'FAUCET_DB_PASSWORD=postgres\n'
  printf 'FAUCET_DB_DATABASE_NAME=faucet\n'
  printf 'RECAPTCHA_SECRET=\n'
} >"${env_file}"
chmod 0600 "${env_file}"

echo "Installed Stage 0 human-test release artifact and wrote deploy/stage0/.env"
