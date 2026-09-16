---
id: 01M2MB6G1AM8YGKE3AY7AVTC1T
slug: report-a-failed-game-startup-as-failed-not-completed
title: Report a failed game startup as failed, not completed
origin: parked
status: To Do
priority: medium
labels:
  - korrid
  - sessions
  - hardware-verified
created: 2026-09-16
source: se-work
---

# Report a failed game startup as failed, not completed

## Why it matters

On haku, a launch whose runner exits 1 during startup (unreadable content file) returns the same app.session.status result as a normal quit: code SessionCompleted, "host launch <id> completed". A surface therefore cannot tell a player "the game closed" apart from "the game never started", and cannot show the reason. Cleanup itself is correct: no stale korri-game unit remains and the next launch succeeds.

## Acceptance Criteria

- [ ] A runner that exits non-zero before readiness is reported with a distinct status and the exit status
- [ ] A normal quit keeps its existing result
- [ ] The distinction is covered by a korrid test

## Related

- `services/korrid/src/host/mod.rs`
- `services/korrid/src/host/session_state.rs`

## Notes

Observed on haku (RG353M) with launch e32b29fbfa3be3593759f19afe878014 after removing the gameplay ACL from the ROM.
