#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
IMAGE="${MINIJAM_COMPILER_IMAGE:-minijam-compiler:service0-ci}"
OUT_DIR="${MINIJAM_SERVICE0_PROVENANCE_DIR:-${ROOT}/target/minijam-service0-provenance}"
MANIFEST="${ROOT}/artifacts/system-service.manifest.json"
SOURCE="${ROOT}/services/system-service/src/service.c"
EXPECTED_BLOB="${ROOT}/artifacts/system-service.blob"
EXPECTED_PVM="${ROOT}/artifacts/system-service.polkavm"

command -v docker >/dev/null 2>&1 || {
  echo "error: Docker is required for Service 0 provenance" >&2
  exit 1
}
command -v jq >/dev/null 2>&1 || {
  echo "error: jq is required for Service 0 manifest validation" >&2
  exit 1
}

install -d -m 0777 "${OUT_DIR}"
rm -f "${OUT_DIR}/service.blob" "${OUT_DIR}/service.polkavm" "${OUT_DIR}/toolchain-diagnostics.txt"

printf 'MINIJAM_SHA=%s\n' "$(git rev-parse HEAD)"
printf 'SERVICE0_SOURCE=C\n'
printf 'SERVICE0_SOURCE_MIME=%s\n' "$(file -b --mime-type "${SOURCE}")"
printf 'SERVICE0_SOURCE_SHA256=%s\n' "$(sha256sum "${SOURCE}" | awk '{print $1}')"
printf 'SERVICE0_COMMITTED_BLOB_SHA256=%s\n' "$(sha256sum "${EXPECTED_BLOB}" | awk '{print $1}')"
printf 'SERVICE0_MANIFEST_SHA256=%s\n' "$(sha256sum "${MANIFEST}" | awk '{print $1}')"
printf 'COMPILER_SHA=%s\n' "$(git rev-parse HEAD:service-toolchain/compiler/toolchain.lock)"
printf 'COMPILER_TOOLCHAIN_SHA256=%s\n' "$(sha256sum "${ROOT}/service-toolchain/compiler/toolchain.lock" | awk '{print $1}')"
printf 'COMPILER_IMAGE=%s\n' "${IMAGE}"

docker build -f "${ROOT}/deploy/compiler/Dockerfile" -t "${IMAGE}" "${ROOT}"

docker run --rm --network=none --read-only --user=65532:65532 \
  --cpus=1 --memory=512m --pids-limit=64 --cap-drop=ALL \
  --security-opt=no-new-privileges \
  --mount "type=bind,src=${ROOT},dst=/workspace,readonly" \
  --mount "type=bind,src=${OUT_DIR},dst=/output" \
  --tmpfs /tmp:rw,noexec,nosuid,size=64m \
  --env MINIJAM_REPOSITORY=/workspace \
  --env MINIJAM_CONVERTER_BIN=/usr/local/bin/polkavm-to-jam \
  "${IMAGE}" /workspace/scripts/compile-service c \
  /workspace/services/system-service/src/service.c /output Os

docker run --rm --network=none --read-only --user=65532:65532 \
  --cap-drop=ALL --security-opt=no-new-privileges \
  --mount "type=bind,src=${ROOT},dst=/workspace,readonly" \
  --tmpfs /tmp:rw,noexec,nosuid,size=16m \
  --env MINIJAM_REPOSITORY=/workspace \
  --env MINIJAM_CONVERTER_BIN=/usr/local/bin/polkavm-to-jam \
  "${IMAGE}" /workspace/scripts/print-service-toolchain-diagnostics.sh \
  | tee "${OUT_DIR}/toolchain-diagnostics.txt"

cmp "${OUT_DIR}/service.blob" "${EXPECTED_BLOB}"
cmp "${OUT_DIR}/service.polkavm" "${EXPECTED_PVM}"

actual_sha256="$(sha256sum "${OUT_DIR}/service.blob" | awk '{print $1}')"
actual_size="$(stat -c '%s' "${OUT_DIR}/service.blob")"
expected_sha256="$(jq -r '.sha256' "${MANIFEST}")"
expected_size="$(jq -r '.byte_len' "${MANIFEST}")"

[[ "$(jq -r '.artifact' "${MANIFEST}")" == system-service.blob ]]
[[ "$(jq -r '.source' "${MANIFEST}")" == services/system-service/src/service.c ]]
[[ "$(jq -r '.stage' "${MANIFEST}")" == 0 ]]
jq -e '.consumed_by_stages | index(1) != null' "${MANIFEST}" >/dev/null
[[ "${actual_sha256}" == "${expected_sha256}" ]]
[[ "${actual_size}" == "${expected_size}" ]]

printf 'SERVICE0_REBUILT_BLOB_SHA256=%s\n' "${actual_sha256}"
printf 'SERVICE0_REBUILT_BLOB_SIZE=%s\n' "${actual_size}"
printf 'SERVICE0_RUNTIME_REQUIRES_LLVM=false\n'
printf 'SERVICE0_REBUILD_REQUIRES_LLVM=true\n'
printf 'LLVM_ENVIRONMENT_PINNED=true\n'
printf 'SERVICE0_PROVENANCE_GATE=PASS\n'
