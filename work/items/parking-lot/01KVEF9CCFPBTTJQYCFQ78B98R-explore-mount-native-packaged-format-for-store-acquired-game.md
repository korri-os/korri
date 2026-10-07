---
id: 01KVEF9CCFPBTTJQYCFQ78B98R
slug: explore-mount-native-packaged-format-for-store-acquired-game
title: Explore mount-native packaged format for store-acquired game payloads
origin: parked
status: To Do
priority: medium
labels:
  - itchio
  - acquisition
  - exploration
  - mounts
  - handheld-performance
created: 2026-06-18
source: user
context:
  cwd: .worktrees/feat/itchio-public-provider
  branch: feat/itchio-public-provider
  commit: 7768feca
  repo: simonwjackson/korri
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KVEF9CCFPBTTJQYCFQ78B98R-explore-mount-native-packaged-format-for-store-acquired-game.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep the measured-format question for real Butler-installed itch.io payloads. This is exploration, not selection of squashfs, erofs, or tar.gz. It also covers the overlapping ROM-like packaged artifact question in 01KVEF94FNN205AHATC2GH3QEJ.

## Scope on main

Compare against an actual acquired game and current host-owned installation. Keep saves separate only after inspecting the existing game and storage producers. No new package or mount schema is approved by this import.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

Mount support, launch latency, memory use, updates, and writable game state can make a single-file package worse than an unpacked install.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Explore mount-native packaged format for store-acquired game payloads

## Why it matters

A mount-native image such as squashfs or erofs might preserve the ROM-like single-file store payload idea while avoiding tar.gz extraction latency and memory pressure, but handheld targets like RG353M may not have enough RAM/CPU/kernel support for a good experience.

## Acceptance Criteria

- [ ] Prototype at least one mount-native format for a Butler-installed itch.io payload and measure launch/setup latency versus tar.gz extraction and unpacked installs.
- [ ] Validate feasibility on low-resource handheld assumptions, including RG353M-class memory, CPU, kernel module/filesystem support, and read amplification.
- [ ] Define whether saves/config should live outside the mounted payload and how overlays would work.
- [ ] Recommend a default format or explain why mount-native packaged artifacts should not be pursued.

## Related

- `product/platform/acquisition/plugins/itchio.ts`
- `docs/acceptance/itchio-public-provider.md`
- `product/systems/nixos`
