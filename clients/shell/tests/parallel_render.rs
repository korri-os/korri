//! The vendored software renderer's parallel rasterisation (vendor/README.md,
//! `parallel.rs`): a frame rasterised in bands on several threads has the
//! same pixels as the same frame drawn on one thread, through full and
//! partial frames, every primitive kind, cached layers, moves and resizes,
//! with the draw calls streamed to the band threads in several chunks; with
//! threads moving between bands as chunks are published (the default) and with
//! each thread replaying a whole band (the former ordering).

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Rgb8Pixel};
use slint_interpreter::{ComponentInstance, Value};
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
}

struct Reused(std::time::Instant);

impl Platform for Reused {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        WINDOWS.with(|w| w.borrow_mut().push(window.clone()));
        Ok(window)
    }
    fn duration_since_start(&self) -> std::time::Duration {
        self.0.elapsed()
    }
}

/// Every primitive the renderer records: solid, rounded and bordered
/// rectangles, linear, radial and conic gradients, a filled and a stroked
/// path, text (glyph textures), an opacity group and a cached layer.
const SOURCE: &str = r#"
import "fonts/tiny5.ttf";
export component Surface inherits Window {
    in property <string> word: "BANDS";
    in property <length> mover-x: 10px;
    in property <length> layer-y: 200px;
    in property <float> fade: 0.6;
    background: #102030;
    default-font-family: "Pico Tiny5";
    Rectangle { x: 0; y: 0; width: 640px; height: 120px; background: @linear-gradient(30deg, #ff0055 0%, #00ffaa 100%); }
    Rectangle { x: 40px; y: 140px; width: 200px; height: 200px; background: @radial-gradient(circle, #ffffff 0%, #0033ff 60%, #00000000 100%); }
    Rectangle { x: 300px; y: 140px; width: 160px; height: 160px; background: @conic-gradient(#ff0000 0deg, #00ff00 120deg, #0000ff 240deg, #ff0000 360deg); }
    Rectangle { x: root.mover-x; y: 60px; width: 120px; height: 300px; border-radius: 18px; border-width: 5px; border-color: #ffcc00; background: #33669980; }
    Path { x: 470px; y: 130px; width: 150px; height: 150px; commands: "M 0 0 L 100 20 L 60 100 Z"; fill: #ff8800; stroke: #ffffff; stroke-width: 4px; }
    Rectangle {
        y: root.layer-y;
        x: 200px;
        width: 380px;
        height: 200px;
        cache-rendering-hint: true;
        opacity: root.fade;
        for i in 6: Text {
            x: 10px + i * 2px;
            y: 20px + i * 26px;
            text: root.word + " " + i;
            color: i == 5 ? #ffff00 : #000000;
            font-size: 24px;
        }
    }
    for i in 20: Rectangle { x: Math.mod(i, 10) * 64px; y: 420px + Math.floor(i / 10) * 30px; width: 60px; height: 26px; background: i == 7 ? #ffffff : #406080; }
    // More draw calls than one chunk of the stream (`parallel::CHUNK`), so that
    // band threads replay chunks while the walk records the next.
    for i in 200: Rectangle { x: Math.mod(i, 40) * 16px; y: 360px + Math.floor(i / 40) * 11px; width: 12px; height: 8px; background: Math.mod(i, 3) == 0 ? #ff00ff80 : #00ffff; }
}
"#;

struct Pair {
    instances: [ComponentInstance; 2],
    windows: [Rc<MinimalSoftwareWindow>; 2],
    threads: [usize; 2],
    buffers: [Vec<Rgb8Pixel>; 2],
    size: (u32, u32),
}

impl Pair {
    fn new(threads: [usize; 2]) -> Self {
        let _ = slint::platform::set_platform(Box::new(Reused(std::time::Instant::now())));
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/packages/probe/parallel-test.slint");
        let result = pollster::block_on(
            slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
        );
        assert!(
            !result.has_errors(),
            "{:?}",
            result.diagnostics().collect::<Vec<_>>()
        );
        let definition = result.component("Surface").expect("Surface");
        let make = || {
            let instance = definition.create().unwrap();
            instance.show().unwrap();
            instance
        };
        let instances = [make(), make()];
        let windows = WINDOWS.with(|w| {
            let w = w.borrow();
            [w[w.len() - 2].clone(), w[w.len() - 1].clone()]
        });
        let mut pair = Self {
            instances,
            windows,
            threads,
            buffers: [Vec::new(), Vec::new()],
            size: (0, 0),
        };
        pair.resize(640, 480);
        pair
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.size = (width, height);
        for (window, buffer) in self.windows.iter().zip(&mut self.buffers) {
            window.set_size(slint::PhysicalSize::new(width, height));
            *buffer = vec![Rgb8Pixel::default(); (width * height) as usize];
        }
    }

    fn set(&self, name: &str, value: Value) {
        for instance in &self.instances {
            instance.set_property(name, value.clone()).unwrap();
        }
    }

    fn render_and_compare(&mut self, step: &str) {
        slint::platform::update_timers_and_animations();
        for ((window, buffer), threads) in
            self.windows.iter().zip(&mut self.buffers).zip(self.threads)
        {
            window.draw_if_needed(|renderer| {
                renderer.set_render_threads(threads);
                renderer.render(buffer, self.size.0 as usize);
            });
        }
        let differing = self.buffers[0]
            .iter()
            .zip(&self.buffers[1])
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differing, 0,
            "{step}: {differing} pixels differ, threads {:?}",
            self.threads
        );
    }
}

fn scenario(threads: [usize; 2]) {
    let mut pair = Pair::new(threads);
    pair.render_and_compare("first frame (whole window)");
    for x in [40.0, 333.0, 500.5] {
        pair.set("mover-x", Value::Number(x));
        pair.render_and_compare(&format!("rounded rectangle at {x}"));
    }
    pair.set("word", Value::String("CHANGED".into()));
    pair.render_and_compare("text inside the layer");
    for y in [180.0, 151.5, 260.0] {
        pair.set("layer-y", Value::Number(y));
        pair.render_and_compare(&format!("layer at {y}"));
        pair.render_and_compare(&format!("layer at {y}, settled"));
    }
    pair.set("fade", Value::Number(0.3));
    pair.render_and_compare("layer opacity");
    pair.resize(500, 333);
    pair.render_and_compare("resized to odd lines");
}

#[test]
fn bands_on_four_threads_match_one_thread() {
    scenario([1, 4]);
}

#[test]
fn bands_on_three_threads_match_one_thread() {
    scenario([1, 3]);
}

#[test]
fn one_band_thread_beside_the_walk_matches_one_thread() {
    scenario([1, 2]);
}

/// The former ordering: each band thread replays one whole band at a time. Both orderings
/// replay every op into every band in order, so both match one thread; the switch is
/// global, and a test running beside this one draws the same pixels either way.
#[test]
fn whole_bands_on_three_threads_match_one_thread() {
    slint::platform::software_renderer::SoftwareRenderer::set_interleaved_bands(false);
    scenario([1, 3]);
    slint::platform::software_renderer::SoftwareRenderer::set_interleaved_bands(true);
}
