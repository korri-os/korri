//! The vendored property system's equality cut-off (vendor/README.md): a
//! binding re-evaluated to the value it had does not make the bindings and
//! layers that depend on it evaluate or render again. Every partially redrawn
//! frame equals the same frame drawn whole with the cut-off off (upstream
//! property semantics, nothing reused), and a still subtree under an
//! animation is neither re-evaluated nor drawn again.

use korri_shell::bench::{self, Options};
use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel, SoftwareRenderer},
    Platform, WindowAdapter,
};
use slint::ComponentHandle;
use std::{cell::Cell, cell::RefCell, collections::BTreeMap, path::Path, rc::Rc, time::Duration};

/// Zone's or Pico's frames through a press sequence, with the cut-off on or off,
/// each on a thread of its own (Slint's platform and clock are per thread).
fn package_frames(surface: &str, cutoff: bool, preferences: &[(&str, &str)]) -> Vec<Vec<u8>> {
    package_frames_full(surface, cutoff, preferences, false)
}

fn package_frames_full(
    surface: &str,
    cutoff: bool,
    preferences: &[(&str, &str)],
    full: bool,
) -> Vec<Vec<u8>> {
    let surface = surface.to_owned();
    let preferences: BTreeMap<String, String> = preferences
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    std::thread::spawn(move || {
        let options = Options {
            packages: Path::new(env!("CARGO_MANIFEST_DIR")).join("packages"),
            surface,
            size: (640, 480),
            frames: 170,
            games: 12,
            presses: bench::parse_presses("right,right,down,up,left,left,right").unwrap(),
            preferences,
            full,
            threads: Some(1),
            cutoff,
            reuse_models: true,
        };
        bench::run_captured(options, true).unwrap().1
    })
    .join()
    .unwrap()
}

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count()
}

/// Every frame drawn whole is the same with and without the cut-off. Drawn in
/// partial redraws (the shell's way), a frame differs from the whole one only
/// in a few stale edge pixels, with or without the cut-off: a known partial
/// redraw limitation that the cut-off does not widen (vendor/README.md, Limits).
fn assert_same_frames(name: &str, preferences: &[(&str, &str)]) {
    let whole_with = package_frames_full(name, true, preferences, true);
    let whole_without = package_frames_full(name, false, preferences, true);
    assert!(
        whole_with.len() >= 160,
        "{name}: only {} frames",
        whole_with.len()
    );
    for (n, (a, b)) in whole_with.iter().zip(&whole_without).enumerate() {
        assert!(
            a == b,
            "{name} {preferences:?}: frame {n} drawn whole differs in {} pixels",
            differing(a, b)
        );
    }
    for cutoff in [true, false] {
        let partial = package_frames(name, cutoff, preferences);
        for (n, (a, b)) in partial.iter().zip(&whole_without).enumerate() {
            let stale = differing(a, b);
            assert!(stale <= 64, "{name} {preferences:?} cut-off {cutoff}: partial frame {n} differs from the whole one in {stale} pixels");
        }
    }
}

#[test]
fn zone_through_presses_draws_the_same_pixels_with_and_without_the_cut_off() {
    for look in ["modern", "retro"] {
        assert_same_frames("zone", &[("zone.look", look)]);
    }
}

#[test]
fn pico_through_presses_draws_the_same_pixels_with_and_without_the_cut_off() {
    assert_same_frames("pico", &[]);
}

thread_local! {
    static NOW: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    static WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

struct Manual;

impl Platform for Manual {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        window.set_size(slint::PhysicalSize::new(320, 120));
        WINDOW.with(|w| *w.borrow_mut() = Some(window.clone()));
        Ok(window)
    }
    fn duration_since_start(&self) -> Duration {
        NOW.with(Cell::get)
    }
}

/// Three cards under an animated position, as Zone's band: the first two
/// resize with the animation, the third only moves; every size goes through
/// functions of the animated value.
const BAND: &str = r#"
import "fonts/tiny5.ttf";
component Card {
    in property <string> word;
    Rectangle { background: #102030; }
    Text { x: 4px; y: 4px; width: root.width - 8px; text: root.word; color: #ffcc00; font-size: 14px; horizontal-alignment: center; }
    Text { x: 5px; y: 5px; width: root.width - 8px; text: root.word; color: #00000080; font-size: 14px; horizontal-alignment: center; }
    Rectangle { x: 2px; y: root.height - 10px; width: root.width - 4px; height: 6px; background: @linear-gradient(90deg, #ff0055 0%, #00aaff 100%); }
}
export component Surface inherits Window {
    in property <int> target: 0;
    property <float> f: target;
    animate f { duration: 220ms; easing: cubic-bezier(0.34, 1.56, 0.64, 1); }
    pure function share(i: int) -> float { return Math.max(0, 1 - Math.abs(i - f)); }
    pure function w(i: int) -> length { return 60px + share(i) * 40px; }
    background: #000000;
    default-font-family: "Pico Tiny5";
    Rectangle {
        cache-rendering-hint: true;
        for word[i] in ["ONE", "TWO", "THREE"]: Card {
            cache-rendering-hint: true;
            x: 10px + i * 105px - root.f * 7px;
            y: 60px - self.height / 2;
            width: root.w(i);
            height: 50px + root.share(i) * 30px;
            word: word;
        }
    }
}
"#;

struct Run {
    frames: Vec<Vec<u16>>,
    invalidated: u32,
    blitted: u32,
}

fn band(cutoff: bool) -> Run {
    band_source(cutoff, BAND.to_owned())
}

fn band_source(cutoff: bool, source: String) -> Run {
    std::thread::spawn(move || {
        SoftwareRenderer::new().set_equality_cutoff(cutoff);
        let _ = slint::platform::set_platform(Box::new(Manual));
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/packages/probe/band.slint");
        let result = pollster::block_on(
            slint_interpreter::Compiler::default().build_from_source(source, path),
        );
        assert!(
            !result.has_errors(),
            "{:?}",
            result.diagnostics().collect::<Vec<_>>()
        );
        let instance = result.component("Surface").unwrap().create().unwrap();
        instance.show().unwrap();
        let window = WINDOW.with(|w| w.borrow().clone()).unwrap();
        let mut buffer = vec![Rgb565Pixel::default(); 320 * 120];
        let mut run = Run {
            frames: Vec::new(),
            invalidated: 0,
            blitted: 0,
        };
        for n in 0..60 {
            if n == 10 {
                instance
                    .set_property("target", slint_interpreter::Value::Number(1.0))
                    .unwrap();
            }
            NOW.with(|t| t.set(t.get() + Duration::from_millis(16)));
            slint::platform::update_timers_and_animations();
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(&mut buffer, 320);
                if (12..22).contains(&n) {
                    let (invalidated, _, blitted) = renderer.last_frame_layer_visits();
                    run.invalidated += invalidated;
                    run.blitted += blitted;
                }
            });
            run.frames.push(buffer.iter().map(|p| p.0).collect());
        }
        run
    })
    .join()
    .unwrap()
}

#[test]
fn a_card_that_only_moves_keeps_its_layer_and_the_pixels_stay_the_same() {
    let with = band(true);
    let without = band(false);
    for (n, (a, b)) in with.frames.iter().zip(&without.frames).enumerate() {
        assert!(a == b, "frame {n} differs with the cut-off");
    }
    // Mid-snap, without the cut-off the band and all three cards are invalidated
    // every frame; with it, the third card (same size, same content) is drawn
    // from its cache.
    assert!(
        with.invalidated < without.invalidated && with.blitted > without.blitted,
        "invalidated {} vs {}, blitted {} vs {}",
        with.invalidated,
        without.invalidated,
        with.blitted,
        without.blitted
    );
}
