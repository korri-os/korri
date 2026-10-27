# One post-reply catalog reflow per kiosk — 2026-10-27

## Correction and evidence

`5a900abeb` fixes catalog presentation after a game, but its single distinct-size
restore is insufficient at cold kiosk startup. On RG DS b3cafi59, the startup
restore took 428 ms at 03:29:05.6, with 640×900 preparation and 250 ms settling.
At 03:31:34 Pico still showed Loading and clock 3:29. It recovered without a
placement event before the 03:32:44 capture. Play → Leave → End and manual
display off/on passed. Source: `/tmp/rgds-acceptance/final-checks.log` and the
parent's `fin-*.png` observations. The idle service failure remains #188 and is
outside this change.

The hypothesis that this restore precedes portal mount was wrong. In
`clients/portal/src/surface/surface-displays.ts`, `useSurfaceDisplays` sends
`surfaceDisplays` from React `useEffect`. `lib.rs::dispatch` calls
`HostRuntime::surface_displays`, which runs
`HostSessionControl::surface_displays` synchronously on a blocking worker. That
request already performs the logged startup restore. Waiting for the first
request therefore describes the existing, failing behavior.

The successful q:12192 / `cold900.log` device probe performed a **second**
640×900 → global 640×960 restore after that initial automatic restore. The
parent measured roughly 500 ms between restorations and explicitly chose to
preserve this sequence instead of replacing the first restore with a single
asynchronous one. The rounded log records both at 02:27:31; it does not itself
supply a subsecond measurement. This is evidence for a workaround, not proof of
a Chromium first-frame, Viz or LocalSurfaceId root cause.

## Implementation and ordering

1. Preserve the first successful synchronous distinct-size catalog restore.
   It publishes full-size rectangles as before.
2. Record one pending post-reply reflow for the verified portal window instance
   (Sway node ID and actual window PID, selected uniquely from the kiosk unit's
   live PIDs). No app-id or title inference is introduced.
3. Only a successful capability-bound catalog `app.surface.displays` HTTP reply
   can claim that pending reflow. `lib.rs::catalog_reply_then_restore` drains
   the complete JSON response body before it queues work. Dispatch completing
   by itself does not queue it. Dropping an unfinished reply cancels it.
4. `HostRuntime::schedule_catalog_readiness_restore` runs outside the RPC, no
   earlier than the first restore receipt plus the named
   `CATALOG_READINESS_REFLOW_DELAY` of **500 ms**. If response completion is later
   than that deadline, it queues the work then. The original response does not
   wait for the follow-up.
5. Under existing native transition authority, verify that the same catalog
   placement receipt remains current and resolve the same unique unit-owned
   portal window again. Then call the existing shared `place_surface` path:
   640×900, 250 ms settling, identity recheck, global fullscreen and focus.

HTTP body EOF means the complete body has been handed to the HTTP transport.
It is not an acknowledgement that Chromium received it, mounted new facts or
painted a real frame. The mounted request plus post-response boundary and
measured spacing are the chosen readiness workaround; no new first-paint
signal is claimed.

`host/portal_readiness.rs` owns the one-shot receipt and its lifecycle.
`AppliedDisplayPlacement::Catalog` is the existing placement cache extended to
own catalog facts and an opaque placement receipt. A newer game or overlay
placement invalidates it before native effects, including failed partial
attempts. The delayed job cancels if that receipt changed, the verified kiosk
window changed, or shutdown started. Cancellation does not reconcile focus,
power or the portal freezer. A new kiosk PID/window can arm one new reflow.

Subsequent same-instance catalog requests read the cached full-size facts,
validate the cached window PID against the live kiosk unit, and perform no
placement or reconciliation. This fast path does not acquire the session
transition mutex, so it also works while the follow-up is running. Concurrent
first requests recheck the cache after acquiring native authority, preventing
duplicate synchronous restores. Overlay facts keep their existing frozen-game
cache behavior.

The reflow is consumed once. Failures are logged, not automatically retried. A
failed native follow-up invalidates the placement cache; an explicit later
request can perform a fresh synchronous placement without rearming the same
instance's follow-up. There is no persisted readiness schema, new RPC, new
program, frontend change or additional kiosk permission.

## Diagnostics, costs and acceptance

Info-priority `PortalReadinessRestore` lines distinguish `Started`, `Cancelled`
and `Failed`, with PID and con_id. `Started` includes
`since_initial_restore_ms`. Existing `PlacementTransition` retains requested
`phase1_size` and measured `portal_settle` for the second restore too.

Cost: **one additional visible reflow, about 430 ms on the measured device**, near
portal start, with its existing 250 ms settle. Native authority remains held
through that reflow to prevent Return or a new placement racing final focus.
A native command arriving during it can wait for that transition. The 500 ms
spacing is outside the mutex and does not delay the original response or
cached display-facts responses. No 500 ms wait is added to normal Home/Return.

The fake-Sway regression uses production capability-bound HTTP routing and
native subprocess IPC. It proves first request → synchronous restore → reply
completion → exactly one further 900-high restore; a second request performs
none. It also covers a preexisting startup restore, concurrent first requests,
game cancellation, kiosk PID replacement and rearming, aborted replies, and a
cached reply while the follow-up's native command is held. A real loopback TCP
server/client check confirms Hyper drains the response and runs exactly one
follow-up; this is not only an in-process response-body test.

185 focused korrid tests passed. All-target check, Clippy and formatting passed
with existing warnings. No hardware access or deployment was performed by this
child. The actual cold-start stall still requires device acceptance on this
commit; the local RPC test proves ordering and fencing, not Chromium recovery.
