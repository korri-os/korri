---
id: 01M20KN9KFYZ0CMVX9JY4V49P0
slug: connect-linux-resume-to-compositor-focus-in-korrid
title: Connect Linux resume to compositor focus in korrid
origin: parked
status: To Do
priority: high
labels:
  - odin2portal
  - korrid
  - compositor
  - session-lifecycle
created: 2026-09-08
source: se-work
---

# Connect Linux resume to compositor focus in korrid

## Why it matters

The Linux Portal shows a now-playing session but reports "Resume is unavailable: compositor focus is not connected." The opaque hub covers running games, so a user who leaves a game cannot get back to it. This is the last purely local blocker before Odin session verification, and it stays broken until korrid can name and raise the exact launch's window.

## Acceptance Criteria

- [ ] korrid selects the window of one exact launch and refuses to guess between several windows
- [ ] korrid never focuses the kiosk browser as a game window
- [ ] a window that only claims a game's name by class or title is ignored
- [ ] the Portal resume path replaces the unavailable notice with real behavior, proven by tests
- [ ] the compositor transport and any new wire contract are grounded in an existing consumer or an explicit user choice

## Related

- `services/korrid/src/host/session_state.rs`
- `clients/portal/src/surface/use-launchables.ts`
- `services/inputd/nix/korri-linux-host.nix`
- `work/items/active/01M1PJ6V5T4A8K2R0C3H9E7N4D-odin2portal-raw-nixos/web-session-progress.md`

## Notes

A pure Sway-tree selector with nine passing tests was written and then reverted because it had no caller, which this repository forbids. The reverted code is kept at /tmp/compositor_focus_slice.rs. It matches windows by the launch unit's own processes, treats app_id and class only as an exclusion list for the kiosk browser, and returns NoWindow, Window, Ambiguous, or Unreadable. Open decisions: how korrid reaches Sway IPC at /run/korri-compositor/sway-ipc.sock, how it learns the launch unit's PIDs, and whether resume needs a new RPC or extends an existing one. Legacy grounding: product/services/device/sessiond-sway-events.ts and services/inputd/nix/korri-linux-host.nix validation action.
