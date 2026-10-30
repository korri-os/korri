//! THROWAWAY webOS experiment: a Slint custom platform over an existing SDL2.
//!
//! The view renders with Slint's software renderer into a buffer in this
//! process. SDL only shows that buffer: one streaming texture, copied and
//! presented by whatever SDL renderer the system offers (GLES2 on the LG B7).
//! SDL owns the window and the input; the adapter turns SDL events into Slint
//! window events at the host edge (`keys.rs`).
//!
//! What it deliberately does not do: grab Home or Exit (it asks LG's SDL for
//! Back only, so Back can close the inspector), keep the screen awake, redraw without a change, or talk to
//! the compositor directly. `SDL_VIDEO_ALLOW_SCREENSAVER=1` is set before
//! `SDL_Init`, and `SDL_EnableScreenSaver` runs after it, so SDL never holds
//! off the TV's screen saver.

mod ffi;
pub mod keys;
mod probe;
mod webos_cursor;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::{c_int, CString};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{EventLoopProxy, Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{EventLoopError, LogicalPosition, PhysicalSize, PlatformError};

use ffi::Sdl;

/// How the SDL window is placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Fill the display (`SDL_WINDOW_FULLSCREEN_DESKTOP`). The TV default.
    Fullscreen,
    /// A resizable window of this size, for the workstation check under Xvfb.
    Windowed { width: u32, height: u32 },
}

/// Logical height the view is laid out for when no scale is given: a
/// 1080-line output gets scale 1.5. A guess for a TV seen from a sofa, not a
/// measured value. `SLINT_SCALE_FACTOR`, Slint's own name, overrides it.
const DESIGN_HEIGHT: f32 = 720.0;
/// Logical pixels one wheel notch scrolls. A guess.
const WHEEL_STEP: f32 = 60.0;
/// Longest wait between loop turns when Slint has no timer pending.
const IDLE_WAIT: Duration = Duration::from_secs(1);

/// Set the two variables Moonlight TV's `main()` sets on webOS before SDL
/// starts (`src/main.c` at 89221f66: `EGL_PLATFORM=wayland`,
/// `XDG_RUNTIME_DIR=/tmp/xdg`, each only when unset). They apply only when
/// there is no X11 display, so the workstation check keeps its own values.
///
/// Also set `APPID` when unset. The B7's own SDL (LG `libsdl2-webos`
/// 2.0.4) carries the string `APPID` beside its `wl_webos_shell` and
/// `registerNativeApp` code. EXPERIMENT: the window gains and then loses
/// keyboard focus on the TV, and no key reaches SDL; this tests whether that
/// SDL needs the app id to bind the surface to the app. The value is the
/// package id from `webos/package.py`, never an inherited value.
///
/// Call before any thread starts: it changes the process environment.
pub fn prepare_environment() {
    if std::env::var_os("DISPLAY").is_some() {
        return;
    }
    APPID_INHERITED.store(std::env::var_os("APPID").is_some(), Ordering::SeqCst);
    for (name, value) in [
        ("EGL_PLATFORM", "wayland"),
        ("XDG_RUNTIME_DIR", "/tmp/xdg"),
        ("APPID", APP_ID),
    ] {
        if std::env::var_os(name).is_none() {
            std::env::set_var(name, value);
        }
    }
}

/// The webOS package id (`webos/package.py` `APP_ID`).
const APP_ID: &str = "org.korri.slint-spike";
/// Whether the launcher already supplied `APPID`, for the input probe.
static APPID_INHERITED: AtomicBool = AtomicBool::new(false);

/// Load SDL, open the window and install the platform. After this,
/// `InspectorWindow::new()` gets the SDL-backed window.
pub fn install(title: &str, placement: Placement) -> Result<(), String> {
    let platform = SdlPlatform::open(title, placement)?;
    slint::platform::set_platform(Box::new(platform)).map_err(|error| error.to_string())
}

/// State shared with other threads: queued closures and the quit request.
struct Shared {
    sdl: Arc<Sdl>,
    wake_event: u32,
    jobs: Mutex<VecDeque<Box<dyn FnOnce() + Send>>>,
    quit: AtomicBool,
    /// False once SDL has shut down; held while pushing a wake event.
    alive: Mutex<bool>,
}

impl Shared {
    fn wake(&self) -> Result<(), EventLoopError> {
        let alive = self.alive.lock().unwrap();
        if !*alive {
            return Err(EventLoopError::EventLoopTerminated);
        }
        let mut event = ffi::Event::zeroed();
        event.set_kind(self.wake_event);
        // SAFETY: SDL_PushEvent is thread-safe; SDL is initialised while `alive`.
        unsafe { (self.sdl.SDL_PushEvent)(&mut event) };
        Ok(())
    }
}

struct Proxy(Arc<Shared>);

impl EventLoopProxy for Proxy {
    fn quit_event_loop(&self) -> Result<(), EventLoopError> {
        self.0.quit.store(true, Ordering::SeqCst);
        self.0.wake()
    }

    fn invoke_from_event_loop(
        &self,
        event: Box<dyn FnOnce() + Send>,
    ) -> Result<(), EventLoopError> {
        self.0.jobs.lock().unwrap().push_back(event);
        self.0.wake()
    }
}

/// SDL objects owned by the event-loop thread.
struct Screen {
    window: *mut ffi::Window,
    renderer: *mut ffi::Renderer,
    texture: *mut ffi::Texture,
    /// Output size in pixels; the buffer and texture have this size.
    width: u32,
    height: u32,
    /// Output pixels per SDL window coordinate (mouse events use the latter).
    pixels_per_point: f32,
    buffer: Vec<PremultipliedRgbaColor>,
    /// The texture no longer holds the buffer (new texture, device reset).
    upload_all: bool,
}

#[derive(Default)]
struct FrameStats {
    frames: u32,
    render: Duration,
    present: Duration,
    worst: Duration,
}

pub struct SdlPlatform {
    shared: Arc<Shared>,
    slint_window: Rc<MinimalSoftwareWindow>,
    screen: RefCell<Screen>,
    scale: Cell<f32>,
    fixed_scale: Option<f32>,
    pointer: Cell<LogicalPosition>,
    visible: Cell<bool>,
    /// A Back press seen; Back acts on its release, so the release does not
    /// reach whatever LG shows next.
    back_pressed: Cell<bool>,
    stats: RefCell<FrameStats>,
    /// TEMPORARY: fixed-label focus and key-event evidence for the TV.
    probe: RefCell<probe::InputProbe>,
    /// The loaded SDL is LG's webOS build, whose keyboard event carries an
    /// extra field. Decided by the rule `prepare_environment` uses: no X11
    /// display means the TV. The workstation check runs upstream SDL under X11.
    webos_abi: bool,
    /// TV only: whether the D-pad or the pointer drives the view.
    mode: Cell<keys::Mode>,
    /// TV only: the pressed left button was turned into confirm, so its
    /// release must be too, whatever the mode is by then.
    click_confirming: Cell<bool>,
    /// TV only: the compositor's cursor request, when available.
    webos_cursor: Option<webos_cursor::WebosCursor>,
}

impl SdlPlatform {
    fn open(title: &str, placement: Placement) -> Result<Self, String> {
        let sdl = Arc::new(Sdl::load()?);
        let mut version = ffi::Version::default();
        // SAFETY: fills a plain struct.
        unsafe { (sdl.SDL_GetVersion)(&mut version) };
        println!(
            "sdl: runtime {}.{}.{} from {}",
            version.major,
            version.minor,
            version.patch,
            ffi::LIBRARY
        );
        if !sdl.set_hint(ffi::SDL_HINT_VIDEO_ALLOW_SCREENSAVER, "1") {
            return Err("SDL refused SDL_VIDEO_ALLOW_SCREENSAVER=1".into());
        }
        sdl.set_hint(ffi::SDL_HINT_NO_SIGNAL_HANDLERS, "1");
        // LG's SDL reads this when it creates the window. Without it the
        // compositor keeps Back and shows its own overlay. Only Back: Home,
        // Exit and the ribbon stay with LG.
        sdl.set_hint(ffi::SDL_HINT_WEBOS_ACCESS_POLICY_KEYS_BACK, "true");
        // SAFETY: first SDL call that starts subsystems, on the main thread.
        if unsafe { (sdl.SDL_Init)(ffi::SDL_INIT_VIDEO) } != 0 {
            return Err(format!("SDL_Init(VIDEO) failed: {}", sdl.error()));
        }
        // SAFETY: video is initialised.
        let screensaver = unsafe {
            (sdl.SDL_EnableScreenSaver)();
            (sdl.SDL_StopTextInput)();
            (sdl.SDL_IsScreenSaverEnabled)() != 0
        };
        // SAFETY: returns a static string or null.
        let driver = unsafe { ffi::text((sdl.SDL_GetCurrentVideoDriver)()) };
        println!("sdl: video driver {driver}; screen saver allowed: {screensaver}");
        if !screensaver {
            // SAFETY: undo SDL_Init before refusing.
            unsafe { (sdl.SDL_Quit)() };
            return Err("SDL still suppresses the screen saver; refusing to open".into());
        }

        let screen = match open_screen(&sdl, title, placement) {
            Ok(screen) => screen,
            Err(error) => {
                // SAFETY: nothing else holds SDL objects yet.
                unsafe { (sdl.SDL_Quit)() };
                return Err(error);
            }
        };
        // SAFETY: video is initialised.
        let wake_event = unsafe { (sdl.SDL_RegisterEvents)(1) };
        if wake_event == u32::MAX {
            return Err(format!("SDL_RegisterEvents failed: {}", sdl.error()));
        }

        let fixed_scale = std::env::var("SLINT_SCALE_FACTOR")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|scale| *scale > 0.0);
        let mut platform = Self {
            shared: Arc::new(Shared {
                sdl,
                wake_event,
                jobs: Mutex::new(VecDeque::new()),
                quit: AtomicBool::new(false),
                alive: Mutex::new(true),
            }),
            slint_window: MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer),
            screen: RefCell::new(screen),
            scale: Cell::new(0.0),
            fixed_scale,
            pointer: Cell::new(LogicalPosition::default()),
            visible: Cell::new(true),
            back_pressed: Cell::new(false),
            stats: RefCell::new(FrameStats::default()),
            probe: RefCell::new(probe::InputProbe::for_tv()),
            webos_abi: std::env::var_os("DISPLAY").is_none(),
            mode: Cell::new(keys::Mode::Keys),
            click_confirming: Cell::new(false),
            webos_cursor: None,
        };
        if platform.webos_abi {
            let window = platform.screen.borrow().window;
            let status = match webos_cursor::WebosCursor::open(wayland_display(
                &platform.shared.sdl,
                window,
            )) {
                Ok(cursor) => {
                    // The view opens in D-pad mode, so LG's cursor starts
                    // hidden; a shake shows it and switches to the pointer.
                    cursor.set_visible(false);
                    platform.webos_cursor = Some(cursor);
                    probe::Record::Cursor(None)
                }
                Err(why) => probe::Record::Cursor(Some(why)),
            };
            platform.probe.borrow_mut().record(status);
        }
        platform
            .probe
            .borrow_mut()
            .record(if APPID_INHERITED.load(Ordering::SeqCst) {
                probe::Record::AppIdInherited
            } else {
                probe::Record::AppIdSupplied
            });
        platform.apply_size();
        Ok(platform)
    }

    fn sdl(&self) -> &Sdl {
        &self.shared.sdl
    }

    /// Tell Slint the output size and the scale that goes with it.
    fn apply_size(&self) {
        let (width, height) = {
            let screen = self.screen.borrow();
            (screen.width, screen.height)
        };
        let scale = self
            .fixed_scale
            .unwrap_or_else(|| (height as f32 / DESIGN_HEIGHT).max(1.0));
        if scale != self.scale.get() {
            self.scale.set(scale);
            self.slint_window
                .dispatch_event(WindowEvent::ScaleFactorChanged {
                    scale_factor: scale,
                });
        }
        self.slint_window.set_size(PhysicalSize::new(width, height));
        println!(
            "sdl: output {width}x{height} px, scale {scale}, view {}x{} logical px",
            (width as f32 / scale).round(),
            (height as f32 / scale).round()
        );
    }

    fn resized(&self) -> Result<(), PlatformError> {
        let changed = {
            let mut screen = self.screen.borrow_mut();
            let (old_width, old_height) = (screen.width, screen.height);
            measure(self.sdl(), &mut screen);
            if (screen.width, screen.height) != (old_width, old_height) {
                make_texture(self.sdl(), &mut screen).map_err(PlatformError::Other)?;
                true
            } else {
                false
            }
        };
        if changed {
            self.apply_size();
        }
        self.slint_window.request_redraw();
        Ok(())
    }

    fn render(&self) -> Result<(), PlatformError> {
        if !self.visible.get() {
            return Ok(());
        }
        let mut failure = None;
        self.slint_window.draw_if_needed(|renderer| {
            let mut screen = self.screen.borrow_mut();
            let screen = &mut *screen;
            let started = Instant::now();
            if screen.upload_all {
                // A fresh texture or buffer: draw everything once.
                renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            }
            let region = renderer.render(&mut screen.buffer, screen.width as usize);
            renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            let rendered = Instant::now();
            let dirty = if screen.upload_all {
                Some((0, 0, screen.width, screen.height))
            } else {
                let origin = region.bounding_box_origin();
                let size = region.bounding_box_size();
                (size.width > 0 && size.height > 0).then_some((
                    origin.x.max(0) as u32,
                    origin.y.max(0) as u32,
                    size.width,
                    size.height,
                ))
            };
            if let Some(rect) = dirty {
                if let Err(error) = upload(&self.shared.sdl, screen, rect) {
                    failure = Some(error);
                    return;
                }
                screen.upload_all = false;
            }
            // SAFETY: renderer and texture are live; full-target copy.
            unsafe {
                (self.shared.sdl.SDL_RenderCopy)(
                    screen.renderer,
                    screen.texture,
                    std::ptr::null(),
                    std::ptr::null(),
                );
                (self.shared.sdl.SDL_RenderPresent)(screen.renderer);
            }
            let presented = Instant::now();
            let mut stats = self.stats.borrow_mut();
            stats.frames += 1;
            stats.render += rendered - started;
            stats.present += presented - rendered;
            stats.worst = stats.worst.max(presented - started);
            if stats.frames <= 3 {
                println!(
                    "sdl: frame {} rendered in {} ms, uploaded and presented in {} ms",
                    stats.frames,
                    (rendered - started).as_millis(),
                    (presented - rendered).as_millis()
                );
            }
        });
        match failure {
            Some(error) => Err(PlatformError::Other(error)),
            None => Ok(()),
        }
    }

    fn handle(&self, event: &ffi::Event) -> Result<(), PlatformError> {
        let kind = event.kind();
        match kind {
            ffi::SDL_QUIT | ffi::SDL_APP_TERMINATING => {
                println!("sdl: quit requested by the system (event {kind:#x})");
                self.shared.quit.store(true, Ordering::SeqCst);
            }
            ffi::SDL_APP_LOWMEMORY
            | ffi::SDL_APP_WILLENTERBACKGROUND
            | ffi::SDL_APP_DIDENTERBACKGROUND
            | ffi::SDL_APP_WILLENTERFOREGROUND
            | ffi::SDL_APP_DIDENTERFOREGROUND => {
                println!("sdl: application lifecycle event {kind:#x}");
            }
            ffi::SDL_WINDOWEVENT => self.window_event(event)?,
            ffi::SDL_KEYDOWN | ffi::SDL_KEYUP => {
                self.probe.borrow_mut().record(if kind == ffi::SDL_KEYDOWN {
                    probe::Record::KeyDown
                } else {
                    probe::Record::KeyUp
                });
                self.key_event(event, kind == ffi::SDL_KEYDOWN)
            }
            // TV: OK on the Magic Remote arrives as a left click wherever the
            // LG pointer last was. While the user drives the D-pad, that click
            // confirms the focused control instead; once the pointer moves,
            // clicks aim again. See `keys::Mode`.
            ffi::SDL_MOUSEBUTTONDOWN | ffi::SDL_MOUSEBUTTONUP
                if self.webos_abi && self.click_is_confirm(event, kind) =>
            {
                let text = char::from(slint::platform::Key::Return).into();
                self.slint_window
                    .dispatch_event(if kind == ffi::SDL_MOUSEBUTTONDOWN {
                        WindowEvent::KeyPressed { text }
                    } else {
                        WindowEvent::KeyReleased { text }
                    });
            }
            ffi::SDL_MOUSEMOTION => {
                let (x, y) = event.motion();
                let position = self.logical(x, y);
                self.pointer.set(position);
                // TV: LG's cursor state decides, not motion. A hidden cursor
                // neither hovers nor aims; it can still report small motion.
                if !self.webos_abi || self.mode.get() == keys::Mode::Pointer {
                    self.slint_window
                        .dispatch_event(WindowEvent::PointerMoved { position });
                }
            }
            ffi::SDL_MOUSEBUTTONDOWN | ffi::SDL_MOUSEBUTTONUP => {
                let (button, x, y) = event.button();
                let position = self.logical(x, y);
                self.pointer.set(position);
                let button = match button {
                    ffi::SDL_BUTTON_LEFT => PointerEventButton::Left,
                    ffi::SDL_BUTTON_RIGHT => PointerEventButton::Right,
                    ffi::SDL_BUTTON_MIDDLE => PointerEventButton::Middle,
                    _ => PointerEventButton::Other,
                };
                self.slint_window
                    .dispatch_event(if kind == ffi::SDL_MOUSEBUTTONDOWN {
                        WindowEvent::PointerPressed { position, button }
                    } else {
                        WindowEvent::PointerReleased { position, button }
                    });
            }
            ffi::SDL_MOUSEWHEEL if self.webos_abi && self.mode.get() == keys::Mode::Keys => {}
            ffi::SDL_MOUSEWHEEL => {
                let (x, y) = event.wheel();
                self.slint_window
                    .dispatch_event(WindowEvent::PointerScrolled {
                        position: self.pointer.get(),
                        delta_x: x as f32 * WHEEL_STEP,
                        delta_y: y as f32 * WHEEL_STEP,
                    });
            }
            ffi::SDL_RENDER_TARGETS_RESET => {
                self.screen.borrow_mut().upload_all = true;
                self.slint_window.request_redraw();
            }
            ffi::SDL_RENDER_DEVICE_RESET => {
                println!("sdl: render device reset; recreating the texture");
                let mut screen = self.screen.borrow_mut();
                make_texture(self.sdl(), &mut screen).map_err(PlatformError::Other)?;
                self.slint_window.request_redraw();
            }
            _ => {}
        }
        Ok(())
    }

    fn window_event(&self, event: &ffi::Event) -> Result<(), PlatformError> {
        let (id, _, _) = event.window();
        match id {
            ffi::SDL_WINDOWEVENT_SHOWN
            | ffi::SDL_WINDOWEVENT_EXPOSED
            | ffi::SDL_WINDOWEVENT_RESTORED => {
                if !self.visible.replace(true) {
                    println!("sdl: window visible");
                }
                // The compositor may have dropped what was on screen.
                self.screen.borrow_mut().upload_all = true;
                self.slint_window.request_redraw();
            }
            ffi::SDL_WINDOWEVENT_HIDDEN | ffi::SDL_WINDOWEVENT_MINIMIZED => {
                if self.visible.replace(false) {
                    println!("sdl: window hidden; drawing paused");
                }
            }
            ffi::SDL_WINDOWEVENT_SIZE_CHANGED => self.resized()?,
            ffi::SDL_WINDOWEVENT_FOCUS_GAINED | ffi::SDL_WINDOWEVENT_FOCUS_LOST => {
                let active = id == ffi::SDL_WINDOWEVENT_FOCUS_GAINED;
                self.probe.borrow_mut().record(if active {
                    probe::Record::FocusGained
                } else {
                    probe::Record::FocusLost
                });
                println!(
                    "sdl: window {} input focus",
                    if active { "has" } else { "lost" }
                );
                self.back_pressed.set(false);
                self.slint_window
                    .dispatch_event(WindowEvent::WindowActiveChanged(active));
            }
            ffi::SDL_WINDOWEVENT_LEAVE => {
                self.slint_window.dispatch_event(WindowEvent::PointerExited);
            }
            ffi::SDL_WINDOWEVENT_CLOSE => {
                println!("sdl: window close requested");
                self.shared.quit.store(true, Ordering::SeqCst);
            }
            _ => {}
        }
        Ok(())
    }

    fn key_event(&self, event: &ffi::Event, pressed: bool) {
        let key = event.key(self.webos_abi);
        let action = keys::translate(key.keycode, key.scancode, key.modifiers);
        if self.webos_abi && pressed {
            if let Some(mode) = keys::mode_after_key(key.scancode, action) {
                // Moonlight TV hides LG's cursor on every D-pad press
                // (`lv_drv_sdl_key.c` `webos_key_input_mode`); LG shows it
                // again when the remote is shaken and reports that as 484.
                if let Some(cursor) = &self.webos_cursor {
                    cursor.set_visible(mode == keys::Mode::Pointer);
                }
                match (self.mode.replace(mode), mode) {
                    // LG hid its cursor: drop the pointer's hover highlight.
                    (keys::Mode::Pointer, keys::Mode::Keys) => {
                        self.slint_window.dispatch_event(WindowEvent::PointerExited);
                    }
                    // LG showed its cursor again: hover where it is.
                    (keys::Mode::Keys, keys::Mode::Pointer) => {
                        self.slint_window.dispatch_event(WindowEvent::PointerMoved {
                            position: self.pointer.get(),
                        });
                    }
                    _ => {}
                }
            }
        }
        if pressed && !key.repeat {
            self.probe.borrow_mut().record(probe::Record::KeyPress {
                scancode: key.scancode,
                keycode: key.keycode,
                action: match action {
                    keys::Action::Key(' ') => probe::KeyAction::Space,
                    keys::Action::Key(text) if text == char::from(slint::platform::Key::Tab) => {
                        probe::KeyAction::Tab
                    }
                    keys::Action::Key(text)
                        if text == char::from(slint::platform::Key::Backtab) =>
                    {
                        probe::KeyAction::Backtab
                    }
                    keys::Action::Key(_) => probe::KeyAction::Return,
                    keys::Action::Back => probe::KeyAction::Back,
                    keys::Action::Unmapped => probe::KeyAction::Unmapped,
                },
            });
            // Key numbers only, never text: shows which code the TV remote sends.
            println!(
                "sdl: key scancode={} keycode={:#x} -> {:?}",
                key.scancode, key.keycode, action
            );
        }
        match action {
            keys::Action::Key(text) => {
                let text = text.into();
                self.slint_window
                    .dispatch_event(match (pressed, key.repeat) {
                        (true, false) => WindowEvent::KeyPressed { text },
                        (true, true) => WindowEvent::KeyPressRepeated { text },
                        (false, _) => WindowEvent::KeyReleased { text },
                    });
            }
            keys::Action::Back if pressed => self.back_pressed.set(true),
            keys::Action::Back => {
                if self.back_pressed.replace(false) {
                    println!("sdl: back: closing the inspector");
                    self.shared.quit.store(true, Ordering::SeqCst);
                }
            }
            keys::Action::Unmapped => {}
        }
    }

    fn logical(&self, x: i32, y: i32) -> LogicalPosition {
        let factor = self.screen.borrow().pixels_per_point / self.scale.get();
        LogicalPosition::new(x as f32 * factor, y as f32 * factor)
    }

    fn run_jobs(&self) {
        let jobs: Vec<_> = self.shared.jobs.lock().unwrap().drain(..).collect();
        for job in jobs {
            job();
        }
    }

    fn close(&self) {
        *self.shared.alive.lock().unwrap() = false;
        let screen = self.screen.borrow();
        // SAFETY: the loop has ended; nothing uses these objects again.
        unsafe {
            (self.sdl().SDL_DestroyTexture)(screen.texture);
            (self.sdl().SDL_DestroyRenderer)(screen.renderer);
            (self.sdl().SDL_DestroyWindow)(screen.window);
            (self.sdl().SDL_Quit)();
        }
        let stats = self.stats.borrow();
        if stats.frames > 0 {
            println!(
                "sdl: {} frames; mean render {} ms, mean upload+present {} ms, worst frame {} ms",
                stats.frames,
                (stats.render / stats.frames).as_millis(),
                (stats.present / stats.frames).as_millis(),
                stats.worst.as_millis()
            );
        }
        println!("sdl: window closed");
    }
}

impl Platform for SdlPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.slint_window.clone())
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        if !*self.shared.alive.lock().unwrap() {
            return Err(PlatformError::Other(
                "the SDL window is already closed".into(),
            ));
        }
        let outcome = self.event_loop();
        self.close();
        outcome
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(Proxy(self.shared.clone())))
    }
}

impl SdlPlatform {
    /// TV: should this mouse button event confirm the focused control?
    fn click_is_confirm(&self, event: &ffi::Event, kind: u32) -> bool {
        let (button, _, _) = event.button();
        if button != ffi::SDL_BUTTON_LEFT {
            return false;
        }
        if kind == ffi::SDL_MOUSEBUTTONDOWN {
            let confirm = self.mode.get() == keys::Mode::Keys;
            self.click_confirming.set(confirm);
            confirm
        } else {
            self.click_confirming.replace(false)
        }
    }

    /// Compare SDL's keyboard-focus window with ours. Only the category is
    /// recorded; the pointers never leave this function.
    fn sample_keyboard_focus(&self) {
        // SAFETY: video is initialised; the call only reads SDL state.
        let focused = unsafe { (self.sdl().SDL_GetKeyboardFocus)() };
        let focus = if focused.is_null() {
            probe::Focus::None
        } else if focused == self.screen.borrow().window {
            probe::Focus::Own
        } else {
            probe::Focus::Other
        };
        self.probe.borrow_mut().focus(focus);
    }

    fn event_loop(&self) -> Result<(), PlatformError> {
        loop {
            self.sample_keyboard_focus();
            slint::platform::update_timers_and_animations();
            self.run_jobs();
            if self.shared.quit.load(Ordering::SeqCst) {
                return Ok(());
            }
            self.render()?;
            let wait = if self.slint_window.has_active_animations() {
                Duration::from_millis(16)
            } else {
                slint::platform::duration_until_next_timer_update()
                    .map_or(IDLE_WAIT, |next| next.min(IDLE_WAIT))
            };
            let mut event = ffi::Event::zeroed();
            // SAFETY: valid event storage; called on the thread that ran SDL_Init.
            let got =
                unsafe { (self.sdl().SDL_WaitEventTimeout)(&mut event, wait.as_millis() as c_int) };
            if got == 1 {
                self.handle(&event)?;
                // SAFETY: as above.
                while unsafe { (self.sdl().SDL_PollEvent)(&mut event) } == 1 {
                    self.handle(&event)?;
                }
            }
        }
    }
}

/// SDL's `wl_display`, or null when SDL is not on Wayland.
fn wayland_display(sdl: &Sdl, window: *mut ffi::Window) -> *mut std::ffi::c_void {
    let mut info = ffi::SysWmInfo {
        version: ffi::Version {
            major: 2,
            minor: 0,
            patch: 4,
        },
        subsystem: 0,
        info: [std::ptr::null_mut(); 16],
    };
    // SAFETY: live window; `info` has the 2.0.4 layout and its version set.
    let ok = unsafe { (sdl.SDL_GetWindowWMInfo)(window, &mut info) } != 0;
    if ok && info.subsystem == ffi::SDL_SYSWM_WAYLAND {
        info.info[0]
    } else {
        std::ptr::null_mut()
    }
}

fn open_screen(sdl: &Sdl, title: &str, placement: Placement) -> Result<Screen, String> {
    let title = CString::new(title).unwrap();
    let (width, height, flags) = match placement {
        Placement::Windowed { width, height } => (
            width as c_int,
            height as c_int,
            ffi::SDL_WINDOW_SHOWN | ffi::SDL_WINDOW_RESIZABLE,
        ),
        Placement::Fullscreen => {
            let mut mode = ffi::DisplayMode {
                format: 0,
                w: 0,
                h: 0,
                refresh_rate: 0,
                driverdata: std::ptr::null_mut(),
            };
            // SAFETY: fills a plain struct for display 0.
            if unsafe { (sdl.SDL_GetDesktopDisplayMode)(0, &mut mode) } != 0 {
                return Err(format!("SDL_GetDesktopDisplayMode failed: {}", sdl.error()));
            }
            println!(
                "sdl: display 0 desktop mode {}x{} at {} Hz",
                mode.w, mode.h, mode.refresh_rate
            );
            (
                mode.w,
                mode.h,
                ffi::SDL_WINDOW_SHOWN | ffi::SDL_WINDOW_FULLSCREEN_DESKTOP,
            )
        }
    };
    // SAFETY: valid title; SDL video is initialised.
    let window = unsafe {
        (sdl.SDL_CreateWindow)(
            title.as_ptr(),
            ffi::SDL_WINDOWPOS_UNDEFINED,
            ffi::SDL_WINDOWPOS_UNDEFINED,
            width,
            height,
            flags,
        )
    };
    if window.is_null() {
        return Err(format!("SDL_CreateWindow failed: {}", sdl.error()));
    }
    // Accelerated first (GLES2 on the TV); any renderer if that fails.
    // SAFETY: live window.
    let mut renderer =
        unsafe { (sdl.SDL_CreateRenderer)(window, -1, ffi::SDL_RENDERER_ACCELERATED) };
    if renderer.is_null() {
        println!("sdl: no accelerated renderer ({}); trying any", sdl.error());
        // SAFETY: live window.
        renderer = unsafe { (sdl.SDL_CreateRenderer)(window, -1, 0) };
    }
    if renderer.is_null() {
        let error = sdl.error();
        // SAFETY: live window, no renderer.
        unsafe { (sdl.SDL_DestroyWindow)(window) };
        return Err(format!("SDL_CreateRenderer failed: {error}"));
    }
    let mut info = ffi::RendererInfo {
        name: std::ptr::null(),
        flags: 0,
        num_texture_formats: 0,
        texture_formats: [0; 16],
        max_texture_width: 0,
        max_texture_height: 0,
    };
    // SAFETY: live renderer; fills a plain struct whose name is static.
    if unsafe { (sdl.SDL_GetRendererInfo)(renderer, &mut info) } == 0 {
        let formats = &info.texture_formats[..(info.num_texture_formats as usize).min(16)];
        println!(
            "sdl: renderer {} (flags {:#x}), max texture {}x{}, ABGR8888 native: {}",
            // SAFETY: SDL's static renderer name.
            unsafe { ffi::text(info.name) },
            info.flags,
            info.max_texture_width,
            info.max_texture_height,
            formats.contains(&ffi::SDL_PIXELFORMAT_ABGR8888)
        );
    }
    let mut screen = Screen {
        window,
        renderer,
        texture: std::ptr::null_mut(),
        width: 0,
        height: 0,
        pixels_per_point: 1.0,
        buffer: Vec::new(),
        upload_all: true,
    };
    measure(sdl, &mut screen);
    if let Err(error) = make_texture(sdl, &mut screen) {
        // SAFETY: live renderer and window, owned here.
        unsafe {
            (sdl.SDL_DestroyRenderer)(renderer);
            (sdl.SDL_DestroyWindow)(window);
        }
        return Err(error);
    }
    Ok(screen)
}

/// Read the window size in SDL coordinates and the output size in pixels.
fn measure(sdl: &Sdl, screen: &mut Screen) {
    let (mut points_w, mut points_h, mut pixels_w, mut pixels_h) = (0, 0, 0, 0);
    // SAFETY: live window and renderer; plain out-parameters.
    unsafe {
        (sdl.SDL_GetWindowSize)(screen.window, &mut points_w, &mut points_h);
        if (sdl.SDL_GetRendererOutputSize)(screen.renderer, &mut pixels_w, &mut pixels_h) != 0 {
            (pixels_w, pixels_h) = (points_w, points_h);
        }
    }
    screen.width = pixels_w.max(1) as u32;
    screen.height = pixels_h.max(1) as u32;
    screen.pixels_per_point = if points_w > 0 {
        pixels_w as f32 / points_w as f32
    } else {
        1.0
    };
}

/// (Re)create the streaming texture and the buffer at the output size.
fn make_texture(sdl: &Sdl, screen: &mut Screen) -> Result<(), String> {
    if !screen.texture.is_null() {
        // SAFETY: texture owned by this screen.
        unsafe { (sdl.SDL_DestroyTexture)(screen.texture) };
        screen.texture = std::ptr::null_mut();
    }
    // SAFETY: live renderer.
    let texture = unsafe {
        (sdl.SDL_CreateTexture)(
            screen.renderer,
            ffi::SDL_PIXELFORMAT_ABGR8888,
            ffi::SDL_TEXTUREACCESS_STREAMING,
            screen.width as c_int,
            screen.height as c_int,
        )
    };
    if texture.is_null() {
        return Err(format!(
            "SDL_CreateTexture {}x{} failed: {}",
            screen.width,
            screen.height,
            sdl.error()
        ));
    }
    // The view paints an opaque background; copy without blending.
    // SAFETY: live texture.
    unsafe { (sdl.SDL_SetTextureBlendMode)(texture, ffi::SDL_BLENDMODE_NONE) };
    screen.texture = texture;
    screen.buffer =
        vec![PremultipliedRgbaColor::default(); screen.width as usize * screen.height as usize];
    screen.upload_all = true;
    Ok(())
}

/// Copy one rectangle of the buffer into the texture.
fn upload(sdl: &Sdl, screen: &Screen, (x, y, w, h): (u32, u32, u32, u32)) -> Result<(), String> {
    let x = x.min(screen.width);
    let y = y.min(screen.height);
    let w = w.min(screen.width - x);
    let h = h.min(screen.height - y);
    if w == 0 || h == 0 {
        return Ok(());
    }
    let rect = ffi::Rect {
        x: x as c_int,
        y: y as c_int,
        w: w as c_int,
        h: h as c_int,
    };
    let start = y as usize * screen.width as usize + x as usize;
    let pitch = screen.width as c_int * 4;
    // SAFETY: `start` and the rectangle lie inside the buffer, whose rows are
    // `pitch` bytes apart; PremultipliedRgbaColor is four bytes R, G, B, A.
    let status = unsafe {
        (sdl.SDL_UpdateTexture)(
            screen.texture,
            &rect,
            screen.buffer[start..].as_ptr().cast(),
            pitch,
        )
    };
    if status != 0 {
        return Err(format!("SDL_UpdateTexture failed: {}", sdl.error()));
    }
    Ok(())
}
