#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"

forbidden='clang|llvm|libclang|apt\.llvm\.org|llvm\.sh|deploy/compiler|compile-service|test-compiler-image|test-counter-services'
if rg -n -i "${forbidden}" "${WORKFLOW}"; then
  echo "Stage-1 core CI must not invoke the LLVM/compiler or generic C/C++ paths" >&2
  exit 1
fi

rg -q 'name: CI' "${WORKFLOW}"
rg -q 'Run committed Service 0 runtime tests' "${WORKFLOW}"
printf 'STAGE1_CORE_BOUNDARY=PASS\n'
printf 'STAGE1_CORE_REQUIRES_LLVM=false\n'
printf 'STAGE1_CORE_GATE=PASS\n'
