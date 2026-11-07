//! Zone's GAMES page stops drawing under the attract (ZoneGames hides its
//! shell while the attract covers the window), and nothing on screen
//! changes: the attract's entry, its turns, the next game and the wake are
//! drawn frame for frame as by a copy of the package that keeps the page
//! drawn underneath, in all three looks. Both run on the shell's manual
//! clock and render at the same instants.

use korri_shell::{
    backend::headless::Headless, catalog, engine, images::ImageCache, package, view,
};
use serde_json::{json, Value};
use slint::ComponentHandle;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

fn package_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/zone")
}

/// A copy of the package whose page keeps drawing under the attract.
fn reference() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("zone");
    copy(&package_dir(), &dir);
    let page = dir.join("pages/ZoneGames.slint");
    let source = std::fs::read_to_string(&page).unwrap();
    let hidden = "visible: !root.attract;";
    assert!(
        source.contains(hidden),
        "ZoneGames no longer hides its shell under the attract"
    );
    std::fs::write(&page, source.replace(hidden, "visible: true;")).unwrap();
    (temp, dir)
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// One package driven by the logic, drawn in its own window.
struct Run {
    engine: engine::Engine,
    instance: slint_interpreter::ComponentInstance,
    binder: view::Binder,
    images: ImageCache,
}

impl Run {
    fn new(dir: &Path, model: &Value, look: &str) -> Self {
        let views = package::compile(&package::Package::at(dir).unwrap()).unwrap();
        let source = std::fs::read_to_string(dir.join("logic.js")).unwrap();
        let mut engine = engine::Engine::load(
            &source,
            engine::Limits {
                stuck: Duration::from_secs(5),
                clock: korri_shell::budget_clock::BudgetClock::ThreadCpu,
                ..engine::Limits::default()
            },
        )
        .unwrap();
        engine.set_model(&model.to_string()).unwrap();
        let stored = BTreeMap::from([("zone.look".to_owned(), look.to_owned())]);
        engine
            .init(&engine::effective_preferences(
                engine.declaration(),
                &stored,
            ))
            .unwrap();
        let instance = views.surface.create().unwrap();
        instance
            .window()
            .set_size(slint::PhysicalSize::new(640, 480));
        instance.show().unwrap();
        let mut run = Self {
            engine,
            instance,
            binder: view::Binder::new(&views.surface),
            images: ImageCache::new(dir),
        };
        run.send(json!({"_tag": "Resized", "width": 640, "height": 480}));
        run.send(json!({"_tag": "KorriPublished"}));
        run
    }

    fn send(&mut self, message: Value) {
        self.engine.update(&message).unwrap();
        let view = self.engine.view().unwrap();
        self.binder
            .apply(&self.instance, &view, &mut self.images)
            .unwrap();
    }
}

/// Both windows at the clock's current instant: the reference's frame, then the change's.
fn frames(reference: &Run, changed: &Run) -> (Vec<u16>, Vec<u16>) {
    let mut out = Vec::new();
    for (shown, other) in [(reference, changed), (changed, reference)] {
        other.instance.hide().unwrap();
        shown.instance.show().unwrap();
        shown.instance.window().request_redraw();
        out.push(Headless::render().unwrap());
    }
    reference.instance.show().unwrap();
    let changed_frame = out.pop().unwrap();
    (out.pop().unwrap(), changed_frame)
}

#[test]
fn the_page_under_the_attract_stops_drawing_and_nothing_on_screen_changes() {
    Headless::install();
    Headless::manual_clock();
    let (_temp, reference_dir) = reference();
    let (mut model, examples) = korri_shell::verify::fixture();
    model["catalog"] = catalog::window(
        &catalog::games(&examples, 12),
        &catalog::default_query(),
        &json!({"_tag": "Start"}),
        catalog::DEFAULT_COUNT,
    );
    let mut different = Vec::new();
    let mut compared = 0;
    for look in ["modern", "blend", "retro"] {
        let mut before = Run::new(&reference_dir, &model, look);
        let mut after = Run::new(&package_dir(), &model, look);
        // (what to send, then the instants to render at, ms after it)
        let steps: [(Option<Value>, &[u64]); 5] = [
            (None, &[0, 16, 500]),
            (
                Some(json!({"_tag": "Tick", "idleMs": 30_000})),
                &[0, 16, 33, 160, 320, 1_000, 2_900, 3_000, 4_000, 5_600],
            ),
            (
                Some(json!({"_tag": "Tick", "idleMs": 36_000})),
                &[0, 16, 160, 320],
            ),
            (
                Some(json!({"_tag": "Pressed", "button": "right"})),
                &[0, 16, 33, 100, 220, 500],
            ),
            (Some(json!({"_tag": "Tick", "idleMs": 1_000})), &[0, 16]),
        ];
        for (index, (message, instants)) in steps.iter().enumerate() {
            if let Some(message) = message {
                before.send(message.clone());
                after.send(message.clone());
            }
            let mut at = 0;
            for &instant in *instants {
                Headless::advance(Duration::from_millis(instant - at));
                at = instant;
                let (a, b) = frames(&before, &after);
                compared += 1;
                let changed = a.iter().zip(&b).filter(|(x, y)| x != y).count();
                if changed > 0 && std::env::var_os("ZONE_ATTRACT_OUT").is_some() {
                    let dir = PathBuf::from(std::env::var_os("ZONE_ATTRACT_OUT").unwrap());
                    std::fs::create_dir_all(&dir).unwrap();
                    for (name, pixels) in [("before", &a), ("after", &b)] {
                        let mut img = image::RgbImage::new(640, 480);
                        for (i, p) in pixels.iter().enumerate() {
                            let (r, g, bl) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
                            img.put_pixel(
                                i as u32 % 640,
                                i as u32 / 640,
                                image::Rgb([(r << 3) as u8, (g << 2) as u8, (bl << 3) as u8]),
                            );
                        }
                        img.save(dir.join(format!("{look}-{index}-{instant}-{name}.png")))
                            .unwrap();
                    }
                }
                if changed > 0 {
                    different.push(format!(
                        "{look}, step {index}, +{instant} ms: {changed} px differ"
                    ));
                }
            }
            // A second later, drawn (as a device draws every frame), so the
            // next message lands on a clock both windows have seen.
            Headless::advance(Duration::from_millis(1_000));
            frames(&before, &after);
        }
    }
    assert!(compared >= 3 * 25, "only {compared} frames");
    assert!(different.is_empty(), "{different:#?}");
}
