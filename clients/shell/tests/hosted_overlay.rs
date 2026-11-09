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

#[test]
fn hosted_overlay_touch_scrolls_natively_and_cancel_never_activates() {
    use i_slint_core::{
        input::TouchPhase,
        platform::{InternalEvent, WindowEvent},
    };
    use slint_interpreter::Value;
    Headless::install();
    let adapter = ComponentWindow::new(
        SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::NewBuffer),
        PhysicalSize::new(640, 480),
    );
    adapter.resize(PhysicalSize::new(640, 480), 2.0);
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(
            r#"
        export component Overlay inherits Window {
            callback activated(target: string, at: float, released: bool);
            out property <float> offset: content.viewport-y / 1px;
            content := Flickable {
                viewport-height: 800px;
                for i in 10: Rectangle {
                    y: i * 80px; height: 80px; width: parent.width;
                    TouchArea { clicked => { root.activated("row", -1, false); } }
                }
            }
        }
    "#
            .into(),
            "overlay.slint".into(),
        ),
    );
    assert!(
        !result.has_errors(),
        "{:?}",
        result.diagnostics().collect::<Vec<_>>()
    );
    let component = result
        .component("Overlay")
        .unwrap()
        .create_with_existing_window(adapter.window())
        .unwrap();
    let activations = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = activations.clone();
    korri_shell::view::bind_activated(&component, move |message| sink.borrow_mut().push(message))
        .unwrap();
    component.show().unwrap();
    let touch = |y, phase| {
        adapter
            .window()
            .dispatch_event(WindowEvent::internal(InternalEvent::Touch {
                id: 0,
                position: i_slint_core::lengths::LogicalPoint::new(40.0, y),
                phase,
            }))
    };
    touch(180.0, TouchPhase::Started);
    touch(160.0, TouchPhase::Moved);
    touch(80.0, TouchPhase::Moved);
    touch(80.0, TouchPhase::Ended);
    assert!(matches!(component.get_property("offset").unwrap(), Value::Number(y) if y < -20.0));
    assert!(
        activations.borrow().is_empty(),
        "a native scroll is not an activation"
    );
    touch(40.0, TouchPhase::Started);
    touch(40.0, TouchPhase::Cancelled);
    assert!(
        activations.borrow().is_empty(),
        "cancellation must not activate"
    );
    touch(40.0, TouchPhase::Started);
    touch(40.0, TouchPhase::Ended);
    assert_eq!(
        *activations.borrow(),
        vec![serde_json::json!({"_tag": "Activated", "target": "row"})],
        "a normal finger-up after cancellation must still activate"
    );
    component.hide().unwrap();
}
