#!/usr/bin/env bash
set -euo pipefail

repository="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

touch "${scratch}/keystore.tar.gz"
touch "${scratch}/worker-1" "${scratch}/worker-2" "${scratch}/worker-3"

digest="sha256:0000000000000000000000000000000000000000000000000000000000000000"
config="$(
  MINIJAM_NODE_IMAGE="ghcr.io/archelabs/minijam-node@${digest}" \
  MINIJAM_WORKER_IMAGE="ghcr.io/archelabs/minijam-worker@${digest}" \
  MINIJAM_COMPILER_IMAGE="ghcr.io/archelabs/minijam-compiler-api@${digest}" \
  MINIJAM_PLAYGROUND_API_IMAGE="ghcr.io/archelabs/minijam-playground-api@${digest}" \
  MINIJAM_PLAYGROUND_WEB_IMAGE="ghcr.io/archelabs/minijam-playground-web@${digest}" \
  MINIJAM_GENESIS_HASH="0x0000000000000000000000000000000000000000000000000000000000000000" \
  MINIJAM_RELAYER_URI="runtime-injected-value" \
  NODE_KEY_OR_SEED_PATH="${scratch}/keystore.tar.gz" \
  WORKER_1_SEED_PATH="${scratch}/worker-1" \
  WORKER_2_SEED_PATH="${scratch}/worker-2" \
  WORKER_3_SEED_PATH="${scratch}/worker-3" \
  docker compose -f "${repository}/compose.stage0.yml" config --format json
)"

jq -e '
  (.services | keys) == [
    "compiler-api",
    "node",
    "playground-api",
    "playground-web",
    "worker-1",
    "worker-2",
    "worker-3"
  ]
  and all(.services[]; has("build") | not)
  and all(.services[]; .pull_policy == "always")
  and all(.services[]; .image | test(
    "^ghcr\\.io/archelabs/minijam-[a-z-]+@sha256:[0-9a-f]{64}$"
  ))
  and (
    [.services | to_entries[] | select(.value | has("ports")) | .key]
    == ["playground-web"]
  )
  and (.services["playground-api"].environment.MINIJAM_COMPILER_URL
    == "http://compiler-api:8081")
  and (.services["playground-api"].environment.MINIJAM_RPC_URL
    == "ws://node:9944")
  and (.services["worker-1"].environment.MINIJAM_WORKER_SEED_FILE
    == "/run/secrets/worker-seed")
' >/dev/null <<<"${config}"

MINIJAM_IMAGE_TAG=static-check \
  docker compose -f "${repository}/compose.dev.yml" config --quiet

echo "Stage 0 release and development Compose files are valid"
