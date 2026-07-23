#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
NODE_BIN="${1:-${ROOT}/target/release/minijam-node}"
OUT_DIR="${2:-${ROOT}/chain-specs}"

if [[ ! -x "${NODE_BIN}" ]]; then
  printf 'export-stage0-chain-specs: node binary not executable: %s\n' "${NODE_BIN}" >&2
  printf 'build it with: cargo build --release -p minijam-node\n' >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
"${NODE_BIN}" export-chain-spec --chain stage0 > "${OUT_DIR}/stage0.json"
"${NODE_BIN}" export-chain-spec --chain stage0 --raw > "${OUT_DIR}/stage0-raw.json"

printf 'export-stage0-chain-specs: wrote %s/stage0.json and %s/stage0-raw.json\n' "${OUT_DIR}" "${OUT_DIR}"
