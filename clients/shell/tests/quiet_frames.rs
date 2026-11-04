//! Frames where nothing visible changed are skipped (vendored
//! `WindowInner::draw_contents_if_changed`): a binding on `animation-tick()`
//! that only steps every 200 ms asks for a frame every tick, but only the
//! frames where its value changed walk the items. The pixels stay those of a
//! window that redraws everything every frame.

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Rgb8Pixel};
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

const SOURCE: &str = r#"
global Cycle {
    out property <int> phase: mod(floor(animation-tick() / 200ms), 3);
}
export component Surface inherits Window {
    in property <length> mover-x: 0px;
    in property <bool> extra;
    background: #204060;
    for i in 40: Rectangle {
        x: mod(i, 8) * 30px;
        y: 40px + floor(i / 8) * 20px;
        width: 24px;
        height: 14px;
        background: #ffffff20;
    }
    Rectangle {
        x: 10px;
        y: 10px;
        width: 40px;
        height: 20px;
        background: Cycle.phase == 0 ? #ff0044 : Cycle.phase == 1 ? #ff8800 : #ff66cc;
    }
    Rectangle { x: root.mover-x; y: 150px; width: 20px; height: 20px; background: #00ff88; }
    if root.extra: Rectangle { x: 200px; y: 10px; width: 30px; height: 30px; background: #ffff00; }
}
"#;

#[test]
fn quiet_frames_are_skipped_and_the_pixels_match_full_redraws() {
    let _ = slint::platform::set_platform(Box::new(Manual));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/quiet.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let definition = result.component("Surface").unwrap();
    let (width, height) = (260usize, 180usize);
    let make = |kind| {
        KIND.with(|k| k.set(kind));
        let instance = definition.create().unwrap();
        instance.show().unwrap();
        let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
        window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
        (instance, window, vec![Rgb8Pixel::default(); width * height])
    };
    let mut windows: [(ComponentInstance, Rc<MinimalSoftwareWindow>, Vec<Rgb8Pixel>); 2] = [
        make(RepaintBufferType::ReusedBuffer),
        make(RepaintBufferType::NewBuffer),
    ];
    let mut walked_frames = 0;
    let frames = 120;
    for frame in 0..frames {
        NOW.with(|n| n.set(n.get() + Duration::from_millis(16)));
        // Now and then a change of its own: a move, a branch shown and hidden.
        for (instance, _, _) in &windows {
            if frame == 40 {
                instance
                    .set_property("mover-x", Value::Number(50.0))
                    .unwrap();
            }
            if frame == 70 || frame == 90 {
                instance
                    .set_property("extra", Value::Bool(frame == 70))
                    .unwrap();
            }
        }
        slint::platform::update_timers_and_animations();
        for (i, (_, window, buffer)) in windows.iter_mut().enumerate() {
            // Every frame is offered: the window asks for one while animation-tick() is read.
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(buffer, width);
                if i == 0 && renderer.dirty_region_walked_items() > 0 {
                    walked_frames += 1;
                }
            });
        }
        let differing = windows[0]
            .2
            .iter()
            .zip(&windows[1].2)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differing, 0,
            "frame {frame}: {differing} pixels differ from a full redraw"
        );
    }
    // 120 frames of 16 ms: the phase steps 9 times; with the 3 changes above and the
    // first frame, about 13 frames have something to draw, and each may take one more.
    assert!(
        walked_frames <= 30,
        "{walked_frames} of {frames} frames walked the items"
    );
    assert!(
        walked_frames >= 10,
        "the changes were drawn: {walked_frames}"
    );
}
