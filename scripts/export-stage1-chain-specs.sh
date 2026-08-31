#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
node="${MINIJAM_NODE_BIN:-$root/target/release/minijam-node}"
out="${MINIJAM_STAGE1_CHAIN_SPEC_DIR:-$root/chain-specs}"
: "${MINIJAM_STAGE1_INGRESS_RELAYER_PUBLIC_KEY:?set the public Work-ingress AccountId32}"
: "${MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY:?set the public allocation/deployment AccountId32}"
mkdir -p "$out"
"$node" build-spec --chain stage1 > "$out/stage1.json"
"$node" build-spec --chain "$out/stage1.json" --raw > "$out/stage1-raw.json"
