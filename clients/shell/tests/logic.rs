//! A surface's logic on its own thread (`src/logic.rs`): calls run one at a
//! time in the order sent, a waiting `Tick` gives way to a newer one, the call
//! budget holds there, and dropping the logic stops a running call.

use korri_shell::{
    engine::{Limits, Violation},
    logic::{Call, Done, Logic, Outcome},
};
use serde_json::{json, Value};
use std::{
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

/// Records every message; `{"_tag": "Busy", "ms": n}` spins n ms; `Loop` never returns.
const SOURCE: &str = r#"
globalThis.surface = {
  preferences: {},
  init: () => ({ seen: [] }),
  update: (state, message) => {
    if (message._tag === "Busy") { let x = 0; for (let i = 0; i < message.ms * 20000; i++) x += i; }
    if (message._tag === "Loop") { for (;;) {} }
    return { state: { seen: [...state.seen, message] }, requests: [] };
  },
  view: (state) => ({ seen: JSON.stringify(state.seen) }),
};
"#;

fn start(limits: Limits) -> (Logic, Done, mpsc::Receiver<Outcome>) {
    let logic = Logic::start(SOURCE.into(), limits, "test").unwrap();
    logic
        .now(|w| {
            let engine = w.engine();
            engine.set_model("{}").unwrap();
            engine.init(&Default::default()).unwrap();
        })
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let done: Done = Arc::new(move |outcome| {
        let _ = tx.lock().unwrap().send(outcome);
    });
    (logic, done, rx)
}

/// Wait for the next outcome and let the logic send its next call, as the shell does.
fn next(logic: &mut Logic, done: &Done, rx: &mpsc::Receiver<Outcome>) -> Outcome {
    let outcome = rx.recv_timeout(Duration::from_secs(5)).expect("an outcome");
    logic.returned(done);
    outcome
}

fn seen(outcome: &Outcome) -> Vec<Value> {
    let step = outcome.result.as_ref().unwrap();
    serde_json::from_str(step.view.as_ref().unwrap().0["seen"].as_str().unwrap()).unwrap()
}

#[test]
fn calls_run_one_at_a_time_in_the_order_sent() {
    let (mut logic, done, rx) = start(Limits::default());
    for n in 0..20 {
        logic.submit(Call::Message(json!({"_tag": "Pressed", "n": n})), &done);
    }
    assert!(logic.busy());
    let mut last = Vec::new();
    for _ in 0..20 {
        last = seen(&next(&mut logic, &done, &rx));
    }
    let order: Vec<i64> = last.iter().map(|m| m["n"].as_i64().unwrap()).collect();
    assert_eq!(order, (0..20).collect::<Vec<_>>());
    assert!(!logic.busy());
    assert!(rx.try_recv().is_err(), "one outcome per call");
}

#[test]
fn ticks_waiting_behind_a_running_call_give_way_to_the_newest() {
    // The busy call counts as running until the UI side hears it returned (`next`), however
    // fast it is; a loose budget keeps a loaded machine from breaking it.
    let limits = Limits {
        call: Duration::from_secs(10),
        ..Limits::default()
    };
    let (mut logic, done, rx) = start(limits);
    logic.submit(Call::Message(json!({"_tag": "Busy", "ms": 1})), &done);
    for idle in [1000, 2000, 3000] {
        logic.submit(
            Call::Message(json!({"_tag": "Tick", "idleMs": idle})),
            &done,
        );
    }
    logic.submit(Call::Message(json!({"_tag": "Pressed", "n": 1})), &done);
    logic.submit(
        Call::Message(json!({"_tag": "Tick", "idleMs": 4000})),
        &done,
    );
    // Busy, then one Tick (the newest of the three waiting before the press: 3000),
    // the press, and the Tick after it.
    let mut outcomes = 0;
    let mut last = Vec::new();
    while outcomes < 4 {
        last = seen(&next(&mut logic, &done, &rx));
        outcomes += 1;
    }
    let tags: Vec<String> = last
        .iter()
        .map(|m| {
            format!(
                "{}{}",
                m["_tag"].as_str().unwrap(),
                m["idleMs"]
                    .as_i64()
                    .map_or(String::new(), |i| i.to_string())
            )
        })
        .collect();
    assert_eq!(last.len(), 4, "{tags:?}");
    assert_eq!(tags[0], "Busy");
    assert_eq!(tags[1], "Tick3000");
    assert_eq!(tags[2], "Pressed");
    assert_eq!(tags[3], "Tick4000");
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "no more calls"
    );
}

#[test]
fn a_runaway_call_breaks_the_budget_on_the_logic_thread() {
    let (mut logic, done, rx) = start(Limits::default());
    let started = Instant::now();
    logic.submit(Call::Message(json!({"_tag": "Loop"})), &done);
    let outcome = next(&mut logic, &done, &rx);
    assert!(
        matches!(outcome.result, Err(Violation::Time { .. })),
        "{:?}",
        outcome.result
    );
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn dropping_the_logic_stops_a_running_call() {
    // A budget far beyond the test: only the stop can end the loop.
    let limits = Limits {
        call: Duration::from_secs(60),
        ..Limits::default()
    };
    let (mut logic, done, _rx) = start(limits);
    logic.submit(Call::Message(json!({"_tag": "Loop"})), &done);
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    drop(logic);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "took {:?}",
        started.elapsed()
    );
}
