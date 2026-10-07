---
id: 01KVVAD3QZ3H7YCKPBA2ANY4Y8
slug: build-a-nixified-generic-gamemaker-apk-compatibility-layer
title: Build a Nixified generic GameMaker APK compatibility layer
origin: parked
status: To Do
priority: high
labels:
  - gmloader
  - portmaster
  - itchio
  - rg353m
  - nix
  - compatibility
created: 2026-06-23
source: user
context:
  cwd: .
  branch: trunk
  commit: d3d21712
  repo: simonwjackson/korri
  invoked_by: conversation
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/active/01KVVAD3QZ3H7YCKPBA2ANY4Y8-build-a-nixified-generic-gamemaker-apk-compatibility-layer/item.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep the GMLoader compatibility findings for RG353M. This is a Linux SDL/GLES runtime that consumes GameMaker APK payloads, not restoration of Android support. The current published output map has no GMLoader package.

## Scope on main

Preserve the observed payload and ABI distinctions. Native dependencies must arrive prebuilt. Plugin source declares preparation and launch work; korrid owns effects. Do not treat the old compatibility-profile proposal as a current schema decision.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

The legacy evidence is rendering evidence, not proof of working controls for every APK. Licensing, armhf support, and runner compatibility remain separate limits.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Build a Nixified generic GameMaker APK compatibility layer

## Why it matters

The RG353M spike showed native GMLoader performance is dramatically more promising than Chromium/HTML5 for GameMaker titles, but arbitrary itch.io APKs do not work blindly. Capturing a reusable compatibility layer would turn per-game discoveries into shared transforms so future GameMaker APK/PortMaster work compounds instead of repeating manual extraction, repacking, ABI checks, and launch debugging.

## Acceptance Criteria

- [ ] A standalone Nix package exposes a reusable GMLoader Next runtime instead of each game package owning a bundled runner.
- [ ] A Nix/package helper or CLI accepts a GameMaker Android APK and emits a normalized run directory or derivation containing game.port, gmloader.json, extracted libyoyo.so/support libs, and a compatibility-profile.json.
- [ ] The APK prep path detects and reports: not GameMaker, arm64 GameMaker, 32-bit-only GameMaker, compressed assets/game.droid requiring stored repack, missing libyoyo.so, and likely runner/API incompatibility.
- [ ] Known reusable quirks from the RG353M spike are centralized as transforms: store/repack assets/game.droid, extract lib/<abi>/libyoyo.so, seed Android shim libs, choose SDL audio backend, choose force_platform, and attach an input profile.
- [ ] The compatibility layer supports or explicitly rejects 32-bit-only APKs; if unsupported, it records that armhf/32-bit runtime work is required.
- [ ] At least five GameMaker titles are validated to reach a visible title/menu/game screen with acceptable apparent frame rate on RG353M, even if controller/touch input remains unresolved.
- [ ] A compatibility matrix records each attempted title, source URL, ABI, GameMaker payload shape, transforms applied, launch result, performance impression, input status, audio status, and reason for rejection/failure.

## Related

- `.worktrees/spike/gmloader-nix/product/plugins/portmaster/packages/gmloader-port/default.nix`
- `.worktrees/spike/gmloader-nix/product/plugins/portmaster/packages/gmloader-port/check.nix`
- `.worktrees/spike/gmloader-nix/product/plugins/portmaster/packages/portmaster-stargrove-scramble-gmloader/default.nix`
- `.worktrees/spike/gmloader-nix/product/plugins/portmaster/packages/portmaster-spelunky-gmloader/default.nix`
- `/tmp/inspect-itch-download.py`
- `/tmp/fetch-itch-upload-url.py`

## Notes

Conversation context: HTML5/Chromium Stargrove was too slow on RG353M even with gamescope/pixel scaling, while native GMLoader/PortMaster Stargrove and Spelunky rendered at promising speed. The spike then tested itch.io APK intake. Useful reusable findings: APKs are containers for assets/game.droid plus lib/<abi>/libyoyo.so; GMLoader is not Android emulation, it loads libyoyo.so and shims enough Android/JNI while using Linux SDL/GLES. Entry point discovery is generic, but compatibility is not guaranteed. PortMaster GMLoader ports are high-confidence because they are curated. itch APKs split into classes: Sacrificio Inc. arm64 GameMaker loaded and rendered after repacking game.droid stored/uncompressed and seeding shim libs; Spelunky Classic HD itch APK had arm64 libyoyo but failed due Android asset-manager expectations; Sokoban Land DX, WILOO, and QLRZ were GameMaker but 32-bit-only; Digimon Digital Heroes was Android/Cordova rather than GameMaker. Remap is installed after upgrade and GMLoader can see Korri Remap Gamepad, but controls/touch did not work for Sacrificio yet. Audio was disabled with SDL_AUDIODRIVER=dummy during remap-runner tests because Pulse access under the isolated runner blocked SDL init. Current spike code packages PortMaster zips, not yet a standalone generic GMLoader runtime.
