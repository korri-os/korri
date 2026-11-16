//! Opt-in CPU phase diagnostics. Never logs view/model/input payloads.
use std::sync::OnceLock;

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("KORRI_SHELL_PROFILE").is_some_and(|v| v == "1"))
}

thread_local! {
    static FRAME_STARTED: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// The window was asked to draw: the frame's CPU time starts here.
pub fn frame_started() {
    FRAME_STARTED.with(|f| f.set(Some(std::time::Instant::now())));
    i_slint_core::frame_profile::take();
}

/// Rendering ended (before the buffer swap): log the frame's phases.
pub fn frame_rendered() {
    let Some(started) = FRAME_STARTED.with(|f| f.take()) else {
        return;
    };
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    let p = i_slint_core::frame_profile::take();
    eprintln!(
        "SHELL_PROFILE phase=frame render_ms={:.3} instantiate_ms={:.3} geometry_ms={:.3} bindings_ms={:.3} text_measure_ms={:.3} shape_ms={:.3} text_draw_ms={:.3} layers={} layer_ms={:.3} items={} bindings={} created={} rows={}",
        ms(started.elapsed()),
        ms(p.instantiate.time),
        ms(p.geometry.time),
        ms(p.binding.time),
        ms(p.text_measure.time),
        ms(p.shape.time),
        ms(p.text_draw.time),
        p.layer_draw.count,
        ms(p.layer_draw.time),
        p.items_visited,
        p.binding.count,
        p.instances_created,
        p.rows_updated
    );
}
