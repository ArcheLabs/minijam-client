#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SOURCE_DIR="${ROOT}/examples/services/counter"
OUT_DIR="${SOURCE_DIR}/artifacts"
INCLUDE="${ROOT}/service-toolchain/sdk/include"
SDK_SRC="${ROOT}/service-toolchain/sdk/src"
CONVERTER="${ROOT}/service-toolchain/compiler/polkavm-to-jam/Cargo.toml"
LLVM_CLANG="${MINIJAM_CLANG:-/usr/lib/llvm-20/bin/clang}"
LLVM_CLANGXX="${MINIJAM_CLANGXX:-/usr/lib/llvm-20/bin/clang++}"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT

for compiler in "${LLVM_CLANG}" "${LLVM_CLANGXX}"; do
  if [[ ! -x "${compiler}" ]]; then
    printf 'build-counter-services: compiler not found: %s\n' "${compiler}" >&2
    exit 1
  fi
done

COMMON_FLAGS=(
  --target=riscv64-unknown-elf
  -march=rv64emac
  -mabi=lp64e
  -ffreestanding
  -fno-builtin
  -fdata-sections
  -ffunction-sections
  -ffile-prefix-map="${ROOT}"=.
  -fdebug-prefix-map="${ROOT}"=.
  -g0
  -Os
  -Wall
  -Wextra
  -Werror
  -I "${INCLUDE}"
)

"${LLVM_CLANG}" -std=c11 "${COMMON_FLAGS[@]}" \
  -c "${SDK_SRC}/host.c" -o "${TMP}/host.o"
"${LLVM_CLANG}" -std=c11 "${COMMON_FLAGS[@]}" \
  -c "${SDK_SRC}/minijam.c" -o "${TMP}/minijam.o"
"${LLVM_CLANG}" -std=c11 "${COMMON_FLAGS[@]}" \
  -c "${SDK_SRC}/crypto.c" -o "${TMP}/crypto.o"
"${LLVM_CLANG}" -std=c11 "${COMMON_FLAGS[@]}" \
  -c "${SOURCE_DIR}/service.c" -o "${TMP}/counter-c.o"
"${LLVM_CLANGXX}" -std=c++17 "${COMMON_FLAGS[@]}" -fno-exceptions -fno-rtti \
  -fno-threadsafe-statics -fno-use-cxa-atexit \
  -c "${SOURCE_DIR}/service.cpp" -o "${TMP}/counter-cpp.o"

for language in c cpp; do
  "${LLVM_CLANG}" --target=riscv64-unknown-elf -march=rv64emac -mabi=lp64e \
    -nostdlib -Wl,--build-id=none -Wl,--gc-sections -Wl,--emit-relocs \
    -Wl,-e,minijam_refine -Wl,-u,minijam_accumulate \
    "${TMP}/host.o" "${TMP}/minijam.o" "${TMP}/crypto.o" \
    "${TMP}/counter-${language}.o" -o "${TMP}/counter-${language}.elf"
  cargo run --quiet --locked --release --manifest-path "${CONVERTER}" -- \
    "${TMP}/counter-${language}.elf" "${TMP}/counter-${language}.blob" \
    "${TMP}/counter-${language}.polkavm"
done

install -d "${OUT_DIR}"
for artifact in counter-c.blob counter-c.polkavm counter-cpp.blob counter-cpp.polkavm; do
  install -m 0644 "${TMP}/${artifact}" "${OUT_DIR}/${artifact}"
done

C_SIZE="$(wc -c < "${OUT_DIR}/counter-c.blob" | tr -d ' ')"
CPP_SIZE="$(wc -c < "${OUT_DIR}/counter-cpp.blob" | tr -d ' ')"
C_SHA="$(sha256sum "${OUT_DIR}/counter-c.blob" | cut -d ' ' -f 1)"
CPP_SHA="$(sha256sum "${OUT_DIR}/counter-cpp.blob" | cut -d ' ' -f 1)"
cat > "${OUT_DIR}/manifest.json" <<JSON
{
  "name": "minijam-counter-services",
  "abi_version": 1,
  "toolchain": "service-toolchain/compiler/toolchain.lock",
  "artifacts": {
    "c": {
      "source": "examples/services/counter/service.c",
      "blob": "counter-c.blob",
      "polkavm": "counter-c.polkavm",
      "byte_len": ${C_SIZE},
      "sha256": "${C_SHA}"
    },
    "cpp": {
      "source": "examples/services/counter/service.cpp",
      "blob": "counter-cpp.blob",
      "polkavm": "counter-cpp.polkavm",
      "byte_len": ${CPP_SIZE},
      "sha256": "${CPP_SHA}"
    }
  }
}
JSON

printf 'build-counter-services: wrote reproducible C and C++ artifacts to %s\n' \
  "${OUT_DIR}"
