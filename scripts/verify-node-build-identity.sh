#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${1:-${ROOT}/target/release/minijam-node}"
EXPECTED_REF="${EXPECTED_MINIJAM_REF:-$(git -C "${ROOT}" rev-parse HEAD)}"

source_ref="$(git -C "${ROOT}" rev-parse HEAD)"
[[ "${source_ref}" == "${EXPECTED_REF}" ]] || {
  echo "source ref mismatch: expected ${EXPECTED_REF}, got ${source_ref}" >&2
  exit 1
}

jambda_ref="$(git -C "${ROOT}/external/jambda" rev-parse HEAD)"
[[ -n "${jambda_ref}" ]] || exit 1
[[ -x "${BIN}" ]] || { echo "node binary not executable: ${BIN}" >&2; exit 1; }

version="$(${BIN} --version)"
[[ "${version}" == *"${source_ref:0:11}"* ]] || {
  echo "binary identity mismatch: ${version} (expected ${source_ref:0:11})" >&2
  exit 1
}

sha256sum "${BIN}"
printf 'source_ref=%s\njambda_ref=%s\nbinary=%s\nreported_version=%s\nruntime_spec_version=1\ntransaction_version=1\n' \
  "${source_ref}" "${jambda_ref}" "${BIN}" "${version}"
