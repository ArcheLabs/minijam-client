#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
# shellcheck source=stage1-core-packages.sh
source "${ROOT}/scripts/stage1-core-packages.sh"

cargo_args=()
for package in "${STAGE1_CORE_PACKAGES[@]}"; do
  cargo_args+=(--package "${package}")
done

cargo test --locked --manifest-path "${ROOT}/Cargo.toml" "${cargo_args[@]}"
