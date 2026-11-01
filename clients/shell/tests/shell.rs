//! The shell end to end, without a display: real packages, the real korrid
//! wire against the fixture korrid, the software renderer in memory.
#![cfg(feature = "fixture")]

use korri_shell::{
    backend::{headless::Headless, Backend, HostEvent, Placement},
    catalog, fonts,
    engine::Limits,
    fixture::{Behaviour, Fixture},
    input::Key,
    korrid::Korrid,
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
    let backend: Rc<dyn Backend> = Rc::new(Headless::install());
    let root = tempfile::tempdir().unwrap();
    let packages: Vec<Package> = ids
        .iter()
        .map(|id| {
            let dir = root.path().join(id);
            copy(&source(id), &dir);
            Package::at(&dir).unwrap()
        })
        .collect();
    let (model, examples) = verify::fixture();
    let fixture = Fixture::start(model, catalog::games(&examples, games), behaviour).unwrap();
    let queue = Arc::new(Queue::default());
    let options = Options { limits: Limits::default(), placement: Placement::Windowed { width: 640, height: 480 }, stats: false };
    let mut shell = Shell::new(packages, Korrid::new(fixture.connection()), backend, queue.clone(), options);
    shell.start().unwrap();
    let mut rig = Rig { shell, queue, fixture, _root: root };
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
            assert!(started.elapsed() < Duration::from_secs(10), "timed out waiting for {what}; log: {:?}", self.shell.log());
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
            self.shell.handle(Event::Host { generation: 0, window: 0, event: HostEvent::Key { key, pressed } });
        }
        self.settle();
    }

    fn int(&self, name: &str) -> f64 {
        match self.shell.surface_property(name) {
            Some(SlintValue::Number(n)) => n,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn string(&self, name: &str) -> String {
        match self.shell.surface_property(name) {
            Some(SlintValue::String(s)) => s.to_string(),
            other => panic!("{name}: {other:?}"),
        }
    }

    fn requests(&self, tag: &str) -> Vec<Value> {
        self.fixture.requests().into_iter().filter(|(_, r)| r["_tag"] == tag).map(|(_, r)| r).collect()
    }
}

#[test]
fn loads_declares_reads_preferences_and_shows_the_view() {
    let rig = rig(&["probe"], 12, Behaviour::default());
    assert_eq!(rig.shell.current_id(), Some("probe"));
    let calls = rig.fixture.calls();
    assert_eq!(&calls[..3], &["app.surface.declare", "app.surface.preferences", "app.surface.model"]);
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
    rig.shell.handle(Event::Host { generation: 0, window: 0, event: HostEvent::Key { key: Key::Left, pressed: true } });
    std::thread::sleep(Duration::from_millis(560));
    rig.until("repeats", |r| r.int("count") <= 0.0);
    rig.shell.handle(Event::Host { generation: 0, window: 0, event: HostEvent::Key { key: Key::Left, pressed: false } });
    rig.settle();
    let count = rig.int("count");
    // One press plus at least one repeat after 400 ms, then every 100 ms.
    assert!(count <= 0.0 && count >= -4.0, "{count}");
}

#[test]
fn preferences_round_trip_through_korrid() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    rig.key(Key::O);
    rig.until("PreferenceStored", |r| matches!(r.shell.surface_property("stored"), Some(SlintValue::Bool(true))));
    assert_eq!(rig.fixture.stored("probe").get("probe.word").map(String::as_str), Some("second"));
    // A fresh load gets the stored value in init.
    let backend: Rc<dyn Backend> = Rc::new(Headless::install());
    let package = Package::at(&rig._root.path().join("probe")).unwrap();
    let mut again = Shell::new(vec![package], Korrid::new(rig.fixture.connection()), backend, rig.queue.clone(), Options::default());
    again.start().unwrap();
    assert!(matches!(again.surface_property("word"), Some(SlintValue::String(s)) if s.as_str() == "second"));
}

#[test]
fn a_refused_declaration_keeps_the_current_surface() {
    let behaviour = Behaviour { refuse_declare: vec!["pico".into()], ..Default::default() };
    let mut rig = rig(&["probe", "pico"], 12, behaviour);
    rig.key(Key::F2);
    assert_eq!(rig.shell.current_id(), Some("probe"));
    assert!(rig.shell.log().iter().any(|l| l.contains("pico not loaded") && l.contains("app.surface.declare")), "{:?}", rig.shell.log());
}

#[test]
fn model_pushes_run_korri_published_and_are_acked_in_order() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    rig.key(Key::Down); // MoveCatalog End
    rig.until("new window", |r| r.fixture.requests().iter().any(|(_, q)| q["_tag"] == "MoveCatalog"));
    let before = rig.fixture.acks().len();
    rig.fixture.publish(|model| model["clockLabel"] = json!("12:34"));
    rig.until("ack", |r| r.fixture.acks().len() > before);
    let acks = rig.fixture.acks();
    let revisions: Vec<u64> = acks.iter().map(|(_, r)| r.parse().unwrap()).collect();
    assert!(revisions.windows(2).all(|w| w[0] < w[1]), "{revisions:?}");
}

#[test]
fn the_overlay_follows_the_presentation() {
    let mut rig = rig(&["probe"], 12, Behaviour::default());
    assert!(!rig.shell.overlay_shown());
    rig.fixture.set_overlay(true);
    rig.until("overlay", |r| r.shell.overlay_shown());
    assert!(matches!(rig.shell.overlay_property("heading"), Some(SlintValue::String(s)) if s.as_str() == "Paused"));
    assert_eq!(Headless::live_windows(), 2);
    rig.key(Key::Escape); // back: DismissGameplayOverlay
    rig.until("overlay gone", |r| !r.shell.overlay_shown());
    assert_eq!(rig.requests("DismissGameplayOverlay").len(), 1);
    assert_eq!(Headless::live_windows(), 1);
}

#[test]
fn switching_replaces_the_surface_after_its_first_frame_and_fonts_do_not_pile_up() {
    let mut rig = rig(&["pico", "zone"], 12, Behaviour::default());
    assert_eq!(rig.shell.current_id(), Some("pico"));
    assert_eq!(fonts::registered_faces("Pico Tiny5"), 1);
    rig.key(Key::F2);
    rig.until("zone shown, pico retired", |r| r.shell.current_id() == Some("zone") && r.shell.retiring() == 0);
    assert_eq!(Headless::live_windows(), 1);
    for _ in 0..6 {
        rig.key(Key::F2);
        rig.until("switched", |r| r.shell.retiring() == 0);
    }
    assert_eq!(rig.shell.current_id(), Some("zone"));
    // Each font file is registered once, however often its surface loads.
    assert_eq!(fonts::registered_faces("Pico Tiny5"), 1);
    assert_eq!(fonts::registered_faces("Anton"), 1);
}

#[test]
fn a_runaway_call_unloads_the_surface_and_restores_the_previous_one() {
    let mut rig = rig(&["pico", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("probe", |r| r.shell.current_id() == Some("probe") && r.shell.retiring() == 0);
    let started = Instant::now();
    rig.key(Key::Return); // probe loops forever on confirm
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(rig.shell.current_id(), Some("pico"));
    assert!(rig.shell.log().iter().any(|l| l.contains("probe unloaded (time)")), "{:?}", rig.shell.log());
}

#[test]
fn a_heap_blow_up_unloads_the_surface() {
    let mut rig = rig(&["pico", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("probe", |r| r.shell.current_id() == Some("probe"));
    rig.key(Key::M); // probe allocates without end on menu
    assert_eq!(rig.shell.current_id(), Some("pico"));
    assert!(rig.shell.log().iter().any(|l| l.contains("probe unloaded (memory)")), "{:?}", rig.shell.log());
}

#[test]
fn an_unknown_request_is_a_violation() {
    let mut rig = rig(&["pico", "probe"], 12, Behaviour::default());
    rig.key(Key::F2);
    rig.until("probe", |r| r.shell.current_id() == Some("probe"));
    rig.shell.press(korri_shell::messages::Button::System);
    rig.settle();
    assert_eq!(rig.shell.current_id(), Some("pico"));
    assert!(rig.shell.log().iter().any(|l| l.contains("probe unloaded (contract)")));
}

#[test]
fn native_gamepad_input_moves_and_l1_r1_switches() {
    let mut rig = rig(&["probe", "pico"], 12, Behaviour::default());
    let connection = rig.fixture.connection();
    let queue = rig.queue.clone();
    let _input = korri_shell::input::native::NativeInput::start(connection, true, Box::new(move |event| queue.post(Event::Native(event))));
    rig.until("subscribed", |r| r.fixture.input_subscribers() == 1);
    rig.fixture.send_input(&Fixture::gamepad_frames("pad"));
    rig.settle();
    rig.fixture.send_input(&Fixture::press_frames("pad", 0x223)); // D-pad right
    rig.until("moved", |r| r.int("count") == 1.0);
    let key = |code: u16, value: u8| json!({"kind": "input", "deviceId": "pad", "class": "gamepad", "type": 1, "code": code, "value": value, "timestamp": 0});
    let report = json!({"kind": "input", "deviceId": "pad", "class": "gamepad", "type": 0, "code": 0, "value": 0, "timestamp": 0});
    rig.fixture.send_input(&[key(0x136, 1), report.clone(), key(0x137, 1), report.clone(), key(0x136, 0), key(0x137, 0), report]);
    rig.until("switched by L1+R1", |r| r.shell.current_id() == Some("pico"));
}

#[test]
fn zone_pages_a_10000_game_catalog_and_keeps_its_focus() {
    let mut rig = rig(&["zone"], 10_000, Behaviour::default());
    assert_eq!(rig.int("total"), 10_000.0);
    // A person moves one card at a time; korrid answers between presses.
    for _ in 0..60 {
        rig.shell.press(korri_shell::messages::Button::Right);
        rig.settle();
    }
    assert!(rig.int("first-index") > 0.0, "the window moved");
    let focus = rig.int("first-index") + rig.int("focused-card");
    assert_eq!(focus, 61.0, "60 presses from the second game reach the 62nd");
    let moves = rig.requests("MoveCatalog");
    assert!(!moves.is_empty() && moves.iter().all(|m| m["from"]["_tag"] == "Around" && m["count"] == 50));
    let focus_reports = rig.requests("FocusGame");
    assert_eq!(focus_reports.last().unwrap()["gameId"], json!(catalog::games(&verify::fixture().1, 10_000)[61]["id"]));
}
