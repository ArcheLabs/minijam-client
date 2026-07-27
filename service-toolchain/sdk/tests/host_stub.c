// SPDX-License-Identifier: Apache-2.0
#include <stdint.h>

uint64_t minijam_host_call(uint32_t call, const uint64_t args[6]) {
  (void)call;
  (void)args;
  return UINT64_MAX;
}
