//! The shell: loads verified surface packages, drives their logic with
//! korrid's model and the person's input, and switches between them.
//!
//! Every event reaches [`Shell::handle`] on the UI thread through a
//! [`Post`]. Nothing here re-enters itself from a window callback.
//!
//! Lifecycle of a surface (contract 3 and 6):
//! 1. compile `main.slint` (`Surface`, and `Overlay` when exported)
//! 2. evaluate `logic.js` in a fresh engine and read its preferences
//! 3. `app.surface.declare`, then `app.surface.preferences`
//! 4. `app.surface.model`, `init`, `view`, create and show `Surface`
//! 5. watch the model; each push runs `update(KorriPublished)` then acks
//!
//! A failure in steps 1 to 4 keeps the surface already shown. A logic
//! violation at run time unloads the surface and restores the previous one.

use crate::{
    backend::{Backend, FrameCounter, HostEvent, Placement},
    engine::{self, Engine, Limits, Violation},
    images::ImageCache,
    input::{native::NativeEvent, Press, Router},
    korrid::{Korrid, WatchEvent},
    messages::{self, Button},
    package::{self, Package},
    view::Binder,
};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use slint::ComponentHandle;
use slint_interpreter::{ComponentDefinition, ComponentInstance};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

/// Everything the shell reacts to.
#[derive(Debug)]
pub enum Event {
    /// `window`: 0 for a surface's `Surface`, 1 for its `Overlay`.
    Host { generation: u64, window: u8, event: HostEvent },
    Native(NativeEvent),
    Model { generation: u64, revision: Option<String>, model: String },
    Reconnected { generation: u64 },
    RequestDone { generation: u64, request: Value, outcome: std::result::Result<(), String> },
    /// Repeat timer for held directions.
    Tick,
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
            placement: Placement::Windowed { width: 640, height: 480 },
            stats: false,
        }
    }
}

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
    package: Package,
    engine: Engine,
    overlay_definition: Option<ComponentDefinition>,
    surface: ComponentInstance,
    surface_binder: Binder,
    overlay: Option<(ComponentInstance, Binder)>,
    images: ImageCache,
    watch: Option<crate::korrid::Watch>,
    drawn: Rc<Cell<bool>>,
    gameplay_overlay: bool,
    started: Instant,
    timing: LoadTiming,
}

struct RequestJob {
    generation: u64,
    surface: String,
    request: Value,
}

pub struct Shell {
    packages: Vec<Package>,
    korrid: Korrid,
    backend: Rc<dyn Backend>,
    post: Arc<dyn Post>,
    options: Options,
    router: Router,
    current: Option<Loaded>,
    retiring: Vec<Loaded>,
    previous: Option<String>,
    next_generation: u64,
    jobs: mpsc::Sender<RequestJob>,
    clock: Instant,
    frames: FrameCounter,
    repeat: Option<slint::Timer>,
    native_active: Option<Box<dyn Fn(bool)>>,
    /// Shell windows that have keyboard focus; `None` until the first focus event.
    focused: Option<std::collections::HashSet<(u64, u8)>>,
    quit: Option<Box<dyn Fn(Result<(), String>)>>,
    log: Vec<String>,
}

impl Shell {
    pub fn new(packages: Vec<Package>, korrid: Korrid, backend: Rc<dyn Backend>, post: Arc<dyn Post>, options: Options) -> Self {
        let (jobs, receiver) = mpsc::channel::<RequestJob>();
        let worker_korrid = korrid.clone();
        let worker_post = post.clone();
        // Requests go out in order on one thread; their consequences come
        // back as new models.
        thread::Builder::new()
            .name("korrid-requests".into())
            .spawn(move || {
                for job in receiver {
                    let outcome = worker_korrid.request(&job.surface, &job.request).map_err(|e| e.to_string());
                    worker_post.post(Event::RequestDone { generation: job.generation, request: job.request, outcome });
                }
            })
            .expect("spawn the request thread");
        Self {
            packages,
            korrid,
            backend,
            post,
            options,
            router: Router::new(true),
            current: None,
            retiring: Vec::new(),
            previous: None,
            next_generation: 0,
            jobs,
            clock: Instant::now(),
            frames: FrameCounter::default(),
            repeat: None,
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
    pub fn on_quit(&mut self, callback: Box<dyn Fn(Result<(), String>)>) {
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

    /// Lines the shell logged, for tests.
    pub fn log(&self) -> &[String] {
        &self.log
    }

    pub fn overlay_shown(&self) -> bool {
        self.current.as_ref().is_some_and(|l| l.overlay.is_some())
    }

    pub fn retiring(&self) -> usize {
        self.retiring.len()
    }

    /// The current surface's QuickJS heap, in bytes.
    pub fn heap_bytes(&self) -> usize {
        self.current.as_ref().map_or(0, |l| l.engine.heap_bytes())
    }

    /// A property of the current `Surface`, for tests and proofs.
    pub fn surface_property(&self, name: &str) -> Option<slint_interpreter::Value> {
        self.current.as_ref()?.surface.get_property(name).ok()
    }

    /// A property of the current `Overlay`.
    pub fn overlay_property(&self, name: &str) -> Option<slint_interpreter::Value> {
        self.current.as_ref()?.overlay.as_ref()?.0.get_property(name).ok()
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
            Event::Host { generation, window, event } => self.host(generation, window, event),
            Event::Native(event) => {
                let now = self.now();
                let presses = self.router.native(event, now);
                self.presses(presses);
            }
            Event::Model { generation, revision, model } => self.model(generation, revision, model),
            Event::Reconnected { generation } => {
                let Some(current) = self.current.as_ref().filter(|c| c.generation == generation) else { return };
                let id = current.package.id.clone();
                match self.korrid.model(&id) {
                    Ok(model) => self.model(generation, None, model),
                    Err(error) => self.say(format!("{id}: model after reconnect: {error}")),
                }
            }
            Event::RequestDone { generation, request, outcome } => self.request_done(generation, request, outcome),
            Event::Tick => self.tick(),
        }
    }

    fn host(&mut self, generation: u64, window: u8, event: HostEvent) {
        match event {
            HostEvent::FirstFrame => {
                if let Some(current) = self.current.as_mut().filter(|c| c.generation == generation) {
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
            }
            HostEvent::Key { key, pressed } => {
                let now = self.now();
                let presses = self.router.key(key, pressed, now);
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
        let active = self.focused.as_ref().is_none_or(|windows| !windows.is_empty());
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
                Press::Switch => self.switch(),
                Press::Button(button) => self.press(button),
            }
        }
        if self.router.holding() && self.repeat.is_none() {
            let timer = slint::Timer::default();
            let post = self.post.clone();
            timer.start(slint::TimerMode::Repeated, Duration::from_millis(10), move || post.post(Event::Tick));
            self.repeat = Some(timer);
        }
    }

    pub fn press(&mut self, button: Button) {
        self.update(&messages::pressed(button));
    }

    /// Show the next package in order. A package that does not load leaves
    /// the current surface on screen.
    pub fn switch(&mut self) {
        if self.packages.len() < 2 {
            return;
        }
        let current = self.current.as_ref().map(|c| c.package.id.clone());
        let start = current.as_ref().and_then(|id| self.packages.iter().position(|p| &p.id == id)).map_or(0, |i| i + 1);
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
                }
            }
        }
    }

    pub fn switch_to(&mut self, id: &str) -> Result<()> {
        let index = self.packages.iter().position(|p| p.id == id).with_context(|| format!("no package {id}"))?;
        let loaded = self.load(index)?;
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
        }
    }

    fn retire_drawn(&mut self) {
        if self.current.as_ref().is_some_and(|c| c.drawn.get()) {
            self.retire_all();
        }
    }

    fn retire_all(&mut self) {
        for old in std::mem::take(&mut self.retiring) {
            self.unload(old);
        }
    }

    fn unload(&mut self, loaded: Loaded) {
        let Loaded { generation, surface, overlay, watch, engine, images, .. } = loaded;
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
        drop(engine);
        drop(images);
    }

    fn load(&mut self, index: usize) -> Result<Loaded> {
        let package = self.packages[index].clone();
        let started = Instant::now();
        let views = package::compile(&package)?;
        let compiled = started.elapsed();
        let mut engine = Engine::load(&package.logic_source()?, self.options.limits).map_err(|v| anyhow!("logic: {v}"))?;
        let logic_loaded = started.elapsed();
        let declaration = serde_json::to_value(engine.declaration()).expect("declaration serialises");
        self.korrid.declare(&package.id, &declaration).context("app.surface.declare")?;
        let stored = self.korrid.preferences(&package.id).context("app.surface.preferences")?;
        let preferences = engine::effective_preferences(engine.declaration(), &stored);
        let model = self.korrid.model(&package.id).context("app.surface.model")?;
        let korrid_done = started.elapsed();
        let gameplay_overlay = messages::is_gameplay_overlay(&serde_json::from_str(&model)?);
        engine.set_model(&model).map_err(|v| anyhow!("logic: {v}"))?;
        engine.init(&preferences).map_err(|v| anyhow!("logic: {v}"))?;
        let view = engine.view().map_err(|v| anyhow!("logic: {v}"))?;
        let logic_done = started.elapsed();

        self.next_generation += 1;
        let generation = self.next_generation;
        let mut images = ImageCache::new(&package.dir);
        let surface = views.surface.create()?;
        let mut surface_binder = Binder::new(&views.surface);
        let drawn = Rc::new(Cell::new(false));
        self.attach(surface.window(), generation, 0);
        let applied = surface_binder.apply(&surface, &view, &mut images);
        if let Err(error) = applied {
            drop(surface);
            return Err(error.context("view"));
        }
        self.backend.place(surface.window(), self.options.placement);
        surface.show()?;
        let post = self.post.clone();
        let watch = self.korrid.watch(
            &package.id,
            Box::new(move |event| {
                post.post(match event {
                    WatchEvent::Model { revision, model } => Event::Model { generation, revision: Some(revision), model },
                    WatchEvent::Reconnected => Event::Reconnected { generation },
                })
            }),
        );
        let mut loaded = Loaded {
            generation,
            package,
            engine,
            overlay_definition: views.overlay,
            surface,
            surface_binder,
            overlay: None,
            images,
            watch: Some(watch),
            drawn,
            gameplay_overlay,
            started,
            timing: LoadTiming {
                compile: compiled,
                logic: (logic_loaded - compiled) + (logic_done - korrid_done),
                korrid: korrid_done - logic_loaded,
                shown: Duration::ZERO,
            },
        };
        if let Err(error) = self.render_overlay(&mut loaded) {
            let id = loaded.package.id.clone();
            self.unload(loaded);
            return Err(anyhow!("{id} overlay: {error:#}"));
        }
        Ok(loaded)
    }

    fn attach(&self, window: &slint::Window, generation: u64, role: u8) {
        let post = self.post.clone();
        self.backend.attach(window, Rc::new(move |event| post.post(Event::Host { generation, window: role, event })), self.frames.clone());
    }

    /// Show, refresh or remove the Overlay to match the presentation.
    fn render_overlay(&mut self, loaded: &mut Loaded) -> Result<()> {
        let wanted = loaded.gameplay_overlay && loaded.overlay_definition.is_some() && loaded.engine.has_overlay();
        if !wanted {
            if let Some((overlay, _)) = loaded.overlay.take() {
                if let Some(windows) = &mut self.focused {
                    windows.remove(&(loaded.generation, 1));
                }
                if !self.backend.shared_window() {
                    overlay.hide()?;
                } else {
                    loaded.surface.show()?;
                }
            }
            return Ok(());
        }
        let view = loaded.engine.overlay().map_err(|v| anyhow!("{v}"))?.context("overlay")?;
        if loaded.overlay.is_none() {
            let definition = loaded.overlay_definition.as_ref().expect("checked");
            let component = definition.create()?;
            self.attach(component.window(), loaded.generation, 1);
            self.backend.place(component.window(), self.options.placement);
            let binder = Binder::new(definition);
            loaded.overlay = Some((component, binder));
        }
        let (component, binder) = loaded.overlay.as_mut().expect("just created");
        binder.apply(component, &view, &mut loaded.images)?;
        component.show()?;
        Ok(())
    }

    /// Run `update`, render, and send its requests.
    fn update(&mut self, message: &Value) {
        let Some(mut current) = self.current.take() else { return };
        let outcome = (|| -> std::result::Result<Vec<Value>, Failure> {
            let requests = current.engine.update(message)?;
            for request in &requests {
                check_request(request, current.engine.declaration())?;
            }
            self.render(&mut current)?;
            Ok(requests)
        })();
        match outcome {
            Ok(requests) => {
                for request in requests {
                    let _ = self.jobs.send(RequestJob { generation: current.generation, surface: current.package.id.clone(), request });
                }
                self.current = Some(current);
            }
            Err(failure) => self.fail(current, failure),
        }
    }

    fn render(&mut self, current: &mut Loaded) -> std::result::Result<(), Failure> {
        let view = current.engine.view()?;
        current.surface_binder.apply(&current.surface, &view, &mut current.images).map_err(Failure::View)?;
        self.render_overlay(current).map_err(Failure::View)?;
        Ok(())
    }

    fn model(&mut self, generation: u64, revision: Option<String>, model: String) {
        if self.current.as_ref().is_none_or(|c| c.generation != generation) {
            return; // a retired surface's watch
        }
        let gameplay_overlay = serde_json::from_str::<Value>(&model).map(|m| messages::is_gameplay_overlay(&m));
        let Some(mut current) = self.current.take() else { return };
        current.gameplay_overlay = gameplay_overlay.unwrap_or(false);
        match current.engine.set_model(&model) {
            Ok(()) => self.current = Some(current),
            Err(violation) => {
                self.fail(current, Failure::Logic(violation));
                return;
            }
        }
        self.update(&messages::korri_published());
        if let (Some(revision), Some(current)) = (revision, self.current.as_ref().filter(|c| c.generation == generation)) {
            if let Some(watch) = &current.watch {
                watch.ack(&revision);
            }
        }
    }

    fn request_done(&mut self, generation: u64, request: Value, outcome: std::result::Result<(), String>) {
        let tag = messages::request_tag(&request).unwrap_or("?").to_owned();
        if let Err(error) = &outcome {
            self.say(format!("request {tag}: {error}"));
        }
        if self.current.as_ref().is_none_or(|c| c.generation != generation) {
            return;
        }
        if outcome.is_ok() {
            if let Some((key, _)) = messages::remembered(&request) {
                let message = messages::preference_stored(key);
                self.update(&message);
            }
        }
    }

    /// Unload a surface that broke a limit and restore the previous one.
    fn fail(&mut self, failed: Loaded, failure: Failure) {
        let id = failed.package.id.clone();
        let kind = match &failure {
            Failure::Logic(v) => v.kind(),
            Failure::Request(_) => "contract",
            Failure::View(_) => "view",
        };
        self.say(format!("{id} unloaded ({kind}): {failure}"));
        self.unload(failed);
        let mut candidates: Vec<usize> = Vec::new();
        if let Some(previous) = self.previous.take() {
            if let Some(index) = self.packages.iter().position(|p| p.id == previous) {
                candidates.push(index);
            }
        }
        candidates.extend((0..self.packages.len()).filter(|&i| self.packages[i].id != id));
        candidates.dedup();
        for index in candidates {
            if self.packages[index].id == id {
                continue;
            }
            match self.load(index) {
                Ok(loaded) => {
                    let restored = loaded.package.id.clone();
                    self.current = Some(loaded);
                    self.say(format!("{restored} restored after {id} was unloaded"));
                    self.retire_all();
                    return;
                }
                Err(error) => {
                    let other = self.packages[index].id.clone();
                    self.say(format!("{other} not loaded: {error:#}"));
                }
            }
        }
        if let Some(quit) = &self.quit {
            quit(Err(format!("{id} was unloaded and no other surface loads")));
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

/// The shell's own checks on a request before korrid sees it.
fn check_request(request: &Value, declaration: &engine::Declaration) -> std::result::Result<(), Failure> {
    let tag = messages::request_tag(request).ok_or_else(|| Failure::Request("update returned a request that is not a SurfaceRequest".into()))?;
    if let Some((key, value)) = messages::remembered(request) {
        if !declaration.contains_key(key) {
            return Err(Failure::Request(format!("RememberPreference of undeclared key {key:?}")));
        }
        if value.len() > engine::MAX_PREFERENCE_BYTES {
            return Err(Failure::Request(format!("RememberPreference value for {key} is over 256 bytes")));
        }
    } else if tag == "RememberPreference" {
        return Err(Failure::Request("RememberPreference needs string key and value".into()));
    }
    if tag == "MoveCatalog" && request.get("count").and_then(Value::as_u64).is_none_or(|c| c == 0 || c > messages::MAX_CATALOG_COUNT) {
        return Err(Failure::Request("MoveCatalog count must be 1..=200".into()));
    }
    Ok(())
}
