#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT

cp "${ROOT}/.github/workflows/ci.yml" "${TMP}/ci.yml"
printf '\n# forbidden core ownership leak: llvm-config\n' >> "${TMP}/ci.yml"

if MINIJAM_STAGE1_CORE_WORKFLOW="${TMP}/ci.yml" \
  "${ROOT}/scripts/check-stage1-core-boundary.sh"; then
  echo "Stage-1 core boundary accepted an injected LLVM dependency" >&2
  exit 1
fi

printf 'STAGE1_CORE_BOUNDARY_NEGATIVE=PASS\n'
