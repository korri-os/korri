// SPDX-License-Identifier: GPL-3.0-or-later
// The viewer's own record of each frame, for the outside measure (#435).
// Off unless Rust opened it (picture_frame_times_open); then every call is a
// no-op. A decode unit's record index travels through FFmpeg as the packet's
// and the frame's pts.
#ifndef FRAME_TIMES_H
#define FRAME_TIMES_H
#include <Limelight.h>
#include <stdint.h>

// A decode unit arrived; `data` is its whole buffer chain. Returns the pts to
// give its packet: its record index, or AV_NOPTS_VALUE when no record is kept.
int64_t frame_times_unit(PDECODE_UNIT unit, const void* data, int length);
// The decoder returned the frame of this pts.
void frame_times_decoded(int64_t pts);
// The presenter's call that hands the frame of this pts to the display returned.
void frame_times_displayed(int64_t pts);
#endif
