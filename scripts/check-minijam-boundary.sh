#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"

production_paths=(
  "${root}/crates/minijam-worker/src"
  "${root}/crates/minijam-work-package-builder/src"
  "${root}/runtime/src"
  "${root}/pallets/minijam/src"
  "${root}/pallets/minijam-workers/src"
  "${root}/node/src"
  "${root}/external/jambda/crates/minijam-executive/src"
)

if rg -n '\b(TinySpec|FullSpec)\b' "${production_paths[@]}"; then
  echo "MiniJAM production path must use jambda_minijam_spec::MiniJamSpec" >&2
  exit 1
fi

if rg -n '5_000_000_000|5B' "${production_paths[@]}"; then
  echo "MiniJAM production path contains a forbidden 5B Refine ceiling" >&2
  exit 1
fi

rg -n 'MiniJamSpec' \
  "${root}/crates/minijam-worker/src" \
  "${root}/crates/minijam-work-package-builder/src" \
  "${root}/external/jambda/crates/minijam-executive/src" >/dev/null

echo "MiniJAM specification boundary check passed"
