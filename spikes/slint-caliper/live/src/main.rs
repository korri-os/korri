//! THROWAWAY SPIKE `live` (README.md beside this file).
//!
//! One surface package (Zone or Pico) runs with its real `update` and `view`,
//! through the shell's own pieces (`package::compile`, `logic::Logic`,
//! `view::Binder`, `korrid::Korrid`, `input::Router`, the winit backend),
//! against clients/shell's fixture korrid as a child process. A bench window
//! picks the device size and the fixture model (`model FILE` on the
//! fixture's stdin). A saved `.slint` swaps the views in the same window and
//! keeps the logic; a saved `.ts` runs the package's own `bun run ID:build`,
//! and the new `logic.js` replaces the logic by one of three modes.

use anyhow::{anyhow, bail, Context, Result};
use korri_shell::{
    backend::{winit::Winit, Backend, FrameCounter, HostEvent, Placement},
    config::{self, Connection},
    engine::{self, Limits},
    images::ImageCache,
    input::{Press, Router},
    korrid::{Korrid, WatchEvent},
    logic::{Call, Done, Logic, Outcome},
    messages::{self, Button},
    package::{self, Package},
    view::{self, Binder, Diff},
};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model as _};
use slint_interpreter::{ComponentDefinition, ComponentInstance, Value as SValue};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Write},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    rc::Rc,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant, SystemTime},
};

/// Device CSS sizes. Grounding: packages/pico/preview/render.sh `size_of`
/// (Pico's Caliper devices) and Caliper's `HANDHELD_DEVICES`
/// (src/client/device-frame.js). rg35xxsp at 640x480 comes from the spike's
/// brief only: UNRESOLVED, no Korri file states its CSS size.
const DEVICES: &[(&str, u32, u32)] = &[
    ("rg35xxsp", 640, 480),
    ("rg353m", 640, 480),
    ("odin2portal", 1920, 1080),
    ("rpminiv2", 1240, 1080),
    ("r36tmax", 720, 720),
    ("rgds", 640, 960),
];
const PACKAGES: [&str; 2] = ["zone", "pico"];
const MODES: [&str; 3] = ["replay", "transfer", "fresh"];
const POLL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// A new engine, then every message and model since the load, in order.
    Replay,
    /// The old engine's state as JSON, handed to the new engine's `init`.
    Transfer,
    /// A new engine, `init` on the current model: the state is lost.
    Fresh,
}

#[derive(Debug)]
enum Tool {
    Package(usize),
    Device(usize),
    Scenario(usize),
    Mode(usize),
    Play,
}

#[derive(Debug)]
enum Ev {
    Host { gen: u64, event: HostEvent },
    Model { gen: u64, revision: Option<String>, model: String },
    Reconnected { gen: u64 },
    Logic { gen: u64, outcome: Box<Outcome> },
    RequestDone { gen: u64, request: Value, ok: bool },
    Idle,
    Repeat,
    Step,
    Changed(Vec<(PathBuf, SystemTime)>),
    Transpiled { took: Duration, result: Result<(), String> },
    Tool(Tool),
}

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

fn post(ev: Ev) {
    let _ = slint::invoke_from_event_loop(move || {
        LIVE.with(|live| {
            if let Ok(mut live) = live.try_borrow_mut() {
                if let Some(live) = live.as_mut() {
                    live.handle(ev);
                }
            }
        })
    });
}

#[derive(Clone, Debug)]
enum Step {
    Press(Button),
    Model(usize),
    Wait(f64),
}

#[derive(Clone, Debug)]
struct Scenario {
    name: String,
    reference: String,
    /// Model files in the private directory, in order (`model N` publishes the Nth).
    models: Vec<PathBuf>,
    steps: Vec<Step>,
    displays: Option<Value>,
}

/// clients/shell's fixture korrid, as its own process.
struct Fixture {
    child: Child,
    stdin: ChildStdin,
}

impl Fixture {
    fn send(&mut self, line: &str) {
        let _ = writeln!(self.stdin, "{line}");
        let _ = self.stdin.flush();
    }
}

/// What the logic was given since the surface loaded: enough to build the same state
/// again in a new engine (the engine forbids the clock and randomness).
#[derive(Clone)]
struct Journal {
    model: String,
    preferences: std::collections::BTreeMap<String, String>,
    size: (f32, f32),
    calls: Vec<Call>,
    /// False once a transfer replaced the state by one the journal does not explain.
    valid: bool,
}

struct Mounted {
    component: ComponentInstance,
    binder: Binder,
    overlay: bool,
}

struct Surface {
    package: Package,
    logic: Logic,
    logic_gen: u64,
    window_gen: u64,
    done: Done,
    views: package::Views,
    mounted: Mounted,
    images: ImageCache,
    watch: Option<korri_shell::korrid::Watch>,
    last_view: Value,
    last_overlay: Option<Value>,
    gameplay: bool,
    latest_model: String,
    journal: Journal,
    last_press: Instant,
    size: (f32, f32),
}

struct Pending {
    kind: &'static str,
    saved: SystemTime,
    parts: Vec<(&'static str, Duration)>,
    detail: String,
    kept: bool,
    failed: bool,
}

struct Live {
    root: PathBuf,
    private: PathBuf,
    korrid: Korrid,
    fixture: Fixture,
    backend: Rc<Winit>,
    frames: FrameCounter,
    router: Router,
    clock: Instant,
    repeat: Option<slint::Timer>,
    _idle: slint::Timer,
    tool: ComponentInstance,
    package_index: usize,
    device_index: usize,
    mode: Mode,
    scenarios: Vec<Scenario>,
    scenario_index: i32,
    steps: VecDeque<Step>,
    step_timer: Option<slint::Timer>,
    current: Option<Surface>,
    next_gen: u64,
    requests: mpsc::Sender<(u64, String, Value)>,
    pending: Option<Pending>,
    transpiling: bool,
    transpile_again: bool,
    ts_saved: Option<SystemTime>,
    /// logic.js as the last transpile wrote it: the watcher's report of that write is no edit.
    js_written: Option<SystemTime>,
    reloads: Vec<(String, String, String, bool, bool)>,
    /// The last view applied before a logic reload, to compare with the first one after.
    status: String,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut packages: Option<PathBuf> = None;
    let mut surface = "zone".to_owned();
    let mut device = "rg35xxsp".to_owned();
    let mut mode = Mode::Replay;
    let mut scenario: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--packages" => packages = args.next().map(PathBuf::from),
            "--surface" => surface = args.next().context("--surface ID")?,
            "--device" => device = args.next().context("--device NAME")?,
            "--scenario" => scenario = Some(args.next().context("--scenario NAME")?),
            "--mode" => {
                mode = match args.next().as_deref() {
                    Some("replay") => Mode::Replay,
                    Some("transfer") => Mode::Transfer,
                    Some("fresh") => Mode::Fresh,
                    _ => bail!("--mode replay|transfer|fresh"),
                }
            }
            _ => bail!("usage: korri-live [--packages DIR] [--surface zone|pico] [--device NAME] [--mode replay|transfer|fresh] [--scenario NAME]"),
        }
    }
    let root = match packages {
        Some(p) => p,
        None => find_packages()?,
    }
    .canonicalize()?;
    let package_index = PACKAGES
        .iter()
        .position(|p| *p == surface)
        .context("--surface is zone or pico")?;
    let device_index = DEVICES
        .iter()
        .position(|d| d.0 == device)
        .context("unknown --device")?;

    // A private directory: the fixture's credential and the model files.
    let private = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("korri-live-{}", std::process::id()));
    std::fs::DirBuilder::new().mode(0o700).create(&private)?;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(private.join("credentials"))?;

    let fixture_bin = std::env::var_os("KORRI_LIVE_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .expect("current exe")
                .with_file_name("fixture-korrid")
        });
    let first_model = root.join("../fixtures/model.json");
    let (fixture, connection) = start_fixture(&fixture_bin, &private, &first_model)?;

    let backend = Rc::new(Winit::install()?);
    let tool_def = compile_source(include_str!("../ui/tool.slint"), "tool.slint")?;
    let tool = tool_def.create()?;
    for (name, make) in [
        ("choose-package", Tool::Package as fn(usize) -> Tool),
        ("choose-device", Tool::Device),
        ("choose-scenario", Tool::Scenario),
        ("choose-mode", Tool::Mode),
    ] {
        tool.set_callback(name, move |args| {
            if let Some(SValue::Number(n)) = args.first() {
                post(Ev::Tool(make(*n as usize)));
            }
            SValue::Void
        })?;
    }
    tool.set_callback("play-steps", |_| {
        post(Ev::Tool(Tool::Play));
        SValue::Void
    })?;
    tool.show()?;

    let (requests, receiver) = mpsc::channel::<(u64, String, Value)>();
    let worker = Korrid::new(connection.clone());
    thread::spawn(move || {
        for (gen, surface, request) in receiver {
            eprintln!("korri-live: request {}", messages::request_tag(&request).unwrap_or("?"));
            let ok = match worker.request(&surface, &request) {
                Ok(()) => true,
                Err(error) => {
                    eprintln!("korri-live: request {}: {error}", messages::request_tag(&request).unwrap_or("?"));
                    false
                }
            };
            post(Ev::RequestDone { gen, request, ok });
        }
    });
    let idle = slint::Timer::default();
    idle.start(slint::TimerMode::Repeated, messages::TICK_INTERVAL, || post(Ev::Idle));
    watch_files(root.clone());

    let mut live = Live {
        root,
        private,
        korrid: Korrid::new(connection),
        fixture,
        backend,
        frames: FrameCounter::default(),
        router: Router::new(true),
        clock: Instant::now(),
        repeat: None,
        _idle: idle,
        tool,
        package_index,
        device_index,
        mode,
        scenarios: Vec::new(),
        scenario_index: 0,
        steps: VecDeque::new(),
        step_timer: None,
        current: None,
        next_gen: 0,
        requests,
        pending: None,
        transpiling: false,
        transpile_again: false,
        ts_saved: None,
        js_written: None,
        reloads: Vec::new(),
        status: String::new(),
    };
    live.open(package_index)?;
    if let Some(name) = scenario {
        let index = live
            .scenarios
            .iter()
            .position(|s| s.name == name)
            .context("unknown --scenario")?;
        live.tool(Tool::Scenario(index));
    }
    LIVE.with(|cell| *cell.borrow_mut() = Some(live));
    slint::run_event_loop_until_quit()?;
    LIVE.with(|cell| {
        if let Some(mut live) = cell.borrow_mut().take() {
            let _ = live.fixture.child.kill();
            let _ = std::fs::remove_dir_all(&live.private);
        }
    });
    Ok(())
}

fn find_packages() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join("clients/shell/packages");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        if !dir.pop() {
            bail!("no clients/shell/packages above the working directory; pass --packages");
        }
    }
}

/// Start the fixture korrid the way clients/shell's proofs do: it writes the capability
/// into a private credentials directory and prints the address and origin; the shell side
/// reads them through `Connection::from_environment`, and nothing logs the capability.
fn start_fixture(bin: &Path, private: &Path, model: &Path) -> Result<(Fixture, Connection)> {
    let credentials = private.join("credentials");
    let mut command = Command::new(bin);
    // The fixture serves until killed (clients/shell/src/fixture/process.rs): end it with us,
    // also when we are killed.
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let mut child = command
        .args(["--games", "12", "--credentials"])
        .arg(&credentials)
        .arg("--model")
        .arg(model)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("start {}", bin.display()))?;
    let stdin = child.stdin.take().context("fixture stdin")?;
    let stdout = child.stdout.take().context("fixture stdout")?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.starts_with("KORRID_") {
                let _ = tx.send(line);
            }
        }
    });
    let (mut address, mut origin) = (None, None);
    while address.is_none() || origin.is_none() {
        let line = rx
            .recv_timeout(Duration::from_secs(10))
            .context("fixture korrid did not print its address")?;
        if let Some(v) = line.strip_prefix("KORRID_ADDRESS=") {
            address = Some(v.to_owned());
        }
        if let Some(v) = line.strip_prefix("KORRID_SHELL_ORIGIN=") {
            origin = Some(v.to_owned());
        }
    }
    std::env::set_var("KORRID_ADDRESS", address.unwrap());
    std::env::set_var("KORRID_SHELL_ORIGIN", origin.unwrap());
    std::env::set_var("CREDENTIALS_DIRECTORY", &credentials);
    let connection = Connection::from_environment().map_err(|e| anyhow!(e))?;
    config::scrub_environment();
    let korrid = Korrid::new(connection.clone());
    if !korrid.wait_until_listening(Duration::from_secs(10)) {
        bail!("fixture korrid is not listening");
    }
    Ok((Fixture { child, stdin }, connection))
}

fn compile_source(source: &str, name: &str) -> Result<ComponentDefinition> {
    let compiler = slint_interpreter::Compiler::default();
    let result = pollster::block_on(compiler.build_from_source(source.into(), name.into()));
    for d in result.diagnostics() {
        eprintln!("korri-live: {d}");
    }
    if result.has_errors() {
        bail!("{name} does not compile");
    }
    let first = result.component("Tool").context("no component Tool");
    first
}

/// Wrap `logic.js` so the spike can read the state out (`__LiveExport`) and hand a state
/// in (`seed`, returned by `init`). Spike-only: the shell's own engine stays as it is.
fn shim(source: &str, seed: Option<&str>) -> String {
    let seed = seed.map_or("undefined".to_owned(), |s| {
        serde_json::to_string(s).expect("string")
    });
    format!(
        "{source}\n;(() => {{ const s = globalThis.surface; const seed = {seed}; \
         globalThis.surface = Object.assign({{}}, s, {{ \
           init: (m, p) => seed === undefined ? s.init(m, p) : JSON.parse(seed), \
           update: (st, msg, m) => msg && msg._tag === \"__LiveExport\" \
             ? {{ state: st, requests: [{{ _tag: \"__LiveState\", json: JSON.stringify(st) }}] }} \
             : s.update(st, msg, m) }}); }})();\n"
    )
}

/// Poll the packages' files: `.slint`, `.ts` (not tests or previews) and `logic.js`.
fn watch_files(root: PathBuf) {
    fn scan(dir: &Path, out: &mut HashMap<PathBuf, SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if !matches!(
                    name.as_ref(),
                    "test" | "preview" | "node_modules" | "fonts" | "art" | ".git"
                ) {
                    scan(&path, out);
                }
            } else if name.ends_with(".slint") || name.ends_with(".ts") || name == "logic.js" {
                if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                    out.insert(path, modified);
                }
            }
        }
    }
    thread::spawn(move || {
        let mut known = HashMap::new();
        scan(&root, &mut known);
        loop {
            thread::sleep(POLL);
            let mut now = HashMap::new();
            scan(&root, &mut now);
            let changed: Vec<_> = now
                .iter()
                .filter(|(p, m)| known.get(*p) != Some(m))
                .map(|(p, m)| (p.clone(), *m))
                .collect();
            if !changed.is_empty() {
                post(Ev::Changed(changed));
            }
            known = now;
        }
    });
}

fn logical_size(window: &slint::Window) -> (f32, f32) {
    let size = window.size().to_logical(window.scale_factor());
    (size.width, size.height)
}

fn button_of_step(step: &str) -> Option<Button> {
    Some(match step {
        "up" | "Up" => Button::Up,
        "down" | "Down" => Button::Down,
        "left" | "Left" => Button::Left,
        "right" | "Right" => Button::Right,
        "confirm" | "Return" => Button::Confirm,
        "back" | "Escape" => Button::Back,
        "menu" | "m" => Button::Menu,
        "options" | "o" => Button::Options,
        "system" | "s" => Button::System,
        _ => return None,
    })
}

/// Paths of every entry with `focused: true`.
fn focused_paths(view: &Value) -> Vec<String> {
    fn walk(v: &Value, path: String, out: &mut Vec<String>) {
        match v {
            Value::Object(map) => {
                if map.get("focused") == Some(&Value::Bool(true)) {
                    out.push(path.clone());
                }
                for (k, v) in map {
                    walk(v, format!("{path}.{k}"), out);
                }
            }
            Value::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    walk(v, format!("{path}[{i}]"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(view, String::new(), &mut out);
    out
}

/// Leaf paths where two views differ.
fn differences(a: &Value, b: &Value) -> Vec<String> {
    fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                    walk(
                        x.get(k).unwrap_or(&Value::Null),
                        y.get(k).unwrap_or(&Value::Null),
                        format!("{path}.{k}"),
                        out,
                    );
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (x, y)) in x.iter().zip(y).enumerate() {
                    walk(x, y, format!("{path}[{i}]"), out);
                }
            }
            (x, y) if x == y => {}
            _ => out.push(path),
        }
    }
    let mut out = Vec::new();
    walk(a, b, String::new(), &mut out);
    out
}

fn strings(items: impl IntoIterator<Item = String>) -> SValue {
    SValue::Model(slint::ModelRc::new(slint::VecModel::from(
        items
            .into_iter()
            .map(|s| SValue::String(s.into()))
            .collect::<Vec<_>>(),
    )))
}

fn record(fields: &[(&str, SValue)]) -> SValue {
    SValue::Struct(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

impl Live {
    fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64() * 1000.0
    }

    fn device(&self) -> (u32, u32) {
        let d = DEVICES[self.device_index];
        (d.1, d.2)
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Host { gen, event } => self.host(gen, event),
            Ev::Model {
                gen,
                revision,
                model,
            } => {
                if let Some(s) = self.current.as_mut().filter(|s| s.window_gen == gen) {
                    s.latest_model = model.clone();
                    let call = Call::Model { model, revision };
                    s.journal.calls.push(Call::Model {
                        model: s.latest_model.clone(),
                        revision: None,
                    });
                    let done = s.done.clone();
                    s.logic.submit(call, &done);
                }
            }
            Ev::Reconnected { gen } => {
                if self.current.as_ref().is_some_and(|s| s.window_gen == gen) {
                    let id = self.current.as_ref().unwrap().package.id.clone();
                    if let Ok(model) = self.korrid.model(&id) {
                        self.handle(Ev::Model {
                            gen,
                            revision: None,
                            model,
                        });
                    }
                }
            }
            Ev::Logic { gen, outcome } => self.logic_done(gen, *outcome),
            Ev::RequestDone { gen, request, ok } => {
                if ok && self.current.as_ref().is_some_and(|s| s.window_gen == gen) {
                    if let Some((key, _)) = messages::remembered(&request) {
                        self.submit(Call::Message(messages::preference_stored(&key)));
                    }
                }
            }
            Ev::Idle => {
                if let Some(s) = self.current.as_ref() {
                    let idle = s.last_press.elapsed().as_millis().min(u32::MAX as u128) as u32;
                    self.submit(Call::Message(messages::tick(idle)));
                }
            }
            Ev::Repeat => {
                let presses = self.router.tick(self.now());
                self.presses(presses);
                if !self.router.holding() {
                    self.repeat = None;
                }
            }
            Ev::Step => self.next_step(),
            Ev::Changed(changed) => self.changed(changed),
            Ev::Transpiled { took, result } => self.transpiled(took, result),
            Ev::Tool(action) => self.tool(action),
        }
    }

    fn host(&mut self, gen: u64, event: HostEvent) {
        let Some(s) = self.current.as_ref() else { return };
        if s.window_gen != gen {
            return;
        }
        match event {
            HostEvent::Key { key, pressed } => {
                let presses = self.router.key(key, pressed, self.now());
                self.presses(presses);
            }
            HostEvent::Resized => {
                let window = s.mounted.component.window();
                let size = logical_size(window);
                if size != s.size && size.0 > 0.0 && size.1 > 0.0 {
                    self.current.as_mut().unwrap().size = size;
                    self.submit(Call::Message(messages::resized(size.0, size.1)));
                }
            }
            HostEvent::FirstFrame => self.frame_drawn(),
            HostEvent::Close => {
                let _ = slint::quit_event_loop();
            }
            HostEvent::Focus(_) => {}
        }
    }

    fn presses(&mut self, presses: Vec<Press>) {
        for press in presses {
            match press {
                Press::Button(button) => self.press(button),
                Press::Switch => {
                    let next = (self.package_index + 1) % PACKAGES.len();
                    self.tool(Tool::Package(next));
                }
            }
        }
        if self.router.holding() && self.repeat.is_none() {
            let timer = slint::Timer::default();
            timer.start(slint::TimerMode::Repeated, Duration::from_millis(10), || {
                post(Ev::Repeat)
            });
            self.repeat = Some(timer);
        }
    }

    fn press(&mut self, button: Button) {
        if let Some(s) = self.current.as_mut() {
            s.last_press = Instant::now();
        }
        eprintln!("korri-live: press {button:?}");
        self.submit(Call::Message(messages::pressed(button)));
    }

    fn submit(&mut self, call: Call) {
        if let Some(s) = self.current.as_mut() {
            s.journal.calls.push(call.clone());
            let done = s.done.clone();
            s.logic.submit(call, &done);
        }
    }

    fn done_for(gen: u64) -> Done {
        Arc::new(move |outcome| {
            post(Ev::Logic {
                gen,
                outcome: Box::new(outcome),
            })
        })
    }

    // ---- loading -------------------------------------------------------

    fn open(&mut self, index: usize) -> Result<()> {
        let id = PACKAGES[index];
        let package = Package::at(&self.root.join(id))?;
        let started = Instant::now();
        let views = package::compile(&package)?;
        let compile = started.elapsed();
        let logic = Logic::start(shim(&package.logic_source()?, None), Limits::default(), id)
            .map_err(|v| anyhow!("logic: {v}"))?;
        let declaration = serde_json::to_value(logic.declaration())?;
        self.korrid
            .declare(id, &declaration, logic.displays())
            .context("declare")?;
        let stored = self.korrid.preferences(id)?;
        let preferences = engine::effective_preferences(logic.declaration(), &stored);
        if id == "pico" {
            let (w, h) = self.device();
            self.fixture.send(&format!(
                "displays [{{\"id\":\"live\",\"main\":true,\"x\":0,\"y\":0,\"width\":{w},\"height\":{h},\"touch\":false}}]"
            ));
        } else {
            self.fixture.send("displays none");
        }
        let model = self.korrid.model(id)?;
        self.next_gen += 1;
        let gen = self.next_gen;
        let gameplay = messages::is_gameplay_overlay(&serde_json::from_str(&model)?);
        let old = self.current.take();
        let component = match &old {
            Some(old) => views
                .surface
                .create_with_existing_window(old.mounted.component.window())?,
            None => views.surface.create()?,
        };
        let binder = Binder::new(&views.surface);
        self.attach(component.window(), gen);
        let (w, h) = self.device();
        self.backend.place(
            component.window(),
            Placement::Windowed {
                width: w,
                height: h,
            },
        );
        let size = (w as f32, h as f32);
        let (model2, prefs2) = (model.clone(), preferences.clone());
        let (requests, view, diff) = logic
            .now(move |worker| -> Result<_, engine::Violation> {
                let e = worker.engine();
                e.set_model(&model2)?;
                e.init(&prefs2)?;
                let requests = e.update(&messages::resized(size.0, size.1))?;
                e.views_drawn();
                let view = e.view()?;
                let diff = worker.diff_view(&view);
                Ok((requests, view, diff))
            })
            .and_then(|r| r)
            .map_err(|v| anyhow!("logic: {v}"))?;
        let mut images = ImageCache::new(&package.dir);
        let mut mounted = Mounted {
            component,
            binder,
            overlay: false,
        };
        mounted
            .binder
            .apply_diff(&mounted.component, &view, &diff, &mut images)?;
        mounted.component.show()?;
        fix_size(mounted.component.window(), w, h, id);
        drop(old);
        let watch = self.korrid.watch(
            id,
            Box::new(move |event| {
                post(match event {
                    WatchEvent::Model { revision, model } => Ev::Model {
                        gen,
                        revision: Some(revision),
                        model,
                    },
                    WatchEvent::Reconnected => Ev::Reconnected { gen },
                })
            }),
        );
        for r in requests {
            let _ = self.requests.send((gen, id.to_owned(), r));
        }
        eprintln!(
            "korri-live: opened {id} at {w}x{h}: compile {} ms, total {} ms",
            compile.as_millis(),
            started.elapsed().as_millis()
        );
        self.current = Some(Surface {
            package,
            logic,
            logic_gen: gen,
            window_gen: gen,
            done: Self::done_for(gen),
            views,
            mounted,
            images,
            watch: Some(watch),
            last_view: view,
            last_overlay: None,
            gameplay,
            latest_model: model.clone(),
            journal: Journal {
                model,
                preferences,
                size,
                calls: Vec::new(),
                valid: true,
            },
            last_press: Instant::now(),
            size,
        });
        self.package_index = index;
        self.load_scenarios();
        self.refresh_tool();
        Ok(())
    }

    fn attach(&self, window: &slint::Window, gen: u64) {
        self.backend.attach(
            window,
            Rc::new(move |event| post(Ev::Host { gen, event })),
            self.frames.clone(),
        );
    }

    // ---- logic outcomes ------------------------------------------------

    fn logic_done(&mut self, gen: u64, outcome: Outcome) {
        let Some(mut s) = self.current.take() else { return };
        if s.logic_gen != gen {
            self.current = Some(s);
            return;
        }
        match outcome.result {
            Ok(step) => {
                if let Some(g) = step.gameplay_overlay {
                    s.gameplay = g;
                }
                if let Some((view, diff)) = step.view {
                    if !s.mounted.overlay {
                        if let Err(e) =
                            s.mounted
                                .binder
                                .apply_diff(&s.mounted.component, &view, &diff, &mut s.images)
                        {
                            eprintln!("korri-live: view refused: {e:#}");
                        }
                    }
                    s.last_view = view;
                }
                if let Some((view, diff)) = step.overlay {
                    if s.mounted.overlay {
                        let _ = s.mounted.binder.apply_diff(
                            &s.mounted.component,
                            &view,
                            &diff,
                            &mut s.images,
                        );
                    }
                    s.last_overlay = Some(view);
                }
                let want_overlay =
                    s.gameplay && s.views.overlay.is_some() && s.last_overlay.is_some();
                if want_overlay != s.mounted.overlay {
                    if let Err(e) = self.mount(&mut s, want_overlay, None) {
                        eprintln!("korri-live: mount: {e:#}");
                    }
                }
                for r in step.requests {
                    let _ = self.requests.send((s.window_gen, s.package.id.clone(), r));
                }
                if let (Some(revision), Some(watch)) = (outcome.revision, &s.watch) {
                    watch.ack(&revision);
                }
            }
            Err(violation) => {
                eprintln!("korri-live: logic violation: {violation}");
                self.status = format!("logic violation: {violation}");
                if let Some(watch) = &s.watch {
                    if let Some(revision) = outcome.revision {
                        watch.ack(&revision);
                    }
                }
            }
        }
        let done = s.done.clone();
        s.logic.returned(&done);
        self.current = Some(s);
        self.refresh_tool();
    }

    /// Put the Surface or the Overlay (from `views`, or the current ones) in the window,
    /// as a new instance with the last view applied whole.
    fn mount(
        &mut self,
        s: &mut Surface,
        overlay: bool,
        views: Option<&package::Views>,
    ) -> Result<()> {
        let views = views.unwrap_or(&s.views);
        let definition = if overlay {
            views.overlay.as_ref().context("no Overlay")?
        } else {
            &views.surface
        };
        let view = if overlay {
            s.last_overlay.clone().context("no overlay view")?
        } else {
            s.last_view.clone()
        };
        // Check first: the new instance replaces the window's content as it is created.
        view::check(&view, &view::declared(definition), &mut s.images)?;
        let component = definition.create_with_existing_window(s.mounted.component.window())?;
        let mut binder = Binder::new(definition);
        binder.apply_diff(&component, &view, &Diff::Replaced, &mut s.images)?;
        component.show()?;
        self.next_gen += 1;
        s.window_gen = self.next_gen;
        self.attach(component.window(), s.window_gen);
        component.window().request_redraw();
        s.mounted = Mounted {
            component,
            binder,
            overlay,
        };
        Ok(())
    }

    // ---- live reload ---------------------------------------------------

    fn changed(&mut self, changed: Vec<(PathBuf, SystemTime)>) {
        let Some(dir) = self.current.as_ref().map(|s| s.package.dir.clone()) else {
            return;
        };
        let (mut slint_saved, mut ts_saved, mut js_saved) = (None, None, None);
        for (path, modified) in changed {
            if !path.starts_with(&dir) {
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let slot = if name.ends_with(".slint") {
                &mut slint_saved
            } else if name == "logic.js" {
                &mut js_saved
            } else {
                &mut ts_saved
            };
            *slot = Some(slot.map_or(modified, |m: SystemTime| m.min(modified)));
        }
        if let Some(saved) = slint_saved {
            self.reload_views(saved);
        }
        if let Some(saved) = ts_saved {
            self.ts_saved.get_or_insert(saved);
            if self.transpiling {
                self.transpile_again = true;
            } else {
                self.transpile();
            }
        }
        if let Some(saved) = js_saved {
            // A transpile reloads when it ends; this is logic.js saved by hand.
            if !self.transpiling && self.js_written != Some(saved) {
                self.reload_logic(saved, Vec::new());
            }
        }
    }

    fn reload_views(&mut self, saved: SystemTime) {
        let Some(mut s) = self.current.take() else { return };
        let detected = SystemTime::now().duration_since(saved).unwrap_or_default();
        let started = Instant::now();
        let outcome = package::compile(&s.package).and_then(|views| {
            let compiled = started.elapsed();
            let overlay = s.mounted.overlay;
            self.mount(&mut s, overlay, Some(&views))?;
            s.views = views;
            Ok(compiled)
        });
        match outcome {
            Ok(compiled) => {
                let focus = focused_paths(&s.last_view);
                self.supersede();
                self.pending = Some(Pending {
                    kind: ".slint",
                    saved,
                    parts: vec![
                        ("seen", detected),
                        ("compile", compiled),
                        ("swap", started.elapsed() - compiled),
                    ],
                    detail: format!(
                        "views swapped, logic untouched ({} calls kept); focus {}",
                        s.journal.calls.len(),
                        focus.first().map_or("none", String::as_str)
                    ),
                    kept: true,
                    failed: false,
                });
            }
            Err(e) => {
                let message = format!("{e:#}");
                eprintln!("korri-live: .slint reload refused: {message}");
                self.add_reload(".slint", "refused".into(), message.chars().take(160).collect(), false, true);
            }
        }
        self.current = Some(s);
        self.refresh_tool();
    }

    fn transpile(&mut self) {
        let Some(s) = self.current.as_ref() else { return };
        let id = s.package.id.clone();
        let cwd = self.root.join("..");
        self.transpiling = true;
        self.transpile_again = false;
        thread::spawn(move || {
            let started = Instant::now();
            let result = Command::new("bun")
                .args(["run", &format!("{id}:build")])
                .current_dir(cwd)
                .output()
                .map_err(|e| format!("bun: {e}"))
                .and_then(|o| {
                    if o.status.success() {
                        Ok(())
                    } else {
                        Err(String::from_utf8_lossy(&o.stderr)
                            .lines()
                            .rev()
                            .take(4)
                            .collect::<Vec<_>>()
                            .join(" / "))
                    }
                });
            post(Ev::Transpiled {
                took: started.elapsed(),
                result,
            });
        });
    }

    fn transpiled(&mut self, took: Duration, result: Result<(), String>) {
        self.transpiling = false;
        self.js_written = self
            .current
            .as_ref()
            .and_then(|s| std::fs::metadata(s.package.dir.join("logic.js")).ok())
            .and_then(|m| m.modified().ok());
        let saved = self.ts_saved.take().unwrap_or_else(SystemTime::now);
        match result {
            Ok(()) => self.reload_logic(saved, vec![("bun build", took)]),
            Err(e) => {
                eprintln!("korri-live: transpile failed: {e}");
                self.add_reload("logic.ts", "bun build failed".into(), e, false, true);
            }
        }
        if self.transpile_again {
            self.ts_saved.get_or_insert_with(SystemTime::now);
            self.transpile();
        }
    }

    fn reload_logic(&mut self, saved: SystemTime, mut parts: Vec<(&'static str, Duration)>) {
        let Some(mut s) = self.current.take() else { return };
        let kind = if parts.is_empty() { "logic.js" } else { "logic.ts" };
        let started = Instant::now();
        let mut mode = self.mode;
        if mode == Mode::Replay && !s.journal.valid {
            mode = Mode::Fresh;
        }
        let result = (|| -> Result<(Logic, Value, Diff, Vec<Value>, usize)> {
            let source = s.package.logic_source()?;
            let seed = if mode == Mode::Transfer {
                let replies = s
                    .logic
                    .now(|w| w.engine().update(&json!({"_tag": "__LiveExport"})))
                    .and_then(|r| r)
                    .map_err(|v| anyhow!("export: {v}"))?;
                Some(
                    replies
                        .iter()
                        .find_map(|r| r.get("json").and_then(Value::as_str).map(str::to_owned))
                        .context("the old logic did not export its state")?,
                )
            } else {
                None
            };
            let logic = Logic::start(
                shim(&source, seed.as_deref()),
                Limits::default(),
                &s.package.id,
            )
            .map_err(|v| anyhow!("load: {v}"))?;
            let journal = s.journal.clone();
            let latest = s.latest_model.clone();
            let size = s.size;
            let stored = self.korrid.preferences(&s.package.id).unwrap_or_default();
            let preferences = engine::effective_preferences(logic.declaration(), &stored);
            let replayed = journal.calls.len();
            let (view, diff, requests) = logic
                .now(move |worker| -> Result<_, engine::Violation> {
                    let e = worker.engine();
                    let mut requests = Vec::new();
                    match mode {
                        Mode::Replay => {
                            e.set_model(&journal.model)?;
                            e.init(&journal.preferences)?;
                            e.update(&messages::resized(journal.size.0, journal.size.1))?;
                            for call in &journal.calls {
                                match call {
                                    Call::Message(m) => {
                                        e.update(m)?;
                                    }
                                    Call::Model { model, .. } => {
                                        e.set_model(model)?;
                                        e.update(&messages::korri_published())?;
                                    }
                                }
                            }
                        }
                        Mode::Transfer => {
                            e.set_model(&latest)?;
                            e.init(&preferences)?;
                        }
                        Mode::Fresh => {
                            e.set_model(&latest)?;
                            e.init(&preferences)?;
                            requests = e.update(&messages::resized(size.0, size.1))?;
                        }
                    }
                    e.views_drawn();
                    let view = e.view()?;
                    let diff = worker.diff_view(&view);
                    Ok((view, diff, requests))
                })
                .and_then(|r| r)
                .map_err(|v| anyhow!("{v}"))?;
            Ok((logic, view, diff, requests, replayed))
        })();
        match result {
            Ok((logic, view, diff, requests, replayed)) => {
                parts.push(("reload", started.elapsed()));
                let before = std::mem::replace(&mut s.last_view, view.clone());
                let applied = if s.mounted.overlay {
                    Ok(0)
                } else {
                    s.mounted
                        .binder
                        .apply_diff(&s.mounted.component, &view, &diff, &mut s.images)
                };
                if let Err(e) = applied {
                    s.last_view = before;
                    self.add_reload(kind, "view refused".into(), format!("{e:#}"), false, true);
                    self.current = Some(s);
                    return;
                }
                self.next_gen += 1;
                let gen = self.next_gen;
                s.logic = logic; // drops the old engine and joins its thread
                s.logic_gen = gen;
                s.done = Self::done_for(gen);
                s.window_gen = gen;
                self.attach(s.mounted.component.window(), gen);
                s.mounted.component.window().request_redraw();
                // Rebind the model watch to the new generation.
                let id = s.package.id.clone();
                s.watch = None;
                s.watch = Some(self.korrid.watch(
                    &id,
                    Box::new(move |event| {
                        post(match event {
                            WatchEvent::Model { revision, model } => Ev::Model {
                                gen,
                                revision: Some(revision),
                                model,
                            },
                            WatchEvent::Reconnected => Ev::Reconnected { gen },
                        })
                    }),
                ));
                for r in requests {
                    let _ = self.requests.send((gen, id.clone(), r));
                }
                let (f0, f1) = (focused_paths(&before), focused_paths(&view));
                let diffs = differences(&before, &view);
                let kept = f0 == f1;
                match mode {
                    Mode::Replay => {}
                    Mode::Transfer => s.journal.valid = false,
                    Mode::Fresh => {
                        s.journal = Journal {
                            model: s.latest_model.clone(),
                            preferences: s.journal.preferences.clone(),
                            size: s.size,
                            calls: Vec::new(),
                            valid: true,
                        };
                    }
                }
                let detail = format!(
                    "{mode:?}{}: focus {} → {}; {} view values differ{}",
                    if mode == Mode::Replay { format!(" of {replayed} calls") } else { String::new() },
                    f0.first().map_or("none", String::as_str),
                    f1.first().map_or("none", String::as_str),
                    diffs.len(),
                    diffs
                        .first()
                        .map_or(String::new(), |d| format!(" (first {d})")),
                );
                eprintln!("korri-live: logic reload: {detail}");
                self.supersede();
                self.pending = Some(Pending {
                    kind,
                    saved,
                    parts,
                    detail,
                    kept,
                    failed: false,
                });
            }
            Err(e) => {
                let message = format!("{e:#}");
                eprintln!("korri-live: logic reload refused, old logic kept: {message}");
                self.add_reload(kind, "refused".into(), message, false, true);
            }
        }
        self.current = Some(s);
        self.refresh_tool();
    }

    /// A second reload before the first one's frame: that frame never drew.
    fn supersede(&mut self) {
        if let Some(p) = self.pending.take() {
            eprintln!("LIVE_RELOAD_SUPERSEDED package={} kind={} (another save came before its frame)", PACKAGES[self.package_index], p.kind);
        }
    }

    fn frame_drawn(&mut self) {
        let Some(p) = self.pending.take() else { return };
        let total = SystemTime::now()
            .duration_since(p.saved)
            .unwrap_or_default();
        let parts = p
            .parts
            .iter()
            .map(|(k, d)| format!("{k} {}", d.as_millis()))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "LIVE_RELOAD package={} kind={} save_to_frame_ms={} parts=[{parts}] kept={} detail={}",
            PACKAGES[self.package_index],
            p.kind,
            total.as_millis(),
            p.kept,
            p.detail
        );
        self.add_reload(
            p.kind,
            format!("{} ms ({parts})", total.as_millis()),
            p.detail,
            p.kept,
            p.failed,
        );
    }

    fn add_reload(&mut self, kind: &str, ms: String, detail: String, kept: bool, failed: bool) {
        self.reloads
            .insert(0, (format!("{} {kind}", PACKAGES[self.package_index]), ms, detail, kept, failed));
        self.reloads.truncate(3);
        self.refresh_tool();
    }

    // ---- the bench -----------------------------------------------------

    fn tool(&mut self, action: Tool) {
        match action {
            Tool::Package(i) if i != self.package_index && i < PACKAGES.len() => {
                self.steps.clear();
                if let Err(e) = self.open(i) {
                    eprintln!("korri-live: {} not opened: {e:#}", PACKAGES[i]);
                    self.status = format!("{} not opened: {e:#}", PACKAGES[i]);
                }
            }
            Tool::Package(_) => {}
            Tool::Device(i) if i < DEVICES.len() => {
                self.device_index = i;
                let (w, h) = self.device();
                if let Some(s) = self.current.as_ref() {
                    let window = s.mounted.component.window();
                    fix_size(window, w, h, &s.package.id);
                    self.backend.place(
                        window,
                        Placement::Windowed {
                            width: w,
                            height: h,
                        },
                    );
                    if s.package.id == "pico" {
                        self.fixture.send(&format!(
                            "displays [{{\"id\":\"live\",\"main\":true,\"x\":0,\"y\":0,\"width\":{w},\"height\":{h},\"touch\":false}}]"
                        ));
                    }
                }
                self.refresh_tool();
            }
            Tool::Device(_) => {}
            Tool::Scenario(i) => {
                let Some(scenario) = self.scenarios.get(i).cloned() else {
                    return;
                };
                self.scenario_index = i as i32;
                self.steps.clear();
                if let Some(first) = scenario.models.first() {
                    self.fixture.send(&format!("model {}", first.display()));
                }
                if self.package_index == 1 {
                    let displays = scenario.displays.clone().unwrap_or_else(|| {
                        let (w, h) = self.device();
                        json!([{"id":"live","main":true,"x":0,"y":0,"width":w,"height":h,"touch":false}])
                    });
                    self.fixture.send(&format!("displays {displays}"));
                }
                eprintln!("korri-live: model {}", scenario.name);
                self.refresh_tool();
            }
            Tool::Mode(i) => {
                self.mode = [Mode::Replay, Mode::Transfer, Mode::Fresh][i.min(2)];
                self.refresh_tool();
            }
            Tool::Play => {
                if let Some(s) = self.scenarios.get(self.scenario_index.max(0) as usize) {
                    self.steps = s.steps.iter().cloned().collect();
                    self.next_step();
                }
            }
        }
    }

    fn next_step(&mut self) {
        let Some(step) = self.steps.pop_front() else {
            self.step_timer = None;
            return;
        };
        let delay = match step {
            Step::Press(b) => {
                self.press(b);
                0.25
            }
            Step::Model(n) => {
                if let Some(path) = self
                    .scenarios
                    .get(self.scenario_index.max(0) as usize)
                    .and_then(|s| s.models.get(n))
                {
                    self.fixture.send(&format!("model {}", path.display()));
                }
                0.6
            }
            Step::Wait(s) => s,
        };
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_secs_f64(delay),
            || post(Ev::Step),
        );
        self.step_timer = Some(timer);
    }

    /// The package's own fixture models: Zone's preview/scenarios (written from Zone's
    /// fixtures), Pico's preview/scenarios.ts (run with bun), and the shell's fixtures.
    fn load_scenarios(&mut self) {
        let dir = self.private.join(PACKAGES[self.package_index]);
        let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
        let mut list = vec![Scenario {
            name: "shell fixture".into(),
            reference: "clients/shell/fixtures/model.json".into(),
            models: vec![self.root.join("../fixtures/model.json")],
            steps: Vec::new(),
            displays: None,
        }];
        let result = if self.package_index == 0 {
            zone_scenarios(&self.root.join("zone/preview/scenarios"), &dir, &mut list)
        } else {
            pico_scenarios(&self.root.join("pico"), &dir, &mut list)
        };
        if let Err(e) = result {
            eprintln!("korri-live: scenarios: {e:#}");
            self.status = format!("scenarios: {e:#}");
        }
        self.scenarios = list;
        self.scenario_index = 0;
    }

    fn refresh_tool(&self) {
        let t = &self.tool;
        let _ = t.set_property("packages", strings(PACKAGES.iter().map(|s| s.to_string())));
        let _ = t.set_property("package-index", SValue::Number(self.package_index as f64));
        let _ = t.set_property(
            "devices",
            strings(DEVICES.iter().map(|d| format!("{} {}×{}", d.0, d.1, d.2))),
        );
        let _ = t.set_property("device-index", SValue::Number(self.device_index as f64));
        let _ = t.set_property("modes", strings(MODES.iter().map(|s| s.to_string())));
        let _ = t.set_property(
            "mode-index",
            SValue::Number(match self.mode {
                Mode::Replay => 0.0,
                Mode::Transfer => 1.0,
                Mode::Fresh => 2.0,
            }),
        );
        let same = matches!(t.get_property("scenarios"), Ok(SValue::Model(m)) if m.row_count() == self.scenarios.len());
        if !same || self.scenarios.is_empty() {
            let rows: Vec<SValue> = self
                .scenarios
                .iter()
                .map(|s| {
                    record(&[
                        ("name", SValue::String(s.name.clone().into())),
                        ("ref", SValue::String(s.reference.clone().into())),
                        ("steps", SValue::Number(s.steps.len() as f64)),
                    ])
                })
                .collect();
            let _ = t.set_property(
                "scenarios",
                SValue::Model(slint::ModelRc::new(slint::VecModel::from(rows))),
            );
        }
        let _ = t.set_property("scenario-index", SValue::Number(self.scenario_index as f64));
        let rows: Vec<SValue> = self
            .reloads
            .iter()
            .map(|(kind, ms, detail, kept, failed)| {
                record(&[
                    ("kind", SValue::String(kind.clone().into())),
                    ("ms", SValue::String(ms.clone().into())),
                    ("detail", SValue::String(detail.clone().into())),
                    ("kept", SValue::Bool(*kept)),
                    ("failed", SValue::Bool(*failed)),
                ])
            })
            .collect();
        let _ = t.set_property(
            "reloads",
            SValue::Model(slint::ModelRc::new(slint::VecModel::from(rows))),
        );
        let status = match self.current.as_ref() {
            Some(s) => format!(
                "{} running: {} logic calls since load{}{}",
                s.package.id,
                s.journal.calls.len(),
                if s.journal.valid { "" } else { " (transferred state)" },
                if self.status.is_empty() { String::new() } else { format!(". {}", self.status) }
            ),
            None => self.status.clone(),
        };
        let _ = t.set_property("status", SValue::String(status.into()));
        let _ = &self.private;
    }
}

/// Hold the package window at the device's CSS size: a tiling compositor otherwise gives
/// it a tile, and the package would lay out for that.
fn fix_size(window: &slint::Window, w: u32, h: u32, id: &str) {
    use slint::winit_030::{winit::dpi::LogicalSize, WinitWindowAccessor};
    let title = format!("korri-live {id} {w}x{h}");
    window.with_winit_window(|win| {
        let size = LogicalSize::new(w as f64, h as f64);
        win.set_min_inner_size(Some(size));
        win.set_max_inner_size(Some(size));
        win.set_resizable(false);
        win.set_title(&title);
    });
}

fn zone_scenarios(dir: &Path, out_dir: &Path, list: &mut Vec<Scenario>) -> Result<()> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    for file in files {
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
        let name = value["name"].as_str().unwrap_or("?").to_owned();
        let mut models = Vec::new();
        for (i, model) in value["models"].as_array().into_iter().flatten().enumerate() {
            let path = out_dir.join(format!("{name}.{i}.json"));
            std::fs::write(&path, serde_json::to_vec(model)?)?;
            models.push(path);
        }
        let steps = value["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|s| {
                if let Some(n) = s.strip_prefix("model ") {
                    n.parse().ok().map(Step::Model)
                } else if let Some(t) = s.strip_prefix("wait ") {
                    t.parse().ok().map(Step::Wait)
                } else {
                    button_of_step(s).map(Step::Press)
                }
            })
            .collect();
        let reference = value["ref"]
            .as_array()
            .map(|r| {
                r.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default();
        list.push(Scenario {
            name,
            reference,
            models,
            steps,
            displays: None,
        });
    }
    Ok(())
}

fn pico_scenarios(package: &Path, out_dir: &Path, list: &mut Vec<Scenario>) -> Result<()> {
    if !out_dir.join("scenarios.json").is_file() {
        let output = Command::new("bun")
            .arg("preview/scenarios.ts")
            .arg(out_dir)
            .current_dir(package)
            .output()
            .context("bun preview/scenarios.ts")?;
        if !output.status.success() {
            bail!(
                "bun preview/scenarios.ts: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    let index: Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("scenarios.json"))?)?;
    for s in index.as_array().into_iter().flatten() {
        let name = s["name"].as_str().unwrap_or("?").to_owned();
        let mut models = vec![out_dir.join(format!("{name}.json"))];
        models.extend(
            s["then"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(PathBuf::from),
        );
        let mut steps: Vec<Step> = s["keys"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(button_of_step)
            .map(Step::Press)
            .collect();
        steps.extend((1..models.len()).map(Step::Model));
        list.push(Scenario {
            name,
            reference: s["ref"].as_str().unwrap_or("").to_owned(),
            models,
            steps,
            displays: s.get("displays").filter(|d| !d.is_null()).cloned(),
        });
    }
    Ok(())
}
