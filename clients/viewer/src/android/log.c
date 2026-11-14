// SPDX-License-Identifier: GPL-3.0-or-later
// The network core's log on Android goes to logcat, with the tag that the
// app's own lines use. Rust cannot define a C-variadic function, so this
// one stays C. No session logic belongs here.
#include <android/log.h>
#include <stdarg.h>

void viewer_android_log(const char* format, ...) {
    va_list arguments;
    va_start(arguments, format);
    __android_log_vprint(ANDROID_LOG_INFO, "korri-viewer", format, arguments);
    va_end(arguments);
}
