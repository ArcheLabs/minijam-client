// SPDX-License-Identifier: Apache-2.0
#ifndef MINIJAM_CRYPTO_H
#define MINIJAM_CRYPTO_H

#include <stddef.h>
#include <stdint.h>

void minijam_blake2b_256(const void *input, size_t input_size,
                         uint8_t output[32]);

#endif
