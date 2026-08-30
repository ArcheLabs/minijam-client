#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
node_bin="${1:-${root}/target/release/minijam-node}"
out_dir="${2:-${root}/chain-specs}"
: "${MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY:?set the Season 2 Ingress Relayer public key}"
: "${MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY:?set the Season 2 Allocation Relayer public key}"

if [[ ! -x "${node_bin}" ]]; then
  echo "export-season2-chain-specs: node binary is not executable: ${node_bin}" >&2
  exit 1
fi

mkdir -p "${out_dir}"
MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY="${MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY}" \
MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY="${MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY}" \
  "${node_bin}" export-chain-spec --chain season2 > "${out_dir}/season2.json"
MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY="${MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY}" \
MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY="${MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY}" \
  "${node_bin}" export-chain-spec --chain season2 --raw > "${out_dir}/season2-raw.json"

echo "export-season2-chain-specs: wrote ${out_dir}/season2.json and ${out_dir}/season2-raw.json"
