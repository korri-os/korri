//! Velvet's package (packages/velvet), drawn by the shell's own interpreter
//! and software renderer, display-free. (Working harness; the gates follow.)

use korri_shell::{catalog, engine, images::ImageCache, view};
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
    image::save_buffer(dir.join(format!("{name}.png")), &rgb, width, height, image::ColorType::Rgb8).unwrap();
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
        let actions = model.pointer("/focusedGame/actions").cloned().unwrap_or(json!([]));
        model.as_object_mut().unwrap().remove("focusedGame");
        Self { model, games, query: catalog::default_query(), from: json!({"_tag": "Start"}), count: catalog::DEFAULT_COUNT, focused: None, actions }
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
    engine::Limits { stuck: std::time::Duration::from_secs(5), clock: korri_shell::budget_clock::BudgetClock::ThreadCpu, ..engine::Limits::default() }
}

/// VELVET_FRAMES=DIR draws scenarios (VELVET_ONLY=a,b; all without it) at
/// VELVET_SIZES (WxH,WxH; 1280x720 without it) into DIR/<WxH>/<name>.png.
#[test]
fn frames() {
    let Some(out) = std::env::var_os("VELVET_FRAMES").map(PathBuf::from) else { return };
    install();
    let root = package();
    let views = korri_shell::package::compile(&korri_shell::package::Package::at(&root).unwrap()).unwrap();
    let source = std::fs::read_to_string(root.join("logic.js")).unwrap();
    let sizes: Vec<(u32, u32)> = std::env::var("VELVET_SIZES").unwrap_or("1280x720".into()).split(',').filter_map(|s| { let (w, h) = s.split_once('x')?; Some((w.parse().ok()?, h.parse().ok()?)) }).collect();
    let only: Option<Vec<String>> = std::env::var("VELVET_ONLY").ok().map(|o| o.split(',').map(str::to_owned).collect());
    let mut scenarios: Vec<PathBuf> = std::fs::read_dir(root.join("preview/scenarios")).unwrap().map(|e| e.unwrap().path()).collect();
    scenarios.sort();
    for path in &scenarios {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if only.as_ref().is_some_and(|only| !only.contains(&name)) { continue; }
        let scenario: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for &(width, height) in &sizes {
            let mut korri = Korri::new(scenario["models"][0].clone());
            let mut engine = engine::Engine::load(&source, limits()).unwrap();
            engine.set_model(&korri.published()).unwrap();
            engine.init(&engine::effective_preferences(engine.declaration(), &BTreeMap::new())).unwrap();
            let overlay = korri.model["presentation"]["kind"] == "gameplay-overlay";
            let definition = if overlay { views.overlay.clone().unwrap() } else { views.surface.clone() };
            let instance = definition.create().unwrap();
            instance.window().set_size(slint::PhysicalSize::new(width, height));
            instance.show().unwrap();
            let mut binder = view::Binder::new(&definition);
            let mut images = ImageCache::new(&root);
            deliver(&mut engine, &mut korri, json!({"_tag": "Resized", "width": width, "height": height}));
            deliver(&mut engine, &mut korri, json!({"_tag": "KorriPublished"}));
            let mut draw = |engine: &mut engine::Engine, binder: &mut view::Binder, images: &mut ImageCache| {
                let view = if overlay { engine.overlay().unwrap().unwrap() } else { engine.view().unwrap() };
                binder.apply(&instance, &view, images).unwrap();
                for _ in 0..4 { render(width, height); settle(); }
            };
            draw(&mut engine, &mut binder, &mut images);
            for step in scenario["steps"].as_array().unwrap() {
                let step = step.as_str().unwrap();
                if let Some(index) = step.strip_prefix("model ") {
                    korri = Korri { model: Korri::new(scenario["models"][index.parse::<usize>().unwrap()].clone()).model, ..korri };
                    engine.set_model(&korri.published()).unwrap();
                    deliver(&mut engine, &mut korri, json!({"_tag": "KorriPublished"}));
                } else if !step.starts_with("wait") {
                    deliver(&mut engine, &mut korri, json!({"_tag": "Pressed", "button": step}));
                }
                draw(&mut engine, &mut binder, &mut images);
            }
            deliver(&mut engine, &mut korri, json!({"_tag": "Tick", "idleMs": 1000}));
            draw(&mut engine, &mut binder, &mut images);
            let pixels = render(width, height);
            save(&pixels, width, height, &out.join(format!("{width}x{height}")), &name);
            instance.hide().unwrap();
        }
    }
}
