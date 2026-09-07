#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
IMAGE="${MINIJAM_COMPILER_IMAGE:-minijam-compiler:ci}"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT
REPRODUCED_DIR="${MINIJAM_REPRODUCED_ARTIFACT_DIR:-${ROOT}/target/minijam-compat-artifacts}"
MANIFEST="${REPRODUCED_DIR}/manifest.tsv"

install -d -m 0777 "${REPRODUCED_DIR}/c" "${REPRODUCED_DIR}/cpp"
rm -f \
  "${REPRODUCED_DIR}/c/service.blob" \
  "${REPRODUCED_DIR}/c/service.polkavm" \
  "${REPRODUCED_DIR}/cpp/service.blob" \
  "${REPRODUCED_DIR}/cpp/service.polkavm"
printf 'language\tblob\tblob_sha256\tblob_size\tpolkavm\tpolkavm_sha256\tpolkavm_size\n' > "${MANIFEST}"

printf 'MINIJAM_SHA=%s\n' "$(git rev-parse HEAD)"
printf 'COMPILER_SHA=%s\n' "$(git rev-parse HEAD:service-toolchain/compiler/toolchain.lock)"
printf 'COMPILER_TOOLCHAIN_SHA256=%s\n' "$(sha256sum "${ROOT}/service-toolchain/compiler/toolchain.lock" | awk '{print $1}')"
printf 'COMPILER_IMAGE=%s\n' "${IMAGE}"

docker build -f "${ROOT}/deploy/compiler/Dockerfile" -t "${IMAGE}" "${ROOT}"
for language in c cpp; do
  source="${ROOT}/examples/services/counter/service.c"
  expected="${ROOT}/examples/services/counter/artifacts/counter-c.blob"
  [[ "${language}" == cpp ]] && {
    source="${ROOT}/examples/services/counter/service.cpp"
    expected="${ROOT}/examples/services/counter/artifacts/counter-cpp.blob"
  }
  install -d "${TMP}/${language}"
  chmod 0777 "${TMP}" "${TMP}/${language}"
  output_dir="${REPRODUCED_DIR}/${language}"
  printf 'ARTIFACT_BUILD_LANGUAGE=%s\n' "${language}"
  printf 'ARTIFACT_SOURCE=%s\n' "${source}"
  printf 'ARTIFACT_OUTPUT_DIR=%s\n' "${output_dir}"
  docker run --rm --network=none --read-only --user=65532:65532 \
    --cpus=1 --memory=512m --pids-limit=64 --cap-drop=ALL \
    --security-opt=no-new-privileges \
    --mount "type=bind,src=${ROOT},dst=/workspace,readonly" \
    --mount "type=bind,src=${source},dst=/input/service.${language},readonly" \
    --mount "type=bind,src=${output_dir},dst=/output" \
    --tmpfs /tmp:rw,noexec,nosuid,size=64m \
    --env MINIJAM_CONVERTER_BIN=/usr/local/bin/polkavm-to-jam \
    "${IMAGE}" /workspace/scripts/compile-service "${language}" \
    "/input/service.${language}" /output Os
  generated_blob="${output_dir}/service.blob"
  generated_polkavm="${output_dir}/service.polkavm"
  cmp "${generated_blob}" "${expected}"
  blob_sha256="$(sha256sum "${generated_blob}" | awk '{print $1}')"
  blob_size="$(stat -c '%s' "${generated_blob}")"
  polkavm_sha256="$(sha256sum "${generated_polkavm}" | awk '{print $1}')"
  polkavm_size="$(stat -c '%s' "${generated_polkavm}")"
  printf 'ARTIFACT_PATH=%s\n' "${generated_blob}"
  printf 'ARTIFACT_SHA256=%s\n' "${blob_sha256}"
  printf 'ARTIFACT_SIZE=%s\n' "${blob_size}"
  printf 'PVM_PATH=%s\n' "${generated_polkavm}"
  printf 'PVM_SHA256=%s\n' "${polkavm_sha256}"
  printf 'PVM_SIZE=%s\n' "${polkavm_size}"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "${language}" \
    "${generated_blob}" \
    "${blob_sha256}" \
    "${blob_size}" \
    "${generated_polkavm}" \
    "${polkavm_sha256}" \
    "${polkavm_size}" >> "${MANIFEST}"
done

printf 'ARTIFACT_MANIFEST=%s\n' "${MANIFEST}"
