#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
NODE_IMAGE="${MINIJAM_NODE_IMAGE:?set the exact Stage-1 node image reference}"
OUT="${MINIJAM_STAGE1_CHAIN_SPEC_DIR:-${ROOT}/chain-specs}"
: "${MINIJAM_STAGE1_INGRESS_RELAYER_PUBLIC_KEY:?set the public Work-ingress AccountId32}"
: "${MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY:?set the public allocation/deployment AccountId32}"

docker image inspect "${NODE_IMAGE}" >/dev/null
mkdir -p "${OUT}"

docker run --rm --network none \
  -e MINIJAM_STAGE1_INGRESS_RELAYER_PUBLIC_KEY \
  -e MINIJAM_STAGE1_ALLOCATION_RELAYER_PUBLIC_KEY \
  "${NODE_IMAGE}" \
  build-spec --chain stage1 > "${OUT}/stage1.json"

docker run --rm --network none \
  -v "${OUT}:/chain-specs:ro" \
  "${NODE_IMAGE}" \
  build-spec --chain /chain-specs/stage1.json --raw > "${OUT}/stage1-raw.json"

for spec in "${OUT}/stage1.json" "${OUT}/stage1-raw.json"; do
  jq -e '.id == "minijam_stage1"' "${spec}" >/dev/null
done
grep -Eq 'pub const SS58Prefix: u8 = 42' "${ROOT}/runtime/src/lib.rs"

printf 'STAGE1_IMAGE_CHAIN_SPEC=PASS\n'
