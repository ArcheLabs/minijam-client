#!/usr/bin/env bash
set -euo pipefail

ROOT="${MINIJAM_REPOSITORY:-/workspace}"
LOCK="${ROOT}/service-toolchain/compiler/toolchain.lock"
SDK="${ROOT}/service-toolchain/sdk"
CONVERTER="${MINIJAM_CONVERTER_BIN:-/usr/local/bin/polkavm-to-jam}"

resolve_llvm_tool() {
  local override="$1"
  local versioned="$2"
  local unversioned="$3"

  if [[ -n "${override}" ]]; then
    printf '%s\n' "${override}"
    return
  fi
  if [[ -x "/usr/lib/llvm-20/bin/${unversioned}" ]]; then
    printf '/usr/lib/llvm-20/bin/%s\n' "${unversioned}"
    return
  fi
  if command -v "${versioned}" >/dev/null 2>&1; then
    command -v "${versioned}"
    return
  fi
  command -v "${unversioned}"
}

clang="$(resolve_llvm_tool "${MINIJAM_CLANG:-}" clang-20 clang)"
clangxx="$(resolve_llvm_tool "${MINIJAM_CLANGXX:-}" clang++-20 clang++)"
lld="${MINIJAM_LLD:-}"
if [[ -z "${lld}" ]]; then
  if [[ -x /usr/lib/llvm-20/bin/ld.lld ]]; then
    lld=/usr/lib/llvm-20/bin/ld.lld
  elif command -v ld.lld-20 >/dev/null 2>&1; then
    lld="$(command -v ld.lld-20)"
  else
    lld="$(command -v ld.lld)"
  fi
fi

[[ -f "${LOCK}" ]] || { echo "missing compiler manifest: ${LOCK}" >&2; exit 1; }
[[ -x "${clang}" && -x "${clangxx}" && -x "${lld}" ]] || {
  echo "LLVM 20 compiler tools are not executable" >&2
  exit 1
}
[[ -x "${CONVERTER}" ]] || { echo "missing converter: ${CONVERTER}" >&2; exit 1; }

clang_version="$(${clang} --version | head -n 1)"
clangxx_version="$(${clangxx} --version | head -n 1)"
lld_version="$(${lld} --version | head -n 1)"
llvm_major="$(printf '%s\n' "${clang_version}" | sed -nE 's/.*version ([0-9]+).*/\1/p')"
expected_major="$(sed -nE 's/^clang_major[[:space:]]*=[[:space:]]*([0-9]+).*$/\1/p' "${LOCK}")"

sdk_sha256="$({
  find "${SDK}" -type f -print0 | sort -z | while IFS= read -r -d '' file; do
    digest="$(sha256sum "${file}" | awk '{print $1}')"
    printf '%s  %s\n' "${digest}" "${file#"${ROOT}"/}"
  done
} | sha256sum | awk '{print $1}')"

printf 'CLANG_PATH=%s\n' "${clang}"
printf 'CLANG_VERSION=%s\n' "${clang_version}"
printf 'CLANGXX_PATH=%s\n' "${clangxx}"
printf 'CLANGXX_VERSION=%s\n' "${clangxx_version}"
printf 'LD_LLD_PATH=%s\n' "${lld}"
printf 'LD_LLD_VERSION=%s\n' "${lld_version}"
printf 'LLVM_MAJOR=%s\n' "${llvm_major}"
printf 'LLVM_EXPECTED_MAJOR=%s\n' "${expected_major}"
printf 'LLVM_BASE_IMAGE=%s\n' "${MINIJAM_LLVM_BASE_IMAGE:-unknown}"
printf 'CONVERTER_BASE_IMAGE=%s\n' "${MINIJAM_CONVERTER_BASE_IMAGE:-unknown}"
printf 'COMPILER_MANIFEST_SHA256=%s\n' "$(sha256sum "${LOCK}" | awk '{print $1}')"
printf 'CONVERTER_SHA256=%s\n' "$(sha256sum "${CONVERTER}" | awk '{print $1}')"
printf 'SDK_SHA256=%s\n' "${sdk_sha256}"

[[ "${llvm_major}" == "${expected_major}" ]] || {
  echo "LLVM major does not match compiler manifest" >&2
  exit 1
}
