//! The shell end to end, without a display: real packages, the real korrid
//! wire against the fixture korrid, the software renderer in memory.
#![cfg(feature = "fixture")]

mod support;

use korri_shell::{
    backend::{headless::Headless, Backend, HostEvent, Placement},
    catalog,
    engine::Limits,
    fixture::{Behaviour, Fixture},
    fonts,
    input::Key,
    korrid::Korrid,
    messages::Button,
    notice::{Notice, Reason},
    package::Package,
    shell::{Event, Options, Post, Shell},
    verify,
};
use serde_json::{json, Value};
use slint_interpreter::Value as SlintValue;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Queue(Mutex<Vec<Event>>);

impl Post for Queue {
    fn post(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

struct Rig {
    shell: Shell,
    queue: Arc<Queue>,
    fixture: Fixture,
    _root: tempfile::TempDir,
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn source(id: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    if id == "probe" {
        root.join("tests/packages/probe")
    } else {
        root.join("packages").join(id)
    }
}

fn rig(ids: &[&str], games: usize, behaviour: Behaviour) -> Rig {
    let (_, examples) = verify::fixture();
    rig_with(ids, catalog::games(&examples, games), behaviour, |_| {})
}

/// `rig` over these games; `prepare` sets the fixture korrid up before the shell
/// loads its first surface.
fn rig_with(
    ids: &[&str],
    games: Vec<Value>,
    behaviour: Behaviour,
    prepare: impl FnOnce(&Fixture),
) -> Rig {
    let backend: Rc<dyn Backend> = Rc::new(Headless::install());
    let root = tempfile::tempdir().unwrap();
    let packages: Vec<Package> = ids
        .iter()
        .map(|id| {
            let dir = root.path().join(id);
            copy(&source(id), &dir);
            if *id == "probe" {
                static SPIN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
                let spin = SPIN.get_or_init(|| support::cpu_spin(120));
                let path = dir.join("logic.js");
                let source = std::fs::read_to_string(&path).unwrap().replace(
                    "const t0 = new Date().getTime(); while (new Date().getTime() - t0 < 120) {}",
                    spin,
                );
                std::fs::write(path, source).unwrap();
            }
            Package::at(&dir).unwrap()
        })
        .collect();
    let (model, _) = verify::fixture();
    let fixture = Fixture::start(model, games, behaviour).unwrap();
    prepare(&fixture);
    let queue = Arc::new(Queue::default());
    let options = Options {
        limits: Limits {
            clock: korri_shell::budget_clock::BudgetClock::ThreadCpu,
            ..Limits::default()
        },
        placement: Placement::Windowed {
            width: 640,
            height: 480,
        },
        stats: false,
    };
    let mut shell = Shell::new(
        packages,
        Korrid::new(fixture.connection()),
        backend,
        queue.clone(),
        options,
    );
    shell.start().unwrap();
    let mut rig = Rig {
        shell,
        queue,
        fixture,
        _root: root,
    };
    rig.settle();
    rig
}

impl Rig {
    /// Deliver queued events and draw, until `test` holds.
    fn until(&mut self, what: &str, mut test: impl FnMut(&Rig) -> bool) {
        let started = Instant::now();
        loop {
            Headless::render();
            let events: Vec<Event> = std::mem::take(&mut *self.queue.0.lock().unwrap());
            for event in events {
                self.shell.handle(event);
            }
            if test(self) {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "timed out waiting for {what}; log: {:?}",
                self.shell.log()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Run until nothing is queued for a short while.
    fn settle(&mut self) {
        let mut quiet = 0;
        while quiet < 10 {
            Headless::render();
            let events: Vec<Event> = std::mem::take(&mut *self.queue.0.lock().unwrap());
            if events.is_empty() {
                quiet += 1;
            } else {
                quiet = 0;
            }
            for event in events {
                self.shell.handle(event);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn key(&mut self, key: Key) {
        for pressed in [true, false] {
            self.shell.handle(Event::Host {
                generation: 0,
                window: 0,
                event: HostEvent::Key { key, pressed },
            });
        }
        self.settle();
    }

    fn int(&self, name: &str) -> f64 {
        match self.shell.surface_property(name) {
            Some(SlintValue::Number(n)) => n,
            other => panic!("{name}: {other:?}"),
        }
    }

    /// The width of an image property: 0 while it is empty.
    fn image_width(&self, name: &str) -> u32 {
        match self.shell.surface_property(name) {
            Some(SlintValue::Image(image)) => image.size().width,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn string(&self, name: &str) -> String {
        match self.shell.surface_property(name) {
            Some(SlintValue::String(s)) => s.to_string(),
            other => panic!("{name}: {other:?}"),
        }
    }

    fn notice_string(&self, name: &str) -> String {
        match self.shell.notice_property(name) {
            Some(SlintValue::String(s)) => s.to_string(),
            other => panic!("{name}: {other:?}"),
        }
    }

    fn notice_int(&self, name: &str) -> f64 {
        match self.shell.notice_property(name) {
            Some(SlintValue::Number(n)) => n,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn requests(&self, tag: &str) -> Vec<Value> {
        self.fixture
            .requests()
            .into_iter()
            .filter(|(_, r)| r["_tag"] == tag)
            .map(|(_, r)| r)
            .collect()
    }
}

/// The asset id of a stored cover.
fn cover_id(fill: char) -> String {
    format!("{}.png", fill.to_string().repeat(64))
}

/// How korrid's model names a stored cover (`SurfaceGame.coverArtUrl`).
fn cover_url(asset_id: &str) -> String {
    format!("/game-assets/{asset_id}")
}

/// A PNG `width` pixels wide and one high.
fn png(width: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(width, 1, image::Rgba([0x2b, 0xac, 0xfd, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

fn game_with_cover(id: &str, cover: &str) -> Value {
    json!({"id": id, "title": id, "coverArtUrl": cover})
}

#[test]
fn a_cover_korrid_stores_is_fetched_with_the_capability_and_shown_when_it_arrives() {
    let (stored, missing) = (cover_id('a'), cover_id('b'));
    let mut rig = rig_with(
        &["probe"],
        vec![
            game_with_cover("first", &cover_url(&stored)),
            game_with_cover("second", &cover_url(&missing)),
        ],
        Behaviour::default(),
        |fixture| fixture.set_asset(&stored, png(3)),
    );
    // The surface is up before any cover: the view never waits for an image.
    assert_eq!(rig.shell.current_id(), Some("probe"));
    rig.until("the stored cover", |rig| rig.image_width("art") == 3);
    rig.until("both answers", |rig| {
        rig.fixture.asset_requests().len() == 2 && !rig.shell.log().is_empty()
    });
    rig.settle();

    // The cover korrid does not have stays empty, and the surface stays loaded.
    assert_eq!(rig.image_width("second-art"), 0);
    assert_eq!(rig.shell.current_id(), Some("probe"));
    assert!(rig.shell.notice().is_none());
    let failures: Vec<&String> = rig
        .shell
        .log()
        .iter()
        .filter(|line| line.contains("image /game-assets/"))
        .collect();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].contains(&missing) && failures[0].contains("HTTP 404"));

    // Later views name the same covers: each was asked for once, by its asset id.
    rig.key(Key::Right);
    rig.key(Key::Right);
    assert_eq!(rig.int("count"), 2.0);
    assert_eq!(rig.image_width("art"), 3);
    let mut asked = rig.fixture.asset_requests();
    asked.sort();
    assert_eq!(asked, vec![stored, missing]);
}

#[test]
fn loads_declares_reads_preferences_and_shows_the_view() {
    let rig = rig(&["probe"], 12, Behaviour::default());
    assert_eq!(rig.shell.current_id(), Some("probe"));
    let calls = rig.fixture.calls();
    assert_eq!(
        &calls[..3],
        &[
            "app.surface.declare",
            "app.surface.preferences",
            "app.surface.model"
        ]
    );
    assert_eq!(rig.string("word"), "first");
    assert_eq!(rig.int("total"), 12.0);
    // The watch delivered the model too, and the shell acked it.
    assert!(!rig.fixture.acks().is_empty());
    assert_eq!(Headless::live_windows(), 1);
}

#[test]
fn keyboard_presses_reach_update_and_repeat_when_held() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    rig.key(Key::Right);
    rig.key(Key::Right);
    assert_eq!(rig.int("count"), 2.0);
    rig.shell.handle(Event::Host {
        generation: 0,
        window: 0,
        event: HostEvent::Key {
            key: Key::Left,
            pressed: true,
        },
    });
    std::thread::sleep(Duration::from_millis(560));
    rig.until("repeats", |r| r.int("count") <= 0.0);
    rig.shell.handle(Event::Host {
        generation: 0,
        window: 0,
        event: HostEvent::Key {
            key: Key::Left,
            pressed: false,
        },
    });
    rig.settle();
    let count = rig.int("count");
    // One press plus at least one repeat after 400 ms, then every 100 ms.
    assert!((-4.0..=0.0).contains(&count), "{count}");
}

#[test]
fn preferences_round_trip_through_korrid() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    rig.key(Key::O);
    rig.until("PreferenceStored", |r| {
        matches!(
            r.shell.surface_property("stored"),
            Some(SlintValue::Bool(true))
        )
    });
    assert_eq!(
        rig.fixture
            .stored("probe")
            .get("probe.word")
            .map(String::as_str),
        Some("second")
    );
    // A fresh load gets the stored value in init.
    let backend: Rc<dyn Backend> = Rc::new(Headless::install());
    let package = Package::at(&rig._root.path().join("probe")).unwrap();
    let mut again = Shell::new(
        vec![package],
        Korrid::new(rig.fixture.connection()),
        backend,
        rig.queue.clone(),
        Options::default(),
    );
    again.start().unwrap();
    assert!(
        matches!(again.surface_property("word"), Some(SlintValue::String(s)) if s.as_str() == "second")
    );
}

#[test]
fn a_refused_declaration_keeps_the_current_surface() {
    let behaviour = Behaviour {
        refuse_declare: vec!["pico".into()],
        ..Default::default()
    };
    let mut rig = rig(&["probe", "pico"], 12, behaviour);
    rig.until("probe placed", |r| r.fixture.declarations().len() == 2);
    rig.key(Key::F2);
    assert_eq!(rig.shell.current_id(), Some("probe"));
    // The failed switch declared pico; probe is declared again so korrid
    // puts its displays back.
    let ids: Vec<String> = rig
        .fixture
        .declarations()
        .iter()
        .map(|d| d["surfaceId"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, ["probe", "probe", "pico", "probe"]);
    assert!(
        rig.shell
            .log()
            .iter()
            .any(|l| l.contains("pico not loaded") && l.contains("app.surface.declare")),
        "{:?}",
        rig.shell.log()
    );
}

#[test]
fn model_pushes_run_korri_published_and_are_acked_in_order() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    rig.key(Key::Down); // MoveCatalog End
    rig.until("new window", |r| {
        r.fixture
            .requests()
            .iter()
            .any(|(_, q)| q["_tag"] == "MoveCatalog")
    });
    let before = rig.fixture.acks().len();
    rig.fixture
        .publish(|model| model["clockLabel"] = json!("12:34"));
    rig.until("ack", |r| r.fixture.acks().len() > before);
    let acks = rig.fixture.acks();
    let revisions: Vec<u64> = acks.iter().map(|(_, r)| r.parse().unwrap()).collect();
    assert!(revisions.windows(2).all(|w| w[0] < w[1]), "{revisions:?}");
}

#[test]
fn declare_carries_the_declared_displays_and_only_all_gets_model_displays() {
    let mut rig = rig(&["pico", "zone", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("zone", |r| r.shell.current_id() == Some("zone"));
    rig.key(Key::F2);
    rig.until("probe", |r| r.shell.current_id() == Some("probe"));
    rig.until("probe placed", |r| r.fixture.declarations().len() == 6);
    let displays: Vec<(String, Option<Value>)> = rig
        .fixture
        .declarations()
        .iter()
        .map(|d| {
            (
                d["surfaceId"].as_str().unwrap().to_owned(),
                d.get("displays").cloned(),
            )
        })
        .collect();
    assert_eq!(
        displays,
        vec![
            // Each surface declares before its preferences, then once more
            // after its first frame, when its window is the only one.
            ("pico".into(), Some(json!("all"))),
            ("pico".into(), Some(json!("all"))),
            ("zone".into(), Some(json!("main"))),
            ("zone".into(), Some(json!("main"))),
            ("probe".into(), None),
            ("probe".into(), None),
        ]
    );
    let declarations = rig.fixture.declarations();
    assert_eq!(
        declarations[2], declarations[3],
        "the second declare repeats the first"
    );
    assert!(rig.fixture.model("pico").get("displays").is_some());
    assert!(rig.fixture.model("zone").get("displays").is_none());
    assert!(rig.fixture.model("probe").get("displays").is_none());
}

// All three viewer exits close the hosted Surface and publish catalog. Check
// both Android callback orders with the same controller ID across the boundary.
fn catalog_receives_android_pressed_after_overlay_exit(focus_first: bool, model_first: bool) {
    use korri_shell::input::android::AndroidInput;
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    let mut source = AndroidInput::default();
    rig.fixture.set_overlay(true);
    rig.until("hosted overlay", |r| r.shell.overlay_shown());
    rig.shell.overlay_focus(true);
    for pressed in [true, false] {
        rig.shell
            .handle(Event::Native(source.key(7, 22, pressed).unwrap()));
    }
    rig.until("overlay receives Pressed right", |r| r.int("count") == 1.0);

    let main_focus = |rig: &mut Rig| {
        rig.shell.handle(Event::Host {
            generation: 0,
            window: 0,
            event: HostEvent::Focus(true),
        })
    };
    if focus_first {
        main_focus(&mut rig);
        source.set_active(true);
    }
    if model_first {
        rig.fixture.set_overlay(false);
        rig.until("exit publishes catalog", |r| !r.shell.overlay_shown());
    }
    rig.shell.detach_overlay_window();
    // The same handoff used by the Android Surface-release callback.
    source.reset(focus_first);
    if !model_first {
        rig.fixture.set_overlay(false);
    }
    if !focus_first {
        main_focus(&mut rig);
        source.set_active(true);
    }
    rig.settle();
    assert!(!rig.shell.overlay_shown());
    assert_eq!(Headless::live_windows(), 1);
    for pressed in [true, false] {
        rig.shell
            .handle(Event::Native(source.key(7, 22, pressed).unwrap()));
    }
    rig.until("catalog receives Pressed right after viewer exit", |r| {
        r.int("count") == 2.0
    });
    // Stick samples from that same device work too, not only digital keys.
    rig.shell
        .handle(Event::Native(source.axes(7, -0.8, 0.0, 0.0, 0.0)));
    rig.shell
        .handle(Event::Native(source.axes(7, 0.0, 0.0, 0.0, 0.0)));
    rig.until("catalog receives Pressed left after viewer exit", |r| {
        r.int("count") == 1.0
    });
    assert!(!rig.shell.overlay_shown());
}

#[test]
fn disconnect_hands_controller_pressed_back_to_the_catalog() {
    catalog_receives_android_pressed_after_overlay_exit(true, true);
}

#[test]
fn end_hands_controller_pressed_back_to_the_catalog() {
    catalog_receives_android_pressed_after_overlay_exit(false, true);
}

#[test]
fn open_korri_hands_controller_pressed_back_to_the_catalog() {
    catalog_receives_android_pressed_after_overlay_exit(true, false);
}

#[test]
fn the_overlay_follows_the_presentation() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    assert!(!rig.shell.overlay_shown());
    rig.until("surface placed", |r| r.fixture.declarations().len() == 2);
    rig.fixture.set_overlay(true);
    rig.until("overlay", |r| r.shell.overlay_shown());
    // Every newly mapped window is placed: the Overlay's first frame declares again.
    rig.until("overlay placed", |r| r.fixture.declarations().len() == 3);
    assert!(
        matches!(rig.shell.overlay_property("heading"), Some(SlintValue::String(s)) if s.as_str() == "Paused")
    );
    // One mapped window: korrid places the one window its shell unit owns.
    assert_eq!(Headless::live_windows(), 1);
    rig.key(Key::Escape); // back: DismissGameplayOverlay
    rig.until("overlay gone", |r| !r.shell.overlay_shown());
    assert_eq!(rig.requests("DismissGameplayOverlay").len(), 1);
    assert_eq!(Headless::live_windows(), 1);
    // The Surface mapped again is placed again.
    rig.until("surface placed again", |r| {
        r.fixture.declarations().len() == 4
    });
    rig.settle();
    assert_eq!(
        rig.fixture.declarations().len(),
        4,
        "one declare per mapping"
    );
}

#[test]
fn switching_replaces_the_surface_after_its_first_frame_and_fonts_do_not_pile_up() {
    let mut rig = rig(&["pico", "zone"], 12, Behaviour::default());
    assert_eq!(rig.shell.current_id(), Some("pico"));
    assert_eq!(fonts::registered_faces("Pico Tiny5"), 1);
    rig.key(Key::F2);
    rig.until("zone shown, pico retired", |r| {
        r.shell.current_id() == Some("zone") && r.shell.retiring() == 0
    });
    assert_eq!(Headless::live_windows(), 1);
    for _ in 0..6 {
        rig.key(Key::F2);
        rig.until("switched", |r| r.shell.retiring() == 0);
    }
    assert_eq!(rig.shell.current_id(), Some("zone"));
    // Each package compiled once: later loads reuse its views.
    assert_eq!(rig.shell.compilations(), 2);
    // Each font file is registered once, however often its surface loads.
    assert_eq!(fonts::registered_faces("Pico Tiny5"), 1);
    assert_eq!(fonts::registered_faces("Anton"), 1);
}

/// Amendment A5: a call over the 50 ms budget is a performance defect, logged once per
/// surface and message type; the surface keeps running.
#[test]
fn a_slow_call_keeps_the_surface_and_is_logged_once() {
    let mut rig = rig(&["pico", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("probe", |r| {
        r.shell.current_id() == Some("probe") && r.shell.retiring() == 0
    });
    for n in 1..=3 {
        rig.shell.press(Button::Up); // probe spins 120 ms on up, then counts 10
        rig.until("slow press applied", |r| {
            r.int("count") == f64::from(n * 10)
        });
    }
    assert_eq!(rig.shell.current_id(), Some("probe"));
    assert!(rig.shell.notice().is_none());
    let log = rig.shell.log();
    let defects: Vec<&String> = log
        .iter()
        .filter(|l| l.starts_with("performance defect: probe update for Pressed"))
        .collect();
    assert_eq!(defects.len(), 1, "rate-limited: {log:?}");
    assert!(defects[0].contains("over the 50 ms call budget"), "{log:?}");
    assert!(!log.iter().any(|l| l.contains("unloaded")), "{log:?}");
}

/// Amendment A5: a call still running after 2 s unloads the surface, and the shell shows
/// its notice instead of switching. Nothing switches until the person chooses.
#[test]
fn a_stuck_call_unloads_at_two_seconds_and_shows_the_notice() {
    let mut rig = rig(&["pico", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("probe", |r| {
        r.shell.current_id() == Some("probe") && r.shell.retiring() == 0
    });
    let started = Instant::now();
    rig.shell.press(Button::Confirm); // probe loops forever on confirm
    rig.until("notice", |r| r.shell.notice().is_some());
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(2) && elapsed < Duration::from_secs(4),
        "{elapsed:?}"
    );
    let notice = rig.shell.notice().unwrap().clone();
    assert_eq!(notice.reason, Reason::Stuck);
    assert_eq!(notice.heading(), "Probe stopped responding");
    assert_eq!(labels(&notice), ["Try Probe again", "Use Pico"]);
    assert_eq!(rig.notice_string("heading"), "Probe stopped responding");
    assert!(rig
        .shell
        .log()
        .iter()
        .any(|l| l.contains("probe unloaded (time)")));
    // No switch without a choice: time passes, ticks come, L1+R1 is ignored.
    rig.key(Key::F2);
    std::thread::sleep(Duration::from_millis(1_200));
    rig.settle();
    assert_eq!(rig.shell.current_id(), None);
    assert!(rig.shell.notice().is_some());
    assert_eq!(Headless::live_windows(), 1, "the notice alone");
    // Try again: probe loads afresh.
    rig.shell.press(Button::Confirm);
    rig.until("probe again", |r| {
        r.shell.current_id() == Some("probe") && r.shell.notice().is_none()
    });
    rig.settle();
    assert_eq!(rig.int("count"), 0.0);
    assert_eq!(
        Headless::live_windows(),
        1,
        "the notice went after the first frame"
    );
}

/// Amendment A5: a throw shows the notice; down and confirm use the other surface.
#[test]
fn a_crash_shows_the_notice_and_the_choice_switches() {
    let mut rig = rig(&["pico", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("probe", |r| {
        r.shell.current_id() == Some("probe") && r.shell.retiring() == 0
    });
    rig.fixture
        .publish(|model| model["clockLabel"] = json!("crash"));
    rig.until("notice", |r| r.shell.notice().is_some());
    let notice = rig.shell.notice().unwrap().clone();
    assert_eq!(notice.reason, Reason::Error);
    assert_eq!(notice.heading(), "Probe stopped because of an error");
    assert!(rig
        .shell
        .log()
        .iter()
        .any(|l| l.contains("probe unloaded (exception)") && l.contains("planted crash")));
    rig.shell.press(Button::Down);
    assert_eq!(rig.shell.notice().unwrap().focused, 1);
    assert_eq!(rig.notice_int("focused"), 1.0);
    assert_eq!(
        rig.shell.current_id(),
        None,
        "moving the focus chooses nothing"
    );
    rig.shell.press(Button::Confirm);
    rig.until("pico", |r| {
        r.shell.current_id() == Some("pico") && r.shell.notice().is_none()
    });
    rig.settle();
    assert_eq!(Headless::live_windows(), 1);
}

#[test]
fn back_on_the_notice_uses_the_other_surface() {
    let mut rig = rig(&["pico", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("probe", |r| r.shell.current_id() == Some("probe"));
    rig.key(Key::M); // probe allocates without end on menu
    rig.until("notice", |r| r.shell.notice().is_some());
    assert_eq!(rig.shell.notice().unwrap().reason, Reason::Memory);
    assert!(rig
        .shell
        .log()
        .iter()
        .any(|l| l.contains("probe unloaded (memory)")));
    rig.shell.press(Button::Back);
    rig.until("pico", |r| r.shell.current_id() == Some("pico"));
}

/// With no other surface the notice offers "try again" only, and back does nothing.
#[test]
fn with_one_surface_the_notice_offers_try_again_only() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    rig.shell.press(Button::System); // an unknown request
    rig.until("notice", |r| r.shell.notice().is_some());
    let notice = rig.shell.notice().unwrap().clone();
    assert_eq!(labels(&notice), ["Try Probe again"]);
    assert!(rig
        .shell
        .log()
        .iter()
        .any(|l| l.contains("probe unloaded (contract)")));
    rig.shell.press(Button::Back);
    rig.shell.press(Button::Down);
    rig.settle();
    assert_eq!(rig.shell.current_id(), None);
    rig.shell.press(Button::Confirm);
    rig.until("probe again", |r| r.shell.current_id() == Some("probe"));
}

fn labels(notice: &Notice) -> Vec<String> {
    notice
        .actions()
        .into_iter()
        .map(|(label, _)| label)
        .collect()
}

/// The logic runs on its own thread: a press whose call never returns does not hold up
/// the UI thread, and switching away while it runs drops that logic (its call is
/// interrupted, its thread joined) without waiting for its budget.
#[test]
fn a_switch_while_the_logic_runs_drops_that_logic() {
    let mut rig = rig(&["pico", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("probe", |r| {
        r.shell.current_id() == Some("probe") && r.shell.retiring() == 0
    });
    let started = Instant::now();
    rig.shell.press(Button::Confirm); // probe loops forever on confirm
    assert!(
        started.elapsed() < Duration::from_millis(20),
        "the press waited on logic"
    );
    rig.shell.switch();
    rig.until("pico after the switch", |r| {
        r.shell.current_id() == Some("pico") && r.shell.retiring() == 0
    });
    assert!(started.elapsed() < Duration::from_secs(2));
    rig.settle();
    assert_eq!(rig.shell.current_id(), Some("pico"));
    assert!(
        !rig.shell.log().iter().any(|l| l.contains("unloaded")),
        "the retired logic's outcome is ignored: {:?}",
        rig.shell.log()
    );
}

#[test]
fn native_gamepad_input_moves_and_l1_r1_switches() {
    let mut rig = rig(&["probe", "pico"], 12, Behaviour::default());
    let connection = rig.fixture.connection();
    let queue = rig.queue.clone();
    let _input = korri_shell::input::native::NativeInput::start(
        connection,
        true,
        Box::new(move |event| queue.post(Event::Native(event))),
    );
    rig.until("subscribed", |r| r.fixture.input_subscribers() == 1);
    rig.fixture.send_input(&Fixture::gamepad_frames("pad"));
    rig.settle();
    rig.fixture.send_input(&Fixture::press_frames("pad", 0x223)); // D-pad right
    rig.until("moved", |r| r.int("count") == 1.0);
    let key = |code: u16, value: u8| json!({"kind": "input", "deviceId": "pad", "class": "gamepad", "type": 1, "code": code, "value": value, "timestamp": 0});
    let report = json!({"kind": "input", "deviceId": "pad", "class": "gamepad", "type": 0, "code": 0, "value": 0, "timestamp": 0});
    rig.fixture.send_input(&[
        key(0x136, 1),
        report.clone(),
        key(0x137, 1),
        report.clone(),
        key(0x136, 0),
        key(0x137, 0),
        report,
    ]);
    rig.until("switched by L1+R1", |r| {
        r.shell.current_id() == Some("pico")
    });
}

#[test]
fn zone_pages_a_10000_game_catalog_and_keeps_its_focus() {
    let mut rig = rig(&["zone"], 10_000, Behaviour::default());
    assert_eq!(rig.int("total"), 10_000.0);
    // A person moves one card at a time; korrid answers between presses.
    for _ in 0..60 {
        rig.shell.press(Button::Right);
        rig.settle();
    }
    assert!(rig.int("first-index") > 0.0, "the window moved");
    let focus = rig.int("first-index") + rig.int("focused-card");
    assert_eq!(
        focus, 60.0,
        "60 presses from the first game (Zone's start) reach the 61st"
    );
    let moves = rig.requests("MoveCatalog");
    assert!(
        !moves.is_empty()
            && moves
                .iter()
                .all(|m| m["from"]["_tag"] == "Around" && m["count"] == 50)
    );
    let focus_reports = rig.requests("FocusGame");
    assert_eq!(
        focus_reports.last().unwrap()["gameId"],
        json!(catalog::games(&verify::fixture().1, 10_000)[60]["id"])
    );
}

#[test]
fn ticks_arrive_each_second_with_idle_time_that_a_press_and_a_switch_reset() {
    let mut rig = rig(&["probe", "pico"], 12, Behaviour::default());
    // Amendment A1: once a second, the time since the last press or load.
    rig.until("first tick", |r| r.int("ticks") >= 1.0);
    let first = rig.int("idle-ms");
    assert!((900.0..2_000.0).contains(&first), "{first}");
    rig.until("second tick", |r| r.int("ticks") >= 2.0);
    assert!(
        rig.int("idle-ms") >= first + 900.0,
        "idle grows without presses"
    );
    // A press resets the count.
    rig.key(Key::Right);
    assert_eq!(rig.int("count"), 1.0);
    let ticks = rig.int("ticks");
    rig.until("tick after the press", |r| r.int("ticks") > ticks);
    // Ticks come once a second; a loaded machine delivers them late (slack, not exactness).
    assert!(rig.int("idle-ms") < SLACK_MS, "{}", rig.int("idle-ms"));
    // A switch away and back loads the probe again: its count starts over.
    rig.until("idle again", |r| r.int("idle-ms") >= 1_800.0);
    rig.key(Key::F2);
    rig.until("pico", |r| r.shell.current_id() == Some("pico"));
    rig.key(Key::F2);
    rig.until("probe again", |r| r.shell.current_id() == Some("probe"));
    // The reloaded probe counts from zero: a loaded machine may already have delivered
    // its first tick, but not more, and that tick's idle time starts at the load.
    assert!(
        rig.int("ticks") <= 1.0,
        "a fresh load starts its count over"
    );
    rig.until("first tick after the switch", |r| r.int("ticks") >= 1.0);
    assert!(rig.int("idle-ms") < SLACK_MS, "{}", rig.int("idle-ms"));
}

/// The largest idle time a tick right after a press or load may report: one second, and
/// what a loaded machine adds to delivering it.
const SLACK_MS: f64 = 1_800.0;

#[test]
fn resized_comes_before_the_first_view_and_follows_every_window_change() {
    let mut rig = rig(&["probe", "pico"], 12, Behaviour::default());
    // A2 (8.1): one Resized right after init, before the first view.
    assert_eq!(rig.int("views-before-first-resize"), 0.0);
    assert_eq!(rig.int("resizes"), 1.0);
    assert_eq!(
        (rig.int("width-seen"), rig.int("height-seen")),
        (640.0, 480.0)
    );
    rig.shell.resize_window(800, 600);
    rig.settle();
    assert_eq!(
        (rig.int("width-seen"), rig.int("height-seen")),
        (800.0, 600.0)
    );
    assert_eq!(rig.int("resizes"), 2.0);
    // The same size again sends nothing.
    rig.shell.resize_window(800, 600);
    rig.settle();
    assert_eq!(rig.int("resizes"), 2.0);
    // A switch loads again: its first Resized again precedes its first view.
    rig.key(Key::F2);
    rig.until("pico", |r| r.shell.current_id() == Some("pico"));
    rig.key(Key::F2);
    rig.until("probe again", |r| r.shell.current_id() == Some("probe"));
    assert_eq!(rig.int("resizes"), 1.0);
    assert_eq!(rig.int("views-before-first-resize"), 0.0);
}

#[test]
fn native_pointer_events_reset_idle_without_cursor_presses() {
    use i_slint_core::input::TouchPhase;
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    for (phase, x, y) in [
        (TouchPhase::Started, 400.0, 400.0),
        (TouchPhase::Moved, 300.0, 200.0),
        (TouchPhase::Ended, 300.0, 200.0),
        (TouchPhase::Started, 400.0, 400.0),
        (TouchPhase::Cancelled, 300.0, 200.0),
    ] {
        std::thread::sleep(Duration::from_millis(300));
        rig.shell.handle(Event::Idle);
        rig.settle();
        assert!(rig.int("idle-ms") >= 300.0);
        Headless::touch(0, slint::LogicalPosition::new(x, y), phase);
        // Deliver the queued pointer activity before asking for the tick.
        for event in std::mem::take(&mut *rig.queue.0.lock().unwrap()) {
            rig.shell.handle(event);
        }
        rig.shell.handle(Event::Idle);
        rig.settle();
        assert!(rig.int("idle-ms") < 250.0, "{}", rig.int("idle-ms"));
        assert_eq!(
            rig.int("presses"),
            0.0,
            "a drag must not synthesize any Pressed, including up/down"
        );
        assert_eq!(rig.int("count"), 0.0);
    }
}

#[test]
fn taps_on_both_package_roots_reach_logic_as_activated() {
    use i_slint_core::input::TouchPhase;
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    let tap = |rig: &mut Rig| {
        for phase in [TouchPhase::Started, TouchPhase::Ended] {
            Headless::touch(0, slint::LogicalPosition::new(80.0, 60.0), phase);
        }
        rig.settle();
    };
    tap(&mut rig);
    assert_eq!(
        serde_json::from_str::<Value>(&rig.string("activation")).unwrap(),
        json!({"_tag": "Activated", "target": "probe:tap"})
    );
    rig.fixture.set_overlay(true);
    rig.until("overlay", |r| r.shell.overlay_shown());
    // A different callback value proves that Overlay, not the hidden Surface, delivered it.
    tap(&mut rig);
    assert!(
        matches!(rig.shell.overlay_property("activation"), Some(SlintValue::String(s))
        if serde_json::from_str::<Value>(&s).unwrap() == json!({"_tag": "Activated", "target": "probe:overlay"}))
    );
    assert_eq!(rig.int("count"), 0.0);
}

#[test]
fn touch_outside_the_overlay_panel_and_cancel_activate_nothing() {
    use i_slint_core::input::TouchPhase;
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    rig.fixture.set_overlay(true);
    rig.until("overlay", |r| r.shell.overlay_shown());
    for (x, y, finish) in [
        (300.0, 300.0, TouchPhase::Ended),
        (80.0, 60.0, TouchPhase::Cancelled),
    ] {
        Headless::touch(0, slint::LogicalPosition::new(x, y), TouchPhase::Started);
        Headless::touch(0, slint::LogicalPosition::new(x, y), finish);
        rig.settle();
        assert!(
            matches!(rig.shell.overlay_property("activation"), Some(SlintValue::String(s)) if s.is_empty())
        );
        assert!(rig.shell.overlay_shown());
        assert_eq!(rig.int("count"), 0.0);
    }
}

#[test]
fn package_row_drags_on_both_roots_reach_logic_without_pressed() {
    use i_slint_core::input::TouchPhase;
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    let drag = |rig: &mut Rig| {
        Headless::touch(
            0,
            slint::LogicalPosition::new(240.0, 60.0),
            TouchPhase::Started,
        );
        Headless::touch(
            0,
            slint::LogicalPosition::new(220.0, 60.0),
            TouchPhase::Moved,
        );
        Headless::touch(
            0,
            slint::LogicalPosition::new(220.0, 60.0),
            TouchPhase::Ended,
        );
        rig.settle();
    };
    drag(&mut rig);
    assert_eq!(
        serde_json::from_str::<Value>(&rig.string("drag")).unwrap(),
        json!({"_tag": "Dragged", "region": "surface-shelf", "by": -2})
    );
    rig.fixture.set_overlay(true);
    rig.until("overlay", |r| r.shell.overlay_shown());
    drag(&mut rig);
    assert!(
        matches!(rig.shell.overlay_property("drag"), Some(SlintValue::String(s))
        if serde_json::from_str::<Value>(&s).unwrap() == json!({"_tag": "Dragged", "region": "overlay-shelf", "by": -2}))
    );
    assert_eq!(rig.int("presses"), 0.0);
    assert_eq!(rig.int("count"), 0.0);
}

#[test]
fn touched_is_once_per_gesture_not_per_finger_and_survives_cancel() {
    use i_slint_core::input::TouchPhase;
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    let at = slint::LogicalPosition::new(500.0, 400.0);
    for (id, phase) in [
        (0, TouchPhase::Started),
        (1, TouchPhase::Started),
        (0, TouchPhase::Moved),
        (1, TouchPhase::Moved),
        (0, TouchPhase::Ended),
        (1, TouchPhase::Cancelled),
        (0, TouchPhase::Moved),
        (0, TouchPhase::Ended),
        (0, TouchPhase::Started),
        (0, TouchPhase::Cancelled),
    ] {
        Headless::touch(id, at, phase);
    }
    rig.settle();
    assert_eq!(
        serde_json::from_str::<Value>(&rig.string("touch-events")).unwrap(),
        json!([{"_tag": "Touched"}, {"_tag": "Touched"}])
    );
}

#[test]
fn touched_precedes_down_release_and_drag_callbacks_on_both_roots() {
    use i_slint_core::input::TouchPhase;
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    let mut expected = Vec::new();
    for overlay in [false, true] {
        if overlay {
            rig.fixture.set_overlay(true);
            rig.until("overlay", |r| r.shell.overlay_shown());
        }
        // This control activates on the down edge, before touch() returns.
        Headless::touch(
            0,
            slint::LogicalPosition::new(340.0, 60.0),
            TouchPhase::Started,
        );
        rig.settle();
        expected.extend([
            json!({"_tag": "Touched"}),
            json!({"_tag": "Activated", "target": "probe:down"}),
        ]);
        assert_eq!(
            serde_json::from_str::<Value>(&rig.string("touch-events")).unwrap(),
            json!(expected)
        );
        Headless::touch(
            0,
            slint::LogicalPosition::new(340.0, 60.0),
            TouchPhase::Cancelled,
        );
        for phase in [TouchPhase::Started, TouchPhase::Moved, TouchPhase::Ended] {
            Headless::touch(0, slint::LogicalPosition::new(80.0, 60.0), phase);
        }
        expected.extend([json!({"_tag": "Touched"}), json!({"_tag": "Activated", "target": if overlay { "probe:overlay" } else { "probe:tap" }})]);
        Headless::touch(
            0,
            slint::LogicalPosition::new(240.0, 60.0),
            TouchPhase::Started,
        );
        Headless::touch(
            0,
            slint::LogicalPosition::new(220.0, 60.0),
            TouchPhase::Moved,
        );
        Headless::touch(
            0,
            slint::LogicalPosition::new(220.0, 60.0),
            TouchPhase::Ended,
        );
        expected.extend([json!({"_tag": "Touched"}), json!({"_tag": "Dragged", "region": if overlay { "overlay-shelf" } else { "surface-shelf" }, "by": -2})]);
        rig.settle();
        assert_eq!(
            serde_json::from_str::<Value>(&rig.string("touch-events")).unwrap(),
            json!(expected)
        );
        if overlay {
            assert!(
                matches!(rig.shell.overlay_property("touch-events"), Some(SlintValue::String(s))
                if serde_json::from_str::<Value>(&s).unwrap() == json!(expected))
            );
        }
    }
}

#[test]
fn touched_from_a_missing_or_hidden_overlay_is_not_delivered() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    // The first loaded package has generation 1. Neither an absent Overlay
    // nor an old generation may deliver touch messages to the current logic.
    for (generation, window) in [(1, 1), (999, 0)] {
        rig.shell.handle(Event::Host {
            generation,
            window,
            event: HostEvent::TouchStarted,
        });
    }
    rig.settle();
    assert_eq!(rig.string("touch-events"), "[]");
    rig.fixture.set_overlay(true);
    rig.until("overlay", |r| r.shell.overlay_shown());
    rig.shell.handle(Event::Host {
        generation: 1,
        window: 0,
        event: HostEvent::TouchStarted,
    });
    rig.settle();
    assert_eq!(rig.string("touch-events"), "[]");
}
