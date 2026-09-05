---
id: 01M0KME6CP7ZWE66X93RNWR4C9
slug: make-odin-launcher-image-builds-byte-reproducible
title: Make Odin launcher image builds byte-reproducible
origin: parked
status: To Do
priority: medium
labels:
  - android
  - odin2portal
  - firmware
  - reproducibility
created: 2026-08-22
source: se-code-review
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  commit: 167aff5ed873
  repo: korri
  invoked_by: se-work
---

# Make Odin launcher image builds byte-reproducible

## Why it matters

Two host-only builds from the same stock source, signed APK, and AVB key produced different product root digests, which changed `super` and `vbmeta_system_a` hashes. Exact artifact pinning keeps the current install safe, but regeneration cannot independently reproduce the approved bytes.

## Acceptance Criteria

- [ ] Two clean launcher-image builds from identical inputs produce identical `super`, `vbmeta_system_a`, and manifest hashes.
- [ ] The pipeline pins or normalizes all filesystem metadata that affects the product hashtree.
- [ ] A regression check compares two generated outputs without device writes.

## Related

- `clients/android/firmware/odin2portal/launcher-product-dry-run.sh`
- `clients/android/firmware/odin2portal/launcher-image-dry-run.sh`
