// SPDX-License-Identifier: Apache-2.0
#include <minijam/minijam.h>
#include <stdint.h>

extern "C" MINIJAM_REFINE {
  int64_t increment = 0;
  size_t size = 0;
  if (minijam_payload(&increment, sizeof(increment), &size) == MINIJAM_OK &&
      size == sizeof(increment))
    return minijam_refine_ok(&increment, sizeof(increment));
  else
    return minijam_refine_error(1);
}

extern "C" MINIJAM_ACCUMULATE { minijam_yield(nullptr, 0); }
