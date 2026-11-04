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
    /// Rasterising threads; `None` keeps the renderer's default (online cores, at most 4).
    pub threads: Option<usize>,
    /// The property system's equality cut-off (vendor/README.md); `false` evaluates every
    /// dependent binding again, as upstream Slint.
    pub cutoff: bool,
    /// The view binder keeps the models of changed arrays (`view::Binder`); `false` sets a
    /// new model for every changed array, as before.
    pub reuse_models: bool,
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
    /// Of `logic`: the `view()` call.
    view: f64,
    convert: f64,
    set: f64,
    animate: f64,
    /// Of the render, before the dirty walk (`draw_contents`: instantiating repeaters).
    prepare: f64,
    walk: f64,
    raster: f64,
    /// Of `raster`: replaying the recorded draw calls on the band threads.
    replay: f64,
    /// Of `raster`: layer images rendered, their pixels, and the time.
    layers: f64,
    layer_px: f64,
    layer_time: f64,
    total: f64,
    dirty_px: f64,
    /// `KORRI_BENCH_PROFILE`: where the walk and the raster phase went.
    profile: Option<Profile>,
}

/// One frame's profile (`KORRI_BENCH_PROFILE=1`), times in microseconds. Walk: from the
/// end of the previous render through this frame's dirty-region walk (logic, properties,
/// animations, then the walk). Raster: the rest of the render, on the UI thread.
#[derive(Default, Clone, Copy)]
struct Profile {
    walk: TextProfile,
    raster: TextProfile,
    image_draw: f64,
    path_draw: f64,
    glyph_raster_count: f64,
    glyph_raster: f64,
    glyph_run_setup: f64,
    glyph_runs: f64,
    glyphs: f64,
    /// Cached layers: invalidated, waiting for their turn, drawn from an image.
    layers_invalidated: f64,
    layers_waiting: f64,
    layers_blitted: f64,
    walked_items: f64,
}

#[derive(Default, Clone, Copy)]
struct TextProfile {
    measure: f64,
    measures: f64,
    draw: f64,
    draws: f64,
    shape: f64,
    shapes: f64,
    line_break: f64,
    line_breaks: f64,
    /// Outermost property bindings' time, and every binding evaluated.
    binding: f64,
    bindings: f64,
}

/// A `TextProfile` from the renderer's frame profile (its type is Slint's, unnamed here).
macro_rules! text_profile {
    ($p:expr) => {{
        let p = &$p;
        TextProfile {
            measure: us(p.text_measure.time),
            measures: p.text_measure.count as f64,
            draw: us(p.text_draw.time),
            draws: p.text_draw.count as f64,
            shape: us(p.shape.time),
            shapes: p.shape.count as f64,
            line_break: us(p.line_break.time),
            line_breaks: p.line_break.count as f64,
            binding: us(p.binding.time),
            bindings: p.binding.count as f64,
        }
    }};
}

fn us(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

type Phase = (&'static str, fn(&Frame) -> f64);
const PHASES: [Phase; 11] = [
    ("logic (Tick/update/view)", |f| f.logic),
    ("view to properties: convert", |f| f.convert),
    ("view to properties: set", |f| f.set),
    ("timers and animations", |f| f.animate),
    ("before the walk (instantiate)", |f| f.prepare),
    ("dirty walk and layout", |f| f.walk),
    ("rasterisation", |f| f.raster),
    ("  of which banded replay", |f| f.replay),
    ("  of which layer images", |f| f.layer_time),
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
    run_captured(options, false).map(|(report, _)| report)
}

/// `run`, and with `capture` every reported frame's pixels (premultiplied RGBA, 4 bytes a
/// pixel). Each call needs a thread of its own: Slint's platform and clock are per thread.
pub fn run_captured(options: Options, capture: bool) -> Result<(String, Vec<Vec<u8>>)> {
    slint::platform::software_renderer::SoftwareRenderer::new().set_equality_cutoff(options.cutoff);
    let mut captured = Vec::new();
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
    binder.set_reuse_models(options.reuse_models);
    let mut images = ImageCache::new(&package.dir);
    let view = engine.view().map_err(|v| anyhow!("logic: {v}"))?;
    binder.apply(&surface, &view, &mut images)?;
    let mut last_view = view;
    surface.show()?;

    let (width, height) = options.size;
    let mut buffer = vec![PremultipliedRgbaColor::default(); (width * height) as usize];
    let mut frames = Vec::with_capacity(options.frames);
    let mut last_press = 0usize;
    // KORRI_BENCH_FRAMES=1: one line per frame on stderr.
    let log_frames = std::env::var_os("KORRI_BENCH_FRAMES").is_some();
    // KORRI_BENCH_DUMP=DIR: every drawn frame as a PNG, dirty rectangles outlined in red.
    let dump = std::env::var_os("KORRI_BENCH_DUMP").map(std::path::PathBuf::from);
    // KORRI_BENCH_PROFILE=1: where each frame's walk and raster time went (adds timing cost).
    let profile = std::env::var_os("KORRI_BENCH_PROFILE").is_some();
    let mut raster_sizes: BTreeMap<i16, u64> = BTreeMap::new();
    // Warm up: the first frames fill caches; they are not reported.
    let warm = 10;
    for n in 0..options.frames + warm {
        NOW.with(|t| t.set(t.get() + FRAME));
        let started = Instant::now();
        let mut frame = Frame::default();
        // Logic: a press every PRESS_EVERY frames from frame 30, a Tick
        // every second, and everything their requests cause.
        let mut message = None;
        // Each logic call this frame and its time (KORRI_BENCH_FRAMES).
        let mut calls: Vec<String> = Vec::new();
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
                let call = Instant::now();
                let requests = engine.update(&message).map_err(|v| anyhow!("logic: {v}"))?;
                calls.push(format!(
                    "update({})={:.0}",
                    message["_tag"].as_str().unwrap_or_default(),
                    us(call.elapsed())
                ));
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
                    let call = Instant::now();
                    let text = model(&query, &from, count, &focused);
                    let built = us(call.elapsed());
                    let call = Instant::now();
                    engine.set_model(&text).map_err(|v| anyhow!("logic: {v}"))?;
                    calls.push(format!(
                        "model(build)={built:.0} set_model={:.0}",
                        us(call.elapsed())
                    ));
                    queue.push(messages::korri_published());
                }
            }
            // As the shell: view() only when the state or model changed.
            if engine.view_stale() {
                engine.views_drawn();
                let view_started = Instant::now();
                let view = engine.view().map_err(|v| anyhow!("logic: {v}"))?;
                frame.view = view_started.elapsed().as_secs_f64() * 1e6;
                frame.logic = logic.elapsed().as_secs_f64() * 1e6;
                binder.apply(&surface, &view, &mut images)?;
                if let (Some(new), Some(old)) = (view.as_object(), last_view.as_object()) {
                    let changed: Vec<&str> = new
                        .iter()
                        .filter(|(k, v)| old.get(*k) != Some(*v))
                        .map(|(k, _)| k.as_str())
                        .collect();
                    calls.push(format!("changed=[{}]", changed.join(",")));
                }
                last_view = view;
                frame.convert = binder.timing.0.as_secs_f64() * 1e6;
                frame.set = binder.timing.1.as_secs_f64() * 1e6;
            } else {
                frame.logic = logic.elapsed().as_secs_f64() * 1e6;
            }
        }
        if !calls.is_empty() {
            calls.push(format!("view={:.0}", frame.view));
        }
        let animate = Instant::now();
        slint::platform::update_timers_and_animations();
        frame.animate = animate.elapsed().as_secs_f64() * 1e6;
        window.draw_if_needed(|renderer| {
            if let Some(threads) = options.threads {
                renderer.set_render_threads(threads);
            }
            renderer.set_frame_profile(profile);
            let render = Instant::now();
            let region = renderer.render(&mut buffer, width as usize);
            let render = us(render.elapsed());
            if profile {
                let walk = renderer.last_walk_profile();
                let raster = renderer.take_frame_profile();
                let (invalidated, waiting, blitted) = renderer.last_frame_layer_visits();
                for (size, n) in walk.raster_sizes.iter().chain(&raster.raster_sizes) {
                    *raster_sizes.entry(*size).or_default() += n;
                }
                frame.profile = Some(Profile {
                    walk: text_profile!(walk),
                    raster: text_profile!(raster),
                    image_draw: us(raster.image_draw.time),
                    path_draw: us(raster.path_draw.time),
                    glyph_raster_count: (walk.glyph_raster.count + raster.glyph_raster.count)
                        as f64,
                    glyph_raster: us(walk.glyph_raster.time + raster.glyph_raster.time),
                    glyph_run_setup: us(raster.glyph_run_setup.time),
                    glyph_runs: raster.glyph_run_setup.count as f64,
                    glyphs: raster.glyphs as f64,
                    layers_invalidated: invalidated as f64,
                    layers_waiting: waiting as f64,
                    layers_blitted: blitted as f64,
                    walked_items: renderer.dirty_region_walked_items() as f64,
                });
            }
            let (walk, raster) = renderer.last_frame_phases();
            frame.walk = walk.as_secs_f64() * 1e6;
            frame.raster = raster.as_secs_f64() * 1e6;
            // Before the walk: instantiating repeaters and conditionals, updating layouts.
            frame.prepare = (render - frame.walk - frame.raster).max(0.0);
            frame.replay = renderer.last_frame_replay().as_secs_f64() * 1e6;
            let (layers, layer_px, layer_time) = renderer.last_frame_layer_renders();
            frame.layers = layers as f64;
            frame.layer_px = layer_px as f64;
            frame.layer_time = layer_time.as_secs_f64() * 1e6;
            frame.dirty_px = region
                .iter()
                .map(|(_, size)| size.width as f64 * size.height as f64)
                .sum();
            if let Some(dir) = &dump {
                let rects: Vec<_> = region.iter().collect();
                save_frame(dir, n, &buffer, width, height, &rects);
            }
        });
        frame.total = started.elapsed().as_secs_f64() * 1e6;
        if n >= warm && capture {
            captured.push(
                buffer
                    .iter()
                    .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
                    .collect(),
            );
        }
        if n >= warm {
            if log_frames {
                eprintln!(
                    "FRAME {n} logic={:.0} view={:.0} set={:.0} prepare={:.0} walk={:.0} raster={:.0} replay={:.0} layers={:.0}/{:.0}px/{:.0}us total={:.0} px={:.0}",
                    frame.logic, frame.view, frame.set, frame.prepare, frame.walk, frame.raster, frame.replay,
                    frame.layers, frame.layer_px, frame.layer_time, frame.total, frame.dirty_px
                );
                if !calls.is_empty() {
                    eprintln!("  LOGIC {n} {}", calls.join(" "));
                }
                if let Some(p) = &frame.profile {
                    eprintln!(
                        "  PROFILE {n} walk: bindings={:.0}us/{:.0} measure={:.0}us/{:.0} shape={:.0}us/{:.0} break={:.0}us/{:.0} items={:.0} | raster: bindings={:.0}us/{:.0} text={:.0}us/{:.0} shape={:.0}us/{:.0} break={:.0}us/{:.0} glyphs={:.0} new-glyphs={:.0}/{:.0}us runs={:.0}/{:.0}us image={:.0}us path={:.0}us | layers invalidated={:.0} waiting={:.0} blitted={:.0}",
                        p.walk.binding, p.walk.bindings, p.walk.measure, p.walk.measures, p.walk.shape, p.walk.shapes,
                        p.walk.line_break, p.walk.line_breaks, p.walked_items,
                        p.raster.binding, p.raster.bindings, p.raster.draw, p.raster.draws, p.raster.shape, p.raster.shapes,
                        p.raster.line_break, p.raster.line_breaks, p.glyphs,
                        p.glyph_raster_count, p.glyph_raster, p.glyph_runs, p.glyph_run_setup,
                        p.image_draw, p.path_draw, p.layers_invalidated, p.layers_waiting, p.layers_blitted
                    );
                }
            }
            frames.push(frame);
        }
    }
    drop(surface);
    let mut out = report(&options, &frames);
    if profile {
        out += &profile_report(&frames, &raster_sizes);
    }
    Ok((out, captured))
}

type ProfilePhase = (&'static str, fn(&Profile) -> f64);
const PROFILE_PHASES: [ProfilePhase; 26] = [
    ("walk: bindings, outermost (us)", |p| p.walk.binding),
    ("walk: bindings evaluated", |p| p.walk.bindings),
    ("walk: text measuring (us)", |p| p.walk.measure),
    ("walk: texts measured", |p| p.walk.measures),
    ("walk: shaping (us)", |p| p.walk.shape),
    ("walk: texts shaped", |p| p.walk.shapes),
    ("walk: line breaking (us)", |p| p.walk.line_break),
    ("walk: texts broken", |p| p.walk.line_breaks),
    ("walk: items walked", |p| p.walked_items),
    ("raster: bindings, outermost (us)", |p| p.raster.binding),
    ("raster: bindings evaluated", |p| p.raster.bindings),
    ("raster: text drawing (us)", |p| p.raster.draw),
    ("raster: texts drawn", |p| p.raster.draws),
    ("raster: shaping (us)", |p| p.raster.shape),
    ("raster: line breaking (us)", |p| p.raster.line_break),
    ("raster: texts broken", |p| p.raster.line_breaks),
    ("raster: glyphs drawn", |p| p.glyphs),
    ("raster: glyph runs", |p| p.glyph_runs),
    ("raster: glyph run setup (us)", |p| p.glyph_run_setup),
    ("new glyphs rasterised", |p| p.glyph_raster_count),
    ("new glyph rasterising (us)", |p| p.glyph_raster),
    ("raster: image drawing (us)", |p| p.image_draw),
    ("raster: path drawing (us)", |p| p.path_draw),
    ("layers invalidated", |p| p.layers_invalidated),
    ("layers waiting for an image", |p| p.layers_waiting),
    ("layers drawn from an image", |p| p.layers_blitted),
];

fn profile_report(frames: &[Frame], raster_sizes: &BTreeMap<i16, u64>) -> String {
    let profiles: Vec<Profile> = frames.iter().filter_map(|f| f.profile).collect();
    let mut out = format!("profile ({} frames drawn)\n", profiles.len());
    for (name, get) in PROFILE_PHASES {
        let mut values: Vec<f64> = profiles.iter().map(get).collect();
        values.sort_by(f64::total_cmp);
        out += &format!(
            "{name:<30} {:>10.0} {:>10.0} {:>10.0}\n",
            percentile(&values, 0.5),
            percentile(&values, 0.95),
            values.last().copied().unwrap_or_default()
        );
    }
    let sizes: Vec<String> = raster_sizes
        .iter()
        .map(|(s, n)| format!("{s}px:{n}"))
        .collect();
    out += &format!("glyphs rasterised by pixel size: {}\n", sizes.join(" "));
    out
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
        "bench {} {}x{} frames={} games={} presses={} prefs={:?} buffer={} threads={}\n",
        options.surface,
        options.size.0,
        options.size.1,
        frames.len(),
        options.games,
        options.presses.len(),
        options.preferences,
        if options.full { "new" } else { "reused" },
        options
            .threads
            .map_or("default".to_owned(), |t| t.to_string())
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

fn save_frame(
    dir: &std::path::Path,
    n: usize,
    buffer: &[PremultipliedRgbaColor],
    width: u32,
    height: u32,
    rects: &[(slint::PhysicalPosition, slint::PhysicalSize)],
) {
    let mut img = image::RgbImage::new(width, height);
    for (i, p) in buffer.iter().enumerate() {
        img.put_pixel(
            i as u32 % width,
            i as u32 / width,
            image::Rgb([p.red, p.green, p.blue]),
        );
    }
    for (pos, size) in rects {
        let (x0, y0) = (pos.x.max(0) as u32, pos.y.max(0) as u32);
        let (x1, y1) = (
            (x0 + size.width).min(width) - 1,
            (y0 + size.height).min(height) - 1,
        );
        for x in x0..=x1 {
            img.put_pixel(x, y0, image::Rgb([255, 0, 0]));
            img.put_pixel(x, y1, image::Rgb([255, 0, 0]));
        }
        for y in y0..=y1 {
            img.put_pixel(x0, y, image::Rgb([255, 0, 0]));
            img.put_pixel(x1, y, image::Rgb([255, 0, 0]));
        }
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = img.save(dir.join(format!("frame-{n:04}.png")));
}
