# Seed bundled plugins into images and select per-device emulator defaults

Status: ready-for-agent
Blocked by: 03, Phase 2 gate

## What to build

Make a fresh image start with the approved removable plugin selection for its device. Seed real plugin packages and approvals without letting the system image keep them alive after installation.

## Acceptance criteria

- [ ] The image producer derives seeded receipts and packages from the existing `korri-plugin seed` path and its tests. It does not create a second receipt format or image schema.
- [ ] On first boot, the plugin host re-derives the same approval digest as the image producer for every seeded plugin.
- [ ] After seeding, plugin selections alone own bundled plugins. The system image keeps no reference that prevents uninstall from reclaiming eligible storage.
- [ ] Each of the six device models receives exactly the curated emulator selection recorded in `.scratch/consistent-korri-product/evidence/rocknix-recommendations.md`.
- [ ] Missing native packages, integration, BIOS handling, content discovery, or redistribution rights are completed or reported as blockers. No selected runner is silently replaced.
- [ ] The user resolves the standalone/libretro PPSSPP identity conflict before PSP remains in a default selection. The implementation does not invent a second identity.
- [ ] Moonlight is seeded as a removable default plugin for this release where the approved selection requires it. SSH and Tailscale are not preinstalled.
- [ ] Content-acquisition providers are not ported or seeded by this ticket.
- [ ] Existing devices do not receive newly recommended optional defaults through a system update. Fresh installations receive the full selected defaults.
- [ ] Image and VM tests cover receipt agreement, first-boot ownership, removal of a seeded plugin, rollback retention, and storage reclamation.
- [ ] Native programs arrive prebuilt. No target device compiles software.

## PSP decision, 2026-09-22

Simon chose to ignore PSP for this release. Omit PPSSPP from every default selection. Do not resolve the standalone/libretro identity conflict, install either package by default, or introduce a second identity. This narrows the earlier ROCKNIX selection; it does not select another PSP runner. PSP remains available for a later decision. Simon also omitted DraStic, YabaSanshiro, Dolphin, Azahar, and AetherSX2 from this release's defaults rather than replace their missing native plugins with different runners.

The current image builder has `sdImage.storePaths` and `sdImage.populateRootCommands`; the existing `korri-plugin seed` command produces the actual receipt. `services/korrid/plugin-host/image-seed.nix` now produces that image builder fragment from real plugin packages, with a check written for the existing SSH package. No device image consumes it yet, and no image owns plugin selection. The image builder creates receipt files under a build-user UID. The plugin-host module now uses boot-time `systemd-tmpfiles` `Z` rules to give its private receipt and GC-root trees root ownership before restore, without changing modes. A VM case checks this with UID 12345 and checks that GC-root symlinks are not followed. These new checks have not run yet. `nix/product/plugin-selection.nix` now records the exact selected package outputs per device, with the user-removed defaults omitted. No image consumes that list yet. The curated libretro producers live in `korri-os/plugins`, whose local `main` at `7f4eae0c` is four commits ahead of its remote. Simon chose that repository to publish Sunshine too. Its isolated `feature/sunshine-release` worktree now exports Korri's unmodified Sunshine package and adds it to the signed release workflow; its Korri lock still needs the final verified core commit. The user removed the five missing native runners from this release's default selection. The selected libretro producers exist only in the local `korri-os/plugins` tree at `7f4eae0c`; `origin/main` is `74a9d8c2` and lacks those four later commits. No first-party Linux Moonlight plugin producer exists in core or that local plugin tree. `services/korrid/src/linux_viewer.rs` explicitly leaves the production systemd/Moonlight adapter for a later slice, so merely packaging a Moonlight binary would not establish a playable route. Do not ship unsigned default receipts or silently substitute a different stream viewer. This ticket is not complete.
