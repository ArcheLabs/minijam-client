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
grep -Eq '^[[:space:]]+- --alice$' "$root/deploy/stage1/compose.create-service-e2e.yml"
grep -Eq '^[[:space:]]+- --force-authoring$' "$root/deploy/stage1/compose.create-service-e2e.yml"

for profile in compact split; do
  compose="$root/deploy/stage1/compose.${profile}.yml"
  if grep -Eq '^[[:space:]]+- --rpc-external$' "$compose"; then
    echo "${profile} Stage-1 validator profile uses the rejected --rpc-external flag" >&2
    exit 1
  fi
  grep -Eq '^[[:space:]]+- --unsafe-rpc-external$' "$compose"
  grep -Eq '^[[:space:]]+- --rpc-methods=safe$' "$compose"
  grep -Eq '^[[:space:]]+- --rpc-cors=all$' "$compose"
  printf '%s_VALIDATOR_RPC_BIND=PASS\n' "${profile^^}"
done
grep -Eq '^[[:space:]]+ports: \["127\.0\.0\.1:9944:9944"\]$' \
  "$root/deploy/stage1/compose.compact.yml"
grep -Eq '^[[:space:]]+networks: \[chain, node-edge\]$' \
  "$root/deploy/stage1/compose.compact.yml"
grep -Eq '^  chain: \{internal: true\}$' \
  "$root/deploy/stage1/compose.compact.yml"
if grep -Eq '0\.0\.0\.0:9944' "$root/deploy/stage1/compose.split.yml"; then
  echo 'Split Stage-1 profile must not publish node RPC on a public host interface' >&2
  exit 1
fi
release_spec_export="$root/scripts/export-stage1-chain-specs-image.sh"
grep -Fq 'build-spec --chain stage1 >' "${release_spec_export}"
if grep -RFn -- 'stage1-e2e' \
  "$root/.github/workflows/stage1-release.yml" \
  "$root/deploy/stage1" \
  "${release_spec_export}"; then
  echo 'Stage-1 release or deployment path must not use the E2E chain spec' >&2
  exit 1
fi
printf 'COMPACT_RPC_HOST_BOUNDARY=PASS\n'
printf 'COMPACT_NODE_EDGE=PASS\n'
printf 'RPC_METHODS_SAFE=PASS\n'
printf 'STAGE1_RELEASE_SPEC_NOT_E2E=PASS\n'
