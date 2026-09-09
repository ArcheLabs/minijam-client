#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"

"${ROOT}/scripts/check-stage1-boundary.sh"
"${ROOT}/scripts/check-release-secret-hygiene.sh"
command -v docker >/dev/null 2>&1 || {
  echo 'Docker is required for rendered Stage-1 Compose hardening checks' >&2
  exit 127
}
docker info >/dev/null 2>&1 || {
  echo 'A running Docker daemon is required for rendered Stage-1 Compose hardening checks' >&2
  exit 1
}
command -v jq >/dev/null 2>&1 || {
  echo 'jq is required for rendered Stage-1 Compose hardening checks' >&2
  exit 127
}

render_compose() {
  local profile="$1"
  MINIJAM_NODE_NETWORK_KEY=0x1111111111111111111111111111111111111111111111111111111111111111 \
  MINIJAM_WORKER_SEED=0x2222222222222222222222222222222222222222222222222222222222222222 \
  MINIJAM_FORMAL_RPC_RELAYER_URI=0x3333333333333333333333333333333333333333333333333333333333333333 \
  MINIJAM_RPC_URL=ws://node:9944 \
  MINIJAM_STAGE1_CHAIN_SPEC_FILE=/etc/hosts \
    docker compose -f "${ROOT}/deploy/stage1/compose.${profile}.yml" config --format json
}

compact="$(render_compose compact)"
split="$(render_compose split)"

jq -e '.services.node.command | index("--unsafe-rpc-external") != null' <<<"${compact}" >/dev/null
jq -e '.services.node.command | index("--rpc-methods=safe") != null' <<<"${compact}" >/dev/null
jq -e '.services.node.command | index("--rpc-cors=all") != null' <<<"${compact}" >/dev/null
jq -e '.services.node.command | index("--rpc-methods=unsafe") == null' <<<"${compact}" >/dev/null
jq -e 'any(.services.node.ports[]?; (.published | tostring) == "9944" and .host_ip == "127.0.0.1")' <<<"${compact}" >/dev/null
jq -e '.services.node.networks | has("chain") and has("node-edge")' <<<"${compact}" >/dev/null
jq -e '.networks.chain.internal == true' <<<"${compact}" >/dev/null

jq -e '.services.node.command | index("--rpc-cors=all") != null' <<<"${split}" >/dev/null
jq -e '.services.node.command | index("--rpc-methods=safe") != null' <<<"${split}" >/dev/null
jq -e '.services.node.command | index("--rpc-methods=unsafe") == null' <<<"${split}" >/dev/null
jq -e '(.services.node.ports // []) | length == 0' <<<"${split}" >/dev/null
jq -e '.networks.chain.external == true' <<<"${split}" >/dev/null

printf 'STAGE1_HARDENING=PASS\n'
