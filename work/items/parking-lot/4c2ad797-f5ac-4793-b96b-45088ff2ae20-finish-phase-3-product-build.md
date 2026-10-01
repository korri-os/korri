---
id: 4c2ad797-f5ac-4793-b96b-45088ff2ae20
slug: finish-phase-3-product-build
title: Finish Phase 3 product build
origin: parked
status: To Do
priority: high
labels:
  - product
  - linux
  - plugins
created: 2026-09-23
source: conversation
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  repo: korri
---

# Finish Phase 3 product build

## Why it matters

Phase 3 is not complete. Korri's `integrate/phase3` worktree and the plugin publisher's `feature/sunshine-release` worktree contain uncommitted, unverified work. Device images do not yet own the selected removable plugins. Physical acceptance remains open.

## Acceptance Criteria

- [ ] Complete ticket 15's Sunshine plugin and shared-host deletion in one atomic cut. Verify unit and socket ownership, firewall withdrawal, group and udev-rule cleanup, rollback, and storage reclamation. Record pairing, streaming, selected encoder, CPU load, and temperature on each tested device. A device without the plugin must still boot to the portal, accept input, and reach its local korrid.
- [ ] Pin the verified Korri commit in `korri-os/plugins`. Build and publish the Sunshine package for each supported Nix system with the existing plugin publisher key. Do not sign it with Korri's separate device-cache key.
- [ ] Connect the approved per-device plugin selection and `korri-plugin seed` receipts to fresh images. Verify first-boot approval agreement and root ownership, removal, rollback retention, and eligible storage reclamation. The installed system must not retain plugins after their selection is removed. Existing devices must not gain newly recommended optional defaults through an update.
- [ ] Provide the missing Linux Moonlight plugin and production viewer integration before claiming streamed play. Do not seed SSH or Tailscale. Do not substitute another viewer.
- [ ] Complete ticket 17's power and lid handling, exact-session freeze and wake, and the 900-second clean-shutdown delay for light sleep. All six devices declare no sleep state on day one. No unverified suspend claim or hibernation path is allowed.
- [ ] Measure RG35XXSP display and input facts on hardware. Then complete its product-module import, signed image and closure delivery, and fresh-flash acceptance for screen, buttons, Wi-Fi, audio, and a local game. Record remaining hardware limits explicitly. Do not publish it as supported before physical acceptance.
- [ ] Verify Phase 2 integration, finish implementation before running the automated test set once at the end, fix failures, and update the ticket evidence. Commit and land the work on `main`, then deploy and verify each available target. State any unavailable hardware gate clearly.

## Decisions and limits

Simon removed PSP, DraStic, YabaSanshiro, Dolphin, Azahar, and AetherSX2 from this release's default selection. Do not replace those runners or add compatibility paths. Sunshine's signed plugin release belongs to `korri-os/plugins`. Keep the main checkout's unrelated changes untouched. Never build on a target device or write protected firmware partitions.

## Related

- `.scratch/korri-product-build/issues/15-ship-the-streaming-host-as-a-removable-plugin-and-delete-the-shared-host-composition.md`
- `.scratch/korri-product-build/issues/16-seed-bundled-plugins-into-images-and-select-per-device-emulator-defaults.md`
- `.scratch/korri-product-build/issues/17-declare-sleep-states-handle-power-and-lid-and-freeze-before-suspend.md`
- `.scratch/korri-product-build/issues/18-return-rg35xxsp-on-the-product-module-with-delivery.md`
- `.scratch/consistent-korri-product/evidence/rocknix-recommendations.md`
- `nix/product/plugin-selection.nix` in `integrate/phase3`
