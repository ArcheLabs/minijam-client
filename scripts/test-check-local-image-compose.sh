#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
fixture="${root}/scripts/fixtures/local-image-compose"

PATH="${fixture}:${PATH}" "${root}/scripts/check-local-image-compose.sh" "${fixture}"
