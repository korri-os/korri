---
id: 01M0KXMKBRA96ZFDP1ZB5SFC8V
slug: bake-the-immersive-shell-fix-into-the-odin-launcher-image
title: Bake the immersive shell fix into the Odin launcher image
origin: parked
status: To Do
priority: medium
labels:
  - android
  - odin2portal
  - firmware
  - shell
created: 2026-08-22
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  commit: bf67948460c6
  repo: korri
  invoked_by: se-work
---

# Bake the immersive shell fix into the Odin launcher image

## Why it matters

The edge-to-edge shell currently runs on device `ef201f64` only as a `/data` update that shadows the flashed product build. The image and its pinned hashes still describe the older non-immersive APK, and `launcher-device-acceptance.sh` fails while the shadow is installed. A reboot keeps the update, but any stock rollback or reflash silently reverts the behavior.

## Acceptance Criteria

- [ ] A release APK containing commit bf679484 is pinned in `contract/korri-launcher-apk-SHA256.txt`.
- [ ] A regenerated launcher image is pinned in `contract/launcher-install-SHA256SUMS` and passes install readiness.
- [ ] The `/data` update is removed so `pm path` returns only `/product/app/Korri/Korri.apk`.
- [ ] `odin2portal-launcher-device-acceptance` passes again with Korri as HOME.

## Related

- `clients/android/app/src/main/java/com/limelight/KorriShellActivity.java`
- `clients/android/firmware/odin2portal/contract/korri-launcher-apk-SHA256.txt`
- `clients/android/firmware/odin2portal/INSTALL.md`
