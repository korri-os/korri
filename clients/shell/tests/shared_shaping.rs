//! Shaping shared across items (vendored `TextLayoutCache::shared`): rows of a
//! list handed text that another row showed before take its shaped paragraphs
//! instead of shaping again, and draw the same pixels as a window that shapes
//! every text itself: through texts moving between rows, rows added and
//! removed, sizes and fonts changing, wrapping text, and the component's texts
//! in an RGB8 buffer.

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Model, Rgb8Pixel, VecModel};
use slint_interpreter::{ComponentInstance, Struct, Value};
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
}

struct Plain;

impl Platform for Plain {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        WINDOWS.with(|w| w.borrow_mut().push(window.clone()));
        Ok(window)
    }
}

const SOURCE: &str = r#"
import "fonts/tiny5.ttf";
export struct Row { label: string, size: length }
export component Surface inherits Window {
    in property <[Row]> rows;
    in property <length> wide: 120px;
    background: #102030;
    default-font-family: "Pico Tiny5";
    for row[i] in root.rows: Rectangle {
        x: 4px + mod(i, 2) * 150px;
        y: 4px + floor(i / 2) * 40px;
        width: 140px;
        height: 36px;
        background: #203040;
        Text { x: 2px; y: 2px; width: root.wide; text: row.label; font-size: row.size; color: #ffffff; wrap: word-wrap; }
    }
}
"#;

const WORDS: [&str; 6] = [
    "Celeste",
    "Hollow Knight",
    "Tide Pool and more",
    "Bramble",
    "Petal Quest",
    "Comet",
];

fn row(label: &str, size: f64) -> Value {
    let mut s = Struct::default();
    s.set_field("label".into(), Value::String(label.into()));
    s.set_field("size".into(), Value::Number(size));
    Value::Struct(s)
}

struct Side {
    instance: ComponentInstance,
    window: Rc<MinimalSoftwareWindow>,
    rows: Rc<VecModel<Value>>,
    pixels: Vec<Rgb8Pixel>,
    shapes: u64,
}

const W: usize = 320;
const H: usize = 200;

fn side(share: bool) -> Side {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/packages/probe/shared.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let instance = result.component("Surface").unwrap().create().unwrap();
    let rows = Rc::new(VecModel::from(
        WORDS
            .iter()
            .take(4)
            .map(|w| row(w, 14.0))
            .collect::<Vec<_>>(),
    ));
    instance
        .set_property("rows", Value::Model(slint::ModelRc::from(rows.clone())))
        .unwrap();
    instance.show().unwrap();
    let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
    window.set_size(slint::PhysicalSize::new(W as u32, H as u32));
    window.draw_if_needed(|renderer| renderer.text_layout_cache().set_share_shaping(share));
    Side {
        instance,
        window,
        rows,
        pixels: vec![Rgb8Pixel::default(); W * H],
        shapes: 0,
    }
}

impl Side {
    fn frame(&mut self) {
        self.window.request_redraw();
        let (pixels, shapes) = (&mut self.pixels, &mut self.shapes);
        self.window.draw_if_needed(|renderer| {
            renderer.set_frame_profile(true);
            renderer.render(pixels, W);
            *shapes += renderer.last_walk_profile().shape.count
                + renderer.take_frame_profile().shape.count;
        });
    }
}

#[test]
fn shaping_shared_across_rows_draws_the_same_pixels_and_shapes_less() {
    let _ = slint::platform::set_platform(Box::new(Plain));
    let mut sides = [side(true), side(false)];
    let mut step = |name: &str, change: &dyn Fn(&Side)| {
        for s in sides.iter_mut() {
            change(s);
            s.frame();
        }
        let differing = sides[0]
            .pixels
            .iter()
            .zip(&sides[1].pixels)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differing, 0, "{name}: {differing} pixels differ");
    };
    step("first", &|_| {});
    // Texts move between rows, and come back.
    for k in 0..12 {
        step(&format!("rotation {k}"), &|s| {
            for i in 0..s.rows.row_count() {
                let word = WORDS[(i + k) % WORDS.len()];
                s.rows.set_row_data(i, row(word, 14.0));
            }
        });
    }
    step("row added", &|s| s.rows.push(row(WORDS[1], 14.0)));
    step("row removed", &|s| {
        s.rows.remove(0);
    });
    step("a size that differs", &|s| {
        s.rows.set_row_data(0, row(WORDS[1], 18.5))
    });
    step("narrower, wrapping", &|s| {
        s.instance
            .set_property("wide", Value::Number(60.0))
            .unwrap()
    });
    step("back", &|s| s.rows.set_row_data(0, row(WORDS[1], 14.0)));
    assert!(
        sides[0].shapes * 2 < sides[1].shapes,
        "sharing shaped {} times, not sharing {}",
        sides[0].shapes,
        sides[1].shapes
    );
}
