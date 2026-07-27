// SPDX-License-Identifier: Apache-2.0
#include <minijam/host.h>

#ifndef MINIJAM_HOST_TEST
uint64_t minijam_host_call6(uint32_t call, uint64_t a0, uint64_t a1,
                            uint64_t a2, uint64_t a3, uint64_t a4,
                            uint64_t a5) {
#if defined(__riscv)
  register uint64_t r7 __asm__("a3") = a0;
  register uint64_t r8 __asm__("a4") = a1;
  register uint64_t r9 __asm__("a5") = a2;
  register uint64_t r10 __asm__("a6") = a3;
  register uint64_t r11 __asm__("a7") = a4;
  register uint64_t r12 __asm__("t0") = a5;
  __asm__ volatile("ecalli %6"
                   : "+r"(r7)
                   : "r"(r8), "r"(r9), "r"(r10), "r"(r11), "r"(r12),
                     "i"(call)
                   : "memory");
  return r7;
#else
#error "MiniJAM production SDK must be compiled for the pinned PolkaVM target"
#endif
}
#endif
