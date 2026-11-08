//! Opt-in plane-composition probe, not a surface presentation policy.
//!
//! Keep Slint's reused buffer and streaming texture intact. Clearing the
//! target and copying only its right half leaves exact transparent black on
//! the left, without corrupting pixels needed by a later partial repaint.

use std::ffi::{c_int, c_void, OsStr};

use crate::ffi::{self, Sdl};

type Marshal = unsafe extern "C" fn(*mut c_void, u32, ...);

pub fn enabled(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("1"))
}

/// Run after SDL_Init, before creating either the window or its renderer.
pub fn request_alpha(sdl: &Sdl) -> Result<(), String> {
    // SAFETY: SDL video is initialized; SDL_GL_ALPHA_SIZE exists in 2.0.4.
    if unsafe { (sdl.SDL_GL_SetAttribute)(ffi::SDL_GL_ALPHA_SIZE, 8) } != 0 {
        return Err(format!(
            "see-through: requesting 8 alpha bits failed: {}",
            sdl.error()
        ));
    }
    Ok(())
}

pub struct Diagnostic {
    surface: *mut c_void,
    marshal: Option<Marshal>,
}

impl Diagnostic {
    pub fn open(sdl: &Sdl, window: *mut ffi::Window, gl_renderer: bool) -> Result<Self, String> {
        // SAFETY: live SDL window; the pixel-format name is static.
        let format = unsafe {
            ffi::text((sdl.SDL_GetPixelFormatName)(
                (sdl.SDL_GetWindowPixelFormat)(window),
            ))
        };
        eprintln!("sdl: see-through: SDL window format {format}; texture ABGR8888 (RGBA bytes)");
        let mut alpha: c_int = 0;
        // SAFETY: live renderer/context on the SDL event-loop thread.
        if !gl_renderer {
            eprintln!("sdl: see-through: framebuffer alpha bits unavailable (not a GL renderer)");
        } else if unsafe { (sdl.SDL_GL_GetAttribute)(ffi::SDL_GL_ALPHA_SIZE, &mut alpha) } == 0 {
            eprintln!("sdl: see-through: framebuffer alpha bits {alpha} (requested 8)");
            if alpha == 0 {
                return Err("see-through: framebuffer has no alpha channel".into());
            }
        } else {
            // Do not mistake a missing measurement for proof of an
            // alpha-capable window.
            eprintln!(
                "sdl: see-through: framebuffer alpha bits unavailable: {}",
                sdl.error()
            );
        }
        let mut diagnostic = Self {
            surface: std::ptr::null_mut(),
            marshal: None,
        };
        // SAFETY: SDL video is initialized; returns a static driver name.
        let wayland = unsafe { ffi::text((sdl.SDL_GetCurrentVideoDriver)()) } == "wayland";
        if wayland {
            let info = crate::wayland_window_info(sdl, window)
                .ok_or("see-through: SDL returned no Wayland window info")?;
            // SDL_SysWMinfo 2.0.4's Wayland union: display, surface,
            // shell_surface. Use SDL's connection, never a second window.
            diagnostic.surface = info.info[1];
            if diagnostic.surface.is_null() {
                return Err("see-through: SDL returned no Wayland surface".into());
            }
            // SAFETY: load the same client SONAME SDL uses. Keep it loaded
            // for the process lifetime, as the other run-time bindings do.
            unsafe {
                let client = libc::dlopen(
                    c"libwayland-client.so.0".as_ptr(),
                    libc::RTLD_NOW | libc::RTLD_LOCAL,
                );
                if client.is_null() {
                    return Err("see-through: libwayland-client.so.0 unavailable".into());
                }
                let marshal = libc::dlsym(client, c"wl_proxy_marshal".as_ptr());
                if marshal.is_null() {
                    return Err("see-through: wl_proxy_marshal unavailable".into());
                }
                diagnostic.marshal = Some(std::mem::transmute::<*mut c_void, Marshal>(marshal));
            }
            eprintln!("sdl: see-through: clearing Wayland opaque region before each present");
        }
        eprintln!(
            "sdl: see-through enabled: left floor(width/2) columns transparent; right unchanged"
        );
        Ok(diagnostic)
    }

    pub fn copy(
        &self,
        sdl: &Sdl,
        renderer: *mut ffi::Renderer,
        texture: *mut ffi::Texture,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        if let Some(marshal) = self.marshal {
            // SAFETY: live SDL-owned wl_surface on SDL's thread. Core
            // wl_surface v1 opcode 4 is set_opaque_region(?object). NULL
            // means no opaque pixels. Repeat because SDL 2.0.4 can reset it
            // on resize/configure. SDL's next EGL swap commits this state.
            unsafe { marshal(self.surface, 4, std::ptr::null_mut::<c_void>()) };
        }
        // SDL_BLENDMODE_NONE on the texture is intentional: it replaces
        // destination RGBA, retaining Slint's premultiplied alpha exactly.
        // SDL 2.0.4's ordinary BLEND would multiply premultiplied RGB again.
        let rect = right_half(width, height);
        // SAFETY: live SDL objects; both rectangles are in texture/output bounds.
        unsafe {
            if (sdl.SDL_SetRenderDrawColor)(renderer, 0, 0, 0, 0) != 0
                || (sdl.SDL_RenderClear)(renderer) != 0
                || (sdl.SDL_RenderCopy)(renderer, texture, &rect, &rect) != 0
            {
                return Err(format!("see-through: clear/copy failed: {}", sdl.error()));
            }
        }
        Ok(())
    }
}

fn right_half(width: u32, height: u32) -> ffi::Rect {
    ffi::Rect {
        x: (width / 2) as c_int,
        y: 0,
        w: (width - width / 2) as c_int,
        h: height as c_int,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_one_enables_the_diagnostic() {
        assert!(enabled(Some(OsStr::new("1"))));
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("true"),
            Some("01"),
            Some("1 "),
        ] {
            assert!(!enabled(value.map(OsStr::new)));
        }
    }

    #[test]
    fn test_region_tracks_even_odd_and_minimum_output_sizes() {
        for width in [1, 2, 3, 64, 1920, 1921] {
            let rect = right_half(width, 1080);
            assert_eq!(rect.x, (width / 2) as c_int);
            assert_eq!(rect.x + rect.w, width as c_int);
            assert_eq!((rect.y, rect.h), (0, 1080));
            assert!(rect.w > 0);
        }
    }
}
