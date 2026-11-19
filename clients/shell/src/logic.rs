//! A surface's logic on its own thread, so a long `update` or `view()` never
//! holds up a frame.
//!
//! The QuickJS engine ([`Engine`]) is created and runs on one thread per
//! surface. The UI thread sends it [`Call`]s and gets back an [`Outcome`] each:
//! the requests, and the views when the state or model changed. One call runs
//! at a time, in the order sent; a `Tick` waiting behind a running call is
//! replaced by a newer one. The stuck limit and the load budget (2 s each)
//! are the engine's own, enforced on that thread; each [`Step`] carries its
//! slowest engine call, which the shell compares with the 50 ms call budget.
//! Dropping a [`Logic`] interrupts a running call and joins the thread.
//!
//! Loading stays synchronous ([`Logic::now`]): the shell needs the
//! declaration, the first model's result and the first view before it shows
//! a surface.

use crate::{
    engine::{Declaration, Engine, Limits, Panes, Violation},
    messages,
    view::{Diff, Differ},
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

/// What the UI thread asks of the logic.
#[derive(Clone, Debug)]
pub enum Call {
    /// `update(message)`: a press, a tick, a resize, a stored preference.
    Message(Value),
    /// A new model from korrid: `setModel`, then `update(KorriPublished)`.
    Model {
        model: String,
        revision: Option<String>,
    },
}

impl Call {
    fn is_tick(&self) -> bool {
        matches!(self, Call::Message(m) if m.get("_tag").and_then(Value::as_str) == Some("Tick"))
    }
}

/// What one call gave back.
#[derive(Debug)]
pub struct Step {
    pub requests: Vec<Value>,
    /// `view()`, when the state or the model changed, and how it differs from the view
    /// before it (`view::Binder::apply_diff`).
    pub view: Option<(Value, Diff)>,
    /// `overlay()`, when it changed and the presentation is the gameplay overlay, and its
    /// diff against the overlay view before it.
    pub overlay: Option<(Value, Diff)>,
    /// Whether the model asks for the gameplay overlay (after a `Model` call).
    pub gameplay_overlay: Option<bool>,
    /// ADR 0086: the panes each window holds (after a `Model` call), from
    /// `SurfaceModel.windows`; one window when the model has none.
    pub windows: Option<Vec<Vec<String>>>,
    /// `view(state, model, window)` of each further window, by index from 1,
    /// when the views were drawn again, and how each differs from that
    /// window's view before.
    pub panes: Vec<(u32, Value, Diff)>,
    /// Selected-clock time of the call on the logic thread.
    pub took: Duration,
    /// The message's `_tag` (`KorriPublished` for a model).
    pub message: String,
    /// The slowest engine call in this step (`update`, `view`, `overlay`, `setModel`).
    pub slowest: Option<(&'static str, Duration)>,
}

#[derive(Debug)]
pub struct Outcome {
    /// The model revision a `Model` call carried, to acknowledge once applied.
    pub revision: Option<String>,
    pub result: Result<Step, Violation>,
}

type Job = Box<dyn FnOnce(&mut Worker) + Send>;

/// The logic thread's side.
pub struct Worker {
    engine: Engine,
    gameplay: bool,
    views: Differ,
    overlays: Differ,
    /// How many windows the last model gave the package (ADR 0086).
    windows: usize,
    /// The diff state of windows 1.. (index - 1).
    panes: Vec<Differ>,
}

impl Worker {
    pub fn engine(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// The diff of a view the UI thread applies, against the one it applied before: every
    /// view the UI thread is given goes through here, in order.
    pub fn diff_view(&mut self, view: &Value) -> Diff {
        self.views.next(view)
    }

    /// `diff_view`, for overlay views. A new overlay starts from no view.
    pub fn diff_overlay(&mut self, view: &Value) -> Diff {
        self.overlays.next(view)
    }

    /// The overlay was dropped: its next view is diffed against none.
    pub fn overlay_dropped(&mut self) {
        self.overlays = Differ::default();
    }

    /// Take the windows of a model (ADR 0086). A window that is new, or
    /// comes back, starts its diff from no view.
    pub fn set_windows(&mut self, windows: &[Vec<String>]) {
        self.windows = windows.len().max(1);
        self.panes.truncate(self.windows - 1);
        while self.panes.len() < self.windows - 1 {
            self.panes.push(Differ::default());
        }
    }

    /// `view(state, model, window)` of each further window, with its diff.
    pub fn pane_views(&mut self) -> Result<Vec<(u32, Value, Diff)>, Violation> {
        let mut views = Vec::new();
        for window in 1..self.windows {
            let view = self.engine.view_window(window as u32)?;
            let diff = self.panes[window - 1].next(&view);
            views.push((window as u32, view, diff));
        }
        Ok(views)
    }

    fn run(&mut self, call: Call) -> Outcome {
        let started = self.engine.limits().clock.start();
        self.engine.take_slowest();
        let message = match &call {
            Call::Message(m) => m
                .get("_tag")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_owned(),
            Call::Model { .. } => "KorriPublished".to_owned(),
        };
        let (revision, result) = match call {
            Call::Message(message) => (None, self.step(&message, None)),
            Call::Model { model, revision } => {
                let parsed = serde_json::from_str::<Value>(&model).ok();
                let gameplay = parsed.as_ref().is_some_and(messages::is_gameplay_overlay);
                let windows = parsed
                    .as_ref()
                    .map(messages::windows)
                    .unwrap_or_else(|| vec![Vec::new()]);
                self.set_windows(&windows);
                let result = self
                    .engine
                    .set_model(&model)
                    .and_then(|()| self.step(&messages::korri_published(), Some(gameplay)))
                    .map(|mut step| {
                        step.windows = Some(windows);
                        step
                    });
                (revision, result)
            }
        };
        Outcome {
            revision,
            result: result.map(|mut step| {
                step.took = started.elapsed();
                step.message = message;
                step.slowest = self.engine.take_slowest();
                step
            }),
        }
    }

    fn step(&mut self, message: &Value, gameplay: Option<bool>) -> Result<Step, Violation> {
        if let Some(gameplay) = gameplay {
            self.gameplay = gameplay;
        }
        let requests = self.engine.update(message)?;
        let (mut view, mut overlay, mut panes) = (None, None, Vec::new());
        if self.engine.view_stale() {
            self.engine.views_drawn();
            let next = self.engine.view()?;
            let started = std::time::Instant::now();
            let diff = self.views.next(&next);
            if crate::profile::enabled() {
                eprintln!(
                    "SHELL_PROFILE phase=diff ms={:.3}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            view = Some((next.clone(), diff));
            panes = self.pane_views()?;
            if self.gameplay && self.engine.has_overlay() {
                overlay = self
                    .engine
                    .overlay()?
                    .map(|next| (next.clone(), self.overlays.next(&next)));
            } else {
                self.overlays = Differ::default();
            }
        }
        Ok(Step {
            requests,
            view,
            overlay,
            gameplay_overlay: gameplay,
            windows: None,
            panes,
            took: Duration::ZERO,
            message: String::new(),
            slowest: None,
        })
    }
}

/// The UI thread's side of one surface's logic thread.
pub struct Logic {
    jobs: Option<mpsc::Sender<Job>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    declaration: Declaration,
    panes: Option<Panes>,
    has_overlay: bool,
    heap: Arc<AtomicUsize>,
    /// Calls not sent yet; the next goes once the running one has returned.
    waiting: VecDeque<Call>,
    running: bool,
}

/// Called on the logic thread with each call's outcome.
pub type Done = Arc<dyn Fn(Outcome) + Send + Sync>;

impl Logic {
    /// Start the thread and load `logic.js` on it (`Engine::load`).
    pub fn start(source: String, limits: Limits, name: &str) -> Result<Self, Violation> {
        let stop = Arc::new(AtomicBool::new(false));
        let heap = Arc::new(AtomicUsize::new(0));
        let (jobs, receiver) = mpsc::channel::<Job>();
        let (loaded_tx, loaded_rx) = mpsc::channel();
        let (thread_stop, thread_heap) = (stop.clone(), heap.clone());
        let thread = thread::Builder::new()
            .name(format!("logic-{name}"))
            .spawn(move || {
                let engine = match Engine::load_stoppable(&source, limits, thread_stop) {
                    Ok(engine) => engine,
                    Err(violation) => {
                        let _ = loaded_tx.send(Err(violation));
                        return;
                    }
                };
                let _ = loaded_tx.send(Ok((
                    engine.declaration().clone(),
                    engine.panes().cloned(),
                    engine.has_overlay(),
                )));
                let mut worker = Worker {
                    engine,
                    gameplay: false,
                    views: Differ::default(),
                    overlays: Differ::default(),
                    windows: 1,
                    panes: Vec::new(),
                };
                for job in receiver {
                    job(&mut worker);
                    thread_heap.store(worker.engine.heap_bytes(), Ordering::Relaxed);
                }
            })
            .map_err(|e| Violation::Exception {
                call: "load",
                message: e.to_string(),
            })?;
        let (declaration, panes, has_overlay) =
            loaded_rx.recv().map_err(|_| Violation::Exception {
                call: "load",
                message: "the logic thread ended".into(),
            })??;
        Ok(Self {
            jobs: Some(jobs),
            stop,
            thread: Some(thread),
            declaration,
            panes,
            has_overlay,
            heap,
            waiting: VecDeque::new(),
            running: false,
        })
    }

    pub fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// The package's panes (ADR 0086), as declared.
    pub fn panes(&self) -> Option<&Panes> {
        self.panes.as_ref()
    }

    pub fn has_overlay(&self) -> bool {
        self.has_overlay
    }

    /// The QuickJS heap after the last call, in bytes.
    pub fn heap_bytes(&self) -> usize {
        self.heap.load(Ordering::Relaxed)
    }

    /// Run `f` on the logic thread and wait for its result (loading only).
    pub fn now<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Worker) -> R + Send + 'static,
    ) -> Result<R, Violation> {
        let (tx, rx) = mpsc::channel();
        let ended = || Violation::Exception {
            call: "logic",
            message: "the logic thread ended".into(),
        };
        self.jobs
            .as_ref()
            .ok_or_else(ended)?
            .send(Box::new(move |worker| {
                let _ = tx.send(f(worker));
            }))
            .map_err(|_| ended())?;
        rx.recv().map_err(|_| ended())
    }

    /// Queue `call`; it runs once the calls before it have returned. A `Tick` sent while
    /// the last waiting call is a `Tick` replaces it: ticks never pile up behind a slow
    /// call, and never move ahead of a press.
    pub fn submit(&mut self, call: Call, done: &Done) {
        if call.is_tick() {
            if let Some(last) = self.waiting.back_mut().filter(|c| c.is_tick()) {
                *last = call;
                return;
            }
        }
        self.waiting.push_back(call);
        self.next(done);
    }

    /// The running call returned (its outcome reached the UI thread): send the next.
    pub fn returned(&mut self, done: &Done) {
        self.running = false;
        self.next(done);
    }

    /// Whether a call runs or waits.
    pub fn busy(&self) -> bool {
        self.running || !self.waiting.is_empty()
    }

    fn next(&mut self, done: &Done) {
        if self.running {
            return;
        }
        let Some(call) = self.waiting.pop_front() else {
            return;
        };
        let done = done.clone();
        let sent = self.jobs.as_ref().is_some_and(|jobs| {
            jobs.send(Box::new(move |worker| done(worker.run(call))))
                .is_ok()
        });
        self.running = sent;
    }
}

impl Drop for Logic {
    fn drop(&mut self) {
        // Interrupt a running call; the thread ends once its queue is closed.
        self.stop.store(true, Ordering::Relaxed);
        drop(self.jobs.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
