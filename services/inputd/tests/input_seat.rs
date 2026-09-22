use korri_inputd::input_seat::{
    invert_sunshine_axis, GamepadState, MirrorOutcome, SeatBackend, SeatResetOutcome, SeatRuntime,
    SeatSpec,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Default)]
struct RecordingSeatBackend {
    inner: Arc<Mutex<RecordingState>>,
}

#[derive(Default)]
struct RecordingState {
    created: Vec<u8>,
    destroyed: Vec<u8>,
    states: Vec<(u8, GamepadState)>,
}

impl RecordingSeatBackend {
    fn created_slots(&self) -> Vec<u8> {
        self.inner.lock().unwrap().created.clone()
    }
    fn destroyed_slots(&self) -> Vec<u8> {
        self.inner.lock().unwrap().destroyed.clone()
    }
    fn states(&self, slot: u8) -> Vec<GamepadState> {
        self.inner
            .lock()
            .unwrap()
            .states
            .iter()
            .filter(|(candidate, _)| *candidate == slot)
            .map(|(_, state)| *state)
            .collect()
    }
}

impl SeatBackend for RecordingSeatBackend {
    fn create(&mut self, spec: &SeatSpec) -> Result<(), String> {
        self.inner.lock().unwrap().created.push(spec.slot);
        Ok(())
    }
    fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String> {
        self.inner.lock().unwrap().states.push((slot, state));
        Ok(())
    }
    fn destroy(&mut self, slot: u8) -> Result<(), String> {
        self.inner.lock().unwrap().destroyed.push(slot);
        Ok(())
    }
}

const LAUNCH: &str = "0123456789abcdef0123456789abcdef";
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn envelope(frame: &str, token: &str) -> Vec<u8> {
    (format!(r#"{{"mirrorToken":"{token}","frame":{frame}}}"#) + "\n").into_bytes()
}

fn connected(controller: u8) -> String {
    format!(
        r#"{{"kind":"source-connected","launchId":"{LAUNCH}","controllerNumber":{controller}}}"#
    )
}

fn state(controller: u8, buttons: u32, left_y: i16, right_y: i16) -> String {
    format!(
        r#"{{"kind":"source-state","launchId":"{LAUNCH}","controllerNumber":{controller},"buttons":{buttons},"leftTrigger":3,"rightTrigger":4,"leftStickX":5,"leftStickY":{left_y},"rightStickX":6,"rightStickY":{right_y}}}"#
    )
}

fn neutral_state(controller: u8) -> String {
    format!(
        r#"{{"kind":"source-state","launchId":"{LAUNCH}","controllerNumber":{controller},"buttons":0,"leftTrigger":0,"rightTrigger":0,"leftStickX":0,"leftStickY":0,"rightStickX":0,"rightStickY":0}}"#
    )
}

#[test]
fn strict_envelope_and_authority_are_required() {
    let backend = RecordingSeatBackend::default();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    assert_eq!(
        runtime.accept(&envelope(&connected(0), "bad"), 0),
        MirrorOutcome::Unauthorized
    );
    let extra = format!(
        r#"{{"mirrorToken":"{TOKEN}","frame":{},"extra":1}}"#,
        connected(0)
    ) + "\n";
    assert_eq!(runtime.accept(extra.as_bytes(), 0), MirrorOutcome::Invalid);
    let mut no_newline = envelope(&connected(0), TOKEN);
    no_newline.pop();
    assert_eq!(runtime.accept(&no_newline, 0), MirrorOutcome::Invalid);
}

#[test]
fn four_sources_get_stable_seats_and_a_fifth_is_rejected() {
    let backend = RecordingSeatBackend::default();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    for controller in 0..4 {
        assert_eq!(
            runtime.accept(&envelope(&connected(controller), TOKEN), 0),
            MirrorOutcome::Accepted {
                slot: controller + 1
            }
        );
    }
    assert_eq!(
        runtime.accept(&envelope(&connected(4), TOKEN), 0),
        MirrorOutcome::NoSeat
    );
    assert_eq!(
        runtime.accept(&envelope(&connected(1), TOKEN), 0),
        MirrorOutcome::Accepted { slot: 2 }
    );
}

#[test]
fn state_without_a_connected_frame_acquires_a_seat() {
    let backend = RecordingSeatBackend::default();
    let probe = backend.clone();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();

    assert_eq!(
        runtime.accept(&envelope(&state(7, 0x1000, 1, -1), TOKEN), 1),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(probe.states(1).len(), 1);
}

#[test]
fn unsupported_extended_buttons_are_masked_without_dropping_supported_state() {
    let backend = RecordingSeatBackend::default();
    let probe = backend.clone();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    let extended = 0x0001_0000 | 0x1000;

    assert_eq!(
        runtime.accept(&envelope(&state(0, extended, 11, 12), TOKEN), 1),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(probe.states(1).last().unwrap().buttons, 0x1000);
    assert_eq!(probe.states(1).last().unwrap().left_stick_y, -11);
}

#[test]
fn state_is_inverted_forwarded_and_neutralized_on_disconnect() {
    let backend = RecordingSeatBackend::default();
    let probe = backend.clone();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    runtime.accept(&envelope(&connected(0), TOKEN), 0);
    assert_eq!(
        runtime.accept(&envelope(&state(0, 0x1000, -32768, 123), TOKEN), 1),
        MirrorOutcome::Accepted { slot: 1 }
    );
    let states = probe.states(1);
    assert_eq!(
        states.last(),
        Some(&GamepadState {
            buttons: 0x1000,
            left_trigger: 3,
            right_trigger: 4,
            left_stick_x: 5,
            left_stick_y: 32767,
            right_stick_x: 6,
            right_stick_y: -123
        })
    );
    let disconnected = format!(
        r#"{{"kind":"source-disconnected","launchId":"{LAUNCH}","controllerNumber":0,"reason":"gone"}}"#
    );
    runtime.accept(&envelope(&disconnected, TOKEN), 2);
    let states = probe.states(1);
    assert_eq!(states.last(), Some(&GamepadState::neutral()));
    assert_eq!(
        runtime.accept(&envelope(&connected(0), TOKEN), 3),
        MirrorOutcome::Accepted { slot: 1 }
    );
}

#[test]
fn stale_source_timeout_releases_held_state() {
    let backend = RecordingSeatBackend::default();
    let probe = backend.clone();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    runtime.accept(&envelope(&state(0, 0x1000, 0, 0), TOKEN), 10);

    assert_eq!(runtime.expire_stale(1_259).unwrap(), 0);
    assert_eq!(runtime.expire_stale(1_260).unwrap(), 1);
    assert_eq!(probe.states(1).last(), Some(&GamepadState::neutral()));
    assert_eq!(
        runtime.accept(&envelope(&state(0, 0, 0, 0), TOKEN), 511),
        MirrorOutcome::Accepted { slot: 1 }
    );
}

#[test]
fn reset_neutralizes_active_seats_and_discards_held_state_until_a_neutral_frame() {
    let backend = RecordingSeatBackend::default();
    let probe = backend.clone();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    runtime.accept(&envelope(&state(0, 0x1000, 0, 0), TOKEN), 1);
    runtime.accept(&envelope(&state(1, 0x2000, 0, 0), TOKEN), 1);

    assert_eq!(runtime.reset(LAUNCH), SeatResetOutcome::Accepted);
    assert_eq!(probe.states(1).last(), Some(&GamepadState::neutral()));
    assert_eq!(probe.states(2).last(), Some(&GamepadState::neutral()));
    assert!(probe.states(3).is_empty());

    let seat_one_writes = probe.states(1).len();
    assert_eq!(
        runtime.accept(&envelope(&state(0, 0x1000, 0, 0), TOKEN), 2),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(probe.states(1).len(), seat_one_writes);

    assert_eq!(
        runtime.accept(&envelope(&neutral_state(0), TOKEN), 3),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(probe.states(1).len(), seat_one_writes);
    assert_eq!(
        runtime.accept(&envelope(&state(0, 0x1000, 0, 0), TOKEN), 4),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(probe.states(1).len(), seat_one_writes + 1);
}

#[test]
fn reset_refuses_a_stale_launch_without_touching_seats() {
    let backend = RecordingSeatBackend::default();
    let probe = backend.clone();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    runtime.accept(&envelope(&state(0, 0x1000, 0, 0), TOKEN), 1);
    let writes = probe.states(1).len();

    assert_eq!(
        runtime.reset("fedcba9876543210fedcba9876543210"),
        SeatResetOutcome::StaleLaunch
    );
    assert_eq!(probe.states(1).len(), writes);
}

#[test]
fn partial_reset_failure_gates_every_active_source_until_post_reset_neutral() {
    struct FailingWriteBackend {
        probe: RecordingSeatBackend,
        fail_slot: Arc<Mutex<Option<u8>>>,
    }
    impl SeatBackend for FailingWriteBackend {
        fn create(&mut self, spec: &SeatSpec) -> Result<(), String> {
            self.probe.create(spec)
        }
        fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String> {
            let mut fail_slot = self.fail_slot.lock().unwrap();
            if *fail_slot == Some(slot) {
                *fail_slot = None;
                Err("write failed".into())
            } else {
                drop(fail_slot);
                self.probe.write_state(slot, state)
            }
        }
        fn destroy(&mut self, slot: u8) -> Result<(), String> {
            self.probe.destroy(slot)
        }
    }
    let probe = RecordingSeatBackend::default();
    let fail_slot = Arc::new(Mutex::new(None));
    let mut runtime = SeatRuntime::start(
        LAUNCH,
        TOKEN,
        FailingWriteBackend {
            probe: probe.clone(),
            fail_slot: fail_slot.clone(),
        },
    )
    .unwrap();
    runtime.accept(&envelope(&state(0, 0x1000, 0, 0), TOKEN), 1);
    runtime.accept(&envelope(&state(1, 0x2000, 0, 0), TOKEN), 1);

    *fail_slot.lock().unwrap() = Some(2);
    assert_eq!(runtime.reset(LAUNCH), SeatResetOutcome::BackendFailed);
    assert_eq!(probe.states(1).last(), Some(&GamepadState::neutral()));
    assert_eq!(probe.states(2).last().unwrap().buttons, 0x2000);

    let seat_one_writes = probe.states(1).len();
    let seat_two_writes = probe.states(2).len();
    assert_eq!(
        runtime.accept(&envelope(&state(0, 0x4000, 0, 0), TOKEN), 2),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(
        runtime.accept(&envelope(&state(1, 0x4000, 0, 0), TOKEN), 2),
        MirrorOutcome::Accepted { slot: 2 }
    );
    assert_eq!(probe.states(1).len(), seat_one_writes);
    assert_eq!(probe.states(2).len(), seat_two_writes);

    assert_eq!(runtime.reset(LAUNCH), SeatResetOutcome::Accepted);
    let seat_one_writes = probe.states(1).len();
    let seat_two_writes = probe.states(2).len();
    runtime.accept(&envelope(&state(0, 0x4000, 0, 0), TOKEN), 3);
    runtime.accept(&envelope(&state(1, 0x4000, 0, 0), TOKEN), 3);
    assert_eq!(probe.states(1).len(), seat_one_writes);
    assert_eq!(probe.states(2).len(), seat_two_writes);

    runtime.accept(&envelope(&neutral_state(0), TOKEN), 4);
    runtime.accept(&envelope(&neutral_state(1), TOKEN), 4);
    runtime.accept(&envelope(&state(0, 0x4000, 0, 0), TOKEN), 5);
    runtime.accept(&envelope(&state(1, 0x4000, 0, 0), TOKEN), 5);
    assert_eq!(probe.states(1).last().unwrap().buttons, 0x4000);
    assert_eq!(probe.states(2).last().unwrap().buttons, 0x4000);
}

#[test]
fn duplicate_connected_frames_do_not_reset_the_rate_limit() {
    let backend = RecordingSeatBackend::default();
    let mut runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    runtime.accept(&envelope(&connected(0), TOKEN), 1);
    for index in 0..240 {
        assert_eq!(
            runtime.accept(&envelope(&state(0, index & 1, 0, 0), TOKEN), 1),
            MirrorOutcome::Accepted { slot: 1 }
        );
    }
    runtime.accept(&envelope(&connected(0), TOKEN), 1);
    assert_eq!(
        runtime.accept(&envelope(&state(0, 0, 0, 0), TOKEN), 1),
        MirrorOutcome::RateLimited
    );
}

#[test]
fn disconnect_write_failure_is_reported_and_can_be_neutralized_on_retry() {
    struct FailingWriteBackend {
        probe: RecordingSeatBackend,
        fail: Arc<AtomicBool>,
    }
    impl SeatBackend for FailingWriteBackend {
        fn create(&mut self, spec: &SeatSpec) -> Result<(), String> {
            self.probe.create(spec)
        }
        fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String> {
            if self.fail.load(Ordering::SeqCst) {
                Err("write failed".into())
            } else {
                self.probe.write_state(slot, state)
            }
        }
        fn destroy(&mut self, slot: u8) -> Result<(), String> {
            self.probe.destroy(slot)
        }
    }
    let probe = RecordingSeatBackend::default();
    let fail = Arc::new(AtomicBool::new(false));
    let mut runtime = SeatRuntime::start(
        LAUNCH,
        TOKEN,
        FailingWriteBackend {
            probe: probe.clone(),
            fail: fail.clone(),
        },
    )
    .unwrap();
    runtime.accept(&envelope(&state(0, 0x1000, 0, 0), TOKEN), 1);
    let disconnected = format!(
        r#"{{"kind":"source-disconnected","launchId":"{LAUNCH}","controllerNumber":0,"reason":"gone"}}"#
    );

    fail.store(true, Ordering::SeqCst);
    assert_eq!(
        runtime.accept(&envelope(&disconnected, TOKEN), 2),
        MirrorOutcome::BackendFailed
    );
    assert_ne!(probe.states(1).last(), Some(&GamepadState::neutral()));

    fail.store(false, Ordering::SeqCst);
    assert_eq!(
        runtime.accept(&envelope(&disconnected, TOKEN), 3),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(probe.states(1).last(), Some(&GamepadState::neutral()));
}

#[test]
fn stop_releases_all_seats() {
    let backend = RecordingSeatBackend::default();
    let probe = backend.clone();
    let runtime = SeatRuntime::start(LAUNCH, TOKEN, backend).unwrap();
    assert_eq!(probe.created_slots(), vec![1, 2, 3, 4]);
    runtime.stop().unwrap();
    assert_eq!(probe.destroyed_slots(), vec![4, 3, 2, 1]);
}

#[test]
fn partial_creation_failure_releases_created_seats_in_reverse_order() {
    struct FailingBackend {
        probe: RecordingSeatBackend,
    }
    impl SeatBackend for FailingBackend {
        fn create(&mut self, spec: &SeatSpec) -> Result<(), String> {
            if spec.slot == 3 {
                Err("create failed".into())
            } else {
                self.probe.create(spec)
            }
        }
        fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String> {
            self.probe.write_state(slot, state)
        }
        fn destroy(&mut self, slot: u8) -> Result<(), String> {
            self.probe.destroy(slot)
        }
    }
    let probe = RecordingSeatBackend::default();
    assert!(SeatRuntime::start(
        LAUNCH,
        TOKEN,
        FailingBackend {
            probe: probe.clone()
        }
    )
    .is_err());
    assert_eq!(probe.created_slots(), vec![1, 2]);
    assert_eq!(probe.destroyed_slots(), vec![2, 1]);
}

#[test]
fn sunshine_axis_inversion_handles_the_minimum() {
    assert_eq!(invert_sunshine_axis(-32768), 32767);
    assert_eq!(invert_sunshine_axis(32767), -32767);
    assert_eq!(invert_sunshine_axis(0), 0);
}
