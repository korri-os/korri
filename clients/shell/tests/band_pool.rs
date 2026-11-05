//! Band threads stay between frames (vendored `pool.rs`): a frame rasterised in bands
//! uses the threads of the frames before, parked in between, instead of starting new
//! ones. Zone's attract started about 240 threads a second on the RG353M. Pixels are
//! `parallel_render.rs`'s concern; this checks the threads.

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Rgb8Pixel};
use std::{cell::Cell, cell::RefCell, rc::Rc, time::Duration};

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
    static NOW: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

struct Manual;

impl Platform for Manual {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        WINDOWS.with(|w| w.borrow_mut().push(window.clone()));
        Ok(window)
    }
    fn duration_since_start(&self) -> Duration {
        NOW.with(|n| n.get())
    }
}

// A whole-window gradient that changes every frame: each frame is drawn in bands.
const SOURCE: &str = r#"
export component Surface inherits Window {
    property <float> t: mod(animation-tick() / 1ms, 1000) / 1000;
    Rectangle {
        background: @linear-gradient(30deg, #ff0055 0%, #00ffaa 100%);
        opacity: 0.5 + root.t / 2;
    }
}
"#;

/// The threads of this process named as the renderer's band threads.
fn band_threads() -> Vec<String> {
    let mut threads: Vec<String> = std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(|task| {
            let task = task.ok()?;
            let name = std::fs::read_to_string(task.path().join("comm")).ok()?;
            (name.trim() == "slint-band").then(|| task.file_name().to_string_lossy().into())
        })
        .collect();
    threads.sort();
    threads
}

#[test]
fn band_threads_stay_between_frames() {
    let _ = slint::platform::set_platform(Box::new(Manual));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/band_pool.slint");
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let instance = result.component("Surface").unwrap().create().unwrap();
    instance.show().unwrap();
    let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
    let (width, height) = (320usize, 240usize);
    window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
    let mut buffer = vec![Rgb8Pixel::default(); width * height];
    let mut frame = |window: &MinimalSoftwareWindow| {
        NOW.with(|n| n.set(n.get() + Duration::from_millis(16)));
        slint::platform::update_timers_and_animations();
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.set_render_threads(4);
            let region = renderer.render(&mut buffer, width);
            assert!(region.iter().next().is_some(), "every frame draws");
        });
    };
    for _ in 0..3 {
        frame(&window);
    }
    let first = band_threads();
    assert!(
        !first.is_empty(),
        "no band thread stays after a frame drawn in bands"
    );
    for _ in 0..50 {
        frame(&window);
    }
    assert_eq!(
        band_threads(),
        first,
        "50 more frames started other band threads"
    );
}
