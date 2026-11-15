---
status: accepted
---

# The shell draws with the GPU on every device

**Context, 2026-11-14 (issue #482).** On ibuki (Intel i3-8109U, a TV at
3840×2160) the owner found Korri's UI "incredibly" slow. `korri-shell bench`
on ibuki measured one full-screen Velvet frame at 277 to 316 ms with the
shell's CPU renderer, about 3 frames a second. Velvet redraws the whole
window on every focus move, because the light behind its content follows the
focus.

The Linux shell drew only on the CPU. The choice came from its first target,
the RG35XX SP (640×480, four Cortex-A53 cores), where a full frame costs
about 11 ms, and from the spike it grew from (`85fa8cc1f`). No record
rejected the GPU; nobody had measured a large screen. The Android shell
already draws with Skia on the GPU.

**Owner, 2026-11-14:** "Every single device should use gpu for rendering.
Focus on ibuki for now."

## Decision

- The shell draws every window with a GPU renderer on every device. The CPU
  renderer stays only where no display exists: the headless backend, the
  tests, the bench and the screenshot tools.
- On Linux the winit backend uses Slint's FemtoVG renderer on OpenGL (EGL on
  Wayland, GLX on X11). Android keeps Skia.
- The Linux unit keeps `PrivateDevices=true`. It binds only the compositor's
  selected render node into its private `/dev/dri/renderD128`, grants read/write
  device-cgroup access to that node, and adds only the `render` supplementary
  group. DRM primary nodes and `/dev/input` stay hidden.
- Issue #484 removes the staged migration from #482. There is no
  `services.korri.shell.renderer` option and no `--renderer` argument. Winit's
  software adapter is not compiled. Headless tools use the software renderer
  as a direct dependency, without forwarding its feature to the backend
  selector. A GPU setup error cannot select the software adapter.
- The separate webOS SDL adapter is outside this Linux winit cut; it keeps
  its software path until a measured webOS GLES integration exists.

## Consequences

- The patched CPU renderer (`clients/shell/vendor/README.md`: layer caching,
  drawing on several threads) no longer draws Linux device windows.
  It stays for the tests and the bench until they measure the GPU path too.
- Text and edges come from FemtoVG, not the CPU rasteriser, so a device's
  pixels differ slightly from the headless screenshots.
- The shell's package carries libglvnd and finds the system's Mesa driver at
  run time; a device needs a working OpenGL ES 2 or newer driver.
- FemtoVG over Skia on Linux is an agent's choice (2026-11-14): FemtoVG
  builds from crates in the Nix sandbox as it is. Skia needs prebuilt
  binaries downloaded during the build, or a Skia source build, for every
  architecture.
