---
id: 01KZFGY0Z4FM1CBRZ2BBEC57V3
slug: authenticate-mutating-linux-host-rpc-requests
title: Authenticate mutating Linux host RPC requests
origin: parked
status: To Do
priority: high
labels:
  - security
  - linux
  - korrid
  - rpc
created: 2026-08-08
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktrees/feat/restore-linux-inputplumber
  branch: feat/restore-linux-inputplumber
  commit: 1f5018b0
  repo: korri
  invoked_by: se-work
---

# Authenticate mutating Linux host RPC requests

## Why it matters

Zao's host RPC listener is LAN-bound and currently accepts session prepare without a capability. This predates the InputPlumber work, but systemd-owned launches make the existing unauthorized launch/DoS surface more important to close without mixing an authentication redesign into input restoration.

## Acceptance Criteria

- [ ] Mutating Linux host RPC requests require device-authenticated authority.
- [ ] Unauthenticated LAN callers retain only explicitly approved read-only operations.
- [ ] Current trusted launch clients pass the new authority without exposing it in logs, argv, or browser storage.
- [ ] Tests prove unauthenticated LAN and ordinary loopback callers cannot prepare or mutate sessions.

## Related

- `services/korrid/src/lib.rs`
- `services/korrid/src/main.rs`
- `work/items/active/019fde6b-8c02-7b01-8dfb-ffe97bcb5ef1-restore-linux-inputplumber/plan.md`

## Notes

Discovered during U4 security review. Existing LAN prepare behavior is outside the approved input-stack scope; local status/stop remains private in the current work.
