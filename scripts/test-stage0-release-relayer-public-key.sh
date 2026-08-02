#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
export MINIJAM_STAGE0_RELEASE_TEST_LIB=1
# shellcheck source=test-stage0-release.sh
source "${root}/scripts/test-stage0-release.sh"

public_key="0x4242424242424242424242424242424242424242424242424242424242424242"
other_public_key="0x4343434343434343434343434343434343434343434343434343434343434343"

inspection_with_prefix() {
  local prefix="$1"
  printf '%sSecret phrase:       -\n%sPublic key (hex):  %s\n' \
    "${prefix}" "${prefix}" "${public_key}"
}

assert_equal() {
  [[ "$1" == "$2" ]] || {
    echo "expected '$2', got '$1'" >&2
    exit 1
  }
}

assert_equal "$(parse_public_key_hex <<<"$(inspection_with_prefix '')")" "${public_key}"
assert_equal "$(parse_public_key_hex <<<"$(inspection_with_prefix '  ')")" "${public_key}"
assert_equal "$(parse_public_key_hex <<<"$(inspection_with_prefix '       ')")" "${public_key}"

if error="$(verify_relayer_public_key 'Secret phrase: -' "${public_key}" 2>&1)"; then
  echo "missing public-key line unexpectedly passed" >&2
  exit 1
fi
assert_equal "${error}" "failed to parse Relayer public key from minijam-node key inspect output"

verify_relayer_public_key "$(inspection_with_prefix '  ')" "${public_key^^}"

if error="$(verify_relayer_public_key "$(inspection_with_prefix ' ')" "${other_public_key}" 2>&1)"; then
  echo "mismatched public keys unexpectedly passed" >&2
  exit 1
fi
[[ "${error}" == *"release Relayer URI does not match the public key in the chain spec"* ]] || {
  echo "missing mismatch error" >&2
  exit 1
}
[[ "${error}" == *"expected public key: ${other_public_key}"* ]] || {
  echo "missing expected public key" >&2
  exit 1
}
[[ "${error}" == *"derived public key:  ${public_key}"* ]] || {
  echo "missing derived public key" >&2
  exit 1
}

echo "Stage 0 Relayer public-key parsing tests passed"
