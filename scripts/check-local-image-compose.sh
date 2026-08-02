#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
bundle="${1:?usage: $0 BUNDLE_DIRECTORY}"
env_file="${bundle}/release.env"
compose_file="${bundle}/compose.yml"
resolved="$(mktemp)"

trap 'rm -f "${resolved}"' EXIT

fail() {
  echo "check-local-image-compose: $*" >&2
  exit 1
}

require_jq() {
  local expression="$1"
  local message="$2"

  jq -e "${expression}" "${resolved}" >/dev/null || fail "${message}"
}

docker compose \
  --env-file "${env_file}" \
  -f "${compose_file}" \
  config \
  --format json \
  >"${resolved}"

image_count="$(
  grep -Eo \
    'ghcr\.io/archelabs/[a-z0-9-]+@sha256:[0-9a-f]{64}' \
    "${env_file}" |
    wc -l
)"

[[ "${image_count}" -eq 6 ]] ||
  fail "release.env must contain exactly six immutable image digests"

require_jq \
  '.name == "minijam-local"' \
  "Compose project name must be minijam-local"

require_jq \
  'all(.services[]; has("build") | not)' \
  "public bundle must not contain build directives"

require_jq \
  'all(.services[]?.volumes[]?; .type != "bind")' \
  "public bundle must not contain bind mounts"

if grep -Eq 'stage0-raw\.json|deploy/stage0/secrets' "${resolved}"; then
  fail "public bundle contains Stage 0 files or secret paths"
fi

require_jq \
  'any(.services.node.ports[]?; .host_ip == "127.0.0.1" and (.published | tostring) == "9944" and (.target | tostring) == "9944")' \
  "Node RPC must bind 127.0.0.1:9944 to container port 9944"

require_jq \
  'any(.services["playground-web"].ports[]?; .host_ip == "127.0.0.1" and (.published | tostring) == "4173" and (.target | tostring) == "8080")' \
  "Playground Web must bind 127.0.0.1:4173 to container port 8080"

require_jq \
  '(.services.node.command | index("--dev")) != null' \
  "local Node must use --dev"

require_jq \
  '(.services.node.command | index("--alice")) != null' \
  "local Node must use --alice"

require_jq \
  '(.services["worker-1"].command | index("--key=//Alice")) != null' \
  "worker-1 must use //Alice"

require_jq \
  '(.services["worker-2"].command | index("--key=//Bob")) != null' \
  "worker-2 must use //Bob"

require_jq \
  '(.services["worker-3"].command | index("--key=//Charlie")) != null' \
  "worker-3 must use //Charlie"

require_jq \
  '.services["playground-api"].environment.MINIJAM_RELAYER_URI == "0x9292929292929292929292929292929292929292929292929292929292929292"' \
  "public local bundle must use the deterministic local Relayer"

echo "Public local image Compose policy passed."
