---
id: 01M1PEYVR6GN15CT3Z6876RDW7
slug: free-builder-disk-on-fuji-for-kernel-touching-rg353m-work
title: Free builder disk on fuji for kernel-touching RG353M work
origin: parked
status: To Do
priority: medium
labels:
  - infrastructure
  - builder
  - rg353m
created: 2026-09-04
source: se-work
---

# Free builder disk on fuji for kernel-touching RG353M work

## Why it matters

fuji sits at 89 percent on a 200 GB root with about 24 GB free. An aarch64 kernel build needs roughly 30 GB of scratch and has already failed twice mid-compile, once after 125 minutes. Any future RG353M change that touches kernel config, including rkvdec for HEVC, is blocked behind this. A garbage collect has already been run and reclaimed everything dead, so the remaining space belongs to user data and needs an owner decision rather than an automated sweep.

## Acceptance Criteria

- [ ] fuji has at least 40 GB free before a kernel build starts
- [ ] A full aarch64 kernel build for the RG353M completes without running out of space
- [ ] The chosen reclamation is recorded so it is repeatable

## Notes

Known levers: journalctl --vacuum-size=500M frees about 3.5 GB and is safe. /home/simonwjackson/code holds 44 GB and needs the owner's judgment. Alternatively use a different aarch64 builder. Note that nix store gc deletes flake sources copied for remote builds unless they are pinned with nix-store --add-root, which broke one build in this session.
