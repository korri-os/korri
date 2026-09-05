---
id: 01M1SBJQJN6EBGMS9TRGSM1DV3
slug: diagnose-android-emulator-instrumentation-sigkill-failures
title: Diagnose Android emulator instrumentation SIGKILL failures
origin: parked
status: To Do
priority: medium
labels:
  - android
  - test-reliability
created: 2026-09-05
source: se-work
---

# Diagnose Android emulator instrumentation SIGKILL failures

## Why it matters

Two unchanged API 34 federation acceptance runs lost the test app to Android SIGKILL without an assertion failure. A third unchanged run passed. This unresolved failure makes the acceptance gate unreliable and can hide real regressions behind repeated retries.

## Acceptance Criteria

- [ ] Capture all Android log buffers and application exit reasons for a failing run before emulator cleanup.
- [ ] Identify the action that kills the instrumentation app and distinguish fixture setup from a product defect.
- [ ] Verify a targeted correction without relaxing federation assertions or silently retrying failed tests.

## Related

- `clients/android/federation-acceptance-check.sh`
- `clients/android/bridge-contract-check.sh`
- `clients/android/app/src/androidTest/java/com/limelight/KorriFederationAcceptanceTest.java`
- `docs/research/federation-restoration-brief.md`

## Notes

Observed 2026-09-05. proc_9b64 and proc_f093 failed; proc_b319 passed unchanged in 96 seconds. Both failures show system libprocessgroup killing Korri plus other processes; cause not established. Local artifacts: /tmp/federation-a6-sigkill-634242, /tmp/federation-a6-sigkill-641877. Diagnostic successful run: /tmp/federation-a6-all-buffers.log. Do not describe the instability as fixed.
