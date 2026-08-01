#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
bundle="${1:?usage: $0 BUNDLE_DIRECTORY}"
env_file="${bundle}/release.env"
compose_file="${bundle}/compose.yml"
resolved="$(mktemp)"
trap 'rm -f "${resolved}"' EXIT
docker compose --env-file "${env_file}" -f "${compose_file}" config >"${resolved}"
! rg -q '^[[:space:]]*build:|stage0-raw\.json|deploy/stage0/secrets|(^|/)\.\.?/' "${resolved}"
test "$(rg -o 'ghcr\.io/archelabs/[a-z0-9-]+@sha256:[0-9a-f]{64}' "${env_file}" | wc -l)" -eq 6
rg -q '127\.0\.0\.1:9944:9944' "${resolved}"
rg -q '127\.0\.0\.1:4173:8080' "${resolved}"
rg -q 'name: minijam-local' "${resolved}"
rg -q -- '--dev' "${resolved}"
rg -q -- '--alice' "${resolved}"
rg -q -- '--key=//Alice' "${resolved}"
rg -q -- '--key=//Bob' "${resolved}"
rg -q -- '--key=//Charlie' "${resolved}"
rg -q '9292929292929292929292929292929292929292929292929292929292929292' "${resolved}"
