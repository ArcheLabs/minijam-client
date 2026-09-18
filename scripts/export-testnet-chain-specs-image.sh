#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
NODE_IMAGE="${MINIJAM_NODE_IMAGE:?set the exact MiniJAM node image reference}"
OUT="${MINIJAM_TESTNET_CHAIN_SPEC_DIR:-${ROOT}/chain-specs}"

docker image inspect "${NODE_IMAGE}" >/dev/null
mkdir -p "${OUT}"

docker run --rm --network none "${NODE_IMAGE}" \
  build-spec --chain testnet > "${OUT}/testnet.json"

docker run --rm --network none \
  -v "${OUT}:/chain-specs:ro" \
  "${NODE_IMAGE}" \
  build-spec --chain /chain-specs/testnet.json --raw > "${OUT}/testnet-raw.json"

for spec in "${OUT}/testnet.json" "${OUT}/testnet-raw.json"; do
  jq -e '.id == "minijam_testnet"' "${spec}" >/dev/null
done
grep -Eq 'pub const SS58Prefix: u8 = 42' "${ROOT}/runtime/src/lib.rs"

printf 'TESTNET_IMAGE_CHAIN_SPEC=PASS\n'
