#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
for compose_file in "${repository}/deploy/season2/compose.compact.yml" "${repository}/deploy/season2/compose.split.yml"; do
  rg -q -- '--rpc-methods=safe' "${compose_file}"
  compiler_block="$(awk '/^  compiler:/{seen=1; next} seen && /^  [a-zA-Z0-9_-]+:/{exit} seen{print}' "${compose_file}")"
  if printf '%s\n' "${compiler_block}" | rg -qi 'validator|sudo|relayer|key|mount'; then
    echo "compiler security boundary contains a forbidden key or mount: ${compose_file}" >&2
    exit 1
  fi
done

# The Season 2 runtime contains only the one-way Allocation ingress. Keep any
# legacy bridge pallet outside runtime composition and reject new reverse-call
# spellings in the active Season 2 sources.
if rg -n -i 'RuntimeCall::.*(redeem|release)|Hub.*(redeem|release)|((redeem|release).*Hub)' \
    "${repository}/runtime/src" "${repository}/pallets/minijam/src"; then
  echo "reverse Hub redemption found in active Season 2 runtime sources" >&2
  exit 1
fi

echo "Season 2 hardening checks passed"
