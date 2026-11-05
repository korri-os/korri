//! The vendored renderer's run path for colour textures (`draw_texture_line`,
//! `fill_scaled_runs`): RGB, RGBA (opaque, translucent and transparent pixels) and
//! premultiplied RGBA images, scaled up and down by whole and fractional factors, at
//! fractional positions, clipped on every side, tiled and moved by whole and fractional
//! pixels, draw the same pixels with runs and a pixel at a time, into RGB8 and RGB565
//! buffers. Zone's covers and the attract's panning cover are such images.

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
export component Surface inherits Window {
    in property <image> rgb;
    in property <image> rgba;
    in property <image> premultiplied;
    in property <length> shift: 0px;
    background: #203040;
    // Up by 6.03, at a fractional place, clipped by the window on the left.
    Image { x: -13.4px + root.shift; y: 2.5px; width: 211px; height: 70px; source: root.rgb; image-rendering: pixelated; image-fit: fill; }
    // Translucent and transparent pixels over a gradient, up by a fraction.
    Rectangle {
        x: 0; y: 74px; width: 260px; height: 60px;
        background: @linear-gradient(90deg, #ff0044 0%, #00ffcc 100%);
        Image { x: 3.7px + root.shift; y: 1px; width: 97.3px; height: 58px; source: root.rgba; image-fit: fill; }
        Image { x: 120.2px; y: 0; width: 140px; height: 60px; source: root.premultiplied; image-rendering: pixelated; image-fit: fill; }
    }
    // Down by a fraction, and clipped on every side.
    Rectangle {
        x: 20px; y: 140px; width: 90px; height: 40px; clip: true;
        Image { x: -7.25px + root.shift; y: -3px; width: 120px; height: 50px; source: root.rgba; image-fit: fill; }
    }
    Image { x: 120px; y: 140px; width: 20.5px; height: 13px; source: root.premultiplied; image-fit: fill; }
    // Tiled across, and contained (letterboxed).
    Image { x: 150px + root.shift; y: 140px; width: 105px; height: 30px; source: root.rgb; horizontal-tiling: repeat; }
    Image { x: 150px; y: 172px; width: 100px; height: 26px; source: root.rgba; image-fit: contain; image-rendering: pixelated; }
}
"#;

fn pixels(alpha: impl Fn(u32, u32) -> u8) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    let (w, h) = (35u32, 23u32);
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
    for (i, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        let a = alpha(x, y);
        *pixel = slint::Rgba8Pixel {
            r: ((x * 7) as u16 * a as u16 / 255) as u8,
            g: ((y * 11) as u16 * a as u16 / 255) as u8,
            b: (((x ^ y) * 5) as u16 * a as u16 / 255) as u8,
            a,
        };
    }
    buffer
}

fn images() -> [slint::Image; 3] {
    let opaque = pixels(|_, _| 255);
    let mut rgb = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(35, 23);
    for (to, from) in rgb.make_mut_slice().iter_mut().zip(opaque.as_slice()) {
        *to = slint::Rgb8Pixel {
            r: from.r,
            g: from.g,
            b: from.b,
        };
    }
    // Opaque, transparent and translucent pixels in bands.
    let mixed = |x: u32, y: u32| match (x + 2 * y) % 5 {
        0 => 0,
        1 => 90,
        2 => 200,
        _ => 255,
    };
    [
        slint::Image::from_rgb8(rgb),
        slint::Image::from_rgba8(pixels(mixed)),
        slint::Image::from_rgba8_premultiplied(pixels(mixed)),
    ]
}

fn draw<P: TargetPixel + Default + Clone>(runs: bool, shift: f64) -> Vec<P> {
    SoftwareRenderer::set_scaled_runs(runs);
    let result = pollster::block_on(
        slint_interpreter::Compiler::default()
            .build_from_source(SOURCE.into(), "runs.slint".into()),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let instance = result.component("Surface").unwrap().create().unwrap();
    let [rgb, rgba, premultiplied] = images();
    instance.set_property("rgb", Value::Image(rgb)).unwrap();
    instance.set_property("rgba", Value::Image(rgba)).unwrap();
    instance
        .set_property("premultiplied", Value::Image(premultiplied))
        .unwrap();
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
    SoftwareRenderer::set_scaled_runs(true);
    buffer
}

fn same<P: TargetPixel + Default + Clone + PartialEq>(name: &str) {
    for shift in [0.0, 0.4, 1.0, 1.25, 7.0, -3.6] {
        let runs = draw::<P>(true, shift);
        let general = draw::<P>(false, shift);
        let differing = runs.iter().zip(&general).filter(|(a, b)| a != b).count();
        assert_eq!(
            differing, 0,
            "{name}, shifted {shift}: {differing} pixels differ"
        );
        assert!(
            runs.iter().any(|p| *p != runs[0]),
            "{name}: nothing was drawn"
        );
    }
}

#[test]
fn colour_textures_draw_the_same_pixels_with_runs() {
    slint::platform::set_platform(Box::new(Plain)).unwrap();
    same::<Rgb8Pixel>("RGB8");
    same::<Rgb565Pixel>("RGB565");
}
