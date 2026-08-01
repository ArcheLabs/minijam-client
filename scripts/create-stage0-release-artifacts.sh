#!/usr/bin/env bash
set -euo pipefail

repository="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
release="${1:?usage: $0 RELEASE IMAGE_DIGEST_FILE [OUTPUT_DIR]}"
digest_file="${2:?usage: $0 RELEASE IMAGE_DIGEST_FILE [OUTPUT_DIR]}"
output="${3:-${repository}/stage0-release}"
node="${repository}/target/release/minijam-node"
worker="${repository}/target/release/minijam-worker"
wasm="${repository}/target/release/wbuild/minijam-runtime/minijam_runtime.compact.compressed.wasm"
plain="${repository}/chain-specs/stage0.json"
raw="${repository}/chain-specs/stage0-raw.json"

for file in "${node}" "${worker}" "${wasm}" "${digest_file}"; do
  test -f "${file}" || {
    echo "missing release input: ${file}" >&2
    exit 1
  }
done

"${repository}/scripts/export-stage0-chain-specs.sh" "${node}" "${repository}/chain-specs"

jq -e '
  .properties.tokenSymbol == "MINI"
  and .properties.tokenDecimals == 12
' "${plain}" >/dev/null

faucet_public="1a690444d160a1f63281203ede449ba996c560b7980e404375765f2aeacd886a"
sudo_public="64da539020cd743fed81ed5de922f0b3e7769bf3b77a953af3c0779ecefd7f23"
old_faucet="$(printf '91%.0s' {1..32})"
old_sudo="$(printf '81%.0s' {1..32})"
grep -q "5CfLJGrEfAnDLbNGQuSa5CUwGgU13gt7rsWXJLCsNCMFjDUr" "${plain}"
grep -q "5ELwW5Q5vLgPKqBpRxuQwGcaGwUhYUVzEd9MhfVUzWWdhLTr" "${plain}"
! grep -q "5EzWWGzTPufAaduJ648oGtjDV2ShXueWiJHkvo4951xWAVs8" "${plain}"
! grep -q "5FMa3U7CiNC6dUnokc6YMDvsef1X1B6go5Zeak5oEtc9eRXN" "${plain}"
grep -qi "${faucet_public}" "${raw}"
grep -qi "${sudo_public}" "${raw}"
! grep -qi "${old_faucet}" "${raw}"
! grep -qi "${old_sudo}" "${raw}"

scratch="$(mktemp -d)"
node_pid=""
cleanup() {
  if [[ -n "${node_pid}" ]]; then
    kill "${node_pid}" >/dev/null 2>&1 || true
    wait "${node_pid}" >/dev/null 2>&1 || true
  fi
  rm -rf "${scratch}"
}
trap cleanup EXIT

rpc_port="${MINIJAM_RELEASE_RPC_PORT:-19944}"
prometheus_port="${MINIJAM_RELEASE_PROMETHEUS_PORT:-19615}"
"${node}" \
  --chain="${raw}" \
  --base-path="${scratch}/node" \
  --rpc-port="${rpc_port}" \
  --rpc-methods=safe \
  --prometheus-port="${prometheus_port}" \
  --no-telemetry \
  --no-mdns \
  >"${scratch}/node.log" 2>&1 &
node_pid="$!"

genesis_hash=""
for _ in {1..90}; do
  if ! kill -0 "${node_pid}" >/dev/null 2>&1; then
    cat "${scratch}/node.log" >&2
    exit 1
  fi
  genesis_hash="$(
    curl -fsS \
      -H "content-type: application/json" \
      --data '{"id":1,"jsonrpc":"2.0","method":"chain_getBlockHash","params":[0]}' \
      "http://127.0.0.1:${rpc_port}" 2>/dev/null \
      | jq -r '.result // empty' \
      || true
  )"
  if [[ "${genesis_hash}" =~ ^0x[0-9a-f]{64}$ ]]; then
    break
  fi
  sleep 1
done
[[ "${genesis_hash}" =~ ^0x[0-9a-f]{64}$ ]] || {
  cat "${scratch}/node.log" >&2
  echo "failed to read Stage 0 genesis hash" >&2
  exit 1
}
kill "${node_pid}" >/dev/null 2>&1 || true
wait "${node_pid}" >/dev/null 2>&1 || true
node_pid=""

image() {
  local name="$1"
  local value
  value="$(grep -E "^ghcr\\.io/archelabs/${name}@sha256:[0-9a-f]{64}$" "${digest_file}")"
  test "$(wc -l <<<"${value}")" -eq 1
  printf '%s' "${value}"
}

node_image="$(image minijam-node)"
worker_image="$(image minijam-worker)"
compiler_image="$(image minijam-compiler-api)"
playground_api_image="$(image minijam-playground-api)"
playground_web_image="$(image minijam-playground-web)"
playground_web_local_image="$(image minijam-playground-web-local)"
relayer_public="${MINIJAM_STAGE0_RELAYER_PUBLIC_KEY:?set release Relayer public key}"

rm -rf "${output}"
mkdir -p "${output}"
install -m 0755 "${node}" "${output}/minijam-node"
install -m 0755 "${worker}" "${output}/minijam-worker"
install -m 0644 "${wasm}" "${output}/minijam_runtime.compact.compressed.wasm"
install -m 0644 "${plain}" "${output}/stage0-plain.json"
install -m 0644 "${raw}" "${output}/stage0-raw.json"

jq -n \
  --arg release "${release}" \
  --arg minijam_commit "$(git -C "${repository}" rev-parse HEAD)" \
  --arg jambda_commit "$(git -C "${repository}/external/jambda" rev-parse HEAD)" \
  --arg rust_toolchain "$(rustc --version)" \
  --arg wasm_hash "$(b2sum -l 256 "${wasm}" | awk '{print $1}')" \
  --arg plain_hash "$(sha256sum "${plain}" | awk '{print $1}')" \
  --arg raw_hash "$(sha256sum "${raw}" | awk '{print $1}')" \
  --arg genesis_hash "${genesis_hash}" \
  --arg node_image "${node_image}" \
  --arg worker_image "${worker_image}" \
  --arg compiler_image "${compiler_image}" \
  --arg playground_api_image "${playground_api_image}" \
  --arg playground_web_image "${playground_web_image}" \
  --arg playground_web_local_image "${playground_web_local_image}" \
  --arg relayer_public "${relayer_public}" \
  '{
    release: $release,
    minijam_client_commit: $minijam_commit,
    jambda_commit: $jambda_commit,
    rust_toolchain: $rust_toolchain,
    polkadot_sdk_commit: "2e4dd0bc22366a5af820492528869a493b5a5208",
    runtime_spec_version: 1,
    runtime_wasm_blake2_256: $wasm_hash,
    stage0_plain_chain_spec_sha256: $plain_hash,
    stage0_raw_chain_spec_sha256: $raw_hash,
    genesis_hash: $genesis_hash,
    token_symbol: "MINI",
    token_decimals: 12,
    faucet: {
      public_key: "0x1a690444d160a1f63281203ede449ba996c560b7980e404375765f2aeacd886a",
      ss58: "5CfLJGrEfAnDLbNGQuSa5CUwGgU13gt7rsWXJLCsNCMFjDUr",
      genesis_balance_mini: "1000000",
      drip_mini: "100",
      cooldown_blocks: 100
    },
    sudo: {
      public_key: "0x64da539020cd743fed81ed5de922f0b3e7769bf3b77a953af3c0779ecefd7f23",
      ss58: "5ELwW5Q5vLgPKqBpRxuQwGcaGwUhYUVzEd9MhfVUzWWdhLTr",
      genesis_balance_mini: "1000000"
    },
    playground_relayer: { public_key: $relayer_public },
    images: {
      node: $node_image,
      worker: $worker_image,
      compiler: $compiler_image,
      playground_api: $playground_api_image,
      playground_web: $playground_web_image,
      playground_web_local: $playground_web_local_image
    }
  }' >"${output}/release-manifest.json"

(
  cd "${output}"
  sha256sum \
    minijam-node \
    minijam-worker \
    minijam_runtime.compact.compressed.wasm \
    stage0-plain.json \
    stage0-raw.json \
    release-manifest.json \
    > SHA256SUMS
  sha256sum --check SHA256SUMS
)

echo "Stage 0 release artifacts created in ${output}"
