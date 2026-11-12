//! Velvet's package (packages/velvet), drawn by the shell's own interpreter
//! and software renderer, display-free, at the sizes Velvet must work at:
//! 1920x1080, 1240x1080, 640x480, 480x800 (portrait), and the verifier's
//! 320x240 and 1280x720.
//!
//! - Every preview state of every component (`*.preview.slint`, written by
//!   preview/previews.ts) compiles and draws something at five sizes, with
//!   the logic's plan for that size.
//! - Focus stays in sight: every scenario (preview/scenarios), played
//!   through logic.js against a small Korri (the fixture korrid's own
//!   catalog code, `korri_shell::catalog`), then walked on with a tour of
//!   presses. After every press, when the view has a focused entry, exactly
//!   one drawn target is selected (`accessible-item-selected`) and all of
//!   it is inside the window and every clip. Where a press goes (nearest
//!   drawn, then the region's exit) is the shell's probe's to judge
//!   (preview/probe.sh, `korri-shell verify`); this is the part the probe
//!   does not do: every scenario, at every size.
//! - The whole convention once a scenario (`korri_shell::targets::drawn`):
//!   each focus entry of the view selects one element, and no element is
//!   selected with no entry focused.
//! - The backdrop has no seam: the stage's gradients reach every edge of the
//!   window at every size (no neighbouring pixels more than a few levels
//!   apart), alone and under the home with no art.
//! - A system's set shows only that system's games, over the fixture
//!   korrid's catalog (it filters `MoveCatalog` by `query.systemId`).
//! - Budgets: logic.js loads well inside one call's budget; every `view()`
//!   over 10,000 games stays under 50 ms on a slow device's clock, the first
//!   sight of a screen and its cards included.
//!
//! `VELVET_FRAMES=DIR` (with `VELVET_SIZES`, `VELVET_ONLY`) writes each
//! scenario's last frame as a PNG; `VELVET_PREVIEW_OUT=DIR` writes every
//! preview state.

use korri_shell::{catalog, engine, images::ImageCache, targets, view};
use serde_json::{json, Value};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Rgb8Pixel};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
};

static SKIPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Step the clock past every motion.
fn settle() {
    SKIPPED.fetch_add(3_000, std::sync::atomic::Ordering::Relaxed);
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
    std::env::set_var("SLINT_EMIT_DEBUG_INFO", "1");
    let _ = slint::platform::set_platform(Box::new(Headless {
        started: std::time::Instant::now(),
    }));
}

fn package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/velvet")
}

/// Render the window last created, at its size, as RGB.
fn render(width: u32, height: u32) -> Vec<Rgb8Pixel> {
    slint::platform::update_timers_and_animations();
    let window = WINDOW.with(|w| w.borrow().clone()).expect("a window");
    let mut buffer = vec![Rgb8Pixel::default(); (width * height) as usize];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, width as usize);
    });
    buffer
}

fn save(pixels: &[Rgb8Pixel], width: u32, height: u32, dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let rgb: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    image::save_buffer(
        dir.join(format!("{name}.png")),
        &rgb,
        width,
        height,
        image::ColorType::Rgb8,
    )
    .unwrap();
}

/// A mini korrid: the scenario's model, the cursor window over its games,
/// `systems`, and MoveCatalog and FocusGame answered as korrid would.
struct Korri {
    model: Value,
    games: Option<Vec<Value>>,
    query: Value,
    from: Value,
    count: usize,
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
            count: catalog::DEFAULT_COUNT,
            focused: None,
            actions,
        }
    }
    fn published(&self) -> String {
        let mut model = self.model.clone();
        if let Some(games) = &self.games {
            model["catalog"] = catalog::window(games, &self.query, &self.from, self.count);
            model["systems"] = json!(catalog::systems(games));
        }
        if let Some(id) = &self.focused {
            model["focusedGame"] = json!({"gameId": id, "actions": self.actions});
        }
        model.to_string()
    }
    fn answer(&mut self, requests: &[Value]) -> bool {
        let mut changed = false;
        for request in requests {
            match request["_tag"].as_str() {
                Some("MoveCatalog") => {
                    self.query = request["query"].clone();
                    self.from = request["from"].clone();
                    self.count = request["count"].as_u64().unwrap_or(50) as usize;
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

fn deliver(engine: &mut engine::Engine, korri: &mut Korri, message: Value) {
    let mut queue = vec![message];
    while let Some(message) = queue.pop() {
        let requests = engine.update(&message).unwrap();
        if korri.answer(&requests) {
            engine.set_model(&korri.published()).unwrap();
            queue.push(json!({"_tag": "KorriPublished"}));
        }
    }
}

fn limits() -> engine::Limits {
    engine::Limits {
        stuck: std::time::Duration::from_secs(5),
        clock: korri_shell::budget_clock::BudgetClock::ThreadCpu,
        ..engine::Limits::default()
    }
}

/// The sizes the previews draw at (the verifier's five).
const PREVIEW_SIZES: [(u32, u32); 5] = [
    (320, 240),
    (640, 480),
    (480, 800),
    (1280, 720),
    (1920, 1080),
];
/// The sizes focus is proven at: the four the design names, and the verifier's other two.
const SIZES: [(u32, u32); 6] = [
    (1920, 1080),
    (1240, 1080),
    (640, 480),
    (480, 800),
    (320, 240),
    (1280, 720),
];

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

fn scenarios(root: &Path) -> Vec<(String, Value)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join("preview/scenarios"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    let only: Option<Vec<String>> = std::env::var("VELVET_ONLY")
        .ok()
        .map(|o| o.split(',').map(str::to_owned).collect());
    paths
        .iter()
        .map(|path| {
            (
                path.file_stem().unwrap().to_string_lossy().to_string(),
                path,
            )
        })
        .filter(|(name, _)| only.as_ref().is_none_or(|only| only.contains(name)))
        .map(|(name, path)| {
            (
                name,
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap(),
            )
        })
        .collect()
}

fn scenario(root: &Path, name: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(root.join(format!("preview/scenarios/{name}.json"))).unwrap(),
    )
    .unwrap()
}

/// logic.js loaded and started on a scenario's first model, at a size.
fn start(root: &Path, model: &Value, width: u32, height: u32) -> (engine::Engine, Korri) {
    let source = std::fs::read_to_string(root.join("logic.js")).unwrap();
    let mut korri = Korri::new(model.clone());
    let mut engine = engine::Engine::load(&source, limits()).unwrap();
    engine.set_model(&korri.published()).unwrap();
    engine
        .init(&engine::effective_preferences(
            engine.declaration(),
            &BTreeMap::new(),
        ))
        .unwrap();
    // The shell's Resized comes before the first view.
    deliver(
        &mut engine,
        &mut korri,
        json!({"_tag": "Resized", "width": width, "height": height}),
    );
    deliver(&mut engine, &mut korri, json!({"_tag": "KorriPublished"}));
    (engine, korri)
}

fn step(engine: &mut engine::Engine, korri: &mut Korri, scenario: &Value, step: &str) {
    if let Some(index) = step.strip_prefix("model ") {
        let next = Korri::new(scenario["models"][index.parse::<usize>().unwrap()].clone());
        korri.model = next.model;
        korri.games = next.games;
        korri.actions = next.actions;
        engine.set_model(&korri.published()).unwrap();
        deliver(engine, korri, json!({"_tag": "KorriPublished"}));
    } else if let Some(seconds) = step.strip_prefix("wait") {
        let ms = (seconds.trim().parse::<f64>().unwrap_or(1.0) * 1000.0) as u64;
        deliver(engine, korri, json!({"_tag": "Tick", "idleMs": ms}));
    } else {
        deliver(engine, korri, json!({"_tag": "Pressed", "button": step}));
    }
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

#[test]
fn every_preview_state_draws_at_every_size() {
    install();
    let root = package();
    let mut files = Vec::new();
    walk(&root.join("ui"), &mut files);
    walk(&root.join("pages"), &mut files);
    files.sort();
    assert!(files.len() >= 45, "only {} preview files", files.len());
    let home = scenario(&root, "home");
    let planned: BTreeMap<(u32, u32), Value> = PREVIEW_SIZES
        .iter()
        .map(|&(w, h)| {
            (
                (w, h),
                start(&root, &home["models"][0], w, h).0.view().unwrap()["screen"].clone(),
            )
        })
        .collect();
    let out = std::env::var_os("VELVET_PREVIEW_OUT").map(PathBuf::from);
    let mut failures = Vec::new();
    let mut drawn = 0;
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
            for (width, height) in PREVIEW_SIZES {
                let instance = definition.create().unwrap();
                instance
                    .window()
                    .set_size(slint::PhysicalSize::new(width, height));
                // The logic's plan for this size, as main.slint gets it.
                let _ = instance.set_property("screen", screen_value(&planned[&(width, height)]));
                instance.show().unwrap();
                render(width, height);
                settle();
                let pixels = render(width, height);
                let first = pixels[0];
                if pixels.iter().all(|p| *p == first) {
                    failures.push(format!("{stem} {name} at {width}x{height} drew nothing"));
                }
                if let Some(out) = &out {
                    save(
                        &pixels,
                        width,
                        height,
                        &out.join(format!("{width}x{height}")),
                        &format!("{stem}.{name}"),
                    );
                }
                instance.hide().unwrap();
                drawn += 1;
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(drawn >= 5 * 150, "only {drawn} frames");
}

/// The most one backdrop pixel may differ from its neighbour, a channel, in
/// 8-bit levels. The stage's gradients run over hundreds of pixels: a level
/// or two a pixel at their steepest. A layer that stops short of the window
/// steps by ten or more.
const SEAM_LEVELS: u8 = 4;

/// The largest step between neighbours along the rows `rows` (across the
/// window) and, with `down`, along every column too.
fn worst_step(
    pixels: &[Rgb8Pixel],
    width: u32,
    height: u32,
    rows: &[u32],
    down: bool,
) -> (u8, u32, u32) {
    let at = |x: u32, y: u32| pixels[(y * width + x) as usize];
    let delta = |a: Rgb8Pixel, b: Rgb8Pixel| {
        a.r.abs_diff(b.r)
            .max(a.g.abs_diff(b.g))
            .max(a.b.abs_diff(b.b))
    };
    let mut worst = (0, 0, 0);
    for &y in rows {
        for x in 1..width {
            let step = delta(at(x - 1, y), at(x, y));
            if step > worst.0 {
                worst = (step, x, y);
            }
        }
    }
    if down {
        for x in 0..width {
            for y in 1..height {
                let step = delta(at(x, y - 1), at(x, y));
                if step > worst.0 {
                    worst = (step, x, y);
                }
            }
        }
    }
    worst
}

/// Windows no device has, for what must hold at every size: odd numbers, so
/// that a half or a fifth of the window is a fraction of a pixel.
const ODD_SIZES: [(u32, u32); 6] = [
    (333, 251),
    (853, 481),
    (1001, 563),
    (1367, 769),
    (777, 1203),
    (2561, 1441),
];

/// The backdrop has no seam. The stage is a few static gradients (the light,
/// the vignette, the house lights) and each must reach every edge of the
/// window at every size: one that stops short shows as a line down or across
/// the stage. (The vignette's left-edge part once stood in the middle of the
/// window, a fifth of the width in: #425.)
///
/// - The stage's parts alone (the light, the vignette, the stage with no
///   picture), at the six sizes and six odd ones: no pixel differs from its neighbour, to the
///   side or below, by more than SEAM_LEVELS.
/// - The whole home with no art and the empty library, through logic.js, at
///   the same sizes: the same along the rows over and under everything the
///   page draws, to the window's last column.
#[test]
fn the_backdrop_has_no_seam() {
    install();
    let root = package();
    let home = scenario(&root, "home-no-art");
    let mut failures = Vec::new();
    let mut checked = 0;
    let parts = [
        (
            "ui/atoms/VelvetLight.atom.preview.slint",
            vec!["Ember", "Tide", "House"],
        ),
        (
            "ui/atoms/VelvetScrim.atom.preview.slint",
            vec!["Wide", "Narrow"],
        ),
        (
            "ui/organisms/VelvetStage.organism.preview.slint",
            vec!["LightOnly", "Library", "House"],
        ),
    ];
    for (file, states) in &parts {
        let result = pollster::block_on(
            slint_interpreter::Compiler::default().build_from_path(root.join(file)),
        );
        assert!(
            !result.has_errors(),
            "{file}: {:?}",
            result.diagnostics().collect::<Vec<_>>()
        );
        for state in states {
            let definition = result
                .component(state)
                .unwrap_or_else(|| panic!("{file} has no state {state}"));
            for (width, height) in SIZES.into_iter().chain(ODD_SIZES) {
                let screen = start(&root, &home["models"][0], width, height)
                    .0
                    .view()
                    .unwrap()["screen"]
                    .clone();
                let instance = definition.create().unwrap();
                instance
                    .window()
                    .set_size(slint::PhysicalSize::new(width, height));
                let _ = instance.set_property("screen", screen_value(&screen));
                instance.show().unwrap();
                // The frame hands the plan to the tokens after its first draw.
                for _ in 0..3 {
                    render(width, height);
                    settle();
                }
                let pixels = render(width, height);
                let rows: Vec<u32> = (0..height).collect();
                let (step, x, y) = worst_step(&pixels, width, height, &rows, true);
                checked += 1;
                if step > SEAM_LEVELS {
                    failures.push(format!(
                        "{file} {state} at {width}x{height}: a step of {step} levels at ({x},{y})"
                    ));
                }
                instance.hide().unwrap();
            }
        }
    }
    // The whole page: the rows over the Menu pill and under the row of games hold the backdrop alone.
    let views =
        korri_shell::package::compile(&korri_shell::package::Package::at(&root).unwrap()).unwrap();
    for (name, rows_of) in [("home-no-art", true), ("empty", false)] {
        let scenario = scenario(&root, name);
        for (width, height) in SIZES.into_iter().chain(ODD_SIZES) {
            let (mut engine, _korri) = start(&root, &scenario["models"][0], width, height);
            let instance = views.surface.create().unwrap();
            instance
                .window()
                .set_size(slint::PhysicalSize::new(width, height));
            instance.show().unwrap();
            let mut binder = view::Binder::new(&views.surface);
            let mut images = ImageCache::new(&root);
            binder
                .apply(&instance, &engine.view().unwrap(), &mut images)
                .unwrap();
            for _ in 0..3 {
                render(width, height);
                settle();
            }
            let pixels = render(width, height);
            let mut rows = vec![1, 2, 3];
            if rows_of {
                rows.extend([height - 4, height - 3, height - 2]);
            }
            let (step, x, y) = worst_step(&pixels, width, height, &rows, false);
            checked += 1;
            if step > SEAM_LEVELS {
                failures.push(format!(
                    "{name} at {width}x{height}: a step of {step} levels at ({x},{y})"
                ));
            }
            instance.hide().unwrap();
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(checked >= 12 * 10, "only {checked} backdrops");
}

/// The tour after a scenario's own steps: around, further in, and out again.
const TOUR: [&str; 12] = [
    "down", "right", "right", "down", "down", "right", "up", "left", "down", "down", "up", "back",
];

#[test]
fn focus_stays_in_sight_through_every_scenario_at_every_size() {
    install();
    let root = package();
    let views =
        korri_shell::package::compile(&korri_shell::package::Package::at(&root).unwrap()).unwrap();
    let all = scenarios(&root);
    assert!(
        std::env::var_os("VELVET_ONLY").is_some() || all.len() >= 85,
        "only {} scenarios",
        all.len()
    );
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut focused_views = 0;
    let rounds: usize = std::env::var("VELVET_ROUNDS")
        .ok()
        .and_then(|r| r.parse().ok())
        .unwrap_or(2);
    let sizes: Vec<(u32, u32)> = match std::env::var("VELVET_SIZES") {
        Ok(list) => list
            .split(',')
            .filter_map(|s| {
                s.split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
            })
            .collect(),
        Err(_) => SIZES.to_vec(),
    };
    for (name, scenario) in &all {
        for &(width, height) in &sizes {
            let (mut engine, mut korri) = start(&root, &scenario["models"][0], width, height);
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
            let mut check = |engine: &mut engine::Engine, label: &str| {
                let view = if overlay {
                    engine.overlay().unwrap().unwrap()
                } else {
                    engine.view().unwrap()
                };
                binder.apply(&instance, &view, &mut images).unwrap();
                // A changed list's rows are made when the window draws, and a motion starts
                // when its place is first read; then the clock steps past every motion.
                for _ in 0..rounds {
                    render(width, height);
                    settle();
                }
                checked += 1;
                let entries = targets::entries(&view);
                let focused = entries.iter().filter(|entry| entry.focused).count();
                let selected: Vec<targets::Element> = targets::elements(&instance)
                    .into_iter()
                    .filter(|e| e.selected && !e.hidden)
                    .collect();
                if focused > 1 {
                    failures.push(format!(
                        "{name} {width}x{height} after {label}: {focused} entries focused"
                    ));
                }
                if focused == 0 {
                    if !selected.is_empty() {
                        failures.push(format!("{name} {width}x{height} after {label}: a target is selected with no entry focused"));
                    }
                    return;
                }
                focused_views += 1;
                match selected.as_slice() {
                    [one] => {
                        // All of it, less half a pixel a side: a row that ends where its list's clip
                        // ends is in sight, whatever the last bit of a float says.
                        let inner = targets::Element { x: one.x + 0.5, y: one.y + 0.5, w: (one.w - 1.0).max(0.0), h: (one.h - 1.0).max(0.0), ..one.clone() };
                        if !one.visible || !targets::selected_rect_is_visible(&instance, &inner) {
                            failures.push(format!(
                                "{name} {width}x{height} after {label}: the focused target is not all in sight ({:.0},{:.0} {:.0}x{:.0}{})",
                                one.x,
                                one.y,
                                one.w,
                                one.h,
                                if one.visible { ", cut by a clip or the window" } else { ", outside every clip" }
                            ));
                        }
                    }
                    [] => failures.push(format!("{name} {width}x{height} after {label}: the focused entry draws no selected target")),
                    many => failures.push(format!("{name} {width}x{height} after {label}: {} targets selected", many.len())),
                }
            };
            check(&mut engine, "the start");
            let steps: Vec<String> = scenario["steps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect();
            for (n, press) in steps.iter().enumerate() {
                step(&mut engine, &mut korri, scenario, press);
                check(&mut engine, &format!("step {n} ({press})"));
            }
            for (n, press) in TOUR.iter().enumerate() {
                step(&mut engine, &mut korri, scenario, press);
                check(&mut engine, &format!("tour {n} ({press})"));
            }
            instance.hide().unwrap();
        }
    }
    eprintln!("{checked} views checked, {focused_views} with a focused target");
    // Each scenario and size once, with its first failure and how many there were.
    let mut grouped: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for failure in &failures {
        let key = failure
            .split(" after ")
            .next()
            .unwrap_or(failure)
            .to_owned();
        grouped.entry(key).or_insert((0, failure.clone())).0 += 1;
    }
    let summary: Vec<String> = grouped
        .values()
        .map(|(count, first)| format!("{count} x {first}"))
        .collect();
    assert!(
        failures.is_empty(),
        "{} failures in {} places:\n{:#?}",
        failures.len(),
        summary.len(),
        summary
    );
    assert!(
        std::env::var_os("VELVET_ONLY").is_some()
            || std::env::var_os("VELVET_SIZES").is_some()
            || focused_views >= 4_000,
        "only {focused_views} focused views"
    );
}

/// Contract 8.4 in full, once a scenario at one size: every focus entry of
/// the view selects exactly one element, none is selected with no entry
/// focused, and the focused one is in the window.
#[test]
fn every_focus_entry_selects_one_drawn_target() {
    install();
    let root = package();
    let views =
        korri_shell::package::compile(&korri_shell::package::Package::at(&root).unwrap()).unwrap();
    let mut failures = Vec::new();
    let mut entries = 0;
    for (name, scenario) in &scenarios(&root) {
        let (width, height) = (640, 480);
        let (mut engine, mut korri) = start(&root, &scenario["models"][0], width, height);
        for press in scenario["steps"].as_array().unwrap() {
            step(&mut engine, &mut korri, scenario, press.as_str().unwrap());
        }
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
        let view = if overlay {
            engine.overlay().unwrap().unwrap()
        } else {
            engine.view().unwrap()
        };
        binder.apply(&instance, &view, &mut images).unwrap();
        render(width, height);
        settle();
        render(width, height);
        match targets::drawn(&instance, &mut binder, &view, &mut images) {
            Ok(found) => entries += found.len(),
            Err(problem) => failures.push(format!("{name}: {problem}")),
        }
        instance.hide().unwrap();
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(
        std::env::var_os("VELVET_ONLY").is_some() || entries >= 800,
        "only {entries} entries"
    );
}

/// The fixture korrid filters `MoveCatalog` by `query.systemId` and
/// publishes `systems` (`korri_shell::catalog`, the code `fixture-korrid`
/// serves from). Over it, each system's tile on the By system shelf opens a
/// set that asks for that system and shows only that system's games.
#[test]
fn a_system_set_shows_only_that_systems_games() {
    let root = package();
    let home = scenario(&root, "home");
    let games = home["models"][0]["catalog"]["games"]
        .as_array()
        .unwrap()
        .clone();
    let systems = catalog::systems(&games);
    assert!(
        systems.len() >= 10,
        "the library has {} systems",
        systems.len()
    );
    for (at, system) in systems.iter().enumerate() {
        let id = system["systemId"].as_str().unwrap();
        let (mut engine, mut korri) = start(&root, &home["models"][0], 1280, 720);
        // Home, the row, the lobby's four shelves down to By system, then along to this system.
        for press in ["down", "down", "down", "down", "down"] {
            step(&mut engine, &mut korri, &home, press);
        }
        for _ in 0..at {
            step(&mut engine, &mut korri, &home, "right");
        }
        step(&mut engine, &mut korri, &home, "confirm");
        assert_eq!(
            korri.query["systemId"],
            json!(id),
            "the set asks Korri for {id}"
        );
        let view = engine.view().unwrap();
        assert_eq!(view["page"], json!("set"), "{id}");
        let mut shown: Vec<String> = view["setMarks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|mark| mark["label"].as_str().unwrap().to_owned())
            .collect();
        shown.sort();
        let mut wanted: Vec<String> = games
            .iter()
            .filter(|game| game["systemId"] == json!(id))
            .map(|game| game["title"].as_str().unwrap().to_owned())
            .collect();
        wanted.sort();
        assert!(!wanted.is_empty());
        assert_eq!(shown, wanted, "the {id} set");
        assert_eq!(
            wanted.len() as u64,
            system["total"].as_u64().unwrap(),
            "{id}: Korri's count"
        );
        // A game with no system is in none of them.
        assert!(
            !shown
                .iter()
                .any(|title| title == "Night Ferry" || title == "Glass Garden"),
            "{id}"
        );
    }
}

/// The package's logic.js parses, evaluates and runs to `globalThis.surface`
/// well inside one call's 50 ms (the best of ten on the thread's CPU clock,
/// so a busy machine's worst case does not decide it).
#[test]
fn logic_loads_well_inside_the_call_budget() {
    let source = std::fs::read_to_string(package().join("logic.js")).unwrap();
    let mut best = std::time::Duration::MAX;
    for _ in 0..10 {
        let started = korri_shell::budget_clock::BudgetClock::ThreadCpu.start();
        let _ = engine::Engine::load(&source, limits()).unwrap();
        best = best.min(started.elapsed());
    }
    eprintln!(
        "logic.js ({} bytes) loads in {best:?} at best",
        source.len()
    );
    assert!(best < std::time::Duration::from_millis(25), "{best:?}");
}

/// How much slower the RG353M at 408 MHz runs a surface's logic than this
/// workstation: Zone measured 25 to 28 times (tests/zone_package.rs).
const SLOW_FACTOR: f64 = 28.0;
/// One call's budget on the device (contract A5).
const CALL_BUDGET_MS: f64 = 50.0;

/// Fresh engines the tour runs on: a call's time is its median over them, so
/// one busy moment of this machine does not decide it.
const TIMED_ENGINES: usize = 7;

/// Every `view()` over 10,000 games stays under 50 ms on the slow device's
/// clock (contract A5: a call over the budget is a performance defect, with
/// no exception for the first sight of anything). The tour: the home, along
/// the row, the lobby, along and down its shelves, a set and moves through
/// it (which re-ask the window), the sort sheet, the detail, an idle Tick.
///
/// The tour runs twice on each of TIMED_ENGINES fresh engines. The first
/// run is the first sight of every screen and every card on the way; the
/// second is the same screens again. Each single call, in both runs, is
/// held to the one budget: its median over the engines, on the thread's CPU
/// clock, times SLOW_FACTOR.
#[test]
fn view_calls_stay_well_inside_the_budget_on_a_slow_clock() {
    let root = package();
    let home = scenario(&root, "home");
    let mut model = home["models"][0].clone();
    let examples = model["catalog"]["games"].as_array().unwrap().clone();
    model["catalog"] = json!({"_tag": "Ready", "games": catalog::games(&examples, 10_000)});
    let json_only = std::env::var_os("VELVET_VIEW_JSON").is_some();
    // One entry a call of the two runs: what it is, and its time on each engine.
    let mut calls: Vec<(&'static str, Vec<u64>)> = Vec::new();
    let mut bytes = 0;
    for engine_number in 0..TIMED_ENGINES {
        let (mut engine, mut korri) = start(&root, &model, 640, 480);
        engine.view().unwrap();
        let mut at = 0;
        let mut timed =
            |engine: &mut engine::Engine, korri: &mut Korri, what: &'static str, press: &str| {
                step(engine, korri, &home, press);
                let started = korri_shell::budget_clock::BudgetClock::ThreadCpu.start();
                let view = engine.view().unwrap();
                let took = started.elapsed().as_nanos() as u64;
                if engine_number == 0 {
                    calls.push((what, Vec::new()));
                }
                assert_eq!(calls[at].0, what, "the tour is the same on every engine");
                calls[at].1.push(took);
                at += 1;
                bytes = bytes.max(view.to_string().len());
                if json_only && engine_number == 0 {
                    // The same view again: only its JSON, both ways.
                    let again = korri_shell::budget_clock::BudgetClock::ThreadCpu.start();
                    engine.view().unwrap();
                    eprintln!(
                        "{what}: {took} ns, of which JSON {} ns, {} bytes",
                        again.elapsed().as_nanos(),
                        view.to_string().len()
                    );
                }
            };
        for _run in 0..2 {
            timed(&mut engine, &mut korri, "into the row", "down");
            for _ in 0..4 {
                timed(&mut engine, &mut korri, "along the row", "right");
            }
            for _ in 0..4 {
                timed(&mut engine, &mut korri, "along the row", "left");
            }
            timed(&mut engine, &mut korri, "into the lobby", "down");
            for _ in 0..5 {
                timed(&mut engine, &mut korri, "along a shelf", "right");
            }
            timed(&mut engine, &mut korri, "into a set", "confirm");
            for _ in 0..30 {
                timed(&mut engine, &mut korri, "down a set", "down");
            }
            for _ in 0..6 {
                timed(&mut engine, &mut korri, "along a set", "right");
            }
            timed(&mut engine, &mut korri, "the sort sheet", "options");
            timed(&mut engine, &mut korri, "the sort sheet", "down");
            timed(&mut engine, &mut korri, "the sort sheet", "back");
            timed(&mut engine, &mut korri, "the detail", "confirm");
            timed(&mut engine, &mut korri, "out of the detail", "back");
            timed(&mut engine, &mut korri, "out of the set", "back");
            for _ in 0..3 {
                timed(&mut engine, &mut korri, "down the shelves", "down");
            }
            timed(&mut engine, &mut korri, "an idle tick", "wait 3");
            for _ in 0..4 {
                timed(&mut engine, &mut korri, "back home", "back");
            }
        }
    }
    eprintln!(
        "the largest view: {bytes} bytes; {} calls a tour, {TIMED_ENGINES} engines",
        calls.len() / 2
    );
    let limit = (CALL_BUDGET_MS * 1_000_000.0 / SLOW_FACTOR) as u64;
    let median = |times: &Vec<u64>| {
        let mut sorted = times.clone();
        sorted.sort();
        sorted[sorted.len() / 2]
    };
    let device = |ns: u64| ns as f64 * SLOW_FACTOR / 1_000_000.0;
    // Every call against the budget; a line a kind of call for the record.
    let half = calls.len() / 2;
    let mut over = Vec::new();
    let mut kinds: BTreeMap<&str, [Vec<u64>; 2]> = BTreeMap::new();
    for (index, (what, times)) in calls.iter().enumerate() {
        let run = usize::from(index >= half);
        let took = median(times);
        kinds.entry(what).or_default()[run].push(took);
        if took > limit {
            over.push(format!(
                "{what} (call {} of the {} run): {took} CPU ns, about {:.1} ms on the device, over the {CALL_BUDGET_MS} ms budget",
                index % half + 1,
                if run == 0 { "first" } else { "second" },
                device(took)
            ));
        }
    }
    for (what, runs) in &kinds {
        let worst = |list: &Vec<u64>| *list.iter().max().unwrap();
        eprintln!(
            "{what}: first sight median {:.1} ms, worst call {:.1} ms; again median {:.1} ms, worst call {:.1} ms (on the device; budget {CALL_BUDGET_MS} ms)",
            device(median(&runs[0])),
            device(worst(&runs[0])),
            device(median(&runs[1])),
            device(worst(&runs[1]))
        );
    }
    assert!(over.is_empty(), "{over:#?}");
    assert!(bytes < 32_000, "a view of {bytes} bytes");
}

/// VELVET_FRAMES=DIR draws each scenario's last frame (VELVET_ONLY=a,b; all
/// without it) at VELVET_SIZES (WxH,WxH; 1280x720 without it) into
/// DIR/<WxH>/<name>.png. Without VELVET_FRAMES it does nothing.
#[test]
fn frames() {
    let Some(out) = std::env::var_os("VELVET_FRAMES").map(PathBuf::from) else {
        return;
    };
    install();
    let root = package();
    let views =
        korri_shell::package::compile(&korri_shell::package::Package::at(&root).unwrap()).unwrap();
    let sizes: Vec<(u32, u32)> = std::env::var("VELVET_SIZES")
        .unwrap_or("1280x720".into())
        .split(',')
        .filter_map(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .collect();
    for (name, scenario) in &scenarios(&root) {
        for &(width, height) in &sizes {
            let (mut engine, mut korri) = start(&root, &scenario["models"][0], width, height);
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
            let mut draw = |engine: &mut engine::Engine| {
                let view = if overlay {
                    engine.overlay().unwrap().unwrap()
                } else {
                    engine.view().unwrap()
                };
                binder.apply(&instance, &view, &mut images).unwrap();
                for _ in 0..4 {
                    render(width, height);
                    settle();
                }
            };
            draw(&mut engine);
            for press in scenario["steps"].as_array().unwrap() {
                step(&mut engine, &mut korri, scenario, press.as_str().unwrap());
                draw(&mut engine);
            }
            // The focus has rested: the facts are in.
            deliver(
                &mut engine,
                &mut korri,
                json!({"_tag": "Tick", "idleMs": 1000}),
            );
            draw(&mut engine);
            let pixels = render(width, height);
            save(
                &pixels,
                width,
                height,
                &out.join(format!("{width}x{height}")),
                name,
            );
            instance.hide().unwrap();
        }
    }
}
