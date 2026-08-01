#!/usr/bin/env bash
set -euo pipefail

repository="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
artifacts_input="${1:?usage: $0 RELEASE_ARTIFACT_DIR}"
test -d "${artifacts_input}"
artifacts="$(cd -- "${artifacts_input}" && pwd -P)"
manifest="${artifacts}/release-manifest.json"
raw="${artifacts}/stage0-raw.json"
compose_file="${repository}/compose.stage0.yml"
project="minijam-stage0-release-smoke"

required_environment=(
  MINIJAM_RELEASE_NODE_KEYSTORE_B64
  MINIJAM_RELEASE_RELAYER_URI
  MINIJAM_RELEASE_WORKER_1_URI
  MINIJAM_RELEASE_WORKER_2_URI
  MINIJAM_RELEASE_WORKER_3_URI
  MINIJAM_E2E_WALLET_SEED
)
for name in "${required_environment[@]}"; do
  test -n "${!name:-}" || {
    echo "missing release smoke environment: ${name}" >&2
    exit 1
  }
done
test -f "${manifest}"
test -f "${raw}"

scratch="$(mktemp -d)"
env_file="${scratch}/stage0.env"
diagnostics="${MINIJAM_RELEASE_DIAGNOSTICS:-${repository}/stage0-release-diagnostics}"
compose=(docker compose --project-name "${project}" --env-file "${env_file}" -f "${compose_file}")

cleanup() {
  "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
  rm -rf "${scratch}"
}
trap cleanup EXIT

printf '%s' "${MINIJAM_RELEASE_NODE_KEYSTORE_B64}" \
  | base64 --decode >"${scratch}/node-keystore.tar.gz"
if tar -tzf "${scratch}/node-keystore.tar.gz" \
  | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
  echo "unsafe path in Stage 0 Node keystore archive" >&2
  exit 1
fi
chmod 600 "${scratch}/node-keystore.tar.gz"

printf '%s\n' "${MINIJAM_RELEASE_WORKER_1_URI}" >"${scratch}/worker-1.seed"
printf '%s\n' "${MINIJAM_RELEASE_WORKER_2_URI}" >"${scratch}/worker-2.seed"
printf '%s\n' "${MINIJAM_RELEASE_WORKER_3_URI}" >"${scratch}/worker-3.seed"
sudo chown 10001:10001 \
  "${scratch}/node-keystore.tar.gz" \
  "${scratch}/worker-1.seed" \
  "${scratch}/worker-2.seed" \
  "${scratch}/worker-3.seed"
sudo chmod 0400 \
  "${scratch}/node-keystore.tar.gz" \
  "${scratch}/worker-1.seed" \
  "${scratch}/worker-2.seed" \
  "${scratch}/worker-3.seed"

node_image="$(jq -er '.images.node' "${manifest}")"
worker_image="$(jq -er '.images.worker' "${manifest}")"
compiler_image="$(jq -er '.images.compiler' "${manifest}")"
playground_api_image="$(jq -er '.images.playground_api' "${manifest}")"
playground_web_image="$(jq -er '.images.playground_web' "${manifest}")"
genesis_hash="$(jq -er '.genesis_hash' "${manifest}")"
relayer_public="$(jq -er '.playground_relayer.public_key' "${manifest}")"
relayer_inspection="$("${artifacts}/minijam-node" key inspect --scheme Sr25519 "${MINIJAM_RELEASE_RELAYER_URI}")"
relayer_derived="$(sed -n 's/^Public key (hex):[[:space:]]*//p' <<<"${relayer_inspection}" | head -n1)"
[[ "${relayer_derived,,}" == "${relayer_public,,}" ]] || {
  echo "release Relayer URI does not match the public key in the chain spec" >&2
  exit 1
}

{
  printf 'MINIJAM_NODE_IMAGE=%s\n' "${node_image}"
  printf 'MINIJAM_WORKER_IMAGE=%s\n' "${worker_image}"
  printf 'MINIJAM_COMPILER_IMAGE=%s\n' "${compiler_image}"
  printf 'MINIJAM_PLAYGROUND_API_IMAGE=%s\n' "${playground_api_image}"
  printf 'MINIJAM_PLAYGROUND_WEB_IMAGE=%s\n' "${playground_web_image}"
  printf 'MINIJAM_CHAIN_SPEC_PATH=%s\n' "${raw}"
  printf 'MINIJAM_GENESIS_HASH=%s\n' "${genesis_hash}"
  printf 'MINIJAM_RELAYER_URI=%s\n' "${MINIJAM_RELEASE_RELAYER_URI}"
  printf 'NODE_KEY_OR_SEED_PATH=%s\n' "${scratch}/node-keystore.tar.gz"
  printf 'WORKER_1_SEED_PATH=%s\n' "${scratch}/worker-1.seed"
  printf 'WORKER_2_SEED_PATH=%s\n' "${scratch}/worker-2.seed"
  printf 'WORKER_3_SEED_PATH=%s\n' "${scratch}/worker-3.seed"
  printf 'MINIJAM_WEB_BIND=127.0.0.1\n'
  printf 'MINIJAM_WEB_PORT=4173\n'
} >"${env_file}"
chmod 600 "${env_file}"

docker logout ghcr.io >/dev/null 2>&1 || true
for image in \
  "${node_image}" \
  "${worker_image}" \
  "${compiler_image}" \
  "${playground_api_image}" \
  "${playground_web_image}"; do
  docker image rm --force "${image}" >/dev/null 2>&1 || true
done
"${compose[@]}" pull --policy always
"${compose[@]}" up --detach --no-build --pull always

deadline=$((SECONDS + ${MINIJAM_READY_TIMEOUT_SECONDS:-300}))
services=(node compiler-api playground-api worker-1 worker-2 worker-3 playground-web)
while ((SECONDS < deadline)); do
  pending=0
  for service in "${services[@]}"; do
    container="$("${compose[@]}" ps -q "${service}")"
    if [[ -z "${container}" ]]; then
      pending=1
      continue
    fi
    health="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${container}")"
    [[ "${health}" == "healthy" ]] || pending=1
  done
  ((pending != 0)) || break
  sleep 2
done

if ((pending != 0)); then
  mkdir -p "${diagnostics}"
  "${compose[@]}" ps --all >"${diagnostics}/compose-ps.txt" 2>&1 || true
  "${compose[@]}" logs --no-color >"${diagnostics}/compose.log" 2>&1 || true
  echo "Stage 0 release services did not become ready" >&2
  exit 1
fi

(
  cd "${repository}/apps/playground-web"
  MINIJAM_E2E_BASE_URL="http://127.0.0.1:4173" \
  MINIJAM_E2E_COMPOSE_FILE="${compose_file}" \
  MINIJAM_E2E_COMPOSE_PROJECT="${project}" \
  MINIJAM_STAGE0_ENV="${env_file}" \
  npm run test:stage0
)

echo "Public Stage 0 digest smoke flow passed"
