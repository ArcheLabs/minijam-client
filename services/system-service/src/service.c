// SPDX-License-Identifier: Apache-2.0
#include <minijam/crypto.h>
#include <minijam/host.h>
#include <minijam/minijam.h>
#include <stddef.h>
#include <stdint.h>

#define SYSTEM_INPUT_MAX 16384u
#define SYSTEM_KEY_MAX 80u
#define REJECT_REQUEST_ID 3u
#define REJECT_NONCE 4u
#define REJECT_COMMAND 5u
#define REJECT_NEW 6u

static const uint8_t LAST_NONCE_PREFIX[] = "system/last-nonce/";
static const uint8_t RECEIPT_PREFIX[] = "system/receipt/";
static const uint8_t CONTROLLER_PREFIX[] = "system/controller/";
static const uint8_t SERVICES_PREFIX[] = "system/services/";
static const uint8_t REQUEST_DOMAIN[] = "minijam/system-op/v1";
static uint8_t input[SYSTEM_INPUT_MAX];

static uint32_t load_u32(const uint8_t *p) {
  return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 |
         (uint32_t)p[3] << 24;
}

static uint64_t load_u64(const uint8_t *p) {
  return (uint64_t)load_u32(p) | (uint64_t)load_u32(p + 4) << 32;
}

static void store_u32(uint8_t *p, uint32_t value) {
  for (size_t i = 0; i < 4; ++i) p[i] = (uint8_t)(value >> (8 * i));
}

static void store_u64(uint8_t *p, uint64_t value) {
  for (size_t i = 0; i < 8; ++i) p[i] = (uint8_t)(value >> (8 * i));
}

static void copy(uint8_t *out, const uint8_t *in, size_t size) {
  for (size_t i = 0; i < size; ++i) out[i] = in[i];
}

static size_t prefixed_key(uint8_t *key, const uint8_t *prefix,
                           size_t prefix_size, const uint8_t *suffix,
                           size_t suffix_size) {
  copy(key, prefix, prefix_size);
  copy(key + prefix_size, suffix, suffix_size);
  return prefix_size + suffix_size;
}

static int compact_u32(const uint8_t **cursor, const uint8_t *end,
                       uint32_t *value) {
  if (*cursor == end) return 0;
  uint8_t first = *(*cursor)++;
  if ((first & 3u) == 0) {
    *value = first >> 2;
    return 1;
  }
  if ((first & 3u) == 1 && (size_t)(end - *cursor) >= 1) {
    *value = ((uint32_t)(*(*cursor)++) << 6) | (first >> 2);
    return 1;
  }
  if ((first & 3u) == 2 && (size_t)(end - *cursor) >= 3) {
    *value = (first >> 2) | (uint32_t)(*cursor)[0] << 6 |
             (uint32_t)(*cursor)[1] << 14 | (uint32_t)(*cursor)[2] << 22;
    *cursor += 3;
    return 1;
  }
  return 0;
}

static void write_rejected(const uint8_t request_id[32], uint32_t code) {
  uint8_t key[SYSTEM_KEY_MAX];
  uint8_t receipt[5];
  size_t key_size =
      prefixed_key(key, RECEIPT_PREFIX, sizeof(RECEIPT_PREFIX) - 1, request_id,
                   32);
  receipt[0] = 2;
  store_u32(receipt + 1, code);
  (void)minijam_storage_write(key, key_size, receipt, sizeof(receipt));
}

static int request_id_matches(const uint8_t request_id[32],
                              const uint8_t sender[32],
                              const uint8_t encoded_nonce[8],
                              const uint8_t *encoded_command,
                              size_t command_size) {
  uint8_t message[sizeof(REQUEST_DOMAIN) - 1 + 32 + 8 + 89];
  uint8_t actual[32];
  size_t offset = 0;
  copy(message + offset, REQUEST_DOMAIN, sizeof(REQUEST_DOMAIN) - 1);
  offset += sizeof(REQUEST_DOMAIN) - 1;
  copy(message + offset, sender, 32);
  offset += 32;
  copy(message + offset, encoded_nonce, 8);
  offset += 8;
  copy(message + offset, encoded_command, command_size);
  offset += command_size;
  minijam_blake2b_256(message, offset, actual);
  uint8_t difference = 0;
  for (size_t i = 0; i < 32; ++i) difference |= actual[i] ^ request_id[i];
  return difference == 0;
}

static int nonce_is_fresh(const uint8_t sender[32], uint64_t nonce) {
  uint8_t key[SYSTEM_KEY_MAX];
  uint8_t old[8];
  size_t old_size = 0;
  size_t key_size =
      prefixed_key(key, LAST_NONCE_PREFIX, sizeof(LAST_NONCE_PREFIX) - 1,
                   sender, 32);
  minijam_status status =
      minijam_storage_read(key, key_size, old, sizeof(old), &old_size);
  return status == MINIJAM_NOT_FOUND ||
         (status == MINIJAM_OK && old_size == sizeof(old) &&
          nonce > load_u64(old));
}

static void write_nonce(const uint8_t sender[32], uint64_t nonce) {
  uint8_t key[SYSTEM_KEY_MAX];
  uint8_t encoded[8];
  size_t key_size =
      prefixed_key(key, LAST_NONCE_PREFIX, sizeof(LAST_NONCE_PREFIX) - 1,
                   sender, 32);
  store_u64(encoded, nonce);
  (void)minijam_storage_write(key, key_size, encoded, sizeof(encoded));
}

static void create_service(const uint8_t request_id[32],
                           const uint8_t sender[32], uint64_t nonce,
                           const uint8_t *command) {
  const uint8_t *controller = command;
  const uint8_t *code_hash = command + 32;
  uint32_t code_len = load_u32(command + 64);
  uint64_t min_item_gas = load_u64(command + 68);
  uint64_t min_memo_gas = load_u64(command + 76);

  if (!nonce_is_fresh(sender, nonce)) {
    write_rejected(request_id, REJECT_NONCE);
    return;
  }

  uint64_t sid = minijam_host_call6(
      MINIJAM_HOST_NEW, (uintptr_t)code_hash, code_len, min_item_gas,
      min_memo_gas, 0, UINT64_MAX);
  if (sid > UINT32_MAX) {
    write_rejected(request_id, REJECT_NEW);
    return;
  }

  uint8_t key[SYSTEM_KEY_MAX];
  uint8_t encoded_sid[4];
  uint8_t receipt[37];
  store_u32(encoded_sid, (uint32_t)sid);
  receipt[0] = 0;
  copy(receipt + 1, encoded_sid, sizeof(encoded_sid));
  copy(receipt + 5, controller, 32);

  size_t key_size =
      prefixed_key(key, RECEIPT_PREFIX, sizeof(RECEIPT_PREFIX) - 1, request_id,
                   32);
  (void)minijam_storage_write(key, key_size, receipt, sizeof(receipt));
  key_size = prefixed_key(key, CONTROLLER_PREFIX,
                          sizeof(CONTROLLER_PREFIX) - 1, encoded_sid, 4);
  (void)minijam_storage_write(key, key_size, controller, 32);
  key_size =
      prefixed_key(key, SERVICES_PREFIX, sizeof(SERVICES_PREFIX) - 1,
                   controller, 32);
  copy(key + key_size, encoded_sid, 4);
  key_size += 4;
  (void)minijam_storage_write(key, key_size, encoded_sid, 4);
  write_nonce(sender, nonce);
}

MINIJAM_REFINE { return minijam_refine_ok(0, 0); }

MINIJAM_ACCUMULATE {
  size_t input_size = 0;
  if (minijam_result(0, input, sizeof(input), &input_size) != MINIJAM_OK)
    return;

  const uint8_t *cursor = input;
  const uint8_t *end = input + input_size;
  uint32_t count = 0;
  if (!compact_u32(&cursor, end, &count) || count > 64) return;

  for (uint32_t i = 0; i < count; ++i) {
    if ((size_t)(end - cursor) < 73) return;
    const uint8_t *request_id = cursor;
    const uint8_t *sender = cursor + 32;
    const uint8_t *encoded_nonce = cursor + 64;
    const uint8_t *encoded_command = cursor + 72;
    uint64_t nonce = load_u64(cursor + 64);
    uint8_t command = cursor[72];
    cursor += 73;

    if (command == 0) {
      if ((size_t)(end - cursor) < 84) return;
      if (request_id_matches(request_id, sender, encoded_nonce, encoded_command,
                             85))
        create_service(request_id, sender, nonce, cursor);
      else
        write_rejected(request_id, REJECT_REQUEST_ID);
      cursor += 84;
    } else if (command == 1) {
      if ((size_t)(end - cursor) < 88) return;
      if (!request_id_matches(request_id, sender, encoded_nonce,
                              encoded_command, 89))
        write_rejected(request_id, REJECT_REQUEST_ID);
      cursor += 88;
    } else {
      write_rejected(request_id, REJECT_COMMAND);
      return;
    }
  }
  if (cursor != end) return;
  minijam_yield(0, 0);
}
