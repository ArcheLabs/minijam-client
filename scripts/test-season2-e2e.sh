#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_file="${MINIJAM_E2E_COMPOSE_FILE:-${repository}/deploy/season2/compose.compact.yml}"
compose_project="${MINIJAM_E2E_COMPOSE_PROJECT:-minijam-season2-e2e}"
pull_policy="${MINIJAM_E2E_PULL_POLICY:-always}"

if ! command -v docker >/dev/null 2>&1; then
  echo "Season 2 E2E requires Docker and Docker Compose" >&2
  exit 77
fi
if ! docker info >/dev/null 2>&1; then
  echo "Season 2 E2E requires a running Docker daemon" >&2
  exit 77
fi
if [[ -z "${MINIJAM_E2E_WALLET_SEED:-}" ]]; then
  echo "MINIJAM_E2E_WALLET_SEED must contain the 32-byte controller seed" >&2
  exit 2
fi
for required in \
  MINIJAM_RELAYER_URI \
  MINIJAM_ALLOCATION_RELAYER_URI \
  MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY \
  MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY \
  MINIJAM_WORKER_KEY_FILE; do
  if [[ -z "${!required:-}" ]]; then
    echo "${required} must be configured" >&2
    exit 2
  fi
done

compose=(docker compose --project-name "${compose_project}" -f "${compose_file}")
cleanup() {
  "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

"${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
"${compose[@]}" up --detach --pull "${pull_policy}"

deadline=$((SECONDS + ${MINIJAM_READY_TIMEOUT_SECONDS:-300}))
until curl --fail --silent http://127.0.0.1:8080/health/ready >/dev/null; do
  if (( SECONDS >= deadline )); then
    "${compose[@]}" ps
    "${compose[@]}" logs --no-color --tail=200 node worker api compiler
    exit 1
  fi
  sleep 2
done

wait_service_healthy() {
  local service="$1"
  local deadline=$((SECONDS + ${MINIJAM_READY_TIMEOUT_SECONDS:-300}))
  while (( SECONDS < deadline )); do
    local container health
    container="$("${compose[@]}" ps -q "${service}")"
    if [[ -n "${container}" ]]; then
      health="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${container}")"
      if [[ "${health}" == "healthy" ]]; then
        return 0
      fi
    fi
    sleep 2
  done
  echo "timed out waiting for ${service} to recover" >&2
  "${compose[@]}" ps
  return 1
}

(
  cd "${repository}/apps/playground-web"
  export MINIJAM_E2E_BASE_URL="${MINIJAM_E2E_BASE_URL:-http://127.0.0.1:4173}"
  export MINIJAM_E2E_COMPOSE_FILE="${compose_file}"
  export MINIJAM_E2E_COMPOSE_PROJECT="${compose_project}"
  npm run test:season2
)

# Clean-server recovery smoke: keep the data volumes and restart each
# operational dependency independently after the functional E2E.
for service in worker api node; do
  "${compose[@]}" restart "${service}"
  wait_service_healthy "${service}"
  until curl --fail --silent http://127.0.0.1:8080/health/ready >/dev/null; do
    sleep 2
  done
done
