#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SRC="${ROOT}/services/system-service/src/system-service.placeholder"
OUT_BLOB="${ROOT}/artifacts/system-service.blob"
OUT_MANIFEST="${ROOT}/artifacts/system-service.manifest.json"

if [[ ! -f "${SRC}" ]]; then
  printf 'build-system-service: missing source: %s\n' "${SRC}" >&2
  exit 1
fi

install -d "${ROOT}/artifacts"
install -m 0644 "${SRC}" "${OUT_BLOB}"

BYTE_LEN="$(wc -c < "${OUT_BLOB}" | tr -d ' ')"
SHA256="$(sha256sum "${OUT_BLOB}" | cut -d ' ' -f 1)"

cat > "${OUT_MANIFEST}" <<JSON
{
  "name": "minijam-system-service",
  "abi_version": 1,
  "artifact": "system-service.blob",
  "byte_len": ${BYTE_LEN},
  "sha256": "${SHA256}",
  "source": "services/system-service/src/system-service.placeholder",
  "stage": 0,
  "note": "Stage 0 placeholder artifact until the system-service PVM compiler pipeline is wired."
}
JSON

printf 'build-system-service: wrote %s and %s\n' "${OUT_BLOB}" "${OUT_MANIFEST}"
