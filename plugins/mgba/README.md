# mGBA on Android

This directory owns the pinned mGBA source and build pipeline for **Android**,
plus the Android plugin declaration in `android/plugin.ts`.

The Linux plugin is not here. `@korri:mgba` for Linux is generated from the
catalogue in [`../libretro/cores.nix`](../libretro/cores.nix), together with
every other libretro core. One catalogue, one package per core.

Build the Android core directly with `nix run .#mgba-build`.

## Temporary Android packaging

Android currently requires the core in RetroArch's private executable core
directory. Until a separate authenticated core-import path exists, the
RetroArch APK temporarily carries the built `.so` and installs it there. This
is a packaging bridge only: mGBA remains a separate plugin and owns its source,
build output, system, and runner identity.
