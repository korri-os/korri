#include <assert.h>
#include "../receive-clock.h"

int main(void) {
    uint64_t stamp = 0;
    const uint64_t host = UINT64_C(176556000000000);
    assert(receive_monotonic_ns(host, host + 2000, 502000, 483000, &stamp));
    assert(stamp == host + 1000 - UINT64_C(19000000));
    /* Preserve the actual receive delay, not a frame's callback timestamp. */
    assert(host + 1000 - stamp == UINT64_C(19000000));
    assert(!receive_monotonic_ns(host, host - 1, 502000, 483000, &stamp));
    assert(!receive_monotonic_ns(host, host, 483000, 502000, &stamp));
    assert(!receive_monotonic_ns(1, 1, 2, 0, &stamp));
    return 0;
}
