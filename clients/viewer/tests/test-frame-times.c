// SPDX-License-Identifier: GPL-3.0-or-later
// The frame record's contract: one row for each unit, in index order, with
// the hand-off only for the units that a presenter handed to the display,
// and the recording byte for byte. Real files, the real core clock.
#include "bridge.h"
#include "frame_times.h"
#include <assert.h>
#include <fcntl.h>
#include <inttypes.h>
#include <libavutil/avutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

// The standalone presenter check observes the same callback Rust owns in
// the program. A record is optional; the lifecycle fact is not.
static int first_frames;
void korri_viewer_first_frame(void) { ++first_frames; }

static int64_t arrive(int frame_number, const char* bytes) {
  LENTRY entry = {.data = (char*)bytes, .length = (int)strlen(bytes)};
  DECODE_UNIT unit = {.frameNumber = frame_number, .bufferList = &entry,
                      .fullLength = entry.length, .enqueueTimeUs = LiGetMicroseconds()};
  usleep(1000);
  return frame_times_unit(&unit, bytes, entry.length);
}

static double monotonic_ms(void) {
  struct timespec now;
  clock_gettime(CLOCK_MONOTONIC, &now);
  return (double)now.tv_sec * 1e3 + (double)now.tv_nsec / 1e6;
}

static char* contents(const char* path) {
  static char text[1 << 16];
  FILE* file = fopen(path, "r");
  assert(file);
  size_t length = fread(text, 1, sizeof text - 1, file);
  text[length] = 0;
  fclose(file);
  return text;
}

int main(void) {
  const char* directory = getenv("TMPDIR") ? getenv("TMPDIR") : "/tmp";
  char times[4096], recording[4096];
  snprintf(times, sizeof times, "%s/frame-times-XXXXXX", directory);
  snprintf(recording, sizeof recording, "%s/frame-recording-XXXXXX", directory);
  int times_fd = mkstemp(times), recording_fd = mkstemp(recording);
  assert(times_fd >= 0 && recording_fd >= 0);

  // Off: no record, and the decoder gets no pts. The first frame on the
  // display still writes the start's last mark (#391), once, in the form of
  // the viewer's own marks (src/marks.rs), on CLOCK_MONOTONIC.
  char marks[4096];
  snprintf(marks, sizeof marks, "%s/frame-marks-XXXXXX", directory);
  int marks_fd = mkstemp(marks), saved = dup(2);
  assert(marks_fd >= 0 && saved >= 0 && dup2(marks_fd, 2) == 2);
  LENTRY entry = {.data = "x", .length = 1};
  DECODE_UNIT unit = {.bufferList = &entry, .fullLength = 1};
  assert(frame_times_unit(&unit, "x", 1) == AV_NOPTS_VALUE);
  double before = monotonic_ms();
  frame_times_displayed(AV_NOPTS_VALUE);
  double after = monotonic_ms();
  frame_times_displayed(AV_NOPTS_VALUE);
  assert(first_frames == 1);
  assert(picture_frame_times_close() == 0);
  fflush(stderr);
  assert(dup2(saved, 2) == 2);
  close(saved);
  close(marks_fd);
  double shown = 0;
  int read = 0;
  const char* mark = contents(marks);
  assert(sscanf(mark, "start: first frame shown at %lf ms\n%n", &shown, &read) == 1);
  assert(read > 0 && mark[read] == 0);
  // The line rounds to the microsecond.
  assert(before - 0.001 <= shown && shown <= after + 0.001);
  unlink(marks);

  assert(picture_frame_times_open(times_fd, recording_fd) == 0);
  assert(arrive(7, "aa") == 0);
  frame_times_decoded(0);
  assert(arrive(8, "bbb") == 1);
  frame_times_decoded(1);
  assert(arrive(9, "c") == 2); // the decoder returned no frame for this unit
  assert(arrive(10, "dddd") == 3);
  frame_times_decoded(3);
  // A newer frame replaced units 1 and 2 before the presenter took one.
  frame_times_displayed(0);
  frame_times_displayed(3);
  frame_times_displayed(1); // older than a shown frame: no second row
  assert(arrive(11, "e") == 4);
  frame_times_decoded(4);
  assert(picture_frame_times_close() == 0);

  assert(first_frames == 1);
  assert(strcmp(contents(recording), "aabbbcdddde") == 0);
  char* line = contents(times);
  assert(strncmp(line, "index,frame_number,enqueue_ns,decoded_ns,display_ns\n", 52) == 0);
  line = strchr(line, '\n') + 1;
  const struct { int frame_number; int decoded, displayed; } expected[] = {
      {7, 1, 1}, {8, 1, 0}, {9, 0, 0}, {10, 1, 1}, {11, 1, 0}};
  for (int index = 0; index < 5; ++index) {
    int64_t row;
    int frame_number, used = 0;
    uint64_t enqueue, decoded = 0, display = 0;
    assert(sscanf(line, "%" SCNd64 ",%d,%" SCNu64 ",%n", &row, &frame_number, &enqueue, &used) == 3);
    assert(row == index && frame_number == expected[index].frame_number && enqueue > 0);
    line += used;
    if (expected[index].decoded) {
      assert(sscanf(line, "%" SCNu64 "%n", &decoded, &used) == 1);
      assert(decoded >= enqueue);
      line += used;
    }
    assert(*line++ == ',');
    if (expected[index].displayed) {
      assert(sscanf(line, "%" SCNu64 "%n", &display, &used) == 1);
      assert(display >= decoded);
      line += used;
    }
    assert(*line++ == '\n');
  }
  assert(*line == 0);

  // A closed record starts anew, here with neither file.
  assert(picture_frame_times_open(-1, -1) == 0);
  assert(arrive(1, "z") == AV_NOPTS_VALUE);
  assert(picture_frame_times_close() == 0);
  unlink(times);
  unlink(recording);
  puts("frame record: rows in index order, hand-offs only for shown frames, recording intact, the first frame's start mark once");
}
