//! The vendored text layout cache (vendor/README.md, `cached_paragraphs`): a
//! text whose dependencies are marked dirty while its shaping inputs keep
//! their values is not shaped again, and its pixels equal a fresh window's.
//! A real change of an input still shapes it again.

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

/// `box` animates a card's size, as Zone's cards do. The font size and the
/// text depend on it but are clamped, so their values stay the same.
const SOURCE: &str = r#"
import "fonts/tiny5.ttf";
export component Surface inherits Window {
    in property <length> box: 200px;
    in property <length> cap: 24px;
    background: #000000;
    default-font-family: "Pico Tiny5";
    for i in 6: Text {
        x: 10px;
        y: 10px + i * 30px;
        width: root.box;
        text: root.box > 0 ? "CARD " + i : "";
        font-size: Math.max(root.cap, root.box / 100);
        color: #ffffff;
    }
}
"#;

fn instance() -> (ComponentInstance, Rc<MinimalSoftwareWindow>) {
    let _ = slint::platform::set_platform(Box::new(Reused(std::time::Instant::now())));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/packages/probe/text-reshape-test.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let instance = result
        .component("Surface")
        .expect("Surface")
        .create()
        .unwrap();
    instance.show().unwrap();
    let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
    window.set_size(slint::PhysicalSize::new(320, 240));
    (instance, window)
}

/// Renders a frame; returns the pixels and the shaping misses since the last call.
fn render(window: &MinimalSoftwareWindow, buffer: &mut [Rgb8Pixel]) -> u64 {
    slint::platform::update_timers_and_animations();
    let mut misses = 0;
    window.draw_if_needed(|renderer| {
        renderer.render(buffer, 320);
        let cache = renderer.text_layout_cache();
        misses = cache.cache_miss_count();
        cache.reset_cache_miss_count();
    });
    misses
}

#[test]
fn a_dirty_text_with_unchanged_inputs_is_not_shaped_again() {
    let (moving, window) = instance();
    let mut buffer = vec![Rgb8Pixel::default(); 320 * 240];
    assert!(
        render(&window, &mut buffer) > 0,
        "the first frame shapes the texts"
    );

    for (step, width) in [210.0, 230.0, 260.0, 300.0].into_iter().enumerate() {
        moving.set_property("box", Value::Number(width)).unwrap();
        let misses = render(&window, &mut buffer);
        assert_eq!(misses, 0, "step {step}: same text and font, shaped again");

        // A window that starts at this size draws the same pixels.
        let (fresh, fresh_window) = instance();
        fresh.set_property("box", Value::Number(width)).unwrap();
        let mut expected = vec![Rgb8Pixel::default(); 320 * 240];
        render(&fresh_window, &mut expected);
        let differing = buffer.iter().zip(&expected).filter(|(a, b)| a != b).count();
        assert_eq!(
            differing, 0,
            "step {step}: pixels differ from a fresh window"
        );
        fresh.hide().unwrap();
    }

    moving.set_property("cap", Value::Number(30.0)).unwrap();
    assert_eq!(
        render(&window, &mut buffer),
        6,
        "a new font size shapes all six texts"
    );
}
