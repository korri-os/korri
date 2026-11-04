//! Zone's package (packages/zone), drawn by the shell's own interpreter and
//! software renderer, display-free, at the five sizes Zone must work at:
//! 320x240, 640x480, 480x800 (portrait), 1280x720 and 1920x1080.
//!
//! - Every preview state of every component (`*.preview.slint`, the
//!   Caliper part's role) compiles and draws something at every size.
//! - Every focus target the logic reaches is inside the window: the
//!   scenarios Zone's own fixtures make (packages/zone/preview/scenarios),
//!   played through logic.js, then walked with every direction; after each
//!   press the focused control's mark (Zone's pink drop or edge) is drawn
//!   inside the window. Screens that hold no focus by design are listed.
//! - Every direction goes where contract 8.4 says: the shell sends
//!   `Resized` before the first view; before each direction press the walk
//!   reads what is drawn (each focus target's place and label, from its
//!   accessible properties: `accessible-item-selectable` marks a target,
//!   `accessible-item-selected` the focused one) and the view's regions
//!   (`focusRegion`, `focusActive`, `focusPlan`). A drawn target is its
//!   view entry by label; a label that stands in several regions (two rows
//!   of the same buttons, a card and a button with the same word) is told
//!   apart by drawing each of its entries alone focused. Inside the focused
//!   target's region the press must reach the target drawn nearest that way
//!   (spatial-focus.ts: ahead by more than 1 px, then along + 3 x across);
//!   with none there, the region's declared exit and its entry rule
//!   (`first`: top to bottom, then left to right, by centre; `active`;
//!   `remembered`),
//!   or no move without one. The clock is the test's own, stepped past every
//!   motion before anything is read. This is Zone's own gate until the
//!   shell's probe judges the same convention.
//!
//! `ZONE_PREVIEW_OUT=DIR` writes every frame as a PNG there.

use korri_shell::{catalog, engine, images::ImageCache, view};
use serde_json::{json, Value};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, WindowAdapter};
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
};

const SIZES: [(u32, u32); 5] = [
    (320, 240),
    (640, 480),
    (480, 800),
    (1280, 720),
    (1920, 1080),
];

/// Time the test has stepped its clock past: every motion ends inside it.
static SKIPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Step the clock past every motion (the longest, the title card's, is 700 ms).
fn settle() {
    SKIPPED.fetch_add(2_000, std::sync::atomic::Ordering::Relaxed);
}

thread_local! {
    static WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

struct Headless {
    started: std::time::Instant,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        WINDOW.with(|w| *w.borrow_mut() = Some(window.clone()));
        Ok(window)
    }
    fn duration_since_start(&self) -> std::time::Duration {
        self.started.elapsed()
            + std::time::Duration::from_millis(SKIPPED.load(std::sync::atomic::Ordering::Relaxed))
    }
}

fn install() {
    // The walk reads what is drawn through the element tree: it needs the
    // compiler's debug info (i_slint_compiler reads this when it configures).
    std::env::set_var("SLINT_EMIT_DEBUG_INFO", "1");
    let _ = slint::platform::set_platform(Box::new(Headless {
        started: std::time::Instant::now(),
    }));
}

fn package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/zone")
}

/// Render the window last created, at its size, as RGB565.
fn render(width: u32, height: u32) -> Vec<u16> {
    slint::platform::update_timers_and_animations();
    let window = WINDOW.with(|w| w.borrow().clone()).expect("a window");
    let mut buffer = vec![Rgb565Pixel::default(); (width * height) as usize];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, width as usize);
    });
    buffer.iter().map(|p| p.0).collect()
}

fn save(pixels: &[u16], width: u32, height: u32, name: &str) {
    let Some(dir) = std::env::var_os("ZONE_PREVIEW_OUT") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut rgb = Vec::with_capacity(pixels.len() * 3);
    for p in pixels {
        let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
        rgb.extend([
            (r << 3 | r >> 2) as u8,
            (g << 2 | g >> 4) as u8,
            (b << 3 | b >> 2) as u8,
        ]);
    }
    image::save_buffer(
        dir.join(format!("{name}.{width}x{height}.png")),
        &rgb,
        width,
        height,
        image::ColorType::Rgb8,
    )
    .unwrap();
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.to_string_lossy().ends_with(".preview.slint") {
            out.push(path);
        }
    }
}

#[test]
fn every_preview_state_draws_at_every_size() {
    install();
    let root = package();
    let mut files = Vec::new();
    walk(&root.join("ui"), &mut files);
    walk(&root.join("pages"), &mut files);
    files.sort();
    assert!(files.len() >= 55, "only {} preview files", files.len());
    let mut failures = Vec::new();
    let mut drawn = 0;
    let planned: BTreeMap<(u32, u32), Value> = SIZES
        .iter()
        .map(|&(w, h)| ((w, h), plan_for(&root, w, h)))
        .collect();
    for file in &files {
        let result =
            pollster::block_on(slint_interpreter::Compiler::default().build_from_path(file));
        if result.has_errors() {
            failures.push(format!(
                "{}: {:?}",
                file.display(),
                result.diagnostics().collect::<Vec<_>>()
            ));
            continue;
        }
        let stem = file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".preview.slint", "");
        for name in result.component_names() {
            let definition = result.component(name).unwrap();
            for (width, height) in SIZES {
                let instance = definition.create().unwrap();
                instance
                    .window()
                    .set_size(slint::PhysicalSize::new(width, height));
                // The logic's plan for this size, as main.slint gets it.
                let _ = instance.set_property("screen", screen_value(&planned[&(width, height)]));
                instance.show().unwrap();
                render(width, height);
                let pixels = render(width, height);
                let colours: std::collections::BTreeSet<u16> = pixels.iter().copied().collect();
                if colours.len() < 2 {
                    failures.push(format!("{stem} {name} at {width}x{height} drew nothing"));
                }
                save(&pixels, width, height, &format!("{stem}.{name}"));
                instance.hide().unwrap();
                drawn += 1;
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(drawn >= 5 * 100, "only {drawn} frames");
}

/// The logic's plan for a window: its view's `screen` after Resized.
fn plan_for(root: &Path, width: u32, height: u32) -> Value {
    let source = std::fs::read_to_string(root.join("logic.js")).unwrap();
    let mut engine = engine::Engine::load(
        &source,
        engine::Limits {
            call: std::time::Duration::from_secs(5),
            ..engine::Limits::default()
        },
    )
    .unwrap();
    let scenario: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("preview/scenarios/games.json")).unwrap(),
    )
    .unwrap();
    let korri = Korri::new(scenario["models"][0].clone());
    engine.set_model(&korri.published()).unwrap();
    let preferences = engine::effective_preferences(engine.declaration(), &BTreeMap::new());
    engine.init(&preferences).unwrap();
    engine
        .update(&json!({"_tag": "Resized", "width": width, "height": height}))
        .unwrap();
    engine.view().unwrap()["screen"].clone()
}

/// The plan's `screen` as the Screen struct (ui/types.slint).
fn screen_value(screen: &Value) -> slint_interpreter::Value {
    use slint_interpreter::Value as V;
    let mut out = slint_interpreter::Struct::default();
    for (key, value) in screen.as_object().unwrap() {
        let kebab: String = key
            .chars()
            .flat_map(|c| {
                if c.is_uppercase() {
                    vec!['-', c.to_ascii_lowercase()]
                } else {
                    vec![c]
                }
            })
            .collect();
        let value = match value {
            Value::Bool(b) => V::Bool(*b),
            Value::Number(n) => V::Number(n.as_f64().unwrap()),
            _ => unreachable!(),
        };
        out.set_field(kebab, value);
    }
    V::Struct(out)
}

/// RGB565 of Zone's pink (#ce6584): the focus drop and edge.
const PINK: u16 = (0xce >> 3) << 11 | (0x65 >> 2) << 5 | (0x84 >> 3);

fn focus_marks(pixels: &[u16]) -> usize {
    pixels.iter().filter(|&&p| p == PINK).count()
}

/// One focus target as drawn: its label, its box, whether it holds the focus.
#[derive(Clone, Debug)]
struct Target {
    label: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    selected: bool,
}

impl Target {
    fn key(&self) -> (String, i64, i64) {
        (
            self.label.clone(),
            self.x.round() as i64,
            self.y.round() as i64,
        )
    }
    fn centre(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

fn target_of(element: &i_slint_backend_testing::ElementHandle) -> Target {
    let at = element.absolute_position();
    let size = element.size();
    Target {
        label: element.accessible_label().unwrap_or_default().to_string(),
        x: at.x,
        y: at.y,
        w: size.width,
        h: size.height,
        selected: element.accessible_item_selected() == Some(true),
    }
}

/// What is drawn: every focus target, in its place.
fn drawn_targets(instance: &slint_interpreter::ComponentInstance) -> Vec<Target> {
    use i_slint_backend_testing::ElementRoot;
    instance
        .root_element()
        .query_descendants()
        .match_predicate(|e| e.accessible_item_selectable() == Some(true))
        .find_all()
        .iter()
        .map(target_of)
        .filter(|t| t.w > 0.0 || t.h >= 0.0)
        .collect()
}

/// The view's focus entries. Each is named `label#k`, the k-th entry with
/// that label in the view, so two rows of buttons with the same words, or a
/// card and a button with the same word, stay apart.
struct Regions {
    /// Every entry: its path in the view, label and name.
    entries: Vec<(Vec<Value>, String, String)>,
    /// Name to region.
    of: BTreeMap<String, String>,
    /// Region to the name of its active entry.
    active: BTreeMap<String, String>,
    /// The focused entry's name and region.
    focused: Option<(String, String)>,
}

fn regions(view: &Value) -> Regions {
    fn walk(
        value: &Value,
        path: &mut Vec<Value>,
        out: &mut Regions,
        seen: &mut BTreeMap<String, usize>,
    ) {
        match value {
            Value::Array(items) => items.iter().enumerate().for_each(|(i, item)| {
                path.push(json!(i));
                walk(item, path, out, seen);
                path.pop();
            }),
            Value::Object(entry) => {
                if let Some(region) = entry.get("focusRegion").and_then(Value::as_str) {
                    let label = ["label", "title", "header"]
                        .iter()
                        .find_map(|key| entry.get(*key).and_then(Value::as_str))
                        .unwrap_or("")
                        .to_owned();
                    let label = match (
                        entry.get("seat").and_then(Value::as_str),
                        entry.get("name").and_then(Value::as_str),
                    ) {
                        (Some(seat), Some(name)) => format!("{seat} {name}"),
                        _ => label,
                    };
                    let k = seen.entry(label.clone()).or_default();
                    *k += 1;
                    let name = format!("{label}#{k}");
                    out.entries.push((path.clone(), label, name.clone()));
                    out.of.insert(name.clone(), region.to_owned());
                    if entry.get("focusActive").and_then(Value::as_bool) == Some(true) {
                        out.active.insert(region.to_owned(), name.clone());
                    }
                    if entry.get("focused").and_then(Value::as_bool) == Some(true) {
                        out.focused = Some((name, region.to_owned()));
                    }
                }
                entry.iter().for_each(|(key, inner)| {
                    path.push(json!(key));
                    walk(inner, path, out, seen);
                    path.pop();
                });
            }
            _ => {}
        }
    }
    let mut out = Regions {
        entries: Vec::new(),
        of: BTreeMap::new(),
        active: BTreeMap::new(),
        focused: None,
    };
    walk(view, &mut Vec::new(), &mut out, &mut BTreeMap::new());
    // A label several entries of one region carry: `label#*`, any of them.
    for (_, label, name) in &out.entries {
        if !name.ends_with("#1") {
            let region = out.of[name].clone();
            let one = out
                .entries
                .iter()
                .filter(|(_, l, _)| l == label)
                .all(|(_, _, n)| out.of[n] == region);
            if one {
                out.of.insert(format!("{label}#*"), region);
            }
        }
    }
    out
}

/// Two names of the same target: equal, or the same label where one is `label#*`.
fn same(a: &str, b: &str) -> bool {
    let label = |n: &str| {
        n.rsplit_once('#')
            .map_or(n.to_owned(), |(l, _)| l.to_owned())
    };
    a == b || ((a.ends_with("#*") || b.ends_with("#*")) && label(a) == label(b))
}

/// Every `focused` in a view, false.
fn unfocused(view: &Value) -> Value {
    match view {
        Value::Array(items) => Value::Array(items.iter().map(unfocused).collect()),
        Value::Object(entry) => Value::Object(
            entry
                .iter()
                .map(|(key, inner)| {
                    (
                        key.clone(),
                        if key == "focused" {
                            json!(false)
                        } else {
                            unfocused(inner)
                        },
                    )
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Each drawn target's view entry, by name. A label one entry and one
/// drawn target carry is matched by it; otherwise the walk draws the view
/// with each entry of that label alone focused and sees which target is
/// drawn selected (then draws the view again as it was).
fn names(
    targets: &[Target],
    known: &Regions,
    view: &Value,
    instance: &slint_interpreter::ComponentInstance,
    binder: &mut view::Binder,
    images: &mut ImageCache,
) -> Vec<String> {
    let mut out: Vec<String> = targets.iter().map(|t| format!("{}#?", t.label)).collect();
    let mut labels: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, (_, label, _)) in known.entries.iter().enumerate() {
        labels.entry(label.as_str()).or_default().push(i);
    }
    let mut lit = false;
    for (label, entries) in labels {
        let drawn: Vec<usize> = (0..targets.len())
            .filter(|&i| targets[i].label == label)
            .collect();
        if entries.len() == 1 && !drawn.is_empty() {
            for &i in &drawn {
                out[i] = known.entries[entries[0]].2.clone();
            }
            continue;
        }
        let star = format!("{label}#*");
        if known.of.contains_key(&star) {
            for &i in &drawn {
                out[i] = star.clone();
            }
            continue;
        }
        // The label stands in several regions: light each entry alone.
        for &e in &entries {
            let mut alone = unfocused(view);
            let mut at = &mut alone;
            for step in &known.entries[e].0 {
                at = match step {
                    Value::String(key) => &mut at[key.as_str()],
                    Value::Number(i) => &mut at[i.as_u64().unwrap() as usize],
                    _ => unreachable!(),
                };
            }
            at["focused"] = json!(true);
            binder.apply(instance, &stripped(&alone), images).unwrap();
            lit = true;
            // A changed row's repeater instance is made again when the window draws.
            let size = instance.window().size();
            render(size.width, size.height);
            let now = drawn_targets(instance);
            if let Some(i) = now.iter().position(|t| t.selected) {
                // The same element: the i-th target drawn now is the i-th before.
                if i < out.len() && now.len() == targets.len() && now[i].label == targets[i].label {
                    out[i] = known.entries[e].2.clone();
                }
            }
        }
    }
    if lit {
        binder.apply(instance, &stripped(view), images).unwrap();
        let size = instance.window().size();
        render(size.width, size.height);
    }
    out
}

/// The view without the keys the shell strips before it binds (contract 8.4).
fn stripped(view: &Value) -> Value {
    match view {
        Value::Array(items) => Value::Array(items.iter().map(stripped).collect()),
        Value::Object(entry) => Value::Object(
            entry
                .iter()
                .filter(|(key, _)| {
                    !matches!(key.as_str(), "focusRegion" | "focusActive" | "focusPlan")
                })
                .map(|(key, inner)| (key.clone(), stripped(inner)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// spatial-focus.ts's choice: ahead by more than 1 px, then along + 3 x across.
/// The index in `reach` of the target chosen; the first on a tie.
fn nearest(from: &Target, reach: &[Target], direction: &str) -> Option<usize> {
    let (fx, fy) = from.centre();
    reach
        .iter()
        .enumerate()
        .filter(|(_, t)| t.key() != from.key() && t.w > 0.0)
        .filter_map(|(i, t)| {
            let (cx, cy) = t.centre();
            let (dx, dy) = (cx - fx, cy - fy);
            let (along, across) = match direction {
                "left" => (-dx, dy.abs()),
                "right" => (dx, dy.abs()),
                "up" => (-dy, dx.abs()),
                _ => (dy, dx.abs()),
            };
            (along > 1.0).then_some((along + across * 3.0, i))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i)
}

/// A mini korrid for the walk: the scenario's model, the cursor window over
/// its games, MoveCatalog and FocusGame answered as korrid would.
struct Korri {
    model: Value,
    games: Option<Vec<Value>>,
    query: Value,
    from: Value,
    focused: Option<String>,
    actions: Value,
}

impl Korri {
    fn new(mut model: Value) -> Self {
        let games = model["catalog"]["games"].as_array().cloned();
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
        }
    }
    fn published(&self) -> String {
        let mut model = self.model.clone();
        if let Some(games) = &self.games {
            model["catalog"] =
                catalog::window(games, &self.query, &self.from, catalog::DEFAULT_COUNT);
        }
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

/// Scenarios whose screen holds no focus by design: nothing on it takes the cursor.
const NO_FOCUS: [&str; 8] = [
    "games-loading",    // LOADING: nothing to choose until Korri publishes
    "games-empty",      // NO GAMES with no store: nothing to choose
    "games-busy",       // Busy without an action of Korri's: the page is inert
    "games-title-card", // the title card holds the page until Korri answers
    "games-tally",      // the tally is one button with no mark: any press closes it
    "games-attract",    // the attract is a screensaver
    "savedata-working", // Korri is working: nothing to press
    "status-progress", // the strip's CANCEL is focused but lit, not pink-dropped: checked by its lit drop
];

#[test]
fn every_reachable_focus_target_is_inside_the_window_at_every_size() {
    install();
    let root = package();
    let views =
        korri_shell::package::compile(&korri_shell::package::Package::at(&root).unwrap()).unwrap();
    let source = std::fs::read_to_string(root.join("logic.js")).unwrap();
    let mut scenarios: Vec<PathBuf> = std::fs::read_dir(root.join("preview/scenarios"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    scenarios.sort();
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut directions = 0;
    let mut wrong_way = Vec::new();
    for path in &scenarios {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if NO_FOCUS.contains(&name.as_str()) {
            continue;
        }
        // ZONE_GATE_ONLY=a,b walks only those scenarios (while working on them).
        if std::env::var("ZONE_GATE_ONLY").is_ok_and(|only| !only.split(',').any(|o| o == name)) {
            continue;
        }
        let scenario: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let steps: Vec<String> = scenario["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect();
        for (width, height) in SIZES {
            let mut korri = Korri::new(scenario["models"][0].clone());
            // A focus test, not a timing test: a busy machine must not fail it.
            // The verifier and the shell hold the 50 ms call budget.
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
            let overlay = korri.model["presentation"]["kind"] == "gameplay-overlay";
            let definition = if overlay {
                views.overlay.clone().unwrap()
            } else {
                views.surface.clone()
            };
            let instance = definition.create().unwrap();
            instance
                .window()
                .set_size(slint::PhysicalSize::new(width, height));
            instance.show().unwrap();
            let mut binder = view::Binder::new(&definition);
            let mut images = ImageCache::new(&root);
            let deliver = |engine: &mut engine::Engine, korri: &mut Korri, message: Value| {
                let mut queue = vec![message];
                while let Some(message) = queue.pop() {
                    let requests = engine.update(&message).unwrap();
                    if korri.answer(&requests) {
                        engine.set_model(&korri.published()).unwrap();
                        queue.push(json!({"_tag": "KorriPublished"}));
                    }
                }
            };
            // The shell's Resized comes before the first view (A2, 8.1).
            deliver(
                &mut engine,
                &mut korri,
                json!({"_tag": "Resized", "width": width, "height": height}),
            );
            deliver(&mut engine, &mut korri, json!({"_tag": "KorriPublished"}));
            let press = |button: &str| json!({"_tag": "Pressed", "button": button});
            for step in &steps {
                if let Some(index) = step.strip_prefix("model ") {
                    korri = Korri {
                        model: Korri::new(
                            scenario["models"][index.parse::<usize>().unwrap()].clone(),
                        )
                        .model,
                        ..korri
                    };
                    engine.set_model(&korri.published()).unwrap();
                    deliver(&mut engine, &mut korri, json!({"_tag": "KorriPublished"}));
                } else if !step.starts_with("wait") {
                    deliver(&mut engine, &mut korri, press(step));
                }
            }
            let mut check = |engine: &mut engine::Engine,
                             binder: &mut view::Binder,
                             images: &mut ImageCache,
                             label: &str|
             -> Value {
                let view = if overlay {
                    engine.overlay().unwrap().unwrap()
                } else {
                    engine.view().unwrap()
                };
                binder.apply(&instance, &stripped(&view), images).unwrap();
                render(width, height);
                settle();
                let pixels = render(width, height);
                checked += 1;
                if focus_marks(&pixels) < 12 {
                    save(&pixels, width, height, &format!("unfocused.{name}.{label}"));
                    failures.push(format!(
                        "{name} {width}x{height} after {label}: no focus mark in the window"
                    ));
                }
                save(&pixels, width, height, &format!("walk.{name}.{label}"));
                view
            };
            let mut view = check(&mut engine, &mut binder, &mut images, "steps");
            let mut remembered: BTreeMap<String, String> = BTreeMap::new();
            for (direction, times) in [("down", 8), ("right", 6), ("up", 8), ("left", 6)] {
                for n in 0..times {
                    let targets = drawn_targets(&instance);
                    let known = regions(&view);
                    let named = names(&targets, &known, &view, &instance, &mut binder, &mut images);
                    let region_of = |i: usize| known.of.get(&named[i]).cloned();
                    let from = targets.iter().position(|t| t.selected);
                    let plan = view.get("focusPlan").cloned().unwrap_or(json!({}));
                    deliver(&mut engine, &mut korri, press(direction));
                    view = check(
                        &mut engine,
                        &mut binder,
                        &mut images,
                        &format!("{direction}{n}"),
                    );
                    let (Some(from), Some((focused, region))) = (from, known.focused.clone())
                    else {
                        continue;
                    };
                    if !same(&named[from], &focused) {
                        wrong_way.push(format!("{name} {width}x{height}: {direction}{n}: the drawn focus {:?} is not the view's {focused:?}", named[from]));
                        if std::env::var_os("ZONE_GATE_DEBUG").is_some() {
                            for (i, t) in targets.iter().enumerate() {
                                eprintln!(
                                    "  drawn {} at ({:.0},{:.0}) {}x{} {}",
                                    named[i],
                                    t.x,
                                    t.y,
                                    t.w,
                                    t.h,
                                    if t.selected { "SELECTED" } else { "" }
                                );
                            }
                            eprintln!("  view {:?}", known.of);
                        }
                        continue;
                    }
                    remembered.insert(region.clone(), named[from].clone());
                    let inside: Vec<usize> = (0..targets.len())
                        .filter(|&i| region_of(i).as_ref() == Some(&region))
                        .collect();
                    let want = match nearest(
                        &targets[from],
                        &inside
                            .iter()
                            .map(|&i| targets[i].clone())
                            .collect::<Vec<_>>(),
                        direction,
                    ) {
                        Some(t) => format!("{} (in {region})", named[inside[t]]),
                        None => match plan.pointer(&format!("/{region}/{direction}")) {
                            None => format!("{} (stays)", named[from]),
                            Some(exit) => {
                                let to = exit["to"].as_str().unwrap_or("").to_owned();
                                let entry = exit["entry"].as_str().unwrap_or("");
                                // `first`: top to bottom, then left to right, by the box's centre.
                                let mut there: Vec<usize> = (0..targets.len())
                                    .filter(|&i| region_of(i).as_ref() == Some(&to))
                                    .collect();
                                there.sort_by(|&a, &b| {
                                    let ((ax, ay), (bx, by)) =
                                        (targets[a].centre(), targets[b].centre());
                                    ay.round()
                                        .total_cmp(&by.round())
                                        .then(ax.round().total_cmp(&bx.round()))
                                });
                                // A list scrolled past the region draws none of it: its first entry in the view.
                                let first =
                                    there.first().map(|&i| named[i].clone()).unwrap_or_else(|| {
                                        known
                                            .entries
                                            .iter()
                                            .find(|(_, _, n)| known.of.get(n) == Some(&to))
                                            .map(|(_, _, n)| n.clone())
                                            .unwrap_or_default()
                                    });
                                let label = match entry {
                                    "active" => known.active.get(&to).cloned().unwrap_or_default(),
                                    "remembered" => remembered.get(&to).cloned().unwrap_or(first),
                                    _ => first,
                                };
                                format!("{label} (exit to {to}, {entry})")
                            }
                        },
                    };
                    directions += 1;
                    let after = regions(&view)
                        .focused
                        .map(|(name, _)| name)
                        .unwrap_or_else(|| "nothing".into());
                    let wanted = want.split(" (").next().unwrap_or_default();
                    if !same(wanted, &after) {
                        wrong_way.push(format!(
                            "{name} {width}x{height}: {direction}{n} from {:?} in {region} went to {after:?}, want {want}",
                            named[from]
                        ));
                    }
                }
            }
            instance.hide().unwrap();
        }
    }
    if std::env::var("ZONE_GATE_ONLY").is_err() {
        assert!(checked > 1000, "only {checked} frames checked");
        assert!(directions > 1000, "only {directions} directions checked");
    }
    failures.extend(wrong_way);
    assert!(
        failures.is_empty(),
        "{} of {checked} frames:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// logic.js loads well inside the shell's 50 ms call budget (best of ten,
/// so a busy machine's worst case does not decide it).
#[test]
fn logic_loads_well_inside_the_call_budget() {
    let path = std::env::var("ZONE_LOGIC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| package().join("logic.js"));
    let source = std::fs::read_to_string(path).unwrap();
    let mut best = std::time::Duration::MAX;
    for _ in 0..10 {
        let started = std::time::Instant::now();
        let _ = engine::Engine::load(
            &source,
            engine::Limits {
                call: std::time::Duration::from_secs(5),
                ..engine::Limits::default()
            },
        )
        .unwrap();
        best = best.min(started.elapsed());
    }
    eprintln!("logic.js loads in {best:?} at best");
    assert!(best < std::time::Duration::from_millis(25), "{best:?}");
}

/// The plan measures words as .slint draws them (src/plan/measure.ts against
/// ZoneMeasure): every entry of preview/evidence/measures.json, written by
/// test/write-measures.ts, in every look and face, within a quarter px.
#[test]
fn the_plan_measures_words_as_slint_draws_them() {
    install();
    let root = package();
    let source = r#"
        import { Tokens } from "ui/tokens.slint";
        import { ZoneMeasure } from "ui/atoms/ZoneMeasure.slint";
        export component Measure {
            in property <string> look;
            in property <string> words;
            in property <length> cap;
            in property <string> face;
            out property <length> drawn: m.preferred-width;
            changed look => { Tokens.look = self.look; }
            m := ZoneMeasure { text: root.words; cap: root.cap; face: root.face; }
        }
    "#;
    let result = pollster::block_on(
        slint_interpreter::Compiler::default()
            .build_from_source(source.into(), root.join("measure.slint")),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let instance = result.component("Measure").unwrap().create().unwrap();
    let measures: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(root.join("preview/evidence/measures.json")).unwrap(),
    )
    .unwrap();
    let mut wrong = Vec::new();
    for m in &measures {
        use slint_interpreter::Value as V;
        instance
            .set_property("look", V::String(m["look"].as_str().unwrap().into()))
            .unwrap();
        // A changed callback runs before the next frame: let it write Tokens.look.
        slint::platform::update_timers_and_animations();
        instance
            .set_property("face", V::String(m["face"].as_str().unwrap().into()))
            .unwrap();
        instance
            .set_property("cap", V::Number(m["cap"].as_f64().unwrap()))
            .unwrap();
        instance
            .set_property("words", V::String(m["text"].as_str().unwrap().into()))
            .unwrap();
        let V::Number(drawn) = instance.get_property("drawn").unwrap() else {
            panic!()
        };
        let planned = m["width"].as_f64().unwrap();
        if (drawn - planned).abs() > 0.25 {
            wrong.push(format!(
                "{} {} {} {:?}: planned {planned:.2}, drawn {drawn:.2}",
                m["look"], m["face"], m["cap"], m["text"]
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {}:\n{}",
        wrong.len(),
        measures.len(),
        wrong.join("\n")
    );
}
