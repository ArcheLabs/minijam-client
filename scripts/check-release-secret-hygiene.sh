#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
target="${1:-${root}}"
for forbidden in MINIJAM_RELEASE_NODE_KEYSTORE_B64 'MINIJAM_RELEASE_RELAYER_URI=' 'MINIJAM_RELEASE_WORKER_[123]_URI=' node-keystore.tar.gz worker-1.seed worker-2.seed worker-3.seed; do
  if grep -RIl \
      --exclude-dir='.git' \
      --exclude-dir='target' \
      --exclude-dir='node_modules' \
      --exclude='*.md' \
      --exclude='*.yml' \
      --exclude='*.yaml' \
      --exclude='*.sh' \
      --exclude='.env' \
      --exclude='.env.*' \
      -E "${forbidden}" "${target}" 2>/dev/null; then
    echo "release secret hygiene failure: ${forbidden}" >&2
    exit 1
  fi
done
