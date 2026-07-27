#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
TMP="$(mktemp -d)"
trap 'rm -rf -- "${TMP}"' EXIT

INCLUDE="${ROOT}/service-toolchain/sdk/include"
CFLAGS=(-Wall -Wextra -Werror -I "${INCLUDE}")

clang -std=c11 "${CFLAGS[@]}" -DMINIJAM_HOST_TEST \
  -c "${ROOT}/service-toolchain/sdk/src/host.c" -o "${TMP}/host.o"
clang -std=c11 "${CFLAGS[@]}" -DMINIJAM_HOST_TEST \
  -c "${ROOT}/service-toolchain/sdk/src/minijam.c" -o "${TMP}/minijam.o"
clang -std=c11 "${CFLAGS[@]}" \
  -c "${ROOT}/service-toolchain/sdk/tests/host_stub.c" -o "${TMP}/stub.o"
clang -std=c11 "${CFLAGS[@]}" \
  -c "${ROOT}/examples/services/counter/service.c" -o "${TMP}/counter-c.o"
clang++ -std=c++17 "${CFLAGS[@]}" -fno-exceptions -fno-rtti \
  -fno-threadsafe-statics \
  -c "${ROOT}/examples/services/counter/service.cpp" -o "${TMP}/counter-cpp.o"

clang -nostdlib -Wl,-e,minijam_refine \
  "${TMP}/minijam.o" "${TMP}/stub.o" "${TMP}/counter-c.o" \
  -o "${TMP}/counter-c"
clang++ -nostdlib -Wl,-e,minijam_refine \
  "${TMP}/minijam.o" "${TMP}/stub.o" "${TMP}/counter-cpp.o" \
  -o "${TMP}/counter-cpp"

test -s "${TMP}/counter-c"
test -s "${TMP}/counter-cpp"
printf 'service SDK C/C++ native smoke passed\n'
