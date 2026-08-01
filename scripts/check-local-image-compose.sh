#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
bundle="${1:?usage: $0 BUNDLE_DIRECTORY}"
env_file="${bundle}/release.env"
compose_file="${bundle}/compose.yml"
resolved="$(mktemp)"
trap 'rm -f "${resolved}"' EXIT
docker compose --env-file "${env_file}" -f "${compose_file}" config >"${resolved}"
! grep -Eq '^[[:space:]]*build:|stage0-raw\.json|deploy/stage0/secrets|(^|/)\.\.?/' "${resolved}"
test "$(grep -Eo 'ghcr\.io/archelabs/[a-z0-9-]+@sha256:[0-9a-f]{64}' "${env_file}" | wc -l)" -eq 6
grep -Eq '127\.0\.0\.1:9944:9944' "${resolved}"
grep -Eq '127\.0\.0\.1:4173:8080' "${resolved}"
grep -Eq 'name: minijam-local' "${resolved}"
grep -Eq -- '--dev' "${resolved}"
grep -Eq -- '--alice' "${resolved}"
grep -Eq -- '--key=//Alice' "${resolved}"
grep -Eq -- '--key=//Bob' "${resolved}"
grep -Eq -- '--key=//Charlie' "${resolved}"
grep -Eq '9292929292929292929292929292929292929292929292929292929292929292' "${resolved}"
