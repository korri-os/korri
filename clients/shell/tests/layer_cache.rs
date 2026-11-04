//! The vendored software renderer's layer cache (`cache-rendering-hint: true`,
//! vendor/README.md): a cached layer draws the same pixels as the same tree
//! drawn directly, through partial rendering, property changes, movement
//! around it, sub-pixel placement, resizes and scale changes; and a clean
//! cached layer is not walked.

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel, TargetPixel},
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Rgb8Pixel};
use slint_interpreter::{ComponentDefinition, ComponentInstance, Value};
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
    /// The buffer windows are made with: reused (partial redraws) unless a test asks for new.
    static KIND: std::cell::Cell<RepaintBufferType> = const { std::cell::Cell::new(RepaintBufferType::ReusedBuffer) };
}

struct Reused(std::time::Instant);

impl Platform for Reused {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(KIND.with(|k| k.get()));
        WINDOWS.with(|w| w.borrow_mut().push(window.clone()));
        Ok(window)
    }
    fn duration_since_start(&self) -> std::time::Duration {
        self.0.elapsed()
    }
}

const SOURCE: &str = r#"
import "fonts/tiny5.ttf";
export component Surface inherits Window {
    in property <bool> cached;
    in property <string> word: "STILL";
    in property <color> ink: #ffcc00;
    in property <length> mover-x: 0px;
    in property <length> layer-x: 20px;
    in property <int> count: 3;
    background: #204060;
    default-font-family: "Pico Tiny5";
    // Below the layer: it moves under the cached image.
    Rectangle { x: root.mover-x; y: 30px; width: 24px; height: 24px; background: #ff0055; }
    Rectangle {
        x: root.layer-x;
        y: 10px;
        width: 150px;
        height: 90px;
        cache-rendering-hint: root.cached;
        Rectangle { x: 4px; y: 4px; width: 60px; height: 20px; background: #00ff8880; border-radius: 6px; }
        for i in root.count: Text {
            x: 10px + i * 1px;
            y: 30px + i * 1px;
            text: root.word;
            color: i == root.count - 1 ? root.ink : #000000;
            font-size: 21px;
        }
        // A child reaching outside the layer's own geometry.
        Rectangle { x: 140px; y: 80px; width: 24px; height: 24px; background: #ffffffc0; }
    }
    // Above the layer.
    Rectangle { x: root.mover-x; y: 70px; width: 16px; height: 16px; background: #00aaff; }
}
"#;

fn compile(source: &str) -> ComponentDefinition {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/packages/probe/layer-test.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(source.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    result.component("Surface").expect("Surface")
}

struct Pair<P = Rgb8Pixel> {
    instances: [ComponentInstance; 2],
    windows: [Rc<MinimalSoftwareWindow>; 2],
    buffers: [Vec<P>; 2],
    size: (u32, u32),
}

impl<P: TargetPixel + Default + Clone + PartialEq> Pair<P> {
    fn new(source: &str, width: u32, height: u32) -> Self {
        let _ = slint::platform::set_platform(Box::new(Reused(std::time::Instant::now())));
        let definition = compile(source);
        let make = |cached: bool| {
            let instance = definition.create().unwrap();
            instance
                .set_property("cached", Value::Bool(cached))
                .unwrap();
            instance.show().unwrap();
            instance
        };
        let instances = [make(true), make(false)];
        let windows = WINDOWS.with(|w| {
            let w = w.borrow();
            [w[w.len() - 2].clone(), w[w.len() - 1].clone()]
        });
        let mut pair = Self {
            instances,
            windows,
            buffers: [Vec::new(), Vec::new()],
            size: (0, 0),
        };
        pair.resize(width, height);
        pair
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.size = (width, height);
        for (window, buffer) in self.windows.iter().zip(&mut self.buffers) {
            window.set_size(slint::PhysicalSize::new(width, height));
            *buffer = vec![P::default(); (width * height) as usize];
        }
    }

    fn set(&self, name: &str, value: Value) {
        for instance in &self.instances {
            instance.set_property(name, value.clone()).unwrap();
        }
    }

    /// Render both; return how many items the cached window walked.
    fn render(&mut self) -> [usize; 2] {
        slint::platform::update_timers_and_animations();
        let mut walked = [0; 2];
        for (i, (window, buffer)) in self.windows.iter().zip(&mut self.buffers).enumerate() {
            window.draw_if_needed(|renderer| {
                renderer.render(buffer, self.size.0 as usize);
                walked[i] = renderer.dirty_region_walked_items();
            });
        }
        walked
    }

    fn assert_same(&self, step: &str) {
        let differing = self.buffers[0]
            .iter()
            .zip(&self.buffers[1])
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differing, 0,
            "{step}: {differing} pixels differ between the cached and the direct layer"
        );
    }
}

#[test]
fn a_cached_layer_matches_direct_rendering_through_every_change() {
    every_change::<Rgb8Pixel>();
}

/// RGB565, as the handhelds draw: text blended over a translucent child inside
/// the layer gives the same 16-bit pixels cached and direct.
#[test]
fn a_cached_layer_matches_direct_rendering_in_rgb565() {
    every_change::<Rgb565Pixel>();
}

/// Every frame drawn whole (a fresh buffer, as a compositor with buffer age 0 gives): a
/// still cached layer copies the pixels its calls drew last time over the same pixels, and
/// draws again where what lies under it changed. Same pixels as drawing directly.
#[test]
fn a_cached_layer_matches_direct_rendering_in_whole_frames() {
    KIND.with(|k| k.set(RepaintBufferType::NewBuffer));
    let mut pair = Pair::<Rgb565Pixel>::new(SOURCE, 200, 120);
    for step in 0..6 {
        for window in &pair.windows {
            window.request_redraw();
        }
        pair.render();
        pair.assert_same(&format!("still, frame {step}"));
    }
    for x in [5, 30, 60, 90, 130, 170] {
        pair.set("mover-x", Value::Number(x as f64));
        for window in &pair.windows {
            window.request_redraw();
        }
        pair.render();
        pair.assert_same(&format!("whole frame, mover at {x}"));
    }
    pair.set("word", Value::String("MOVED".into()));
    for step in 0..3 {
        for window in &pair.windows {
            window.request_redraw();
        }
        pair.render();
        pair.assert_same(&format!("whole frame after a change, {step}"));
    }
}

/// Opacity on the layer: drawn as its children are, each with the opacity.
#[test]
fn a_cached_layer_under_opacity_matches_direct_rendering() {
    let source = SOURCE.replace(
        "        cache-rendering-hint: root.cached;",
        "        cache-rendering-hint: root.cached;\n        opacity: 0.6;",
    );
    let mut pair = Pair::<Rgb565Pixel>::new(&source, 200, 120);
    pair.render();
    pair.assert_same("first frame");
    pair.set("mover-x", Value::Number(40.0));
    pair.render();
    pair.render();
    pair.assert_same("mover under the translucent layer");
}

fn every_change<P: TargetPixel + Default + Clone + PartialEq>() {
    let mut pair = Pair::<P>::new(SOURCE, 200, 120);
    pair.render();
    pair.assert_same("first frame");
    // A still layer, with something moving under and over it.
    for x in [5, 30, 60, 90, 130, 170] {
        pair.set("mover-x", Value::Number(x as f64));
        pair.render();
        pair.assert_same(&format!("mover at {x}"));
    }
    // A property inside the layer changes.
    pair.set("word", Value::String("MOVED".into()));
    pair.render();
    pair.assert_same("word changed");
    pair.set(
        "ink",
        Value::Brush(slint::Brush::SolidColor(slint::Color::from_rgb_u8(
            10, 200, 90,
        ))),
    );
    pair.render();
    pair.assert_same("colour changed");
    pair.set("count", Value::Number(5.0));
    pair.render();
    pair.assert_same("repeater grew");
    // The layer moves, by whole and by fractional pixels. A layer at a new
    // sub-pixel phase is first shifted by whole pixels (it may be moving) and
    // asks for one more frame, which renders it at its exact phase.
    for x in [40.0, 40.5, 41.25] {
        pair.set("layer-x", Value::Number(x));
        pair.render();
        pair.render();
        pair.assert_same(&format!("layer at {x}, settled"));
    }
    // The window is resized, and the scale factor changes.
    pair.resize(260, 150);
    pair.render();
    pair.assert_same("resized");
    for window in &pair.windows {
        window
            .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: 2.0 });
    }
    pair.resize(400, 240);
    pair.render();
    pair.assert_same("scale factor 2");
    pair.set("mover-x", Value::Number(12.0));
    pair.render();
    pair.assert_same("mover after scaling");
}

#[test]
fn a_clean_cached_layer_is_not_walked() {
    let source = SOURCE.replace(
        "        // A child reaching outside the layer's own geometry.",
        "        for i in 300: Rectangle { x: Math.mod(i, 30) * 5px; y: 50px + Math.floor(i / 30) * 3px; width: 4px; height: 2px; background: #ffffff; }\n        // A child reaching outside the layer's own geometry.",
    );
    let mut pair = Pair::<Rgb8Pixel>::new(&source, 200, 120);
    pair.render();
    pair.set("mover-x", Value::Number(10.0));
    pair.render();
    pair.set("mover-x", Value::Number(20.0));
    let [cached, direct] = pair.render();
    pair.assert_same("many children");
    assert!(direct > 300, "the direct layer walks every child: {direct}");
    assert!(
        cached < 20,
        "the clean cached layer is skipped: walked {cached}"
    );
    // A change inside draws the layer directly, walking it. The next frame finds
    // the content unchanged and draws its area again; that frame renders the
    // image. Then it is clean again.
    pair.set("word", Value::String("AGAIN".into()));
    let [cached, _] = pair.render();
    assert!(cached > 300, "a dirty layer is walked: {cached}");
    pair.assert_same("after the change");
    pair.render();
    pair.render();
    pair.assert_same("image rendered after the change");
    pair.set("mover-x", Value::Number(30.0));
    let [cached, _] = pair.render();
    assert!(cached < 20, "clean again: walked {cached}");
    pair.assert_same("clean again");
}

/// Captions as Zone draws them: a cached layer at fractional positions holding elided
/// text whose glyphs and ellipsis reach past their boxes and past the layer, text raised
/// above its box, and offset copies of each caption (an edge and a drop). Cached and
/// direct give the same RGB565 pixels.
const CAPTIONS: &str = r#"
import "fonts/tiny5.ttf";
export component Surface inherits Window {
    in property <bool> cached;
    in property <length> layer-x: 20.3px;
    in property <length> layer-y: 10.6px;
    in property <string> word: "A caption much too long";
    background: #204060;
    default-font-family: "Pico Tiny5";
    // As ZoneCaps: the layer is the caption's box; its texts are raised above it and elided
    // in it, so their glyphs and ellipsis reach past the layer and past the texts' boxes.
    Rectangle {
        x: root.layer-x;
        y: root.layer-y;
        width: 70.5px;
        height: 9.2px;
        cache-rendering-hint: root.cached;
        for d in [{x: -1px, y: -1px}, {x: 1px, y: -1px}, {x: -1px, y: 1px}, {x: 1px, y: 1px}, {x: 1.5px, y: 2.5px}]: Text {
            x: d.x; y: -4.5px + d.y; width: 70.5px; height: 7px;
            text: root.word; font-size: 19px; color: #000000; overflow: elide;
        }
        Text { x: 0; y: -4.5px; width: 70.5px; height: 7px; text: root.word; font-size: 19px; color: #ffffff; overflow: elide; }
    }
    Rectangle {
        x: root.layer-x; y: root.layer-y + 30px; width: 30px; height: 4px;
        cache-rendering-hint: root.cached;
        Text { x: 0; y: 0; width: 30px; height: 4px; text: "Cut text"; font-size: 15px; color: #ffcc00; overflow: elide; }
    }
}
"#;

#[test]
fn cached_captions_at_fractional_positions_match_direct_rendering() {
    // Whole frames: a changed caption's glyphs past its box are drawn again too. (Partial
    // frames repaint a changed item's box only, cached or not: ink past it can stay, as
    // equality_cutoff.rs bounds.)
    KIND.with(|k| k.set(RepaintBufferType::NewBuffer));
    let mut pair = Pair::<Rgb565Pixel>::new(CAPTIONS, 200, 80);
    let frames = |pair: &mut Pair<Rgb565Pixel>, step: &str| {
        // Moved or changed, then settled: drawn directly, then from the cache.
        for n in 0..4 {
            for window in &pair.windows {
                window.request_redraw();
            }
            pair.render();
            pair.assert_same(&format!("{step}, frame {n}"));
        }
    };
    for (x, y) in [(20.3, 10.6), (20.5, 10.5), (21.75, 11.25), (33.1, 9.9)] {
        pair.set("layer-x", Value::Number(x));
        pair.set("layer-y", Value::Number(y));
        frames(&mut pair, &format!("captions at ({x}, {y})"));
    }
    pair.set("word", Value::String("Elided again and again".into()));
    frames(&mut pair, "another caption");
    KIND.with(|k| k.set(RepaintBufferType::ReusedBuffer));
}

/// Rings larger than the window, as Zone's ground draws them: a cached layer whose
/// children are clipped by the window gives the same RGB565 pixels as drawing directly.
const RINGS: &str = r#"
export component Surface inherits Window {
    in property <bool> cached;
    in property <length> layer-x: 0px;
    in property <length> layer-y: 0px;
    in property <string> word;
    in property <length> a: 2px;
    background: #4ad49e;
    Rectangle {
        x: root.layer-x; y: root.layer-y; width: root.width; height: root.height;
        cache-rendering-hint: root.cached;
        Rectangle { x: 50 * root.a - 30 * root.a - 3 * root.a; y: 33 * root.a - 30 * root.a - 3 * root.a; width: 66 * root.a; height: self.width; border-radius: self.width / 2; border-width: 6 * root.a; border-color: #36b2ce; }
        Rectangle { x: 80 * root.a - 20 * root.a - 3 * root.a; y: 10 * root.a - 20 * root.a - 3 * root.a; width: 46 * root.a; height: self.width; border-radius: self.width / 2; border-width: 6 * root.a; border-color: #f7931e; }
    }
}
"#;

#[test]
fn cached_rings_past_the_window_match_direct_rendering() {
    for kind in [
        RepaintBufferType::ReusedBuffer,
        RepaintBufferType::NewBuffer,
    ] {
        KIND.with(|k| k.set(kind));
        let mut pair = Pair::<Rgb565Pixel>::new(RINGS, 200, 150);
        for a in [2.0, 2.37, 1.640625, 3.015625, 2.5] {
            pair.set("a", Value::Number(a));
            for n in 0..6 {
                for window in &pair.windows {
                    window.request_redraw();
                }
                let [walked, _] = pair.render();
                let _ = walked;
                pair.assert_same(&format!("rings, a {a}, {kind:?}, frame {n}"));
            }
        }
    }
    KIND.with(|k| k.set(RepaintBufferType::ReusedBuffer));
}
