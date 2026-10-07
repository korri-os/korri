---
id: 01KTWXG8RZ90R1D5AX1S1Y9AS4
slug: portal-ui-never-recovers-from-korrid-restarts-stuck-on-loadi
title: "Portal UI never recovers from korrid restarts (stuck on \"loading library\")"
origin: parked
status: To Do
priority: high
labels:
  - portal
  - resilience
  - ux
created: 2026-06-12
source: se-debug
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KTWXG8RZ90R1D5AX1S1Y9AS4-portal-ui-never-recovers-from-korrid-restarts-stuck-on-loadi.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep the appliance restart acceptance requirement, not the Electrobun diagnosis. Current use-launchables.ts loads once, refreshes on selected focus and discovery transitions, and polls known local launch state. SurfaceRoot.recovery.test.tsx covers exact-launch recovery, not a fresh process restart of the real local brain and recovery of every catalog source.

## Scope on main

First reproduce with the current Linux host and private window.KorriRpc binding. Treat the reported legacy failure as unverified on main. Preserve the ten-second recovery criterion unless new device evidence changes it.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

A retry must not repeat a mutation or reuse stale authority. A unit test with an in-memory client does not prove real daemon restart recovery.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Portal UI never recovers from korrid restarts (stuck on "loading library")

## Why it matters

Three times today on bandai the kiosk UI sat on "loading library.." indefinitely while korrid answered RPC perfectly — every korrid restart strands the electrobun front-end on a dead connection with no retry/reconnect, requiring a sessiond restart to recover. On an appliance the daemon will restart (crashes, config redeploys, watchdog) and the UI must resync itself; a couch user has no systemctl. Needs reconnect-with-backoff in the portal's RPC client (or sessiond watching korrid restarts and reloading the renderer).

## Acceptance Criteria

- [ ] Restarting korrid.service while the UI is open results in the library reappearing without any manual intervention (within ~10 s)
- [ ] Reconnect behavior covered by a test or documented manual validation on device

## Related

- `product/apps/portal`
- `product/services/device/sessiond-renderer.ts`
