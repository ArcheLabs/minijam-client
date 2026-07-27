#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
IMAGE="${MINIJAM_COMPILER_IMAGE:-minijam-compiler:ci}"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT

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
  docker run --rm --network=none --read-only --user=65532:65532 \
    --cpus=1 --memory=512m --pids-limit=64 --cap-drop=ALL \
    --security-opt=no-new-privileges \
    --mount "type=bind,src=${ROOT},dst=/workspace,readonly" \
    --mount "type=bind,src=${source},dst=/input/service.${language},readonly" \
    --mount "type=bind,src=${TMP}/${language},dst=/output" \
    --tmpfs /tmp:rw,noexec,nosuid,size=64m \
    --env MINIJAM_CONVERTER_BIN=/usr/local/bin/polkavm-to-jam \
    "${IMAGE}" /workspace/scripts/compile-service "${language}" \
    "/input/service.${language}" /output Os
  cmp "${TMP}/${language}/service.blob" "${expected}"
done
