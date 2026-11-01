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
    }
}

fn install() {
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

/// RGB565 of Zone's pink (#ce6584): the focus drop and edge.
const PINK: u16 = (0xce >> 3) << 11 | (0x65 >> 2) << 5 | (0x84 >> 3);

fn focus_marks(pixels: &[u16]) -> usize {
    pixels.iter().filter(|&&p| p == PINK).count()
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
    for path in &scenarios {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if NO_FOCUS.contains(&name.as_str()) {
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
            let mut engine = engine::Engine::load(&source, engine::Limits::default()).unwrap();
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
            let mut check = |engine: &mut engine::Engine, label: &str| {
                let view = if overlay {
                    engine.overlay().unwrap().unwrap()
                } else {
                    engine.view().unwrap()
                };
                binder.apply(&instance, &view, &mut images).unwrap();
                render(width, height);
                let pixels = render(width, height);
                checked += 1;
                if focus_marks(&pixels) < 12 {
                    save(&pixels, width, height, &format!("unfocused.{name}.{label}"));
                    failures.push(format!(
                        "{name} {width}x{height} after {label}: no focus mark in the window"
                    ));
                }
            };
            check(&mut engine, "steps");
            for (direction, times) in [("down", 8), ("right", 6), ("up", 8), ("left", 6)] {
                for n in 0..times {
                    deliver(&mut engine, &mut korri, press(direction));
                    check(&mut engine, &format!("{direction}{n}"));
                }
            }
            instance.hide().unwrap();
        }
    }
    assert!(checked > 1000, "only {checked} frames checked");
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
