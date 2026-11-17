# Korri viewer — stage 1

A Linux process owned by Korri, under GPL-3.0-or-later (ADR 0059).
The code belongs in Korri, not the plugin repository (ADR 0061). It is in
`clients/viewer` because the viewer belongs to a device's client part
(`CONTEXT.md`). It is not in korrid or the shell (ADR 0060).

```sh
nix build .#korri-viewer
nix develop .#viewer --command xvfb-run -a env SDL_RENDER_DRIVER=software cargo test --manifest-path clients/viewer/Cargo.toml
nix develop .#viewer --command cargo clippy --manifest-path clients/viewer/Cargo.toml --all-targets -- -D warnings
nix build .#checks.x86_64-linux.korri-viewer-picture
nix build .#checks.x86_64-linux.korri-viewer-io
# The Rust audio output (#391 step F, below): its package runs its tests.
nix build .#korri-viewer-rust-audio-output
nix develop .#viewer --command xvfb-run -a cargo test --manifest-path clients/viewer/Cargo.toml --features rust-audio-output audio_output
# The Rust EGL presenter (#391 step F, below): its package draws the same
# surfaces with both presenters through Mesa's software EGL and compares
# the pixels.
nix build .#korri-viewer-rust-egl-presenter
# The Rust X11 window and its input (#391 step F, below): its package runs
# both windows on private Xvfb servers and compares what they do.
nix build .#korri-viewer-rust-x11-window
# Every VA-API picture through the Rust EGL presenter (#391 step F, below):
# the Rust EGL presenter's binary with -filledpicture egl.
nix build .#korri-viewer-rust-egl-filled-picture
# Handheld path: aarch64 builds on aarch64 builders (here fuji or yari), never
# on a device. The x86_64 check then starts that build under qemu-aarch64.
nix build .#packages.aarch64-linux.korri-viewer
nix build .#checks.x86_64-linux.korri-viewer-aarch64
clients/viewer/proof/v4l2request/compare.sh ../korri-plugins
# The session part for Android, both ABIs (below). No device or emulator.
nix build .#checks.x86_64-linux.korri-viewer-android
# The Android app (slices D1 to D5, D4b and #476's size change, below), and
# its proof on one headless emulator against a private Sunshine on aka.
# Nothing heavy runs on zao: run both on aka itself, the proof against
# aka's own fixture, which the emulator reaches at aka's Tailscale address.
# --build-store auto builds in aka's own store; without it a proof builds
# nothing and refuses what is in neither this host's store nor a cache.
nix build .#korri-viewer-android-apk
nix run .#viewer-android-proof -- --build-store auto --sender local --stream-host AKA_TAILSCALE_ADDRESS
# The decoder's recovery (#490): emulator and sender fixture on this host.
nix run .#viewer-android-recovery-proof -- --build-store auto --stream-host AKA_TAILSCALE_ADDRESS
```

## Boundaries

- `src/session.rs`, `pair.rs`, `stream.rs`: Rust owns HTTPS discovery,
  application selection, launch/resume, PIN pairing for the private fixture,
  connection start, termination and stop. SIGINT, SIGTERM, SIGHUP and SIGQUIT
  disconnect. Disconnect does not cancel the remote application.
- `src/network_core.rs` is the seam to the network core (#391 step E): the
  trait `NetworkCore` holds the connection's start, interrupt and stop, the
  input packets, the core's clock, its round trip and its pending sound.
  The platform's video, audio and listener callback tables go to the core
  at the start, as `LiStartConnection` takes them. `stream.rs` selects the
  core (`-networkcore`, `c` by default) before it starts, and the input and
  readings of Android's Rust code and of Linux's carried C code reach that
  core (`network_core::active`). The carried C code keeps Moonlight's
  `LiSend*` names: `build.rs` force-includes `src/network_core/c_calls.h`,
  which renames them to `src/network_core/c_calls.rs`. The C core
  (`src/network_core/moonlight_common_c.rs`) makes the same calls as before.
- `src/network.rs` is private. `build.rs` generates the small FFI from the
  pinned core's own `Limelight.h`, through bindgen. No `moonlight-common-sys`.
  Nix builds the unmodified core and its pinned ENet once, with CMake;
  Rust links its shared library. The core is the gitlink
  `b126e481a195fdc7152d211def17190e3434bcce` from Moonlight Embedded
  `775444287305849ebdf4736c75298ad0713e2d5d`. `default.nix` pins the recursive
  source by its NAR hash. No second network core is linked.
- `picture/` holds the carried C decoder and presenters. `io/` holds the
  carried C Opus/output and input code. Rust still owns the session and its
  lifetime. No Moonlight C session or HTTP code is compiled.
- Sound follows Moonlight's platform selection: SDL audio on the software
  fallback and on `v4l2request`; on X11 VA-API, probe Pulse first, then use
  ALSA. The fixture's
  isolated null ALSA configuration works without a sound server. The Rust
  port of that code chooses the same way ("Sound in Rust", below).
- The unchanged stage-0 tool needs `bin/moonlight` and the mapping path
  `share/moonlight/korri-inputplumber-gamecontrollerdb.txt`. The binary name
  is a package-entry symlink, not a second implementation. The mapping is
  the pinned upstream controller database with the plugin package's two
  InputPlumber corrections. It accepts the PC flags emitted by
  `linux_viewer::stream_args` and the plugin's rendered settings. On PC,
  `-input` fails explicitly: korrid passes no explicit input node there.
  X11 VA-API uses evdev/udev discovery plus X11 keyboard and mouse. SDL
  fallback uses SDL controllers, keyboard and mouse. The aarch64 build also
  accepts the handheld arguments (below). `-viewonly` skips input device
  discovery and initialization: no udev, evdev, SDL controller or CEC. As in
  Moonlight, the window's own keyboard and mouse still forward.

`KORRI_MOONLIGHT_SERVER_CERTIFICATE` is mandatory, including for fixture
pairing. HTTPS checks both that trust anchor and its SPKI, with hostname
verification off because Sunshine does not use DNS names in its certificate.
This matches the security of plugin patch 0020 and fails closed when the pin
is missing, malformed or wrong. HTTP discovers only the HTTPS port and serves
the PIN exchange. Authenticated serverinfo, applist and launch/resume have no
HTTP fallback. The existing `client.pem`, `key.pem` and `uniqueid.dat` names
come from Moonlight's key-directory producer; korrid provisioning is unchanged.
Each HTTP connection closes once its answer is in (`CURLOPT_FORBID_REUSE`,
#510): Sunshine closes it after the answer and waits for the client, and
without korri-plugins patch 0045 its HTTPS serves no one else meanwhile. A
connection kept for reuse held it for the whole stream, and the next request
went into the closed connection before a new one. Callers in one process
make the key pair in turn (`Identity::load_or_generate`), so the Android app
can make it when it starts while a launch asks for it.

The Rust HTTP/session and PIN exchange follow Moonlight Embedded's
`libgamestream/client.c`, Copyright (C) 2015–2017 Iwan Timmer. The Rust
adaptation is Korri work dated 2026-11-13, under the same GPL-3.0-or-later
terms. Original notices remain on the carried C files.

## Picture provenance

The source is Moonlight Embedded at the pin above, with the plugin patch stack
through `korri-plugins` commit `2094c1e77471` (#466). In particular:

| Change retained | Plugin source |
| --- | --- |
| Crop coded padding on present | patch 0015 |
| Present at the current X11 window size | patch 0024 |
| Initialize VA-API on the existing X11 display | `688b54b`, patch 0025 (#439) |
| Independent presentation display, retained newest frame, released frame references | `9445f79`, `2094c1e`, patch 0026 and regression tests (#466) |

`video/{ffmpeg,ffmpeg_vaapi,sdl}.c` and their headers are copied, not
rewritten (`video/egl.c` was too, until its removal on 2026-11-17: "EGL
presenter in Rust"). The generic FFmpeg decoder selection stays unchanged.
The x86_64 package compiles only these PC files and links the unpatched
FFmpeg. The handheld file and its FFmpeg are in the next section.

The adaptations do not change decode, queue or presentation policy:

- `video/x11.c` routes input setup through the boundary in `io/`.
- `sdl.c` routes event forwarding through the same boundary. Its existing event
  loop becomes a single timed poll, so Rust owns the lifetime. Texture
  updates, two-frame skip rule, renderer, vsync and rendering calls stay the
  same. Teardown also releases its SDL texture, renderer and mutex.
- `bridge.c` supplies the descriptor loop used by X11. The core's C-variadic
  log callback is `src/core_log.c`, on both platforms. Rust polls either
  presenter, then stops and joins the network core before destroying the
  presenter. No connection logic lives in C.
- The frame record (below) adds calls and no policy. `video/ffmpeg.c` gives
  each packet a pts, so that the frame that the decoder returns names its
  decode unit. `video/x11.c` and `video/sdl.c` report each unit and its
  decoded frame. `video/ffmpeg_vaapi.c` reports the hand-off after a
  successful `vaPutSurface`. `video/sdl.c` puts the decoded `AVFrame` in its
  event, not its two arrays; `sdl.c` reads the same planes and the pts from
  it under the same mutex, and reports the hand-off after
  `SDL_RenderPresent`. With no record, each call returns at once and every
  pts stays `AV_NOPTS_VALUE`, as before.

One presentation path is new (#477). Intel's VA-API drivers present
`vaPutSurface` through DRI2, and Xwayland has only DRI3, so on an Intel PC
every `vaPutSurface` returns "invalid parameter". When the first
`vaPutSurface` of a session fails, `video/ffmpeg_vaapi.c` gives that frame and
every later one to `video/vaapi_egl.c`. It exports the decoder's surface as a
dmabuf, imports it as an EGL image, and draws it over the window; the driver
converts the colour by the frame's matrix and range. Decode stays on the GPU,
and the retained newest frame policy is the same. A device whose
`vaPutSurface` works (Mesa on AMD) never takes this path. No check in the
build sandbox reaches it, because the sandbox has no GPU: the proof is a run
on the device (`proof/ibuki-egl/`).

The two copied C regression tests use a real Xvfb. They exercise VA failure
cleanup, resize destinations, newest-frame replacement during presentation,
and frame references after drops and presentation. A third C test decodes
and presents four actual H.264 frames through the SDL software fallback,
then reads the rendered pixels and the frame record of those four frames.
It uses Xvfb and Mesa software rendering, not a device. A fourth checks the
frame record's rows, empty times and recording with real files and the
network core's own clock. Rust tests exercise real TLS and HTTP requests, CLI compatibility,
pin failure, fresh mutual-TLS authentication and launch selection.

## Handheld picture path (`v4l2request`, aarch64 only)

Moonlight's `v4l2request` platform, used on the RK3566 (RG353M) and the H700
(RG35XX Pro, RG35XX SP), from the same source and patch stack as above
(patches 0021 and 0022 add it). It is carried for `aarch64-linux` only, as
groundwork for step G of #391. No device ran it in this slice; the bench
proof is a later step. The `v4l2m2m` (Qualcomm) path is not carried:
ADR 0063 postpones those devices.

Moonlight keeps both platforms in one file, `src/video/v4l2m2m.c`.
`picture/video/v4l2m2m.c` is that file with 1016 lines removed and 1 line
changed. Nothing is added. The removed code is all code that the
`v4l2request` platform cannot reach:

| Removed from the patched file | Why the request path cannot reach it |
| --- | --- |
| The stateful decoder setup, `get_drm_prime_fmt`, `find_v4l2m2m_decoder`, `v4l2m2m_setup_stateful`, `decoder_callbacks_v4l2m2m` | Only `-platform v4l2m2m` selects them. |
| The "Plan C" direct V4L2 decoder: its state, ioctls, setup, submit and cleanup, and its dma-buf presenter and caches | `v4l2m2m_setup` sets `direct_v4l2` to false in request mode, after reading `MOONLIGHT_V4L2M2M_DIRECT`. The read and the flag go. |
| The SDL renderer presenter: `draw_frame_nv12`, its texture and the renderer branch of `display_loop` | Request mode sets `direct_dmabuf_present`, so `display_loop` makes a GLES context, never a renderer. `get_request_fmt` accepts only DRM PRIME frames, so no NV12 frame reaches `draw_frame`. |
| `import_dmabuf` | Moonlight never calls it. |
| The blocking tail of `v4l2m2m_setup` | Request mode returns before it. The final `return 0;` stays. |

The changed line is in `draw_frame`. The dispatch that started with the
removed direct-presenter branch now starts at
`if (frame->format == AV_PIX_FMT_DRM_PRIME)`. If an NV12 frame did arrive,
Moonlight's removed branch would return without drawing; here the remaining
`else` logs it once and draws nothing. The header comment, the
`v4l2m2m:` log prefix and the `v4l2m2m_*` names stay as Moonlight wrote them.
`video/video.h` is unchanged; `HAVE_V4L2M2M` is Moonlight's guard for the
`decoder_callbacks_v4l2request` declaration, so aarch64 builds define it.
Modification notice (GPL-3 §5a): Korri modified `picture/video/v4l2m2m.c`
on 2026-11-13 by these removals and the one changed line.

`ffmpeg/0001-v4l2-request.patch` is byte for byte the plugin's FFmpeg patch
(Jonas Karlman's V4L2 Request hwaccel series as LibreELEC ships it).
`default.nix` copies the plugin's `ffmpegV4l2Request` expression: Opus
without custom modes, that patch, `--enable-libudev`,
`--enable-v4l2-request` and `systemdMinimal`. One difference: it patches
this repository's nixpkgs FFmpeg, 9.0.1; the plugin's pin is 9.0.2. Applied
to both releases, the patch makes the same text in the 27 files it touches,
except where 9.0.2's own fixes differ: `h264_slice.c` (error concealment for
a second field, which only field-coded H.264 has), `hevc/hevcdec.c` (slices
of a skipped picture), `vp8.c` and `vp9.c`. The x86_64 viewer already uses
9.0.1.

The platform follows Moonlight's `main.c` and `platform.c` for
`V4L2REQUEST`, from korrid's arguments
(`linux_viewer::production::stream_args`):

- `-platform v4l2request` selects it. `build.rs` compiles the file and sets
  the `v4l2request` Rust cfg only for an aarch64 Linux target. Other platforms keep
  their PC behaviour on aarch64. `-platform v4l2m2m` fails.
- Video: `decoder_callbacks_v4l2request`. Nothing initializes SDL or X11
  before it; the decoder's display thread makes its own window. `auto` codec
  means H.264 only, as `platform_prefers_codec` says.
- `-autowindowresize` sets `MOONLIGHT_AUTO_WINDOW_RESIZE` to `1`, otherwise
  `0`, as `main.c` does; the presenter reads it. `picture/bridge.c` sets it
  before the network core and the display thread start.
- Sound: SDL audio, as `platform_get_audio` chooses. `-audio` is accepted and
  ignored, as Moonlight does outside its SDL platform.
- Input: Moonlight's embedded branch. korrid's `-input` nodes (at most six,
  Moonlight's `MAX_INPUTS`) are opened before udev starts, and udev adds no
  other device. With no `-input`, udev discovers devices, as on X11 VA-API.
  `-viewonly` skips input. CEC stays omitted, as on PC.
- The loop: Rust polls Moonlight's descriptor loop (evdev and udev), as on
  X11 VA-API, and owns signals and teardown. The display thread pumps its own
  SDL window events, as in Moonlight.
- Encryption: on aarch64 the viewer reads `HWCAP_AES` from the auxiliary
  vector, as Moonlight's `has_fast_aes` does on ARM. With it, audio and video
  are encrypted; without it, audio only. `stream.rs` keeps the x86 CPU check
  for x86 targets; `is_x86_feature_detected!` does not exist on aarch64.

Checks: the aarch64 build runs the Rust tests on its builder, including the
handheld argument test. `checks.x86_64-linux.korri-viewer-aarch64` starts the
aarch64 binary under `qemu-aarch64` with `LD_BIND_NOW=1` and `--help`. It
checks that the loader resolves `libavcodec` to the patched FFmpeg, that this
FFmpeg lists the `v4l2request` hwaccel, that korrid's handheld arguments
parse, and that `v4l2m2m` is refused. Moonlight had no C test for this path,
so none is carried. Not proven here: any decode, presentation, input or
sound on RK3566 or H700 hardware. Records are in `proof/v4l2request/`.

## Lifecycle socket

The Rust treaty is `contracts/viewer`; `contracts/generated/viewer.ts` is
Typeshare output. Its requests and replies extract `LinuxStreamViewer` and its
snapshot types. Native connection callbacks, as in Moonlight patch `0023`, emit
connection snapshots. The opaque connection identity uses that patch's PID and
monotonic timestamp. The socket carries no frame timing: the frame record
below is a file for the outside measure.

```sh
korri-viewer --lifecycle-socket PATH --launch-id ID --controller-uid UID stream [Moonlight arguments] HOST
```

The launcher must supply korrid's numeric UID with `--controller-uid`; there is
no default. The viewer keeps its own runtime UID, separate from korrid. `PATH`'s
immediate parent must not exist: the viewer creates it atomically with mode
0700, binds and restricts the socket inside it, then grants only the controller
search access to the directory and read/write access to the socket through Linux
POSIX ACLs. The owning group and all other users get no access. A filesystem
without ACL support fails closed. Ancestors must be traversable by both UIDs.

The launcher provides the directory above the new one, for example a systemd
`RuntimeDirectory=` that the viewer user owns, below the root-owned `/run`.
Before it creates anything, the viewer walks from `/` to that directory. It
opens each directory by its name in the directory before it, with
`O_DIRECTORY|O_NOFOLLOW`, and judges it with `fstat` on the open descriptor.
Root or the viewer UID must own each one, and the group and other users must
have no write access. A sticky directory such as `/tmp` is refused like any
other: another user can own a name in it. A symlink in the path is refused, so
give the real path. The controller can have search access to those directories,
but not write access. A user namespace that does not map root, such as the Nix
build sandbox, shows `/` with an unmapped owner, and the viewer refuses every
location there.

The viewer resolves no pathname after that walk. It creates its directory with
`mkdirat` in the last descriptor, opens it there without following a symlink,
and keeps both descriptors. If the opened directory is not owned by the viewer
UID, the viewer removes it from the parent descriptor and exits. It binds the
socket and sets the socket's ACL through `/proc/self/fd/<descriptor>/<name>`;
mode, the directory's ACL and
removal of the socket and the directory use the descriptors. After `bind()` the
directory must hold a socket that the viewer UID owns. If not, or if `/proc` is
missing, or if that address is longer than `sun_path`, the viewer removes what
it made and exits. A replaced name anywhere in the path cannot move the socket
or the cleanup. The viewer also exits if its directory is no longer in its
parent after `bind()`. Only root or the viewer UID can cause that, so it is not
a defence; it reports at once a socket that no controller can reach.

The process waits for `Start` before connecting. Start's address includes the optional HTTP
port, as in korrid's Moonlight adapter. Its optional display is the local logical
screen; encoded dimensions still come from rendered settings. No screen means
no shape observation. Linux keeps its local dimensions, not the holder's size.

Messages are newline-delimited JSON, at most 64 KiB each, with one request in
flight. `ViewerMessage` separates unsolicited events from replies. `Start`
confirms the native connection callback; `Stop` confirms network/picture
teardown, then the process exits. A lost controlling socket requests teardown.
Existing paths are not replaced. Shutdown closes an idle or partial reader and
removes only the socket and empty directory the viewer owns. Requests and
connection setup have 30-second
deadlines; a cancelled exchange cannot reuse a late reply.

The viewer blocks SIGINT, SIGTERM, SIGHUP and SIGQUIT before it creates its
directory and before it starts a thread, and one thread takes them. A signal
at any time, before or after `Start`, removes the socket and the directory at
once through the held descriptors, then requests teardown as `Stop` does. A
signal that arrives while the socket is made waits until it exists. A stop
before the stream runs, by a signal, `Stop` or a lost controller, emits
`Stopped` and exits with status 0. After `Start` the viewer checks for a stop
before each connection step and never sends the launch request after one. A
request in progress ends within 100 ms of a stop, while it connects or waits
for its answer. `LiStartConnection` is interrupted too, but the core ends it
only at its own points: before each RTSP exchange, between RTSP connect
retries and in its ENet waits. One TCP connect or RTSP exchange inside it can
still take its 10 s or 15 s timeout. A `Start` that arrives after a stop gets
the error `viewer is stopping`.

korrid's `SocketLinuxStreamViewer` takes injected unit-start, cleanup and
compositor-focus effects. It never reads unit state or compositor PIDs for
lifecycle observations. korrid confirms compositor focus before sending
`Background` or `ResumeAndFocus`; the socket reports that confirmed state, not
an independent X11 focus model. korrid retries a socket it cannot reach yet,
including one it may not yet search, for 30 s, then logs the last error.
After `Stop`, korrid also stops the unit and waits until systemd confirms it,
also when the socket was lost; that wait outlasts the unit's own stop timeout
and SIGKILL. Live resize,
apply and notice return typed unavailable errors, as the existing Linux trait
defaults do: korrid asks the holder for a new stream size itself (#476, Route
C), and the viewer follows the size that arrives (below, "A new stream size in
the middle of the stream"). On Linux, snapshots have no battery, health or controls until a
real producer supplies them; on Android the app supplies them (slice D11,
below).

## The `@korri:viewer` plugin and korrid (slice 3)

The plugin is built in the plugins repository from this directory
(`plugins/viewer/` there, ADR 0061). It declares the viewer, the controller
mapping, an identity-bootstrap helper, `treaty` (this package's
`share/korri-viewer/viewer.ts`) and `packages.source`, the GPL-3
corresponding source that `default.nix` builds as `correspondingSource`
(ADR 0059). Its settings are `@korri:moonlight`'s, under the runner
`@korri:viewer/viewer`.

korrid (`services/korrid/src/linux_viewer/selection.rs`) chooses at each start:

- `@korri:viewer` enabled and its `files.treaty` equal, byte for byte, to
  korrid's own `contracts/generated/viewer.ts`: the socket adapter.
- Otherwise Moonlight, exactly as before. A refused `@korri:viewer` is logged
  with korrid's treaty hash, and with the plugin's when it declares a treaty
  file. Without Moonlight the refusal is the start's error. A launch stays
  with the viewer that started it.
- After a korrid restart, a launch it no longer knows goes to the viewer whose
  unit holds it. The Korri viewer's unit carries the live-unit runner marker
  `KORRI_LIVE_RUNNER_ID=@korri:viewer/viewer`; a unit without it is
  Moonlight's. With no unit, the launch goes where a start would go.

The treaty check covers the wire types only. A change to the launch
arguments, or to behaviour that keeps the wire types, is not caught before
start: the viewer then fails at start. If that happens, the next step is to
put the launch arguments into the contract crate, so that the same check
covers them.

korrid starts the viewer in its exact-launch unit `korri-game-<launch>`, as it
starts Moonlight: same user, sandbox, environment, broker certificate
credential and key directory. The lifecycle viewer's unit adds:

```text
korri-viewer --lifecycle-socket /run/korri-game-<launch>/control/viewer.sock \
  --launch-id <launch> --controller-uid <korrid's UID> stream [Moonlight arguments] HOST
RuntimeDirectory=korri-game-<launch>   RuntimeDirectoryMode=0700
ExecStartPost=<KORRID_SETFACL> -m u:<korrid's UID>:x /run/korri-game-<launch>
Environment=KORRI_LIVE_RUNNER_ID=@korri:viewer/viewer
```

systemd applies `RuntimeDirectoryMode` again before each Exec command, and a
chmod resets an ACL mask. So the search grant comes after the main process
starts, and `systemd-run` returns only after `ExecStartPost`. A connect that
still comes before the grant gets `EACCES`, which korrid retries. systemd
removes the directory when the unit stops. Moonlight's units do not change.

`services/korrid/viewer-vm-test.nix`, run by the plugins repository as
`korri-viewer-lifecycle`, proves this on a cold NixOS guest with the real
plugin host, korrid module, units and sandbox. It does not exercise the
federated path: the peer request that makes korrid start a viewer in a real
stream. The probe (`services/korrid/src/bin/viewer_probe.rs`) calls the
daemon's own `installed_viewer` instead, as user `korrid` in korrid's service
sandbox, with a broker that serves a private Sunshine in the guest. The aka
binary proof and the owner's one game after the autonomous proof (ADR 0063)
cover that path first.

## Frame record (#435)

The outside measure (`nix/stream-cadence`) asks for it. A korrid launch does
not. The tool names two new files in its run directory with the environment
names of its stage-0 client shim (`client-shim.c`):

| Variable | File | What the viewer writes |
| --- | --- | --- |
| `CADENCE_FRAME_TIMES` | `times.csv` | One row for each decode unit |
| `CADENCE_DUMP` | `recording.video` | Each decode unit's bytes as they arrived, in the same order |

Rust creates each file with `O_EXCL` and mode 0600, before the launch; an
existing name, a link included, stops the stream before it starts.
`picture/frame_times.c` writes both. It closes them after the network core
has joined its threads. After a failed write it writes nothing more, and the
viewer exits with an error when the stream ends.

Each row is `index,frame_number,enqueue_ns,decoded_ns,display_ns`, in index
order. Every time is `CLOCK_MONOTONIC` in nanoseconds, the clock of the
measure's display record.

| Field | Meaning | Grounding |
| --- | --- | --- |
| `index` | The unit's place in the stream as it arrived and in `recording.video` | Every stage-0 record (`index`) |
| `frame_number` | The stream's frame index | `DECODE_UNIT.frameNumber`; the stage-0 receive record's `frame_number` |
| `enqueue_ns` | The network core had the whole frame: about when its last packet arrived | `DECODE_UNIT.enqueueTimeUs` (`Limelight.h`), carried to `CLOCK_MONOTONIC` through the short delay since then, as the stage-0 shim carries `receiveTimeUs` |
| `decoded_ns` | The decoder returned the frame; empty when it returned none | The viewer's own moment, after `avcodec_receive_frame`. With VA-API the GPU can still be decoding; presentation waits for it with `vaSyncSurface` |
| `display_ns` | The call that hands the frame to the display returned (`vaPutSurface`, `eglSwapBuffers` on Intel, `SDL_RenderPresent`); empty for a frame that a newer one replaced | The viewer's own moment. The stage-0 display record's `display_ns` is the compositor's commit, later |

The viewer does not read the frame ID that the sender drew into the picture:
with VA-API the picture stays in GPU memory, and a readback for each frame
is not cheap. The tool decodes `recording.video` instead, as `run` decodes its
own recording, and so knows the drawn ID of each unit. `decoded_ns` has no
earlier field name; it is the moment that #435 asks for.

The cost for each frame is one `write` of the unit's bytes and four clock
reads on the decode thread, and one clock read and one buffered CSV row on
the presenter's thread. The record writes no row until a frame is handed over or known
to be passed over, so the CSV stays in index order. A unit that no
presenter took for 256 units is written as not shown.

On Android the decoder is Rust (`src/android/picture.rs`), and
`src/frame_record.rs` writes the same rows, with the same file rules. The
ask comes in the starting intent, with the same two names
(`CADENCE_FRAME_TIMES`, `CADENCE_DUMP`) as extras that hold paths in the
app's files directory; the app's process has no environment that a caller
can set. There, `decoded_ns` is the moment `AMediaCodec_dequeueOutputBuffer`
returned the frame, and `display_ns` the moment
`AMediaCodec_releaseOutputBufferAtTime` returned: the decoder has queued the
frame to the video layer's buffer queue, and the compositor takes it from
there. When the decoder returns two frames before the viewer hands one over,
the older is dropped and has no `display_ns`. **The API 31–32 limit:** at
Android 12 the NDK gives no later moment. `AMediaCodec_setOnFrameRenderedCallback`,
which reports when the compositor showed a frame, starts at API 33, so a
frame that the compositor itself replaced in one refresh still has a
`display_ns`. The record does not use the callback at API 33 either yet
(slice D6).

Only the PC presenters and Android report so far. The handheld `v4l2request` path
(`picture/video/v4l2m2m.c`) has its own decode loop and calls none of the
record; with the variables set it writes a header and an empty recording,
and the tool then gives number 1 no value.

## The start's timeline (#391)

Every start writes one line for each step from Start to the first frame,
on both platforms, with or without a record (`src/marks.rs`):
`start: <step> at <time> ms`, `<time>` on CLOCK_MONOTONIC, the clock of
stream-cadence's start and display times and of the frame record. The
steps: Start (or the command's own start without a lifecycle); each HTTP
request with libcurl's connect, TLS, first byte and total times; the pair
check; the picture and input; every stage of the network core
(`stageStarting`, `stageComplete`); the RTSP answers that the C core names
in its log and its first video and audio packet (`marks::core_step`,
through `src/core_log.c`); on Android the decoder (`decoders listed`,
`decoder made`, `decoder taken`); the first frame that reached the
display. The C core logs the three SETUP answers on every host. It logs
nothing after OPTIONS or ANNOUNCE, and after DESCRIBE only on a host
without reference frame invalidation ("Reference frame invalidation is not
supported by this host", `RtspConnection.c`), such as Sunshine with H.264:
elsewhere SETUP audio's mark ends OPTIONS and DESCRIBE together.
nightjar-core logs none of these lines; with it, the stage marks bracket
the RTSP handshake. On Linux the first frame's line
comes from `picture/frame_times.c`, at the presenter's hand-off; on Android
from the decoder's output thread, at the first frame released to the
Surface. The Korri app adds its own steps before Start (korrid's start, the
certificates, the layers, the window's turn), and its play proof reads them
all into `result.json` (`start_timeline`).

## Sound and input provenance and checks

See `io/README.md` for every C adaptation and its source. The I/O check
encodes real Opus packets, decodes stereo/5.1/7.1 and verifies the PCM written
through ALSA's file plugin over null. It exercises the SDL event loop with a
virtual controller, X11 events on a real Xvfb window, and evdev's event and
mapping code with real libevdev capabilities. It checks forwarding at the
network-core boundary; it does not measure button-to-packet delay. In
view-only mode both PC platform branches forward a window key, and a strace
of them proves no `/dev/input` open is attempted.
The aka comparison and isolation records are in `proof/`.

## Sound in Rust (#391 step F, slice F1)

`src/audio_output/renderer.rs` is the Rust port of the carried C sound:
`io/audio/{alsa,pulse,sdl}.c` and the choice in `io/bridge.c`. The carried
C stays the default. The cargo feature `rust-audio-output` builds the port,
and `-audiooutput rust` chooses it (`-audiooutput c`, the default, keeps
the C). Without the feature, `-audiooutput rust` fails with the feature's
name. `korri-viewer-rust-audio-output` is the viewer with the feature; its
`bin/moonlight` appends `-audiooutput rust`, so that the stage-0 tool runs
it as the candidate beside `korri-viewer`, as `korri-viewer-nightjar` does.
Its turns need sound: `view --tone 1000` (`nix/stream-cadence/README.md`,
"The stream's sound").

Only the language changes, so that the measure judges one cause:

| | Carried C | Rust port |
| --- | --- | --- |
| Choice | `viewer_audio_callbacks`: SDL unless X11 VA-API; then Pulse when a 48 kHz stereo probe stream opens on the device, else ALSA | The same, with the same probe (`choose`) |
| Decode | libopus on the core's thread (`CAPABILITY_DIRECT_SUBMIT`); NULL for a lost packet | The same libopus calls, bound by `build.rs` from `opus_multistream.h` |
| ALSA | alsa-lib: the device or `sysdefault`, non-blocking, S16_LE, 20 ms period, 60 ms buffer, start and wake at the granted period, one `snd_pcm_recover` and one retry of a failed write, drain then close | The same calls, through the `alsa` crate 0.12 (Apache-2.0 or MIT) |
| PulseAudio | libpulse-simple: names "Moonlight Embedded" and "Streaming", the default buffer, ALSA's channel map, blocking writes, free without a drain | The same calls, through `libpulse-simple-binding` 2.29 (MIT or Apache-2.0) |
| SDL | SDL2's default device, 4096 frames, `SDL_QueueAudio`, the subsystem left open | The same calls, bound by `build.rs` from `SDL.h` (no `sdl2-sys`: it carries 89 MB of SDL source) |
| Channel order | ALSA's (FL FR RL RR C LFE SL SR) for ALSA and Pulse, the core's for SDL | The same |

What differs: the port checks the core's Opus configuration (at most 8
channels; a frame of at most 120 ms), and a decoder that libopus refuses
fails the start; the C went on with none. Its log lines start with
`sound:`, go to standard error, and name the output that it chose and
opened (`sound: the Rust audio output chose ALSA`, then the device and
its period). A callback that panics gives the core its fallback
(`callback::guarded`).

Tests (`renderer_tests.rs`, real libopus packets with a tone of its own on
each channel):

- ALSA: the port and the carried C play the same packets, with a lost one,
  into alsa-lib's own `file` PCM; the two files are equal, byte for byte,
  for stereo, 5.1 and 7.1, and each position holds the tone of the
  channel that ALSA's order puts there.
- PulseAudio: a private `pulseaudio` with one null sink in ALSA's 5.1
  order (as stream-cadence's tone server), `PULSE_SERVER` in a child
  process; the probe chooses Pulse, and the sink's monitor holds each
  channel's tone in its place.
- SDL: SDL's disk driver in a child process; the file holds the tone.
- A device that does not open fails the start, as in C (-1); a
  configuration of 9 channels is refused.

The product's sound server is PipeWire's Pulse server
(`services/inputd/nix/korri-linux-host-core.nix`); the test runs
PulseAudio's own. Neither the C nor the port sets a buffer size for
Pulse, so the server's default decides the delay; not measured here.

Not proven: a stream through the port (no stream ran in this slice), a
real sound card, and anything about delay or gaps: no number of ADR 0062
measures sound. The turns judge its CPU, start, picture and stability with
the tone playing.

## EGL presenter in Rust (#391 step F, slice F3)

`src/egl_presenter/vaapi.rs` is the Rust port of the carried C EGL
presenter, `picture/video/vaapi_egl.c`: it shows a VA-API surface that
`vaPutSurface` cannot show (Intel under Xwayland, #477) or that has bars
(#476). It uses `khronos-egl` 6.0.0 (MIT or Apache-2.0, linked to the
same libEGL) and `glow` 0.18.0 (MIT, Apache-2.0 or Zlib); `build.rs` binds
the picture's types and the EGL and OpenGL ES extension constants from the
headers that the C includes. The window stays Xlib (`video/x11.c`), the
place comes from `place.c` (#476), and the frame record's call stays in
the caller (`video/ffmpeg_vaapi.c`). The carried C stays the default. The
cargo feature `rust-egl-presenter` builds the port, and `-eglpresenter
rust` chooses it; `korri-viewer-rust-egl-presenter`'s `bin/moonlight`
appends that option.

With the feature, Rust owns the two names that the caller uses
(`vaapi_egl_present`, `vaapi_egl_cleanup`) and chooses at run time;
`build.rs` compiles `vaapi_egl.c` with `-D` renames to `carried_*`. Without
the feature, the build is as before. The caller does not change.

What stays as in C: the config, context, extension checks, shaders,
texture, uniforms, viewport, clear and draw calls, in the same order; the
import of a composed dmabuf export with its colour hints and modifiers; a
swap interval of 0; the same log lines (`VAAPI EGL presentation: ...`).
What differs:

- The corners are in a buffer object: glow takes no client-side array.
  The pixels are the same (below).
- A failed setup releases its EGL display at once; the C keeps it until
  its cleanup. Both then show no frame and say why once.
- Each log line's `(EGL 0x...)` is the error of the call that failed.
  khronos-egl reads `eglGetError` itself, which clears it, so the port
  keeps the code that the call returned. For a swap that fails, the C
  reads `eglGetError` after `eglDestroyImageKHR` and prints `0x3000`; the
  port prints the swap's own code.
- The port logs one line when it starts (`picture: the Rust EGL presenter
  shows VA-API surfaces`), so a run shows which code drew.

Tests (`src/egl_presenter/tests.rs`), on Xvfb with Mesa's software EGL
(llvmpipe), as the picture check's X11 tests run:

- No VA driver runs in the sandbox, but Mesa imports a memfd as a dmabuf.
  Each case is an NV12 surface written by hand, with a coded size larger
  than its visible one and white padding: the window's shape, BT.601
  narrow range; a portrait picture with bars at the sides, BT.709; a
  smaller picture, full range, with a linear modifier. Through
  `vaapi_egl_present` and `vaapi_egl_cleanup`, both show each case, the
  window's pixels, read from the X server, are equal, and the C's are
  black outside the place, bright in the left half and dark in the right.
- An export of two layers, or one that names a missing object: neither
  shows it.
- A setup on a window that is gone fails with `EGL_BAD_NATIVE_WINDOW`
  (`0x300b`), the surface's own error, not `0x3000`.
- The import's attributes, plane by plane, with and without modifiers;
  the colour hints from the frame's matrix and range.

A changed `visible` uniform or a missing clear makes the pixel test fail
(checked by hand on aka, 2026-11-17).

**Where it runs.** `vaapi_egl.c` runs on Intel under Xwayland for every
frame, and on AMD only for a picture with bars, or for every picture with
`-filledpicture egl` (slice F5, below). The private pair's viewer
output has the stream's size, so a turn on aka uses `vaPutSurface` on
both sides unless the output's shape differs: `view --viewer-output
1920x1200` (`nix/stream-cadence/README.md`, "Bars around the picture").

**The X11 software picture is gone** (2026-11-17, owner decision).
`picture/video/egl.c` drew a software-decoded frame in the X11 window. It
was reached only through `decoder_callbacks_x11`, and through
`decoder_callbacks_x11_vdpau`, whose decoder is software here too;
`picture_callbacks` returned neither, and `x11_vaapi` always decodes
through VA-API. Both tables, `x11_setup_vdpau`, `egl.c` with its header,
its test (`tests/test-egl.c`) and its Rust port are removed. The software
fallback is SDL (`sdl.c`, `video/sdl.c`), as before. GPL-3 §5a:
`video/x11.c` and `video/video.h` modified by Korri on 2026-11-17 by these
removals.

Not proven: a real VA-API surface through the port (the sandbox has no
GPU; Mesa's software import is not radeonsi's or iHD's), Intel, and any
number.

## X11 window in Rust (#391 step F, slice F4)

`src/x11_window/window.rs` is the Rust port of the X11 VA-API platform's
window and its keyboard and mouse: `picture/video/x11_window.c` (the window
part of Moonlight's `x11_setup`) and `io/input/x11.c`, with
`io/input/keyboard.h`'s key table in `keyboard.rs`. It uses `x11rb` 0.14.0
(MIT or Apache-2.0), the X11 protocol in Rust. The carried C stays the
default. The cargo feature `rust-x11-window` builds the port, and
`-x11window rust` chooses it; `korri-viewer-rust-x11-window`'s
`bin/moonlight` appends that option. On the SDL platform and on
`v4l2request` the option is refused: neither has this window.

The window's lines of `x11_setup` are moved, unchanged, into their own
file, `picture/video/x11_window.c`, as `x11_window_open` (GPL-3 §5a:
`video/x11.c` modified by Korri on 2026-11-17 by this move). With the
feature, Rust owns that name and chooses at run time; `build.rs` compiles
the C file with a `-D` rename to `carried_x11_window_open`. `video/x11.c`
keeps its Xlib display for VA-API and for the presenter, and asks
`x11_window_open` for the window. The Rust window connects to the same
display (Xlib's `XDisplayString`) on a connection of its own, and adds its
handler to the same descriptor loop (`loop.h`), which the stream's thread
polls as before. `picture/bridge.c` gains `picture_poll_descriptors`, the
loop alone, which `picture_poll` now calls and the tests use.

What stays as in C: the window's size, event mask, `WM_NAME` "Moonlight",
`WM_PROTOCOLS` with `WM_DELETE_WINDOW` and blank cursor; the fullscreen
request to the root (`_NET_WM_STATE`); every key through the same table,
with shift, alt and control tracked by key code; Ctrl+Alt+Shift+Q to quit
and Ctrl+Alt+Shift with another key's release to toggle the grab; the
buttons, scroll and horizontal scroll; the relative mouse, warped back to
(640, 360) while grabbed; `WM_DELETE_WINDOW` to quit. Two quirks of the C
stay too, so that the measure judges the language alone: buttons 4 to 7
scroll on both their press and their release, and the handler's `button`
is set once for each read, not for each event, so a scroll (or a button
past 9) that follows another button in the same read sends that button's
press or release again.

What differs:

- The window has its own X11 connection. Its requests go out at the end
  of the open; the C's last ones (`WM_PROTOCOLS`, the cursor) wait for the
  display's next flush.
- An X11 error is logged; Xlib's default handler ends the process. A lost
  connection ends the stream through its teardown (the loop's -1); Xlib
  ends the process.
- A window that cannot open, or an open that panics, ends the stream
  through its teardown (`PlayerIo::poll` returns -1 before any frame); the
  C has no such case.
- A call of the handler while no window is open fails the loop (-1): only
  a descriptor left in the loop after its window closed can cause it.
- The window goes when the stream's I/O drops, a moment after the core
  stops; the C's goes with the display in the decoder's cleanup.
- The port logs one line when it opens (`picture: the Rust X11 window
  ...`).

Tests (`src/x11_window/tests.rs`), each window on a private Xvfb with a
1920x1080 screen, so that the warp lands inside a 1280x720 window:

- The window: another client sees the same size, map state, event masks,
  `WM_NAME` and `WM_PROTOCOLS` from both.
- Fullscreen: a client that acts as the window manager receives the same
  create, map request and `_NET_WM_STATE` message from both.
- Input: another client sends a script of events with `XSendEvent` (keys
  with and without shift, buttons 1 to 10, keys outside the table, moves
  grabbed and ungrabbed, the grab toggled and back, `WM_DELETE_WINDOW`,
  the quit chord). Each group, then the descriptor loop until it is
  quiet: the same loop results, the same packets at the network core and
  the same real pointer position from both. With the button reset for each
  event, or with no warp, the test fails (checked by hand on aka).
- The key table equals `io/input/keyboard.h`'s, read from the file.
- A closed window is gone, and its descriptor has left the loop: a turn
  of the loop then calls no handler. The handler fails the loop (-1) when
  no window is open, so a descriptor that the close left behind (closed,
  so poll marks it invalid) fails the test; with the close's
  `loop_remove_fd` removed, it does (checked by hand on aka).
- A window that cannot open, or an open that panics, makes `failed()` true
  until the next stream.

Not proven: the window under a real compositor (sway, Xwayland) and its
focus by korrid, a key from a real keyboard through XTEST, and any number.
Number 5 of ADR 0062 (a key on the viewer's window to its packet) and
number 4 (startup) are the ones this slice can move.

## Every VA-API picture through EGL (#391 step F, slice F5)

On X11 VA-API, `video/ffmpeg_vaapi.c` shows a picture that fills its window
with `vaPutSurface`, and one with bars through the EGL presenter (#476).
With `-filledpicture egl`, the EGL presenter shows the pictures that fill
the window too; `vaPutSurface` then shows none on AMD. The presenter is
the one that `-eglpresenter` chooses. `-filledpicture putsurface`, the
default, keeps `vaPutSurface`. On the SDL platform and on `v4l2request`
`egl` is refused: neither shows pictures through this code.
`korri-viewer-rust-egl-filled-picture`'s `bin/moonlight` appends
`-eglpresenter rust -filledpicture egl` to the binary of
`korri-viewer-rust-egl-presenter`, whose `bin/moonlight` appends
`-eglpresenter rust` alone. Its turns beside that package judge this
choice alone.

This is a change of policy, not a port: no code moves into Rust. It
changes how each frame reaches the screen on AMD, so it can move numbers 1
to 3 of ADR 0062 (packet to frame on the display; frames late, twice or
dropped; CPU on the viewing device), not only startup or input. `ffmpeg_vaapi.c` (GPL-3 §5a: modified by Korri on
2026-11-17) gains `picture_vaapi_egl_for_filled`, which the viewer calls
before each stream, and names its rule `vaapi_presents_through_egl`. The
rest stays: a refusal by `vaPutSurface` still sends every frame of the
session to EGL (#477), and an EGL failure still sends every later frame of
the session to `vaPutSurface`, with one log line (`EGL cannot present here
(...); vaPutSurface shows every picture`).

Tests:

- The rule (`tests/test-vaapi.c`, in `korri-viewer-picture`): by default
  a picture that fills its window goes to `vaPutSurface` and one with
  bars to EGL; after `picture_vaapi_egl_for_filled(1)` both go to EGL, and
  the session's cleanup leaves that choice.
- The pixels (`src/egl_presenter/tests.rs`, with both presenters, as in
  slice F3): five pictures in one 400x225 window, each different from the
  one before it everywhere. Four fill the window, one with bars of 2
  pixels that `place.c` counts as none; one in the middle is portrait,
  with bars. Each picture that fills the window is at the place that
  `vaPutSurface` is given, the whole window, and shows whole: no black
  pixel and its own pattern, so no part of an earlier picture stays. The
  portrait one is black outside its place. The Rust presenter's pixels
  equal the C's.
- The option: `putsurface` by default, `egl` with either presenter.

Not proven: a real VA-API surface (the sandbox has no GPU), radeonsi's
pacing of an EGL swap against `vaPutSurface`'s, tearing (the presenter's
swap interval is 0, as in slice F3), and any number.

## Android session part (#391 step D, slice D0)

The session part also builds for Android at API 31 (ADR 0064), with the NDK
of `clients/android/sdk.nix`: `aarch64-linux-android` for phones and
`x86_64-linux-android` for the emulator. `android.nix` holds the recipe;
`checks.x86_64-linux.korri-viewer-android` builds both. Nothing in
`clients/android` uses it.

- Shared: `options.rs`, `session.rs`, `pair.rs`, `stream.rs`, `lifecycle.rs`
  and the binding are the same files on both platforms.
- Platform parts: `player_io.rs` is Linux's picture, sound and input.
  `src/android/player_io.rs` has the same interface; since slice D2 it has
  the picture, since slice D3 the sound, since slice D4 the launch's
  controller mask and since slice D4b the controllers' feedback (below);
  the input itself runs on the app's thread.
  Without the app's picture, as in the session tests, the core runs its own
  empty decoder and audio callbacks. The core's log goes to logcat, tag
  `korri-viewer`, through `src/core_log.c`: the callback is C-variadic,
  which Rust cannot define. The signal handlers in `stream.rs` are Linux
  only: on Android the app owns its process's signals. The `v4l2request`
  cfg stays off.
- Network core: the same unmodified `b126e481` and ENet as on Linux, built
  with their own CMake recipe and the NDK's toolchain file, as static
  archives. `build.rs` binds `Limelight.h` alone and links the archives.
- TLS and HTTP: the code does not change. Android gets the same two C
  libraries as Linux, built for it: nixpkgs' OpenSSL release (3.6.4 on this
  date, without nixpkgs' certificate-path patches) as static archives, and
  curl 8.22.0 from the `curl-sys` crate (`static-curl`, Android only), the
  release that nixpkgs gives Linux. The core needs libcrypto on Android in
  any case; the core and the session's crates link one OpenSSL. Linux still
  links nixpkgs' shared OpenSSL and curl, so its HTTPS does not change.
- Trust anchor: the curl crate gives its own libcurl the CA file and the CA
  directory that it probes for when it makes a handle (`SSL_CERT_FILE`,
  `SSL_CERT_DIR` or a system path). `Transport::new` resets the directory
  on Android, and the pin replaces the file, so the pinned certificate is
  the only trust anchor, as on Linux. `tests/trust_anchor.rs` proves it: a
  server with the pinned key in a certificate from a CA in the probed
  directory is refused. On the API 31 emulator that test failed before the
  reset (the server was accepted) and passes with it.
- Cost: the app carries OpenSSL and curl as C code that Korri builds, which
  is against ADR 0062's stretch goal. Android's curl has fewer features than
  Linux's (no HTTP/2, so no `h2` offer in the TLS handshake; no PSL, brotli
  or zstd); the session uses none of them. A pure-Rust stack (rustls) would
  remove curl only: the core and the pairing code still need libcrypto, and
  Linux's certificate check, session cache, connection reuse and interrupts
  would need a new proof.
- The check builds the library and links the HTTPS and trust-anchor test
  executables for each ABI. The HTTPS test drives `Transport` and the
  launch chain into the core. It then reads the HTTPS executable: its
  machine, `/system/bin/linker64`, no library beyond `libc.so`, `libm.so`,
  `libdl.so`, `libz.so` and `liblog.so`, and the symbols of the core, curl,
  OpenSSL and the log callback inside. It does not run them; the app's
  proof below runs the x86_64 pair on the emulator.

Live resolution: Android also uses the unpatched core, so the Rust viewer
answers `Resize` with "Live resolution is unavailable", as on Linux. Today's
Android app changes the stream's resolution live (korrid's Portrait control,
fold and rotation) through `SunshineRuntimeSettings.c`. Korri wrote that file
in its Artemis copy of the core (`1fd8f951a`); Sunshine patch 0044 in
`korri-plugins` serves it. **Gate:** slice D7, which removes `Game.java`, does
not ship until live resolution works in the Rust viewer. Dropping a live
feature is not "equal or better" (ADR 0062). #476 chose the route, a control
channel that korrid owns (Route C), in five slices. Slice 4, the Android
decoder and video layer following a size change that the sender makes in
the middle of a stream, is below ("A new picture size in the middle of the
stream"). Slices 2 and 3, the request, are in korrid and Sunshine patch 0046
(`clients/android-app/README.md`, "korrid's resize request"). korrid makes
the request only for a viewer that reports `ready`. This viewer reports
`ready: true` since it follows a new size (below), and `Resize` stays
unavailable: korrid asks the holder, never the viewer.

## Android app (#391 step D, slice D1)

Slice D1 of `docs/research/2026-11-13-viewer-rust-android.md`: the viewer in
the app's own process (ADR 0060), started and stopped through its
lifecycle, against the private Sunshine, with the core's empty decoder and
no sound. It is the first Rust viewer that runs on Android. It has no
picture, sound or input: those are D2 to D4. The Artemis app in
`clients/android` does not change; it stays the shipping Android viewer
until D7, so D1 removes none of its Java.

- `android/` is the app: a `cdylib` crate with its own `Cargo.lock` (the
  viewer's crates at the same versions, `android-activity` 0.6.1 and `jni`),
  and `AndroidManifest.xml`. Android's own `android.app.NativeActivity`
  loads the library; the APK has no `classes.dex` (`hasCode="false"`) and no
  resources of its own. The activity connects where its intent says, so a
  caller needs the `DUMP` permission to start it: the shell user and the
  system, a privileged app that holds `DUMP`, and any app that asks for
  `DUMP` and is granted it through adb (`pm grant`; `DUMP` is a development
  permission). Other apps are refused; the proof checks both. The app
  allows no backup (its files hold a client key pair), and a finished
  stream leaves no entry in Recents to start it again. Package
  `com.simonwjackson.korri.viewer`, minimum API 31, target 34 as the Artemis
  app.
- In-process lifecycle: `Lifecycle::in_process` is the same runtime and the
  same replies as the socket, with no socket, UID check or signals. Its
  controller calls `Lifecycle::request` with the treaty's `ViewerRequest`,
  and each `ViewerEvent` goes to a function. `launch::run` holds the steps
  that the Linux command ran in `main.rs`; both platforms now call it.
- The app is that controller. It reads the launch from its starting intent,
  sends Start once its window exists (slice D2), and sends Stop when the
  window goes, when the activity is destroyed, or on Ctrl+Alt+Shift+Q
  (slice D4). Until slice D5, Back sent Stop too; since D5 it opens the
  gameplay overlay, as in Artemis's Korri mode.
  When the runtime ends, the activity finishes. Every request, reply and
  event goes to logcat as the treaty's JSON, with the core's own lines. The
  intent's extras are D1's entry until korrid drives the viewer in this
  process; each takes the name of its source: `launch_id`
  (`ViewerRequest.launch_id`), `address` (`Start.address`), `args` (the
  command's `stream [Moonlight arguments] HOST`, a string array) and
  `KORRI_MOONLIGHT_SERVER_CERTIFICATE` (the pin's path), and, optional,
  `CADENCE_FRAME_TIMES` and `CADENCE_DUMP` (the frame record's ask).
  `Resize` stays "Live resolution is unavailable" (#476).
- Build: `android.nix` builds the library for both ABIs with cargo, links
  the manifest with `aapt2` against the android-36 framework of `sdk.nix`,
  adds the libraries and aligns with `zipalign`. No Gradle. The output is
  unsigned; the proof signs it with the user's debug key
  (`~/.android/debug.keystore`, made as Android Studio makes it when
  missing).
- Proof (`nix run .#viewer-android-proof`, `android/proof/run.py`): one
  headless Android 12 emulator (google_apis x86_64, 2 cores, `nice` 10,
  `android/emulator.nix`) on the build host, and stream-cadence's sender
  fixture on aka. The tool builds everything in the Nix store that
  `--build-store` names (`auto` on aka itself, stream-cadence's rule;
  without it, it builds nothing), pairs a
  fresh key pair with the Linux viewer's `pair`, runs the x86_64 HTTPS and
  trust-anchor tests in the emulator, installs the APK and gives the app the
  key pair and pin through `run-as`. It starts the app, waits for the
  Running event and the Start reply, streams, presses Home (Back until
  slice D5) and waits for the Stopped event, the Stop reply and the
  activity's end. It reads Sunshine's
  log for the session, closes the fixture and stops the emulator. The
  emulator reaches aka's Tailscale address through its own NAT; aka admits
  any port on `tailscale0`. Record: `proof/android-d1/`.
- Not proven: the arm64 library on a phone (no device in this slice), and
  anything about speed. The emulator's numbers mean nothing (ADR 0063).
- Cost: GPL corresponding source and the notices of OpenSSL, curl and ENet
  are not yet in the APK. The APK is a debug build for the proof, not one to
  ship.

## Android picture (#391 step D, slice D2)

Slice D2 of the research brief, corrected by the D5 spike
(`docs/research/2026-11-14-android-overlay-spike.md`): Android's hardware
H.264 decoder renders straight into a video layer that the app makes. No
frame passes through Korri's code: the decoder writes into the layer's
buffer queue, and the compositor reads the same buffer.

- The video layer (`android/src/layer.rs`): the spike's design C. JNI calls
  into Android's own classes make it: `View.getRootSurfaceControl()` (API
  31), `SurfaceControl.Builder`, `AttachedSurfaceControl.buildReparentTransaction`
  (API 31), `new Surface(SurfaceControl)` and `ANativeWindow_fromSurface`.
  The NDK route of the brief fails: `ASurfaceControl_createFromWindow`
  returns NULL for the NativeActivity window on API 31 and 34. One
  transaction puts the layer above the window's own buffer and places it
  with one `setGeometry` call: the stream's whole picture into the largest
  rectangle of its shape in the window, centred (`src/android/placement.rs`).
  When the window's size changes, the app places the layer again with one
  more `setGeometry`, posted to the main thread: the turn to landscape can
  come after Android gives the window (one emulator run saw a 1x1 window
  then), and a fold changes the size too.
  The app fills its own window black once. Under the video, a black backdrop
  layer covers the window (#495): opaque, one CPU-filled buffer, stretched
  over the window by the same transaction that places the video, and shown
  from the first decoded picture on, as Artemis's Korri mode turns its
  window black at the first frame. It is what shows in the bars in the Korri
  app, whose window the shell draws its library into. The layer
  is made on Android's main thread, with the app's own global reference to
  the activity, after the window exists and outside android-activity's
  event callback. While the app waits for it, it keeps handling
  android-activity's events: if the window or the activity goes first, it
  starts nothing, and the main thread, which waits for the app in
  `onDestroy`, is not held. A layer made after the app stopped waiting is
  removed at once. The app releases the layer only after the viewer's
  runtime has ended, so the layer outlives the decoder, and it does so with
  a post to the main thread that does not wait: one transaction reparents
  the layer to nothing, as the spike did, then its Surface and its
  SurfaceControl are released.
- No `AImageReader`: the decoder renders into the layer's own Surface, so
  the spike's retry after release (its silent stall) does not apply.
- The decoder: the core's decoder callbacks, in Rust
  (`src/android/picture.rs`), over `AMediaCodec` through ndk-sys, and the
  decoder's two loops (`src/android/decoder.rs`), which the tests run over
  a scripted codec. No panic leaves a callback for the core: each body runs
  under `catch_unwind` (`src/callback.rs`), and gives the core its fallback
  (`DR_NEED_IDR`, or -1 from setup); Rust would abort the app instead. Android's decoder list comes from `android.media.MediaCodecList`
  through JNI, as ADR 0064 says (no NDK list before API 36), and
  `src/android/codec.rs` chooses as Artemis's `MediaCodecHelper` does
  (brief, finding 20): a decoder with `FEATURE_LowLatency` and High profile
  first, then High profile, then the first; no alias; no software decoder
  except on the emulator. It is configured with Artemis's low-latency keys
  in tries, riskiest first (brief, finding 21): `low-latency`,
  `vdec-lowlatency`, `operating-rate` (Qualcomm and the decoders with
  reference frame invalidation) or `priority`, and on Qualcomm
  `vendor.qti-ext-dec-picture-order.enable`,
  `vendor.qti-ext-dec-low-latency.enable` and the three output-fence keys.
  A decoder with `FEATURE_LowLatency` gets `low-latency` alone in try 0. The
  colour keys are the core's defaults: limited range, BT.601, SDR.
- The decoder is made early (#391): from `picture::open`, on a thread of
  its own, for the picture that the stream asked for (H.264 at the asked
  size and rate, `decoder::Asked`), while the stream launches and its RTSP
  handshake runs. The core's setup takes it when it asks for that picture;
  for another (a height that the core rounded), the early decoder goes and
  setup makes one, as before. `picture::close` waits for an early decoder
  that no setup took and lets it go before the Surface. In the #495 proof
  the decoder's making took 0.28 s inside the core's video start.
- The SPS rewrite (`src/android/sps.rs`): Artemis's patch for a stream
  without reference frame invalidation, so that the decoder buffers no
  frame: `level_idc` 31, 32 or 42 for the stream's size and rate,
  `max_num_ref_frames` 1, a VUI bitstream restriction with
  `max_dec_frame_buffering` 1, and Constrained High only on `omx.intel`. It
  rewrites each syntax element and gives a patch that changes nothing back
  byte for byte. libavcodec's own parser (`trace_headers`) read the patched
  SPS of five libx264 streams with the new values, and FFmpeg decoded them.
- Input: on each IDR frame the parameter sets go first in their own
  `CODEC_CONFIG` buffer, then the frame. With no input buffer free, the
  decoder thread waits in 10 ms steps until the stream stops, as Artemis
  does; a frame given up would cost an IDR frame. A dequeued buffer that
  takes no frame goes back to the codec empty: when
  `AMediaCodec_getInputBuffer` gives NULL, as it does after a decoder
  error, and when a frame is too large for it.
- Decoder errors (#490): a recovery in steps. Its three steps and their
  fall-through are Artemis's (`doCodecRecoveryIfRequired`): a flush, a
  restart (stop, configure with the format the codec had, start) and a
  reset (release the codec, then make, configure and start a new one of
  the same name for the same Surface); a step whose own call fails leads
  to the next at once. Which step a failure takes, and when the stream
  ends, are Korri's (`decoder::Recovery`): Artemis picks a restart or a
  reset from the exception (`isRecoverable`), which the NDK's calls do not
  give, and it never flushes after a failure. Here a failure takes the step
  after the one that worked last, starting with a flush; a reclaimed or
  deleted codec goes straight to a reset. The frame that met the failure
  is lost, so the core asks the host for an IDR frame after each step. A
  decoded frame ends the run of failures, and the next failure starts with
  a flush again. Only the core's decoder thread changes the codec: the
  output thread that finds a failure asks it to, and leaves the codec alone
  until it has; a recovery waits while the output thread is inside a codec
  call, and the output thread does not take the codec again while the
  core's thread waits for it. A failure while the stream stops runs no
  recovery, as Artemis ignores errors then. Each failure backs off, 10 ms
  doubling to 160 ms, so that a failed decoder costs no CPU. The stream
  ends when failures still come 3 s after the first of a run, with no
  decoded frame between, and a reset has run; or at the first failure after
  10 runs of failures in one stream, so that a decoder that fails every few
  frames cannot flush and ask for IDR frames for ever. The 3 s and the 10
  are Artemis's numbers (`EXCEPTION_REPORT_DELAY_MS`, `CR_MAX_TRIES`), not
  its rule: Artemis stops at once when its new codec fails, and applies its
  3 s only after its 10 tries, which count each restart and reset, with no
  new start on a decoded frame. Once the decoder has failed, frames ask for
  no IDR frame; the player reports the failure (`picture: the decoder
  failed; the stream ends: …`) and the stream stops with `picture or input
  event loop failed`. The frames that the codec held when it failed get
  rows with no times in the frame record. Each step and its result go to
  the log (`picture: … recovery from a flush`, `picture: reset done; …`,
  `picture: the decoder returns frames again after a reset`). Proof
  (`nix run .#viewer-android-recovery-proof -- --build-store auto --stream-host ADDRESS`,
  `android/proof/recovery.py`, record `proof/android-recovery/`): the
  emulator and the sender fixture both on one host; the tool kills the
  emulator's Codec2 service twice while the app streams, and the frame IDs
  that screenshots read go on rising after each kill. Two of three runs
  passed; in the other, the fixture's Sunshine dropped the client after the
  decoder had recovered, for a reason that its log does not give.
- Pacing: upstream Moonlight's lowest-latency mode. An output thread hands
  each frame to the display at once
  (`AMediaCodec_releaseOutputBufferAtTime` with the time now); when the
  decoder has returned more than one, only the newest goes.
- Licence: `codec.rs`, `sps.rs`, `decoder.rs` and `picture.rs` port
  Artemis code (each names its source). Artemis ships under the GNU GPL
  version 3 with no "or later" grant, so these files are `GPL-3.0-only`
  and keep that attribution, and the Android library is under
  GPL-3.0-only. The Linux binary does not build them.
- The picture path in the log, as the Linux viewer reports its own:
  `picture: decode <decode> decoder <name> … try <n> keys <keys>`, with the
  list it chose from (`picture: decoders for video/avc: …`) and the
  placement (`picture: video layer … placed at …`). `<decode>` has the
  meaning of the measure's `viewer_ran.decode`: `mediacodec` for a hardware
  decoder, `software` for a software one, which only the emulator may use.
  The proof keeps it as `viewer_ran.decode` in its result. The lifecycle
  treaty does not carry it; the Linux viewer's does not either.
- The frame record: see "Frame record" above, and its API 31–32 limit.
- Lifecycle: the app sends Start once the layer exists. The loss of the
  window sends Stop, which Android takes when the app leaves the screen,
  as Artemis's Game activity ends the stream in `onStop`. Back sent Stop
  until slice D5; since D5 the proof presses Home. The layer
  follows a window resize (above) and a new decoded size (#476, below); a
  request to resize the stream stays unavailable.
- Not carried from Artemis: reference frame invalidation (Artemis turns it
  on for Qualcomm, NVIDIA and MediaTek), the vendor keys of NVIDIA,
  MediaTek, Kirin, Exynos and Amlogic (not in the brief's list), the
  Adreno 620 exception of `operating-rate` (it needs the GL renderer's
  name), the baseline-SPS and bitstream-fixup lists for old devices, fused
  IDR frames, direct submit, slices per frame, HEVC, AV1 and HDR (H.264
  only: the stream asks for H.264 unless `-codec` says otherwise, and the
  decoder refuses another format), the balanced pacing, the frame-rate hint
  and the display mode (slice D6).
- Proof (`nix run .#viewer-android-proof`): D1's run, plus the picture. The
  sender fixture's picture source draws stream-cadence's frame-ID bar.
  stream-cadence's capture reads a wlr-screencopy compositor, and the
  emulator's screen is not one, so the proof reads the bar from screenshots
  with stream-cadence's own decoder (`bar.py`): a burst after the start
  until the bar reads (the startup as the screen shows it), then four
  screenshots a second apart, whose IDs must rise. It reads the frame record
  from the app and decodes its recording with `decode.py`, so each row has
  the ID of its frame, and it takes the startup from the system's `START` of
  the activity to the first hand-off. It checks that SurfaceFlinger has the
  layer. A refusal probe (`android/proof/refusal`, a test app, never
  shipped) asks Android to start the viewer: refused without `DUMP`,
  allowed after `pm grant`. It also runs the picture's pure parts' tests
  on the emulator. Record: `proof/android-d2/`.
- Not proven: the arm64 library and a real phone's decoder, HWC composition
  and speed (step H, the Galaxy Z Fold 8). The emulator's decoder
  (`c2.goldfish.h264.decoder`) is backed by the host, and its numbers mean
  nothing (ADR 0063).

## Android sound (#391 step D, slice D3)

Slice D3 of the research brief: the stream's sound plays through AAudio, in
its low-latency mode, with Opus decoded by libopus as the Linux viewer
decodes it.

- The core's audio callbacks are Rust (`src/android/sound.rs`). The core
  decodes on its own audio thread, as it does for Artemis (no
  `CAPABILITY_DIRECT_SUBMIT`). Each packet goes to the player
  (`src/android/audio.rs`): libopus decodes it, a lost packet is concealed
  by Opus, and the PCM goes into a ring with one writer and one reader.
  AAudio's data callback, on AAudio's high-priority thread, empties the
  ring: it takes no lock and never waits, and plays silence for the frames
  that are not there. The tests run the player over a scripted output
  (`ScriptedOutput`), as D2's tests run the decoder over a scripted codec.
- The stream: PCM 16-bit, the core's rate (48 kHz) and channel count,
  shared, usage game, a data callback and an error callback. It asks for
  `AAUDIO_PERFORMANCE_MODE_LOW_LATENCY` first, then the standard mode if
  that open fails. AAudio grants low latency or falls back by itself; the
  log says what it asked and what it granted
  (`sound: output try 0 asked low-latency granted … burst … buffer …`).
  The buffer is two packets (Artemis's small buffer) or two bursts
  (AAudio's double buffering), the larger; AAudio rounds it to whole
  bursts.
- At most 40 ms of sound waits beyond AAudio's own buffer, as in Artemis:
  a decoded packet is dropped while the core's queue and the ring hold
  40 ms or more. Every packet is still decoded, because Opus carries state
  from one packet to the next.
- A lost stream recovers. AAudio calls the error callback, for example when
  the device goes; the callback only marks the stream lost. On its next
  packet the core's audio thread stops and closes that stream and opens a
  new one: AAudio says to do that from another thread than the error
  callback. If no new stream opens, the next try waits, from 10 ms doubling
  to 160 ms, and after 3 s of failures (the decoder's window, `Health` in
  `decoder.rs`) the sound ends and the stream goes on without it. No try
  ever spins: a test plays a packet every millisecond for 5 s against a
  device that stays lost and counts at most 60 opens. When Android's audio
  server dies, AAudio's standard path (the emulator's) reports no error:
  its AudioTrack makes a new track itself when the server is back, and the
  callbacks go on. The proof stops that server during the stream. A
  getter such as `AAudioStream_getXRunCount` can wait while that happens,
  so a teardown in that moment waits for the server too.
- No panic reaches C. Each of the core's five audio callbacks and AAudio's
  two runs its body under `catch_unwind` (`callback::guarded`). A panic in
  the data callback fills that buffer with silence, stops the stream and
  ends the sound; the picture goes on.
- Teardown: the core's audio cleanup, inside `LiStopConnection`, stops and
  closes the stream; `AAudioStream_close` returns after the callbacks.
  `PlayerIo::close` closes a stream that a cleanup left. The proof checks
  that `sound: stream closed` comes before the viewer reports Stopped.
- The log reports what the sound did when it ends: callbacks, frames
  played and silent, short callbacks after the first sound, packets,
  concealed and dropped packets, Opus errors, AAudio's xrun count and
  reopened streams (`sound: ended: …`).
- Licence: `audio.rs` and `sound.rs` port Artemis code
  (`AndroidAudioRenderer.java`, `callbacks.c`) and are GPL-3.0-only, as the
  D2 files are.

**AAudio, not Oboe.** The brief names both (findings 9 and 27). The choice
is AAudio, called through ndk-sys:

| Evidence | Source |
| --- | --- |
| Oboe's reason to be is the fallback: it "calls AAudio when it is available, and falls back to OpenSL ES if AAudio is not available". AAudio exists from API 26; the viewer needs API 31 (ADR 0064). | Android's AAudio guide; ADR 0064 |
| At API 31 no Oboe workaround applies to this stream (output, PCM 16-bit, data callback, no fixed callback size). Its callback-size workaround stops at API 30; its `ErrorTimeout` to `ErrorDisconnected` workaround is for API 30 only; its float, mono, input and Exynos or SM8150 MMAP rules are for other streams or older releases. Its one clamp that applies, a legacy buffer of at least one burst, is below the two bursts asked here. | Oboe `main`: `src/common/QuirksManager.cpp`, `src/aaudio/AudioStreamAAudio.cpp`, read 2026-11-14 |
| The lowest latency needs `AAUDIO_PERFORMANCE_MODE_LOW_LATENCY` with a high-priority data callback. AAudio gives both. | Android's AAudio guide |
| Oboe is C++ that Korri would build into the app, with a Rust wrapper (`oboe-rs`, last release 0.6.1 on 2024-03-03). ADR 0062 wants no C or C++ that Korri builds where a system library does. | ADR 0062; crates.io |

The cost: Oboe can resample inside the app to keep the low-latency path on
a device whose own rate is not 48 kHz; AAudio then takes its standard path
and resamples there, as Artemis does when it skips low latency. The ndk
crate's `audio` module is not used: its callback trampolines abort the
process on a panic (`abort_on_panic`), so the callbacks here are Korri's
own `extern "C"` functions over ndk-sys.

**What differs from Artemis** (`AndroidAudioRenderer.java`, `callbacks.c`):

| | Artemis | Korri D3 |
| --- | --- | --- |
| Output | `AudioTrack`, blocking `write` from the core's audio thread | AAudio, a data callback that pulls from the ring |
| Tries | Four: low latency or standard, each with a buffer of two packets, then the larger minimum buffer | Two: low latency, then standard. AAudio clamps a buffer size instead of refusing it, so the second dimension has nothing to try |
| Low latency skipped | When the device's own rate is not the stream's, or audio effects are on | Never asked away: AAudio grants or refuses it, and the log says which |
| Buffer | Two packets (10 ms at 5 ms packets) | Two packets or two bursts, the larger |
| 40 ms limit | Measured on the core's queue, because the blocking write fills it | The core's queue plus the ring, because the callback pulls |
| Underrun | `AudioTrack` plays silence | The callback plays silence for the missing frames and counts it |
| Device xruns | Not read | Read once at the end (`AAudioStream_getXRunCount`) and logged. The buffer is not tuned on xruns; AAudio's guide offers that, and step H can measure it |
| A lost device | `AudioTrack` follows the route; nothing to do | Close and open again; 3 s of failures end the sound |
| Channel layouts | 2, 4, 6 and 8 channels as stereo, quad, 5.1 and 7.1 masks | The channel count only: `setChannelMask` is API 32. Android's stereo, 5.1 and 7.1 masks have the core's order (front left, front right, centre, LFE, back left, back right, side left, side right), so neither reorders the PCM. That AAudio picks those masks for a count of 6 and 8 is read from AOSP (`audio_channel_out_mask_from_count`), not heard: the proof plays stereo. The core offers no 4-channel stream |
| Usage, content type | `USAGE_GAME`; content type unknown | `AAUDIO_USAGE_GAME`; AAudio's default content type, music |
| Sharing | `AudioTrack` is shared | Shared. Exclusive (MMAP) mode may lower latency on a phone; not asked until step H measures it |
| Audio effects session | Optional broadcast for equalizers | Not carried |
| Opus | libopus as prebuilt archives (`moonlight-mobile-deps`) | nixpkgs' libopus 1.6.1, the Linux viewer's release, built for Android with the NDK (`android.nix`) |
| No output at the start | The core's start fails (-2) | The same |

**Proof** (`nix run .#viewer-android-proof`, record `proof/android-d3/`):
D2's run, with sound. stream-cadence's sender fixture plays a 1000 Hz tone
(its `tone`, `nix/stream-cadence/README.md`). The emulator has its sound
card, and QEMU's `wav` driver writes everything that Android plays to a
file on the build host, so nothing reaches its speakers. The proof finds
the tone in that file, and the 440 Hz tone of the Android test that plays
real Opus packets through real AAudio (`sound::tests`). It reads the
sound's report from logcat, checks that the sound ended before the viewer
reported Stopped, and records Android's mixer while the app plays. While
the stream runs it stops Android's audio server; init starts it again, and
the app must play more sound than the stream had run before. The
screenshots show the picture still moving.

- Cost: libopus's notice is not yet in the APK, as those of OpenSSL, curl
  and ENet are not (slice D1).
- Not proven: the low-latency path. The emulator's mixer runs at 44.1 kHz
  with no fast mixer, so AAudio refused low latency for a 48 kHz stream
  and took its standard path, with 20 ms bursts. A phone's latency,
  underruns, MMAP and Bluetooth: step H, on the Galaxy Z Fold 8. Surround:
  the proof plays stereo; the tests decode 5.1 and 7.1 over the scripted
  output only. A lost device on a phone (an unplugged headset): the tests
  prove the reopen and the end over the scripted output; the emulator
  raised no AAudio error.

## Android input (#391 step D, slice D4)

Slice D4 of the research brief: game controllers, a keyboard, a mouse and
the touch screen reach the stream from the NativeActivity, mapped as
Artemis maps them. The mapping is Rust and pure, and its tests run on the
host and in the emulator over scripted events:

| File | Port of (Korri's copy of Artemis) |
| --- | --- |
| `src/android/input.rs` | `Game.java`'s routing: `handleKeyDown`, `handleKeyUp`, `handleMotionEvent`; the events, the packets and the core's sink |
| `src/android/keyboard.rs` | `KeyboardTranslator.translate`, `KeyMapper.getWindowsKeyCode`, `Game.java`'s modifiers and Ctrl+Alt+Shift keys |
| `src/android/gamepad.rs` | `ControllerHandler.java` and `ControllerHeartbeat.java` |
| `src/android/touch.rs` | `Game.java`'s touch and mouse branches, `AbsoluteTouchContext.java` |
| `src/android/controller_list.rs` | Valve's `controller_list.h` and SDL's `minisdl.c` from the same copy (zlib), as data |
| `src/android/devices.rs` | `android.view.InputDevice` through JNI |

These files port Artemis code and are GPL-3.0-only, as the D2 and D3 files
are; `controller_list.rs` keeps the zlib notice of Valve and SDL, and
`devices.rs` is Korri's own JNI code.

- Where it runs: android-activity gives the app's thread each key and
  motion event (`android/src/input.rs` turns them into plain values). That
  thread hands them to `Input`, which sends each packet with the core's own
  `LiSend*` call (`CoreSink`); the core queues it for its input thread. The
  core calls no new Rust function: D4 adds no `extern "C"` callback (D4b
  adds the first three, below). A panic
  in the input is caught on that thread (`Guarded` in `android/src/lib.rs`):
  the input ends, the stream goes on, and no panic reaches android-activity.
- The core's gate (`CORE_GATE` in `src/android/input.rs`): the stream can
  end while the app's thread sends (the host or the network ends it, and
  the heartbeat sends on). The core's own guard is a plain flag that a send
  reads once, and its stop then destroys the mutex and the queue that the
  send goes on to use (`InputStream.c`, `initialized`). So each send holds a
  read lock for its whole call into the core. `stream.rs` opens the gate
  after `LiStartConnection` succeeds; before `LiStopConnection` it closes
  it, which waits for the send in progress and lets none in after (logged
  as `input: the core takes no more input`). Artemis stops its controller
  handler before it stops the connection. When the app itself sends Stop,
  it also drops the input first. Its counts go to the log (`input: ended:
  … keys, … controller states, … arrivals, … mouse, … touches sent; …
  held; … unsupported, … outside the stream, … failed`); a send that the
  closed gate refused counts as outside the stream, and one that the
  overlay's hold kept counts as held (slice D5).
- Identity: the NDK gives an event's device ID and nothing more. Name,
  vendor, product, descriptor, `isExternal`, sources, keyboard type, the
  joystick axes' motion ranges (`SOURCE_JOYSTICK`, then `SOURCE_GAMEPAD`)
  and which controller keys it has (`hasKeys`) come from `InputDevice`
  through JNI, once for each device. The controller's kind for its arrival
  follows Artemis: Microsoft, Sony and Nintendo by vendor, others by SDL's
  list. Android tells no native code that a device went (that needs
  `InputDeviceListener`, a Java interface): the app asks for
  `InputDevice.getDeviceIds()` twice a second, and a controller that went
  frees its number and sends a state with no buttons and its bit out of the
  mask, as Artemis does on `onInputDeviceRemoved`.
- Numbers: Artemis's defaults. A built-in controller is number 0; an
  external one with joystick axes takes the lowest free number; a device
  without axes takes the number of the joystick beside it with the same
  descriptor (a DualShock 4's touchpad). The launch's `remoteControllersBitmap`
  and `gcmap` hold a bit for each joystick attached when the stream starts,
  as Artemis's `getAttachedControllerMask` does; each packet's active mask
  is those bits and the numbers in use.
- Mapping: Artemis's button table, its corrections by vendor, product and
  scan code (DualShock 4 face buttons and clickpad, Xbox One S over
  Bluetooth, 8BitDo, HORIPAD for Switch, Razer Serval, ROG Kunai, ADT-1,
  ASUS Gamepad, SHIELD, raw d-pad scan codes), its axis choice (triggers on
  LTRIGGER and RTRIGGER, else BRAKE and GAS, else BRAKE and THROTTLE, else
  Z and RZ beside RX and RY, idling at -1), a hat as the d-pad, analog
  triggers over digital ones, Start+LB as Select and Start+Select (or
  Start+RB) as Guide on a pad that has none, and a press shorter than
  25 ms held to 25 ms. A controller's Back is its Select here; since slice
  D5 the app gives Back from any device to the gameplay overlay first, as
  Artemis's Korri mode does, so it no longer reaches this mapping.
- Deadzones: Artemis's default, 5 % for each stick, a circle with no
  rescale (the host has its own). A trigger's deadzone is the larger flat
  of its two axes, 13 % when that is not between 13 % and 30 %.
- Heartbeat: Korri's addition to its Artemis copy. Each controller's last
  state goes again every 250 ms; a controller that went sends its last
  state four more times. A Korri sender's input seat (`services/inputd`)
  drops a source that is quiet for 1.25 s.
- Mouse mode: Artemis's default, "multi-touch" (mode 0). Each finger is a
  touch on the host (`LiSendTouchEvent`), placed in the picture from 0 to
  1, with its pressure, contact area and rotation. A host that takes no
  touch gets Artemis's absolute mouse: a tap clicks where it was, a long
  press (650 ms) right-clicks, a second finger scrolls. A mouse moves the
  host's pointer to the same place in the picture (`LiSendMousePositionEvent`,
  with the picture's size), and clicks and scrolls. Start held for 750 ms
  turns a controller into a mouse, as in Artemis; `-nomouseemulation`
  turns that off, as in Moonlight.
- Keyboard: Android's key code to the Windows virtual key, then the Linux
  scan code; modifiers from the keys themselves and the meta state; a
  repeated key is not sent again. Ctrl+Alt+Shift+Q leaves the stream.
- Unbuffered dispatch: the app asks the window's decor view for
  `requestUnbufferedDispatch` of every input class, as Artemis asks its
  views. NativeActivity's input queue is fed by the same view root, so it
  should apply (inferred); the emulator cannot show a difference, and a
  phone's measure (number 5 of ADR 0062) is step H's.
- Motion sensors: left for later. They need a `SensorEventListener`, a Java
  interface. #434 allows Java bundled unchanged inside a dependency; D4
  takes no such dependency, and the arrival claims no gyroscope or
  accelerometer.

**What differs from Artemis:**

| | Artemis | Korri D4 |
| --- | --- | --- |
| Rumble, trigger rumble, RGB light, battery reports | `VibratorManager`, `LightsManager`, `BatteryState`, SHIELD's API | Slice D4b (below) |
| Motion sensors | Controller or phone sensors through `SensorEventListener` | Not carried (above) |
| A controller's own touchpad | `LiSendControllerTouchEvent` | Not carried; the arrival claims no touchpad |
| USB driver for Xbox and Switch Pro pads | Its own, in Java | Dropped (ADR 0064) |
| Guide and the Start double press | Open Korri's overlay | Slice D5 (below) |
| Pointer capture (relative mouse), Ctrl+Alt+Shift+Z and C | Captures a mouse when one is attached | Not carried: a mouse is absolute, as Artemis's is without capture |
| Mouse modes 1 to 5 and their menu | Absolute, trackpad (two kinds), off, swapped buttons | Multi-touch only, Artemis's default; korrid passes no mode |
| Pen and eraser | `LiSendPenEvent` when the host takes it | A mouse, as Artemis falls back to |
| A touchpad's own gestures, two-finger click | `TrackpadContext`, `getActionButton` | Not carried; `AMotionEvent_getActionButton` is API 33 |
| Text from a soft keyboard, unmapped printable keys | `LiSendUtf8TextEvent` | Not carried: the NDK gives no characters |
| QWERTY normalization | API 33 and newer | Never, as Artemis at API 31 and 32: every key has the non-normalized flag |
| Key flags | Android's virtual navigation keys pass; the "Tablet Remote" fix | android-activity gives no key flags |
| A device that changes | Its context is made again | Not followed |
| A device that goes | `InputDeviceListener` | Asked twice a second |
| Settings | Deadzone, flip face buttons, Back as Guide or Meta, Joy-Con fix, one controller only, mouse buttons, touch sensitivity, gestures, vibration fallback | Artemis's defaults; mouse emulation follows `-nomouseemulation` |
| Input held under the overlay | `NvConnection.setInputHeld` | Slice D5 (below) |

**Proof** (`nix run .#viewer-android-proof`, record `proof/android-d4/`):
D3's run, with input. While the stream runs, the proof uses Android's own
tools in the emulator: `input keyevent` (K, O, R, R, I), `input tap` and
`input swipe` on the picture, `input mouse tap`, and a USB game pad that
the emulator's `hid` command makes through `/dev/uhid` (Microsoft's
vendor ID, an unknown product: `hid-generic` and `Generic.kl`): A, the left
stick, the right trigger, the hat, B, and X held for a second; then the pad
goes. The sender fixture's Sunshine logs each input packet that it
receives (`log_input`, `nix/stream-cadence/README.md`) and passes none on;
the proof reads what arrived there. Screenshots after the input show the
picture still moving. When the pad goes, its release must arrive, then the
heartbeat's four repeats: the first at the heartbeat's next tick, which
can be anything up to one period after the release plus the delay of the
loop and the network, so less than 500 ms is wanted; the next three
250 ms apart, give or take 50. `android/proof/run.test.py` runs that rule
over recorded gaps, with no emulator (its docstring has the command).

- Not proven: a real phone's controllers, touch screen and keyboard, and
  any time from a press to its packet (step H, the Galaxy Z Fold 8). That
  unbuffered dispatch changes anything on NativeActivity.
- Cost: the zlib notice of Valve's and SDL's controller lists
  (`controller_list.rs`) is not yet in the APK, as those of OpenSSL, curl
  and ENet (slice D1) and libopus (slice D3) are not.

## Android rumble, light and battery (#391 step D, slice D4b)

The brief put rumble and battery in D4; they are a slice of their own.
D4b carries what Artemis's `ControllerHandler` does at API 31: the host's
rumble and trigger rumble on the controller's vibrators, the host's colour
on its RGB light, and each controller's battery to the host. All of it is
calls into Android's own classes through JNI (brief, finding 10). Motion
sensors stay out: they need a Java listener (D4, above).

| File | Port of (Korri's copy of Artemis) |
| --- | --- |
| `src/android/feedback.rs` | `ControllerHandler.java`'s vibrator choice, `handleRumble`, `handleRumbleTriggers`, the three `rumble*Vibrators`, `handleSetControllerLED`, `sendControllerBatteryPacket`, `InputDeviceContext.destroy` and `stop`; `Game.java`'s callbacks. Pure, with the feedback thread and the core's callbacks; tested over a scripted Android (`Scripted`) |
| `src/android/feedback_jni.rs` | The same calls into `VibratorManager`, `Vibrator`, `CombinedVibration`, `VibrationEffect`, `LightsManager` and `BatteryState`, from Rust |
| `src/android/gamepad.rs`, `devices.rs` | `sendControllerArrival`'s capabilities; `InputDevice`'s vibrators and lights |

Both new files port Artemis code and are GPL-3.0-only, as the D2 to D4
files are.

- The core's callbacks: `rumble`, `rumbleTriggers` and `setControllerLED`
  are the first callbacks from the core into the input. The core calls
  them on its own thread (`CtrlAsyncCb`). Each runs its body under
  `catch_unwind` (`callback::guarded`) and only leaves the controller's
  latest value for the feedback thread: no JNI and no wait on the core's
  thread. Without the app, as in the session tests, the core keeps its own
  empty callbacks.
- Latest values, no backlog: of rumble, trigger rumble and light only each
  controller's latest value waits for the feedback thread, which wakes on
  a signal and takes them all. While a call into Android stalls, a value
  that is replaced is never sent, and the last one always is, so a rumble
  of 0 is never lost. Artemis gets the same from the core: its callback
  holds the core's thread, and the core merges back-to-back rumble packets
  of a controller. The seats' changes wait in order.
- The feedback thread (`feedback::Worker`, thread `controller-feedback`):
  the app starts it with the stream. It attaches to the VM once, for its
  whole life; Rust's thread-local guard detaches it when it ends. It alone
  calls Android for the feedback, and it reads the batteries, which
  Artemis keeps off its main thread too: `getBatteryState` is a Binder
  call. A panic on it ends the feedback, not the stream: the thread then
  waits for the stop alone, with no deadline, and the stop still runs.
- Seats: when a controller's arrival goes to the host, the app's thread
  tells the feedback thread its device, number, vibrators and light
  (`Seats`). A device that goes leaves its seat: its vibration and light
  stop. Rumble for a number goes to every device with that number, as in
  Artemis; a number that no device holds rumbles nothing.
- Vibrators, as Artemis chooses them: the controller's own
  `VibratorManager` with four vibrators with amplitude control (motors
  and triggers), else two (motors), else its single `Vibrator`. A built-in
  controller without any takes the phone's own, the same way. Two
  vibrators get the high-frequency motor first, then the low one, in
  `getVibratorIds()` order: one parallel vibration of 60 s one-shots at
  each motor's high byte, and a cancel when all are 0. One vibrator gets
  80 % of the big motor and 33 % of the small one, or a 20 ms pulse without
  amplitude control. Trigger rumble needs four vibrators.
- Light: the controller's `LightsManager` session, opened at the first
  colour, asks every light with RGB control for the host's colour.
- Battery: read when the controller arrives and every 120 s, and sent with
  `LiSendControllerBatteryEvent` when status or capacity changed. It goes
  through D4's input gate (`CORE_GATE`), and through the stream's own gate
  for its feedback, which the stream's stop closes: outside the stream, or
  from a read that ends after the stop, it goes nowhere, and the feedback
  counts it.
- The gameplay overlay's hold (slice D5) holds no battery report, rumble or
  light, as Artemis's `setInputHeld` holds none of them.
- Arrival: Artemis's capabilities at API 31. Rumble for any vibrator,
  trigger rumble for four; battery for every external controller, without
  reading it first (a Binder call that can stall); the RGB light for a
  PlayStation pad, or for any pad from Android 14, because before it every
  light says it has RGB control; analog triggers as in D4.
- Teardown: before `LiStopConnection`, after the input's gate closes,
  `stream.rs` stops the feedback (`PlayerIo::stop_feedback`). The core's
  callbacks reach nothing more, and the stream's battery reports stop. The
  stop goes before every value that waits, which it drops: every
  controller's vibrators are cancelled, then the phone's own, as Artemis's
  `stop` cancels it; then the light sessions are closed and the battery
  polling ends. The stop waits for the feedback thread at most 2 s
  (`STOP_WAIT`); a call into Android that is under way cannot be cut
  short, and the stop runs when it returns. The log
  says what it did (`feedback: ended: … rumbles, … trigger rumbles, …
  lights, … calls to Android; … battery reads, … battery reports sent, …
  outside the stream; … failed`).
- Permission: the manifest asks for `VIBRATE`, as the Artemis app does.

**What differs from Artemis:**

| | Artemis | Korri D4b |
| --- | --- | --- |
| Where rumble and light run | Java, on the core's callback thread through JNI | The core's thread only leaves the latest value; the feedback thread calls Android |
| Back-to-back rumble | The callback holds the core's thread, so the core's queue of 30 bounds the work, and the core merges back-to-back rumble packets of a controller | The callback returns at once, so the core seldom has two packets to merge: the feedback keeps each controller's latest rumble, trigger rumble and light value instead |
| A vibrator or light call that throws | The exception goes back through JNI into the core's thread, and the app ends | Logged and counted; the next call goes on |
| Where the battery is read | A `HandlerThread` of its own | The feedback thread |
| A stop that hangs in a Binder call | The UI thread waits | The stream's stop waits at most 2 s, then goes on without the feedback thread, which stops the vibration when its call returns; that thread sends no battery report into a later stream |
| Settings | Rumble on or off, rumble the phone instead of the pad, rumble the phone when the pad has no vibrator (and its strength), battery reports on or off | Artemis's defaults: rumble on, the pad's own vibrators, no fallback to the phone, battery reports on |
| The overlay's rumble switch (`stopRumble`) | Korri's Artemis copy | The stream's control "Rumble" (slice D11, below); the overlay that shows it is D5's |
| SHIELD controllers | `SceManager` rumble and battery | Not carried: NVIDIA's own API |
| USB driver's rumble and trigger rumble | Its own, in Java | Dropped (ADR 0064) |
| A device that changes | Its new context takes the light session and polls the battery again | Not followed, as in D4 |
| More vibrator IDs than motor values | The Java array overflows | The extra vibrators are left out |
| Device objects | The `InputDevice` of the context | `InputDevice.getDevice(id)` for each battery read and each colour, and when a device's vibrators are first used; the vibrators and the light session are kept |
| A battery read that throws | Not caught | Logged and counted; the next read is 120 s later |
| A pulse below Android 13 (one vibrator, no amplitude control) | The deprecated `vibrate(long[], int, AudioAttributes)` | `VibrationEffect.createWaveform` with the same pattern and `AudioAttributes` |
| Motion sensors | `setMotionEventState` | Not carried (D4) |

**Proof** (`nix run .#viewer-android-proof`, record `proof/android-d4b/`):
D4's run, with two more parts. By the owner's rule of 2026-11-14 no build
and no emulator runs on zao: the record's run ran on aka, which is also
the sender, with `--sender local --stream-host <aka's address>`
(`android/proof/run.py`).

- In the stream, after the audio server's restart: a Bluetooth DualShock 4
  that the emulator's `hid` command makes through `/dev/uhid` from
  Android's own CTS data (`android/proof/dualshock4-bluetooth.json`,
  Apache-2.0). The kernel's `hid-sony` drives it, so Android gives it two
  vibrators, an RGB light and a battery. It reports 60 %, discharging, and
  presses circle: the app seats it, reads its battery and sends it. The
  sender's Sunshine logs the arrival (PlayStation, analog triggers,
  rumble, battery, RGB light) and the battery packet. Then the pad goes,
  and its seat with it, before D5's overlay, whose checks read every
  controller packet after their marker. When Home ends the stream, the
  log shows the feedback's end after the input's gate closed and before
  the core began to stop.
- The feedback probe (`android/proof/feedback`, a test app, never
  shipped): the fixture's Sunshine runs with its controllers off and its
  user cannot make input devices, so the host sends no rumble and no
  light. The probe plays the core's part with the viewer's own code from
  the input on: it presses A on a new DualShock 4 through `Input`, then calls
  the stream's `rumble`, `rumble_triggers` and `set_controller_led`
  callbacks from a thread that is not attached to the VM, and stops the
  feedback as the stream does. The pad's own output reports (which `hid`
  prints) and `dumpsys input` show what reached it.

- Not proven: rumble or a colour that a host sends (the fixture's Sunshine
  sends none); trigger rumble on a device (Linux gives no input device four
  vibrators, as Artemis's own comment says); a phone's own vibrator for a
  built-in controller (the emulator has no built-in controller); a real pad
  over Bluetooth or USB, its motors and its battery (step H); Android 14's
  RGB rule (the emulator runs Android 12).

**Gate:** D7, which removes `Game.java`, does not ship until D4b's proof
on a real pad (step H) agrees with this one.

## Android gameplay overlay (#391 step D, slice D5)

Slice D5 of the research brief, by the D5 spike's route
(`docs/research/2026-11-14-android-overlay-spike.md`, design C with a
JNI-made overlay layer): a second layer above the video, opened and
closed as Artemis's Korri mode opens and closes its overlay, with the
stream's input held while it is open. The shell's own overlay cannot run
in this app (below), so the layer shows a stand-in.

- The layer (`android/src/layer.rs`): `korri-viewer-overlay`, made with
  the video layer on Android's main thread through the same JNI calls,
  translucent (`setFormat(TRANSLUCENT)`, `setOpaque(false)`), at z 2 above
  the video's 1 and the black backdrop's 0, hidden. It stands out of the camera cutout, as Artemis
  keeps its overlay view inside the display cutout's safe area
  (`KorriStreamCutout.keepOutOfCutout`, #494): in the window less the
  cutout's insets (`placement::keep_out_of_cutout`, host-tested), which
  the main thread reads from the window
  (`getRootWindowInsets().getInsets(WindowInsets.Type.displayCutout())`)
  each time it places the layer, with one `setGeometry`. Its buffers have
  that size: when the window's size changes, `Transaction.setBufferSize`
  gives its Surface the new size, in the transaction that places the video
  again. A main event after which the cutout may have moved while the size
  stayed (inferred, not logged: a turn by 180° asks the window to draw again, `RedrawNeeded`;
  `layer::may_move_the_cutout`) makes the main thread read the cutout
  again and move the layer when its place changed
  (`Layers::follow_cutout`). That size is what the shell's renderer reads
  (`SurfaceWindow::from_surface` and `resize`), and a touch reaches the
  shell's overlay from the layer's corner (`Layers::overlay_origin`), as
  `Game.dispatchTouchEvent` offsets it. The log names each place:
  `overlay: layer placed at (L, T)-(R, B) in the window WxH, out of the
  display cutout (left …, top …, right …, bottom …)`. The video and the
  backdrop still reach the window's edges, into the cutout. Opening and closing are one `setVisibility` transaction each,
  posted to the main thread without waiting; the main thread logs when it
  has applied it (`overlay: layer shown: posted … ms before, applied in …
  ms`). A hidden layer is not composed. When Android does not make the
  layer, the stream plays without an overlay, and the log says so.
- The stand-in: the window's lower half in black at 75 % opacity, the
  upper half clear, drawn once with the CPU (`ANativeWindow_lock`), and
  again after a resize. It is not a design. The proof reads the frame-ID
  bar through the clear half.
- Open and close (`src/android/overlay.rs`, host-tested):
  `KorriOverlayHomeKey`. Guide or Back, from any device, toggles the
  overlay on its release; a whole press belongs to the state it began in,
  so the release that opened it does not close it. Artemis's quick menu
  too (`gamepad.rs`): Start pressed again within 200 ms of its release,
  held longer than 750 ms and let go, opens the overlay on that release
  instead of turning the pad into a mouse. Start's release goes to the
  host first, as for any release, so that the host does not keep Start
  pressed when no overlay layer opens. While the overlay is open, the
  volume and power keys stay Android's, and every other key and every
  motion event is the overlay's.
- The hold (`Input::hold` and `release` in `src/android/input.rs`,
  host-tested): `NvConnection.setInputHeld`, one gate below every
  producer, so the heartbeat and the touch's and the emulated mouse's
  timers are held too. When the overlay opens, the host sees every
  controller let go (its last mask, nothing pressed), every key that is
  down go up, every mouse button go up and, after a touch, a cancel of
  every touch. Then the mapping starts clean (`resetStreamInputState`:
  controllers with their emulated buttons and Start times, heartbeat,
  modifiers, mouse buttons, the absolute touch),
  and nothing more goes but a controller's arrival. When it closes, the
  mapping starts clean again, and input goes again. The hold sends only
  into the sink it is given, which in the app is the core behind its gate
  (`CORE_GATE`): no let-go reaches a core that stops. The report counts
  what the hold kept (`… held`).
- Leaving: Back no longer ends the stream. Ctrl+Alt+Shift+Q and leaving
  the app (Home, another app) end it. Back leaves only while the app
  waits for its window (`wait_for_window` in `android/src/lib.rs`); from
  the moment the layers are made, before the stream has started too, it
  goes to the overlay.
- `catch_unwind`: D5 adds no `extern "C"` callback. The hold and release
  run inside the input's `Guarded`; the main-thread posts run under
  android-activity's own `catch_unwind`.

**What differs from Artemis** (Korri mode: `Game.showSurfaceOverlay`,
`hideSurfaceOverlay`, `KorriSurfaceHost`, `NvConnection`):

| | Artemis | Korri D5 |
| --- | --- | --- |
| What the overlay shows | The active surface package draws korrid's `GameplayOverlay` model, through the shell's Slint renderer | A stand-in, drawn once with the CPU |
| The layer | A `SurfaceView` (`setZOrderMediaOverlay`), added and removed for each opening | A `SurfaceControl` made once with the video's, shown and hidden |
| Close | Guide (as Menu) and Back go to the package; its Resume goes through korrid, which hides the overlay | Guide or Back closes it |
| Leave and Resume with korrid | `KorriStreamViewer.leave` tells korrid; korrid's Resume, Disconnect, End and Open Korri come back | None: no korrid in this process |
| The overlay's input | Keys, axes and touches to the package (`KorriSurfaceHost.forward`) | Taken and dropped: the stand-in takes none |
| A released key's flags | None | The flags it went down with, so that the host releases the key it pressed |
| A controller's touchpad and gyroscope on hold | Cancelled | Nothing to cancel: D4 sends neither |
| A pen on hold | Cancelled | Nothing to cancel: a pen is a mouse in D4 |
| Five-finger tap, floating menu button | Open the menu | Not carried: the tap is on the trackpad path, which D4 does not carry |
| A failed overlay | A toast, and the overlay closes | No layer: the stream plays without an overlay |
| Back | Opens the overlay; Home leaves | The same |
| The camera cutout | The overlay view's margins are the display cutout's insets, and follow them (`KorriStreamCutout.keepOutOfCutout`) | The layer stands in the window less the cutout's insets, and follows them (#494) |

**Why the shell's overlay cannot run in this app.** The overlay that the
shell draws today needs, in Artemis's process:

1. korrid, for the overlay's content (ADR 0053, `GameplayOverlay`) and its
   actions. This app runs no korrid. Artemis runs it in `KorriBrainService`,
   a Java foreground service (brief, finding 13), and gives the shell its
   port and capability through `KorriShellActivity.shellPort` and
   `shellCapability`.
2. The shell itself, under this app's event loop. `clients/shell` has its
   own `android_main` and runs Slint's `AndroidPlatform` loop for the
   activity (`slint::run_event_loop_until_quit`); this app has its own
   `android_main` and loop, and android-activity gives a process one. The
   shell takes the overlay's Surface, size changes and close from the Java
   bridge `KorriSurfaceHost` through `KorriShellActivity`
   (`shellTakeOverlaySurface`, `shellOverlayDensity`, `shellOverlayResized`,
   `shellOverlayClosing`, `shellOverlayReleased`), on its 16 ms Slint timer.
3. The surface packages: `KorriShellActivity.copyPackage` unpacks velvet,
   pico and zone from the Artemis APK's assets; this APK has none.
4. The overlay's input: `Game` forwards keys, axes and touches into
   `KorriSurfaceHost`'s queues, which the shell drains
   (`shellTakeOverlayKeys`, `shellTakeOverlayAxes`, `shellTakeOverlayPointers`).
5. Leave and Resume: `Game` tells korrid that the person left
   (`KorriStreamViewer.leave`, `KorridServer.leaveViewer`), and korrid's
   Resume comes back through `KorriStreamViewer.resume`, which calls
   `Game.hideSurfaceOverlay`. The viewer treaty (`contracts/viewer`) has no
   Leave; its `Background` and `ResumeAndFocus` are Linux's focus effects.

The renderer itself needs none of these. `SurfaceWindow::from_surface`
takes a JNI environment and an `android.view.Surface`, and this app has
the overlay layer's (`new Surface(SurfaceControl)`). It needs Slint and
Skia in the APK; the vendored backend's build also compiles Slint's own
Java helper into its library, for `AndroidPlatform`, which
`SurfaceWindow` does not use. A throwaway check, not committed (as the
spike's code was not), ran this on the API 31 emulator on 2026-11-14: a
NativeActivity made the overlay layer as `layer.rs` makes it, over its
own window filled blue, gave the layer's Surface to the vendored
`SurfaceWindow::from_surface`, and showed a small `.slint` component
(a dim, a rounded panel and a line of text) through `slint-interpreter`'s
`create_with_existing_window`, under a `slint::platform::Platform` that
makes no window of its own. The first frame came 698 ms after `show`, the
layer showed it see-through, and `release()` returned
(`proof/android-d5/slint-surfacewindow.png`). So the spike's last
inference holds on the emulator: the shell's renderer draws into this
layer. Its library was 33 MB for x86_64 alone, and its Skia came prebuilt
from rust-skia (0.153.3, `ganesh-gl-jpegd-jpege-pdf-vulkan`) at build
time; the viewer's Nix build cannot download, so it would need those
archives as fixed inputs.

What remained for D5 came with the Korri app (#434), which runs korrid,
the shell and this viewer in one process: its slice D5b lends this layer's
Surface to the shell, which draws korrid's gameplay overlay into it
(`clients/android-app/README.md`). The library's parts for it:

- `overlay::Presenter`: what shows the overlay. `StandIn` is this app's
  (the stand-in above); `Shell` is the Korri app's. With `Shell` the layer
  gets no stand-in (a Surface that the CPU has drawn into stays the CPU's,
  and EGL cannot draw into it), `Layers::overlay_surface` gives its
  `android.view.Surface`, and `Layers::overlay_resized` says when its
  buffers took a new size.
- `play::Play` with `Shell`: Guide or Back opens the overlay and holds the
  input as before, and answers `Routed::Opened`; the app brings the shell's
  overlay. What the open overlay takes waits for the shell
  (`Play::forwarded`, `overlay::Forwarded`: `KorriSurfaceHost.forward` and
  `forwardTouch`, ported and host-tested): keys as their first down and
  their up, Guide as Menu, a controller's stick and hat in thousandths, and
  touches in the window's pixels. A Guide or Back press that began with
  the overlay open goes to the shell whole at its release
  (`Game.handleKorriHomeKey`), and the shell's package decides; the app
  closes the overlay (`Play::close`) when korrid resumes the stream.
- `overlay::Route::Overlay`: a key that the open overlay takes, apart from
  `Route::Taken`, an edge of Guide or Back that nobody sees.

With `StandIn` this app does what it did. Resize stays unavailable (#476).

**Proof** (`nix run .#viewer-android-proof`, record `proof/android-d5/`):
D4's run, then the overlay, after Android's audio server has restarted,
so that the sound under it has come back first. A second game pad, fed
through the `hid` command's stdin so that each report goes when the proof
says, holds A and presses Guide. While the overlay is open the proof takes
screenshots (the frame-ID bar must read and rise through the clear half,
and the stand-in must darken the lower half), looks at Android's mixer
twice (the app's track must advance and the last power must be the tone),
and sends the pad's B and left stick, the keys H, E, L, D, a tap and a
mouse click. Then Guide closes it, and X, Z and a tap go; Back opens and
closes it too. The sender's Sunshine logs every input packet
(`log_input`): after a marker key, the held input must not be there, A
must be let go, the pad must send nothing while the overlay is open (the
heartbeat included), and the input after it closes must be. The proof
then presses Home, which ends the stream (Back no longer does).

- Frames with the overlay open: the composer missed more frames while
  the overlay was open, on both hosts. The decoder lost frames only on
  the loaded host. On zao the decoder lost more frames while the overlay was
  open. In the kept run it asked for an IDR frame 0.43 times a second
  while the overlay was open and 0.03 times a second while it was not;
  in an earlier run, 1.55 and 0.26. The emulator's composer took all
  three layers as DEVICE (`surfaceflinger-overlay-open.txt`); this
  emulator's composer runs on the host, in software with
  `-gpu swiftshader_indirect` (inferred). Its missed frames rose by 142
  in the 8.6 s that the overlay was open, and by 55 in the 5.5 s after.
  But zao ran other work then (a load average of 24 in an earlier run,
  `proof/android-d5/README.md`), and the runs do not separate the
  overlay's cost from that load. On aka, #476's run
  (`proof/android-resize/`) had the overlay open for 7.3 s and 1.7 s. Its
  logcat has no "IDR frame request sent" line, and no frame went
  undecoded or dropped (`result.json`, `picture_losses`). But the
  composer's missed frames (`hwc_missed`, `result.json`) went 54, 70,
  73: 16 in the 7.3 s open and 3 in the 4.6 s after, about 3.4 times
  the rate with the overlay open. The spike presented fewer frames with a static overlay shown,
  also on zao, at loads of 5 to 90. Whether a phone's composer puts the
  video and the overlay on hardware planes is the spike's open risk, for
  the bench phone (step H).
- Not proven: a real phone, the arm64 library, a press's time to its
  packet with the overlay closing, a fold or turn while the overlay is
  open (the overlay buffer's resize is written but not exercised: in
  every run the window had its size when the layers were made), and the
  Start quick menu (host tests only).

## Android: a new picture size in the middle of the stream (#476, slice 4)

Slice 4 of #476's Route C, which every route needs: the sender changes the
stream's size, with a new SPS after an IDR frame, and the viewer follows it.
The request for a new size is not in this slice: Route C carries it
(korrid to korrid, then a local entry into Sunshine's resize channel,
slices 2 and 3). korrid asks for a size only when the viewer reports
`ready`; this viewer does with an app's picture (below, "A new stream size in
the middle of the stream"). `Resize` stays "Live resolution is unavailable":
korrid asks the holder, never the viewer.

- **Noticing it.** `decoder.rs` reads each SPS's picture size (`sps.rs`:
  the coded size less the frame cropping, ITU-T H.264 7.4.2.1.1) and
  compares it with the size that the codec was configured for. A new size
  takes effect with the IDR frame that carries it.
- **Following it.** A decoder with `FEATURE_AdaptivePlayback` (Artemis's
  `decoderSupportsAdaptivePlayback`: not `omx.intel` or `omx.mtk`) is
  configured with `max-width` and `max-height` at Korri's live-resolution
  maximum, which Korri wrote into its copy of Artemis (`cbfeff25b`): the
  largest square, from 4096 down to the stream's longer side in steps of
  8, that `VideoCapabilities.areSizeAndRateSupported` allows at the
  stream's rate, so that either orientation fits (`codec::adaptive_max`).
  A decoder that refuses those two keys gets a last configure without them
  (`codec::tries`) and has no adaptive playback. Within the maximum the
  codec takes the new SPS by itself. Any other decoder, and a size beyond
  that maximum, restarts as Artemis's recovery restarts it: stop, configure
  at the new size with the try that worked, start. A restart with adaptive
  playback asks the decoder's capabilities again for the new size; with no
  larger square, the new size itself is the maximum, and a decoder that
  refuses it restarts without adaptive playback. A restart holds the
  decoder's gate, so it never runs beside the output thread's codec calls;
  a frame that is in the codec at that moment is lost, and the IDR frame
  of the new size follows. A failed restart is a decoder failure: the
  recovery (#490, D2's "Decoder errors") takes it, the codec keeps the
  size that it had, and the next IDR frame brings the new size again. A
  restart that keeps failing ends the stream after the recovery's window.
  A restart configures through the same call as the recovery, so a
  recovery after a size change configures the codec at the new size.
- **The SPS patch follows the new size.** `level_idc` is Artemis's level
  for the new size, not for the size at the start.
- **The decoded size** comes from the output format (`crop`, else `width`
  and `height`), as Artemis reads it on `INFO_OUTPUT_FORMAT_CHANGED`. Each
  new size goes to the app on the output thread (`Picture.resized`), which
  wakes the app's loop.
- **The video layer.** One transaction, posted to Android's main thread,
  gives the layer's buffer queue the new size (`setBufferSize`) and places
  the new shape's letterbox (`setGeometry`, `placement.rs`). Without the
  buffer size, the queue would scale the decoder's buffers to the old size
  (scale to fit), and the picture would keep the old shape. The input's
  placement follows, so touches and the mouse map into the new picture.
  Until the transaction applies, a frame or two of the new size can show
  in the old shape. The gameplay overlay's place (slice D5) does not
  depend on the stream, so a new stream size leaves its place and buffers
  as they are; an open
  overlay stays open and the input stays held (`input::tests`, a new
  placement while held).
- **In the log:** `picture: the stream's size changes from 1280x720 to
  720x1280 at frame N; the decoder follows it (adaptive playback up to
  WxH)`, or `the decoder restarts (…)` and `picture: the decoder restarted
  at 720x1280`; then `picture: decoded size 720x1280` and `picture: video
  layer 720x1280 placed at (…) in the window …`. The decoder list and the
  `picture: decode …` line name each decoder's adaptive playback and the
  maximum.
- **D2 to D5's safety rules are kept.** No new `extern "C"` callback: the
  size reaches the app as a Rust call on the decoder's output thread,
  whose panic still ends the stream. The layer still outlives the decoder,
  and the input gate does not change.

**What differs from Artemis** (`MediaCodecDecoderRenderer.java`,
`Game.java`, `StreamContainer.java`):

| | Artemis | Korri |
| --- | --- | --- |
| A decoder without adaptive playback | Refuses a live resolution (`acceptsLiveResolution`) | Restarts at the new size: the sender made the change, so there is no request to refuse |
| `level_idc` after a change | For the size at the start (`initialWidth`, `initialHeight`) | For the new size: level 3.2 cannot hold 1920x1080 |
| A decoder that refuses `max-width` and `max-height` | Every try has them, so it gets no decoder (Korri's copy, `cbfeff25b`) | A last try without them, and a restart on a new size |
| The picture's shape | The SurfaceView's aspect ratio (`setDesiredAspectRatio`), scale to fit | The layer's buffer size and geometry |
| Waiting for the decoded size | `awaitLiveResolution`, up to 1.5 s, before it reports the resize applied | Nothing reports a resize yet (Route C, slice 5) |

**Proof** (`nix run .#viewer-android-proof`, record
`proof/android-resize/`): D5's run, with the size change. The sender is
stream-cadence's fixture with `runtime.sunshineFollowingOutput`, the same
Sunshine with one patch: it encodes at its hidden output's size. After the
first screenshots the proof asks the fixture's agent for `resize` to
720x1280: the hidden output takes that mode and the picture source starts
again at that size, so its frame IDs start again at 1. The proof waits for
the app's new decoded size and placement, then reads the bar in four
screenshots: readable, rising, and with the geometry of a 720x1280 picture
in the new letterbox (the cell width and height of the bar in the
screenshot). The input, sound, overlay and Home steps of D3 to D5 then run
at the new size: the overlay opens over the portrait picture, and one tap
a quarter across and three quarters down the new picture must reach the
sender at (0.25, 0.75). The frame record is decoded frame by frame at each
frame's own size, and its IDs must never go back within a size; a picture
that the sender encoded twice is listed, not failed, since the viewer
cannot make a unit. `--stock-sunshine` runs the stock Sunshine instead,
with no size change; the frame record must then keep one size.

- Not proven: the restart on a device. The emulator's decoder,
  `c2.goldfish.h264.decoder`, has adaptive playback (up to 2960x2960 at
  60 fps there), so the emulator takes the adaptive path; the restart is
  proven over the scripted codec only. A real phone's decoders, which may
  differ in their adaptive playback and their output format's crop (step
  H, the Galaxy Z Fold 8). The size change that a fold or a rotation asks
  for needs the request (Route C).

## A new stream size in the middle of the stream (#476, both viewers)

The holder's Sunshine can change the stream's encode size in the middle of
the stream (korrid's request, patch 0046). Both viewers follow it, and say
so in the treaty's snapshot (`ViewerShape`), which korrid's live-shape reads
(`services/korrid/src/upstreams/live_shape.rs`):

- `ready` is true exactly when the picture follows a new size
  (`PlayerIo::follows_new_sizes`): on Linux with X11 VA-API and the SDL
  fallback, on Android with the app's picture. The handheld `v4l2request`
  path keeps the size it started with and reports false, as does a stream
  with no picture (the session tests). With `ready`, korrid offers the
  holder the viewer's screen and asks for a new size when its shape
  changes; Portrait is enabled.
- `encoded` follows each new size, in the same connection. After each turn
  of its loop the stream's thread reads the size of the picture now shown
  (Linux: the last picture that a presenter handed to the display,
  `picture/place.c`; Android: the newest decoded size) and gives a new one
  to the lifecycle (`Lifecycle::resized`), which sends a Snapshot event. The
  log says `picture: the picture shown is WxH`.
- A shape needs a screen: Start's display. korrid's Linux adapter gives the
  local screen. The Korri app gives its window, longer side first, as the
  Artemis app gives its screen (`clients/android-app/README.md`).

**Linux: an aspect fit.** Every PC picture path fits the picture in its
window with the picture's own shape, centred, with black bars above and
below or at the sides; it is never cut, and stretched only within the
rounding below. One rule (`picture_fit`, `picture/place.c`) is Android's
video-layer rule (`placement.rs`, `letterbox`), and each presenter takes the
window's size for each picture, so a new stream size and a new window size
are the same case:

| Path | Before | Now |
| --- | --- | --- |
| X11 VA-API, `vaPutSurface` (Mesa on AMD) | Stretched to the window | A picture that fills its window: as before. A picture with bars: the EGL path of the next row. Mesa 26.2's `vlVaPutSurface` writes only its destination rectangle and presents its whole back buffer, so bars kept an old frame (the proof's first run on aka) |
| X11 VA-API through EGL (#477, Intel; and AMD for a picture with bars) | Stretched to the window | Clear to black, then the fitted viewport |
| SDL fallback | A texture of the start's size, stretched to the window | The texture takes each new size; clear to black, then the fitted rectangle |

One difference from Android's rule: bars of less than one percent of the
window are not drawn, and the picture fills the window. Such bars come from
the rounding of stream sizes (korrid rounds them to 8 pixels), not from a
shape; the picture is then stretched by less than one percent, as before
#476, and stays on `vaPutSurface`. A stream of another shape than the window
now shows with bars from its first picture, where it was stretched before.
korrid asks for the local screen's size, so a stream normally has the
window's shape.

The carried files change as the table says (GPL-3 §5a: modified by Korri on
2026-11-16): `video/x11.c` asks for the place of each picture and passes it
on, `video/ffmpeg_vaapi.c` takes a place instead of a window size and sends
a picture with bars to EGL, `video/vaapi_egl.c` takes a place, `sdl.c`
remakes its texture and fits its rectangle. (`video/egl.c`, the X11
software picture, did the same until its removal on 2026-11-17.) Each
presenter reports the size it showed (`picture_shown`) where it reports the
hand-off to the frame record.

Checks: `tests/test-place.c` (the rule, with Android's cases, and the
rounding),
`tests/test-x11-vaapi-size.c` (the place that the X11 presenter gives VA-API,
for a portrait picture in a landscape window and back), `tests/test-sdl.c`
(a real H.264 stream that turns from 128x72 to 72x128: the rendered window
has black columns at both sides and the picture between them; it failed
before this change), and the lifecycle tests (`ready`, and one Snapshot event for each new size).

**Proof** (`nix run .#viewer-linux-resize-proof -- --sunshine PACKAGE
--build-store auto`, on aka as root; `proof/linux_resize.py`): stream-cadence's
private pair with Korri's Sunshine and this `korri-viewer` on X11 VA-API.
korrid's resize request (the holder's last step, through the federation
fixture's `resize`) turns the stream to 720x1280 and back. Screenshots of
the fixture's output and the viewer's frame record show the picture fitted
with no stretch at each size, black around the portrait picture, and the
bar rising. Record: `proof/linux-resize/` (24 of 24 checks). Not proven: a
client korrid on Linux, the lifecycle report there (the fixture starts the
viewer without a socket), and what the EGL path costs on AMD for a picture
with bars.

## In the Korri app (#434, slice D10)

The Korri app with no Korri Java (`clients/android-app`) runs this viewer in
its own process, and korrid drives it through `Lifecycle::in_process`, the
treaty's requests and events with no socket: the app's README says how. For
that the parts that the viewer's own app kept to itself are library parts,
unchanged in what they do, and the viewer's own app uses them from here:

| Module | What |
| --- | --- |
| `src/android/layer.rs` | The video and overlay layers, and the video's new place for a new picture size (`Layers::restream`). `Layers::request` asks Android's main thread for them and `Pending::poll` reads the answer, for a loop that Slint owns; `Layers::make` waits while it polls android-activity, for the viewer's own loop. The overlay layer is the stand-in's or the shell's (`overlay::Presenter`, D5 above) |
| `src/android/events.rs` | android-activity's key and motion events as the input's values; the unbuffered dispatch request |
| `src/android/play.rs` | The stream's input with the gameplay overlay (D4, D5): each event's route, the hold, the timers, the device check, the panic guard; with the shell's overlay, what the overlay takes for the shell (D5b) |

`Options::for_lifecycle` makes a stream's options for a controller in the
viewer's process: korrid's display as the size, or Moonlight's default; the
Start request names the host and port. The library depends on
android-activity 0.6.1 on Android, the release of both apps and of the
shell's Slint backend.

## Stream controls and status (#434, slice D11)

Owner decision O4: the viewer carries the stream controls and status that
the Artemis app reports to korrid, through the treaty: Snapshot lists them,
Apply sets a control. Every field is what Artemis and korrid exchange today
(`KorriViewerControls.java`, `KorriMoonlightActionExecutor.java`,
`ViewerVideoStats.java` in `clients/android`); the treaty needed no new type
(`contracts/viewer` now names the surface treaty's control and health types
that it already carried).

Artemis can list thirteen controls (`KorriViewerControls.included`; its
`label` names a fourteenth, Rotate screen, that `included` never lets
through). A control is listed only when its viewer can perform it. This
viewer lists four, and the status:

| Item | Source in Artemis | Here |
| --- | --- | --- |
| Toggle keyboard (`toggle-keyboard`, command, closes the overlay) | `Game.toggleKeyboard`: `InputMethodManager.toggleSoftInput(0, 0)`. Listed only on a touch screen, as Artemis lists its touch controls (`KorriViewerControls.included`, `touchOnly`, `FEATURE_TOUCHSCREEN`) | The same call on Android's main thread (`soft_keyboard.rs`), through JNI. Listed only when `PackageManager.hasSystemFeature(FEATURE_TOUCHSCREEN)` (`status::touchscreen`). Before the call, NativeActivity's own view takes the focus, because the focus can be in Slint's text view (`SlintInputView`, a text editor), where the keyboard would type into the shell. NativeActivity's view is no text editor, as Artemis's stream view is not, so the input method sends key events, and D4's keyboard mapping sends them to the host. Apply answers once the main thread has made the call and the input method serves that view; no value changes (`KorriViewerControls.apply` remembers nothing for it). korrid closes the overlay before it applies (`dismissOnSuccess`). When the stream ends, the keyboard hides and the view gives the focus back. Not carried: text that no key event carries (`KeyEvent.ACTION_MULTIPLE`, Artemis's `LiSendUtf8TextEvent`), as in D4. **Known gap:** on the API 31 emulator the input method shows for the stream's view but its keyboard is never drawn, because its insets leash, which the app's window controls, stays hidden (`clients/android-app/proof/android-app-keyboard/`); so no key was typed through it |
| Crop to fill (`set-fill-mode`, toggle) | `Game.toggleFillMode`, `StreamContainer.setFillDisplay` | The video layer covers the window and its edges are cut (`placement::fill`): the layer's source crop is the stream's part that shows (`placement::geometry`), one `setGeometry`. The fill counts only once Android's main thread has applied it; one that it did not apply is undone (`placement::Fill`). The input maps into the whole picture, as Artemis's oversized view does |
| Flip A/B and X/Y (`set-face-button-flip`, toggle) | `ControllerHandler.handleFlipFaceButtons` | The same swap after the remapping, for each press from then on (`gamepad.rs`) |
| Rumble (`set-rumble`, toggle, on at the start) | `Game.setRumble`, `ControllerHandler.stopRumble`; listed only with a controller that can rumble (`hasRumble`) | Listed only while a controller of the stream can rumble with its own vibrators or the phone's (`Controllers::has_rumble`). Off: the core's rumble and trigger rumble are dropped as they come (`feedback.rs`'s inbox), and the feedback thread sets every controller's motors and triggers to 0 (`Feedback::stop_rumble`). On: the host's next rumble goes through. Apply answers once the switch is in the inbox; the vibrations stop at the feedback thread's next turn |
| Battery | `BatteryManager.BATTERY_PROPERTY_CAPACITY`, 0 to 100 | The same, through JNI (`status.rs`) |
| Health: delivered bitrate, frames dropped, sample time | `ViewerVideoStats`: each decode unit's bytes, each output buffer the display drops, over 1 s or more; stale after 3 s | `video_stats.rs` (its port, with its tests), counted in `decoder.rs`: the units that arrive and the overtaken frames that the output thread drops |
| Health: round trip | `MoonBridge.getEstimatedRttInfo` | `LiGetEstimatedRttInfo`, under the picture's lock (`picture::health`) |

Ids are Artemis's: `viewer:<launch>:<generation>:<lease>:<effect>`. The
generation is the runtime's number in the process (each Start of a launch
has its own lifecycle), the lease the activity that shows it, so a control
of an earlier runtime or activity is refused. Labels, `On`/`Off`, enabled,
not destructive and `dismissOnSuccess` (true only for Toggle keyboard) are
Artemis's. A command takes no value in Apply, and a toggle takes a toggle
(`KorriMoonlightActionExecutor.valid`). The values
belong to one stream and start at Artemis's defaults (fit, not flipped);
nothing stores them (`KorriStreamControlState`).

The treaty side (`src/lifecycle.rs`, host-tested): the thread that owns the
stream attaches the controls with its readings (`Lifecycle::controls`).
While the stream is connected, Snapshot lists them, the battery, the health
and its sample time; the readings run outside the lifecycle's lock. Apply
checks the id and the value as `KorriViewerControls.apply` does, hands the
change to that thread and answers once it has taken effect, within 5 s
(Artemis's wait for its UI thread). A refused change and a stream that
ended are errors and change nothing. When the 5 s pass first, Apply is an
error too, and what follows depends on the change, as with Artemis's
`task.cancel(false)`: one that the stream's thread has not taken yet is
cancelled and never runs; one that it took runs to its end, and the
snapshot then shows what it did. The values change only when the stream's
thread confirms a change that it took. With no controls attached, as on Linux
and in the viewer's own app, Snapshot and Apply answer as before.

Not carried, each because this viewer lacks the feature, not the control:

| Artemis control | What it needs |
| --- | --- |
| Full keyboard | Artemis's own keyboard view (`KeyBoardLayoutController`) |
| Pan & zoom | Artemis's `PanZoomHandler` gestures |
| Mouse mode, Touch sensitivity | Artemis's trackpad modes; D4 carries multi-touch only |
| Keyboard as controller | Artemis's on-screen controller |
| Toggle local cursor | Pointer capture: Artemis's `toggleLocalCursor` requests or releases it (`AndroidNativePointerCaptureProvider`), and a captured mouse is relative; D4 carries an absolute mouse only |
| SGSR sharpness, SGSR edge threshold | Artemis's GL renderer; MediaCodec renders straight into the layer here |
| Picture-in-picture | An activity in picture-in-picture mode; leaving the app ends the stream here (D10) |

Notice (Artemis shows a toast) and Resize (#476) stay unavailable.

## Network cores (#391 step E)

The C core (`moonlight-common-c`) is the default. With the cargo feature
`nightjar` the viewer also carries nightjar-core, a Rust port of
moonlight-common-c (`vendor/README.md`: the pin, the licence and Korri's
edits), and `-networkcore nightjar` streams with it:

- Linux: `nix build .#korri-viewer-nightjar`. Its `bin/moonlight` appends
  the option, so the stage-0 tool runs it as a candidate beside
  `korri-viewer`; `bin/korri-viewer` still streams with the C core unless
  asked.
- Android: `nix build .#korri-viewer-android-nightjar-apk` (the viewer's
  own app, both ABIs) and `.#checks.x86_64-linux.korri-viewer-android-nightjar`
  (the session part and its tests, both ABIs). The app's crate has the same
  feature (`android/Cargo.toml`). The starting intent's `args` pick the core
  as on Linux: `-networkcore nightjar` among them, else the C core.
  `android/proof/run.py --network-core nightjar` builds and starts that APK
  so. On arm64-v8a the build gives rustc `--cfg aes_armv8 --cfg
  polyval_armv8` (`android.nix`): RustCrypto's `aes` 0.8 and `polyval` 0.6,
  which nightjar-core decrypts with, compile their ARMv8 AES and PMULL code
  only under these cfgs and choose it at run time on a CPU that has the
  instructions (cpufeatures reads Android's `getauxval`). nightjar-core
  refuses an aarch64 build without them (`crypto.rs:10`), so that no phone
  decrypts video in software by mistake. The check proves both modules are
  in the arm64 library. The Korri app (`clients/android-app`) keeps the C
  core only.

`src/network_core/nightjar.rs` puts nightjar-core's `Session` behind
`NetworkCore`. The platform's picture, sound and input code does not
change: a frame becomes the C core's `DECODE_UNIT` (one `LENTRY` for each
of its buffers, with its receive, enqueue and presentation times on
nightjar-core's clock, which the frame record also reads); the audio
stream's Opus setup becomes `OPUS_MULTISTREAM_CONFIGURATION`, and a lost
packet is `decodeAndPlaySample(NULL, 0)`; stages, the start, the
termination, the log, rumble, trigger rumble, motion, LED, HDR and the
connection status go to the listener callbacks that are set. Input packets
go to nightjar-core's input queue, and its errors become the C core's
codes. The stream's end keeps the C core's order: each started table's
`stop`, audio then video, before nightjar-core's teardown joins the threads
that call into the tables (`Tables` in `nightjar.rs`). nightjar-core itself
joins first and stops after, and Android's decoder waits in `submit` until
its `stop`; a failed start stops its tables when nightjar-core reports the
stage, just before its teardown.

What differs from the C core, by choice (each a guess, for the measure to
judge):

| | C core | nightjar-core here |
| --- | --- | --- |
| Video datagrams | One `recvfrom` each | `recvmmsg` batches with UDP GRO (`receive_coalescing`, nightjar-cli's and its apps' setting) |
| Thread priorities | None | None while the decoder takes frames on the receive thread (`CAPABILITY_DIRECT_SUBMIT`, which every Linux decoder here sets): with "prioritize stream traffic" (`network_priority`) off, nightjar-core raises no thread and asks no D-Bus service. A decoder without direct submit gets nightjar-core's `VideoDec` thread, which it always raises (`vendor/nightjar/protocol/client/src/priority.rs:447`): on Linux nice -8, else -4, then the desktop portal or RealtimeKit; on Android, whose decoder (`src/android/picture.rs`) has no direct submit, nice -8 or -4 with `setpriority` only. The C core runs its decode thread at the default priority |
| A controller touch | No touchpad index | The controller's first touchpad |
| Its log | The C core's lines, among them one for each unrecoverable frame, IDR request and reference frame invalidation | nightjar-core's lines, which name no loss or request; at the stop its statistics (`nightjar: at the stop: …`: FEC repairs, frames dropped, decrypt failures) and the IDR requests and invalidations it sent (`nightjar: recovery requests sent: …`) |

The proof on aka, with Korri's patched Sunshine, picture, sound and input,
and the turns of the measure against the C core:
`proof/nightjar/README.md`.

Not done: the Korri app with nightjar-core, Linux aarch64 (`korri-viewer-nightjar`
is x86_64 only; it would need the same two cfgs), an emulator or phone run
on Android, and a korrid setting to choose the core.

## Next slices

The bench proof of the handheld `v4l2request` path. No device installs
`@korri:viewer` yet.

The cross-UID socket proof is an explicit root-only test: build
`cargo test --manifest-path clients/viewer/Cargo.toml --test lifecycle_security --no-run`
in the viewer development shell, then run that printed test executable as root
with `--exact separate_viewer_and_controller_uids_are_the_only_users_with_socket_access --ignored`.
It drops the viewer, controller and an unrelated peer to UIDs 65534, 65532 and
65533 in a temporary directory below `/run`, because `/tmp` is refused. It
changes no accounts or services. Run
`--exact filesystem_without_acls_fails_closed_and_keeps_nothing_the_viewer_made --ignored`
the same way: in a private mount namespace it mounts a `ramfs`, which has no
POSIX ACLs, below `/run`, and proves that the viewer refuses it with
`EOPNOTSUPP` and leaves nothing there. Normal checks also cover umask-zero
directory creation, missing controller authority, rejected peer credentials,
an ancestor that another user could write, the sticky `/tmp`, an ancestor or
the directory replaced by a symlink between two steps, a directory that fails
its owner check, and a real signal before `Start` and at stepped moments of
startup. After `Start` they send a real signal during a connect that the host
drops, a request that it holds, and a stream connection that it refuses, and
prove that no launch request follows a stop. The tests bind below
`XDG_RUNTIME_DIR`, which must be a private directory as a login session
provides it. In the Nix build sandbox the tests that need a bound socket prove
only the refusal described above.
