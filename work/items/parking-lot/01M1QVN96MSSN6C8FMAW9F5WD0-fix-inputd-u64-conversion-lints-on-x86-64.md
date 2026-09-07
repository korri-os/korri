---
id: 01M1QVN96MSSN6C8FMAW9F5WD0
slug: fix-inputd-u64-conversion-lints-on-x86-64
title: Fix inputd u64 conversion lints on x86_64
origin: parked
status: To Do
priority: medium
labels:
  - inputd
  - test-gate
  - rust
created: 2026-09-05
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: refactor/runtime-identity
  repo: korri
  invoked_by: runtime identity rename
---

# Fix inputd u64 conversion lints on x86_64

## Why it matters

`nix run .#inputd-check` stops before the NixOS module checks because Rust 1.97 rejects two no-op `.into()` calls under `-D warnings`. This blocks the full inputd quality gate for unrelated changes.

## Acceptance Criteria

- [ ] `nix run .#inputd-check` passes on x86_64.
- [ ] The aarch64 portability behavior for `st_nlink` remains correct.

## Related

- `services/inputd/src/ledger_proof.rs`
- `services/inputd/src/sunshine_state_digest.rs`
