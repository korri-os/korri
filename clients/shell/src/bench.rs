//! `korri-shell bench`: one package's frame cost, headless.
//!
//! The real package runs as the shell runs it (QuickJS logic, the view
//! conversion, the patched Slint software renderer with partial rendering
//! into a reused buffer), but into an offscreen buffer, with no window,
//! compositor or korrid. Time is simulated at 60 Hz, so animations advance
//! 16.67 ms per frame however long a frame takes. Each frame is split into
//! phases; the report gives p50, p95 and max per phase over all frames.

use crate::{
    catalog,
    engine::{self, Engine, Limits},
    images::ImageCache,
    messages::{self, Button},
    package::{self, Package},
    verify,
    view::Binder,
};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::ComponentHandle;
use std::{
    cell::Cell,
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant},
};

pub struct Options {
    pub packages: std::path::PathBuf,
    pub surface: String,
    pub size: (u32, u32),
    pub frames: usize,
    pub games: usize,
    /// Presses, one every `PRESS_EVERY` frames from frame 30.
    pub presses: Vec<Button>,
    /// Preferences as `key=value`.
    pub preferences: BTreeMap<String, String>,
    /// Redraw the whole window every frame (a compositor that gives back a
    /// fresh buffer: softbuffer's buffer age 0).
    pub full: bool,
}

const FRAME: Duration = Duration::from_nanos(16_666_667);
const PRESS_EVERY: usize = 20;
const TICK_EVERY: usize = 60;

thread_local! {
    static NOW: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    static WINDOW: std::cell::RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { std::cell::RefCell::new(None) };
}

struct BenchPlatform {
    size: (u32, u32),
    kind: RepaintBufferType,
}

impl Platform for BenchPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(self.kind);
        window.set_size(slint::PhysicalSize::new(self.size.0, self.size.1));
        WINDOW.with(|w| *w.borrow_mut() = Some(window.clone()));
        Ok(window)
    }
    fn duration_since_start(&self) -> Duration {
        NOW.with(Cell::get)
    }
}

/// The phases of one frame, in microseconds, and its dirty area.
#[derive(Default, Clone, Copy)]
struct Frame {
    logic: f64,
    convert: f64,
    set: f64,
    animate: f64,
    walk: f64,
    raster: f64,
    total: f64,
    dirty_px: f64,
}

type Phase = (&'static str, fn(&Frame) -> f64);
const PHASES: [Phase; 8] = [
    ("logic (Tick/update/view)", |f| f.logic),
    ("view to properties: convert", |f| f.convert),
    ("view to properties: set", |f| f.set),
    ("timers and animations", |f| f.animate),
    ("dirty walk and layout", |f| f.walk),
    ("rasterisation", |f| f.raster),
    ("frame total", |f| f.total),
    ("dirty region (px)", |f| f.dirty_px),
];

pub fn parse_presses(text: &str) -> Result<Vec<Button>> {
    text.split(',')
        .filter(|s| !s.is_empty())
        .map(|name| {
            serde_json::from_value(json!(name)).map_err(|_| anyhow!("unknown button {name:?}"))
        })
        .collect()
}

pub fn run(options: Options) -> Result<String> {
    let kind = if options.full {
        RepaintBufferType::NewBuffer
    } else {
        RepaintBufferType::ReusedBuffer
    };
    slint::platform::set_platform(Box::new(BenchPlatform {
        size: options.size,
        kind,
    }))
    .map_err(|e| anyhow!("platform: {e}"))?;
    let package = Package::at(&options.packages.join(&options.surface))
        .with_context(|| format!("package {}", options.surface))?;
    let views = package::compile(&package)?;
    let mut engine = Engine::load(&package.logic_source()?, Limits::default())
        .map_err(|v| anyhow!("logic: {v}"))?;
    let (base, examples) = verify::fixture();
    let games = catalog::games(&examples, options.games);
    let mut query = catalog::default_query();
    let mut from = json!({"_tag": "Start"});
    let mut count = catalog::DEFAULT_COUNT;
    let mut focused: Option<String> = None;
    let model = |query: &Value, from: &Value, count: usize, focused: &Option<String>| {
        let mut model = base.clone();
        model["catalog"] = catalog::window(&games, query, from, count);
        if let Some(id) = focused {
            model["focusedGame"] = json!({"gameId": id, "actions": []});
        }
        model.to_string()
    };
    let stored = options.preferences.clone();
    engine
        .set_model(&model(&query, &from, count, &focused))
        .map_err(|v| anyhow!("logic: {v}"))?;
    engine
        .init(&engine::effective_preferences(
            engine.declaration(),
            &stored,
        ))
        .map_err(|v| anyhow!("logic: {v}"))?;
    let surface = views.surface.create()?;
    let window = WINDOW.with(|w| w.borrow().clone()).context("no window")?;
    let scale = surface.window().scale_factor();
    let logical = surface.window().size().to_logical(scale);
    engine
        .update(&messages::resized(logical.width, logical.height))
        .map_err(|v| anyhow!("logic: {v}"))?;
    let mut binder = Binder::new(&views.surface);
    let mut images = ImageCache::new(&package.dir);
    let view = engine.view().map_err(|v| anyhow!("logic: {v}"))?;
    binder.apply(&surface, &view, &mut images)?;
    surface.show()?;

    let (width, height) = options.size;
    let mut buffer = vec![PremultipliedRgbaColor::default(); (width * height) as usize];
    let mut frames = Vec::with_capacity(options.frames);
    let mut last_press = 0usize;
    // Warm up: the first frames fill caches; they are not reported.
    let warm = 10;
    for n in 0..options.frames + warm {
        NOW.with(|t| t.set(t.get() + FRAME));
        let started = Instant::now();
        let mut frame = Frame::default();
        // Logic: a press every PRESS_EVERY frames from frame 30, a Tick
        // every second, and everything their requests cause.
        let mut message = None;
        let press_index = n
            .checked_sub(30)
            .filter(|k| k % PRESS_EVERY == 0)
            .map(|k| k / PRESS_EVERY);
        if let Some(button) = press_index.and_then(|i| options.presses.get(i)) {
            message = Some(messages::pressed(*button));
            last_press = n;
        } else if n % TICK_EVERY == 0 && n > 0 {
            let idle = ((n - last_press) as f64 * FRAME.as_secs_f64() * 1000.0) as u32;
            message = Some(messages::tick(idle));
        }
        if let Some(message) = message {
            let logic = Instant::now();
            let mut queue = vec![message];
            let mut rounds = 0;
            while let Some(message) = queue.pop() {
                rounds += 1;
                if rounds > 16 {
                    bail!("requests keep causing requests");
                }
                let requests = engine.update(&message).map_err(|v| anyhow!("logic: {v}"))?;
                let mut changed = false;
                for request in &requests {
                    match messages::request_tag(request) {
                        Some("MoveCatalog") => {
                            query = request["query"].clone();
                            from = request["from"].clone();
                            count = request["count"].as_u64().unwrap_or(50) as usize;
                            changed = true;
                        }
                        Some("FocusGame") => {
                            focused = request["gameId"].as_str().map(str::to_owned);
                            changed = true;
                        }
                        _ => {}
                    }
                }
                if changed {
                    engine
                        .set_model(&model(&query, &from, count, &focused))
                        .map_err(|v| anyhow!("logic: {v}"))?;
                    queue.push(messages::korri_published());
                }
            }
            let view = engine.view().map_err(|v| anyhow!("logic: {v}"))?;
            frame.logic = logic.elapsed().as_secs_f64() * 1e6;
            binder.apply(&surface, &view, &mut images)?;
            frame.convert = binder.timing.0.as_secs_f64() * 1e6;
            frame.set = binder.timing.1.as_secs_f64() * 1e6;
        }
        let animate = Instant::now();
        slint::platform::update_timers_and_animations();
        frame.animate = animate.elapsed().as_secs_f64() * 1e6;
        window.draw_if_needed(|renderer| {
            let region = renderer.render(&mut buffer, width as usize);
            let (walk, raster) = renderer.last_frame_phases();
            frame.walk = walk.as_secs_f64() * 1e6;
            frame.raster = raster.as_secs_f64() * 1e6;
            frame.dirty_px = region
                .iter()
                .map(|(_, size)| size.width as f64 * size.height as f64)
                .sum();
        });
        frame.total = started.elapsed().as_secs_f64() * 1e6;
        if n >= warm {
            frames.push(frame);
        }
    }
    drop(surface);
    Ok(report(&options, &frames))
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i]
}

fn report(options: &Options, frames: &[Frame]) -> String {
    let mut out = format!(
        "bench {} {}x{} frames={} games={} presses={} prefs={:?} buffer={}\n",
        options.surface,
        options.size.0,
        options.size.1,
        frames.len(),
        options.games,
        options.presses.len(),
        options.preferences,
        if options.full { "new" } else { "reused" }
    );
    out += &format!(
        "{:<30} {:>10} {:>10} {:>10}\n",
        "phase (us; px)", "p50", "p95", "max"
    );
    for (name, get) in PHASES {
        let mut values: Vec<f64> = frames.iter().map(get).collect();
        values.sort_by(f64::total_cmp);
        out += &format!(
            "{name:<30} {:>10.0} {:>10.0} {:>10.0}\n",
            percentile(&values, 0.5),
            percentile(&values, 0.95),
            values.last().copied().unwrap_or_default()
        );
    }
    let over = frames.iter().filter(|f| f.total > 16_667.0).count();
    out += &format!("frames over 16.7 ms: {over} of {}\n", frames.len());
    out
}
