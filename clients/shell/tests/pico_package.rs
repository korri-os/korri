//! Pico's package (packages/pico): its calls fit the shell's 50 ms call
//! budget (contract 3) on the slowest device clock.
//!
//! Pico's own gates, previews and focus walk run with `bun test` in
//! packages/pico/test (`nix run .#pico-check`).

use korri_shell::{catalog, engine};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf};

fn package() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/pico")
}

/// The fixture korrid's side of the catalog: a 10,000-game library behind a
/// cursor window, the focused game, and the rest of the model as published.
struct Korri {
    model: Value,
    games: Vec<Value>,
    query: Value,
    from: Value,
    focused: Option<String>,
    actions: Value,
    /// korrid is still reading the library: the catalog is Loading.
    loading: bool,
}

impl Korri {
    fn new(mut model: Value, games: Vec<Value>) -> Self {
        let actions = model
            .pointer("/focusedGame/actions")
            .cloned()
            .unwrap_or(json!([]));
        model.as_object_mut().unwrap().remove("focusedGame");
        Self {
            model,
            games,
            query: catalog::default_query(),
            from: json!({"_tag": "Start"}),
            focused: None,
            actions,
            loading: true,
        }
    }
    fn published(&self) -> String {
        let mut model = self.model.clone();
        model["catalog"] = if self.loading {
            json!({"_tag": "Loading"})
        } else {
            catalog::window(&self.games, &self.query, &self.from, catalog::DEFAULT_COUNT)
        };
        if let Some(id) = &self.focused {
            model["focusedGame"] = json!({"gameId": id, "actions": self.actions});
        }
        model.to_string()
    }
    /// Answer requests; true when the model changed.
    fn answer(&mut self, requests: &[Value]) -> bool {
        let mut changed = false;
        for request in requests {
            match request["_tag"].as_str() {
                Some("MoveCatalog") => {
                    self.query = request["query"].clone();
                    self.from = request["from"].clone();
                    changed = true;
                }
                Some("FocusGame") => {
                    self.focused = request["gameId"].as_str().map(str::to_owned);
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }
}

/// The shell's calls as it makes them, each counted: `update` with the
/// message, then, when Korri answered a request, `setModel` and
/// `update(KorriPublished)`; then `view` and `overlay` when the state moved.
struct Run {
    engine: engine::Engine,
    korri: Korri,
    counter: Option<Instructions>,
    costs: BTreeMap<String, Vec<u64>>,
    /// The last view drawn, to tell where the presses led.
    drawn: Value,
}

impl Run {
    fn now(&self) -> u64 {
        self.counter
            .as_ref()
            .map_or_else(cpu_nanos, Instructions::read)
    }
    fn count<T>(&mut self, what: String, call: impl FnOnce(&mut engine::Engine) -> T) -> T {
        let started = self.now();
        let out = call(&mut self.engine);
        let cost = self.now() - started;
        self.costs.entry(what).or_default().push(cost);
        out
    }
    /// One message, labelled, and every publish it causes.
    fn deliver(&mut self, label: &str, message: Value) {
        let mut queue = vec![(format!("update {label}"), message)];
        while let Some((what, message)) = queue.pop() {
            let requests = self.count(what, |e| e.update(&message).unwrap());
            if self.korri.answer(&requests) {
                let published = self.korri.published();
                self.count(format!("setModel after {label}"), |e| {
                    e.set_model(&published).unwrap()
                });
                queue.push((
                    format!("update KorriPublished after {label}"),
                    json!({"_tag": "KorriPublished"}),
                ));
            }
        }
        self.draw(label);
    }
    /// A publish from Korri's side: the model with one part changed.
    fn publish(&mut self, label: &str, change: impl FnOnce(&mut Value)) {
        change(&mut self.korri.model);
        let published = self.korri.published();
        self.count(format!("setModel {label}"), |e| {
            e.set_model(&published).unwrap()
        });
        self.deliver(label, json!({"_tag": "KorriPublished"}));
    }
    fn draw(&mut self, label: &str) {
        if self.engine.view_stale() {
            let view = self.count(format!("view after {label}"), |e| e.view().unwrap());
            merge(&mut self.drawn, view);
            self.count(format!("overlay after {label}"), |e| e.overlay().unwrap());
            self.engine.views_drawn();
        }
    }
}

/// Pico's calls are cheap enough for the 50 ms call budget on the RG353M at
/// 408 MHz (an idle clock), over 10,000 games (a 50-game window) at 640x480:
/// every update, view and overlay the shell makes as the library arrives,
/// along the shelf past the window's edge both ways, while Pico idles (the
/// shell's Tick each second, attract, the clock's minute, a status), on a
/// game's own screen, MENU, Find, Settings, the grid and the hero, a resize,
/// and the pause menu. Each stays under its limit on the device (`limit_ms`),
/// estimated from the instructions it runs here (or, where the counter is not available, from its CPU time, with
/// room for a busy machine).
#[test]
fn calls_stay_well_inside_the_budget_on_a_slow_clock() {
    let source = std::fs::read_to_string(
        std::env::var("PICO_LOGIC")
            .map(PathBuf::from)
            .unwrap_or_else(|_| package().join("logic.js")),
    )
    .unwrap();
    let (model, mut examples) = korri_shell::verify::fixture();
    // As korrid publishes games today: no cover art (services/korrid/src/surface).
    // PICO_TIMING_ART=1 keeps the fixture's inline art.
    if std::env::var_os("PICO_TIMING_ART").is_none() {
        for game in &mut examples {
            game.as_object_mut().unwrap().remove("coverArtUrl");
        }
    }
    let korri = Korri::new(model, catalog::games(&examples, 10_000));
    let mut engine = engine::Engine::load(
        &source,
        engine::Limits {
            call: std::time::Duration::from_secs(5),
            ..engine::Limits::default()
        },
    )
    .unwrap();
    engine.set_model(&korri.published()).unwrap();
    let preferences = engine::effective_preferences(engine.declaration(), &BTreeMap::new());
    engine.init(&preferences).unwrap();
    let mut run = Run {
        engine,
        korri,
        counter: Instructions::open(),
        costs: BTreeMap::new(),
        drawn: json!({}),
    };
    // Boot: korrid is still reading the library; the shell sends the window.
    run.deliver(
        "Resized",
        json!({"_tag": "Resized", "width": 640, "height": 480}),
    );
    // The library arrives.
    run.korri.loading = false;
    run.publish("catalog arrives", |_| {});
    // Along the shelf and back, past the window's edge both ways.
    for _ in 0..40 {
        run.deliver(
            "Pressed right",
            json!({"_tag": "Pressed", "button": "right"}),
        );
    }
    for _ in 0..40 {
        run.deliver("Pressed left", json!({"_tag": "Pressed", "button": "left"}));
    }
    // Idle: the shell's Tick each second (attract opens at 45 s), the
    // clock's minute, a status and back, and a publish that changes nothing.
    for second in 1..=60u64 {
        run.deliver("Tick", json!({"_tag": "Tick", "idleMs": second * 1_000}));
        if second % 20 == 0 {
            run.publish("clock", |m| {
                m["clockLabel"] = json!(format!("21:{}", 47 + second / 20))
            });
        }
        if second == 30 {
            run.publish(
                "status",
                |m| {
                    m["status"] =
                        json!({"_tag": "Busy", "kicker": "STARTING", "detail": "Celeste Classic"})
                },
            );
            run.publish("status back", |m| m["status"] = json!({"_tag": "Browsing"}));
            run.publish("catalog again", |_| {});
        }
    }
    run.deliver(
        "Pressed confirm (wakes)",
        json!({"_tag": "Pressed", "button": "confirm"}),
    );
    let press = |run: &mut Run, label: &str, button: &str| {
        run.deliver(label, json!({"_tag": "Pressed", "button": button}));
    };
    let at = |run: &Run, pointer: &str, want: Value| {
        assert_eq!(run.drawn.pointer(pointer), Some(&want), "{pointer}");
    };
    // MENU from wherever home's cursor is: down to the MENU key, then confirm.
    let open_menu = |run: &mut Run| {
        for _ in 0..10 {
            let lead = &run.drawn["frame"]["lead"];
            if run.drawn["page"] == "home" && lead["target"] == true && lead["focused"] == true {
                break;
            }
            press(run, "Pressed down (to MENU)", "down");
        }
        press(run, "Pressed confirm (open MENU)", "confirm");
        assert_eq!(run.drawn.pointer("/home/menuOpen"), Some(&json!(true)));
    };
    // A game's own screen, and back.
    press(&mut run, "Pressed confirm (open the game)", "confirm");
    at(&run, "/page", json!("detail"));
    press(&mut run, "Pressed down (game)", "down");
    press(&mut run, "Pressed back (game)", "back");
    at(&run, "/page", json!("home"));
    // MENU: open, along, closed.
    open_menu(&mut run);
    press(&mut run, "Pressed down (menu)", "down");
    press(&mut run, "Pressed back (close MENU)", "back");
    at(&run, "/home/menuOpen", json!(false));
    // Find, over the whole library: along its keys and its results.
    open_menu(&mut run);
    press(&mut run, "Pressed confirm (open Find)", "confirm");
    at(&run, "/page", json!("find"));
    for _ in 0..4 {
        press(&mut run, "Pressed down (find)", "down");
    }
    for _ in 0..6 {
        press(&mut run, "Pressed right (find)", "right");
    }
    press(&mut run, "Pressed back (find)", "back");
    at(&run, "/page", json!("home"));
    // Settings.
    open_menu(&mut run);
    press(&mut run, "Pressed down (menu)", "down");
    press(&mut run, "Pressed confirm (SETTINGS)", "confirm");
    at(&run, "/page", json!("settings"));
    for _ in 0..3 {
        press(&mut run, "Pressed down (settings)", "down");
    }
    press(&mut run, "Pressed back (settings)", "back");
    at(&run, "/page", json!("home"));
    // The grid, the hero and the shelf again: the menu button cycles them.
    press(&mut run, "Pressed menu (grid)", "menu");
    at(&run, "/home/mode", json!("grid"));
    for _ in 0..6 {
        press(&mut run, "Pressed right (grid)", "right");
    }
    for _ in 0..3 {
        press(&mut run, "Pressed down (grid)", "down");
    }
    press(&mut run, "Pressed menu (hero)", "menu");
    at(&run, "/home/mode", json!("hero"));
    press(&mut run, "Pressed down (hero)", "down");
    for _ in 0..6 {
        press(&mut run, "Pressed right (hero)", "right");
    }
    press(&mut run, "Pressed menu (back to the shelf)", "menu");
    at(&run, "/home/mode", json!("shelf"));
    // A resize and back.
    run.deliver(
        "Resized",
        json!({"_tag": "Resized", "width": 480, "height": 800}),
    );
    run.deliver(
        "Resized",
        json!({"_tag": "Resized", "width": 640, "height": 480}),
    );
    // The pause menu over a game, and back to the catalog.
    let catalog_presentation = run.korri.model["presentation"].clone();
    run.publish("pause menu", |m| m["presentation"] = json!({"kind": "gameplay-overlay", "title": "Paused", "controls": [{"id": "resume", "label": "Resume", "enabled": true, "destructive": false, "dismissOnSuccess": true, "interaction": {"kind": "command"}}, {"id": "quit", "label": "Quit", "enabled": true, "destructive": true, "dismissOnSuccess": true, "interaction": {"kind": "command"}}], "groups": []}));
    run.deliver(
        "Pressed down (pause menu)",
        json!({"_tag": "Pressed", "button": "down"}),
    );
    run.publish("catalog again after the game", move |m| {
        m["presentation"] = catalog_presentation
    });
    // The same view again: only its JSON, both ways.
    for _ in 0..5 {
        run.count("view again (JSON only)".into(), |e| e.view().unwrap());
    }
    let instructions = run.counter.is_some();
    let unit = if instructions { "instructions" } else { "ns" };
    let device_ms = |n: u64| {
        if instructions {
            n as f64 / PICO_INSTRUCTIONS_PER_DEVICE_MS
        } else {
            n as f64 * PICO_SLOW_FACTOR / 1_000_000.0
        }
    };
    let mut over = Vec::new();
    for (what, list) in &run.costs {
        let worst = *list.iter().max().unwrap();
        let mut sorted = list.clone();
        sorted.sort();
        eprintln!(
            "{what}: {} calls, median {} {unit}, worst {worst} (about {:.1} ms on the device)",
            sorted.len(),
            sorted[sorted.len() / 2],
            device_ms(worst),
        );
        let limit = limit_ms(what);
        let worst_ms = device_ms(worst);
        let worst_ms = if instructions {
            worst_ms
        } else {
            worst_ms / 2.0
        };
        if worst_ms > limit {
            over.push(format!("{what}: about {worst_ms:.1} ms, over {limit} ms"));
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

/// A call's limit on the device, in ms: 10, but where README.md (Limits)
/// says why not. A window of games not seen before works out every cart it
/// shows (the library's first arrival, the shelf after the hero's window).
/// Find, the grid and the hero are bound by the size of their views: QuickJS
/// writes JSON by the value, and they send the keyboard, the window's 50
/// carts, and the RESUME list.
fn limit_ms(what: &str) -> f64 {
    const BOUND: [(&str, f64); 6] = [
        ("catalog arrives", 15.0),
        ("(back to the shelf)", 15.0),
        ("(open Find)", 20.0),
        ("(find)", 20.0),
        ("(grid)", 26.0),
        ("(hero)", 40.0),
    ];
    BOUND
        .iter()
        .find(|(part, _)| what.contains(part))
        .map_or(10.0, |(_, limit)| *limit)
}

/// `view` applied over what was drawn, as the shell keeps a field it is not sent.
fn merge(drawn: &mut Value, view: Value) {
    match (drawn, view) {
        (Value::Object(drawn), Value::Object(view)) => {
            for (key, value) in view {
                merge(drawn.entry(key).or_insert(Value::Null), value);
            }
        }
        (drawn, view) => *drawn = view,
    }
}

/// How much slower the RG353M at 408 MHz runs a surface's logic than this
/// workstation (an i9-11900H): Zone's measure (tests/zone_package.rs).
const PICO_SLOW_FACTOR: f64 = 28.0;

/// Workstation instructions per millisecond of the same call on the RG353M
/// at 408 MHz: Zone's measure (tests/zone_package.rs), 4.38 M instructions
/// for about 17 ms on the device.
const PICO_INSTRUCTIONS_PER_DEVICE_MS: f64 = 4_380_000.0 / 17.0;

/// The thread's CPU time in ns.
fn cpu_nanos() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
    now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64
}

/// The CPU's count of user-space instructions this thread retires
/// (perf_event_open), when the kernel lets a process count its own.
struct Instructions(i32);

impl Instructions {
    fn open() -> Option<Self> {
        // struct perf_event_attr, as far as this needs it (size 128).
        #[repr(C)]
        struct Attr {
            kind: u32,
            size: u32,
            config: u64,
            sample: u64,
            sample_type: u64,
            read_format: u64,
            flags: u64,
            rest: [u64; 10],
        }
        let attr = Attr {
            kind: 0, // PERF_TYPE_HARDWARE
            size: std::mem::size_of::<Attr>() as u32,
            config: 1, // PERF_COUNT_HW_INSTRUCTIONS
            sample: 0,
            sample_type: 0,
            read_format: 0,
            // exclude_kernel (bit 5) and exclude_hv (bit 6)
            flags: (1 << 5) | (1 << 6),
            rest: [0; 10],
        };
        let fd = unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                &attr as *const Attr,
                0,
                -1,
                -1,
                0,
            )
        };
        (fd >= 0).then_some(Self(fd as i32))
    }

    fn read(&self) -> u64 {
        let mut count = 0u64;
        unsafe { libc::read(self.0, &mut count as *mut u64 as *mut libc::c_void, 8) };
        count
    }
}

impl Drop for Instructions {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}
