#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  clang \
  libclang-dev \
  llvm \
  llvm-dev \
  protobuf-compiler

clang --version
llvm_config="$(command -v llvm-config || true)"
if [[ -z "${llvm_config}" ]]; then
  llvm_config="$(find /usr/bin -maxdepth 1 -type f -name 'llvm-config-*' | sort -V | tail -n1)"
fi
test -n "${llvm_config}"
"${llvm_config}" --version
protoc --version

libclang_path="$("${llvm_config}" --libdir 2>/dev/null || true)"
if [[ -z "${libclang_path}" || ! -d "${libclang_path}" ]] || ! find "${libclang_path}" -maxdepth 1 \
  \( -type f -o -type l \) \( -name 'libclang.so' -o -name 'libclang-*.so*' \) \
  -print -quit 2>/dev/null | grep -q .; then
  libclang_file="$(find /usr/lib /usr/local/lib \
    \( -type f -o -type l \) \
    \( -name 'libclang.so' -o -name 'libclang-*.so*' \) \
    -print -quit 2>/dev/null)"
  test -n "${libclang_file}"
  libclang_path="$(dirname "${libclang_file}")"
fi
test -n "${libclang_path}"
test -d "${libclang_path}"

echo "LIBCLANG_PATH=${libclang_path}" >> "${GITHUB_ENV}"
echo "LLVM_CONFIG_PATH=${llvm_config}" >> "${GITHUB_ENV}"

printf 'NATIVE_BUILD_DEPENDENCIES=PASS\n'
