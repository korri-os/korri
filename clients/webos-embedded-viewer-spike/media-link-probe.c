// Workstation link probe. Never initializes media, connects, pairs or plays.
#include <stdio.h>
#include <Limelight.h>
#include <ss4s.h>
#include <ss4s/modapi.h>

extern bool SS4S_ModuleOpen_SMP_WEBOS3(SS4S_Module *, const SS4S_LibraryContext *);

int main(void) {
    STREAM_CONFIGURATION config;
    LiInitializeStreamConfiguration(&config);
    // Retain the real module dependency without initializing vendor media APIs.
    bool (*volatile entry)(SS4S_Module *, const SS4S_LibraryContext *) =
        SS4S_ModuleOpen_SMP_WEBOS3;
    if (entry == NULL || SS4S_GetVideoModuleName() != NULL) return 1;
    puts("link probe only: common-c and SMP webOS 3; no playback");
    return 0;
}
