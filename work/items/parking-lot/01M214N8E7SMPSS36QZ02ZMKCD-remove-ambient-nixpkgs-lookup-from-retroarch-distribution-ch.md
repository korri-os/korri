---
id: 01M214N8E7SMPSS36QZ02ZMKCD
slug: remove-ambient-nixpkgs-lookup-from-retroarch-distribution-ch
title: Remove ambient nixpkgs lookup from RetroArch distribution checks
origin: parked
status: To Do
priority: high
labels:
  - ci
  - retroarch
created: 2026-09-08
source: se-work
---

# Remove ambient nixpkgs lookup from RetroArch distribution checks

## Why it matters

The published RetroArch workflow fails before producing an APK because ra-check executes test-acceptance-contract.sh through a nix-shell shebang that requires an ambient <nixpkgs> search path. The GitHub runner has no such entry. This blocks distribution independently of the new device cache policy.

## Acceptance Criteria

- [ ] The RetroArch acceptance-contract check runs through the pinned project toolchain when NIX_PATH is empty.
- [ ] The distribution workflow reaches its actual build and package validation steps on a clean GitHub runner.
- [ ] The korrid plugin-route review also runs with an empty NIX_PATH through the pinned project toolchain.
- [ ] No target-device compilation or signature bypass is introduced.

## Related

- `nix/tasks.nix`
- `plugins/retroarch/android/test-acceptance-contract.sh`
- `.github/workflows/retroarch-distribution.yml`
- `services/korrid/plugin-route-review.sh`
- `.github/workflows/korri-checks.yml`
- `https://github.com/simonwjackson/korri/actions/runs/34262894334`
- `https://github.com/simonwjackson/korri/actions/runs/34262894240`

## Notes

Failure at 2026-09-08T18:27:48Z: file 'nixpkgs' was not found in the Nix search path. Earlier fetch/build/install flow tests passed; signing and release jobs were skipped. No fix attempted in the cache workstream.

The Korri checks workflow failed with the same missing search-path entry at 2026-09-08T18:31:47Z. Its diagnostic lists the exact nix-shell inputs from services/korrid/plugin-route-review.sh: bash, coreutils, git, and nix. The Android bridge job was skipped. Inspect both callers when removing this ambient dependency. Local cache-specific checks remain green.
