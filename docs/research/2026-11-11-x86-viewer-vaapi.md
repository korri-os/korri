# x86 Moonlight viewer VA-API (#439)

## Selection and fallback

korrid requests `x11_vaapi` only when the device-tree compatibility file is
absent, the architecture is `x86_64`, and `/dev/dri` contains an existing
`renderD` node with a numeric suffix. This is a candidate check, not a promise
that a driver can initialize. Device-tree platform selection stays unchanged.

Moonlight Embedded `775444287305849ebdf4736c75298ad0713e2d5d` already
implements the probe and software fallback:

- `src/platform.c`, `platform_check`: `x11` calls `x11_init(false, false)`;
  `x11_vaapi` calls `x11_init(false, true)`.
- `src/video/x11.c`, `x11_init`: a successful `vaapi_init_lib` returns
  `INIT_VAAPI`; failed VA-API initialization returns `INIT_EGL`.
- `src/video/ffmpeg_vaapi.c`, `vaapi_init_lib`: FFmpeg initializes a VA-API
  device on X display `:0`. libva chooses the driver; korrid does not pin iHD.
- `platform_check` returns `X11_VAAPI` for `INIT_VAAPI`, otherwise `SDL` when
  built with SDL, or software `X11` without SDL. The shipped plugin builds SDL.

This fallback covers device initialization failure. It does not recover from
later codec/frame-context failures after VA-API initialization succeeds.

The `h264_v4l2m2m` attempt in an `x11` log comes from upstream
`src/video/ffmpeg.c`, `ffmpeg_init`, not Korri's separate `v4l2m2m` platform.
Its `SOFTWARE` branch tries `h264_nvv4l2`, `h264_nvmpi`, `h264_omx`, then
`h264_v4l2m2m`, before the ordinary `h264` decoder. SDL calls this shared
FFmpeg decoder. The VA-API branch skips these named hardware wrappers and uses
`h264`/`hevc` with VA-API callbacks.

The patches and package in `korri-os/plugins/services/moonlight/` (checked at
`9363f21d62b528b115355ccc0e372a361edfc412`) retain those X11, VA-API and SDL
decisions. All ten Moonlight patches applied successfully to a source copy.
The X11 video/input, VA-API, shared FFmpeg and udev files remained byte-identical
to upstream. The added V4L2 platforms do not make `-platform x11` try VA-API.

## Controllers

Do **not** pass `-input` to `x11_vaapi`:

- `src/platform.h` defines `IS_EMBEDDED(system)` as `system != SDL`.
- `src/main.c` initializes evdev and calls `udev_init(!inputAdded, ...)` for
  embedded systems, including `X11_VAAPI`.
- With no `-input`, `inputAdded` remains false (`src/config.c`).
  `src/input/udev.c` enumerates `/dev/input/event*` and watches new devices.
  VA-API thus discovers controllers without explicit seats.
- `src/input/x11.c` itself handles keyboard/mouse, not controllers.
- If VA-API fails, the SDL branch discovers controllers itself and rejects any
  explicit `-input` with a fatal error.

Omitting `-input` keeps both paths interactive. Like the existing SDL `x11`
path, discovery is not limited to the occupied seat list. An empty occupied
seat list still adds `-viewonly`. ARM platforms keep explicit seats. Upstream
X11 setup installs its keyboard/mouse handler unconditionally; unlike gamepad
setup, that handler is not guarded by `viewonly`. Check keyboard/mouse
watch-only behavior separately on the target.

## Sandbox

`host/viewer_units.rs` uses `SystemdLaunchUnitBackend::sandbox_properties`.
It does not set `PrivateDevices`, a device allowlist, a new root, or an
inaccessible path covering `/dev/dri` or `/run/opengl-driver`. The Nix store
also stays visible, so the driver symlink's target can load. The packaged
libva search path includes `/run/opengl-driver/lib/dri`; no driver-path override
or permission expansion is needed.

`services/inputd/nix/korri-linux-host-core.nix` enables `hardware.graphics`
and gives the runtime user the `render` and `video` supplementary groups.
The viewer runs as that runtime user. The Intel target's driver is
`/run/opengl-driver/lib/dri/iHD_drv_video.so`; a sandbox regression test guards
visibility of this path and `/dev/dri/renderD128`.

## Source-level fallback check

A standalone C probe compiled the supplied upstream `src/platform.c`,
`src/video/x11.c` and `src/video/ffmpeg_vaapi.c`, with `HAVE_X11`, `HAVE_SDL`
and `HAVE_VAAPI`. Function-section garbage collection let the probe call the
real `platform_check("x11_vaapi")` without linking the rest of Moonlight.
The same probe passed again after applying all shipped Moonlight patches.
With Xvfb on `:94` and no usable VA display on the VA-API initializer's fixed
`:0`, the assertion `selected == SDL` passed. Its output was:

```text
[VAAPI @ ...] No VA display found for device :0.
x11_vaapi with unavailable VA-API: SDL2 (software decoding)
```

This checks the real initialization-failure branch, not streamed playback.
The Rust argv regression separately checks that no `-input` can make that SDL
fallback exit, and that an interactive viewer does not get `-viewonly`.

## Target acceptance (parent, before landing)

No deployment is part of this change. On ibuki, verify the new viewer command
uses `-platform x11_vaapi`, the log reports `X Window System (VAAPI)`, video
engine activity increases, and controller input reaches aka. Check fullscreen
with stream dimensions different from the output: upstream's X11 renderer
keeps its initial drawing dimensions rather than handling window resize events.
Force an invalid libva driver in a scoped test to check SDL software playback
and controllers. The absence of `-input` is required for that forced-failure
test.
