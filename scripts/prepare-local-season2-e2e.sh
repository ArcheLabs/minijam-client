#!/usr/bin/env bash
set -euo pipefail

# Run a disposable Season 2 E2E with fresh identities.  Private material is
# kept below mktemp(1)'s directory and removed by the EXIT trap; only the
# generated public chain-spec inputs are passed to the child process.
root="$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
node="${MINIJAM_NODE_BIN:-${root}/target/release/minijam-node}"
if [[ ! -x "${node}" ]]; then
  echo "build target/release/minijam-node before preparing E2E credentials" >&2
  exit 1
fi
if [[ "$#" -eq 0 ]]; then
  echo "usage: $0 <season2-e2e-command> [args...]" >&2
  exit 2
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/minijam-season2-e2e.XXXXXX")"
trap 'rm -rf "${tmp}"' EXIT
umask 077
mkdir -m 700 "${tmp}/secrets"

make_seed() {
  local path="$1"
  printf '0x%s\n' "$(openssl rand -hex 32)" >"${path}"
  chmod 600 "${path}"
}
public_key() {
  "${node}" key inspect --scheme sr25519 "$1" |
    awk '/Public key \(hex\):/ { print $4; found = 1 } END { exit !found }'
}

make_seed "${tmp}/secrets/submitter.seed"
make_seed "${tmp}/secrets/ingress-relayer.seed"
make_seed "${tmp}/secrets/allocation-relayer.seed"
make_seed "${tmp}/secrets/worker.seed"

submitter_seed="$(<"${tmp}/secrets/submitter.seed")"
ingress_uri="$(<"${tmp}/secrets/ingress-relayer.seed")"
allocation_uri="$(<"${tmp}/secrets/allocation-relayer.seed")"
ingress_public="$(public_key "${tmp}/secrets/ingress-relayer.seed")"
allocation_public="$(public_key "${tmp}/secrets/allocation-relayer.seed")"

spec_dir="${tmp}/chain-specs"
mkdir -m 700 "${spec_dir}"
MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY="${ingress_public}" \
MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY="${allocation_public}" \
  "${root}/scripts/export-season2-chain-specs.sh" "${node}" "${spec_dir}"

export MINIJAM_E2E_WALLET_SEED="${submitter_seed}"
export MINIJAM_RELAYER_URI="${ingress_uri}"
export MINIJAM_ALLOCATION_RELAYER_URI="${allocation_uri}"
export MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY="${ingress_public}"
export MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY="${allocation_public}"
export MINIJAM_WORKER_KEY_FILE="${tmp}/secrets/worker.seed"
export MINIJAM_SEASON2_CHAIN_SPEC_FILE="${spec_dir}/season2.json"
export MINIJAM_E2E_PULL_POLICY="never"
export MINIJAM_LOCAL_E2E_SECRETS_DIR="${tmp}"

echo "using disposable Season 2 identities"
echo "ingress_public=${ingress_public}"
echo "allocation_public=${allocation_public}"
echo "chain_spec=${MINIJAM_SEASON2_CHAIN_SPEC_FILE}"
"$@"
