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

## Sunshine appversion root cause confirmed by the next owner run

On the diagnostics build, exact launch `3badbfb6…` reached public_serverinfo,
failed with `invalid GameStream appversion`, and compensation stopped that exact
far launch. aka advertises `7.1.431.-1`, not an unsigned fourth component.
This confirms the viewer-failure/compensation mechanism; it does not diagnose
the separate renderer stall.

The linked [common-c parser](../../clients/android/app/src/main/jni/moonlight-core/moonlight-common-c/src/Misc.c)
(`extractVersionQuadFromString`, line 87) uses **signed `strtol`**, not `sscanf`.
[Connection.c](../../clients/android/app/src/main/jni/moonlight-core/moonlight-common-c/src/Connection.c)
passes the original serverinfo string into it. Crucially,
[Limelight-internal.h](../../clients/android/app/src/main/jni/moonlight-core/moonlight-common-c/src/Limelight-internal.h)
defines `IS_SUNSHINE()` as `AppVersionQuad[3] < 0`. The viewer must preserve `-1`,
not normalize it to zero. Moonlight Embedded's pinned
[serverinfo reader](https://github.com/moonlight-stream/moonlight-embedded/blob/775444287305849ebdf4736c75298ad0713e2d5d/libgamestream/client.c)
keeps the full version and uses `atoi` only for its separate major-version gate;
its [stream entry point](https://github.com/moonlight-stream/moonlight-embedded/blob/775444287305849ebdf4736c75298ad0713e2d5d/src/main.c)
hands that server information to common-c.

`protocol.c` now accepts a signed fourth decimal component with `strtol`, while
retaining the major ≥ 7 gate, nonnegative first three components, int bounds,
exact dot separators and complete consumption. It rejects missing digits,
whitespace, overflow and trailing garbage. The exact owner-supplied XML fields
(uniqueid/mac redacted) first failed the new C regression, then passed after the
fix. Tests also call the actual linked common-c parser and verify the -1 build.

The fixture checks GfeVersion `3.23.0.74`, FREE state, PairStatus 0,
HttpsPort 47984, currentgame 0 and ServerCodecModeSupport 197377. The authenticated
TLS fixtures now carry the same version and codec bits. Production `mode.c`
still sets `supportedVideoFormats = VIDEO_FORMAT_H264`; `kv_launch` and the NDL
video callback reject other formats. Host HEVC/AV1 capability bits remain facts
about the host, not permission to choose them on this TV.

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
