//! The shell: loads verified surface packages, drives their logic with
//! korrid's model and the person's input, and switches between them.
//!
//! Every event reaches [`Shell::handle`] on the UI thread through a
//! [`Post`]. Nothing here re-enters itself from a window callback.
//!
//! Lifecycle of a surface (contract 3 and 6):
//! 1. compile `main.slint` (`Surface`, and `Overlay` when exported), the
//!    first time the package loads in this process; later loads reuse it
//! 2. evaluate `logic.js` in a fresh engine and read its preferences
//! 3. `app.surface.declare`, then `app.surface.preferences`
//! 4. `app.surface.model`, `init`, `view`, create and show `Surface`
//! 5. watch the model; each push runs `update(KorriPublished)` then acks
//!
//! The logic runs on its own thread (`crate::logic`): presses, ticks and
//! models go to it as calls, one at a time, and each outcome comes back as
//! [`Event::Logic`]; its view is applied then, in the next frame. The UI
//! thread never waits on logic after a surface loaded.
//!
//! A failure in steps 1 to 4 keeps the surface already shown. At run time
//! (contract amendment A5) a call over the 50 ms budget is logged as a
//! performance defect and the surface keeps running. A call still running at
//! the stuck limit, a throw, a broken contract or the heap limit unloads the
//! surface, and the shell shows its own notice (`crate::notice`): it switches
//! only when the person chooses to try the surface again or use another one.

use crate::{
    backend::{Backend, FrameCounter, HostEvent, Placement},
    engine::{self, Limits, Violation},
    images::ImageCache,
    input::{native::NativeEvent, Press, Router},
    korrid::{Korrid, WatchEvent},
    logic::{Call, Done, Logic, Outcome},
    messages::{self, Button},
    notice::{self, Choice, Notice, Reason},
    package::{self, Package},
    view::{Binder, Diff},
};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use slint::ComponentHandle;
use slint_interpreter::{ComponentDefinition, ComponentInstance};
use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

/// Everything the shell reacts to.
#[derive(Debug)]
pub enum Event {
    /// `window`: 0 for a surface's `Surface`, 1 for its `Overlay`.
    Host {
        generation: u64,
        window: u8,
        event: HostEvent,
    },
    Native(Vec<NativeEvent>),
    Model {
        generation: u64,
        revision: Option<String>,
        model: String,
    },
    Reconnected {
        generation: u64,
    },
    RequestDone {
        generation: u64,
        request: Value,
        outcome: std::result::Result<(), String>,
    },
    /// Repeat timer for held directions.
    Tick,
    /// Once a second: send the active surface a `Tick` (amendment A1).
    Idle,
    /// A logic call returned (`crate::logic`).
    Logic {
        generation: u64,
        outcome: Box<Outcome>,
    },
}

/// Delivers events to the shell on the UI thread, later.
pub trait Post: Send + Sync + 'static {
    fn post(&self, event: Event);
}

#[derive(Clone, Debug)]
pub struct Options {
    pub limits: Limits,
    pub placement: Placement,
    pub stats: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            placement: Placement::Windowed {
                width: 640,
                height: 480,
            },
            stats: false,
        }
    }
}

/// Told when the shell cannot continue (`Err`) or its window closed (`Ok`).
pub type QuitCallback = Box<dyn Fn(Result<(), String>)>;

/// Measured on each load, reported with `--stats`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LoadTiming {
    pub compile: Duration,
    pub logic: Duration,
    pub korrid: Duration,
    pub shown: Duration,
}

struct Loaded {
    generation: u64,
    /// The last `Pressed` delivered to this surface, or when it loaded.
    last_press: Instant,
    /// The last size sent in `Resized`, in logical pixels.
    size: (f32, f32),
    package: Package,
    logic: Logic,
    /// Posts this surface's logic outcomes to the UI thread.
    done: Done,
    overlay_definition: Option<ComponentDefinition>,
    surface: ComponentInstance,
    surface_binder: Binder,
    overlay: Option<(ComponentInstance, Binder)>,
    images: ImageCache,
    watch: Option<crate::korrid::Watch>,
    drawn: Rc<Cell<bool>>,
    gameplay_overlay: bool,
    /// Whether the Surface window is mapped. Only one of Surface and Overlay
    /// is mapped at a time: korrid places the one window its shell unit owns.
    surface_shown: bool,
    /// The `app.surface.declare` preferences payload, sent again once this
    /// surface's window is the only one, so korrid places that window.
    declaration: Value,
    /// Declared again after its first frame, with no other window left.
    placed: bool,
    started: Instant,
    timing: LoadTiming,
}

/// The notice on screen (or, once a choice loaded a surface, until that surface's
/// first frame).
struct Shown {
    notice: Notice,
    component: ComponentInstance,
    generation: u64,
}

/// Slow calls of one surface and one message type since its last log line.
struct SlowCalls {
    logged: Instant,
    since: u32,
    slowest: Duration,
}

/// At most one performance-defect line per surface and message type in this time.
pub const SLOW_LOG_INTERVAL: Duration = Duration::from_secs(60);

struct RequestJob {
    generation: u64,
    surface: String,
    request: Value,
}

pub struct Shell {
    packages: Vec<Package>,
    /// Each package's views, compiled at its first load and kept for the life of the
    /// process: a compile is most of a load (Zone: about 0.4 s on a workstation, 2.6 to
    /// 3.6 s on the RG353M), and the package list is fixed at start.
    compiled: std::collections::HashMap<String, package::Views>,
    compilations: usize,
    korrid: Korrid,
    backend: Rc<dyn Backend>,
    post: Arc<dyn Post>,
    options: Options,
    router: Router,
    current: Option<Loaded>,
    retiring: Vec<Loaded>,
    /// The notice after an unload (A5). It takes the presses while no surface is current.
    notice: Option<Shown>,
    /// `notice.slint`, compiled the first time a notice is shown.
    notice_definition: Option<ComponentDefinition>,
    /// Slow calls by surface id and message `_tag`, for the rate-limited log.
    slow: HashMap<(String, String), SlowCalls>,
    previous: Option<String>,
    next_generation: u64,
    jobs: mpsc::Sender<RequestJob>,
    clock: Instant,
    frames: FrameCounter,
    repeat: Option<slint::Timer>,
    /// Sends `Event::Idle` once a second while the shell runs.
    idle_timer: Option<slint::Timer>,
    native_active: Option<Box<dyn Fn(bool)>>,
    /// Shell windows that have keyboard focus; `None` until the first focus event.
    focused: Option<std::collections::HashSet<(u64, u8)>>,
    quit: Option<QuitCallback>,
    log: Vec<String>,
}

impl Shell {
    pub fn new(
        packages: Vec<Package>,
        korrid: Korrid,
        backend: Rc<dyn Backend>,
        post: Arc<dyn Post>,
        options: Options,
    ) -> Self {
        let (jobs, receiver) = mpsc::channel::<RequestJob>();
        let worker_korrid = korrid.clone();
        let worker_post = post.clone();
        // Requests go out in order on one thread; their consequences come
        // back as new models.
        thread::Builder::new()
            .name("korrid-requests".into())
            .spawn(move || {
                for job in receiver {
                    let outcome = worker_korrid
                        .request(&job.surface, &job.request)
                        .map_err(|e| e.to_string());
                    worker_post.post(Event::RequestDone {
                        generation: job.generation,
                        request: job.request,
                        outcome,
                    });
                }
            })
            .expect("spawn the request thread");
        Self {
            packages,
            compiled: Default::default(),
            compilations: 0,
            korrid,
            backend,
            post,
            options,
            router: Router::new(true),
            current: None,
            retiring: Vec::new(),
            notice: None,
            notice_definition: None,
            slow: HashMap::new(),
            previous: None,
            next_generation: 0,
            jobs,
            clock: Instant::now(),
            frames: FrameCounter::default(),
            repeat: None,
            idle_timer: None,
            native_active: None,
            focused: None,
            quit: None,
            log: Vec::new(),
        }
    }

    /// Called when the native input subscription should follow focus.
    pub fn on_native_active(&mut self, callback: Box<dyn Fn(bool)>) {
        self.native_active = Some(callback);
    }

    /// Called when the shell cannot continue (`Err`) or the window closed.
    pub fn on_quit(&mut self, callback: QuitCallback) {
        self.quit = Some(callback);
    }

    pub fn current_id(&self) -> Option<&str> {
        self.current.as_ref().map(|l| l.package.id.as_str())
    }

    pub fn frames(&self) -> FrameCounter {
        self.frames.clone()
    }

    pub fn timing(&self) -> Option<LoadTiming> {
        self.current.as_ref().map(|l| l.timing)
    }

    /// How many times a package's `main.slint` was compiled, for tests.
    pub fn compilations(&self) -> usize {
        self.compilations
    }

    /// Lines the shell logged, for tests.
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// The notice the person must answer, while no surface is current.
    pub fn notice(&self) -> Option<&Notice> {
        self.notice
            .as_ref()
            .filter(|_| self.current.is_none())
            .map(|shown| &shown.notice)
    }

    /// A property of the shown notice's component, for tests.
    pub fn notice_property(&self, name: &str) -> Option<slint_interpreter::Value> {
        self.notice.as_ref()?.component.get_property(name).ok()
    }

    pub fn overlay_shown(&self) -> bool {
        self.current.as_ref().is_some_and(|l| l.overlay.is_some())
    }

    pub fn retiring(&self) -> usize {
        self.retiring.len()
    }

    /// The current surface's QuickJS heap, in bytes.
    pub fn heap_bytes(&self) -> usize {
        self.current.as_ref().map_or(0, |l| l.logic.heap_bytes())
    }

    /// A property of the current `Surface`, for tests and proofs.
    pub fn surface_property(&self, name: &str) -> Option<slint_interpreter::Value> {
        self.current.as_ref()?.surface.get_property(name).ok()
    }

    /// The native host has lost its overlay Surface. Drop the component before
    /// releasing the renderer, and reset input edges between host lifetimes.
    pub fn detach_overlay_window(&mut self) {
        if let Some(current) = self.current.as_mut() {
            if let Some((overlay, _)) = current.overlay.take() {
                // A shown Slint window retains its component until hide().
                // Retire it before the host releases the renderer/Surface.
                let _ = overlay.hide();
            }
            if let Some(windows) = self.focused.as_mut() {
                windows.remove(&(current.generation, 1));
            }
        }
        self.handle(Event::Native(vec![
            crate::input::native::NativeEvent::Disconnected,
        ]));
        self.follow_focus();
    }

    pub fn overlay_focus(&mut self, focused: bool) {
        if let Some(generation) = self.current.as_ref().map(|c| c.generation) {
            self.host(generation, 1, HostEvent::Focus(focused));
        }
    }

    /// A property of the current `Overlay`.
    pub fn overlay_property(&self, name: &str) -> Option<slint_interpreter::Value> {
        self.current
            .as_ref()?
            .overlay
            .as_ref()?
            .0
            .get_property(name)
            .ok()
    }

    fn say(&mut self, line: String) {
        eprintln!("korri-shell: {line}");
        self.log.push(line);
    }

    fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64() * 1000.0
    }

    /// Show the first package that loads, in order.
    pub fn start(&mut self) -> Result<()> {
        for index in 0..self.packages.len() {
            match self.load(index) {
                Ok(loaded) => {
                    self.current = Some(loaded);
                    let timer = slint::Timer::default();
                    let post = self.post.clone();
                    timer.start(
                        slint::TimerMode::Repeated,
                        messages::TICK_INTERVAL,
                        move || post.post(Event::Idle),
                    );
                    self.idle_timer = Some(timer);
                    return Ok(());
                }
                Err(error) => {
                    let id = self.packages[index].id.clone();
                    self.say(format!("{id} not loaded: {error:#}"));
                }
            }
        }
        Err(anyhow!("no surface package could be loaded"))
    }

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Host {
                generation,
                window,
                event,
            } => self.host(generation, window, event),
            Event::Native(events) => {
                let now = self.now();
                for event in events {
                    if let crate::input::native::NativeEvent::Input {
                        kind, code, value, ..
                    } = &event
                    {
                        korri_input_trace::record(
                            "native_ui",
                            format_args!("type={kind} code={code} value={value}"),
                        );
                    }
                    let presses = self.router.native(event, now);
                    for press in &presses {
                        korri_input_trace::record(
                            "mapper",
                            format_args!("source=pad press={press:?}"),
                        );
                    }
                    self.presses(presses);
                }
            }
            Event::Model {
                generation,
                revision,
                model,
            } => self.model(generation, revision, model),
            Event::Reconnected { generation } => {
                let Some(current) = self.current.as_ref().filter(|c| c.generation == generation)
                else {
                    return;
                };
                let id = current.package.id.clone();
                match self.korrid.model(&id) {
                    Ok(model) => self.model(generation, None, model),
                    Err(error) => self.say(format!("{id}: model after reconnect: {error}")),
                }
            }
            Event::RequestDone {
                generation,
                request,
                outcome,
            } => self.request_done(generation, request, outcome),
            Event::Tick => self.tick(),
            Event::Idle => self.idle(),
            Event::Logic {
                generation,
                outcome,
            } => self.logic_done(generation, *outcome),
        }
    }

    fn host(&mut self, generation: u64, window: u8, event: HostEvent) {
        match event {
            HostEvent::FirstFrame => {
                if let Some(current) = self.current.as_mut().filter(|c| c.generation == generation)
                {
                    current.drawn.set(true);
                    if current.timing.shown.is_zero() {
                        current.timing.shown = current.started.elapsed();
                        let (id, timing) = (current.package.id.clone(), current.timing);
                        if self.options.stats {
                            eprintln!(
                                "SHELL_STATS package={id} compile_ms={:.3} logic_ms={:.3} korrid_ms={:.3} first_frame_ms={:.3}",
                                timing.compile.as_secs_f64() * 1e3,
                                timing.logic.as_secs_f64() * 1e3,
                                timing.korrid.as_secs_f64() * 1e3,
                                timing.shown.as_secs_f64() * 1e3
                            );
                        }
                    }
                }
                self.retire_drawn();
                // A window the shell mapped anew (the Overlay, or the Surface back from
                // it) has drawn: korrid places it on the declare.
                if self
                    .current
                    .as_ref()
                    .is_some_and(|c| c.generation == generation)
                {
                    self.place_current();
                }
            }
            HostEvent::Key { key, pressed } => {
                korri_input_trace::record("key_ui", format_args!("key={key:?} pressed={pressed}"));
                let now = self.now();
                let presses = self.router.key(key, pressed, now);
                for press in &presses {
                    korri_input_trace::record("mapper", format_args!("source=key press={press:?}"));
                }
                self.presses(presses);
            }
            HostEvent::Focus(focused) => {
                let windows = self.focused.get_or_insert_with(Default::default);
                if focused {
                    windows.insert((generation, window));
                } else {
                    windows.remove(&(generation, window));
                }
                self.follow_focus();
            }
            HostEvent::Resized => {
                if self
                    .current
                    .as_ref()
                    .is_some_and(|c| c.generation == generation)
                {
                    self.resized();
                }
            }
            HostEvent::Close => {
                if let Some(quit) = &self.quit {
                    quit(Ok(()));
                }
            }
        }
    }

    /// Input is live while any shell window has focus (the portal's rule:
    /// a page without focus receives no controller input).
    fn follow_focus(&mut self) {
        let active = self
            .focused
            .as_ref()
            .is_none_or(|windows| !windows.is_empty());
        if active == self.router.active() {
            return;
        }
        let now = self.now();
        let presses = self.router.set_active(active, now);
        self.presses(presses);
        if let Some(callback) = &self.native_active {
            callback(active);
        }
    }

    fn tick(&mut self) {
        let now = self.now();
        let presses = self.router.tick(now);
        self.presses(presses);
        if !self.router.holding() {
            self.repeat = None;
        }
    }

    fn presses(&mut self, presses: Vec<Press>) {
        for press in presses {
            match press {
                // While the notice waits for a choice, only its actions switch.
                Press::Switch if self.current.is_none() => {}
                Press::Switch => self.switch(),
                Press::Button(button) => self.press(button),
            }
        }
        if self.router.holding() && self.repeat.is_none() {
            let timer = slint::Timer::default();
            let post = self.post.clone();
            timer.start(
                slint::TimerMode::Repeated,
                Duration::from_millis(10),
                move || post.post(Event::Tick),
            );
            self.repeat = Some(timer);
        }
    }

    /// Send the active surface `Tick` with the time since its last press.
    fn idle(&mut self) {
        let Some(current) = self.current.as_ref() else {
            return;
        };
        let idle = current
            .last_press
            .elapsed()
            .as_millis()
            .min(u32::MAX as u128) as u32;
        self.update(&messages::tick(idle));
    }

    pub fn press(&mut self, button: Button) {
        korri_input_trace::record("ui_apply", format_args!("button={button:?}"));
        korri_input_trace::expect_frame();
        if self.current.is_none() {
            self.notice_press(button);
            return;
        }
        if let Some(current) = self.current.as_mut() {
            current.last_press = Instant::now();
        }
        self.update(&messages::pressed(button));
        korri_input_trace::record("ui_applied", format_args!("button={button:?}"));
    }

    /// Show the next package in order. A package that does not load leaves
    /// the current surface on screen.
    pub fn switch(&mut self) {
        if self.packages.len() < 2 {
            return;
        }
        let current = self.current.as_ref().map(|c| c.package.id.clone());
        let start = current
            .as_ref()
            .and_then(|id| self.packages.iter().position(|p| &p.id == id))
            .map_or(0, |i| i + 1);
        for step in 0..self.packages.len() - 1 {
            let index = (start + step) % self.packages.len();
            if Some(&self.packages[index].id) == current.as_ref() {
                continue;
            }
            match self.load(index) {
                Ok(loaded) => {
                    self.replace(loaded);
                    return;
                }
                Err(error) => {
                    let id = self.packages[index].id.clone();
                    self.say(format!("{id} not loaded, current surface kept: {error:#}"));
                    // Its declare may already have moved korrid's placement.
                    self.redeclare();
                }
            }
        }
    }

    pub fn switch_to(&mut self, id: &str) -> Result<()> {
        let index = self
            .packages
            .iter()
            .position(|p| p.id == id)
            .with_context(|| format!("no package {id}"))?;
        let loaded = match self.load(index) {
            Ok(loaded) => loaded,
            Err(error) => {
                self.redeclare();
                return Err(error);
            }
        };
        self.replace(loaded);
        Ok(())
    }

    fn replace(&mut self, loaded: Loaded) {
        if let Some(old) = self.current.replace(loaded) {
            self.previous = Some(old.package.id.clone());
            self.retiring.push(old);
        }
        if self.backend.shared_window() {
            // One window: the new component already replaced the old one's
            // content, so the old one can go at once.
            self.retire_all();
            self.place_current();
        }
    }

    fn retire_drawn(&mut self) {
        if self.current.as_ref().is_some_and(|c| c.drawn.get()) {
            self.retire_all();
            self.drop_notice();
            self.place_current();
        }
    }

    /// Declare the current surface again once its window is the only one.
    /// The first declare ran before that window existed (preferences need
    /// it); korrid places the window on every declare.
    fn place_current(&mut self) {
        if self.current.as_ref().is_some_and(|c| !c.placed) {
            if let Some(current) = self.current.as_mut() {
                current.placed = true;
            }
            self.redeclare();
        }
    }

    /// Send the current surface's declare again with the same payload. It
    /// changes no preference; korrid places its window and restores its
    /// displays.
    fn redeclare(&mut self) {
        let Some(current) = self.current.as_ref() else {
            return;
        };
        let id = current.package.id.clone();
        if let Err(error) = self
            .korrid
            .declare(&id, &current.declaration, current.logic.displays())
        {
            self.say(format!("{id} declare again failed: {error:#}"));
        }
    }

    fn drop_notice(&mut self) {
        if let Some(shown) = self.notice.take() {
            self.hide_notice(shown);
        }
    }

    fn hide_notice(&mut self, shown: Shown) {
        if let Some(windows) = &mut self.focused {
            windows.retain(|(g, _)| *g != shown.generation);
        }
        if !self.backend.shared_window() {
            let _ = shown.component.hide();
        }
    }

    fn retire_all(&mut self) {
        for old in std::mem::take(&mut self.retiring) {
            self.unload(old);
        }
    }

    fn unload(&mut self, loaded: Loaded) {
        let Loaded {
            generation,
            surface,
            overlay,
            watch,
            logic,
            images,
            ..
        } = loaded;
        if let Some(windows) = &mut self.focused {
            windows.retain(|(g, _)| *g != generation);
        }
        drop(watch);
        if !self.backend.shared_window() {
            let _ = surface.hide();
            if let Some((overlay, _)) = &overlay {
                let _ = overlay.hide();
            }
        }
        drop(overlay);
        drop(surface);
        // Interrupts a running call and joins the logic thread.
        drop(logic);
        drop(images);
        crate::memory::release_free_heap();
    }

    fn load(&mut self, index: usize) -> Result<Loaded> {
        let package = self.packages[index].clone();
        let started = Instant::now();
        let views = match self.compiled.get(&package.id) {
            Some(views) => views.clone(),
            None => {
                let views = package::compile(&package)?;
                self.compilations += 1;
                self.compiled.insert(package.id.clone(), views.clone());
                views
            }
        };
        let compiled = started.elapsed();
        let logic = Logic::start(package.logic_source()?, self.options.limits, &package.id)
            .map_err(|v| anyhow!("logic: {v}"))?;
        let logic_loaded = started.elapsed();
        let declaration =
            serde_json::to_value(logic.declaration()).expect("declaration serialises");
        self.korrid
            .declare(&package.id, &declaration, logic.displays())
            .context("app.surface.declare")?;
        let stored = self
            .korrid
            .preferences(&package.id)
            .context("app.surface.preferences")?;
        let preferences = engine::effective_preferences(logic.declaration(), &stored);
        let model = self
            .korrid
            .model(&package.id)
            .context("app.surface.model")?;
        let korrid_done = started.elapsed();
        let gameplay_overlay = messages::is_gameplay_overlay(&serde_json::from_str(&model)?);
        self.next_generation += 1;
        let generation = self.next_generation;
        let mut images = ImageCache::new(&package.dir);
        let surface = views.surface.create()?;
        let mut surface_binder = Binder::new(&views.surface);
        let drawn = Rc::new(Cell::new(false));
        self.attach(surface.window(), generation, 0);
        self.backend.place(surface.window(), self.options.placement);
        // A2 (8.1): the window's size, once after init and before the first view.
        let size = logical_size(surface.window());
        let overlay_wanted = gameplay_overlay && views.overlay.is_some() && logic.has_overlay();
        let (first_requests, view, overlay_view) = logic
            .now(move |worker| -> std::result::Result<_, Violation> {
                let engine = worker.engine();
                engine.set_model(&model)?;
                engine.init(&preferences)?;
                let requests = engine.update(&messages::resized(size.0, size.1))?;
                engine.views_drawn();
                let view = engine.view()?;
                let overlay = if overlay_wanted {
                    engine.overlay()?
                } else {
                    None
                };
                let diff = worker.diff_view(&view);
                let overlay = overlay.map(|o| {
                    let diff = worker.diff_overlay(&o);
                    (o, diff)
                });
                Ok((requests, (view, diff), overlay))
            })
            .and_then(|r| r)
            .map_err(|v| anyhow!("logic: {v}"))?;
        for request in &first_requests {
            check_request(request, logic.declaration()).map_err(|f| anyhow!("{f}"))?;
        }
        let logic_done = started.elapsed();
        let applied = surface_binder.apply_diff(&surface, &view.0, &view.1, &mut images);
        if let Err(error) = applied {
            drop(surface);
            return Err(error.context("view"));
        }
        let post = self.post.clone();
        let watch = self.korrid.watch(
            &package.id,
            Box::new(move |event| {
                post.post(match event {
                    WatchEvent::Model { revision, model } => Event::Model {
                        generation,
                        revision: Some(revision),
                        model,
                    },
                    WatchEvent::Reconnected => Event::Reconnected { generation },
                })
            }),
        );
        let done: Done = {
            let post = self.post.clone();
            Arc::new(move |outcome| {
                post.post(Event::Logic {
                    generation,
                    outcome: Box::new(outcome),
                })
            })
        };
        let mut loaded = Loaded {
            generation,
            last_press: Instant::now(),
            size,
            package,
            logic,
            done,
            overlay_definition: views.overlay,
            surface,
            surface_binder,
            overlay: None,
            images,
            watch: Some(watch),
            drawn,
            gameplay_overlay,
            surface_shown: false,
            declaration,
            placed: false,
            started,
            timing: LoadTiming {
                compile: compiled,
                logic: (logic_loaded - compiled) + (logic_done - korrid_done),
                korrid: korrid_done - logic_loaded,
                shown: Duration::ZERO,
            },
        };
        if let Err(error) = self.render_overlay(&mut loaded, overlay_view) {
            let id = loaded.package.id.clone();
            self.unload(loaded);
            return Err(anyhow!("{id} overlay: {error:#}"));
        }
        for request in first_requests {
            let _ = self.jobs.send(RequestJob {
                generation,
                surface: loaded.package.id.clone(),
                request,
            });
        }
        Ok(loaded)
    }

    /// Send `Resized` when the mapped window's logical size changed.
    fn resized(&mut self) {
        let Some(current) = self.current.as_mut() else {
            return;
        };
        let window = match &current.overlay {
            Some((overlay, _)) if !current.surface_shown => overlay.window(),
            _ => current.surface.window(),
        };
        let size = logical_size(window);
        if size == current.size || size.0 <= 0.0 || size.1 <= 0.0 {
            return;
        }
        current.size = size;
        self.update(&messages::resized(size.0, size.1));
    }

    /// Resize the current surface's window, as a compositor would (tests).
    #[doc(hidden)]
    pub fn resize_window(&mut self, width: u32, height: u32) {
        if let Some(current) = self.current.as_ref() {
            current
                .surface
                .window()
                .set_size(slint::PhysicalSize::new(width, height));
        }
        self.resized();
    }

    /// `loaded`'s window `role` (0 Surface, 1 Overlay) was mapped anew: it needs placing,
    /// and its next frame is a first frame (`HostEvent::FirstFrame`), which declares again.
    fn remap(&self, loaded: &mut Loaded, role: u8) {
        loaded.placed = false;
        if role == 0 {
            self.attach(loaded.surface.window(), loaded.generation, 0);
        } else if let Some((overlay, _)) = &loaded.overlay {
            self.attach(overlay.window(), loaded.generation, 1);
        }
    }

    fn attach(&self, window: &slint::Window, generation: u64, role: u8) {
        let post = self.post.clone();
        self.backend.attach(
            window,
            Rc::new(move |event| {
                post.post(Event::Host {
                    generation,
                    window: role,
                    event,
                })
            }),
            self.frames.clone(),
        );
    }

    /// Show, refresh or remove the Overlay to match the presentation, and
    /// map exactly one of Surface and Overlay.
    /// `view`: the overlay's view when the logic gave a new one.
    fn render_overlay(&mut self, loaded: &mut Loaded, view: Option<(Value, Diff)>) -> Result<()> {
        let wanted = loaded.gameplay_overlay
            && loaded.overlay_definition.is_some()
            && loaded.logic.has_overlay();
        if !wanted {
            if let Some((overlay, _)) = loaded.overlay.take() {
                if let Some(windows) = &mut self.focused {
                    windows.remove(&(loaded.generation, 1));
                }
                if !self.backend.shared_window() {
                    overlay.hide()?;
                }
                // A shared window shows whichever component showed last.
                loaded.surface_shown = false;
            }
            if !loaded.surface_shown {
                loaded.surface.show()?;
                loaded.surface_shown = true;
                if !self.backend.shared_window() {
                    // Mapped anew: declare again once it has drawn.
                    self.remap(loaded, 0);
                }
            }
            return Ok(());
        }
        if !self.backend.overlay_available() {
            return Ok(());
        }
        let Some((view, diff)) = view else {
            // No new overlay view: the one shown stays (the logic sends one with every
            // change, and with the model that asks for the overlay).
            return Ok(());
        };
        let created = loaded.overlay.is_none();
        if created {
            let definition = loaded.overlay_definition.as_ref().expect("checked");
            let component = self.backend.create_overlay(definition)?;
            self.attach(component.window(), loaded.generation, 1);
            self.backend
                .place(component.window(), self.options.placement);
            let binder = Binder::new(definition);
            loaded.overlay = Some((component, binder));
        }
        if created && !self.backend.shared_window() {
            // A new mapped window: korrid places it on a declare sent after it has drawn.
            loaded.placed = false;
        }
        let (component, binder) = loaded.overlay.as_mut().expect("just created");
        binder.apply_diff(component, &view, &diff, &mut loaded.images)?;
        component.show()?;
        if loaded.surface_shown {
            if !self.backend.shared_window() {
                loaded.surface.hide()?;
                if let Some(windows) = &mut self.focused {
                    windows.remove(&(loaded.generation, 0));
                }
            }
            loaded.surface_shown = false;
        }
        Ok(())
    }

    /// Send a message to the logic; its outcome comes back as `Event::Logic`.
    fn update(&mut self, message: &Value) {
        self.call(Call::Message(message.clone()));
    }

    fn call(&mut self, call: Call) {
        if let Some(current) = self.current.as_mut() {
            let done = current.done.clone();
            current.logic.submit(call, &done);
        }
    }

    /// A logic call returned: apply its views, acknowledge its model, send its requests,
    /// and start the next call.
    fn logic_done(&mut self, generation: u64, outcome: Outcome) {
        if self
            .current
            .as_ref()
            .is_none_or(|c| c.generation != generation)
        {
            return; // a retired surface's logic
        }
        let Some(mut current) = self.current.take() else {
            return;
        };
        let revision = outcome.revision.clone();
        if let Ok(step) = &outcome.result {
            if let Some((call, took)) = step.slowest {
                let id = current.package.id.clone();
                self.slow_call(&id, &step.message, call, took);
            }
        }
        let applied = (|| -> std::result::Result<Vec<Value>, Failure> {
            let step = outcome.result?;
            for request in &step.requests {
                check_request(request, current.logic.declaration())?;
            }
            if let Some(gameplay) = step.gameplay_overlay {
                current.gameplay_overlay = gameplay;
            }
            if let Some((view, diff)) = &step.view {
                current
                    .surface_binder
                    .apply_diff(&current.surface, view, diff, &mut current.images)
                    .map_err(Failure::View)?;
            }
            if step.view.is_some() || step.gameplay_overlay.is_some() {
                self.render_overlay(&mut current, step.overlay)
                    .map_err(Failure::View)?;
            }
            Ok(step.requests)
        })();
        match applied {
            Ok(requests) => {
                for request in requests {
                    let _ = self.jobs.send(RequestJob {
                        generation: current.generation,
                        surface: current.package.id.clone(),
                        request,
                    });
                }
                if let (Some(revision), Some(watch)) = (revision, &current.watch) {
                    watch.ack(&revision);
                }
                let done = current.done.clone();
                current.logic.returned(&done);
                self.current = Some(current);
            }
            Err(failure) => self.fail(current, failure),
        }
    }

    fn model(&mut self, generation: u64, revision: Option<String>, model: String) {
        if self
            .current
            .as_ref()
            .is_none_or(|c| c.generation != generation)
        {
            return; // a retired surface's watch
        }
        self.call(Call::Model { model, revision });
    }

    fn request_done(
        &mut self,
        generation: u64,
        request: Value,
        outcome: std::result::Result<(), String>,
    ) {
        let tag = messages::request_tag(&request).unwrap_or("?").to_owned();
        if let Err(error) = &outcome {
            self.say(format!("request {tag}: {error}"));
        }
        if self
            .current
            .as_ref()
            .is_none_or(|c| c.generation != generation)
        {
            return;
        }
        if outcome.is_ok() {
            if let Some((key, _)) = messages::remembered(&request) {
                let message = messages::preference_stored(&key);
                self.update(&message);
            }
        }
    }

    /// A5: a call over the budget is a performance defect. Log it, at most one line per
    /// surface and message type each `SLOW_LOG_INTERVAL`; the surface keeps running.
    fn slow_call(&mut self, id: &str, message: &str, call: &str, took: Duration) {
        let budget = self.options.limits.call;
        if took <= budget {
            return;
        }
        let key = (id.to_owned(), message.to_owned());
        let line = match self.slow.get_mut(&key) {
            None => {
                self.slow.insert(
                    key,
                    SlowCalls {
                        logged: Instant::now(),
                        since: 0,
                        slowest: Duration::ZERO,
                    },
                );
                format!(
                    "performance defect: {id} {call} for {message} took {} ms, over the {} ms call budget; {id} keeps running",
                    took.as_millis(),
                    budget.as_millis()
                )
            }
            Some(slow) => {
                slow.since += 1;
                slow.slowest = slow.slowest.max(took);
                if slow.logged.elapsed() < SLOW_LOG_INTERVAL {
                    return;
                }
                let line = format!(
                    "performance defect: {id} had {} more {message} calls over the {} ms call budget in {} s, the slowest {} ms; {id} keeps running",
                    slow.since,
                    budget.as_millis(),
                    slow.logged.elapsed().as_secs(),
                    slow.slowest.as_millis()
                );
                *slow = SlowCalls {
                    logged: Instant::now(),
                    since: 0,
                    slowest: Duration::ZERO,
                };
                line
            }
        };
        self.say(line);
    }

    /// A5: unload a surface that cannot work and show the notice. The shell does not
    /// switch: the person chooses to try it again or use another surface.
    fn fail(&mut self, failed: Loaded, failure: Failure) {
        let id = failed.package.id.clone();
        let kind = match &failure {
            Failure::Logic(v) => v.kind(),
            Failure::Request(_) => "contract",
            Failure::View(_) => "view",
        };
        let reason = match &failure {
            Failure::Logic(Violation::Time { .. }) => Reason::Stuck,
            Failure::Logic(Violation::Memory { .. }) => Reason::Memory,
            _ => Reason::Error,
        };
        self.say(format!("{id} unloaded ({kind}): {failure}"));
        let previous = self.previous.clone();
        // The notice's window is up before the failed one goes.
        self.show_notice(&id, reason, previous.as_deref());
        self.unload(failed);
        // Surfaces still retiring from an earlier switch go too.
        self.retire_all();
    }

    /// Show the notice for `surface`, offering `preferred` (or else the first other
    /// package) as the other surface.
    fn show_notice(&mut self, surface: &str, reason: Reason, preferred: Option<&str>) {
        let other = preferred
            .filter(|p| *p != surface && self.packages.iter().any(|q| q.id == *p))
            .map(str::to_owned)
            .or_else(|| {
                self.packages
                    .iter()
                    .find(|p| p.id != surface)
                    .map(|p| p.id.clone())
            });
        let notice = Notice::new(surface, reason, other.as_deref());
        self.say(format!(
            "notice: {}; {}",
            notice.heading(),
            notice
                .actions()
                .iter()
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if let Err(error) = self.draw_notice(notice) {
            if let Some(quit) = &self.quit {
                quit(Err(format!(
                    "{surface} was unloaded and the notice failed: {error:#}"
                )));
            }
        }
    }

    fn draw_notice(&mut self, notice: Notice) -> Result<()> {
        let definition = match &self.notice_definition {
            Some(definition) => definition.clone(),
            None => {
                let definition = notice::compile()?;
                self.notice_definition = Some(definition.clone());
                definition
            }
        };
        let component = definition.create()?;
        notice::bind(&component, &notice)?;
        self.next_generation += 1;
        self.attach(component.window(), self.next_generation, 0);
        self.backend
            .place(component.window(), self.options.placement);
        component.show()?;
        let generation = self.next_generation;
        let old = self.notice.replace(Shown {
            notice,
            component,
            generation,
        });
        if let Some(old) = old {
            self.hide_notice(old);
        }
        Ok(())
    }

    /// A press while the notice waits: move its focus, or act on the person's choice.
    fn notice_press(&mut self, button: Button) {
        let Some(shown) = self.notice.as_mut() else {
            return;
        };
        let choice = shown.notice.press(button);
        let _ = notice::bind(&shown.component, &shown.notice);
        if let Some(choice) = choice {
            self.choose(choice);
        }
    }

    /// Load the surface the person chose. When it does not load, the notice says so.
    fn choose(&mut self, choice: Choice) {
        let Some(failed) = self.notice.as_ref().map(|s| s.notice.surface.clone()) else {
            return;
        };
        let (id, other) = match &choice {
            Choice::Retry(id) => (
                id.clone(),
                self.notice.as_ref().and_then(|s| s.notice.other.clone()),
            ),
            Choice::Use(id) => (id.clone(), Some(failed.clone())),
        };
        self.say(format!(
            "notice: the person chose {}",
            match &choice {
                Choice::Retry(id) => format!("to try {id} again"),
                Choice::Use(id) => format!("to use {id}"),
            }
        ));
        let Some(index) = self.packages.iter().position(|p| p.id == id) else {
            return;
        };
        match self.load(index) {
            Ok(loaded) => {
                self.previous = (id != failed).then_some(failed);
                self.current = Some(loaded);
                if self.backend.shared_window() {
                    self.drop_notice();
                    self.place_current();
                }
                // Otherwise the notice goes after the surface's first frame (`retire_drawn`).
            }
            Err(error) => {
                self.say(format!("{id} not loaded: {error:#}"));
                self.show_notice(&id, Reason::NotStarted, other.as_deref());
            }
        }
    }
}

#[derive(Debug)]
enum Failure {
    Logic(Violation),
    Request(String),
    View(anyhow::Error),
}

impl From<Violation> for Failure {
    fn from(violation: Violation) -> Self {
        Self::Logic(violation)
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Logic(v) => write!(f, "{v}"),
            Self::Request(r) => write!(f, "{r}"),
            Self::View(e) => write!(f, "{e:#}"),
        }
    }
}

/// The shell's own checks on a request before korrid sees it: a treaty
/// request, a declared preference within 256 bytes, a catalog count of 1..=200.
fn check_request(
    request: &Value,
    declaration: &engine::Declaration,
) -> std::result::Result<(), Failure> {
    use korri_surface_treaty::SurfaceRequest;
    match messages::request(request).map_err(Failure::Request)? {
        SurfaceRequest::RememberPreference { key, value } => {
            if !declaration.contains_key(&key) {
                return Err(Failure::Request(format!(
                    "RememberPreference of undeclared key {key:?}"
                )));
            }
            if value.len() > engine::MAX_PREFERENCE_BYTES {
                return Err(Failure::Request(format!(
                    "RememberPreference value for {key} is over 256 bytes"
                )));
            }
        }
        SurfaceRequest::MoveCatalog { count, .. }
            if count == 0 || u64::from(count) > messages::MAX_CATALOG_COUNT =>
        {
            return Err(Failure::Request("MoveCatalog count must be 1..=200".into()));
        }
        _ => {}
    }
    Ok(())
}

/// A window's size in logical pixels, the units `.slint` lays out in.
fn logical_size(window: &slint::Window) -> (f32, f32) {
    let size = window.size().to_logical(window.scale_factor());
    (size.width, size.height)
}
