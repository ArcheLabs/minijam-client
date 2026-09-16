#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
forbidden='playground\.minijam\.xyz|VITE_PLAYGROUND_API_URL|PLAYGROUND_API_URL|/api/v1/build|claim_faucet'
if grep -REn "$forbidden" "$root/deploy/stage1" "$root/crates/minijam-formal-rpc" "$root/runtime" "$root/pallets/minijam"; then
  echo "Stage-1 boundary contains a legacy Playground or runtime-faucet dependency" >&2
  exit 1
fi
for required in 'node:' 'worker:' 'formal-rpc:'; do
  grep -Eq "^[[:space:]]*$required" "$root/deploy/stage1/compose.compact.yml"
done

for compose in "$root/deploy/stage1/compose.compact.yml" "$root/deploy/stage1/compose.split.yml"; do
  worker_count="$(grep -Ec '^  worker:[[:space:]]*$' "$compose")"
  if [[ "$worker_count" != 1 ]]; then
    echo "Stage-1 compose must define exactly one worker service: $compose" >&2
    exit 1
  fi
  grep -Eq -- '--worker-id=0([,[:space:]]|$)' "$compose"
  if grep -Eq -- '--ipfs-gateway=[^,[:space:]]*/ipfs([,[:space:]]|$)' "$compose"; then
    echo "Stage-1 compose gateway must be the Formal RPC origin, not /ipfs" >&2
    exit 1
  fi
done

printf 'STAGE1_WORKER_SERVICE_COUNT=1\n'
printf 'STAGE1_WORKER_ID=0\n'
printf 'STAGE1_GATEWAY_BASE_URL=PASS\n'
