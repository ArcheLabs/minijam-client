#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"

cargo test \
  --locked \
  --manifest-path "${ROOT}/external/jambda/crates/minijam-executive/Cargo.toml" \
  --config "patch.crates-io.minijam-protocol.path='${ROOT}/crates/minijam-protocol'" \
  --config "patch.crates-io.minijam-jamcore-api.path='${ROOT}/crates/minijam-jamcore-api'" \
  counter_ -- --nocapture
