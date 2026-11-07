# webOS M4: embedded Moonlight viewer

**Decision: Moonlight belongs inside `org.korri.shell`, under the TV-side
korrid's exact-launch viewer authority. The installed-app route is rejected.**

Owner, 2026-11-07: “We must embed Moonlight into the app. It is not something
that we reach out to.” This is “an embedded experience just like the handheld
devices and Android.” This supersedes the installed-viewer direction in
[the previous M4 brief](2026-11-07-webos-m4-stream-design.md) and the M4 status
in [the milestone table](2026-11-06-webos-milestones.md). Neither
`com.limelight.webos`, its identity, its host list, nor AppManager launches of
that application form part of this design. No PIN exchange is required.

This is workstation research, not TV acceptance. **No TV access, installation,
stream, pairing, power operation or haku connection was performed.** “Verified”
below means inspected source or the explicitly reported workstation check.
“Inferred” marks a composition that still needs B7 verification.

## Sources

| Source | Exact revision inspected |
| --- | --- |
| Korri | `62169596a` (base of this work); links below name the current source files |
| [Moonlight TV v1.6.36][mt] | `89221f66b79e06178a6465503cc5a306d2794786` |
| [ss4s][ss] | `9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2`, the gitlink in that Moonlight tag |
| [moonlight-common-c][mc] | `c86e0537d11566f9825d6323711ec61e3e702365`, the gitlink in that Moonlight tag |
| [Korri plugin producer][plugins] | `57de061520cc010cbc5ebe9b978f4799d159affa` (reviewed source, not a new observation of haku's deployment) |
| Android's existing embedded client | `clients/android/README.md`: retained Artemis `cf12c432`, common-c fork `c9994368`; the checked-in sources are authoritative for current integration |

The [webOS shell README](../../clients/shell/webos/README.md),
[SDK notes](../../clients/slint-spike/WEBOS-CROSS.md), and
[LG input/power report](2026-10-30-lg-b7-native-input-report.md) remain binding.
Build on the workstation only, one expensive command at a time,
`CARGO_BUILD_JOBS=2`. Do not bundle SDL-webOS 2.30 or initialize LG SDL's
joystick/gamecontroller subsystem. Do not bypass its TV-power registration.

## 1. The existing shape: korrid selects; a platform viewer performs effects

### Linux handhelds

Verified:

- [`stream_play/device.rs::registry`](../../services/korrid/src/stream_play/device.rs)
  injects `production_viewer` and the authenticated certificate broker into
  `UpstreamRegistry`. It wraps the viewer with `viewer_with_local_input`.
  [`upstreams.rs::prepare_stream`](../../services/korrid/src/upstreams.rs)
  prepares the selected peer game, retains its **exact far launch ID**, then
  starts the local viewer. If viewer start fails, it stops that same far launch;
  uncertain compensation remains an error, not an invented successful end.
- [`linux_viewer.rs::LinuxStreamViewer`](../../services/korrid/src/linux_viewer.rs)
  defines `start`, `status`, `background`, `resume_and_focus`, `stop` by exact
  launch ID. Its success contract requires a confirmed runtime state. It is
  already used on Android despite its Linux name.
- [`linux_viewer/production.rs`](../../services/korrid/src/linux_viewer/production.rs)
  resolves the enabled `@korri:moonlight` package's declared `moonlight`,
  `mapping`, and `identity-bootstrap` files. It starts **Moonlight Embedded's
  `bin/moonlight`**, not Moonlight Qt or a second game launcher. `stream_args`
  invokes `stream -app 'Korri Stream' -keydir …` with korrid's selected address,
  display/settings and occupied local input seats. The transport application
  is distinct from the selected Korri game and launch ID.
- `production_viewer` uses `/home/korri/.config/moonlight`. The plugin's
  [native setup unit][linux-setup] allocates it as runtime user `korri`, 0700.
  [Identity bootstrap][linux-bootstrap] invokes stock Moonlight identity
  generation, checks runtime ownership, regular files, 0600 and nonempty
  `client.pem`, `key.pem`, `uniqueid.dat`, then emits **only the bounded public
  certificate**. A deliberately invalid server pin prevents bootstrap network
  traffic. korrid does not read the private key. The stock client identity is
  separate from the federation's device key and the person's signer key.
- Start reads bounded public `/serverinfo`, obtains Sunshine's UUID, calls the
  certificate broker, and receives Sunshine's public certificate.
  [`host/viewer_units.rs`](../../services/korrid/src/host/viewer_units.rs) passes
  that certificate through a per-unit systemd credential. The viewer's
  `KORRI_MOONLIGHT_SERVER_CERTIFICATE` names that credential, not a copied keydir
  pin file. The plugin's [patch 0020][linux-pin] verifies TLS and pins the
  server public key. The helper and viewer use the viewer sandbox, not the
  ordinary game sandbox.
- The viewer runs in the exact `korri-game-<launch-id>.service` unit.
  `ProductionLinuxStreamViewer::observe` checks its unit state and compositor
  PID ownership. A completed unit is `Stopped`; a live unit is `Running` only
  when its own window has focus, otherwise `Backgrounded`. `focus` confirms
  the compositor tree after focusing the exact window or the known shell.
  Stop waits for the unit to be confirmed completed. No `pkill moonlight`.

### Pairing is certificate provisioning, not the PIN protocol

This is shared behavior, not a TV pairing invention:

1. The client generates and keeps its own Moonlight identity. Only its public
   client certificate leaves the viewer boundary.
2. [`upstreams.rs::moonlight_native_peer`](../../services/korrid/src/upstreams.rs)
   asks authenticated native peers to attest the observed Sunshine UUID. It
   requires one matching peer; an arbitrary address/UUID is not authority.
3. Existing `app.moonlight.certificate.provision` carries `hostUuid` and
   `clientCertificate`. [`authorization.rs`](../../services/korrid/src/authorization.rs)
   and [`lib.rs`'s dispatch](../../services/korrid/src/lib.rs) apply existing
   authorization. No new wire or privilege is needed for this public cert.
4. The holding korrid uses
   [`host/moonlight_certificate.rs`](../../services/korrid/src/host/moonlight_certificate.rs)
   to contact its protected, peer-credential-checked Unix `SOCK_SEQPACKET`
   Sunshine adapter. The [Sunshine producer patch][sun-cert] checks the actual
   host UUID and certificate, imports idempotently by certificate fingerprint,
   atomically updates its existing `root.named_devices` state, installs/checks
   the live verifier and returns the **public server certificate**. There is
   no PIN, web admin password, private-key transfer or separate TV trust list.
5. The viewer must authenticate subsequent HTTPS requests with its client
   identity **and verify/pin that broker-returned server certificate**.

### Stream end, Home and Linux's overlay

There are two independent observations:

- [`stream_play/device.rs::spawn_watch`](../../services/korrid/src/stream_play/device.rs)
  polls the **selected far Korri session** every second.
  [`UpstreamRegistry::session_status_resolved`](../../services/korrid/src/upstreams.rs)
  stops the exact local viewer and clears selection when that far launch ends.
- Local viewer exit is an actual completed unit. **It does not necessarily mean
  that the far Korri game ended.** In particular, `recover_viewer` in
  `upstreams.rs` starts a `Stopped` viewer again while an owned far launch is
  still selected. Merely returning `Stopped` is not a quit-to-Zone implementation.
  Stream-player participation has its own stop/leave logic. Do not conflate
  joining an existing stream with starting the selected peer game.

Home is a korrid action, not Moonlight's quit chord:
[`inputd/action_catalog.rs`](../../services/inputd/src/action_catalog.rs) maps
Home tap to the exact panel action;
[`inputd/main.rs`](../../services/inputd/src/main.rs) dispatches
`toggle_panel_exact` through
[`inputd/korrid_client.rs`](../../services/inputd/src/korrid_client.rs).
`app.session.leave` freezes the exact game and focuses the shell; for a selected
native peer, `session_freezer_resolved` freezes the far launch and backgrounds
the local viewer with compensation on failure.
[`host/viewer_input.rs`](../../services/korrid/src/host/viewer_input.rs) returns
input to the shell. [`overlay_intent.rs`](../../services/korrid/src/overlay_intent.rs)
records the exact Home handoff. [`surface/overlay.rs`](../../services/korrid/src/surface/overlay.rs)
produces the existing `GameplayOverlay` with Resume, End and Open Korri.
Resume thaws/refocuses; End stops the exact session and viewer.

**Important distinction:** Linux's viewer process can remain connected behind
the shell, but this Home path freezes the game. The inspected code establishes
focus switching, not a universal translucent overlay over moving video. A B7
video-plane overlay must be tested independently; do not cite Linux focus
switching as proof of webOS plane composition.

### Android

Verified current path:

- [`android/viewer.rs`](../../services/korrid/src/android/viewer.rs) implements
  the same exact-launch viewer trait through JNI. The embedded korrid selects,
  authorizes and controls the far launch.
  [`KorridServer.java`](../../clients/android/app/src/main/java/com/simonwjackson/korri/korrid/KorridServer.java)
  delegates platform viewer effects to
  [`KorriStreamViewer.java`](../../clients/android/app/src/main/java/com/limelight/KorriStreamViewer.java).
- `KorriStreamViewer.prepare` discovers serverinfo, gets the local public client
  certificate, calls Rust's `provisionMoonlightCertificate`, parses the returned
  server cert, checks authenticated pair state, and finds exactly one positive
  app ID named `Korri Stream` through the authenticated app list. It starts the
  bundled **Artemis `Game` Activity**, with the host, app ID, pinned certificate,
  exact Korri launch ID and an activity generation. There is no Java game
  authorization path or external Moonlight application.
- [`AndroidCryptoProvider.java`](../../clients/android/app/src/main/java/com/limelight/binding/crypto/AndroidCryptoProvider.java)
  loads or generates RSA-2048 and a self-signed SHA256withRSA X.509 certificate
  (20-year validity); stores PEM `client.crt` and PKCS8 `client.key` in the app's
  private `getFilesDir()`. [`IdentityManager.java`](../../clients/android/app/src/main/java/com/limelight/computers/IdentityManager.java)
  stores the separate 16-hex-character client `uniqueid` there. These are not
  the NIP-55 person's key or the Rust federation identity.
- [`Game.java`](../../clients/android/app/src/main/java/com/limelight/Game.java)
  retains the decoder, network and input implementation. `connectionStarted`,
  `connectionTerminated`, `stopConnection` and Activity destruction report to
  `KorriStreamViewer`; [`KorriStreamViewerState.java`](../../clients/android/app/src/main/java/com/limelight/KorriStreamViewerState.java)
  rejects stale generations/leases and waits for real connect/focus/release.
  Stop is not confirmed merely because `Activity.finish()` was requested.
- User leave goes through `KorriStreamViewer.leave` → Rust `leaveViewer` → the
  existing exact session leave. `background` raises `KorriShellActivity` and
  releases the Game decoder; resume launches a new exact Game when required.
  Android does **not** prove simultaneous Slint and video-plane composition.
- [`moonlight-core/Android.mk`](../../clients/android/app/src/main/jni/moonlight-core/Android.mk)
  builds common-c, ENet, Reed-Solomon and JNI into `libmoonlight-core.so`, with
  static Opus/OpenSSL. This is already an embedded Moonlight distribution.

## 2. B7 media candidates at the pinned Moonlight tag

`moonlight-common-c` supplies streaming transport, crypto, depacketization and
input packets. It does **not** supply GameStream HTTPS serverinfo, app listing,
`/launch`/`/resume`, certificate generation/pairing, a decoder, audio output or a
UI. Moonlight TV adds [libgamestream][gs], [connection/decoder callbacks][mt-video],
Opus decoding and ss4s. Linking common-c alone cannot play an app ID.

| Backend at pinned ss4s | Source evidence | B7 implication |
| --- | --- | --- |
| `smp-webos3` | [CMake][smp-build]: OS `<4`, legacy C++ ABI wrapper; [video][smp-video]: H.264/H.265, UI compositing; [player][smp-player]: Starfish load/feed/unload; [resource][smp-acb]: AcbAPI main sink and display area | First **proposed experiment** for 3.9.3. Video through the system Starfish hardware pipeline, not software decode into SDL. Requires `libplayerAPIs`, AcbAPI, pbnjson and GLib. No webOS 5 exported SDL window path. |
| `ndl-webos4` (name is misleading) | [CMake][ndl-build]: OS `>=3.5.0,<5`; [video][ndl-video]: `NDL_DirectVideoOpen`, `PlayWithCallback`, `SetArea`, H.264 only, UI compositing | Also version-eligible for 3.9.3. Direct hardware video API; not NDL webOS 5 and not the ESPlayer backend. |
| `ndl-esplayer` / `ndl-webos5` | [ESPlayer CMake][es-build]: `>=2.2,<3.5`; [webOS 5 CMake][ndl5-build]: `>=5` | Neither version range includes 3.9.3. Do not choose by library name alone. |
| SMP audio on webOS 3 | [module][smp-module] removes audio capability unless `SS4S_SMP_AUDIO` is set; only the webOS 5 target sets it | The backend contains audio plumbing, but the pinned webOS 3 module does **not advertise usable audio**. Do not claim SMP alone solves sound. |
| PulseAudio | [pulse module][pulse]: PCM S16LE via `pa_simple_new`/`pa_simple_write`; Moonlight's [audio callback][mt-audio] decodes Opus when the backend cannot accept Opus | Proposed stereo audio companion to SMP. App-jail access and routing need a physical-TV test. Do not copy Linux's PipeWire socket path to webOS. |
| NDL audio | [module][ndl-module] masks audio on `m16p`; [audio][ndl-audio] supports stereo PCM and compressed AAC/AC3 | Read-only machine/SoC evidence must precede selecting it. No claim that the actual B7 audio backend has been measured. |

**Additional NDL risk (high):** [NDL initialization][ndl-module] calls
`SS4S_webOS_M3_KADP_Fix` unconditionally after direct-media init.
[That helper][kadp] uses `dlsym`, `mprotect(PROT_WRITE|PROT_EXEC)` and instruction
patching of `MS_VDEC_Init` in the process's vendor library mapping. Reading a
machine name does not restrict the patch to one named machine. This is not
firmware writing, but it is an unreviewed executable-memory workaround. Do not
silently enable it or use NDL as an automatic fallback. SMP is the smaller
first experiment because it does not call that helper. The common
[jail check][jail] also rejects particular broken `k3lp`/`k5lp` jails; never
remove that check or grant new device access to get past it.

### Does the hardware plane leave graphics drawable above it?

**Yes at the backend contract/source level; not yet verified for Korri on the
B7.** Both SMP and the eligible NDL backend advertise
`SS4S_VIDEO_CAP_TRANSFORM_UI_COMPOSITING`. The [public ss4s definition][ss-video]
explicitly says “Video can be blended with UI”, distinct from `UI_EXCLUSIVE`.
Moonlight [closes its UI only for an exclusive decoder][mt-video]; its
[streaming UI][mt-overlay] draws its own controls while playback exists.
SMP's AcbAPI resource controls the main video sink, separately from the shell's
SDL/Wayland graphics buffers. That supports, but does not establish on this
jail, the intended hardware-video-plane plus graphics-plane model.

The shell must keep its original graphics window alive. Its current opaque
Zone frame can obscure video beneath it; keeping Zone visible during a decode
experiment is useful proof that drawing is still possible, not proof that a
full-screen game will be visible through Zone. M5 needs a design engineer to
use the existing overlay presentation on a correctly composited graphics
surface. This brief makes no visual design, layout or color choice. Verify
alpha/opaque-region behavior, stacking, fullscreen handling and app-ID resource
ownership before promising overlay support. No second SDL window, second host
or another app's foreground/focus lifecycle is required by this proposal.

## 3. Proposed edge and exact lifecycle

**No persisted schema, new treaty or process-control wire is introduced by this
research. The following integration is a proposal, not a shipped adapter.**

- Bundle a dedicated minimal Moonlight viewer executable and only its required
  distributable libraries in the **same IPK**. Use common-c plus the selected
  ss4s backend, Opus and a bounded, certificate-pinned GameStream control client.
  Do not bring Moonlight TV's launcher, host database, LVGL surface, SDL
  backport, auto-pair UI, screensaver/power hooks or joystick enumeration.
- Compose a TV adapter through the existing embedded viewer seam
  (`EmbeddedPlatform` and `start_local_shell_server` in
  [`lib.rs`](../../services/korrid/src/lib.rs)). Today the
  [`webos-korrid` binary](../../services/korrid/src/bin/webos-korrid.rs) supplies
  no viewer. Its injection API is crate-private; adding an appropriate public
  composition seam needs implementation review, not another viewer registry.
- Use the **TV's own** Moonlight identity. Keep private key material inside the
  viewer boundary. Bootstrap exposes only its public certificate, analogous
  to Linux. Exact persistent allocation and stock identity-file producer must
  be approved before code creates them; see section 6. Do not repurpose the
  existing federation `identity/device.key` or read another app's identity.
- Read public serverinfo for the selected route; call the existing attestation
  and provisioning broker. Supply the returned public server certificate to
  the viewer via a private inherited resource, not key contents in argv/logs.
  Resolve exactly one `Korri Stream` through the authenticated app list, as
  Android does. That app ID is the transport, not the selected game's ID.
  Do not persist a TV app-ID mapping or hardcode yesterday's calculated ID.
- korrid owns one child handle for the exact far launch. Spawn without a shell;
  retain/reap the child and its termination result. An owned handle, not a
  process-name search or reused naked PID, authorizes stop. Bound start/stop,
  output and shutdown; ensure the child cannot outlive korrid. Do not pass the
  shell RPC capability to it. The viewer uses `APPID=org.korri.shell` for the
  media resource, not a new application registration.
- Define startup success from actual connection/decoder readiness, **not**
  `fork` success. common-c's connection callback and first accepted decode unit
  are useful observations, but neither proves a visible hardware first frame.
  The adapter must not claim exact success without the agreed readiness signal.
  Start failure reaps this child before existing far-launch compensation runs.
- On disconnect or fatal decode failure the viewer tears down common-c, audio
  and media resources, then exits/reports once. `LiStopConnection` must run
  outside common-c callbacks; callbacks wake the owning control thread (the
  pinned [Moonlight connection callbacks][mt-connection] follow this shape).
- **A local exit must enter an explicit exact-session transition before normal
  polling can restart it.** Reuse Android's user-leave path for an intentional
  leave: exact `app.session.leave`, freeze the owned far game, hand input back
  to the shell and show the existing overlay/catalog behavior. End remains
  existing `app.session.end`, not GameStream `/cancel` as a second game-stop
  authority. The stopped-viewer recovery branch above needs a regression test
  for this ordering. Do not merely expose process death as `Stopped` and call
  M4 done. Whether an unexpected network/decoder failure leaves/freeze or keeps
  the current reconnect behavior is still an owner decision.
- `stop(launch)` confirms this child's exit and resource release. It is
  idempotent for that retired exact launch and never targets a replacement.
  Far-session completion follows the existing watch → stop → clear selection
  path. Background/resume must implement the existing confirmed semantics;
  whether to retain the media pipeline behind the graphics plane or release
  and reconnect it (Android's pattern) depends on the first experiments.

**Security blocker for reusing libgamestream unmodified:** its pinned
[`http.c`][gs-http] disables both TLS peer and hostname verification, grows
response buffers without a cap and debug-logs request URLs (which can include
session material). It does not consume korrid's broker-returned server pin.
Reuse of that code needs a reviewed pinning/bounds/logging patch, or reuse the
already hardened equivalent control path. This brief does not authorize a
“temporary” insecure viewer CLI. `moonlight-common-c` does not fix the HTTPS
control client for us.

## 4. Play-time input without shell evdev

The other worker's optional `EmbeddedPlatform` native-input callback and
`SurfaceHostInputSource` are the input **producer seam**, not yet an authorized
viewer input channel. Follow that worker's actual implementation; do not invent
a parallel reader or claim that a browsing button proves game input forwarding.

| Source / destination | Proposed ownership and existing grounding |
| --- | --- |
| Pad → korrid | TV-side native-input edge alone opens `/dev/input`, reads evdev capabilities/ranges and owns disconnect/held-state recovery. The shell never opens it. Keep a single physical-device producer. |
| korrid → Zone/Pico | Existing native input frames reach the shell mapper and become semantic buttons, as now. Surfaces receive no raw keys or hardware identity. |
| korrid → Moonlight during play | Before semantic reduction, preserve the native controller buttons, analog axes/triggers and seat identity. Adapt to common-c `LiSendMultiControllerEvent` / arrival APIs. Home/host actions are intercepted locally, not blindly sent to Sunshine. Semantic Up/Confirm alone cannot reconstruct an analog gamepad. |
| Remote → game | The existing LG video-only SDL keyboard event path receives physical remote keys in the shell. It is not established that the native-input callback exposes the remote. Playback forwarding needs a reviewed host-side route before adding code; do not translate Zone's semantic requests back into raw game input. Exact remote-key mapping and reserved Back/Home behavior need the owner and real B7 events. |
| Overlay handoff | Gate forwarding on the exact selected launch and confirmed viewer state. Neutralize the host gamepad before routing to the overlay; consume host actions; on resume reconcile a current full snapshot so stale presses cannot stick. On unplug/EOF/stop/send failure send neutral state and retire that seat. |

**Approval gate:** a new cross-process stream-input or remote-event message,
new treaty variant, or extension of the shell input direction requires the
owner's approval **before implementation**. This brief intentionally defines no
packet structure, socket name, environment schema, persisted seat mapping or
new `SurfaceRequest`. A process viewer keeps crash/lifecycle ownership simple
but requires that approved input/readiness transport. An in-process viewer
library can consume an edge callback without a new serialization format, but
shares the brain's crash/address-space boundary. That trade-off is still open;
it is not permission to hide an ad-hoc wire in a private pipe.

## 5. Licensing facts, not a product/legal decision

| Component | Inspected license evidence | Distribution consequence stated by those texts |
| --- | --- | --- |
| moonlight-common-c | [LICENSE][mc-license] is GPL version 3; ENet and Reed-Solomon carry their own notices | Shipping its compiled implementation is shipping GPL-covered code, including when linked as a library. Preserve notices and provide the applicable Corresponding Source under GPL section 6. |
| Moonlight TV, including libgamestream | [LICENSE][mt-license] is GPL version 3; [libgamestream's HTTP source][gs-http] says GPL version 3 **or later** | Copying/adapting its control/session code is not license-free because its launcher is omitted. A linked combined viewer must satisfy the applicable GPL terms. |
| ss4s | [LICENSE][ss-license] is LGPL version 3, incorporating GPLv3; section 4 covers Combined Works | LGPL adds requirements for notices, modification/relinking and reverse-engineering permissions. Dynamic modules do not erase these requirements. Dependencies/vendor libraries need their own review. |
| Current Android distribution | [`clients/android/LICENSE.txt`](../../clients/android/LICENSE.txt) is GPLv3; its [native makefile](../../clients/android/app/src/main/jni/moonlight-core/Android.mk) compiles common-c/JNI into the APK | Korri already bundles an embedded Moonlight implementation on Android. This is evidence of current code/package composition, not proof of compliance for every distribution or permission to license webOS differently. |
| Same IPK versus same linked executable | GPLv3 sections 5–6 discuss covered/combined works and distinguish an aggregate from a larger program | A separate executable in the IPK does not automatically determine which other components are covered. This brief makes no conclusion that IPC avoids GPL or that every file in the IPK becomes GPL. The owner must decide the distribution/linkage after review, including Slint and proprietary LG media APIs. |

A release needs exact source pins, local patches, build/install instructions,
notices and the applicable source/relinking materials. Do not ship SDK/vendor
runtime libraries merely because the workstation linker can find them. No
publication or legal compatibility approval is implied here.

## 6. Decisions left to the owner before integration

No decision below is implemented or encoded as an illustrative schema.

1. **Identity persistence:** approve the stock identity producer/file format
   and its TV allocation. Linux's `client.pem`/`key.pem`/`uniqueid.dat` and
   Android's distinct producer are grounded examples, not permission to create
   new files beneath the owner's existing TV state. The B7 devmode boot
   `chmod -R 777` also affects any identity placed under `/media/developer`;
   the existing edge's directory tightening does not automatically validate
   new viewer key files. Fail closed on untrusted/corrupt files; do not silently
   rotate the identity as the stock fallback might. The installed Moonlight
   identity remains unrelated and untouched.
2. **Process/library and transport:** after the media experiment, select child
   process versus library and approve any input/readiness/control wire required.
   Default architectural proposal is an owned child, not an external app.
3. **Exit intent:** intentional Leave follows the Android exact-leave precedent;
   End follows exact session end. Decide user-facing quit semantics and the
   unexpected disconnect policy before changing reconnect behavior. A stopped
   child alone is not sufficient evidence to stop the far game.
4. **Play keys:** choose the remote mapping and the local overlay/quit key from
   real physical events. LG's system Home may go to the TV launcher; do not
   assume it arrives at our app. Do not steal Back from Zone based on a guess.
5. **License/distribution:** approve the final linkage, source delivery and LGPL
   relinking plan. This research reports licenses; it does not make that choice.

## 7. Tomorrow's TV experiments, smallest first

The parent owns TV access. First confirm that the user is ready and that the
TV UI responds. Coordinate readiness before a timed observation. Use the
normal `org.korri.shell` UID/jail and prebuilt, source-pinned artifacts only.
Keep the known-working shell/system SDL; no power-policy changes, permission
expansion, root media test, vendor-library replacement or installed-app keys.

1. **Read-only preflight.** Record OS 3.9.3, machine/SoC, actual available
   `libplayerAPIs`/AcbAPI/NDL/Pulse SONAMEs and public ABI symbols in the jail.
   Compare to SDK link dependencies, including C++ ABI and pbnjson SONAMEs.
   Check for a pre-existing unrelated media session; do not disrupt it. Confirm
   backup/recovery and unchanged system SDL provenance from the input report.
2. **Media with no federation and no identity.** Load only the chosen SMP webOS
   3 decoder and decode a known local H.264 test elementary stream while the
   original Zone graphics window stays drawable. Start with video only and
   normal `APPID=org.korri.shell`. Observe visible video/graphics, ownership
   failures, decoder cleanup and return to the same Zone process. Do not treat
   a successfully linked shared object or accepted load request as a picture.
3. **Plane composition.** With that same test stream playing, prove graphics
   can update above the video plane and that an appropriately composited test
   graphics surface can expose the video. Use a design-engineer candidate,
   not a speculative Zone redesign. Record alpha/stacking, fullscreen behavior,
   first-frame latency and any stuck video after close. Stop if the shell
   cannot draw or resource release is uncertain. This decides M5 feasibility.
4. **Audio separately.** In the same jail test stereo PCM through Pulse, then
   video plus audio. Check audible output, latency/synchronization, close and a
   second open. NDL is a separate reviewed experiment only if SMP/Pulse fails;
   do not automatically enter the executable-memory workaround or power hooks.
5. **Korri-owned trust.** Only after identity allocation approval, generate the
   TV's own client identity and prove restart/reboot persistence and private
   file checks. Read serverinfo, attest the real UUID through native korrid,
   provision that public certificate, then fetch the app list using verified,
   pinned HTTPS. Confirm one `Korri Stream`; never use a guessed app ID or PIN.
   Check wrong/missing pin refusal without logging cert/session/key material.
6. **First real stream (M4).** Select a haku game in Zone. Observe its exact
   far launch, the owned viewer child, visible hardware-decoded video, sound
   and real remote/pad input. This is the first M4 playback claim. Record the
   source SHA, artifact hashes, first-frame/start/stop latency, RSS and CPU.
7. **Quit/return before overlay polish.** Exercise chosen Leave and End
   separately; confirm the same Zone process returns, exact far state agrees,
   input is neutral, media/audio resources are gone where required, and the
   watcher does not restart a quit viewer. Repeat with a second game. Then
   test far-game-end, viewer crash, network loss, start rejection, stale launch
   completion and killing the owned korrid. Never target another launch.
8. **M5.** Deliver the chosen local host action through korrid, show the existing
   gameplay overlay, Resume and End. Confirm no gameplay input leaks during
   overlay focus, no held button survives a disconnect, and that the measured
   plane composition works without restarting Zone. Record whether the game is
   frozen and whether the stream pipeline remains connected; these are distinct.

## Workstation result

A **partial**, reproducible [media-link spike](../../clients/webos-embedded-viewer-spike/README.md)
was built through `CARGO_BUILD_JOBS=2 nix run .#webos-embedded-viewer-spike-build`.
Nix hash-pins the existing prebuilt SDK, common-c, its ENet gitlink and ss4s.
It builds common-c, the ss4s core, the SMP webOS 3 module and a link-probe ELF.
It never executes ARM code, initializes a decoder or contacts a host/device.
The complete certificate-and-app-ID playback CLI was **not implemented**.

Verified results:

| Check | Result |
| --- | --- |
| Real cross compilation and final executable/module linkage | Pass, two compiler jobs; no NDL or SDL target |
| ELF inspection of all four output ELFs | ARM32 soft-float; executable interpreter `/lib/ld-linux.so.3`; maximum GLIBC `2.4`; SMP maximum GLIBCXX `3.4`; no RPATH/RUNPATH |
| Real-readelf artifact tests | Four passed: valid artifacts, missing protocol library, corrupted machine identity, too-new GLIBC requirement |
| Static analysis | Ruff check/format and strict mypy passed for both Python files; Nixfmt check passed for the spike expression and flake; probe and ss4s core compile with `-Wall -Wextra -Werror` |
| Citation checks | 34 local links and 32 pinned source paths verified before adding this result link; `git diff --check` passed |

The NEEDED report is included in the Nix output. In particular, common-c needs
`libcrypto.so.3`; SMP needs `libplayerAPIs.so.1`, `libAcbAPI.so.1`,
`libpbnjson_c.so.2`, GLib and the C++ runtime. **This is not a self-contained or
TV-runnable package.** SDK linkage does not prove these actual B7 SONAMEs or
vendor C++ exports. No vendor libraries, SDL or TV libc are bundled. The SDK
host-tool preparation also covers its internal `arm-webos-linux-gnueabi/bin`
assembler/linker, not only top-level wrappers. CMake's legacy policy minimum
is set as in the pinned Moonlight build; no upstream source patch was needed.

No full Rust test suite was run: production Rust, the surface treaty and
persisted TV state are unchanged. No workstation result proves B7 playback,
audio, input, resource arbitration, M4 acceptance or M5 plane stacking.

[mt]: https://github.com/mariotaku/moonlight-tv/tree/89221f66b79e06178a6465503cc5a306d2794786
[ss]: https://github.com/mariotaku/ss4s/tree/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2
[mc]: https://github.com/moonlight-stream/moonlight-common-c/tree/c86e0537d11566f9825d6323711ec61e3e702365
[plugins]: https://github.com/korri-os/plugins/tree/57de061520cc010cbc5ebe9b978f4799d159affa
[linux-setup]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/moonlight/korri-moonlight-setup.service
[linux-bootstrap]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/moonlight/identity-bootstrap.py
[linux-pin]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/moonlight/README.md#server-certificate-pinning-new-korri-security-patch
[sun-cert]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/services/sunshine/patches/0020-add-korrid-certificate-control.patch
[gs]: https://github.com/mariotaku/moonlight-tv/tree/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream
[gs-http]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream/src/http.c
[mt-video]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/stream/video/session_video.c
[mt-audio]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/stream/audio/session_audio.c
[mt-connection]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/stream/connection/session_connnection.c
[mt-overlay]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/streaming/streaming.view.c
[smp-build]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/smp/CMakeLists.txt
[smp-video]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/smp/src/smp_video.c
[smp-player]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/smp/src/smp_player.c
[smp-acb]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/smp/src/smp_resource_acb.c
[smp-module]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/smp/src/smp_module.c
[ndl-build]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/ndl/webos4/CMakeLists.txt
[ndl-video]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/ndl/webos4/ndl_video.c
[ndl-audio]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/ndl/webos4/ndl_audio.c
[ndl-module]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/ndl/webos4/ndl_module.c
[es-build]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/ndl/esplayer/CMakeLists.txt
[ndl5-build]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/ndl/webos5/CMakeLists.txt
[kadp]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/utils/m3_kadp_fix.c
[jail]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/webos/utils/jail_check.c
[pulse]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/pulse/pulse.c
[ss-video]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/include/ss4s/video.h
[mc-license]: https://github.com/moonlight-stream/moonlight-common-c/blob/c86e0537d11566f9825d6323711ec61e3e702365/LICENSE.txt
[mt-license]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/LICENSE.txt
[ss-license]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/LICENSE
