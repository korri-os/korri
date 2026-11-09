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
    rig_with_backend(ids, games, behaviour, Rc::new(Headless::install()))
}

fn rig_with_backend(
    ids: &[&str],
    games: usize,
    behaviour: Behaviour,
    backend: Rc<dyn Backend>,
) -> Rig {
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
    let (model, examples) = verify::fixture();
    let fixture = Fixture::start(model, catalog::games(&examples, games), behaviour).unwrap();
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

/// Observes the host/backend contract while using the real headless renderer.
struct ObservedBackend {
    inner: Headless,
    viewer: std::cell::Cell<(bool, bool)>,
}
impl Backend for ObservedBackend {
    fn local_viewer(&self, presenting: bool, overlay: bool) {
        self.viewer.set((presenting, overlay));
    }
    fn shared_window(&self) -> bool {
        self.inner.shared_window()
    }
    fn attach(
        &self,
        window: &slint::Window,
        sink: Rc<dyn Fn(HostEvent)>,
        frames: korri_shell::backend::FrameCounter,
    ) {
        self.inner.attach(window, sink, frames);
    }
    fn place(&self, window: &slint::Window, placement: Placement) {
        self.inner.place(window, placement);
    }
}

#[test]
fn host_viewer_watch_and_overlay_model_drive_the_backend_without_entering_surface_logic() {
    let backend = Rc::new(ObservedBackend {
        inner: Headless::install(),
        viewer: Default::default(),
    });
    let mut rig = rig_with_backend(&["pico"], 12, Behaviour::default(), backend.clone());
    assert_eq!(backend.viewer.get(), (false, false));
    rig.fixture.set_local_viewer_presenting(true);
    rig.until("viewer presenting", |_| {
        backend.viewer.get() == (true, false)
    });
    rig.fixture.set_overlay(true);
    rig.until("gameplay overlay over video", |_| {
        backend.viewer.get() == (true, true)
    });
    rig.fixture.set_local_viewer_presenting(false);
    rig.until("viewer stopped in overlay", |_| {
        backend.viewer.get() == (false, true)
    });
    rig.fixture.set_overlay(false);
    rig.until("catalog restored", |_| {
        backend.viewer.get() == (false, false)
    });
    rig.shell.handle(Event::LocalViewer {
        generation: 9999,
        presenting: true,
    });
    assert_eq!(
        backend.viewer.get(),
        (false, false),
        "retired watches cannot change presentation"
    );
}

#[test]
fn viewer_play_retires_hidden_surface_input_and_returns_it_after_stop() {
    let mut rig = rig(&["probe", "pico"], 12, Behaviour::default());
    let active = Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = active.clone();
    rig.shell
        .on_native_active(Box::new(move |value| observed.borrow_mut().push(value)));
    rig.fixture.set_local_viewer_presenting(true);
    rig.until("native attachment retired", |_| {
        active.borrow().last() == Some(&false)
    });
    rig.key(Key::Right);
    rig.key(Key::F2);
    assert_eq!(rig.shell.current_id(), Some("probe"));
    assert_eq!(rig.int("count"), 0.0);
    rig.fixture.set_local_viewer_presenting(false);
    rig.until("native attachment restored", |_| {
        active.borrow().last() == Some(&true)
    });
    rig.key(Key::Right);
    assert_eq!(rig.int("count"), 1.0);
}
