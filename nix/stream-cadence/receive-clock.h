/* Moonlight's receive clock has a process-local epoch and uses MONOTONIC_RAW.
 * Bridge each decode unit to host MONOTONIC without replacing receive time by
 * callback time. The native receive-to-callback delay remains unchanged.
 */
#ifndef RECEIVE_CLOCK_H
#define RECEIVE_CLOCK_H
#include <stdbool.h>
#include <stdint.h>

static bool receive_monotonic_ns(uint64_t before_ns, uint64_t after_ns,
                                 uint64_t native_now_us, uint64_t receive_us,
                                 uint64_t *result) {
    if (after_ns < before_ns || native_now_us < receive_us) return false;
    uint64_t midpoint = before_ns + (after_ns - before_ns) / 2;
    uint64_t delay_us = native_now_us - receive_us;
    if (delay_us > midpoint / 1000) return false;
    *result = midpoint - delay_us * 1000;
    return true;
}
#endif
