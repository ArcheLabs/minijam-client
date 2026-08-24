#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SRC="${ROOT}/services/system-service/src/service.c"
OUT_BLOB="${ROOT}/artifacts/system-service.blob"
OUT_PVM="${ROOT}/artifacts/system-service.polkavm"
OUT_MANIFEST="${ROOT}/artifacts/system-service.manifest.json"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT

"${ROOT}/scripts/compile-service" c "${SRC}" "${TMP}/compiled" Os
install -m 0644 "${TMP}/compiled/service.blob" "${TMP}/system-service.blob"
install -m 0644 "${TMP}/compiled/service.polkavm" "${TMP}/system-service.polkavm"

install -d "${ROOT}/artifacts"
install -m 0644 "${TMP}/system-service.blob" "${OUT_BLOB}"
install -m 0644 "${TMP}/system-service.polkavm" "${OUT_PVM}"

BYTE_LEN="$(wc -c < "${OUT_BLOB}" | tr -d ' ')"
SHA256="$(sha256sum "${OUT_BLOB}" | cut -d ' ' -f 1)"

cat > "${OUT_MANIFEST}" <<JSON
{
  "name": "minijam-system-service",
  "abi_version": 2,
  "artifact": "system-service.blob",
  "debug_artifact": "system-service.polkavm",
  "byte_len": ${BYTE_LEN},
  "sha256": "${SHA256}",
  "source": "services/system-service/src/service.c",
  "toolchain": "service-toolchain/compiler/toolchain.lock",
  "stage": 0,
  "note": "Ownerless SystemOpV2 Service 0; CreateService and ApplyAllocation are explicit commands."
}
JSON

printf 'build-system-service: wrote %s, %s, and %s\n' \
  "${OUT_BLOB}" "${OUT_PVM}" "${OUT_MANIFEST}"
