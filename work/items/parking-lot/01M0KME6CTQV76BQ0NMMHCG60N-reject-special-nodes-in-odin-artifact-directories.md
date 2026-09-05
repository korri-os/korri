---
id: 01M0KME6CTQV76BQ0NMMHCG60N
slug: reject-special-nodes-in-odin-artifact-directories
title: Reject special nodes in Odin artifact directories
origin: parked
status: To Do
priority: low
labels:
  - android
  - odin2portal
  - firmware
  - hardening
created: 2026-08-22
source: se-code-review
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  commit: 167aff5ed873
  repo: korri
  invoked_by: se-work
---

# Reject special nodes in Odin artifact directories

## Why it matters

The install-readiness inventory checks regular files and symlinks, but pre-existing FIFOs, sockets, device nodes, and unexpected empty directories are outside its file inventory. They do not alter the currently pinned partition files, but the gate is not fully fail-closed for directory shape.

## Acceptance Criteria

- [ ] Launcher and rollback readiness reject every filesystem entry that is not an expected regular file or expected directory.
- [ ] Tests cover a FIFO and an unexpected empty directory.
- [ ] Normal pinned launcher and rollback artifacts still pass.

## Related

- `clients/android/firmware/odin2portal/launcher-install-readiness.sh`
- `clients/android/firmware/odin2portal/test-launcher-install-readiness.sh`
