//! The targeted dirty-region walk (vendored `PartialRenderer::visit`,
//! `SubtreeWalk`): a nested item tree (a repeater's row) whose last walk found
//! nothing and that nothing it read changed since is not walked, and drawing
//! skips a row whose items reach no dirty area. Through colour steps on the
//! animation clock, row data changes, moves, a clip and an opacity on the
//! rows' parent, rows added and removed, nested repeaters and a cached layer
//! whose child reaches outside it, every partial frame equals a window that
//! redraws everything; and a change in one row walks a fraction of the items.

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    Platform, WindowAdapter,
};
use slint::{ComponentHandle, Model, Rgb8Pixel, VecModel};
use slint_interpreter::{ComponentInstance, Struct, Value};
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
import "fonts/tiny5.ttf";
export struct Row { label: string, lit: bool, dots: int, cycling: bool, shift: length }
global Cycle {
    out property <int> phase: mod(floor(animation-tick() / 200ms), 3);
}
export component Surface inherits Window {
    in property <[Row]> rows;
    in property <length> offset: 0px;
    in property <length> clip-height: 400px;
    in property <float> fade: 1;
    background: #102030;
    default-font-family: "Pico Tiny5";
    Rectangle {
        y: 4px;
        height: root.clip-height;
        clip: true;
        Rectangle {
            opacity: root.fade;
            for row[i] in root.rows: Rectangle {
                x: 6px + row.shift;
                y: root.offset + i * 30px;
                width: 220px;
                height: 26px;
                background: row.lit ? #335577 : #223344;
                Text { x: 4px; y: 4px; text: row.label; color: #ffffff; font-size: 14px; }
                Rectangle {
                    x: 120px;
                    y: 6px;
                    width: 14px;
                    height: 14px;
                    background: !row.cycling ? #444444
                        : Cycle.phase == 0 ? #ff0044 : Cycle.phase == 1 ? #ff8800 : #ff66cc;
                }
                for d in row.dots: Rectangle {
                    x: 140px + d * 8px;
                    y: 10px;
                    width: 6px;
                    height: 6px;
                    background: #88ff88;
                }
                // A cached layer whose child reaches outside it, past the row's right edge.
                if i == 1: Rectangle {
                    x: 190px;
                    y: 2px;
                    width: 20px;
                    height: 20px;
                    cache-rendering-hint: true;
                    Rectangle { x: 12px; y: 4px; width: 40px; height: 12px; background: row.lit ? #ffcc00 : #cc00ff; }
                }
            }
        }
    }
}
"#;

fn row(label: &str, lit: bool, dots: i32, cycling: bool, shift: f64) -> Value {
    let mut s = Struct::default();
    s.set_field("label".into(), Value::String(label.into()));
    s.set_field("lit".into(), Value::Bool(lit));
    s.set_field("dots".into(), Value::Number(dots as f64));
    s.set_field("cycling".into(), Value::Bool(cycling));
    s.set_field("shift".into(), Value::Number(shift));
    Value::Struct(s)
}

/// One window: its instance, its window, its pixels, and its rows.
type Side = (
    ComponentInstance,
    Rc<MinimalSoftwareWindow>,
    Vec<Rgb8Pixel>,
    Rc<VecModel<Value>>,
);

struct Pair {
    windows: Vec<Side>,
    size: (usize, usize),
}

impl Pair {
    fn new() -> Self {
        let _ = slint::platform::set_platform(Box::new(Manual));
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/packages/probe/targeted.slint");
        let result = pollster::block_on(
            slint_interpreter::Compiler::default().build_from_source(SOURCE.into(), path),
        );
        assert!(
            !result.has_errors(),
            "{:?}",
            result.diagnostics().collect::<Vec<_>>()
        );
        let definition = result.component("Surface").unwrap();
        let size = (300usize, 260usize);
        let windows = [
            RepaintBufferType::ReusedBuffer,
            RepaintBufferType::NewBuffer,
        ]
        .into_iter()
        .map(|kind| {
            KIND.with(|k| k.set(kind));
            let instance = definition.create().unwrap();
            let rows = Rc::new(VecModel::from(vec![
                row("alpha", false, 2, false, 0.0),
                row("beta", true, 0, false, 0.0),
                row("gamma", false, 3, true, 0.0),
                row("delta", false, 1, false, 0.0),
                row("epsilon", false, 0, false, 0.0),
                row("zeta", false, 2, false, 0.0),
            ]));
            instance
                .set_property("rows", Value::Model(slint::ModelRc::from(rows.clone())))
                .unwrap();
            instance.show().unwrap();
            let window = WINDOWS.with(|w| w.borrow().last().unwrap().clone());
            window.set_size(slint::PhysicalSize::new(size.0 as u32, size.1 as u32));
            (
                instance,
                window,
                vec![Rgb8Pixel::default(); size.0 * size.1],
                rows,
            )
        })
        .collect();
        Self { windows, size }
    }

    fn each(&self, f: impl Fn(&ComponentInstance, &VecModel<Value>)) {
        for (instance, _, _, rows) in &self.windows {
            f(instance, rows);
        }
    }

    /// One frame, 16 ms on: returns the items the partial window walked.
    fn frame(&mut self, step: &str) -> usize {
        NOW.with(|n| n.set(n.get() + Duration::from_millis(16)));
        slint::platform::update_timers_and_animations();
        let mut walked = 0;
        for (i, (_, window, buffer, _)) in self.windows.iter_mut().enumerate() {
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(buffer, self.size.0);
                if i == 0 {
                    walked = renderer.dirty_region_walked_items();
                }
            });
        }
        let differing = self.windows[0]
            .2
            .iter()
            .zip(&self.windows[1].2)
            .filter(|(a, b)| a != b)
            .count();
        if differing > 0 {
            // Where they differ, for the failure message.
            let w = self.size.0;
            let at: Vec<(usize, usize)> = (0..self.windows[0].2.len())
                .filter(|&k| self.windows[0].2[k] != self.windows[1].2[k])
                .map(|k| (k % w, k / w))
                .collect();
            let x = (at.iter().map(|p| p.0).min(), at.iter().map(|p| p.0).max());
            let y = (at.iter().map(|p| p.1).min(), at.iter().map(|p| p.1).max());
            eprintln!("{step}: differing pixels within x {x:?}, y {y:?}");
        }
        assert_eq!(
            differing, 0,
            "{step}: {differing} pixels differ from a full redraw"
        );
        walked
    }

    fn frames(&mut self, step: &str, n: usize) -> Vec<usize> {
        (0..n)
            .map(|k| self.frame(&format!("{step}, frame {k}")))
            .collect()
    }
}

#[test]
fn trees_not_changed_are_not_walked_and_frames_match_full_redraws() {
    let mut pair = Pair::new();
    let first = pair.frame("first");
    pair.frames("settle", 3);

    // Row 2's colour steps on the animation clock. The first step walks every row (none
    // had a walk that found nothing yet); from the second on, only the window's own items
    // and the cycling row are walked.
    let walks = pair.frames("cycling", 40);
    let steps: Vec<usize> = walks.iter().copied().filter(|w| *w > 0).collect();
    assert!(steps.len() >= 3, "the colour stepped: {walks:?}");
    assert!(
        steps[1..].iter().all(|w| w * 2 < first),
        "later steps walk under half of the {first} items: {walks:?}"
    );

    // Row data: a label, the lit flag (into the cached layer's outside child), dots.
    pair.each(|_, rows| rows.set_row_data(0, row("ALPHA", false, 2, false, 0.0)));
    pair.frames("label", 3);
    pair.each(|_, rows| rows.set_row_data(1, row("beta", false, 0, false, 0.0)));
    pair.frames("layer child outside the layer", 4);
    pair.each(|_, rows| rows.set_row_data(3, row("delta", false, 5, false, 0.0)));
    pair.frames("dots added", 3);
    pair.each(|_, rows| rows.set_row_data(3, row("delta", false, 0, false, 0.0)));
    pair.frames("dots removed", 3);
    // One row moves sideways; then all of them move down.
    pair.each(|_, rows| rows.set_row_data(4, row("epsilon", false, 0, false, 13.5)));
    pair.frames("row shifted", 3);
    pair.each(|i, _| i.set_property("offset", Value::Number(9.0)).unwrap());
    pair.frames("rows moved", 3);
    // The clip above the rows shrinks and grows; the rows' parent fades.
    pair.each(|i, _| i.set_property("clip-height", Value::Number(70.0)).unwrap());
    pair.frames("clip shrunk", 3);
    pair.each(|i, _| i.set_property("clip-height", Value::Number(400.0)).unwrap());
    pair.frames("clip grown", 3);
    pair.each(|i, _| i.set_property("fade", Value::Number(0.5)).unwrap());
    pair.frames("faded", 3);
    pair.each(|i, _| i.set_property("fade", Value::Number(1.0)).unwrap());
    pair.frames("unfaded", 3);
    // Rows added and removed, around the cycling one.
    pair.each(|_, rows| rows.insert(1, row("new", true, 1, false, 0.0)));
    pair.frames("row inserted", 3);
    pair.each(|_, rows| {
        rows.remove(0);
    });
    pair.frames("row removed", 3);
    pair.each(|_, rows| rows.push(row("last", false, 4, true, 0.0)));
    pair.frames("row pushed, cycling too", 30);
    assert!(pair.windows[0].3.row_count() == 7);
}
