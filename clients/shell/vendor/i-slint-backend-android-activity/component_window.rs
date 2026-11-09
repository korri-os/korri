//! Component and redraw lifecycle shared by host-provided Surface renderers.
//! No Android or graphics-context ownership belongs here.

use i_slint_core::api::{PhysicalSize, Window};
use i_slint_core::platform::{Renderer, WindowAdapter, WindowEvent};
use std::cell::Cell;
use std::rc::Rc;

pub struct ComponentWindow<R> {
    window: Window,
    renderer: R,
    size: Cell<PhysicalSize>,
    pending_redraw: Cell<bool>,
}

impl<R: Renderer + 'static> ComponentWindow<R> {
    pub fn new(renderer: R, size: PhysicalSize) -> Rc<Self> {
        Rc::<Self>::new_cyclic(|weak| Self {
            window: Window::new(weak.clone() as std::rc::Weak<dyn WindowAdapter>),
            renderer,
            size: Cell::new(size),
            pending_redraw: Cell::new(true),
        })
    }

    pub fn renderer(&self) -> &R {
        &self.renderer
    }

    pub fn render_if_needed<E>(&self, render: impl FnOnce(&R) -> Result<(), E>) -> Result<bool, E> {
        // Surface attachment/resize can precede the asynchronous component.
        // show() associates the renderer with that component. Do not consume
        // its first redraw (or render animations) until that has happened.
        if !self.window.is_visible() {
            return Ok(false);
        }
        let animating = self.window.has_active_animations();
        if !self.pending_redraw.replace(false) && !animating {
            return Ok(false);
        }
        render(&self.renderer)?;
        Ok(true)
    }

    pub fn resize(&self, size: PhysicalSize, scale_factor: f32) {
        self.size.set(size);
        self.window
            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor });
        self.window.dispatch_event(WindowEvent::Resized {
            size: size.to_logical(scale_factor),
        });
        self.pending_redraw.set(true);
    }
}

impl<R: Renderer + 'static> WindowAdapter for ComponentWindow<R> {
    fn window(&self) -> &Window {
        &self.window
    }
    fn size(&self) -> PhysicalSize {
        self.size.get()
    }
    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }
    fn request_redraw(&self) {
        self.pending_redraw.set(true);
    }
}
