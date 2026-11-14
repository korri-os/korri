# Carried PC sound and input (stage 1 / slice 2a, #435)

GPL-3.0-or-later. Original notices stay on every carried file.

## Modification notice (GPL-3 §5a)

Korri's modifications and adaptations identified below are dated 2026-11-13,
including the SDL startup mouse-capture correction. The view-only change
below is dated 2026-11-14.

Source: Moonlight Embedded `775444287305849ebdf4736c75298ad0713e2d5d`, with
`korri-plugins`' `services/moonlight/patches/` at
`2094c1e77471` applied in order. This is the same source as the picture slice.
Only patch 0014 changes the input files: it retains Korri's ownership of the
gamepad quit chord through `KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT`. No patch
changes these three audio backends.

## Exact adaptations

| Files | Difference from that patched source |
| --- | --- |
| `audio/{audio.h,alsa.c,pulse.c,sdl.c}` | None. Opus decoder, channel reorder, PCM buffering, recovery and output are unchanged. |
| `input/{keyboard.h,x11.h}` | None. |
| `input/{evdev.c,udev.c,x11.c}` | Include `../../picture/loop.h` instead of `../loop.h`; use the existing Rust-polled descriptor registry, not Moonlight's session loop. |
| `input/sdl.c` | Include `../../picture/sdl.h` instead of `../sdl.h`. Add `sdlinput_destroy` to close controllers after the core joins its threads. |
| `input/evdev.c` | Extend `evdev_stop` to join mouse-emulation producers before the core stops. Add `evdev_destroy` to remove devices, join mouse-emulation threads, close nodes and release grabs after the core joins its threads. `evdev_handle` returns -1 instead of calling `exit()` on a read error other than `EAGAIN` or `ENODEV`. `evdev_create` returns -1 instead of calling `exit()` when its device array cannot grow; it keeps the existing devices and closes the new node. |
| `input/{mapping.c,mapping.h}` | `mapping_parse` returns `NULL` with `errno` set to `ENOMEM` instead of calling `exit()` when it cannot allocate. `mapping_load` returns -1 instead of calling `exit()` when it cannot open the file or allocate a mapping. It gives the list through a pointer and closes the file. |
| `input/{udev.c,udev.h}` | `udev_init` returns -1 instead of calling `exit()` when `udev_new` fails, and -1 when `evdev_create` fails during discovery. `udev_handle` returns -1 when `evdev_create` fails for a new device. |
| `input/{evdev.h,sdl.h}` | Declare those teardown functions and the `int` result of `evdev_create`. |
| `bridge.{c,h}` | New narrow C boundary. Extract the PC branches of `main.c` input setup, controller count and rumble callbacks, and `platform.c` audio selection. Keep all session/launch/stop decisions in Rust. |
| `bridge.{c,h}` (aarch64) | `viewer_input_init_evdev` is `main.c`'s embedded branch for `-platform v4l2request`: `evdev_create` for each `-input` node, then `udev_init` without autoload when a node was given. It shares the PC evdev setup, so a failure of either returns -1 after it closes the opened devices. Compiled only with `HAVE_V4L2M2M`. |

The boundary checks `viewonly` before either input initializer, where
Moonlight's `main.c` checks it: `-viewonly` starts no udev, evdev, SDL
controller or CEC and opens no `/dev/input` node. It does not stop the
window's own keyboard and mouse, as in Moonlight: `x11_setup` calls
`x11_input_init` on the X11 VA-API platform, and the SDL platform's
`sdl_loop` forwards window events through `sdlinput_handle_event` and
captures the mouse once, whatever `-viewonly` says. Until 2026-11-14 the
boundary also checked `viewonly` before X11 and SDL forwarding, so view-only
mode sent no keys. That difference from Moonlight is removed (#432, number 5
of ADR 0062 measures a key on the viewer's window in a view-only fixture).
Bridge-owned checks for missing
mapping configuration or malformed `SDL_GAMECONTROLLERCONFIG` return errors.
No `io/` file calls `exit()` (#468). A setup failure makes `viewer_input_init`
return -1 after it closes the devices it opened, and Rust reports "input
initialization failed". A failure in the descriptor loop makes `picture_poll`
return -1, and Rust stops the core and reports "picture or input event loop
failed". Both reach the lifecycle failure report and socket removal. In
`picture/`, `loop_add_fd` and `util.c` still abort when they cannot allocate,
and `sdl.c` and `video/egl.c` still call `exit()` on setup failures; those
bypass both.
The bridge duplicates `SDL_GAMECONTROLLERCONFIG` before Moonlight's tokenizing
parser, so the process
environment is not modified. It caps the launch mask at the protocol's existing
16-controller limit. It omits embedded-only CEC on every platform, including
`v4l2request`. It takes explicit input nodes only with `-platform v4l2request`
on aarch64; korrid passes none on PC. Existing C backend failure and buffering
policies stay. The bridge calls `evdev_init` before `udev_init`
(`bridge.c:43,54`); Moonlight's `main.c` called them in the opposite order.
Explicit nodes are opened between the two, so they still precede udev, as in
`main.c`. For SDL, `viewer_input_start` now
makes the one initial `SDL_SetRelativeMouseMode(SDL_TRUE)` call from Moonlight's
`sdl_loop`, on the SDL platform with or without `-viewonly`, as `sdl_loop`
does. Timed polls do not recapture an explicitly released mouse.

Rust `player_io.rs` keeps the C strings alive and releases input and the
presenter on all Rust error paths. Input discovery now precedes launch, as in
Moonlight, so launch receives the discovered controller mask. Grabs start only
after the network connection starts. Rust stops input, joins the core, then
releases devices. No C session, signal handler or HTTP code is linked.

The picture adaptations are limited to:
- `picture/video/x11.c`: restore input setup through the boundary.
  That setup now precedes `ffmpeg_init` (`picture/video/x11.c:168`); the patched
  Moonlight source called it after decoder and pipe setup.
- `picture/sdl.c`: restore forwarding through that boundary; reset its quit state
  on initialization. The timed poll and rendering policy remain unchanged.
- `picture/bridge.c`: grow the descriptor registry for input discovery instead
  of aborting at 16 total descriptors. Free the registry at teardown. End
  `picture_poll` with -1 when a handler returns a negative result.

`default.nix` carries the plugin package's Xbox 360 hat correction and Xbox
Series mapping addition verbatim. It builds Opus without custom modes, as the
plugin does. The Nix package's test runner uses Xvfb because discovery now
precedes launch, including the HTTPS test whose sender refuses launch.

## Verification limits

`korri-viewer-io` checks actual decode/output into ALSA null, SDL virtual
controller discovery/hotplug and forwarding, real X11 events, and the evdev
mapping/event handler. Network-boundary recording uses linker wrappers in the
checks only. It does not replace production code. The check also causes each
former `exit()`: a missing mapping file, `udev_new` and allocations refused
through linker wrappers, and an `EBADF` read. Each returns an error, and the
check process continues. No physical input device is opened or grabbed on zao. The view-only check forwards a key on each PC
platform branch (SDL events, and an X11 key from another client on a real
Xvfb window through the descriptor loop), checks that no controller
subsystem started, traces actual opens and rejects any `/dev/input` path. The aka fixture adds its
own process isolation proof.

Not proven here: audible hardware output, a live Pulse server, physical evdev
hotplug/grabs/rumble, or button-to-network-packet latency. Those policies are
carried, not rewritten. The fixture sender has sound disabled and cannot prove
end-to-end sound without a separate, sound-enabled fixture.
