#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
STAGE1_DOCKERFILE="${ROOT}/deploy/stage1/Dockerfile"
COMPILER_DOCKERFILE="${ROOT}/deploy/compiler/Dockerfile"
toolchain='clang|clang\+\+|llvm|libclang|apt\.llvm\.org|llvm\.sh|protobuf-compiler|cargo|rustc'

extract_stage() {
  local dockerfile="$1"
  local stage="$2"
  awk -v stage="${stage}" '
    /^FROM / {
      capture = ($0 ~ " AS " stage "([[:space:]]|$)")
    }
    capture { print }
  ' "${dockerfile}"
}

for stage in node worker formal-rpc; do
  if extract_stage "${STAGE1_DOCKERFILE}" "${stage}" | grep -Eni "${toolchain}"; then
    echo "Stage-1 production target ${stage} contains compiler-toolchain material" >&2
    exit 1
  fi
done

if grep -En 'apt\.llvm\.org|llvm\.sh' "${COMPILER_DOCKERFILE}" "${STAGE1_DOCKERFILE}"; then
  echo "production compiler image uses a mutable LLVM installer" >&2
  exit 1
fi
grep -Eq '^[[:space:]]*FROM silkeh/clang:20-bullseye@sha256:302c1c6d5cfd72ee154696a11e097bcc3a20e7060dac9add9a75dfbe8319947b$' "${COMPILER_DOCKERFILE}"

printf 'PRODUCTION_IMAGE_TOOLCHAIN_BOUNDARY=PASS\n'

grep -Eq '^[[:space:]]*FROM rust:1\.88-bookworm@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 AS builder$' "${STAGE1_DOCKERFILE}"
grep -Eq '^[[:space:]]*FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818 AS runtime$' "${STAGE1_DOCKERFILE}"
grep -Fq 'COPY --from=builder /out/minijam-node /usr/local/bin/minijam-node' "${STAGE1_DOCKERFILE}"
grep -Fq 'COPY --from=builder /out/minijam-worker /usr/local/bin/minijam-worker' "${STAGE1_DOCKERFILE}"
grep -Fq 'COPY --from=builder /out/minijam-formal-rpc /usr/local/bin/minijam-formal-rpc' "${STAGE1_DOCKERFILE}"
grep -Fq 'cargo build --locked --release' "${STAGE1_DOCKERFILE}"
grep -Fq -- '-p minijam-node' "${STAGE1_DOCKERFILE}"
grep -Fq -- '-p minijam-worker' "${STAGE1_DOCKERFILE}"
grep -Fq -- '-p minijam-formal-rpc' "${STAGE1_DOCKERFILE}"
if grep -Eni 'compiler-api|jam.?computer|minicells|faucet' "${STAGE1_DOCKERFILE}"; then
  echo 'Stage-1 Dockerfile contains a forbidden product role' >&2
  exit 1
fi
grep -Fq 'ENTRYPOINT ["minijam-node"]' "${STAGE1_DOCKERFILE}"
grep -Fq 'ENTRYPOINT ["minijam-worker"]' "${STAGE1_DOCKERFILE}"
grep -Fq 'ENTRYPOINT ["minijam-formal-rpc"]' "${STAGE1_DOCKERFILE}"
grep -Fq 'USER minijam' "${STAGE1_DOCKERFILE}"

printf 'STAGE1_PRODUCTION_IMAGE_BOUNDARY=PASS\n'
