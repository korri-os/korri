//! A frame that draws the image the screen already shows is neither drawn nor presented
//! (vendored `parallel::Recorder`): its recorded draw calls are those of the previous frame
//! over the same region. Zone's attract pans a cover a fraction of a pixel a frame, so most
//! frames round to the image before; its still screen breathes triangles by fractions of a
//! pixel. An opaque rectangle covering the whole region hides what was recorded under it,
//! so a change under it draws nothing either.
//!
//! The pixels shown stay those of a window that redraws everything every frame, with a
//! reused buffer and with two swapped buffers, where a frame not presented leaves the
//! back buffer for the next frame.

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::Rgb8Pixel;
use slint_interpreter::{ComponentInstance, Value};
use std::{cell::Cell, cell::RefCell, rc::Rc, time::Duration};

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
    static KIND: Cell<RepaintBufferType> = const { Cell::new(RepaintBufferType::ReusedBuffer) };
    static NOW: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

struct Manual;

impl Platform for Manual {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(KIND.with(|k| k.get()));
        WINDOWS.with(|w| w.borrow_mut().push(window.clone()));
        Ok(window)
    }
    fn duration_since_start(&self) -> Duration {
        NOW.with(|n| n.get())
    }
}

// A slow pan (an image, a gradient and a rounded rectangle moving 0.15 px a frame) under a
// still word and path, a rectangle breathing by a fraction of a pixel, and from `covered` on, an opaque cover over the
// whole window with its own slow pan, under which a square keeps moving.
// A packaged font: the Nix build sandbox has no system font.
const SOURCE: &str = r#"
import "fonts/tiny5.ttf";
global Clock {
    out property <float> ms: animation-tick() / 1ms;
}
export component Surface inherits Window {
    in property <bool> covered;
    in property <image> art;
    property <length> pan: -Clock.ms / 16 * 0.15px;
    background: #102030;
    default-font-family: "Pico Tiny5";
    for i in 12: Rectangle {
        x: mod(i, 4) * 60px + 8px;
        y: floor(i / 4) * 50px + 8px;
        width: 50px;
        height: 40px;
        background: #ffffff20;
    }
    Rectangle {
        x: 20px + root.pan;
        y: 30px;
        width: 140px;
        height: 90px;
        clip: true;
        Image { x: 0; y: 0; width: 140px; height: 90px; source: root.art; image-rendering: pixelated; }
        Rectangle { x: 70px; y: 0; width: 70px; height: 40px; background: @linear-gradient(-28deg, #ff0044 0%, #ff0044 40%, #ffcc00 40%, #ffcc00 100%); }
        Rectangle { x: 6px; y: 50px; width: 40px; height: 20px; border-radius: 6px; background: #ffffff; }
    }
    // Still text and a still path over the pan: drawn again in each frame's region.
    Text { x: 30px; y: 80px; text: "Over"; font-size: 22px; color: #ffffff; }
    Path {
            x: 100px; y: 80px; width: 30px; height: 30px;
            viewbox-x: 0; viewbox-y: 0; viewbox-width: 2; viewbox-height: 2;
            commands: "M 0 1 A 1 1 0 1 0 2 1 A 1 1 0 1 0 0 1 Z";
            fill: #00ff88;
    }
    // Breathing by less than half a pixel: drawn where it was.
    Rectangle {
        x: 180px + 0.4px * Math.sin(Clock.ms / 1200 * 360deg);
        y: 150px;
        width: 30px;
        height: 12px;
        background: #ffee88;
    }
    // Moving 3 px a frame once the cover hides it.
    Rectangle { x: root.covered ? 200px + root.pan * 20 : 200px; y: 200px; width: 10px; height: 10px; background: #ff00ff; }
    if root.covered: Rectangle {
        x: 0; y: 0; width: root.width; height: root.height;
        background: #000000;
        // Like Zone's attract: a well wider than the window, panning.
        Rectangle {
            x: root.pan; y: 0; width: root.width + 40px; height: root.height;
            clip: true;
            Image { x: 0; y: 0; width: parent.width; height: parent.height; source: root.art; image-rendering: pixelated; }
            Rectangle { x: 40px; y: 60px; width: 100px; height: 60px; background: #2050ff; }
        }
    }
}
"#;

fn art() -> slint::Image {
    let (w, h) = (35u32, 23u32);
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
    for (i, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        *pixel = slint::Rgba8Pixel {
            r: (x * 7) as u8,
            g: (y * 11) as u8,
            b: ((x ^ y) * 5) as u8,
            a: 255,
        };
    }
    slint::Image::from_rgba8(buffer)
}

struct Window {
    instance: ComponentInstance,
    window: Rc<MinimalSoftwareWindow>,
    buffers: Vec<Vec<Rgb8Pixel>>,
    /// The buffer drawn into next (swapped buffers), and the one shown.
    back: usize,
    shown: usize,
}

#[test]
fn frames_with_the_image_shown_are_not_drawn_and_the_pixels_match_full_redraws() {
    let _ = slint::platform::set_platform(Box::new(Manual));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/packages/probe/identical.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let definition = result.component("Surface").unwrap();
    let (width, height) = (260usize, 240usize);
    let make = |kind| {
        KIND.with(|k| k.set(kind));
        let instance = definition.create().unwrap();
        instance.set_property("art", Value::Image(art())).unwrap();
        slint::ComponentHandle::show(&instance).unwrap();
        let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
        window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
        let buffers = vec![vec![Rgb8Pixel::default(); width * height]; 2];
        Window {
            instance,
            window,
            buffers,
            back: 0,
            shown: 0,
        }
    };
    let mut windows = [
        make(RepaintBufferType::ReusedBuffer),
        make(RepaintBufferType::SwappedBuffers),
        make(RepaintBufferType::NewBuffer),
    ];
    let frames = 160;
    let mut not_presented = [0usize; 2];
    let mut presented_unchanged = [0usize; 2];
    for frame in 0..frames {
        NOW.with(|n| n.set(n.get() + Duration::from_millis(16)));
        if frame == 80 {
            for w in &windows {
                w.instance
                    .set_property("covered", Value::Bool(true))
                    .unwrap();
            }
        }
        slint::platform::update_timers_and_animations();
        for (i, w) in windows.iter_mut().enumerate() {
            let swapped = i == 1;
            let before = w.buffers[w.shown].clone();
            let Window {
                window,
                buffers,
                back,
                shown,
                ..
            } = w;
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                let region = renderer.render(&mut buffers[*back], width);
                let presented = region.iter().next().is_some();
                if presented {
                    *shown = *back;
                    if swapped {
                        *back = 1 - *back;
                    }
                }
                if i < 2 && frame >= 2 {
                    if !presented {
                        not_presented[i] += 1;
                    } else if buffers[*shown] == before {
                        presented_unchanged[i] += 1;
                    }
                }
            });
        }
        let full = &windows[2].buffers[windows[2].shown];
        for (i, name) in [(0, "reused"), (1, "swapped")] {
            let shown = &windows[i].buffers[windows[i].shown];
            let differing = shown.iter().zip(full).filter(|(a, b)| a != b).count();
            assert_eq!(
                differing, 0,
                "frame {frame}, {name} buffer: {differing} pixels differ from a full redraw"
            );
        }
    }
    for (i, name) in [(0, "reused"), (1, "swapped")] {
        // The pan moves a whole pixel every 6 or 7 frames: most frames show the image before.
        assert!(
            not_presented[i] >= frames / 2,
            "{name}: only {} of {frames} frames were not drawn",
            not_presented[i]
        );
        // A scaled image moved by a fraction of a pixel records another source offset,
        // which nearest-neighbour sampling may still draw the same: those frames are drawn.
        assert!(
            presented_unchanged[i] <= frames / 4,
            "{name}: {} frames were drawn and presented with the image already shown",
            presented_unchanged[i]
        );
    }
}
