//! A Slint window backed by any host-provided Android Surface.
//! The host must wait for `release()` before destroying that Surface.

use crate::component_window::ComponentWindow;
use crate::ndk::native_window::NativeWindow;
use i_slint_core::api::{PhysicalSize, Window};
use i_slint_core::platform::WindowAdapter;
use i_slint_renderer_skia::{SkiaRenderer, SkiaSharedContext};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct SurfaceWindow {
    adapter: Rc<ComponentWindow<SkiaRenderer>>,
    native: RefCell<Option<NativeWindow>>,
}

impl SurfaceWindow {
    /// # Safety
    /// `env` and `surface` are a live JNIEnv and android.view.Surface.
    pub unsafe fn from_surface(
        env: *mut core::ffi::c_void,
        surface: *mut core::ffi::c_void,
        scale_factor: f32,
    ) -> Result<Rc<Self>, String> {
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return Err("Surface density must be positive and finite".into());
        }
        let native = unsafe { NativeWindow::from_surface(env.cast(), surface.cast()) }
            .ok_or("ANativeWindow_fromSurface failed")?;
        if native.width() <= 0 || native.height() <= 0 {
            return Err("Surface must have a nonempty size".into());
        }
        let size = PhysicalSize {
            width: native.width() as u32,
            height: native.height() as u32,
        };
        let adapter = ComponentWindow::new(
            SkiaRenderer::default_opengl(&SkiaSharedContext::default()),
            size,
        );
        let surface = i_slint_renderer_skia::opengl_surface::OpenGLSurface::new_with_config(
            Arc::new(native.clone()),
            Arc::new(crate::androidwindowadapter::DummyDisplayHandle),
            size,
            None,
            glutin::config::ConfigTemplateBuilder::new().with_alpha_size(8),
            Some(&|config: &glutin::config::Config| {
                use glutin::config::GlConfig;
                config.alpha_size() >= 8
            }),
        )
        .map_err(|e| format!("host EGL surface: {e}"))?;
        adapter.renderer().set_surface(Box::new(surface));
        adapter.resize(size, scale_factor);
        Ok(Rc::new(Self {
            adapter,
            native: RefCell::new(Some(native)),
        }))
    }

    /// Draw when Slint asked for a frame or an animation runs. True when drawn.
    pub fn render_if_needed(&self) -> Result<bool, i_slint_core::platform::PlatformError> {
        if self.native.borrow().is_none() {
            return Ok(false);
        }
        self.adapter
            .render_if_needed(|renderer| renderer.render().map(|_| ()))
    }

    /// Release the EGL surface and the ANativeWindow before Java destroys the Surface.
    pub fn release(&self) -> Result<(), i_slint_core::platform::PlatformError> {
        // Drop Slint's shown-component reference before releasing its Surface.
        // This window is independent of the host's video/game window.
        self.adapter.window().hide()?;
        self.adapter.renderer().suspend()?;
        self.native.borrow_mut().take();
        Ok(())
    }

    pub fn resize(&self, scale_factor: f32) {
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return;
        }
        let Some(native) = self.native.borrow().as_ref().cloned() else {
            return;
        };
        if native.width() <= 0 || native.height() <= 0 {
            return;
        }
        let size = PhysicalSize {
            width: native.width() as u32,
            height: native.height() as u32,
        };
        self.adapter.resize(size, scale_factor);
    }

    pub fn window(&self) -> &Window {
        self.adapter.window()
    }
}
