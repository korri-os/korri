# webOS first stream: failure diagnostics and recovery

Two workstation defects are reproduced and corrected. **The exact TV main-thread
stall is not reproduced or diagnosed.** No TV or real host was contacted for this
follow-up. The next owner run must retain a thread/syscall capture if it stalls.

## Evidence from the owner

IPK SHA256 `9b0622ebd729605619f6da39e20f4fe422686439b017037b473d444e567b6dfe`,
source `151c8323a483134ac2d18236f1215bf724888533`:

- The app-UID identity bootstrap passed twice. The directory was 0700, its three
  files 0600, owner 6236, with stable public certificate and uniqueid hashes.
- On aka, launch `d1b3a8f1552ad659bd669dbfb38eb868` started at
  `2026-11-09T02:14:58.629868Z`. korrid reported `outcome: started` at
  `02:14:58.750440Z`. systemd began stopping it at `02:14:59.842700Z` and
  recorded exit 143 at `02:14:59.931833Z`.
- The TV showed HostLaunchFailed/143, no viewer stages, then stopped applying
  visible clock changes and pad Dismiss. The owner restarted before a stack
  capture was available. Main thread was running; the logic thread waited.

The inspected files are the owner's saved
`~/.cache/research/korri-webos-m2/evidence/launch-log-failed-stream.txt` and
`aka-journal-d1b3a8f1.txt`. The aka journal proves an explicit systemd stop preceded
exit 143. It does **not** identify the caller. TV viewer-start compensation is
consistent with the timing, not proven. An interleaved SSH login is not evidence
that its session caused the stop.

## Reproductions before fixes

1. `surface::launchables::state_tests::local_viewer_failure_survives_host_exit_observation_after_compensation`
   failed: a later HostLaunchFailed observation replaced the existing
   ViewerStartFailed notice and its game subject. The state now keeps the known
   local launch cause and its dismissible Ready state; HostRecoveryBlocked still
   requires recovery. This uses existing RPC failure and notice data, not a new
   persisted or wire field.
2. The real SDL input-pump test queued 4096 finite jobs. One frame drained all of
   them before presentation. A continuing producer could therefore starve frames
   and SDL input. The pump now gives one frame at most 64 queued jobs and 64 polled
   SDL events. Remaining jobs run on the next iteration without sleeping or being
   dropped. A normal complete native frame and SDL key still apply together.

The real Zone regression already passed before these fixes: pad LaunchGame,
pre-READY Problem, pad Dismiss, and clock pushes while the Problem was shown and
after dismissal. That is retained as a regression, not claimed as reproduction
of the owner's stall. The pre-READY presentation/input-gate hypothesis is not
supported by this test: only an actual READY can set presenting=true.

## Diagnostics in the next build

No logger initialization is needed. Like LaunchTimeline, bounded stage records
write directly to stderr, which the existing launcher appends to its jail log.

- Shared korrid stages: far_prepare, far_launch_accepted with exact launch ID,
  attest, provision, and compensation with exact launch ID and result.
- TV adapter stages: start, identity, public_serverinfo, certificate_broker,
  helper_exit, child_spawn, ready, child_exit, and forced-stop timeout.
- C child stages: identity, public_serverinfo, authenticated_serverinfo, applist,
  applist_resolved with the authenticated app ID, transport_launch, stream, READY.
- C failure diagnostics use `kv_error`'s literal messages only. A bounded reader
  relays only the child-owned diagnostic prefix, ignores dependency text, and
  retains the local cause for the existing ViewerStartFailed surface notice.
  Helper failure and stream exit status remain visible even before READY.

No certificate, private key, pin, request URL, RTSP URL, request body, rikey,
capability or inherited environment is logged. Common-c/ss4s debug output stays
muted. No new input/readiness message is added. Trust, compensation authority,
Leave/End semantics and shared reconnect policy are unchanged.

## Owner retest

Use the new pinned IPK and aka, as already authorized. Keep the app UID/jail,
LS2 identity, power policy and system SDL unchanged.

1. Start Korri normally and record its source/IPK SHA and Zone PID.
2. Pick ActRaiser in Zone with the pad. Save the stderr stage sequence and exact
   launch ID, especially the first failed stage and any compensation line.
3. If Problem appears, Dismiss with the pad. Confirm that the clock advances and
   a second PLAY remains usable. Verify local cause text, not only host exit 143.
4. On a stall, **before restarting**, capture each thread's wchan/syscall/stat
   and the owner's planned five-second main-thread strace. The current tests do
   not exclude a renderer/vendor call stuck outside the bounded job pump.
5. If playback reaches READY, continue the ordered picture/sound/pad/quit checks
   in `clients/shell/webos/VIEWER.md`.

A compensation stage naming the same exact launch proves that the TV requested
its stop. If far_prepare fails before far_launch_accepted, local viewer startup
has not run; inspect the peer-side launch path instead. Reconnect give-up and UX
remain product-wide issue #390, not part of this change.
