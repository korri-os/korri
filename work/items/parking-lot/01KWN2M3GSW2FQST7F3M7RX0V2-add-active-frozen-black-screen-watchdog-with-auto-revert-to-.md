---
id: 01KWN2M3GSW2FQST7F3M7RX0V2
slug: add-active-frozen-black-screen-watchdog-with-auto-revert-to-
title: Add active frozen/black-screen watchdog with auto-revert to last known-good
origin: parked
status: To Do
priority: high
labels:
  - runtime-settings
  - reliability
  - safety
  - task-100
  - device-validation
created: 2026-07-03
source: se-work
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KWN2M3GSW2FQST7F3M7RX0V2-add-active-frozen-black-screen-watchdog-with-auto-revert-to-.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep the user-confirmed decode-truth requirement. Current services/korrid/src/linux_viewer.rs does not provide a production adapter or decode-confirmed runtime-setting recovery. The historical reducer and supervisor completion claims apply only to legacy.

## Scope on main

Detect a missing first decoded frame inside the native client during a setting change. Do not add a screenshot watchdog or a process that polls other tools. Reuse the current host effect boundary when a live viewer and setting path exist.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

This depends on a native viewer and setting-control path. The first-frame timeout needs device tuning, and a revert can also fail.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Add active frozen/black-screen watchdog with auto-revert to last known-good

## Progress: the machine-verifiable half of U-B is complete

Per the scope doc, U-B's machine part is "unit-test the known-good bookkeeping,
the revert path, and the recorded event"; the device session is the native
decode-confirm mechanism + window tuning. The machine part is now done:

- Policy core (commit 5551ca4f, `product/platform/stream/runtime-recovery.ts`,
  8 tests): known-good bookkeeping, revert-on-stall, no-oscillation (isRevert
  guard), no spurious revert on pre-apply rejections, never-silent. Pure reducer.
- Reducer refactor fix (commit 4f723450): the reducer type-imported the Moonlight
  protocol from `./moonlight-control-protocol`, which the plugin refactor
  relocated; the import was dangling (erased at runtime, would fail tsc). The
  reducer now owns local protocol-status types so platform stays plugin-agnostic.
- Live supervisor (commit d0d18af3,
  `product/platform/stream/runtime-recovery-supervisor.ts`, 8 tests): drives
  runtime mutations through a streamer-agnostic port, learns each command's native
  requestId, feeds every terminal outcome to the reducer, issues the revert to
  last known-good (or a seeded launch baseline), and surfaces every decision
  through a required never-silent sink. It already reverts on any failed/timed-out
  outcome, so it will handle the native decode-stall the instant that signal
  exists. Outcomes for commands it did not issue are ignored (manual CLI changes
  are the user's own).

Remaining — the device session (genuinely needs a human + screen):

1. Native (client): decode-confirmed applied-truth for resolution — arm a
   first-frame timer on the decoder reopen (paths 0009/0010), emit `failed`
   (decode-stall) when no frame decodes in the window. Cross-thread (the command
   handler that emits the outcome and the decode loop that reopens run on
   different threads), so it is not written blind; it is the moonlight
   patch-export half and needs real decode signals.
2. Wire the supervisor at session start: a thin adapter maps the Moonlight
   control client onto `RuntimeRecoveryControlPort` (setters return the
   command.accepted requestId; onResult filters runtime.commandResult), seeded
   with the launch baseline. Trivial mapping, but activation must be verified on
   the running device.
3. Tune the single device-only constant: the first-frame wait window.

## Framing (2026-07-04, user-confirmed): decode-truth, not a watcher tool

This is NOT a watchdog process that watches other tools. Build it as in-client
decode-truth plus an ordinary revert command:

- Finish the definition of "applied" for resolution: applied means the host
  applied it AND the Moonlight client decoded a frame at the new size. The client
  reopens the decoder (paths 0009/0010) and watches its own decode loop for the
  first frame via a timer armed only during the change and cancelled the instant a
  frame decodes (a timeout, not a standing monitor). No frame in the window ->
  outcome `failed`, reason `decode-stall`, over the existing outcome/event
  channel.
- Revert is Korri policy: on decode-stall (or any failed/timed-out change) send a
  normal set command back to the last decode-confirmed known-good and record it
  (never silent).
- Explicit anti-pattern to reject: a separate process that polls
  `korri stream show` / runtime-watch, screen-captures, or infers "looks black"
  and issues commands. That is tools-watching-tools and cannot see decode state.
- The only device-tuned value is the first-frame wait window; that single constant
  is why this needs a device session, not a watching apparatus.

See `docs/korri-stream-layer3-safety-net-scope.md` (U-B) for the full framing.

## Why it matters

task-100. Phase-1 continuity guarantee: if a live change hangs, times out, or leaves a frozen/black screen, the system should auto-revert to the last known-good settings and record the revert (never silent). Today failed/timed-out changes already keep the stream alive on prior settings, and explicit baseline restore exists (Moonlight records launch baseline; restore = normal set commands). The missing piece is an active detector: a resolution change the host applies but the client can't decode strands the user on black. Building the detector (no-frames-after-change → revert) needs real device signals to tune thresholds and must respect the contract (recovery is Korri-side policy, not fork auto-adaptation). Slated to be built with Gate A device feedback.

## Acceptance Criteria

- [x] A stalled change (failed/timed-out) triggers an automatic revert to the last known-good settings — implemented + unit-tested in the supervisor (d0d18af3).
- [x] The revert decision is surfaced through a required never-silent sink; every stall yields a revert or a recorded unrecoverable — unit-tested.
- [ ] A resolution change that produces no decoded frames within a bounded window is turned into a `failed` (decode-stall) outcome by the native client (device session).
- [ ] The supervisor is wired into the live session via the Moonlight client adapter and observed reverting a real frozen/black case on-device.
- [ ] Watchdog thresholds validated on-device against a real frozen/black case.
- [ ] Recovery policy placement respects the contract (Korri-side or client no-frames safety, not fork auto-adaptation).

## Related

- `docs/acceptance/runtime-settings-protocol-contract.md`
- `work/parking-lot/01KT2T2J1VBF9ETG4A45D8WBPX-add-runtime-resolution-recovery-fallback.md`
- `01KWN0KHT7CF3YXHWXTSCYMFNS`
