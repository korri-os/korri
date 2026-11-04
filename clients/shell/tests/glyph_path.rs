//! The vendored renderer's glyph fast path (`draw_texture_line`, alpha maps):
//! texts at fractional positions and sizes, in opaque and translucent colours,
//! under opacity, clipped, and scrolled draw the same pixels with the fast path
//! and with the general texture path, into RGB8 and RGB565 buffers.

use slint::platform::{
    software_renderer::{
        MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel, SoftwareRenderer, TargetPixel,
    },
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Rgb8Pixel};
use slint_interpreter::Value;
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
}

struct Plain;

impl Platform for Plain {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        WINDOWS.with(|w| w.borrow_mut().push(window.clone()));
        Ok(window)
    }
}

const SOURCE: &str = r#"
import "fonts/tiny5.ttf";
export component Surface inherits Window {
    in property <length> shift: 0px;
    background: #203040;
    default-font-family: "Pico Tiny5";
    Text { x: 3px + root.shift; y: 4.5px; text: "Opaque glyphs 0123"; color: #ffffff; font-size: 17px; }
    Text { x: 10.25px; y: 30px + root.shift; text: "Translucent ink"; color: #ff880080; font-size: 23.5px; }
    Rectangle {
        y: 70px; height: 60px; opacity: 0.6;
        Text { x: 5px + root.shift; text: "Under opacity"; color: #00ffcc; font-size: 31px; }
    }
    Rectangle {
        x: 20px; y: 140px; width: 90px; height: 18px; clip: true;
        Text { x: -7px + root.shift; y: -3px; text: "Clipped on every side"; color: #f0f0f0; font-size: 21px; }
    }
    Text { x: 150.5px; y: 180.75px; text: "tiny"; color: #c0c0ff; font-size: 9px; }
}
"#;

fn draw<P: TargetPixel + Default + Clone>(fast: bool, shift: f64) -> Vec<P> {
    SoftwareRenderer::set_glyph_fast_path(fast);
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/packages/probe/glyphs.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let instance = result.component("Surface").unwrap().create().unwrap();
    instance
        .set_property("shift", Value::Number(shift))
        .unwrap();
    instance.show().unwrap();
    let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
    let (w, h) = (260usize, 200usize);
    window.set_size(slint::PhysicalSize::new(w as u32, h as u32));
    let mut buffer = vec![P::default(); w * h];
    window.draw_if_needed(|renderer| {
        renderer.set_render_threads(1);
        renderer.render(&mut buffer, w);
    });
    SoftwareRenderer::set_glyph_fast_path(true);
    buffer
}

fn same<P: TargetPixel + Default + Clone + PartialEq>(name: &str) {
    for shift in [0.0, 0.5, 1.25, 7.0] {
        let general = draw::<P>(false, shift);
        let fast = draw::<P>(true, shift);
        let differing = general.iter().zip(&fast).filter(|(a, b)| a != b).count();
        assert_eq!(
            differing, 0,
            "{name}, shift {shift}: {differing} pixels differ"
        );
        assert!(
            general.iter().any(|p| *p != general[0]),
            "{name}: something is drawn"
        );
    }
}

#[test]
fn the_glyph_fast_path_draws_what_the_general_path_draws() {
    let _ = slint::platform::set_platform(Box::new(Plain));
    same::<Rgb8Pixel>("RGB8");
    same::<Rgb565Pixel>("RGB565");
}
