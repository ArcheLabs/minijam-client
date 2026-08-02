#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
fixture="${root}/scripts/fixtures/local-image-compose"
scratch="$(mktemp -d)"

trap 'rm -rf "${scratch}"' EXIT

run_fixture() {
  local name="$1"
  local tmpfs="$2"
  local bundle="${scratch}/${name}"

  cp -R "${fixture}" "${bundle}"
  jq --argjson tmpfs "${tmpfs}" \
    '.services["compiler-api"].tmpfs = $tmpfs' \
    "${bundle}/normalized-compose.json" >"${bundle}/normalized-compose.json.next"
  mv "${bundle}/normalized-compose.json.next" "${bundle}/normalized-compose.json"
  PATH="${bundle}:${PATH}" "${root}/scripts/check-local-image-compose.sh" "${bundle}"
}

assert_invalid_tmpfs() {
  local name="$1"
  local tmpfs="$2"
  local output

  if output="$(run_fixture "${name}" "${tmpfs}" 2>&1)"; then
    echo "invalid compiler tmpfs unexpectedly passed: ${tmpfs}" >&2
    exit 1
  fi
  [[ "${output}" == *"Compiler API tmpfs"* ]] || {
    echo "invalid compiler tmpfs did not report its policy error" >&2
    exit 1
  }
}

PATH="${fixture}:${PATH}" "${root}/scripts/check-local-image-compose.sh" "${fixture}"

assert_invalid_tmpfs split-options '["/tmp:size=256m", "mode=1777"]'
assert_invalid_tmpfs missing-options '["/tmp"]'
assert_invalid_tmpfs extra-mount '["/tmp:size=256m,mode=1777", "/cache:size=64m"]'

echo "Compiler API tmpfs regression tests passed."
