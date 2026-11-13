# Korri viewer — stage 1, PC slice

A Linux process owned by Korri, under GPL-3.0-or-later (ADR 0059).
The code belongs in Korri, not the plugin repository (ADR 0061). It is in
`clients/viewer` because the viewer belongs to a device's client part
(`CONTEXT.md`). It is not in korrid or the shell (ADR 0060).

```sh
nix build .#korri-viewer
nix develop .#viewer --command cargo test --manifest-path clients/viewer/Cargo.toml
nix develop .#viewer --command cargo clippy --manifest-path clients/viewer/Cargo.toml --all-targets -- -D warnings
nix build .#checks.x86_64-linux.korri-viewer-picture
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
- `picture/` holds the C decoder and presenters. `build.rs` compiles those
  files, not Moonlight's C session, HTTP, input or audio code.
- Audio packets are consumed by the core's null callbacks. No audio decoder
  or audio output runs. This slice measures pictures, not sound; its CPU
  number does not include the old viewer's Opus and null PCM work.
- The unchanged stage-0 tool needs `bin/moonlight` and the mapping path
  `share/moonlight/korri-inputplumber-gamecontrollerdb.txt`. The binary name
  is a package-entry symlink, not a second implementation. The mapping is
  the pinned upstream controller database; this input-free slice does not
  read it. It accepts the PC flags emitted by `linux_viewer::stream_args`
  and the plugin's rendered settings. `-input` and handheld platforms fail
  explicitly. Input is never enumerated, opened or forwarded.

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
rewritten. Only the PC files cross over. There is no handheld-specific decoder module, DRM/KMS presenter or patched
FFmpeg in this package. The generic FFmpeg decoder selection stays unchanged.

The adaptations do not change decode, queue or presentation policy:

- `video/x11.c` removes the input include and `x11_input_init` call.
- `sdl.c` removes the input include and event forwarding. Its existing event
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
`RuntimeDirectory=` with mode 0700 that the viewer user owns. Before it creates
anything, the viewer reads every directory from there up to `/` with `lstat`.
Root or the viewer UID must own each one, and the group and other users must
have no write access; a root-owned sticky directory such as `/tmp` is the only
exception. A symlink in the path is refused, so give the real path. The
controller can have search access to those directories, but not write access.
A user namespace that does not map root, such as the Nix build sandbox, shows
`/` with an unmapped owner, and the viewer refuses every location there.

The viewer then opens its new directory without following a symlink and keeps
that descriptor. It binds the socket and sets the socket's ACL through
`/proc/self/fd/<descriptor>/<name>`; mode, the directory's ACL and removal use
the descriptor. After `bind()` the directory must hold a socket that the viewer
UID owns, and the directory must still be at its path. If not, or if `/proc` is
missing, or if that address is longer than `sun_path`, the viewer removes what
it made and exits. A replaced directory name cannot move the socket.

The viewer authenticates `SO_PEERCRED` on every accept against the supplied
controller UID. A refused peer is logged and cannot take the controller slot.
korrid's adapter requires the expected viewer UID from its launcher and checks
both socket ownership and peer credentials against it, not against korrid's UID.
No installed launcher or plugin changes in this slice.

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

## Next slices

The removable `@korri:viewer`
plugin and version check; handheld `v4l2request` picture paths and their bench
proof; input; audio; frame timing grounded in the stage-0 frame record. Android
integration comes after the Linux stage. This package is not installed in the
product by this slice.

The cross-UID socket proof is an explicit root-only test: build
`cargo test --manifest-path clients/viewer/Cargo.toml --test lifecycle_security --no-run`
in the viewer development shell, then run that printed test executable as root
with `--exact separate_viewer_and_controller_uids_are_the_only_users_with_socket_access --ignored`.
It drops the viewer, controller and an unrelated peer to UIDs 65534, 65532 and
65533 in a temporary directory. It changes no accounts or services. Normal
checks also cover umask-zero directory creation, missing controller authority,
rejected peer credentials, an ancestor that another user could write, and a
directory replaced by a symlink before `bind()`. In the Nix build sandbox the
tests that need a bound socket prove only the refusal described above.
