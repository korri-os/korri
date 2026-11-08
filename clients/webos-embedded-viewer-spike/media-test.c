// Experiment 2 only: local Annex-B video, no audio, identity or network client.
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <signal.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include <ss4s.h>

#ifndef MEDIA_DRIVER
#define MEDIA_DRIVER "ndl-webos4"
#endif

#define FRAME_COUNT 300
#define MAX_STREAM_BYTES (10 * 1024 * 1024)

static volatile sig_atomic_t interrupted;
static atomic_bool media_error;

typedef struct {
  unsigned char *bytes;
  size_t offsets[FRAME_COUNT + 1];
  bool keyframes[FRAME_COUNT];
} Stream;

static int64_t monotonic_ns(void) {
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
    perror("clock_gettime");
    exit(1);
  }
  return (int64_t)now.tv_sec * 1000000000 + now.tv_nsec;
}

static void log_line(SS4S_LogLevel level, const char *tag, const char *format,
                     ...) {
  // Vendor callbacks can arrive on worker threads. Never close from a callback.
  if (level <= SS4S_LogLevelWarn) {
    atomic_store(&media_error, true);
  }
  int64_t now = monotonic_ns();
  flockfile(stderr);
  fprintf(stderr, "[%lld.%03lld] %s: ", (long long)(now / 1000000000),
          (long long)(now / 1000000 % 1000), tag);
  va_list args;
  va_start(args, format);
  vfprintf(stderr, format, args);
  va_end(args);
  fputc('\n', stderr);
  funlockfile(stderr);
}

static void on_signal(int sig) { interrupted = sig; }

static size_t start_code(const unsigned char *p, size_t size) {
  if (size >= 4 && p[0] == 0 && p[1] == 0 && p[2] == 0 && p[3] == 1)
    return 4;
  if (size >= 3 && p[0] == 0 && p[1] == 0 && p[2] == 1)
    return 3;
  return 0;
}

static bool read_stream(const char *path, Stream *stream) {
  FILE *file = fopen(path, "rb");
  if (file == NULL) {
    perror(path);
    return false;
  }
  stream->bytes = malloc(MAX_STREAM_BYTES + 1);
  if (stream->bytes == NULL) {
    fclose(file);
    return false;
  }
  size_t size = fread(stream->bytes, 1, MAX_STREAM_BYTES + 1, file);
  bool valid = !ferror(file) && size > 0 && size <= MAX_STREAM_BYTES;
  if (fclose(file) != 0)
    valid = false;
  if (!valid)
    return false;

  // The Nix producer emits one slice/frame and AUD before every access unit.
  // This intentionally accepts only that 10 s fixture, not arbitrary H.264.
  size_t frames = 0, slices = 0;
  bool sps = false, pps = false;
  for (size_t i = 0; i < size;) {
    size_t prefix = start_code(stream->bytes + i, size - i);
    if (prefix == 0) {
      if (frames == 0)
        return false;
      ++i;
      continue;
    }
    if (i + prefix >= size)
      return false;
    unsigned char header = stream->bytes[i + prefix];
    if (header & 0x80)
      return false;
    int type = header & 31;
    if (type == 9) {
      if (frames == FRAME_COUNT || (frames > 0 && slices != 1))
        return false;
      stream->offsets[frames++] = i;
      slices = 0;
      sps = pps = false;
    } else if (frames == 0) {
      return false;
    } else if (type == 7) {
      sps = true;
    } else if (type == 8) {
      pps = true;
    } else if (type == 1 || type == 5) {
      if (type == 5 && (!sps || !pps))
        return false;
      stream->keyframes[frames - 1] = type == 5;
      ++slices;
    }
    i += prefix + 1;
  }
  if (frames != FRAME_COUNT || slices != 1 || !stream->keyframes[0])
    return false;
  stream->offsets[frames] = size;
  log_line(SS4S_LogLevelInfo, "stream", "%zu access units, %zu bytes", frames,
           size);
  return true;
}

static bool sleep_until(int64_t target) {
  struct timespec deadline = {.tv_sec = target / 1000000000,
                              .tv_nsec = target % 1000000000};
  while (!interrupted && !atomic_load(&media_error)) {
    int error =
        clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &deadline, NULL);
    if (error == 0)
      return true;
    if (error != EINTR) {
      log_line(SS4S_LogLevelError, "clock", "sleep failed: %s",
               strerror(error));
      return false;
    }
  }
  return false;
}

int main(int argc, char **argv) {
  bool check_only = argc == 3 && strcmp(argv[1], "--check-stream") == 0;
  char *end = NULL;
  errno = 0;
  long seconds = argc == 3 && !check_only ? strtol(argv[1], &end, 10) : 0;
  if (argc != 3 ||
      (!check_only && (errno != 0 || end == argv[1] || *end != '\0' ||
                       seconds < 1 || seconds > 25))) {
    fprintf(stderr,
            "usage: %s SECONDS(1..25) FILE.h264 | --check-stream FILE.h264\n",
            argv[0]);
    return 2;
  }
  Stream stream = {0};
  if (!read_stream(argv[2], &stream)) {
    log_line(SS4S_LogLevelError, "stream",
             "invalid fixture (expected 300 AUD-delimited frames)");
    free(stream.bytes);
    return 1;
  }
  if (check_only) {
    free(stream.bytes);
    return 0;
  }
  const char *app_id = getenv("APPID");
  if (geteuid() == 0 || app_id == NULL ||
      strcmp(app_id, "org.korri.shell") != 0) {
    log_line(SS4S_LogLevelError, "jail",
             "requires non-root app UID and APPID=org.korri.shell");
    free(stream.bytes);
    return 2;
  }
  struct sigaction action = {.sa_handler = on_signal};
  sigemptyset(&action.sa_mask);
  if (sigaction(SIGTERM, &action, NULL) || sigaction(SIGINT, &action, NULL) ||
      sigaction(SIGHUP, &action, NULL) || sigaction(SIGALRM, &action, NULL)) {
    perror("sigaction");
    free(stream.bytes);
    return 1;
  }

  // Bound the experiment; the handler requests owning-thread cleanup.
  alarm(25);

  int status = 1;
  SS4S_Player *player = NULL;
  bool video_attempted = false;
  unsigned int fed = 0;
  SS4S_SetLoggingFunction(log_line);
  log_line(SS4S_LogLevelInfo, "stage", "module load begin (%s)", MEDIA_DRIVER);
  if (interrupted ||
      !SS4S_ModuleAvailable(MEDIA_DRIVER, SS4S_MODULE_CHECK_VIDEO))
    goto cleanup;
  SS4S_Config config = {.videoDriver = MEDIA_DRIVER,
                        .loggingFunction = log_line};
  if (SS4S_Init(argc, argv, &config) != 0 ||
      SS4S_GetVideoModuleName() == NULL || SS4S_PostInit(argc, argv) != 0 ||
      atomic_load(&media_error))
    goto cleanup;
  log_line(SS4S_LogLevelInfo, "stage", "module load complete");
  if (interrupted)
    goto cleanup;
  player = SS4S_PlayerOpen();
  if (player == NULL || interrupted || atomic_load(&media_error))
    goto cleanup;
  SS4S_PlayerSetViewportSize(player, 1920, 1080);
  SS4S_VideoInfo info = {.codec = SS4S_VIDEO_H264,
                         .width = 1920,
                         .height = 1080,
                         .frameRateNumerator = 30,
                         .frameRateDenominator = 1};
  log_line(SS4S_LogLevelInfo, "stage",
           "open begin (1920x1080 H264 30fps, video only)");
  // Even a failed open initializes ss4s statistics and may create an instance.
  video_attempted = true;
  if (SS4S_PlayerVideoOpen(player, &info) != SS4S_VIDEO_OPEN_OK ||
      atomic_load(&media_error))
    goto cleanup;
  log_line(SS4S_LogLevelInfo, "stage",
           "open accepted (not proof of visible video)");
  int64_t start = monotonic_ns();
  for (unsigned int frame = 0; frame < (unsigned long)seconds * 30; ++frame) {
    int64_t deadline = start + (int64_t)frame * 1000000000 / 30;
    if (!sleep_until(deadline))
      goto cleanup;
    if (monotonic_ns() - deadline > 250000000) {
      log_line(SS4S_LogLevelError, "feed",
               "more than 250 ms late; stop, do not burst-feed");
      goto cleanup;
    }
    size_t index = frame % FRAME_COUNT;
    SS4S_VideoFeedFlags flags =
        SS4S_VIDEO_FEED_DATA_FRAME_START | SS4S_VIDEO_FEED_DATA_FRAME_END;
    if (stream.keyframes[index])
      flags |= SS4S_VIDEO_FEED_DATA_KEYFRAME;
    SS4S_VideoFeedResult result = SS4S_PlayerVideoFeed(
        player, stream.bytes + stream.offsets[index],
        stream.offsets[index + 1] - stream.offsets[index], flags);
    if (result != SS4S_VIDEO_FEED_OK) {
      log_line(SS4S_LogLevelError, "feed",
               "frame=%u result=%d; stop without retry/reload", frame, result);
      goto cleanup;
    }
    if (++fed == 1)
      log_line(SS4S_LogLevelInfo, "stage",
               "first frame fed (not proof of hardware presentation)");
  }
  if (sleep_until(start + (int64_t)seconds * 1000000000))
    status = 0;

cleanup:
  log_line(SS4S_LogLevelInfo, "stage",
           "close begin; frames=%u signal=%d error=%d", fed, (int)interrupted,
           atomic_load(&media_error));
  if (player != NULL) {
    if (video_attempted)
      SS4S_PlayerVideoClose(player);
    SS4S_PlayerClose(player);
  }
  SS4S_Quit();
  alarm(0);
  free(stream.bytes);
  log_line(SS4S_LogLevelInfo, "stage",
           "close complete (API calls returned; verify plane release on TV)");
  if (interrupted)
    return 128 + interrupted;
  return atomic_load(&media_error) ? 1 : status;
}
