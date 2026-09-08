#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
WORKFLOW="${MINIJAM_STAGE1_CORE_WORKFLOW:-${ROOT}/.github/workflows/ci.yml}"

forbidden='clang|llvm|libclang|apt\.llvm\.org|llvm\.sh|deploy/compiler|compile-service|test-compiler-image|test-counter-services|cargo test --workspace|cargo build --release -p minijam-node|cargo build --release -p minijam-worker'
if grep -Eni "${forbidden}" "${WORKFLOW}"; then
  echo "Stage-1 core CI must not invoke the LLVM/compiler or generic C/C++ paths" >&2
  exit 1
fi

grep -Eq 'name: CI' "${WORKFLOW}"
grep -Fq 'Run committed Service 0 runtime tests' "${WORKFLOW}"
printf 'STAGE1_CORE_BOUNDARY=PASS\n'
printf 'STAGE1_CORE_REQUIRES_LLVM=false\n'
printf 'STAGE1_CORE_GATE=PASS\n'
