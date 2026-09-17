#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
forbidden='playground\.minijam\.xyz|VITE_PLAYGROUND_API_URL|PLAYGROUND_API_URL|/api/v1/build|claim_faucet'
if grep -REn "${forbidden}" \
    "${root}/deploy/stage1" \
    "${root}/crates/minijam-formal-rpc" \
    "${root}/runtime" \
    "${root}/pallets/minijam"; then
  echo 'Stage-1 boundary contains a legacy Playground or runtime-faucet dependency' >&2
  exit 1
fi

for required in 'node:' 'worker:' 'formal-rpc:'; do
  grep -Eq "^[[:space:]]*${required}" "${root}/deploy/stage1/compose.compact.yml"
done

runtime_source="${root}/runtime/src/lib.rs"
for expected in \
  'type WorkersPerWork = ConstU32<1>;' \
  'type MaxWorksPerRound = ConstU32<64>;' \
  'type MaxDutiesPerWorkerPerRound = ConstU32<64>;' \
  'type SupportThreshold = ConstU32<1>;' \
  'type OpposeThreshold = ConstU32<1>;' \
  'type MaxPendingWorks = ConstU32<64>'; do
  if ! grep -Eq "^[[:space:]]*${expected}" "${runtime_source}"; then
    echo "Stage-1 runtime capacity boundary is not canonical: ${expected}" >&2
    exit 1
  fi
done

for compose in "${root}/deploy/stage1/compose.compact.yml" "${root}/deploy/stage1/compose.split.yml"; do
  worker_count="$(grep -Ec '^  worker:[[:space:]]*$' "${compose}")"
  if [[ "${worker_count}" != 1 ]]; then
    echo "Stage-1 compose must define exactly one worker service: ${compose}" >&2
    exit 1
  fi
  grep -Eq -- '--worker-id=0([,[:space:]]|$)' "${compose}"
  grep -Fq -- '--ipfs-gateway=http://formal-rpc:8080' "${compose}"
  if grep -Eq -- '--ipfs-gateway=[^,[:space:]]*/ipfs([,[:space:]]|$)' "${compose}"; then
    echo "Stage-1 compose gateway must be the Formal RPC origin, not /ipfs" >&2
    exit 1
  fi
  if grep -Eq '^[[:space:]]+- --rpc-external$' "${compose}"; then
    echo "${compose} uses the deprecated rpc-external flag" >&2
    exit 1
  fi
  grep -Eq '^[[:space:]]+- --unsafe-rpc-external$' "${compose}"
  grep -Eq '^[[:space:]]+- --rpc-methods=safe$' "${compose}"
  grep -Eq '^[[:space:]]+- --rpc-cors=all$' "${compose}"
done

grep -Eq '^[[:space:]]+ports: \["127\.0\.0\.1:9944:9944"\]$' \
  "${root}/deploy/stage1/compose.compact.yml"
grep -Eq '^[[:space:]]+networks: \[chain, node-edge\]$' \
  "${root}/deploy/stage1/compose.compact.yml"
grep -Eq '^  chain: \{internal: true\}$' \
  "${root}/deploy/stage1/compose.compact.yml"
if grep -Eq '0\.0\.0\.0:9944' "${root}/deploy/stage1/compose.split.yml"; then
  echo 'Split Stage-1 profile must not publish node RPC on a public host interface' >&2
  exit 1
fi

release_spec_export="${root}/scripts/export-stage1-chain-specs-image.sh"
grep -Fq 'build-spec --chain stage1 >' "${release_spec_export}"
for production_file in \
  "${root}/deploy/stage1/compose.compact.yml" \
  "${root}/deploy/stage1/compose.split.yml" \
  "${root}/deploy/stage1/Dockerfile" \
  "${root}/.github/workflows/stage1-release.yml"; do
  if grep -Fq 'stage1-work-e2e' "${production_file}"; then
    echo "production path references the local-only stage1-work-e2e profile: ${production_file}" >&2
    exit 1
  fi
done

printf 'STAGE1_WORKER_SERVICE_COUNT=1\n'
printf 'STAGE1_WORKER_ID=0\n'
printf 'STAGE1_GATEWAY_BASE_URL=PASS\n'
printf 'STAGE1_CAPACITY_BOUNDARY=PASS\n'
printf 'COMPACT_RPC_HOST_BOUNDARY=PASS\n'
printf 'COMPACT_NODE_EDGE=PASS\n'
printf 'RPC_METHODS_SAFE=PASS\n'
printf 'STAGE1_RELEASE_SPEC_NOT_E2E=PASS\n'
