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
  accepts the handheld arguments (below). `-viewonly` skips ALL input
  discovery, initialization and forwarding.

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

- `video/x11.c` routes input setup through the view-only guard in `io/`.
- `sdl.c` routes event forwarding through the same guard. Its existing event
  loop becomes a single timed poll, so Rust owns the lifetime. Texture
  updates, two-frame skip rule, renderer, vsync and rendering calls stay the
  same. Teardown also releases its SDL texture, renderer and mutex.
- `bridge.c` supplies the descriptor loop used by X11 and the C-variadic log
  callback. Rust polls either presenter, then stops and joins the network
  core before destroying the presenter. No connection logic lives in C.

The two copied C regression tests use a real Xvfb. They exercise VA failure
cleanup, resize destinations, newest-frame replacement during presentation,
and frame references after drops and presentation. A third C test decodes
and presents four actual H.264 frames through the SDL software fallback,
then reads the rendered pixels. It uses Xvfb and Mesa software rendering,
not a device. Rust tests exercise real TLS and HTTP requests, CLI compatibility,
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
  the `v4l2request` Rust cfg only for an aarch64 target. Other platforms keep
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

## Lifecycle socket (opt-in, not installed)

The Rust treaty is `contracts/viewer`; `contracts/generated/viewer.ts` is
Typeshare output. Its requests and replies extract `LinuxStreamViewer` and its
snapshot types. Native connection callbacks, as in Moonlight patch `0023`, emit
connection snapshots. The opaque connection identity uses that patch's PID and
monotonic timestamp. No frame-timing fields are added by this slice.

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
and keeps both descriptors. It binds the socket and sets the socket's ACL
through `/proc/self/fd/<descriptor>/<name>`; mode, the directory's ACL and
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
signal that arrives while the socket is made waits until it exists. Before
`Start`, a stop by a signal, `Stop` or a lost controller emits `Stopped` and
exits with status 0. While the viewer connects after `Start`, the removal does
not wait for the connection step; `Stopped` and the process exit do.

korrid's inactive `SocketLinuxStreamViewer` takes injected unit-start, failure
cleanup and compositor-focus effects. It never reads unit state or compositor
PIDs for lifecycle observations. korrid confirms compositor focus before
sending `Background` or `ResumeAndFocus`; the socket reports that confirmed
state, not an independent X11 focus model. Live resize, apply and notice return
typed unavailable errors, as the existing Linux trait defaults do. Snapshots
have no battery, health or controls until a real producer supplies them.

No installed plugin or production factory selects this adapter. The plugin
slice must supply its per-launch socket location, sandbox access, native unit
command, certificate provisioning, focus/cleanup effects, version check and
installed-plugin selection. Moonlight remains the production path.

## Sound and input provenance and checks

See `io/README.md` for every C adaptation and its source. The I/O check
encodes real Opus packets, decodes stereo/5.1/7.1 and verifies the PCM written
through ALSA's file plugin over null. It exercises the SDL event loop with a
virtual controller, X11 events on a real Xvfb window, and evdev's event and
mapping code with real libevdev capabilities. It checks forwarding at the
network-core boundary; it does not measure button-to-packet delay. A strace
of both view-only platform branches proves no `/dev/input` open is attempted.
The aka comparison and isolation records are in `proof/`.

## Next slices

The removable `@korri:viewer` plugin and version check; the bench proof of
the handheld `v4l2request` path; frame timing grounded in the stage-0 frame
record. Android integration comes after the Linux stage. This package is not
installed in the product by this slice.

The cross-UID socket proof is an explicit root-only test: build
`cargo test --manifest-path clients/viewer/Cargo.toml --test lifecycle_security --no-run`
in the viewer development shell, then run that printed test executable as root
with `--exact separate_viewer_and_controller_uids_are_the_only_users_with_socket_access --ignored`.
It drops the viewer, controller and an unrelated peer to UIDs 65534, 65532 and
65533 in a temporary directory below `/run`, because `/tmp` is refused. It
changes no accounts or services. Normal checks also cover umask-zero directory
creation, missing controller authority, rejected peer credentials, an ancestor
that another user could write, the sticky `/tmp`, and an ancestor or the
directory replaced by a symlink between two steps. The tests bind below
`XDG_RUNTIME_DIR`, which must be a private directory as a login session
provides it. In the Nix build sandbox the tests that need a bound socket prove
only the refusal described above.
