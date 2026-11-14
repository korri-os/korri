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
# Handheld path: aarch64 builds on aarch64 builders (here fuji or yari), never
# on a device. The x86_64 check then starts that build under qemu-aarch64.
nix build .#packages.aarch64-linux.korri-viewer
nix build .#checks.x86_64-linux.korri-viewer-aarch64
clients/viewer/proof/v4l2request/compare.sh ../korri-plugins
# The session part for Android, both ABIs (below). No device or emulator.
nix build .#checks.x86_64-linux.korri-viewer-android
# The Android app (slice D1, below), and its proof on one headless emulator
# against a private Sunshine on aka.
nix build .#korri-viewer-android-apk
nix run .#viewer-android-proof
```

## Boundaries

- `src/session.rs`, `pair.rs`, `stream.rs`: Rust owns HTTPS discovery,
  application selection, launch/resume, PIN pairing for the private fixture,
  connection start, termination and stop. SIGINT, SIGTERM, SIGHUP and SIGQUIT
  disconnect. Disconnect does not cancel the remote application.
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
  isolated null ALSA configuration works without a sound server.
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

`video/{ffmpeg,ffmpeg_vaapi,egl,sdl}.c` and their headers are copied, not
rewritten. The generic FFmpeg decoder selection stays unchanged. The x86_64
package compiles only these PC files and links the unpatched FFmpeg. The
handheld file and its FFmpeg are in the next section.

The adaptations do not change decode, queue or presentation policy:

- `video/x11.c` routes input setup through the boundary in `io/`.
- `sdl.c` routes event forwarding through the same boundary. Its existing event
  loop becomes a single timed poll, so Rust owns the lifetime. Texture
  updates, two-frame skip rule, renderer, vsync and rendering calls stay the
  same. Teardown also releases its SDL texture, renderer and mutex.
- `bridge.c` supplies the descriptor loop used by X11 and the C-variadic log
  callback. Rust polls either presenter, then stops and joins the network
  core before destroying the presenter. No connection logic lives in C.
- The frame record (below) adds calls and no policy. `video/ffmpeg.c` gives
  each packet a pts, so that the frame that the decoder returns names its
  decode unit. `video/x11.c` and `video/sdl.c` report each unit and its
  decoded frame. `video/ffmpeg_vaapi.c` reports the hand-off after a
  successful `vaPutSurface`. `video/sdl.c` puts the decoded `AVFrame` in its
  event, not its two arrays; `sdl.c` reads the same planes and the pts from
  it under the same mutex, and reports the hand-off after
  `SDL_RenderPresent`. With no record, each call returns at once and every
  pts stays `AV_NOPTS_VALUE`, as before.

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
defaults do. Snapshots have no battery, health or controls until a real
producer supplies them.

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
| `display_ns` | The call that hands the frame to the display returned (`vaPutSurface`, `SDL_RenderPresent`); empty for a frame that a newer one replaced | The viewer's own moment. The stage-0 display record's `display_ns` is the compositor's commit, later |

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

Only the PC presenters report so far. The handheld `v4l2request` path
(`picture/video/v4l2m2m.c`) has its own decode loop and calls none of the
record; with the variables set it writes a header and an empty recording,
and the tool then gives number 1 no value.

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

## Android session part (#391 step D, slice D0)

The session part also builds for Android at API 31 (ADR 0064), with the NDK
of `clients/android/sdk.nix`: `aarch64-linux-android` for phones and
`x86_64-linux-android` for the emulator. `android.nix` holds the recipe;
`checks.x86_64-linux.korri-viewer-android` builds both. Nothing in
`clients/android` uses it.

- Shared: `options.rs`, `session.rs`, `pair.rs`, `stream.rs`, `lifecycle.rs`
  and the binding are the same files on both platforms.
- Platform parts: `player_io.rs` is Linux's picture, sound and input.
  `src/android/player_io.rs` has the same interface and none of the three
  yet, so the core runs its own empty decoder and audio callbacks. The
  core's log goes to logcat, tag `korri-viewer`, through
  `src/android/log.c`: the callback is C-variadic, which Rust cannot
  define. The frame record and the signal handlers in `stream.rs`
  are Linux only: on Android the app owns its process's signals. The
  `v4l2request` cfg stays off.
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
feature is not "equal or better" (ADR 0062). A slice before D7 chooses the
route: a Korri patch on the core (first check its packet type 0x5504 against
upstream Sunshine, which is reported to use it for player LEDs), the Rust
network core of step E, or a control channel that korrid owns.

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
  permission). Other apps are refused. The app allows no backup (its files
  hold a client key pair), and a finished stream leaves no entry in Recents
  to start it again. Package `com.simonwjackson.korri.viewer`, minimum API
  31, target 34 as the Artemis app.
- In-process lifecycle: `Lifecycle::in_process` is the same runtime and the
  same replies as the socket, with no socket, UID check or signals. Its
  controller calls `Lifecycle::request` with the treaty's `ViewerRequest`,
  and each `ViewerEvent` goes to a function. `launch::run` holds the steps
  that the Linux command ran in `main.rs`; both platforms now call it.
- The app is that controller. It reads the launch from its starting intent,
  sends Start, and sends Stop on Back or when the activity is destroyed.
  When the runtime ends, the activity finishes. Every request, reply and
  event goes to logcat as the treaty's JSON, with the core's own lines. The
  intent's extras are D1's entry until korrid drives the viewer in this
  process; each takes the name of its source: `launch_id`
  (`ViewerRequest.launch_id`), `address` (`Start.address`), `args` (the
  command's `stream [Moonlight arguments] HOST`, a string array) and
  `KORRI_MOONLIGHT_SERVER_CERTIFICATE` (the pin's path). `Resize` stays
  "Live resolution is unavailable" (#476).
- Build: `android.nix` builds the library for both ABIs with cargo, links
  the manifest with `aapt2` against the android-36 framework of `sdk.nix`,
  adds the libraries and aligns with `zipalign`. No Gradle. The output is
  unsigned; the proof signs it with the user's debug key
  (`~/.android/debug.keystore`, made as Android Studio makes it when
  missing).
- Proof (`nix run .#viewer-android-proof`, `android/proof/run.py`): one
  headless Android 12 emulator (google_apis x86_64, 2 cores, `nice` 10,
  `android/emulator.nix`) on the build host, and stream-cadence's sender
  fixture on aka. The tool builds everything on the build host, pairs a
  fresh key pair with the Linux viewer's `pair`, runs the x86_64 HTTPS and
  trust-anchor tests in the emulator, installs the APK and gives the app the
  key pair and pin through `run-as`. It starts the app, waits for the
  Running event and the Start reply, streams, presses Back and waits for the
  Stopped event, the Stop reply and the activity's end. It reads Sunshine's
  log for the session, closes the fixture and stops the emulator. The
  emulator reaches aka's Tailscale address through its own NAT; aka admits
  any port on `tailscale0`. Record: `proof/android-d1/`.
- Not proven: the arm64 library on a phone (no device in this slice), and
  anything about speed. The emulator's numbers mean nothing (ADR 0063).
- Cost: GPL corresponding source and the notices of OpenSSL, curl and ENet
  are not yet in the APK. The APK is a debug build for the proof, not one to
  ship.

## Next slices

The bench proof of the handheld `v4l2request` path. On Android, slice D2:
MediaCodec into a layer made through JNI (the D5 spike's design C), with
decoder choice. No device installs `@korri:viewer` yet.

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
