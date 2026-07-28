#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_file="${repository}/deploy/local/docker-compose.yml"
env_file="${MINIJAM_STAGE0_ENV:-${repository}/deploy/local/.env}"
project="minijam-stage0"
export MINIJAM_IMAGE_REGISTRY="${MINIJAM_IMAGE_REGISTRY:-ghcr.io/archelabs}"
export MINIJAM_IMAGE_TAG="${MINIJAM_IMAGE_TAG:-$(git -C "${repository}" rev-parse HEAD)}"

compose=(docker compose --project-name "${project}")
if [[ -f "${env_file}" ]]; then
  compose+=(--env-file "${env_file}")
fi
compose+=(-f "${compose_file}")

diagnostics() {
  echo "Stage 0 Compose status:" >&2
  "${compose[@]}" ps >&2 || true
  echo "Stage 0 recent logs:" >&2
  "${compose[@]}" logs --no-color --tail=200 node compiler-api playground-api worker-1 worker-2 worker-3 playground-web >&2 || true
}

wait_ready() {
  local deadline=$((SECONDS + ${MINIJAM_READY_TIMEOUT_SECONDS:-300}))
  local services=(node compiler-api playground-api worker-1 worker-2 worker-3 playground-web)
  while (( SECONDS < deadline )); do
    local pending=0
    for service in "${services[@]}"; do
      local container health
      container="$("${compose[@]}" ps -q "${service}")"
      if [[ -z "${container}" ]]; then
        pending=1
        continue
      fi
      health="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${container}")"
      if [[ "${health}" != "healthy" ]]; then
        pending=1
      fi
    done
    if (( pending == 0 )); then
      echo "all Stage 0 services are ready"
      return 0
    fi
    sleep 2
  done
  diagnostics
  return 1
}

command="${1:-}"
case "${command}" in
  pull)
    "${compose[@]}" pull --policy always
    ;;
  build)
    "${compose[@]}" build
    ;;
  up)
    trap diagnostics ERR
    "${compose[@]}" up --detach --no-build --pull never
    wait_ready
    ;;
  down)
    "${compose[@]}" down
    ;;
  reset)
    "${compose[@]}" down --volumes --remove-orphans
    ;;
  test)
    trap diagnostics ERR
    wait_ready
    web_address="$("${compose[@]}" port playground-web 8080)"
    web_port="${web_address##*:}"
    (
      cd "${repository}/apps/playground-web"
      export MINIJAM_E2E_BASE_URL="http://127.0.0.1:${web_port}"
      if [[ -f "${env_file}" ]]; then
        export MINIJAM_STAGE0_ENV="${env_file}"
      fi
      npm run test:stage0
    )
    ;;
  *)
    echo "usage: $0 {pull|build|up|down|reset|test}" >&2
    exit 2
    ;;
esac
