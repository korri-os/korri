//! One surface's logic in its own QuickJS runtime (rquickjs 0.14, korrid's
//! engine), contract 3.1 and 3.9.
//!
//! The state never leaves the engine: a closure created after `logic.js`
//! holds `state` and the current `model`. Only strings cross: the model and
//! messages as JSON in, requests and views as JSON out.
//!
//! Limits: the runtime's heap is capped at [`HEAP_LIMIT`] and every call at
//! [`CALL_BUDGET`]. Breaking either, throwing, or returning the wrong shape is
//! a [`Violation`]; the shell then unloads the surface.

use anyhow::{anyhow, Result};
use rquickjs::{Context, Function, Object, Persistent, Runtime};
use serde_json::Value;
use std::{
    cell::Cell,
    collections::BTreeMap,
    fmt,
    rc::Rc,
    time::{Duration, Instant},
};

/// Contract 3.9: QuickJS heap per surface.
pub const HEAP_LIMIT: usize = 64 * 1024 * 1024;
/// Contract 3.9: wall time per `update`, `view` and `overlay` call (and per
/// `setModel` after the surface loaded).
pub const CALL_BUDGET: Duration = Duration::from_millis(50);
/// Wall time for one surface load: evaluating `logic.js`, reading its
/// declarations, the first model and `init`, together. Once per load or switch.
pub const LOAD_BUDGET: Duration = Duration::from_millis(2_000);

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub heap: usize,
    /// Per `update`, `view` and `overlay` call.
    pub call: Duration,
    /// Bundle evaluation through `init`, once.
    pub load: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            heap: HEAP_LIMIT,
            call: CALL_BUDGET,
            load: LOAD_BUDGET,
        }
    }
}

/// Why a surface's logic stopped being trusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// A call ran longer than the budget.
    Time { call: &'static str, millis: u128 },
    /// The heap limit was reached.
    Memory { call: &'static str },
    /// The logic threw.
    Exception { call: &'static str, message: String },
    /// The logic returned something the contract does not allow.
    Contract { call: &'static str, message: String },
}

impl Violation {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Time { .. } => "time",
            Self::Memory { .. } => "memory",
            Self::Exception { .. } => "exception",
            Self::Contract { .. } => "contract",
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Time { call, millis } => write!(
                f,
                "{call} ran {millis} ms, over the {} ms budget",
                CALL_BUDGET.as_millis()
            ),
            Self::Memory { call } => write!(f, "{call} reached the QuickJS heap limit"),
            Self::Exception { call, message } => write!(f, "{call} threw: {message}"),
            Self::Contract { call, message } => write!(f, "{call}: {message}"),
        }
    }
}

impl std::error::Error for Violation {}

// Contract 3.6: the treaty's preference types, limits and checks.
pub use korri_surface_treaty::{
    valid_preference_key, SurfacePreferenceDeclaration as Preference,
    SurfacePreferenceMark as Mark, PREFERENCE_MAX_KEYS as MAX_PREFERENCES,
    PREFERENCE_MAX_VALUE_BYTES as MAX_PREFERENCE_BYTES,
};

/// A package's declaration, in key order.
pub type Declaration = BTreeMap<String, Preference>;

/// Check a `preferences` declaration against contract 3.6, through the
/// treaty's own parse and `check_preference_declarations`.
pub fn check_declaration(value: &Value) -> Result<Declaration, String> {
    let declarations: korri_surface_treaty::SurfacePreferenceDeclarations =
        serde_json::from_value(value.clone()).map_err(|error| {
            format!("preferences must map keys to {{ mark: \"shared\" | \"device\", initial: string }}: {error}")
        })?;
    korri_surface_treaty::check_preference_declarations(&declarations)
        .map_err(|refusal| refusal.to_string())?;
    Ok(declarations.into_iter().collect())
}

/// Stored values where korrid has them, the declared initial otherwise.
/// Stored keys the package no longer declares, and oversized values, are dropped.
pub fn effective_preferences(
    declaration: &Declaration,
    stored: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    declaration
        .iter()
        .map(|(key, preference)| {
            let value = stored
                .get(key)
                .filter(|v| v.len() <= MAX_PREFERENCE_BYTES)
                .unwrap_or(&preference.initial);
            (key.clone(), value.clone())
        })
        .collect()
}

const BOOTSTRAP: &str = r#"(() => {
  const CONTRACT = "contract: ";
  const s = globalThis.surface;
  if (s === null || typeof s !== "object") throw new TypeError(CONTRACT + "logic.js must define globalThis.surface");
  for (const name of ["init", "update", "view"]) if (typeof s[name] !== "function") throw new TypeError(CONTRACT + `surface.${name} must be a function`);
  if (s.overlay !== undefined && typeof s.overlay !== "function") throw new TypeError(CONTRACT + "surface.overlay must be a function");
  let state;
  let model;
  const text = (value, what) => {
    const out = JSON.stringify(value);
    if (typeof out !== "string") throw new TypeError(CONTRACT + `${what} must return JSON`);
    return out;
  };
  return {
    preferences: () => text(s.preferences ?? {}, "preferences"),
    displays: () => (s.displays === undefined ? "[]" : text([s.displays], "displays")),
    hasOverlay: () => typeof s.overlay === "function",
    setModel: (json) => { model = JSON.parse(json); },
    init: (preferences) => { state = s.init(model, JSON.parse(preferences)); },
    update: (message) => {
      const step = s.update(state, JSON.parse(message), model);
      if (step === null || typeof step !== "object" || !("state" in step) || !Array.isArray(step.requests))
        throw new TypeError(CONTRACT + "update must return { state, requests }");
      // "1": update returned a new state object, so view() may differ.
      const changed = step.state !== state;
      state = step.state;
      return (changed ? "1" : "0") + text(step.requests, "update requests");
    },
    view: () => text(s.view(state, model), "view"),
    overlay: () => text(s.overlay(state, model), "overlay"),
  };
})()"#;

/// Purity, as the spike's verifier enforced it: the clock and the random
/// generator throw, so a replay of the same messages gives the same views.
/// Explicit `new Date(ms)` stays available for formatting model timestamps.
const SANDBOX: &str = "Date.now = () => { throw new Error('Date.now is not available to surfaces'); };\nMath.random = () => { throw new Error('Math.random is not available to surfaces'); };";

/// Which displays a package draws on: the treaty's `SurfacePackageDisplays`.
/// The shell keeps "absent" apart from `main` so declare passes on exactly
/// what the package wrote.
pub use korri_surface_treaty::SurfacePackageDisplays as Displays;

/// Check a `displays` declaration: `"main"`, `"all"` or absent (`None`).
pub fn check_displays(value: Option<&Value>) -> Result<Option<Displays>, String> {
    match value.map(|v| v.as_str()) {
        None => Ok(None),
        Some(Some("main")) => Ok(Some(Displays::Main)),
        Some(Some("all")) => Ok(Some(Displays::All)),
        Some(_) => Err("displays must be \"main\", \"all\" or absent".into()),
    }
}

pub struct Engine {
    // Field order: the persistent handle and context drop before the runtime.
    dispatch: Persistent<Object<'static>>,
    context: Context,
    runtime: Runtime,
    deadline: Rc<Cell<Instant>>,
    limits: Limits,
    declaration: Declaration,
    displays: Option<Displays>,
    has_overlay: bool,
    /// See `view_stale`.
    stale: bool,
    max_call: Duration,
    /// While loading (until `init` returns): when the load started. Every call
    /// in that phase shares `limits.load`, measured from this instant.
    loading: Option<Instant>,
    load_time: Duration,
}

impl Engine {
    /// Evaluate `logic.js` and read its preference declaration.
    pub fn load(source: &str, limits: Limits) -> Result<Self, Violation> {
        Self::load_stoppable(source, limits, Default::default())
    }

    /// `load`, with a flag another thread sets to interrupt whatever call runs (a surface
    /// unloaded while its logic runs on its own thread, `crate::logic`).
    pub fn load_stoppable(
        source: &str,
        limits: Limits,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self, Violation> {
        let runtime = Runtime::new().map_err(|e| Violation::Exception {
            call: "load",
            message: e.to_string(),
        })?;
        runtime.set_memory_limit(limits.heap);
        runtime.set_max_stack_size(1024 * 1024);
        let deadline = Rc::new(Cell::new(Instant::now() + limits.load));
        let clock = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            stop.load(std::sync::atomic::Ordering::Relaxed) || Instant::now() >= clock.get()
        })));
        let context = Context::full(&runtime).map_err(|e| Violation::Exception {
            call: "load",
            message: e.to_string(),
        })?;
        let started = Instant::now();
        deadline.set(started + limits.load);
        let dispatch = context.with(|ctx| -> Result<Persistent<Object<'static>>, Violation> {
            let run = || -> rquickjs::Result<Persistent<Object<'static>>> {
                ctx.eval::<(), _>(SANDBOX)?;
                ctx.eval::<(), _>(source)?;
                let object: Object = ctx.eval(BOOTSTRAP)?;
                Ok(Persistent::save(&ctx, object))
            };
            run().map_err(|error| classify("load", error, &ctx))
        })?;
        let mut engine = Self {
            dispatch,
            context,
            runtime,
            deadline,
            limits,
            declaration: Declaration::new(),
            displays: None,
            has_overlay: false,
            stale: true,
            max_call: Duration::ZERO,
            loading: Some(started),
            load_time: Duration::ZERO,
        };
        engine.over_budget("load", started)?;
        let declaration = engine.call_string("preferences", &[])?;
        let declaration: Value =
            serde_json::from_str(&declaration).map_err(|e| Violation::Contract {
                call: "preferences",
                message: e.to_string(),
            })?;
        engine.declaration =
            check_declaration(&declaration).map_err(|message| Violation::Contract {
                call: "preferences",
                message,
            })?;
        let displays = engine.call_string("displays", &[])?;
        let displays: Value = serde_json::from_str(&displays).map_err(|e| Violation::Contract {
            call: "displays",
            message: e.to_string(),
        })?;
        engine.displays =
            check_displays(displays.get(0)).map_err(|message| Violation::Contract {
                call: "displays",
                message,
            })?;
        engine.has_overlay = engine.call_bool("hasOverlay")?;
        Ok(engine)
    }

    pub fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// The package's `displays`, exactly as declared (`None` when absent).
    pub fn displays(&self) -> Option<Displays> {
        self.displays
    }

    pub fn has_overlay(&self) -> bool {
        self.has_overlay
    }

    /// Load time: bundle evaluation through `init`. Zero until `init` returned.
    pub fn load_time(&self) -> Duration {
        self.load_time
    }

    /// The slowest `update`, `view`, `overlay` or later `setModel` call so far.
    pub fn max_call(&self) -> Duration {
        self.max_call
    }

    /// Current QuickJS heap use, in bytes.
    pub fn heap_bytes(&self) -> usize {
        self.runtime.memory_usage().memory_used_size.max(0) as usize
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Replace the model the logic sees. Parsed once per publish.
    pub fn set_model(&mut self, model_json: &str) -> Result<(), Violation> {
        self.stale = true;
        self.call_unit("setModel", model_json)
    }

    pub fn init(&mut self, preferences: &BTreeMap<String, String>) -> Result<(), Violation> {
        let text = serde_json::to_string(preferences).expect("string map serialises");
        self.call_unit("init", &text)?;
        if let Some(started) = self.loading.take() {
            self.load_time = started.elapsed();
        }
        Ok(())
    }

    /// Run `update`; returns the requests it asked for.
    pub fn update(&mut self, message: &Value) -> Result<Vec<Value>, Violation> {
        let reply = self.call_string("update", &[&message.to_string()])?;
        let (changed, requests) = reply.split_at(1.min(reply.len()));
        if changed != "0" {
            self.stale = true;
        }
        let requests: Vec<Value> =
            serde_json::from_str(requests).map_err(|e| Violation::Contract {
                call: "update",
                message: e.to_string(),
            })?;
        Ok(requests)
    }

    pub fn view(&mut self) -> Result<Value, Violation> {
        self.call_json("view")
    }

    /// Whether `view()` and `overlay()` may differ from their last results:
    /// the model was replaced, or an `update` returned a new state object
    /// (both are pure functions of state and model, contract 3.1).
    pub fn view_stale(&self) -> bool {
        self.stale
    }

    /// The views were drawn from the current state and model.
    pub fn views_drawn(&mut self) {
        self.stale = false;
    }

    /// `None` when the package has no `overlay`.
    pub fn overlay(&mut self) -> Result<Option<Value>, Violation> {
        if !self.has_overlay {
            return Ok(None);
        }
        self.call_json("overlay").map(Some)
    }

    fn call_json(&mut self, name: &'static str) -> Result<Value, Violation> {
        let text = self.call_string(name, &[])?;
        let value: Value = serde_json::from_str(&text).map_err(|e| Violation::Contract {
            call: name,
            message: e.to_string(),
        })?;
        if !value.is_object() {
            return Err(Violation::Contract {
                call: name,
                message: format!("{name} must return an object"),
            });
        }
        Ok(value)
    }

    fn call_unit(&mut self, name: &'static str, argument: &str) -> Result<(), Violation> {
        self.invoke(name, |function| function.call::<_, ()>((argument,)))
    }

    fn call_bool(&mut self, name: &'static str) -> Result<bool, Violation> {
        self.invoke(name, |function| function.call::<_, bool>(()))
    }

    fn call_string(&mut self, name: &'static str, arguments: &[&str]) -> Result<String, Violation> {
        match arguments {
            [] => self.invoke(name, |function| function.call::<_, String>(())),
            [one] => {
                let one = one.to_string();
                self.invoke(name, move |function| {
                    function.call::<_, String>((one.as_str(),))
                })
            }
            _ => unreachable!("dispatch functions take at most one argument"),
        }
    }

    fn invoke<T>(
        &mut self,
        name: &'static str,
        call: impl for<'js> FnOnce(Function<'js>) -> rquickjs::Result<T>,
    ) -> Result<T, Violation> {
        let started = self.loading.unwrap_or_else(Instant::now);
        let budget = self.budget();
        self.deadline.set(started + budget);
        let dispatch = self.dispatch.clone();
        let result = self.context.with(|ctx| {
            let object = dispatch
                .restore(&ctx)
                .map_err(|e| classify(name, e, &ctx))?;
            let function: Function = object.get(name).map_err(|e| classify(name, e, &ctx))?;
            call(function).map_err(|e| classify(name, e, &ctx))
        });
        if self.loading.is_none() {
            self.max_call = self.max_call.max(started.elapsed());
        }
        self.over_budget(name, started)?;
        result
    }

    /// The load budget while loading, then the per-call budget.
    fn budget(&self) -> Duration {
        if self.loading.is_some() {
            self.limits.load
        } else {
            self.limits.call
        }
    }

    fn over_budget(&self, call: &'static str, started: Instant) -> Result<(), Violation> {
        let elapsed = started.elapsed();
        if elapsed > self.budget() {
            return Err(Violation::Time {
                call,
                millis: elapsed.as_millis(),
            });
        }
        Ok(())
    }
}

fn classify(call: &'static str, error: rquickjs::Error, ctx: &rquickjs::Ctx<'_>) -> Violation {
    let message = match error {
        rquickjs::Error::Exception => {
            let caught = ctx.catch();
            if let Some(exception) = caught.as_exception() {
                exception.message().unwrap_or_default().to_string()
            } else {
                format!("{caught:?}")
            }
        }
        rquickjs::Error::Allocation => return Violation::Memory { call },
        other => other.to_string(),
    };
    if let Some(rest) = message.strip_prefix("contract: ") {
        return Violation::Contract {
            call,
            message: trim(rest),
        };
    }
    if message.contains("out of memory") {
        Violation::Memory { call }
    } else if message.contains("interrupted") {
        Violation::Time {
            call,
            millis: CALL_BUDGET.as_millis(),
        }
    } else {
        Violation::Exception {
            call,
            message: trim(&message),
        }
    }
}

fn trim(message: &str) -> String {
    message.chars().take(300).collect()
}

/// Read a package's `logic.js`.
pub fn read_source(path: &std::path::Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| anyhow!("read {}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| anyhow!("{} is not UTF-8", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const COUNTER: &str = r#"
      globalThis.surface = {
        preferences: { "zone.look": { mark: "device", initial: "modern" } },
        init: (model, prefs) => ({ n: new Map([["n", 0]]), look: prefs["zone.look"], total: model.total }),
        update: (state, message, model) => {
          if (message._tag === "Pressed" && message.button === "right") {
            const n = new Map(state.n); n.set("n", n.get("n") + 1);
            return { state: { ...state, n }, requests: [{ _tag: "FocusGame", gameId: String(n.get("n")) }] };
          }
          return { state, requests: [] };
        },
        view: (state, model) => ({ position: state.n.get("n"), look: state.look, total: model.total }),
      };"#;

    fn prefs(look: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("zone.look".to_owned(), look.to_owned())])
    }

    #[test]
    fn keeps_non_json_state_inside_and_returns_requests() -> Result<(), Violation> {
        let mut engine = Engine::load(COUNTER, Limits::default())?;
        assert_eq!(engine.declaration()["zone.look"].initial, "modern");
        engine.set_model(r#"{"total": 3}"#)?;
        engine.init(&prefs("retro"))?;
        assert_eq!(
            engine.view()?,
            json!({"position": 0, "look": "retro", "total": 3})
        );
        let requests = engine.update(&json!({"_tag": "Pressed", "button": "right"}))?;
        assert_eq!(requests, vec![json!({"_tag": "FocusGame", "gameId": "1"})]);
        engine.set_model(r#"{"total": 9}"#)?;
        assert_eq!(engine.view()?["total"], 9);
        assert_eq!(engine.view()?["position"], 1);
        assert_eq!(engine.overlay()?, None);
        Ok(())
    }

    /// Busy-waits `ms` milliseconds inside QuickJS (`Date.now` is withdrawn).
    fn spin(ms: u32) -> String {
        format!(
            "{{ const t0 = new Date().getTime(); while (new Date().getTime() - t0 < {ms}) {{}} }}"
        )
    }

    #[test]
    fn a_slow_bundle_and_init_share_the_load_budget_not_the_call_budget() {
        // 60 ms of evaluation and 60 ms of init: over the 50 ms call budget,
        // inside the 2 s load budget.
        let source = format!(
            "{}\nglobalThis.surface = {{ init: () => {{ {} return 0; }}, update: (s) => ({{ state: s, requests: [] }}), view: () => ({{}}) }};",
            spin(60),
            spin(60)
        );
        let mut engine = Engine::load(&source, Limits::default()).expect("a 60 ms load succeeds");
        engine.set_model("{}").expect("model");
        engine
            .init(&BTreeMap::new())
            .expect("init inside the load budget");
        assert!(
            engine.load_time() >= Duration::from_millis(110),
            "{:?}",
            engine.load_time()
        );
        assert!(
            engine.max_call() < CALL_BUDGET,
            "load calls are not per-call calls"
        );
        // After init, every call is back on the 50 ms budget.
        engine
            .update(&json!({"_tag": "KorriPublished"}))
            .expect("update");
    }

    #[test]
    fn a_load_over_two_seconds_is_refused() {
        let started = Instant::now();
        let error = Engine::load("while (true) {}", Limits::default())
            .err()
            .expect("refused");
        assert_eq!(error.kind(), "time", "{error}");
        let elapsed = started.elapsed();
        assert!(
            elapsed >= LOAD_BUDGET && elapsed < LOAD_BUDGET + Duration::from_millis(500),
            "{elapsed:?}"
        );
        // The budget covers evaluation and init together.
        let source = format!(
            "{}\nglobalThis.surface = {{ init: () => {{ while (true) {{}} }}, update: (s) => ({{ state: s, requests: [] }}), view: () => ({{}}) }};",
            spin(1500)
        );
        let mut engine = Engine::load(&source, Limits::default()).expect("1.5 s evaluation fits");
        engine.set_model("{}").expect("model");
        let started = Instant::now();
        let error = engine.init(&BTreeMap::new()).unwrap_err();
        assert_eq!(error.kind(), "time", "{error}");
        assert!(
            started.elapsed() < Duration::from_millis(1_000),
            "init gets only what evaluation left"
        );
    }

    #[test]
    fn a_runaway_call_breaks_the_time_budget() {
        let source = "globalThis.surface = { init: () => 0, update: (s) => { while (true) {} }, view: () => ({}) };";
        let mut engine = Engine::load(source, Limits::default()).expect("load");
        engine.set_model("{}").expect("model");
        engine.init(&BTreeMap::new()).expect("init");
        let started = Instant::now();
        let error = engine
            .update(&json!({"_tag": "KorriPublished"}))
            .unwrap_err();
        assert_eq!(error.kind(), "time", "{error}");
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn a_heap_blow_up_breaks_the_memory_limit() {
        let source = "globalThis.surface = { init: () => 0, update: (s) => { const a = []; while (true) a.push(new Uint8Array(1 << 20)); }, view: () => ({}) };";
        let limits = Limits {
            heap: 16 * 1024 * 1024,
            call: Duration::from_secs(5),
            ..Limits::default()
        };
        let mut engine = Engine::load(source, limits).expect("load");
        engine.set_model("{}").expect("model");
        engine.init(&BTreeMap::new()).expect("init");
        let error = engine
            .update(&json!({"_tag": "KorriPublished"}))
            .unwrap_err();
        assert_eq!(error.kind(), "memory", "{error}");
    }

    #[test]
    fn wrong_shapes_and_throws_are_violations() {
        let old_style =
            "globalThis.surface = { init: () => 0, update: (s) => s, view: () => ({}) };";
        let mut engine = Engine::load(old_style, Limits::default()).expect("load");
        engine.set_model("{}").expect("model");
        engine.init(&BTreeMap::new()).expect("init");
        assert_eq!(
            engine
                .update(&json!({"_tag": "KorriPublished"}))
                .unwrap_err()
                .kind(),
            "contract"
        );
        let throws = "globalThis.surface = { init: () => 0, update: (s) => ({state: s, requests: []}), view: () => { throw new Error('planted') } };";
        let mut engine = Engine::load(throws, Limits::default()).expect("load");
        engine.set_model("{}").expect("model");
        engine.init(&BTreeMap::new()).expect("init");
        let error = engine.view().unwrap_err();
        assert!(
            matches!(&error, Violation::Exception { message, .. } if message.contains("planted")),
            "{error}"
        );
        assert!(Engine::load("globalThis.surface = {}", Limits::default()).is_err());
        let clock = "globalThis.surface = { init: () => Date.now(), update: (s) => ({state: s, requests: []}), view: () => ({}) };";
        let mut engine = Engine::load(clock, Limits::default()).expect("load");
        engine.set_model("{}").expect("model");
        assert_eq!(
            engine.init(&BTreeMap::new()).unwrap_err().kind(),
            "exception"
        );
    }

    #[test]
    fn displays_is_main_all_or_absent() {
        let load = |line: &str| {
            Engine::load(
                &format!("globalThis.surface = {{ {line} init: () => 0, update: (s) => ({{ state: s, requests: [] }}), view: () => ({{}}) }};"),
                Limits::default(),
            )
        };
        assert_eq!(load("").unwrap().displays(), None);
        assert_eq!(
            load("displays: \"main\",").unwrap().displays(),
            Some(Displays::Main)
        );
        assert_eq!(
            load("displays: \"all\",").unwrap().displays(),
            Some(Displays::All)
        );
        for wrong in ["\"both\"", "\"Main\"", "1", "[\"main\"]", "null"] {
            let error = load(&format!("displays: {wrong},"))
                .err()
                .unwrap_or_else(|| panic!("{wrong} accepted"));
            assert!(
                matches!(
                    error,
                    Violation::Contract {
                        call: "displays",
                        ..
                    }
                ),
                "{wrong}: {error}"
            );
        }
    }

    #[test]
    fn declarations_follow_contract_3_6() {
        assert!(
            check_declaration(&json!({"pico.font": {"mark": "shared", "initial": "tiny5"}}))
                .is_ok()
        );
        assert!(check_declaration(&json!({"Pico": {"mark": "shared", "initial": ""}})).is_err());
        assert!(check_declaration(&json!({"a": {"mark": "cloud", "initial": ""}})).is_err());
        assert!(
            check_declaration(&json!({"a": {"mark": "device", "initial": "x".repeat(257)}}))
                .is_err()
        );
        let many: serde_json::Map<String, Value> = (0..17)
            .map(|i| (format!("k{i}"), json!({"mark": "device", "initial": ""})))
            .collect();
        assert!(check_declaration(&Value::Object(many)).is_err());
        assert!(!valid_preference_key(&"a".repeat(33)));
        let declaration = check_declaration(&json!({"a": {"mark": "device", "initial": "one"}, "b": {"mark": "shared", "initial": "two"}})).unwrap();
        let stored = BTreeMap::from([
            ("a".to_owned(), "stored".to_owned()),
            ("gone".to_owned(), "x".to_owned()),
        ]);
        let effective = effective_preferences(&declaration, &stored);
        assert_eq!(
            effective,
            BTreeMap::from([
                ("a".to_owned(), "stored".to_owned()),
                ("b".to_owned(), "two".to_owned())
            ])
        );
    }
}
