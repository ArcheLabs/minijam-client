#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
forbidden='/api/v1/build|claim_faucet'
if grep -REn "$forbidden" "$root/deploy/stage1" "$root/crates/minijam-formal-rpc" "$root/runtime" "$root/pallets/minijam"; then
  echo "Stage-1 boundary contains a legacy application or runtime-faucet dependency" >&2
  exit 1
fi
for required in 'node:' 'worker:' 'formal-rpc:'; do
  grep -Eq "^[[:space:]]*$required" "$root/deploy/stage1/compose.compact.yml"
done
