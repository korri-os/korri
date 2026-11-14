// SPDX-License-Identifier: GPL-3.0-or-later
// The viewer's own record of each frame, for the outside measure (#435).
//
// One CSV row for each decode unit, in index order:
//   index,frame_number,enqueue_ns,decoded_ns,display_ns
// - index: the unit's place in the stream as it arrived, and in the recording.
// - frame_number: DECODE_UNIT.frameNumber, the stream's frame index (the
//   stage-0 receive record's field of that name).
// - enqueue_ns: DECODE_UNIT.enqueueTimeUs, when the network core had the whole
//   frame: about when its last packet arrived (Limelight.h).
// - decoded_ns: the decoder returned the frame. Empty when it returned none.
//   With VA-API the GPU can still be decoding; presentation waits for it.
// - display_ns: the presenter's call that hands the frame to the display
//   returned (vaPutSurface, SDL_RenderPresent). Empty for a frame that a newer
//   one replaced, or that was never presented.
// Every time is CLOCK_MONOTONIC in nanoseconds, the clock of the measure's
// display record. The recording holds each unit's bytes in the same order,
// as the stage-0 shim's CADENCE_DUMP does.
#include "bridge.h"
#include "frame_times.h"
#include <errno.h>
#include <inttypes.h>
#include <libavutil/avutil.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

// Units between arrival and their row. A presenter keeps at most the newest
// one or two frames, so a unit this far back was not shown.
#define SLOTS 256

struct slot {
  int frame_number;
  uint64_t enqueue_ns;
  uint64_t decoded_ns; // 0: the decoder returned no frame for this unit
};

static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static FILE* times;
static int recording = -1;
static int64_t received; // units that arrived: the next index
static int64_t written;  // every lower index has its row
static struct slot slots[SLOTS];
// Set by any thread; the recording's writer reads it without the lock.
static atomic_bool failed;

static uint64_t monotonic_ns(void) {
  struct timespec now;
  clock_gettime(CLOCK_MONOTONIC, &now);
  return (uint64_t)now.tv_sec * UINT64_C(1000000000) + (uint64_t)now.tv_nsec;
}

// The record stops at its first fault; close reports it.
static void fail(const char* what) {
  if (!atomic_exchange(&failed, true))
    fprintf(stderr, "frame record: %s: %s\n", what, strerror(errno));
}

// Under lock: the row of the oldest unwritten unit, with its hand-off or none.
static void write_row(uint64_t display_ns) {
  struct slot* unit = &slots[written % SLOTS];
  char decoded[24] = "", display[24] = "";
  if (unit->decoded_ns) snprintf(decoded, sizeof decoded, "%" PRIu64, unit->decoded_ns);
  if (display_ns) snprintf(display, sizeof display, "%" PRIu64, display_ns);
  if (!failed && fprintf(times, "%" PRId64 ",%d,%" PRIu64 ",%s,%s\n", written,
                         unit->frame_number, unit->enqueue_ns, decoded, display) < 0)
    fail("cannot write the frame times");
  ++written;
}

static void record(const char* data, size_t length) {
  while (length && !failed) {
    ssize_t count = write(recording, data, length);
    if (count < 0 && errno == EINTR) continue;
    if (count < 0) {
      fail("cannot write the recording");
      return;
    }
    data += count;
    length -= (size_t)count;
  }
}

int picture_frame_times_open(int times_fd, int recording_fd) {
  // Both descriptors belong to this module from here on. Before the
  // connection starts: no other thread runs here yet.
  received = written = 0;
  atomic_store(&failed, false);
  recording = recording_fd;
  if (times_fd < 0) return 0;
  times = fdopen(times_fd, "w");
  if (!times) close(times_fd);
  // One write for many rows: the record must cost the stream nothing.
  if (!times || setvbuf(times, NULL, _IOFBF, 1 << 16) ||
      fputs("index,frame_number,enqueue_ns,decoded_ns,display_ns\n", times) < 0) {
    fail("cannot open the frame times");
    picture_frame_times_close();
    return -1;
  }
  return 0;
}

int64_t frame_times_unit(PDECODE_UNIT unit, const void* data, int length) {
  if (recording >= 0) record(data, (size_t)length);
  if (!times) return AV_NOPTS_VALUE;
  // The core's clock is its own: microseconds of MONOTONIC_RAW since the
  // process first asked. Carry the enqueue time to CLOCK_MONOTONIC through
  // the short delay since then, as the stage-0 shim does for receive time.
  uint64_t before = monotonic_ns();
  uint64_t native_us = LiGetMicroseconds();
  uint64_t after = monotonic_ns();
  pthread_mutex_lock(&lock);
  if (native_us < unit->enqueueTimeUs) {
    errno = ERANGE;
    fail("the core's enqueue time is later than its clock");
  }
  int64_t index = received++;
  if (index - written >= SLOTS) write_row(0);
  slots[index % SLOTS] = (struct slot){
    .frame_number = unit->frameNumber,
    .enqueue_ns = before + (after - before) / 2 - (native_us - unit->enqueueTimeUs) * 1000,
  };
  pthread_mutex_unlock(&lock);
  return index;
}

void frame_times_decoded(int64_t pts) {
  if (!times || pts == AV_NOPTS_VALUE) return;
  uint64_t now = monotonic_ns();
  pthread_mutex_lock(&lock);
  if (pts >= written && pts < received) slots[pts % SLOTS].decoded_ns = now;
  pthread_mutex_unlock(&lock);
}

void frame_times_displayed(int64_t pts) {
  if (!times || pts == AV_NOPTS_VALUE) return;
  uint64_t now = monotonic_ns();
  pthread_mutex_lock(&lock);
  // Frames reach the display in stream order, so no older unit can follow.
  if (pts >= written && pts < received) {
    while (written < pts) write_row(0);
    write_row(now);
  }
  pthread_mutex_unlock(&lock);
}

int picture_frame_times_close(void) {
  // The core has joined its threads and no presenter runs any more.
  pthread_mutex_lock(&lock);
  if (times) {
    while (written < received) write_row(0);
    if (fclose(times)) fail("cannot write the frame times");
    times = NULL;
  }
  if (recording >= 0) {
    if (close(recording)) fail("cannot write the recording");
    recording = -1;
  }
  int result = atomic_load(&failed) ? -1 : 0;
  pthread_mutex_unlock(&lock);
  return result;
}
