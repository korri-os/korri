//! The vendored software renderer's layer cache (`cache-rendering-hint: true`,
//! vendor/README.md): a cached layer draws the same pixels as the same tree
//! drawn directly, through partial rendering, property changes, movement
//! around it, sub-pixel placement, resizes and scale changes; and a clean
//! cached layer is not walked.

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Rgb8Pixel};
use slint_interpreter::{ComponentDefinition, ComponentInstance, Value};
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

struct Pair {
    instances: [ComponentInstance; 2],
    windows: [Rc<MinimalSoftwareWindow>; 2],
    buffers: [Vec<Rgb8Pixel>; 2],
    size: (u32, u32),
}

impl Pair {
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
            *buffer = vec![Rgb8Pixel::default(); (width * height) as usize];
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
    let mut pair = Pair::new(SOURCE, 200, 120);
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
    let mut pair = Pair::new(&source, 200, 120);
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
