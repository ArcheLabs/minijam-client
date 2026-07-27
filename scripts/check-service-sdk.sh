#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT

INCLUDE="${ROOT}/service-toolchain/sdk/include"
CFLAGS=(-Wall -Wextra -Werror -I "${INCLUDE}")

clang -std=c11 "${CFLAGS[@]}" -DMINIJAM_HOST_TEST \
  -c "${ROOT}/service-toolchain/sdk/src/host.c" -o "${TMP}/host.o"
clang -std=c11 "${CFLAGS[@]}" -DMINIJAM_HOST_TEST \
  -c "${ROOT}/service-toolchain/sdk/src/minijam.c" -o "${TMP}/minijam.o"
clang -std=c11 "${CFLAGS[@]}" \
  -c "${ROOT}/service-toolchain/sdk/tests/host_stub.c" -o "${TMP}/stub.o"
clang -std=c11 "${CFLAGS[@]}" \
  -c "${ROOT}/examples/services/counter/service.c" -o "${TMP}/counter-c.o"
clang++ -std=c++17 "${CFLAGS[@]}" -fno-exceptions -fno-rtti \
  -fno-threadsafe-statics \
  -c "${ROOT}/examples/services/counter/service.cpp" -o "${TMP}/counter-cpp.o"

clang -nostdlib -Wl,-e,minijam_refine \
  "${TMP}/minijam.o" "${TMP}/stub.o" "${TMP}/counter-c.o" \
  -o "${TMP}/counter-c"
clang++ -nostdlib -Wl,-e,minijam_refine \
  "${TMP}/minijam.o" "${TMP}/stub.o" "${TMP}/counter-cpp.o" \
  -o "${TMP}/counter-cpp"

test -s "${TMP}/counter-c"
test -s "${TMP}/counter-cpp"

LLVM_CLANG="${MINIJAM_CLANG:-/usr/lib/llvm-20/bin/clang}"
CONVERTER_MANIFEST="${ROOT}/service-toolchain/compiler/polkavm-to-jam/Cargo.toml"
if [[ -x "${LLVM_CLANG}" ]] && command -v cargo >/dev/null 2>&1; then
  GUEST_FLAGS=(
    --target=riscv64-unknown-elf
    -march=rv64emac
    -mabi=lp64e
    -ffreestanding
    -fno-builtin
    -fdata-sections
    -ffunction-sections
    -Os
    -Wall
    -Wextra
    -Werror
    -I "${INCLUDE}"
  )
  "${LLVM_CLANG}" -std=c11 "${GUEST_FLAGS[@]}" \
    -c "${ROOT}/service-toolchain/sdk/src/host.c" -o "${TMP}/guest-host.o"
  "${LLVM_CLANG}" -std=c11 "${GUEST_FLAGS[@]}" \
    -c "${ROOT}/service-toolchain/sdk/src/minijam.c" -o "${TMP}/guest-minijam.o"
  "${LLVM_CLANG}" -std=c11 "${GUEST_FLAGS[@]}" \
    -c "${ROOT}/examples/services/counter/service.c" -o "${TMP}/guest-counter.o"
  "${LLVM_CLANG}" --target=riscv64-unknown-elf -march=rv64emac -mabi=lp64e \
    -nostdlib -Wl,--gc-sections -Wl,--emit-relocs -Wl,-e,minijam_refine \
    -Wl,-u,minijam_accumulate \
    "${TMP}/guest-host.o" "${TMP}/guest-minijam.o" "${TMP}/guest-counter.o" \
    -o "${TMP}/counter.elf"
  cargo run --quiet --locked --release --manifest-path "${CONVERTER_MANIFEST}" -- \
    "${TMP}/counter.elf" "${TMP}/counter.blob" "${TMP}/counter.polkavm"
  test -s "${TMP}/counter.blob"
  test -s "${TMP}/counter.polkavm"
  printf 'service SDK native, PolkaVM, and JAM blob smoke passed\n'
else
  printf 'service SDK native smoke passed (guest toolchain unavailable)\n'
fi
