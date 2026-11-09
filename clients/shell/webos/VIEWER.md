# Embedded webOS viewer integration (2026-11-08)

The app links its embedded Moonlight into **`bin/korri-webos viewer …`**.
The shell and korrid remain separate programs. No TV or real host was contacted
by this implementation. M4 playback still needs the owner-run checks below.

## Design and source of authority

| Concern | Implementation and grounding |
| --- | --- |
| Executable identity | `launcher.c` dispatches `viewer` in the same executable as appinfo `main`. `clients/webos-viewer/CMakeLists.txt` links the core and retains `media-test`. The owner reports that this path opened NDL without LS2 role edits on 2026-11-08; see the milestone brief. |
| One brain | `webos-korrid.rs` supplies `EmbeddedPlatform.viewer_factory`. `lib.rs::start_local_server_for_platform` constructs it with the existing `UpstreamRegistry::moonlight_certificate_provision` broker, then uses `with_linux_viewer`. There is no second registry or game launcher. |
| Identity | `webos_viewer.rs::prepare` invokes `viewer identity` and receives only the bounded public cert. `clients/webos-viewer/identity.c` owns stock `client.pem`, `key.pem`, `uniqueid.dat` under the approved `/media/developer/org.korri.shell/moonlight/`. It locks, audits and re-tightens the allocation. No other persisted schema is introduced. |
| Trust | `viewer discover` reads bounded public serverinfo. The existing `upstreams.rs::moonlight_native_peer` requires exactly one authenticated native peer attesting the UUID. The existing certificate provision RPC returns the server certificate. `https.c` requires client-cert HTTPS and exact certificate pin verification. No PIN, TOFU, installed Moonlight keys or host-list file. |
| Transport app | `protocol.c::kv_applist` reads the authenticated app list. Its Expat parser requires exactly one positive ID named `Korri Stream`, rejects duplicates/DTDs and bounds XML. `mode.c` refuses an occupied different transport, then launches or resumes the resolved ID. It does not hardcode or persist an app ID. |
| Child ownership | `webos_viewer.rs` owns/reaps a `Child` for the exact far launch. A dedicated parent thread stays alive because Linux PDEATHSIG follows the forking thread. Parent death kills the child; no process-name kill or shell is used. The child gets no shell RPC capability or credential directory. |
| Readiness | The inherited private socket at fd 3 carries `R` only after common-c connectionStarted **and** the first accepted decode unit. Startup waits at most 45 seconds, then releases/reaps the child before shared far-start compensation. This is not proof of a visible frame. The pin arrives once over bounded stdin, never argv/env/files. |
| Presentation | The adapter accepts a `presenting(bool)` callback: true after READY, false on exit/stop/Leave/input loss. The composition wires it to `SurfaceHostInputSource::set_local_viewer_presenting`. The shell receives that host-only watch fact and exposes the video through its existing window; no SurfaceModel/treaty field is added. Its existing focus/input attachment gate retires hidden Zone input during play and rearms on return. The owner reports TV capture proved video-plane/graphics composition with a transparent half; production full-window transparency is separate work. |
| Native pad | `SurfaceHostInputSource::subscribe_local_gamepads` takes an atomic baseline and bounded live frames from the existing source. The adapter sends those unchanged JSON records, newline-framed, on the private socket. `input.c` maps keys, measured axis ranges, triggers and seats to `LiSendMultiControllerEvent`. The shell never reads evdev. Baselines require neutral rearm; SYN_REPORT commits a sample. Home/BTN_MODE is not forwarded. EOF, unplug, overflow and stop neutralize/release seats. LG remote play mapping and the local overlay key are deferred to M5 pending real events. |
| Leave / End | Existing `upstreams.rs::session_freezer_resolved` performs exact far freeze before viewer background, with compensation on failure. The adapter releases/reaps media and retains **Backgrounded**, not Stopped, so polling cannot restart an intentional Leave. Resume reconnects. Existing session end stops the exact far game, then the viewer; far completion uses `session_status_resolved`. The viewer contains no `/cancel`. |
| Unexpected drop | Uses **shared** `upstreams.rs::recover_viewer`, exactly as Linux. A stopped viewer is restarted while its selected far launch remains live; a frozen far launch retains background state. There is no TV-specific retry policy. Today the shared path has **no give-up bound**. Product-wide bounded retries and reconnect UX are issue **#390**. Giving up is disconnect/Leave, never End; that future policy is not implemented here. |

The input/readiness socket and bounded pin pipe were approved by the parent
before implementation. They add no peer RPC field or surface treaty variant.
All network fixtures use loopback. No power, screensaver or app registration
behavior changes. Existing system SDL remains unbundled.

## Audio investigation

The owner has verified full-screen video, automatic provisioning and physical pad
input. Sound still needs the TV check. See [bounded audio diagnostics and the
private Pulse monitor test](../../../docs/briefs/2026-11-09-webos-audio-diagnostics.md).
This build reports audio initialization and up to 30 one-second counter/PCM-peak
summaries. It does not log packet contents or session material.

## First-TV failure follow-up

See [the evidence, test-first reproductions and retest order](../../../docs/briefs/2026-11-09-webos-first-stream-failure.md).
Viewer stages now go to stderr without logger setup. The surface retains a known
local start failure instead of replacing it with the compensated host's exit 143.
The SDL job/event pump yields to presentation under a producer backlog. The exact
TV stall still needs the owner's capture if it recurs.

## Workstation checks

Run expensive commands serially with `CARGO_BUILD_JOBS=2`:

```sh
CARGO_BUILD_JOBS=2 nix run .#webos-viewer-check
CARGO_BUILD_JOBS=2 KORRI_WEBOS_TV_LIBS="$HOME/.cache/research/korri-webos-m2/evidence/tvlibs" \
  nix run .#webos-viewer-build
CARGO_BUILD_JOBS=2 nix run .#shell-check
CARGO_BUILD_JOBS=2 nix run .#korrid-check
CARGO_BUILD_JOBS=2 nix run .#shell-webos-build
sha=$(git rev-parse HEAD)
test "$(git rev-parse HEAD)" = "$sha"
CARGO_BUILD_JOBS=2 nix run .#shell-webos-package -- "$PWD/.cache/webos-viewer-ipk" --source-sha "$sha"
CARGO_BUILD_JOBS=2 nix run .#shell-webos-package-check -- --check-ipk "$PWD/.cache/webos-viewer-ipk/org.korri.shell_0.0.0_arm.ipk"
```

Use the actual version/path printed by packaging, rather than assuming the
example filename's version. Build and package tasks require clean pinned source
and record each artifact's SHA. `package.py::audit_payload` audits all seven
packaged ELFs against original TV providers, including the real `libgcc_s.so.1`
(SHA256 `645c0dfa0eca6d830abbc82bb11a8441176c6d33309ae9254df09efbebe33aa1`).
The previous 13-symbol exception is removed. No provider library is bundled.
The package includes common-c/ss4s/crypto/codec notices. External distribution
still needs the source/relinking/license review in the embedded-viewer design.

## Exact owner-run TV order

These steps are for the owner, not permission for an agent to contact haku.
Use prebuilt pinned artifacts. Keep the normal app UID/jail and the unchanged
LS2 role, system SDL, screensaver and power behavior. Record source/IPK SHA,
boot ID, exact far launch ID and the original Zone PID. Coordinate readiness
before any timed physical check. Installation remains the existing separately
approved clean-uninstall/install workflow; keep `/media/developer` intact.

1. **Identity persistence, offline.** With no stream running, in the installed
   app directory and as its app UID, run:
   ```sh
   APPID=org.korri.shell ./bin/korri-webos viewer identity /media/developer/org.korri.shell/moonlight/ >/tmp/korri-viewer-public.pem
   sha256sum /media/developer/org.korri.shell/moonlight/client.pem /media/developer/org.korri.shell/moonlight/uniqueid.dat
   stat -c '%a %U %n' /media/developer/org.korri.shell/moonlight /media/developer/org.korri.shell/moonlight/*
   ```
   Record only public cert/uniqueid hashes. Expect 0700 directory, three 0600
   caller-owned regular files. Close/relaunch; repeat and compare. After the
   separately coordinated TV reboot, repeat: the hashes must match and modes
   must be tightened again after devmode's boot chmod. Never alter real key
   files to test refusal. Symlink/corruption tests belong to disposable fixtures.
2. **Attest/provision haku, owner-authorized.** Launch Korri normally. Wait for
   haku discovery. Choose a haku game in Zone. This invokes public discovery,
   native-peer UUID attestation, existing certificate provisioning, pinned
   client-cert HTTPS app listing and exactly one transport app resolution. No
   manual PIN, captured TLS pin, Sunshine admin secret or hardcoded app ID.
   Record the exact far launch and child ownership. If startup fails, confirm
   that compensation targets that same launch; uncertainty is a failure, not
   permission to cancel another session.
3. **Picture.** Use the combined host-presentation integration (no diagnostic environment
   is needed for production full-window transparency). Confirm a moving game picture,
   not merely READY or accepted NDL buffers. The owner reports the TV's
   `com.webos.service.tv.capture/executeOneShot` DISPLAY capture includes the
   video plane; capture it with the existing operator tool. Record first-frame
   time and confirm the original Zone process stays alive.
4. **Sound.** Confirm audible stereo through the existing Pulse route, then
   game audio/video synchronization. No NDL audio or automatic fallback. Record
   dropouts and close/reopen behavior. Stop if audio/media ownership is uncertain.
5. **Pad input.** Release all controls at attachment. Test A/B/X/Y, D-pad,
   triggers, both sticks and stick clicks against the host game. Hold then unplug:
   the host must receive neutral state. Reattach while holding A: it must not
   synthesize a new press before release/rearm. The remote still navigates Zone;
   in-game remote mapping and Home/overlay are M5, not part of this test.
6. **Quit/return.** First quit through the game's own menu: far completion must
   stop/reap the child, release video/audio/pads and return to the same Zone PID.
   Separately exercise existing exact `app.session.leave`, Resume, and
   `app.session.end` via the owner's local authenticated control tooling. Leave
   keeps the far launch alive (the current shared freezer path freezes it),
   retires the decoder and stays Backgrounded during polling; Resume reconnects;
   End alone ends the exact far launch. Repeat with a second game and verify
   stale completion/stop cannot affect it. A network failure must use shared
   reconnect, never `/cancel` or End. No give-up acceptance is possible until
   product-wide #390 defines the bound.

Do not claim M4 from workstation tests: RTSP, Sunshine interoperability, visible
NDL video, Pulse routing and physical pad behavior still need those TV checks.
