// SPDX-License-Identifier: Apache-2.0
#ifndef MINIJAM_MINIJAM_H
#define MINIJAM_MINIJAM_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MINIJAM_ABI_VERSION 1u
#define MINIJAM_HOST_NONE UINT64_MAX

typedef struct minijam_refine_output {
  const void *data;
  size_t size;
} minijam_refine_output;

#define MINIJAM_REFINE __attribute__((used)) minijam_refine_output minijam_refine(void)
#define MINIJAM_ACCUMULATE __attribute__((used)) void minijam_accumulate(void)

typedef enum minijam_status {
  MINIJAM_OK = 0,
  MINIJAM_NOT_FOUND = 1,
  MINIJAM_BUFFER_TOO_SMALL = 2,
  MINIJAM_HOST_ERROR = 3
} minijam_status;

size_t minijam_payload_size(void);
minijam_status minijam_payload(void *output, size_t output_capacity,
                               size_t *output_size);
size_t minijam_extrinsic_count(void);
minijam_status minijam_extrinsic(size_t index, void *output,
                                 size_t output_capacity, size_t *output_size);

size_t minijam_result_count(void);
minijam_status minijam_result(size_t index, void *output,
                              size_t output_capacity, size_t *output_size);

minijam_status minijam_storage_read(const void *key, size_t key_size,
                                    void *output, size_t output_capacity,
                                    size_t *output_size);
minijam_status minijam_storage_write(const void *key, size_t key_size,
                                     const void *value, size_t value_size);
minijam_status minijam_storage_delete(const void *key, size_t key_size);

void minijam_log(const char *message, size_t message_size);
void minijam_yield(const void *value, size_t value_size);

// Refine completion is represented by the PVM program's returned byte sequence.
minijam_refine_output minijam_refine_ok(const void *value, size_t value_size);
minijam_refine_output minijam_refine_error(uint32_t code);

#ifdef __cplusplus
}
#endif

#endif
