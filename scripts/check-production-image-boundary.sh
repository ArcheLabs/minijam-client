#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
DOCKERFILE="${ROOT}/deploy/local/Dockerfile"
toolchain='clang|clang\+\+|llvm|libclang|apt\.llvm\.org|llvm\.sh'

extract_stage() {
  local stage="$1"
  awk -v stage="${stage}" '
    /^FROM / {
      capture = ($0 ~ " AS " stage "([[:space:]]|$)")
    }
    capture { print }
  ' "${DOCKERFILE}"
}

for stage in rust-runtime node playground playground-api-release worker compiler-binary; do
  if extract_stage "${stage}" | rg -n -i "${toolchain}"; then
    echo "production target ${stage} contains compiler-toolchain material" >&2
    exit 1
  fi
done

extract_stage compiler | rg -q 'silkeh/clang:20-bullseye@sha256:302c1c6d5cfd72ee154696a11e097bcc3a20e7060dac9add9a75dfbe8319947b'
if rg -n 'apt\.llvm\.org|llvm\.sh' "${ROOT}/deploy/compiler/Dockerfile" "${DOCKERFILE}"; then
  echo "production compiler image uses a mutable LLVM installer" >&2
  exit 1
fi

printf 'PRODUCTION_IMAGE_TOOLCHAIN_BOUNDARY=PASS\n'
