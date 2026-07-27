#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SOURCE_DIR="${ROOT}/examples/services/counter"
OUT_DIR="${SOURCE_DIR}/artifacts"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT

for language in c cpp; do
  source="${SOURCE_DIR}/service.${language/cpp/cpp}"
  [[ "${language}" == c ]] && source="${SOURCE_DIR}/service.c"
  "${ROOT}/scripts/compile-service" "${language}" "${source}" "${TMP}/${language}" Os
  install -m 0644 "${TMP}/${language}/service.blob" "${TMP}/counter-${language}.blob"
  install -m 0644 "${TMP}/${language}/service.polkavm" "${TMP}/counter-${language}.polkavm"
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
