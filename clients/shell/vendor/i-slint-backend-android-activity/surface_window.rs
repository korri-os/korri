//! A Slint window backed by any host-provided Android Surface.
//! The host must wait for `release()` before destroying that Surface.

use crate::ndk::native_window::NativeWindow;
use i_slint_core::api::{PhysicalSize, Window};
use i_slint_core::platform::{WindowAdapter, WindowEvent};
use i_slint_renderer_skia::{SkiaRenderer, SkiaSharedContext};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

pub struct SurfaceWindow {
    window: Window,
    renderer: SkiaRenderer,
    size: Cell<PhysicalSize>,
    pending_redraw: Cell<bool>,
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
        let adapter = Rc::<Self>::new_cyclic(|w| Self {
            window: Window::new(w.clone() as std::rc::Weak<dyn WindowAdapter>),
            renderer: SkiaRenderer::default_opengl(&SkiaSharedContext::default()),
            size: Cell::new(size),
            pending_redraw: Cell::new(true),
            native: RefCell::new(Some(native.clone())),
        });
        let surface = i_slint_renderer_skia::opengl_surface::OpenGLSurface::new_with_config(
            Arc::new(native),
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
        adapter.renderer.set_surface(Box::new(surface));
        adapter
            .window
            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor });
        adapter.window.dispatch_event(WindowEvent::Resized {
            size: size.to_logical(scale_factor),
        });
        Ok(adapter)
    }

    /// Draw when Slint asked for a frame or an animation runs. True when drawn.
    pub fn render_if_needed(&self) -> Result<bool, i_slint_core::platform::PlatformError> {
        if self.native.borrow().is_none() {
            return Ok(false);
        }
        let animating = self.window.has_active_animations();
        if !self.pending_redraw.replace(false) && !animating {
            return Ok(false);
        }
        let _ = self.renderer.render()?;
        Ok(true)
    }

    /// Release the EGL surface and the ANativeWindow before Java destroys the Surface.
    pub fn release(&self) -> Result<(), i_slint_core::platform::PlatformError> {
        self.renderer.suspend()?;
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
        self.size.set(size);
        self.window
            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor });
        self.window.dispatch_event(WindowEvent::Resized {
            size: size.to_logical(scale_factor),
        });
        self.pending_redraw.set(true);
    }

    pub fn window(&self) -> &Window {
        &self.window
    }
}

impl WindowAdapter for SurfaceWindow {
    fn window(&self) -> &Window {
        &self.window
    }
    fn size(&self) -> PhysicalSize {
        self.size.get()
    }
    fn renderer(&self) -> &dyn i_slint_core::platform::Renderer {
        &self.renderer
    }
    fn request_redraw(&self) {
        self.pending_redraw.set(true);
    }
}
