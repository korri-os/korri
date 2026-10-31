//! THROWAWAY SPIKE: Zone's GAMES page, state Ready, Modern look, as a
//! native Rust + Slint window. Fixture values only; no korrid, no network,
//! no input but Escape. See README.md.

mod ground;
mod plan;
mod text;

mod ui {
    slint::include_modules!();
}

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{Color, ComponentHandle, Image, LogicalSize, ModelRc, SharedString, VecModel};

/// The drift's turns (zone-tokens.css `--zone-drift-big`, `--zone-drift-small`).
const DRIFT_A_MS: f32 = 140_000.0;
const DRIFT_B_MS: f32 = 200_000.0;

fn colour(c: u32) -> Color {
    Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn size_from_env() -> (f32, f32) {
    std::env::var("ZONE_SPIKE_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .filter(|&(w, h): &(f32, f32)| w >= 1.0 && h >= 1.0)
        .unwrap_or((640.0, 480.0))
}

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1")
}

fn to_ui(p: &plan::Prim) -> ui::Prim {
    ui::Prim {
        kind: p.kind.code(),
        x: p.x,
        y: p.y,
        w: p.w,
        h: p.h,
        color: colour(p.color),
        radius: p.radius,
        path: SharedString::from(p.path.as_str()),
        text: SharedString::from(p.text.as_str()),
        size: p.size,
        elide: p.elide,
        fx: p.fx,
        r: p.r,
        ring: colour(p.ring),
        drop: colour(p.drop),
        band: colour(p.band),
        band_y: p.band_y,
        band_h: p.band_h,
        art: p.art,
        ix: p.ix,
        iy: p.iy,
        iw: p.iw,
        ih: p.ih,
        // Unclipped words get a box well past their ring and drop.
        clip: p.clip.is_some(),
        cx: p.clip.map_or(p.x - 8.0 * p.r, |c| c.0),
        cy: p.clip.map_or(p.y - 8.0 * p.r, |c| c.1),
        cw: p.clip.map_or(p.w + 16.0 * p.r, |c| c.2),
        ch: p.clip.map_or(p.h + 16.0 * p.r, |c| c.3),
    }
}

/// Plan and paint everything for a window `w` by `h` logical px.
fn apply(window: &ui::ZoneWindow, measure: &text::Measure, w: f32, h: f32) {
    let scene = plan::scene(measure, w, h);
    let a = scene.plan.a;
    let sf = window.window().scale_factor();

    // The rings, once per size (physical px, so they stay crisp).
    let rings = ground::rings(scene.plan.w_art, scene.plan.h_art);
    let pw = (w * sf).round() as u32;
    let ph = (h * sf).round() as u32;
    let ground_px = ground::paint_ground(
        pw,
        ph,
        a * sf,
        rings.b,
        plan::colour::MINT,
        plan::colour::RING_B,
    );
    window.set_ground(Image::from_rgb8(ground_px));
    let (ring_px, x0, y0) = ground::paint_ring(a * sf, rings.a, plan::colour::RING_A);
    let side = ring_px.width() as f32 / sf;
    window.set_ring_a(Image::from_rgba8(ring_px));
    window.set_ring_a_x(x0 / sf);
    window.set_ring_a_y(y0 / sf);
    window.set_ring_a_w(side);
    window.set_ring_a_h(side);

    // The dots: Zone draws each half an art px in from its orbit point.
    let dots = |ring: ground::Ring, dots: &[ground::Dot], drift: f32| -> ModelRc<ui::Dot> {
        ModelRc::new(VecModel::from(
            dots.iter()
                .map(|d| ui::Dot {
                    r: d.r * a,
                    ring_r: ring.r * a,
                    ring_cx: (ring.cx + 0.5) * a,
                    ring_cy: (ring.cy + 0.5) * a,
                    phase: d.phase,
                    turn: ring.turn,
                    drift,
                })
                .collect::<Vec<_>>(),
        ))
    };
    window.set_dots_a(dots(rings.a, &rings.a_dots, DRIFT_A_MS));
    window.set_dots_b(dots(rings.b, &rings.b_dots, DRIFT_B_MS));

    let prims: Vec<ui::Prim> = scene.prims.iter().map(to_ui).collect();
    window.set_prims(ModelRc::new(VecModel::from(prims)));
    // Bake the still page into one image: render it alone, on nothing,
    // once, and drop its ~600 items. Each frame then draws three images
    // and five small paths under the moving parts, instead of every glyph
    // copy and panel they touch (README, "Cost").
    if !flag("ZONE_SPIKE_LIVE") {
        window.set_baking(true);
        match window.window().take_snapshot() {
            Ok(page) => {
                window.set_page(Image::from_rgba8(page));
                window.set_prims(ModelRc::default());
            }
            Err(error) => {
                eprintln!("zone-spike: could not bake the page, drawing it live: {error}")
            }
        }
        window.set_baking(false);
    }
    let tris: Vec<ui::Tri> = scene
        .tris
        .iter()
        .map(|t| ui::Tri {
            edge: t.edge,
            cy: t.cy,
            w: t.w,
            h: t.h,
            near: t.near,
            far: t.far,
            side: t.side,
        })
        .collect();
    window.set_tris(ModelRc::new(VecModel::from(tris)));
}

/// ZONE_SPIKE_STATS=1: time to the first frame, then frames a second every
/// 5 s, on stderr. Frames are counted at winit's RedrawRequested, which
/// Slint answers with one software render.
fn install_stats(window: &ui::ZoneWindow, started: Instant) {
    use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
    let frames = Rc::new(Cell::new(0u64));
    let first = Rc::new(Cell::new(true));
    {
        let frames = frames.clone();
        window.window().on_winit_window_event(move |_, event| {
            if let winit::event::WindowEvent::RedrawRequested = event {
                frames.set(frames.get() + 1);
                if first.replace(false) {
                    let begun = started.elapsed();
                    // Runs once the event loop is back, after this frame is drawn.
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        eprintln!(
                            "zone-spike: first frame drawn {:.1} ms after start (draw began at {:.1} ms)",
                            started.elapsed().as_secs_f64() * 1000.0,
                            begun.as_secs_f64() * 1000.0
                        );
                    });
                }
            }
            EventResult::Propagate
        });
    }
    let timer = slint::Timer::default();
    let last = Cell::new((Instant::now(), 0u64));
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(5),
        move || {
            let (then, count) = last.get();
            let now = Instant::now();
            let total = frames.get();
            let fps = (total - count) as f64 / now.duration_since(then).as_secs_f64();
            eprintln!("zone-spike: {fps:.1} frames/s over the last 5 s ({total} frames in all)");
            last.set((now, total));
        },
    );
    std::mem::forget(timer);
}

fn main() -> Result<(), slint::PlatformError> {
    let started = Instant::now();
    let window = ui::ZoneWindow::new()?;
    let measure = Rc::new(text::Measure::new());

    let time_scale = std::env::var("ZONE_SPIKE_TIME_SCALE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.0);
    window.set_time_scale(time_scale);

    let (w, h) = size_from_env();
    let fullscreen = flag("ZONE_SPIKE_FULLSCREEN");
    window.window().set_size(LogicalSize::new(w, h));
    if fullscreen {
        window.window().set_fullscreen(true);
    }
    apply(&window, &measure, w, h);

    // Re-plan when the window's size changes (fullscreen, the compositor).
    let planned = Rc::new(RefCell::new((w, h)));
    {
        let weak = window.as_weak();
        let measure = measure.clone();
        window.on_resized(move |nw, nh| {
            let Some(window) = weak.upgrade() else { return };
            if nw < 1.0 || nh < 1.0 || *planned.borrow() == (nw, nh) {
                return;
            }
            *planned.borrow_mut() = (nw, nh);
            apply(&window, &measure, nw, nh);
        });
    }
    window.on_quit(|| {
        let _ = slint::quit_event_loop();
    });

    if flag("ZONE_SPIKE_STATS") {
        install_stats(&window, started);
    }
    window.run()
}
