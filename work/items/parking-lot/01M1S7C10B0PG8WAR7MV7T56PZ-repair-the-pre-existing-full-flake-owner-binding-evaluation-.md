---
id: 01M1S7C10B0PG8WAR7MV7T56PZ
slug: repair-the-pre-existing-full-flake-owner-binding-evaluation-
title: Repair the pre-existing full-flake owner-binding evaluation failure
origin: parked
status: To Do
priority: medium
labels:[]
created: 2026-09-05
source: se-debug
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktrees/feat/sunshine-v4l2m2m
  branch: feat/sunshine-v4l2m2m
---

# Repair the pre-existing full-flake owner-binding evaluation failure

## Why it matters

Full nix flake check --no-build stops at checks.x86_64-linux.korrid-linux-device-module because korrid-validate-owner-binding.drv is not valid. The same failure reproduces on unchanged commit 128bb405cc3c3b28ea660245f06618c08e9402d4 with eval-cache disabled. Targeted Sunshine checks can pass while the repository-wide evaluation gate remains unavailable.

## Acceptance Criteria

- [ ] Reproduce the missing derivation error without Sunshine V4L2 changes.
- [ ] Fix the derivation context or import-from-derivation issue without weakening owner-binding validation.
- [ ] Run nix flake check --no-build successfully, or document any distinct remaining failure.

## Related

- `services/korrid/nixos-module-check.nix`
- `services/korrid/nixos-module.nix`
