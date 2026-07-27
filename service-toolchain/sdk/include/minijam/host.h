// SPDX-License-Identifier: Apache-2.0
#ifndef MINIJAM_HOST_H
#define MINIJAM_HOST_H

#include <stdint.h>

enum minijam_host_call {
  MINIJAM_HOST_GAS = 0,
  MINIJAM_HOST_FETCH = 1,
  MINIJAM_HOST_READ = 3,
  MINIJAM_HOST_WRITE = 4,
  MINIJAM_HOST_NEW = 18,
  MINIJAM_HOST_YIELD = 25,
  MINIJAM_HOST_LOG = 100
};

uint64_t minijam_host_call(uint32_t call, const uint64_t args[6]);

static inline uint64_t minijam_host_call6(uint32_t call, uint64_t a0,
                                          uint64_t a1, uint64_t a2,
                                          uint64_t a3, uint64_t a4,
                                          uint64_t a5) {
  const uint64_t args[6] = {a0, a1, a2, a3, a4, a5};
  return minijam_host_call(call, args);
}

#endif
