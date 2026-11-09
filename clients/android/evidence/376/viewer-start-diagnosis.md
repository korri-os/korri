# Aka viewer-start diagnosis (2026-11-08)

The phone's real BotW launch retry at 22:15:49 reached certificate provisioning successfully, then failed at `https-serverinfo-pair-state` at 22:15:56 with `SocketException`. It did not reach HTTPS app-list selection or create the Game Activity.

A separate host probe connected to aka:47984 but timed out during the TLS handshake after three seconds. HTTP serverinfo at aka:47989 responded with `SUNSHINE_SERVER_BUSY`, currentgame `1820356324`. Busy alone does not prove the cause of the TLS failure. No unrelated session was ended.

The `start` and `prepare` behavior is unchanged from baseline `7b9aec440`; the overlay branch changes only Resume and Leave in KorriStreamViewer. Added fixed-stage/exception-class diagnostics preserve the generic public failure and omit addresses, launch IDs, exception messages and certificate material.

Checks: diagnostic Android APK built and installed with `adb install -r`; 14 focused KorriStreamViewer JVM tests passed; `git diff --check` passed. Device retry log: `.cache/overlay-376/launch-diagnosis/diagnostic-viewer-log.txt`.
