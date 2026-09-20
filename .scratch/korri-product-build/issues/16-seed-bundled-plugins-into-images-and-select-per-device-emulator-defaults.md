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
