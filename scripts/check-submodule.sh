#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SUBMODULE_PATH="external/jambda"
SUBMODULE_ABS="${ROOT}/${SUBMODULE_PATH}"

fail() {
  printf 'check-submodule: %s\n' "$*" >&2
  exit 1
}

expected_sha="$(
  git -C "${ROOT}" ls-tree HEAD "${SUBMODULE_PATH}" | awk '{print $3}'
)"

if [[ -z "${expected_sha}" ]]; then
  fail "${SUBMODULE_PATH} is not recorded in the superproject tree"
fi

if [[ ! -d "${SUBMODULE_ABS}/.git" && ! -f "${SUBMODULE_ABS}/.git" ]]; then
  fail "${SUBMODULE_PATH} is not initialized; run git submodule update --init --recursive"
fi

actual_sha="$(git -C "${SUBMODULE_ABS}" rev-parse HEAD)"
if [[ "${actual_sha}" != "${expected_sha}" ]]; then
  fail "${SUBMODULE_PATH} HEAD ${actual_sha} does not match superproject ${expected_sha}"
fi

if [[ ! -d "${SUBMODULE_ABS}/crates/minijam-executive" ]]; then
  fail "${SUBMODULE_PATH}/crates/minijam-executive is missing"
fi

if [[ -n "$(git -C "${SUBMODULE_ABS}" status --porcelain)" ]]; then
  fail "${SUBMODULE_PATH} has uncommitted changes"
fi

printf 'check-submodule: %s pinned at %s\n' "${SUBMODULE_PATH}" "${actual_sha}"
