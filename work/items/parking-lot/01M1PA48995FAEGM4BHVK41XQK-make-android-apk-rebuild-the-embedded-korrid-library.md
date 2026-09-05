---
id: 01M1PA48995FAEGM4BHVK41XQK
slug: make-android-apk-rebuild-the-embedded-korrid-library
title: Make android-apk rebuild the embedded korrid library
origin: parked
status: To Do
priority: high
labels:
  - android
  - build
  - jni
created: 2026-09-04
source: user
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  commit: 388352e6ea66bc9fe22b6bcd1e97bad86571eea4
  repo: korri
---

# Make android-apk rebuild the embedded korrid library

## Why it matters

`nix run .#android-apk` packaged an August 21 `libkorrid.so` with current Java sources. The installed app crashed with `UnsatisfiedLinkError` for `KorridServer.moonlightHostCandidates()`. A normal APK build can therefore pass while shipping an incompatible JNI treaty.

## Acceptance Criteria

- [ ] `nix run .#android-apk` rebuilds or consumes a revision-matched arm64 `libkorrid.so`.
- [ ] A check rejects an APK when a declared `KorridServer` native method has no matching JNI symbol.
- [ ] A clean current-main APK launches `KorriShellActivity` without `UnsatisfiedLinkError`.

## Related

- `nix/tasks.nix`
- `services/korrid/check-in-shell.sh`
- `clients/android/app/src/main/java/com/simonwjackson/korri/korrid/KorridServer.java`

## Notes

Observed during Bandai first-install acceptance on 2026-09-04. `nix run .#korrid-check` is the current path that rebuilds the cdylib before assembling the APK.
