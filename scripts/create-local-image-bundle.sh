#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tag="${1:?usage: $0 RELEASE_TAG RELEASE_MANIFEST OUTPUT_DIR}"
manifest="${2:?usage: $0 RELEASE_TAG RELEASE_MANIFEST OUTPUT_DIR}"
out="${3:?usage: $0 RELEASE_TAG RELEASE_MANIFEST OUTPUT_DIR}"
bundle="${out}/minijam-local-${tag}"
test -f "${manifest}" || { echo "missing manifest" >&2; exit 1; }
rm -rf "${bundle}"
mkdir -p "${bundle}"
install -m 0644 "${root}/deploy/local-image/compose.yml" "${bundle}/compose.yml"
install -m 0755 "${root}/deploy/local-image/minijam-local" "${bundle}/minijam-local"
install -m 0644 "${root}/deploy/local-image/README.md" "${bundle}/README.md"
jq -r --arg tag "${tag}" '
  def image($key): .images[$key] // error("missing image " + $key);
  "MINIJAM_RELEASE_TAG=" + $tag,
  "MINIJAM_NODE_IMAGE=" + image("node"),
  "MINIJAM_WORKER_IMAGE=" + image("worker"),
  "MINIJAM_COMPILER_IMAGE=" + image("compiler"),
  "MINIJAM_PLAYGROUND_API_IMAGE=" + image("playground_api"),
  "MINIJAM_PLAYGROUND_WEB_IMAGE=" + image("playground_web"),
  "MINIJAM_PLAYGROUND_WEB_LOCAL_IMAGE=" + image("playground_web_local"),
  "MINIJAM_NODE_BIND=127.0.0.1", "MINIJAM_NODE_PORT=9944",
  "MINIJAM_WEB_BIND=127.0.0.1", "MINIJAM_WEB_PORT=4173"
' "${manifest}" >"${bundle}/release.env"
for value in $(cut -d= -f2 "${bundle}/release.env" | grep '^ghcr.io/'); do
  [[ "${value}" =~ ^ghcr\.io/archelabs/[a-z0-9-]+@sha256:[0-9a-f]{64}$ ]] || { echo "invalid digest: ${value}" >&2; exit 1; }
done
! rg -q '^[[:space:]]*build:|deploy/stage0/secrets|stage0-raw\.json' "${bundle}/compose.yml" "${bundle}/release.env" || { echo "unsafe bundle content" >&2; exit 1; }
(cd "${bundle}" && sha256sum compose.yml release.env minijam-local README.md > SHA256SUMS && sha256sum -c SHA256SUMS)
tar -C "${out}" -czf "${out}/minijam-local-${tag}.tar.gz" "minijam-local-${tag}"
sha256sum "${out}/minijam-local-${tag}.tar.gz" >"${out}/minijam-local-${tag}.tar.gz.sha256"
