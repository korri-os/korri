---
id: 01M1YA4WKXR80WDZ5Y04QWYN8B
slug: create-missing-config-snapshot-files-without-overwriting-con
title: Create missing config snapshot files without overwriting concurrent edits
origin: parked
status: To Do
priority: medium
labels:
  - config
  - data-integrity
created: 2026-09-07
source: se-code-review
---

# Create missing config snapshot files without overwriting concurrent edits

## Why it matters

The snapshot initializer checks existence and then calls proseQL StorageHost.write. FsStorageHost.write truncates an authored file created between those operations. This race predates the minimum config slice and now applies to its three fixed files. Correcting it requires an exclusive-create operation at the storage interface, which currently exposes only unconditional write.

## Acceptance Criteria

- [ ] A real filesystem test interleaves authored-file creation with snapshot initialization and proves the authored bytes remain unchanged.
- [ ] The storage interface exposes non-replacing creation, and snapshot initialization treats AlreadyExists as success.
- [ ] Memory-backed snapshot tests retain deterministic initialization behavior without production-only branching.

## Related

- `services/korrid/src/config/snapshot.rs`
- `services/korrid/tests/config_snapshot.rs`
- `docs/research/proseql-as-korrid-config.md`

## Notes

Pre-existing finding, verified by source inspection during minimum-config review. No reproduced data loss in a live device.
