// SPDX-License-Identifier: Apache-2.0
#include <minijam/host.h>
#include <minijam/minijam.h>

static minijam_status copy_fetch(uint64_t mode, uint64_t index, void *output,
                                 size_t capacity, size_t *output_size) {
  uint64_t length = minijam_host_call6(
      MINIJAM_HOST_FETCH, (uintptr_t)output, 0, capacity, mode, index, 0);
  if (length == MINIJAM_HOST_NONE) return MINIJAM_NOT_FOUND;
  if (output_size) *output_size = (size_t)length;
  return length > capacity ? MINIJAM_BUFFER_TOO_SMALL : MINIJAM_OK;
}

size_t minijam_payload_size(void) {
  uint64_t length =
      minijam_host_call6(MINIJAM_HOST_FETCH, 0, 0, 0, 13, 0, 0);
  return length == MINIJAM_HOST_NONE ? 0 : (size_t)length;
}

minijam_status minijam_payload(void *output, size_t capacity,
                               size_t *output_size) {
  return copy_fetch(13, 0, output, capacity, output_size);
}

size_t minijam_extrinsic_count(void) {
  // Stage 0 builder preserves external data order. Probe until the first
  // missing entry; the public helper remains bounded by the WorkItem.
  size_t count = 0;
  while (minijam_host_call6(MINIJAM_HOST_FETCH, 0, 0, 0, 4, count, 0) !=
         MINIJAM_HOST_NONE)
    ++count;
  return count;
}

minijam_status minijam_extrinsic(size_t index, void *output, size_t capacity,
                                 size_t *output_size) {
  return copy_fetch(4, index, output, capacity, output_size);
}

size_t minijam_result_count(void) {
  size_t count = 0;
  while (minijam_host_call6(MINIJAM_HOST_FETCH, 0, 0, 0, 15, count, 0) !=
         MINIJAM_HOST_NONE)
    ++count;
  return count;
}

minijam_status minijam_result(size_t index, void *output, size_t capacity,
                              size_t *output_size) {
  return copy_fetch(15, index, output, capacity, output_size);
}

minijam_status minijam_storage_read(const void *key, size_t key_size,
                                    void *output, size_t capacity,
                                    size_t *output_size) {
  uint64_t length = minijam_host_call6(
      MINIJAM_HOST_READ, MINIJAM_HOST_NONE, (uintptr_t)key, key_size,
      (uintptr_t)output, 0, capacity);
  if (length == MINIJAM_HOST_NONE) return MINIJAM_NOT_FOUND;
  if (output_size) *output_size = (size_t)length;
  return length > capacity ? MINIJAM_BUFFER_TOO_SMALL : MINIJAM_OK;
}

minijam_status minijam_storage_write(const void *key, size_t key_size,
                                     const void *value, size_t value_size) {
  uint64_t result = minijam_host_call6(MINIJAM_HOST_WRITE, (uintptr_t)key,
                                       key_size, (uintptr_t)value, value_size,
                                       0, 0);
  return result == 1 ? MINIJAM_HOST_ERROR : MINIJAM_OK;
}

minijam_status minijam_storage_delete(const void *key, size_t key_size) {
  return minijam_storage_write(key, key_size, 0, 0);
}

void minijam_log(const char *message, size_t size) {
  (void)minijam_host_call6(MINIJAM_HOST_LOG, (uintptr_t)message, size, 0, 0, 0,
                           0);
}

void minijam_yield(const void *value, size_t size) {
  (void)minijam_host_call6(MINIJAM_HOST_YIELD, (uintptr_t)value, size, 0, 0, 0,
                           0);
}

minijam_refine_output minijam_refine_ok(const void *value, size_t size) {
  minijam_refine_output output = {value, size};
  return output;
}

minijam_refine_output minijam_refine_error(uint32_t code) {
  static uint32_t error_code;
  error_code = code;
  return minijam_refine_ok(&error_code, sizeof(error_code));
}
