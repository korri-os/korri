//! Small changes far apart stay small regions (vendored `DirtyRegion::MAX_COUNT`):
//! Zone's still screen moves a few small things every frame (its ground dots, the
//! breathing triangles of its button rows, top and bottom). With room for only three
//! rectangles, a fourth change merged into one of them and the frame redrew a box
//! spanning the screen: 57-78 kpx a frame where 3 kpx changed. The pixels stay those
//! of a window that redraws everything, with a reused buffer and with two swapped
//! buffers (Wayland).

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::Rgb8Pixel;
use slint_interpreter::ComponentInstance;
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

// Eight 6 px squares, two in each corner, each moving one pixel a frame over a still
// text-free scene: every frame changes 8 small areas far apart.
const SOURCE: &str = r#"
global Clock {
    out property <int> step: floor(animation-tick() / 16ms);
}
export component Surface inherits Window {
    background: #203040;
    for i in 30: Rectangle {
        x: mod(i, 6) * 50px + 10px;
        y: floor(i / 6) * 45px + 10px;
        width: 40px;
        height: 30px;
        background: #ffffff18;
    }
    for corner in [
        { x: 4, y: 4 }, { x: 30, y: 4 }, { x: 230, y: 4 }, { x: 230, y: 30 },
        { x: 4, y: 200 }, { x: 4, y: 226 }, { x: 230, y: 200 }, { x: 206, y: 226 },
    ]: Rectangle {
        x: (corner.x + Clock.step) * 1px;
        y: corner.y * 1px;
        width: 6px;
        height: 6px;
        background: #ff8800;
    }
}
"#;

#[test]
fn small_changes_far_apart_stay_small_regions() {
    let _ = slint::platform::set_platform(Box::new(Manual));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dirty_rects.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let definition = result.component("Surface").unwrap();
    let (width, height) = (300usize, 240usize);
    let make = |kind| {
        KIND.with(|k| k.set(kind));
        let instance = definition.create().unwrap();
        slint::ComponentHandle::show(&instance).unwrap();
        let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
        window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
        let buffers = vec![vec![Rgb8Pixel::default(); width * height]; 2];
        (instance, window, buffers)
    };
    type Window = (
        ComponentInstance,
        Rc<MinimalSoftwareWindow>,
        Vec<Vec<Rgb8Pixel>>,
    );
    let mut windows: [Window; 3] = [
        make(RepaintBufferType::ReusedBuffer),
        make(RepaintBufferType::SwappedBuffers),
        make(RepaintBufferType::NewBuffer),
    ];
    let frames = 40;
    let mut redrawn = [0usize; 2];
    for frame in 0..frames {
        NOW.with(|n| n.set(n.get() + Duration::from_millis(16)));
        slint::platform::update_timers_and_animations();
        for (i, (_, window, buffers)) in windows.iter_mut().enumerate() {
            // A swapped window draws into its two buffers in turn.
            let buffer = &mut buffers[if i == 1 { frame % 2 } else { 0 }];
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                let region = renderer.render(buffer, width);
                let pixels: usize = region
                    .iter()
                    .map(|(_, size)| size.width as usize * size.height as usize)
                    .sum();
                if i < 2 && frame >= 2 {
                    redrawn[i] = redrawn[i].max(pixels);
                }
            });
        }
        let full = &windows[2].2[0];
        for (i, name) in [(0, "reused"), (1, "swapped")] {
            let drawn = &windows[i].2[if i == 1 { frame % 2 } else { 0 }];
            let differing = drawn.iter().zip(full).filter(|(a, b)| a != b).count();
            assert_eq!(
                differing, 0,
                "frame {frame}, {name} buffer: {differing} pixels differ from a full redraw"
            );
        }
    }
    // Each square's old and new place: 7x6 px, eight of them; a swapped buffer also
    // redraws the frame before. A merged region spans corners: thousands of pixels.
    assert!(
        redrawn[0] <= 8 * 7 * 6,
        "a reused buffer redrew up to {} px a frame",
        redrawn[0]
    );
    assert!(
        redrawn[1] <= 8 * 8 * 6,
        "swapped buffers redrew up to {} px a frame",
        redrawn[1]
    );
}
