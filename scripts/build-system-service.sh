#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SRC="${ROOT}/services/system-service/src/service.c"
INCLUDE="${ROOT}/service-toolchain/sdk/include"
SDK_SRC="${ROOT}/service-toolchain/sdk/src"
CONVERTER="${ROOT}/service-toolchain/compiler/polkavm-to-jam/Cargo.toml"
OUT_BLOB="${ROOT}/artifacts/system-service.blob"
OUT_PVM="${ROOT}/artifacts/system-service.polkavm"
OUT_MANIFEST="${ROOT}/artifacts/system-service.manifest.json"
LLVM_CLANG="${MINIJAM_CLANG:-/usr/lib/llvm-20/bin/clang}"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT

if [[ ! -x "${LLVM_CLANG}" ]]; then
  printf 'build-system-service: LLVM 20 clang not found: %s\n' "${LLVM_CLANG}" >&2
  exit 1
fi

FLAGS=(
  --target=riscv64-unknown-elf
  -march=rv64emac
  -mabi=lp64e
  -std=c11
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

"${LLVM_CLANG}" "${FLAGS[@]}" -c "${SDK_SRC}/host.c" -o "${TMP}/host.o"
"${LLVM_CLANG}" "${FLAGS[@]}" -c "${SDK_SRC}/minijam.c" -o "${TMP}/minijam.o"
"${LLVM_CLANG}" "${FLAGS[@]}" -c "${SRC}" -o "${TMP}/service.o"
"${LLVM_CLANG}" --target=riscv64-unknown-elf -march=rv64emac -mabi=lp64e \
  -nostdlib -Wl,--gc-sections -Wl,--emit-relocs -Wl,-e,minijam_refine \
  -Wl,-u,minijam_accumulate \
  "${TMP}/host.o" "${TMP}/minijam.o" "${TMP}/service.o" \
  -o "${TMP}/system-service.elf"

cargo run --quiet --locked --release --manifest-path "${CONVERTER}" -- \
  "${TMP}/system-service.elf" "${TMP}/system-service.blob" \
  "${TMP}/system-service.polkavm"

install -d "${ROOT}/artifacts"
install -m 0644 "${TMP}/system-service.blob" "${OUT_BLOB}"
install -m 0644 "${TMP}/system-service.polkavm" "${OUT_PVM}"

BYTE_LEN="$(wc -c < "${OUT_BLOB}" | tr -d ' ')"
SHA256="$(sha256sum "${OUT_BLOB}" | cut -d ' ' -f 1)"

cat > "${OUT_MANIFEST}" <<JSON
{
  "name": "minijam-system-service",
  "abi_version": 1,
  "artifact": "system-service.blob",
  "debug_artifact": "system-service.polkavm",
  "byte_len": ${BYTE_LEN},
  "sha256": "${SHA256}",
  "source": "services/system-service/src/service.c",
  "toolchain": "service-toolchain/compiler/toolchain.lock",
  "stage": 0,
  "note": "CreateService executes in the Service 0 PVM; UpgradeService remains a documented Stage 0 native deviation."
}
JSON

printf 'build-system-service: wrote %s, %s, and %s\n' \
  "${OUT_BLOB}" "${OUT_PVM}" "${OUT_MANIFEST}"
