//! The webOS backend: one SDL window (`window-sdl/`), shared by every
//! component. A switch replaces the window's content; the shell drops the old
//! surface at once. SDL keys go to the shell, never to the view.

use super::{Backend, FrameCounter, HostEvent, Placement};
use crate::input::Key;
use korri_window_sdl::HostKey;
use std::{cell::RefCell, rc::Rc};

type Sink = Rc<dyn Fn(HostEvent)>;

thread_local! {
    /// The sink of the component shown last: the window's front content.
    static FRONT: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

pub struct Sdl;

impl Sdl {
    pub fn install(placement: Placement) -> Result<Self, String> {
        korri_window_sdl::prepare_environment();
        let placement = match placement {
            Placement::Fullscreen => korri_window_sdl::Placement::Fullscreen,
            Placement::Windowed { width, height } => {
                korri_window_sdl::Placement::Windowed { width, height }
            }
        };
        korri_window_sdl::install_with_host_keys(
            "Korri",
            placement,
            Box::new(|key, pressed| {
                let key = match key {
                    HostKey::Up => Key::Up,
                    HostKey::Down => Key::Down,
                    HostKey::Left => Key::Left,
                    HostKey::Right => Key::Right,
                    HostKey::Confirm => Key::Return,
                    HostKey::Back => Key::Escape,
                    HostKey::F2 => Key::F2,
                    HostKey::M => Key::M,
                    HostKey::O => Key::O,
                };
                let sink = FRONT.with(|front| front.borrow().clone());
                if let Some(sink) = sink {
                    sink(HostEvent::Key { key, pressed });
                }
            }),
        )?;
        Ok(Self)
    }
}

impl Backend for Sdl {
    fn local_viewer(&self, presenting: bool, gameplay_overlay: bool) {
        korri_window_sdl::set_presentation(korri_window_sdl::Presentation::for_viewer(
            presenting,
            gameplay_overlay,
        ));
    }

    fn shared_window(&self) -> bool {
        true
    }

    fn attach(&self, _window: &slint::Window, sink: Rc<dyn Fn(HostEvent)>, _frames: FrameCounter) {
        FRONT.with(|front| *front.borrow_mut() = Some(sink));
    }

    fn place(&self, _window: &slint::Window, _placement: Placement) {
        // The SDL window was placed when it opened.
    }
}
