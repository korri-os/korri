# Graphics/video-plane diagnostic

`KORRI_SHELL_SEE_THROUGH=1` enables a diagnostic in `window-sdl` only.
It is off for every other value, including unset. No surface package changes.
It does not start a decoder or stream. The parent supplies the existing NDL
local-video experiment and owns all TV operations.

## Verified by source and captured-library inspection

| Stage | Current normal path and alpha consequences |
| --- | --- |
| Window | `window-sdl/src/lib.rs::open_screen` creates one shown `SDL_WINDOW_FULLSCREEN_DESKTOP` window (or a resizable workstation window). It requests no alpha-channel size and sets no transparent-window property. Window/display pixel format is not evidence of the EGL framebuffer's alpha bits. |
| Renderer | `open_screen` asks for an accelerated renderer, then any renderer on failure. Existing TV evidence names GLES2 on LG's system SDL 2.0.4. `ffi.rs::SDL_PIXELFORMAT_ABGR8888` describes the **texture**, not a proven ARGB compositor buffer. |
| Slint pixels | Vendored `clients/shell/vendor/i-slint-renderer-software/draw_functions.rs:1027–1050,1090–1106` defines four ordered RGBA bytes, preserves alpha when premultiplying, blends alpha, and overrides `TargetPixel::background()` with transparent black. It does not globally flatten RGBA output to opaque black. |
| Slint → SDL | `make_texture` creates a streaming ABGR8888 texture. On little-endian ARM its bytes are R,G,B,A, matching `PremultipliedRgbaColor`. `upload` passes those bytes directly to `SDL_UpdateTexture`, including partial rectangles and full-row pitch. Nothing forces alpha to 255. `SDL_BLENDMODE_NONE` disables blending; it does not discard source alpha. `render` copies the full texture with no clear in the normal path. Zone's opaque background supplies opaque pixels. |
| SDL GLES2 | [2.0.4 `GLES2_RenderCopy`][gles] chooses the ABGR shader for this texture. [`GLES2_FragmentSrc_TextureABGRSrc_`][shader] samples all four channels; `SDL_BLENDMODE_NONE` selects that shader and disables blending. There is no forced alpha=1 for ABGR input. RGB/BGR texture shaders are different. |
| EGL choice | [Upstream SDL 2.0.4 `SDL_GL_ResetAttributes`][video] defaults `alpha_size` to **0**. This is a request, not proof that LG selected a zero-alpha config: EGL can supply more bits. The adapter had no actual framebuffer-alpha measurement. The diagnostic explicitly requests 8 before creating the window/renderer and logs the SDL window pixel-format name and `SDL_GL_GetAttribute(SDL_GL_ALPHA_SIZE)` after creation. An actual zero-bit result refuses startup. An unavailable measurement is logged as unavailable, not success. |
| Wayland opaque region | [Upstream 2.0.4 `Wayland_CreateWindow`, `handle_configure`, `Wayland_SetWindowSize`][wayland] declare the whole surface opaque. Thus alpha-capable pixels alone need not show through. LG's fork must not be assumed identical. The diagnostic explicitly sends core `wl_surface.set_opaque_region(NULL)` before each presentation, including after configure/resize. |

The captured B7 library is
`~/.cache/research/korri-webos-m1/evidence/tv-libSDL2-2.0.so.0.4.0`, SHA-256
`0fbeb44edaa62de9ae9214ac0449250f4e3fb2f6b280db2e68d9a7f7cd6015ce`.
All 36 required SDL functions occur in its ELF dynamic symbol table and the
upstream 2.0.4 dynapi export list. Its Thumb routine at `0x81164–0x81182` writes
subsystem 6 at `SDL_SysWMinfo+4`,
`wl_display*` at +8 and `wl_surface*` at +12. This confirms the two Wayland
union fields used here on ARM32, beyond the upstream 2.0.4 header. The existing
cursor adapter already uses the first field and the same Wayland client SONAME.

**Not established off-device:** whether the actual LG EGL config supplies alpha,
whether LG's compositor honors it above NDL, or whether an additional vendor
property is required. The captured library has no string for
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
With the variable unset, no alpha request, clear, clipping or Wayland request
is made; the normal full-target copy is unchanged.

`CARGO_BUILD_JOBS=2 nix run .#shell-webos-input-check` runs the focused checks.
Workstation tests check exact env gating, even/odd/minimum dimensions, and real
SDL RGBA render-target readback before/after partial texture uploads. Readback
uses an alpha-capable texture target under Xvfb; it does **not** prove an
alpha-capable desktop window or LG compositor acceptance.

## Parent's TV check

1. Install the source-pinned IPK. In the installed app's `launch.sh`, add this
   before the shell is spawned:

   ```sh
   export KORRI_SHELL_SEE_THROUGH=1
   ```

2. Stop and launch the app normally. Inspect `/tmp/korri-shell-launch.log` in
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
