#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
REPRODUCED_DIR="${MINIJAM_REPRODUCED_ARTIFACT_DIR:-${ROOT}/target/minijam-compat-artifacts}"
MANIFEST="${REPRODUCED_DIR}/manifest.tsv"
MANIFEST_PATH="${ROOT}/external/jambda/crates/minijam-executive/Cargo.toml"
PATCH_PROTOCOL="patch.crates-io.minijam-protocol.path='${ROOT}/crates/minijam-protocol'"
PATCH_JAMCORE="patch.crates-io.minijam-jamcore-api.path='${ROOT}/crates/minijam-jamcore-api'"

printf 'MINIJAM_SHA=%s\n' "$(git rev-parse HEAD)"
printf 'JAMBDA_SHA=%s\n' "$(git -C "${ROOT}/external/jambda" rev-parse HEAD)"
printf 'COMPILER_SHA=%s\n' "$(git rev-parse HEAD:service-toolchain/compiler/toolchain.lock)"
printf 'COMPILER_TOOLCHAIN_SHA256=%s\n' "$(sha256sum "${ROOT}/service-toolchain/compiler/toolchain.lock" | awk '{print $1}')"
printf 'REPRODUCED_ARTIFACT_MANIFEST=%s\n' "${MANIFEST}"

test -s "${MANIFEST}" || {
  echo "error: reproduced artifact manifest is missing; run scripts/test-compiler-image.sh first" >&2
  exit 1
}

for language in c cpp; do
  reproduced="${REPRODUCED_DIR}/${language}/service.blob"
  executed="${ROOT}/examples/services/counter/artifacts/counter-${language}.blob"
  test -f "${reproduced}" || {
    echo "error: reproduced ${language} artifact is missing: ${reproduced}" >&2
    exit 1
  }
  test -f "${executed}" || {
    echo "error: executed ${language} artifact is missing: ${executed}" >&2
    exit 1
  }
  reproduced_sha256="$(sha256sum "${reproduced}" | awk '{print $1}')"
  executed_sha256="$(sha256sum "${executed}" | awk '{print $1}')"
  reproduced_size="$(stat -c '%s' "${reproduced}")"
  executed_size="$(stat -c '%s' "${executed}")"
  printf 'ARTIFACT_LANGUAGE=%s\n' "${language}"
  printf 'REPRODUCED_ARTIFACT_PATH=%s\n' "${reproduced}"
  printf 'REPRODUCED_ARTIFACT_SHA256=%s\n' "${reproduced_sha256}"
  printf 'REPRODUCED_ARTIFACT_SIZE=%s\n' "${reproduced_size}"
  printf 'EXECUTED_ARTIFACT_PATH=%s\n' "${executed}"
  printf 'EXECUTED_ARTIFACT_SHA256=%s\n' "${executed_sha256}"
  printf 'EXECUTED_ARTIFACT_SIZE=%s\n' "${executed_size}"
  cmp "${reproduced}" "${executed}"
done

printf 'EXECUTED_ARTIFACT_IDENTITY=PASS\n'

printf 'EXEC_COMMAND=cargo test --manifest-path %s --config %s --config %s counter_ -- --nocapture\n' \
  "${MANIFEST_PATH}" "${PATCH_PROTOCOL}" "${PATCH_JAMCORE}"

cargo test \
  --manifest-path "${MANIFEST_PATH}" \
  --config "${PATCH_PROTOCOL}" \
  --config "${PATCH_JAMCORE}" \
  counter_ -- --nocapture

printf 'JAMBDA_EXECUTION=PASS\n'
