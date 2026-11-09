# Graphics/video-plane presentation

## Production presentation and host fact

The surface treaty does **not** report whether this device is presenting a
stream. At base `d4913c15f`, `contracts/surface/src/lib.rs:584–586` defines
Running as a live launch (not a viewer) and Paused as a remote holder pause
that hides the stream. The entire `SurfaceModel` at lines 1344–1400 has no
viewer/session field. `services/korrid/src/surface/model.rs:180–212` reduces
NowPlaying to a Continue card with `resumable`; lines 545–569 return Browsing
in Ready, even with a live session. Those cards can also be outside the
current catalog window. Neither card text nor status is a viewer signal.

Linux uses a separate viewer window.
`services/korrid/src/linux_viewer/production.rs::observe` confirms its unit
and focused window; Home backgrounds that viewer and focuses the shell.
The existing brief, “Stream end, Home and Linux’s overlay”, describes the
exact-launch freeze/background/resume path.
`services/korrid/src/lib.rs:5539` (“the shell is paused behind it”) explains
why the far-session watcher runs independently. It is not a surface-model
fact or a native-shell draw-suppression signal.
`clients/shell/src/backend/winit.rs::attach` passes window focus events;
`shell.rs::follow_focus` disables native input when shell windows lose focus.
The SDL backend separately pauses drawing on window-hidden events. None of
these can represent video beneath a foreground webOS window.

**Parent decision, 2026-11-08:** keep the new fact host-only, outside the
surface treaty. The existing authenticated watch now carries
`{"kind":"localViewer","presenting":true|false}` independently of model
ACKs. Every attach/reconnect sends the current value, initially false. The
model, package logic and generated contracts do not receive this fact.

The edge API is:

```rust
// services/korrid/src/surface_host_input.rs
SurfaceHostInputSource::set_local_viewer_presenting(&self, presenting: bool)
```

`EmbeddedPlatform.native_input` already receives
`Arc<SurfaceHostInputSource>`. The webOS viewer edge can retain that Arc. Call
the setter with true only after the viewer READY byte; call it with false on
exit, stop or disconnect. The source stores a single Tokio watch value and
clears it on source drop. This change supplies the holder and transport, not
the viewer lifecycle caller. Input producer reset is not viewer exit.

| Host fact | Existing presentation | SDL behavior |
| --- | --- | --- |
| false | Either | Normal full-texture copy |
| true | Catalog | Clear RGBA (0,0,0,0), clear opaque region, present; no Slint render or texture upload |
| true | GameplayOverlay | Clear RGBA (0,0,0,0), clear opaque region, render/copy package pixels |

The same SDL window stays in front. The adapter requests framebuffer alpha
at creation because readiness can arrive later. It clears the opaque region
before every transparent presentation, including after resize. Returning from
video invalidates the full Slint buffer and renders the latest view. While
video alone is visible, surface animations do not force a 16 ms render loop;
input, host jobs and timers still run. Desktop backends keep their existing
behavior. Zone and Pico are unchanged; their opaque pixels will still cover
video in GameplayOverlay until their backgrounds are made transparent by
surface work.

`CARGO_BUILD_JOBS=2 nix run .#shell-webos-input-check` includes real SDL RGBA
readback for all three modes and repeated transitions, with transparent holes,
partial uploads, odd/even/minimum sizes, and a real Slint test proving no
surface renders while presenting and a full latest-view repaint on return.
The readback target has alpha under Xvfb; it is not evidence about the TV
compositor. Embedded korrid tests exercise the public setter through an
authenticated watch, including updates during an outstanding model ACK and
current-state resend after reconnect. Shell tests exercise the host message
parser and watch-to-backend decisions, including overlay and retired watches.

### TV acceptance with the existing NDL media test

After installing the pinned IPK as described in README.md, leave
`KORRI_SHELL_SEE_THROUGH` unset. The integrated viewer edge (or the NDL test
composition using the API above) must call true on READY and false after
confirmed cleanup; an unrelated NDL test process does not itself update this
fact. Play the same known H.264 media while the shell stays foreground:

1. Before READY, Zone draws normally. After READY, the entire video must be
   visible, not only its left half. Check the existing alpha-bit/opaque-region
   startup log. The shell must retain its window and input connection.
2. End the test, confirm false, and check that the same shell process draws
   the latest Zone view again. Repeat start/stop, including viewer failure.
3. Exercise the existing GameplayOverlay presentation once the edge can
   request it. Package pixels must draw above video; an opaque Zone/Pico
   background is a known surface limitation, not an alpha failure. A package
   with transparent overlay pixels is needed to accept simultaneous video/UI.
4. Compare idle/during-video shell CPU. Workstation tests prove skipped
   render/upload, not a measured TV CPU reduction.

The legacy split-screen diagnostic below remains available without a viewer
edge. `KORRI_SHELL_SEE_THROUGH=1` affects normal Surface mode only; live
Stream/GameplayOverlay modes take precedence. No TV access was used here.

## Split-screen diagnostic

`KORRI_SHELL_SEE_THROUGH=1` enables a diagnostic in `window-sdl` only.
The split is off for every other value, including unset. No surface package changes.
It does not start a decoder or stream. The parent supplies the existing NDL
local-video experiment and owns all TV operations.

## Verified by source and captured-library inspection

| Stage | Current normal path and alpha consequences |
| --- | --- |
| Window | `window-sdl/src/lib.rs::open_screen` creates one shown `SDL_WINDOW_FULLSCREEN_DESKTOP` window (or a resizable workstation window). It requests 8 alpha bits for future viewer presentation and sets no transparent-window property. Window/display pixel format is not evidence of the EGL framebuffer's alpha bits. |
| Renderer | `open_screen` asks for an accelerated renderer, then any renderer on failure. Existing TV evidence names GLES2 on LG's system SDL 2.0.4. `ffi.rs::SDL_PIXELFORMAT_ABGR8888` describes the **texture**, not a proven ARGB compositor buffer. |
| Slint pixels | Vendored `clients/shell/vendor/i-slint-renderer-software/draw_functions.rs:1027–1050,1090–1106` defines four ordered RGBA bytes, preserves alpha when premultiplying, blends alpha, and overrides `TargetPixel::background()` with transparent black. It does not globally flatten RGBA output to opaque black. |
| Slint → SDL | `make_texture` creates a streaming ABGR8888 texture. On little-endian ARM its bytes are R,G,B,A, matching `PremultipliedRgbaColor`. `upload` passes those bytes directly to `SDL_UpdateTexture`, including partial rectangles and full-row pitch. Nothing forces alpha to 255. `SDL_BLENDMODE_NONE` disables blending; it does not discard source alpha. `render` copies the full texture with no clear in the normal path. Zone's opaque background supplies opaque pixels. |
| SDL GLES2 | [2.0.4 `GLES2_RenderCopy`][gles] chooses the ABGR shader for this texture. [`GLES2_FragmentSrc_TextureABGRSrc_`][shader] samples all four channels; `SDL_BLENDMODE_NONE` selects that shader and disables blending. There is no forced alpha=1 for ABGR input. RGB/BGR texture shaders are different. |
| EGL choice | [Upstream SDL 2.0.4 `SDL_GL_ResetAttributes`][video] defaults `alpha_size` to **0**. This is a request, not proof that LG selected a zero-alpha config: EGL can supply more bits. The adapter originally had no actual framebuffer-alpha measurement. The composition path explicitly requests 8 before creating the window/renderer and logs the SDL window pixel-format name and `SDL_GL_GetAttribute(SDL_GL_ALPHA_SIZE)` after creation. An actual zero-bit result refuses startup. An unavailable measurement is logged as unavailable, not success. |
| Wayland opaque region | [Upstream 2.0.4 `Wayland_CreateWindow`, `handle_configure`, `Wayland_SetWindowSize`][wayland] declare the whole surface opaque. Thus alpha-capable pixels alone need not show through. LG's fork must not be assumed identical. The transparent composition path explicitly sends core `wl_surface.set_opaque_region(NULL)` before each presentation, including after configure/resize. |

The captured B7 library is
`~/.cache/research/korri-webos-m1/evidence/tv-libSDL2-2.0.so.0.4.0`, SHA-256
`0fbeb44edaa62de9ae9214ac0449250f4e3fb2f6b280db2e68d9a7f7cd6015ce`.
All 36 required SDL functions occur in its ELF dynamic symbol table and the
upstream 2.0.4 dynapi export list. Its Thumb routine at `0x81164–0x81182` writes
subsystem 6 at `SDL_SysWMinfo+4`,
`wl_display*` at +8 and `wl_surface*` at +12. This confirms the two Wayland
union fields used here on ARM32, beyond the upstream 2.0.4 header. The existing
cursor adapter already uses the first field and the same Wayland client SONAME.

**Parent-reported B7 result, 2026-11-08:** GLES2 reported 8 alpha bits,
the opaque-region clear worked, and a TV display capture showed the NDL test
pattern through the left half with Zone in the right. This verifies graphics
alpha above the hardware video plane. The production full-window transitions
still need TV acceptance. This branch was rebased onto `93871cfd9`, including
the native launcher fixes through `e80d3a96f`, before packaging. The captured library has no string for
`_WEBOS_WINDOW_TYPE`, `SDL_VIDEO_EGL_ALLOW_TRANSPARENCY`, or a transparent hint.
Absence of a string does not prove a property is unsupported or unnecessary.
No vendor window-type, power, registration, focus or fullscreen policy changes
are made. The newer [SDL-webOS transparency hint][modern] clears the opaque
region, but that implementation is not the TV's 2.0.4 and is not bundled here.

Moonlight's pinned [`lv_app_redraw_now`][moonlight] clears its target with
`SDL_SetRenderDrawColor(renderer, 0, 0, 0, 0)` / `SDL_RenderClear` when there is
no background renderer, then copies its UI texture. That is source evidence for
the transparent-clear technique, not proof that Korri's system SDL has the
same framebuffer as Moonlight's bundled SDL.

## Diagnostic behavior

`window-sdl/src/see_through.rs` requests framebuffer alpha, clears the Wayland
opaque region on SDL's **existing** surface, and clears the target to RGBA
(0,0,0,0). It copies only the right half of the existing streaming texture with
identical source/destination rectangles. The left `floor(width/2)` columns
therefore stay transparent black, while the right half retains the original
Slint RGBA. At 1920×1080 the split is x=960. No new pixel buffer, window or host
is created. Slint's reused buffer and dirty-rectangle uploads stay intact.
Copying without blending avoids applying straight-alpha multiplication to
Slint's already-premultiplied RGB. The diagnostic checks clear/copy errors.
With the variable unset and no viewer presenting, there is no transparent
clear, clipping or opaque-region request. Framebuffer alpha is still requested
at startup; the normal full-target copy remains unchanged.

`CARGO_BUILD_JOBS=2 nix run .#shell-webos-input-check` runs the focused checks.
Workstation tests check exact env gating, even/odd/minimum dimensions, and real
SDL RGBA render-target readback before/after partial texture uploads. Readback
uses an alpha-capable texture target under Xvfb; it does **not** prove an
alpha-capable desktop window or LG compositor acceptance.

## Parent's TV check

1. Install the source-pinned IPK. The native `bin/korri-webos` launcher
   inherits `KORRI_SHELL_SEE_THROUGH`. In the existing approved app-UID/jail
   launch context, supply `KORRI_SHELL_SEE_THROUGH=1` when starting that
   launcher. Do not edit or recreate the removed `launch.sh`; do not change
   the app executable identity or foreground lifecycle.

2. Stop and launch the app with that environment. Inspect `/tmp/korri-shell-launch.log` in
   the app jail. Diagnostic messages use stderr so the launcher captures them. Look for `sdl: see-through: framebuffer alpha bits 8
   (requested 8)` (or another positive count), the Wayland opaque-region line,
   and the left-half-enabled line. `unavailable` is not proof of alpha. A
   zero-bit result or startup error is a failed diagnostic, not a hidden video.
3. With the same approved NDL test video playing beneath this shell, observe
   video in the left half and unchanged shell graphics in the right half.
   Use existing shell navigation to prove the right half keeps updating.
   Record the visible result, source SHA, dimensions and logs. No second SDL
   window or surface redesign is needed.
4. Remove the export (or set it to `0`) and restart to restore normal rendering.

If this fails despite positive alpha bits and an empty opaque region, capture
scoped `WAYLAND_DEBUG=1` output and the selected EGL config on the TV before
choosing a vendor property. If no alpha-capable EGL config exists, alpha cannot
reach the compositor through this renderer. That must be measured, not inferred
from a black screenshot. Do not replace system SDL or add another window as an
automatic fallback. A documented LG transparent-surface property would be the
next small experiment only if evidence identifies one.

[gles]: https://github.com/libsdl-org/SDL/blob/release-2.0.4/src/render/opengles2/SDL_render_gles2.c#L1545-L1640
[shader]: https://github.com/libsdl-org/SDL/blob/release-2.0.4/src/render/opengles2/SDL_shaders_gles2.c#L63-L76
[video]: https://github.com/libsdl-org/SDL/blob/release-2.0.4/src/video/SDL_video.c#L2740-L2775
[wayland]: https://github.com/libsdl-org/SDL/blob/release-2.0.4/src/video/wayland/SDL_waylandwindow.c
[modern]: https://github.com/webosbrew/SDL-webOS/blob/release-2.30.12-webos.5/src/video/wayland/SDL_waylandwindow.c#L2371-L2404
[moonlight]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/lvgl/lv_disp_drv_app.c#L69-L88
