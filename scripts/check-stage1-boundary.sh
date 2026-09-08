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

for profile in compact split; do
  compose="$root/deploy/stage1/compose.${profile}.yml"
  if grep -Eq '^[[:space:]]+- --rpc-external$' "$compose"; then
    echo "${profile} Stage-1 validator profile uses the rejected --rpc-external flag" >&2
    exit 1
  fi
  grep -Eq '^[[:space:]]+- --unsafe-rpc-external$' "$compose"
  grep -Eq '^[[:space:]]+- --rpc-methods=safe$' "$compose"
  printf '%s_VALIDATOR_RPC_BIND=PASS\n' "${profile^^}"
done
printf 'RPC_METHODS_SAFE=PASS\n'
