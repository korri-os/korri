//! A surface's logic on its own thread, so a long `update` or `view()` never
//! holds up a frame.
//!
//! The QuickJS engine ([`Engine`]) is created and runs on one thread per
//! surface. The UI thread sends it [`Call`]s and gets back an [`Outcome`] each:
//! the requests, and the views when the state or model changed. One call runs
//! at a time, in the order sent; a `Tick` waiting behind a running call is
//! replaced by a newer one. The call budget (50 ms) and the load budget (2 s)
//! are the engine's own, enforced on that thread. Dropping a [`Logic`]
//! interrupts a running call and joins the thread.
//!
//! Loading stays synchronous ([`Logic::now`]): the shell needs the
//! declaration, the first model's result and the first view before it shows
//! a surface.

use crate::{
    engine::{Declaration, Displays, Engine, Limits, Violation},
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
    time::{Duration, Instant},
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
    /// Wall time of the call on the logic thread.
    pub took: Duration,
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

    fn run(&mut self, call: Call) -> Outcome {
        let started = Instant::now();
        let (revision, result) = match call {
            Call::Message(message) => (None, self.step(&message, None)),
            Call::Model { model, revision } => {
                let gameplay = serde_json::from_str::<Value>(&model)
                    .map(|m| messages::is_gameplay_overlay(&m))
                    .unwrap_or(false);
                let result = self
                    .engine
                    .set_model(&model)
                    .and_then(|()| self.step(&messages::korri_published(), Some(gameplay)));
                (revision, result)
            }
        };
        Outcome {
            revision,
            result: result.map(|mut step| {
                step.took = started.elapsed();
                step
            }),
        }
    }

    fn step(&mut self, message: &Value, gameplay: Option<bool>) -> Result<Step, Violation> {
        if let Some(gameplay) = gameplay {
            self.gameplay = gameplay;
        }
        let requests = self.engine.update(message)?;
        let (mut view, mut overlay) = (None, None);
        if self.engine.view_stale() {
            self.engine.views_drawn();
            let next = self.engine.view()?;
            view = Some((next.clone(), self.views.next(&next)));
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
            took: Duration::ZERO,
        })
    }
}

/// The UI thread's side of one surface's logic thread.
pub struct Logic {
    jobs: Option<mpsc::Sender<Job>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    declaration: Declaration,
    displays: Option<Displays>,
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
                    engine.displays(),
                    engine.has_overlay(),
                )));
                let mut worker = Worker {
                    engine,
                    gameplay: false,
                    views: Differ::default(),
                    overlays: Differ::default(),
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
        let (declaration, displays, has_overlay) =
            loaded_rx.recv().map_err(|_| Violation::Exception {
                call: "load",
                message: "the logic thread ended".into(),
            })??;
        Ok(Self {
            jobs: Some(jobs),
            stop,
            thread: Some(thread),
            declaration,
            displays,
            has_overlay,
            heap,
            waiting: VecDeque::new(),
            running: false,
        })
    }

    pub fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    pub fn displays(&self) -> Option<Displays> {
        self.displays
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
