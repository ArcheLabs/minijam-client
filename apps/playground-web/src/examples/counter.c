// SPDX-License-Identifier: Apache-2.0
#include <minijam/minijam.h>
#include <stdint.h>

static const char COUNTER_KEY[] = "counter";

MINIJAM_REFINE {
  int64_t increment = 0;
  size_t size = 0;
  if (minijam_payload(&increment, sizeof(increment), &size) != MINIJAM_OK ||
      size != sizeof(increment)) {
    return minijam_refine_error(1);
  }
  return minijam_refine_ok(&increment, sizeof(increment));
}

MINIJAM_ACCUMULATE {
  int64_t increment = 0;
  int64_t counter = 0;
  size_t size = 0;
  if (minijam_result(0, &increment, sizeof(increment), &size) != MINIJAM_OK ||
      size != sizeof(increment))
    return;
  if (minijam_storage_read(COUNTER_KEY, sizeof(COUNTER_KEY) - 1, &counter,
                           sizeof(counter), &size) != MINIJAM_OK)
    counter = 0;
  if (increment > 0 && counter > INT64_MAX - increment)
    counter = INT64_MAX;
  else if (increment < 0 && counter < INT64_MIN - increment)
    counter = INT64_MIN;
  else
    counter += increment;
  (void)minijam_storage_write(COUNTER_KEY, sizeof(COUNTER_KEY) - 1, &counter,
                              sizeof(counter));
  minijam_yield(0, 0);
}
