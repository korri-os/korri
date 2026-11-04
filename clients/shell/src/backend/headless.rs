//! A display-free backend for tests: every component gets its own
//! `MinimalSoftwareWindow`, rendered on request into a pixel buffer. The
//! software renderer is the same one the shell uses on screen.

use super::{Backend, FrameCounter, HostEvent, Placement};
use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel},
    Platform, WindowAdapter,
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// A window, its host sink, its frame counter, and whether its first frame is still due.
type Sink = (
    Weak<MinimalSoftwareWindow>,
    Rc<dyn Fn(HostEvent)>,
    FrameCounter,
    bool,
);

thread_local! {
    /// Set: the clock the platform reports, advanced only by `Headless::advance`.
    static MANUAL: std::cell::Cell<Option<std::time::Duration>> = const { std::cell::Cell::new(None) };
    static STARTED: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    static WINDOWS: RefCell<Vec<Weak<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
    static SINKS: RefCell<Vec<Sink>> = const { RefCell::new(Vec::new()) };
}

struct HeadlessPlatform {
    started: std::time::Instant,
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        window.set_size(slint::PhysicalSize::new(640, 480));
        WINDOWS.with(|w| w.borrow_mut().push(Rc::downgrade(&window)));
        Ok(window)
    }

    fn duration_since_start(&self) -> std::time::Duration {
        MANUAL
            .with(|m| m.get())
            .unwrap_or_else(|| self.started.elapsed())
    }
}

pub struct Headless;

impl Headless {
    /// Install the headless platform for this thread. Idempotent.
    pub fn install() -> Self {
        let started = std::time::Instant::now();
        if slint::platform::set_platform(Box::new(HeadlessPlatform { started })).is_ok() {
            STARTED.with(|s| s.set(Some(started)));
        }
        Self
    }

    /// From now on the clock moves only by `advance`: frames are rendered at instants the
    /// caller chooses, not at whatever time the machine took.
    pub fn manual_clock() {
        let now = STARTED
            .with(|s| s.get())
            .map_or_else(Default::default, |s| s.elapsed());
        MANUAL.with(|m| {
            if m.get().is_none() {
                m.set(Some(now));
            }
        });
    }

    /// Moves the manual clock forward (see `manual_clock`).
    pub fn advance(by: std::time::Duration) {
        MANUAL.with(|m| m.set(m.get().map(|t| t + by)));
    }

    /// Holds `animation-tick()` at the manual clock's current instant: motion drawn from it
    /// stands still between frames, while property animations and timers follow the clock.
    /// For comparing frames rendered at different times (the focus probe).
    pub fn freeze_motion() {
        let now = MANUAL.with(|m| m.get()).unwrap_or_default();
        slint::platform::software_renderer::SoftwareRenderer::new()
            .freeze_animation_tick(Some(now.as_millis() as u64));
    }

    /// Visible windows that are still alive.
    pub fn live_windows() -> usize {
        WINDOWS.with(|w| {
            let mut windows = w.borrow_mut();
            windows.retain(|w| w.strong_count() > 0);
            windows
                .iter()
                .filter_map(Weak::upgrade)
                .filter(|w| w.window().is_visible())
                .count()
        })
    }

    /// Render every live, visible window once; the first frame of each is
    /// reported like a real window's. Returns the RGB565 pixels of the last
    /// visible window created (the front one).
    pub fn render() -> Option<Vec<u16>> {
        slint::platform::update_timers_and_animations();
        let mut front = None;
        let windows: Vec<_> =
            WINDOWS.with(|w| w.borrow().iter().filter_map(Weak::upgrade).collect());
        for window in windows {
            if !window.window().is_visible() {
                continue;
            }
            let size = window.window().size();
            let mut buffer = vec![Rgb565Pixel::default(); (size.width * size.height) as usize];
            window.draw_if_needed(|renderer| {
                renderer.render(&mut buffer, size.width as usize);
            });
            let sink = SINKS.with(|s| {
                let mut sinks = s.borrow_mut();
                sinks.retain(|(w, ..)| w.strong_count() > 0);
                sinks
                    .iter_mut()
                    .find(|(w, ..)| w.upgrade().is_some_and(|w2| Rc::ptr_eq(&w2, &window)))
                    .map(|(_, sink, frames, first)| {
                        frames.add();
                        let was_first = std::mem::replace(first, false);
                        (sink.clone(), was_first)
                    })
            });
            if let Some((sink, true)) = sink {
                sink(HostEvent::FirstFrame);
            }
            front = Some(buffer.iter().map(|p| p.0).collect());
        }
        front
    }
}

impl Backend for Headless {
    fn shared_window(&self) -> bool {
        false
    }

    fn attach(&self, window: &slint::Window, sink: Rc<dyn Fn(HostEvent)>, frames: FrameCounter) {
        let found = WINDOWS.with(|w| {
            w.borrow()
                .iter()
                .filter_map(Weak::upgrade)
                .find(|candidate| std::ptr::eq(candidate.window(), window))
        });
        if let Some(window) = found {
            // Attached again (a window mapped anew): its next frame is a first frame again.
            SINKS.with(|s| {
                let mut sinks = s.borrow_mut();
                sinks.retain(|(w, ..)| !w.upgrade().is_some_and(|w| Rc::ptr_eq(&w, &window)));
                sinks.push((Rc::downgrade(&window), sink, frames, true))
            });
        }
    }

    fn place(&self, window: &slint::Window, placement: Placement) {
        if let Placement::Windowed { width, height } = placement {
            window.set_size(slint::PhysicalSize::new(width, height));
        }
    }
}
