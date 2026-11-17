// SPDX-License-Identifier: GPL-3.0-or-later
// The network core's log callback. Rust cannot define a C-variadic function,
// so this one stays C. It writes the core's line where it always went:
// logcat on Android, with the tag of the app's own lines, and standard output
// elsewhere, as Moonlight does. Then it hands the line to Rust's hook, which
// marks the start's step that the line names (`marks.rs`). No session logic
// belongs here.
#include <stdarg.h>
#include <stdio.h>
#ifdef __ANDROID__
#include <android/log.h>
#endif

static void (*hook)(const char* text);

void viewer_core_log_hook(void (*marks)(const char* text)) {
  hook = marks;
}

void viewer_core_log(const char* format, ...) {
  // Logcat's own limit for one line.
  char text[4096];
  va_list arguments;
  va_start(arguments, format);
  vsnprintf(text, sizeof text, format, arguments);
  va_end(arguments);
#ifdef __ANDROID__
  __android_log_write(ANDROID_LOG_INFO, "korri-viewer", text);
#else
  fputs(text, stdout);
  fflush(stdout);
#endif
  if (hook) hook(text);
}
