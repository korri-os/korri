# Carried PC sound and input (stage 1 / slice 2a, #435)

GPL-3.0-or-later. Original notices stay on every carried file.

## Modification notice (GPL-3 §5a)

Korri's modifications and adaptations identified below are dated 2026-11-13,
including the SDL startup mouse-capture correction.

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
| `input/{keyboard.h,mapping.c,mapping.h,udev.h,x11.h}` | None. |
| `input/{evdev.c,udev.c,x11.c}` | Include `../../picture/loop.h` instead of `../loop.h`; use the existing Rust-polled descriptor registry, not Moonlight's session loop. |
| `input/sdl.c` | Include `../../picture/sdl.h` instead of `../sdl.h`. Add `sdlinput_destroy` to close controllers after the core joins its threads. |
| `input/evdev.c` | Extend `evdev_stop` to join mouse-emulation producers before the core stops. Add `evdev_destroy` to remove devices, join mouse-emulation threads, close nodes and release grabs after the core joins its threads. |
| `input/{evdev.h,sdl.h}` | Declare those teardown functions. |
| `bridge.{c,h}` | New narrow C boundary. Extract the PC branches of `main.c` input setup, controller count and rumble callbacks, and `platform.c` audio selection. Keep all session/launch/stop decisions in Rust. |

The boundary checks `viewonly` before either input initializer and before X11
or SDL forwarding. This is an intentional difference from Moonlight's X11
setup, which installed keyboard/mouse input even with `-viewonly`. It is
required by the fixture's no-input contract. Bridge-owned checks for missing
mapping configuration or malformed `SDL_GAMECONTROLLERCONFIG` return errors.
The carried C still has `exit()` paths, including `input/mapping.c:179`,
`input/evdev.c:780,896` and `input/udev.c:62`. Those exits bypass Rust's
lifecycle failure report and socket removal; they are not converted to Rust
errors by this slice. The bridge duplicates
`SDL_GAMECONTROLLERCONFIG` before Moonlight's tokenizing parser, so the process
environment is not modified. It caps the launch mask at the protocol's existing
16-controller limit. It omits embedded-only CEC and explicit input nodes: korrid
passes neither on PC. Existing C backend failure and buffering policies stay.
The bridge calls `evdev_init` before `udev_init` (`bridge.c:43-44`); Moonlight's
`main.c` called them in the opposite order. For SDL, `viewer_input_start` now
makes the one initial `SDL_SetRelativeMouseMode(SDL_TRUE)` call from Moonlight's
`sdl_loop`, only when input is enabled. Timed polls do not recapture an
explicitly released mouse, and `-viewonly` never enables relative mode.

Rust `player_io.rs` keeps the C strings alive and releases input and the
presenter on all Rust error paths. Input discovery now precedes launch, as in
Moonlight, so launch receives the discovered controller mask. Grabs start only
after the network connection starts. Rust stops input, joins the core, then
releases devices. No C session, signal handler or HTTP code is linked.

The picture adaptations are limited to:
- `picture/video/x11.c`: restore input setup through the guarded boundary.
  That setup now precedes `ffmpeg_init` (`picture/video/x11.c:168`); the patched
  Moonlight source called it after decoder and pipe setup.
- `picture/sdl.c`: restore forwarding through that guard; reset its quit state
  on initialization. The timed poll and rendering policy remain unchanged.
- `picture/bridge.c`: grow the descriptor registry for input discovery instead
  of aborting at 16 total descriptors. Free the registry at teardown.

`default.nix` carries the plugin package's Xbox 360 hat correction and Xbox
Series mapping addition verbatim. It builds Opus without custom modes, as the
plugin does. The Nix package's test runner uses Xvfb because discovery now
precedes launch, including the HTTPS test whose sender refuses launch.

## Verification limits

`korri-viewer-io` checks actual decode/output into ALSA null, SDL virtual
controller discovery/hotplug and forwarding, real X11 events, and the evdev
mapping/event handler. Network-boundary recording uses linker wrappers in the
checks only. It does not replace production code. No physical input device is
opened or grabbed on zao. The view-only check traces actual opens on both PC
platform branches and rejects any `/dev/input` path. The aka fixture adds its
own process isolation proof.

Not proven here: audible hardware output, a live Pulse server, physical evdev
hotplug/grabs/rumble, or button-to-network-packet latency. Those policies are
carried, not rewritten. The fixture sender has sound disabled and cannot prove
end-to-end sound without a separate, sound-enabled fixture.
