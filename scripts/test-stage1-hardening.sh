#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$root/scripts/check-stage1-boundary.sh"
"$root/scripts/check-release-secret-hygiene.sh"
docker compose -f "$root/deploy/stage1/compose.compact.yml" config --no-interpolate >/dev/null
docker compose -f "$root/deploy/stage1/compose.split.yml" config --no-interpolate >/dev/null
rg -q 'pub const SS58Prefix: u8 = 42' "$root/runtime/src/lib.rs"
if rg -n 'mnemonic|seed phrase|//Alice|//Bob' "$root/deploy/stage1" --glob '!README.md' --glob '!.env.example'; then
  echo "Stage-1 deployment contains secret-like material" >&2
  exit 1
fi

