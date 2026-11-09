//! Exercise the very WindowAdapter used by SurfaceWindow, without an Android
//! Surface/EGL dependency. Only the renderer changes (software instead of Skia).
#[path = "../vendor/i-slint-backend-android-activity/component_window.rs"]
mod component_window;

use component_window::ComponentWindow;
use korri_shell::backend::headless::Headless;
use slint::platform::{
    software_renderer::{RepaintBufferType, Rgb565Pixel, SoftwareRenderer},
    WindowAdapter,
};
use slint::{ComponentHandle, PhysicalSize};

#[test]
fn hosted_overlay_waits_for_a_shown_component_and_preserves_its_first_redraw() {
    Headless::install();
    let size = PhysicalSize::new(64, 48);
    let adapter = ComponentWindow::new(
        SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::NewBuffer),
        size,
    );
    adapter.resize(size, 1.0);
    let mut pixels = vec![Rgb565Pixel::default(); 64 * 48];
    let mut draw = || {
        adapter.render_if_needed(|_| {
            adapter.renderer().render(&mut pixels, 64);
            Ok::<(), slint::PlatformError>(())
        })
    };
    // Surface attachment and resize arrive before the asynchronous model/logic.
    // Rendering now used to fail because no component was associated yet.
    assert!(!draw().unwrap());
    assert!(!draw().unwrap());

    let result = pollster::block_on(slint_interpreter::Compiler::default().build_from_source(
        "export component Overlay inherits Window { background: #ff0000; }".into(),
        "overlay.slint".into(),
    ));
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let definition = result.component("Overlay").unwrap();
    let component = definition
        .create_with_existing_window(adapter.window())
        .unwrap();
    assert!(!draw().unwrap());
    component.show().unwrap();
    assert!(adapter.window().is_visible());
    assert!(draw().unwrap());
    assert!(!draw().unwrap());
    component.hide().unwrap();
    adapter.request_redraw();
    assert!(!draw().unwrap());
    component.show().unwrap();
    assert!(draw().unwrap());
}
