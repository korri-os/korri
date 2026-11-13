//! The vendored software renderer clips children to a `Clip`'s border radius
//! (vendor/README.md, "Rounded clip"): a picture, a filled rectangle and a
//! drawn path inside a rounded `clip: true` rectangle leave its corners empty,
//! fill its inside, and blend its curved edge, at scale 1 and 2.

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Rgb8Pixel, SharedPixelBuffer};
use slint_interpreter::Value;
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
}

struct Fresh(std::time::Instant);

impl Platform for Fresh {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        WINDOWS.with(|w| w.borrow_mut().push(window.clone()));
        Ok(window)
    }
    fn duration_since_start(&self) -> std::time::Duration {
        self.0.elapsed()
    }
}

const SOURCE: &str = r#"
export component Surface inherits Window {
    in property <image> picture;
    in property <int> child: 0;
    background: #000000;
    width: 100px;
    height: 100px;
    Rectangle {
        x: 10px;
        y: 10px;
        width: 60px;
        height: 80px;
        border-radius: 12px;
        clip: true;
        if root.child == 0: Image { width: 100%; height: 100%; source: root.picture; image-fit: fill; }
        if root.child == 1: Rectangle { background: #ff0000; }
        if root.child == 2: Path {
            width: 100%;
            height: 100%;
            fill: #ff0000;
            viewbox-width: 60;
            viewbox-height: 80;
            commands: "M 0 0 L 60 0 L 60 80 L 0 80 Z";
        }
    }
}
"#;

/// Renders the surface with child kind `child` at `scale`; returns the pixels and the size.
fn render(child: i32, scale: f32) -> (Vec<Rgb8Pixel>, usize) {
    let _ = slint::platform::set_platform(Box::new(Fresh(std::time::Instant::now())));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/packages/probe/rounded-clip.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let instance = result.component("Surface").unwrap().create().unwrap();
    let mut red = SharedPixelBuffer::<Rgb8Pixel>::new(6, 8);
    red.make_mut_slice().fill(Rgb8Pixel { r: 255, g: 0, b: 0 });
    instance
        .set_property("picture", Value::Image(slint::Image::from_rgb8(red)))
        .unwrap();
    instance
        .set_property("child", Value::Number(child as f64))
        .unwrap();
    instance.show().unwrap();
    let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
    window.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    let side = (100. * scale) as u32;
    window.set_size(slint::PhysicalSize::new(side, side));
    let mut buffer = vec![Rgb8Pixel::default(); (side * side) as usize];
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, side as usize);
    });
    (buffer, side as usize)
}

/// Checks one rendering, in logical coordinates scaled to physical pixels.
fn check(child: i32, scale: f32) {
    let (pixels, side) = render(child, scale);
    let at = |x: f32, y: f32| pixels[(y * scale) as usize * side + (x * scale) as usize].r;
    // Outside the clip, and in its corners outside the 12px arc: nothing drawn.
    assert_eq!(at(5., 5.), 0, "outside the clip");
    for (x, y) in [
        (10.5, 10.5),
        (69.5, 10.5),
        (69.5, 89.5),
        (10.5, 89.5),
        (12., 11.),
        (68., 12.),
    ] {
        assert_eq!(
            at(x, y),
            0,
            "corner ({x}, {y}) of child {child} at scale {scale} must stay empty"
        );
    }
    // Inside the arc and inside the straight sides: fully drawn.
    for (x, y) in [
        (40., 50.),
        (10.5, 50.),
        (69.5, 50.),
        (40., 10.5),
        (40., 89.5),
        (16., 16.),
        (64., 84.),
    ] {
        assert_eq!(
            at(x, y),
            255,
            "inside ({x}, {y}) of child {child} at scale {scale} must be filled"
        );
    }
    // The curved edge is blended: some pixels in the top-left corner square are
    // neither empty nor full.
    let blended = (0..(12. * scale) as usize)
        .flat_map(|dy| (0..(12. * scale) as usize).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| {
            pixels[((10. * scale) as usize + dy) * side + (10. * scale) as usize + dx].r
        })
        .filter(|r| *r > 0 && *r < 255)
        .count();
    assert!(
        blended > 0,
        "child {child} at scale {scale}: the curved edge has no blended pixels"
    );
}

#[test]
fn a_picture_is_clipped_to_the_border_radius() {
    check(0, 1.);
    check(0, 2.);
}

#[test]
fn a_filled_rectangle_is_clipped_to_the_border_radius() {
    check(1, 1.);
    check(1, 2.);
}

#[test]
fn a_path_is_clipped_to_the_border_radius() {
    check(2, 1.);
    check(2, 2.);
}
