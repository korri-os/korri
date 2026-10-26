# Home feedback before freezer latency — 2026-10-26

## Change and exactness

Leave now performs this sequence while holding exact-session transition authority:

1. Recover/identify the expected launch. Reject stale identity and Stopping
   before any dim effect.
2. Resolve that unit's live PIDs, exclude the kiosk unit's PIDs, and select its
   unique compositor window. Dim that exact window using the existing
   same-rectangle floating/opacity batch.
3. Observe the unit's freezer state, then freeze if needed.
4. Revoke gameplay delivery, thaw/place the portal, and power its display.

The unit-state read is moved, not deleted. The mutex-bound active identity and
fresh PID/window ownership prove which window may receive visual feedback;
freezer state need not be read first. No PID cache, app-id fallback, new RPC,
permission expansion, or status-source duplication is introduced.

The inputd read-only status RPC remains. It supplies both the exact launch ID
and the running/frozen phase needed by Home's toggle. Its snapshot is not an
authorization to act on any later replacement: the existing expected-ID check
and locked transaction still reject one. Removing that read would need another
authoritative identity/phase producer or a different atomic-toggle contract;
this change does not invent either.

## Why the brief running-but-dimmed interval is safe

The dim is visual feedback, not a claim that pause already completed. Until the
freezer operation returns, the game can continue rendering/simulating for about
its existing 100 ms freezer latency. The portal's PAUSED overlay is not placed
until after the freeze succeeds.

Korrid changes compositor presentation, not application memory or saved data.
The window keeps its committed rectangle; the game/compositor retain normal
buffer ownership and synchronization. The game may handle the fullscreen/floating
configure while it is still running and continue rendering into its buffer.
Opacity is applied by composition. There is no buffer rewrite or new native
program. The same fullscreen/opacity recipe restores presentation on failure.

## Rollback and failure handling

A transition-scoped receipt is armed before the dim command, including possible
partial command failure. All failed Leave outcomes restore the exact launch's
fullscreen/opacity recipe before publication, unless completion already proves
that unit cannot run. A successful existing rollback does not repeat restoration.
The normal Return recipe remains shared; failure-only restoration does not append
an extra focus request when seat recovery may have failed.

Failure paths include refused/partial dim, failed unit observation, refused
freeze, portal placement/focus failure and failed receiver rollback. If presentation
cannot be restored, korrid keeps the exact game paused and reports recovery
failure instead of deliberately leaving it running dimmed or killing it.
A worker-unwind safety net attempts the same presentation restore, then an exact
freeze if restoration fails. An unrecoverable compositor/freezer failure remains
administrator recovery, not a successful acknowledgement.

An expected-launch mismatch never dims a replacement. Once the receipt is armed,
normal Korri launch/stop/replacement operations cannot interleave through the held
mutex. Live-unit completion is still observed after feedback and is not mistaken
for permission to target another unit.

## Expected latency, not a new device measurement

Grounding: the parent measured unit observation at about 32 ms, freezer change at
about 100 ms, and the existing dim operation at about 50 ms on RG DS.

| Phase before dim | Before | Now | Expected feedback saving |
|---|---:|---:|---:|
| Unit freezer observation | ~32 ms | after dim | ~32 ms |
| Unit freeze | ~100 ms | after dim | ~100 ms |
| Exact PID/window proof and dim | ~50 ms | retained | none |
| Inputd minimal exact status | about one unit read | retained | none |
| Portal thaw/power/geometry | already after dim | unchanged | none |

A nominal one-read status (~32 ms) plus dim (~50 ms), with modest scheduling,
transport and lock time, makes the owner's 80–120 ms target plausible. This is
an estimate, not acceptance evidence. Approximately 132 ms moves off the
pre-feedback path; the full Home transaction still performs those operations.
Concurrent native effects, including catalog's bounded 250 ms settle, can still
add queue time. No hard physical-press latency guarantee is made.

## Timing boundaries

- `PlacementTransition.press_to_dim_ms`: from server dispatch entry for the
  received Leave request to successful dim-batch return. The existing arrival
  instant is passed through the blocking-worker boundary, so authorization,
  worker scheduling and transition-lock wait are not omitted. `None` means no
  successful dim acknowledgement, not a zero-latency dim. Direct host API calls
  use their own entry time.
- `game_dim`: only the ownership-resolution/dim operation.
- `dim_rollback`: failure-only presentation restoration.
- `inputd_panel_dispatch_timing.scheduler_queue_ms`: exact-panel dispatch branch
  to task execution.
- Existing `inputd_panel_timing` retains guard/status time and attempts, and now
  adds `pre_request_ms` (guard plus status/processing before mutation dispatch).

These are monotonic software timings. A dim IPC acknowledgement is not a
measurement of the first physically displayed dim pixel.

## Verification

Public controller/native fake-Sway tests assert dim → unit read → freeze →
power → portal, including repeated cycles and both freezer-enabled and
identity-only kiosks. They cover stale expected identity without compositor
effects, refused dim without a freeze attempt, refused freeze, failed post-dim
observation, completion after dim, portal readback failure, rollback fullscreen/
opacity restoration, failed restoration keeping the game paused, and observer
panic cleanup. Existing replacement/metadata-continuation tests remain in scope.
Inputd's exact-ID, serialized toggle, response framing, bounded panel deadline
and mutation-no-retry tests remain unchanged in behavior.

The next check is RG DS acceptance: inspect `press_to_dim_ms`, inputd queue and
status fields, then confirm visible feedback and all normal Home/Return/End
cycles. The accepted two-phase catalog restore and its `portal_settle` behavior
are unchanged. No RG DS access or deployment was performed by this child agent.
