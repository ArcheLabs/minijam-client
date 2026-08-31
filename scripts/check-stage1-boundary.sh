#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
forbidden='playground\.minijam\.xyz|VITE_PLAYGROUND_API_URL|PLAYGROUND_API_URL|/api/v1/build|claim_faucet'
if rg -n "$forbidden" "$root/deploy/stage1" "$root/crates/minijam-formal-rpc" "$root/runtime" "$root/pallets/minijam"; then
  echo "Stage-1 boundary contains a legacy Playground or runtime-faucet dependency" >&2
  exit 1
fi
for required in 'node:' 'worker:' 'formal-rpc:'; do
  rg -q "^[[:space:]]*$required" "$root/deploy/stage1/compose.compact.yml"
done

