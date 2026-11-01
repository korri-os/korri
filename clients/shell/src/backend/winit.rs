//! Slint's winit backend: Wayland on the handheld (sway), X11 for
//! workstation proofs. Software renderer only. Each component is its own
//! window, so a switch shows the new window before the old one goes.

use super::{Backend, FrameCounter, HostEvent, Placement};
use crate::input::Key;
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use std::{cell::Cell, rc::Rc};

pub struct Winit;

impl Winit {
    /// Select the software renderer before the first window exists.
    pub fn install() -> Result<Self, slint::PlatformError> {
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name("software".into())
            .select()?;
        Ok(Self)
    }
}

fn key_of(code: winit::keyboard::KeyCode) -> Option<Key> {
    use winit::keyboard::KeyCode as K;
    Some(match code {
        K::ArrowUp => Key::Up,
        K::ArrowDown => Key::Down,
        K::ArrowLeft => Key::Left,
        K::ArrowRight => Key::Right,
        K::Enter | K::NumpadEnter => Key::Return,
        K::Escape => Key::Escape,
        K::Backspace => Key::Backspace,
        K::KeyM => Key::M,
        K::KeyO => Key::O,
        K::KeyS => Key::S,
        K::F2 => Key::F2,
        _ => return None,
    })
}

impl Backend for Winit {
    fn shared_window(&self) -> bool {
        false
    }

    fn attach(&self, window: &slint::Window, sink: Rc<dyn Fn(HostEvent)>, frames: FrameCounter) {
        use winit::event::{ElementState, WindowEvent};
        let first = Cell::new(true);
        window.on_winit_window_event(move |_, event| {
            match event {
                WindowEvent::RedrawRequested => {
                    frames.add();
                    if first.replace(false) {
                        sink(HostEvent::FirstFrame);
                    }
                }
                // X11 replays held keys as synthetic presses when a new
                // window gains focus; a replayed F2 would switch again.
                WindowEvent::KeyboardInput {
                    is_synthetic: true, ..
                } => return EventResult::PreventDefault,
                WindowEvent::KeyboardInput { event, .. } => {
                    // Surfaces handle no keys: every key stops here.
                    if !event.repeat {
                        if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                            if let Some(key) = key_of(code) {
                                sink(HostEvent::Key {
                                    key,
                                    pressed: event.state == ElementState::Pressed,
                                });
                            }
                        }
                    }
                    return EventResult::PreventDefault;
                }
                WindowEvent::Focused(focused) => sink(HostEvent::Focus(*focused)),
                WindowEvent::CloseRequested => {
                    sink(HostEvent::Close);
                    return EventResult::PreventDefault;
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }

    fn place(&self, window: &slint::Window, placement: Placement) {
        match placement {
            Placement::Fullscreen => window.set_fullscreen(true),
            Placement::Windowed { width, height } => {
                window.set_size(slint::LogicalSize::new(width as f32, height as f32))
            }
        }
    }
}
