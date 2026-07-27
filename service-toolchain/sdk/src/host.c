// SPDX-License-Identifier: Apache-2.0
#include <minijam/host.h>

#ifndef MINIJAM_HOST_TEST
#if defined(__riscv)
struct __attribute__((packed)) minijam_extern_metadata_v2 {
  uint8_t version;
  uint32_t flags;
  uint32_t symbol_length;
  const uint8_t *symbol;
  uint8_t input_regs;
  uint8_t output_regs;
  uint8_t has_index;
  uint32_t index;
};

#define MINIJAM_IMPORT_METADATA(NAME, ID)                                     \
  static const uint8_t NAME##_symbol[]                                        \
      __attribute__((section(".polkavm_metadata"), used)) = #NAME;            \
  static const struct minijam_extern_metadata_v2 NAME##_metadata              \
      __attribute__((section(".polkavm_metadata"), used)) = {                 \
          2, 0, sizeof(NAME##_symbol) - 1, NAME##_symbol, 6, 2, 1, ID}

MINIJAM_IMPORT_METADATA(minijam_gas, MINIJAM_HOST_GAS);
MINIJAM_IMPORT_METADATA(minijam_fetch, MINIJAM_HOST_FETCH);
MINIJAM_IMPORT_METADATA(minijam_read, MINIJAM_HOST_READ);
MINIJAM_IMPORT_METADATA(minijam_write, MINIJAM_HOST_WRITE);
MINIJAM_IMPORT_METADATA(minijam_new, MINIJAM_HOST_NEW);
MINIJAM_IMPORT_METADATA(minijam_yield, MINIJAM_HOST_YIELD);
MINIJAM_IMPORT_METADATA(minijam_log, MINIJAM_HOST_LOG);
#undef MINIJAM_IMPORT_METADATA
#endif

uint64_t minijam_host_call6(uint32_t call, uint64_t a0, uint64_t a1,
                            uint64_t a2, uint64_t a3, uint64_t a4,
                            uint64_t a5) {
#if defined(__riscv)
  register uint64_t r0 __asm__("a0") = a0;
  register uint64_t r1 __asm__("a1") = a1;
  register uint64_t r2 __asm__("a2") = a2;
  register uint64_t r3 __asm__("a3") = a3;
  register uint64_t r4 __asm__("a4") = a4;
  register uint64_t r5 __asm__("a5") = a5;
#define MINIJAM_ECALLI(METADATA)                                              \
  __asm__ volatile(".insn r 0xb, 0, 0, zero, zero, zero\n"                   \
                   ".8byte %c6\n"                                             \
                   : "+r"(r0)                                                 \
                   : "r"(r1), "r"(r2), "r"(r3), "r"(r4), "r"(r5),           \
                     "i"(&(METADATA))                                         \
                   : "memory")
  switch (call) {
    case MINIJAM_HOST_GAS: MINIJAM_ECALLI(minijam_gas_metadata); break;
    case MINIJAM_HOST_FETCH: MINIJAM_ECALLI(minijam_fetch_metadata); break;
    case MINIJAM_HOST_READ: MINIJAM_ECALLI(minijam_read_metadata); break;
    case MINIJAM_HOST_WRITE: MINIJAM_ECALLI(minijam_write_metadata); break;
    case MINIJAM_HOST_NEW: MINIJAM_ECALLI(minijam_new_metadata); break;
    case MINIJAM_HOST_YIELD: MINIJAM_ECALLI(minijam_yield_metadata); break;
    case MINIJAM_HOST_LOG: MINIJAM_ECALLI(minijam_log_metadata); break;
    default: return UINT64_MAX;
  }
#undef MINIJAM_ECALLI
  return r0;
#else
#error "MiniJAM production SDK must be compiled for the pinned PolkaVM target"
#endif
}
#endif
