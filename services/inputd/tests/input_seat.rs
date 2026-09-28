use korri_inputd::input_seat::*;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct RecordingSeatBackend(Arc<Mutex<Record>>);
#[derive(Default)]
struct Record {
    created: Vec<u8>,
    destroyed: Vec<u8>,
    states: Vec<(u8, GamepadState)>,
    fail_create: Option<u8>,
    fail_write: bool,
}
impl RecordingSeatBackend {
    fn state(&self, slot: u8) -> GamepadState {
        self.0
            .lock()
            .unwrap()
            .states
            .iter()
            .rev()
            .find(|(id, _)| *id == slot)
            .unwrap()
            .1
    }
}
impl SeatBackend for RecordingSeatBackend {
    fn create(&mut self, spec: &SeatSpec) -> Result<(), String> {
        let mut record = self.0.lock().unwrap();
        if record.fail_create == Some(spec.slot) {
            return Err("create failed".into());
        }
        record.created.push(spec.slot);
        Ok(())
    }
    fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String> {
        let mut record = self.0.lock().unwrap();
        if record.fail_write {
            return Err("partial write failed".into());
        }
        record.states.push((slot, state));
        Ok(())
    }
    fn destroy(&mut self, slot: u8) -> Result<(), String> {
        self.0.lock().unwrap().destroyed.push(slot);
        Ok(())
    }
}
const LAUNCH: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "fedcba9876543210fedcba9876543210";
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HELD: GamepadState = GamepadState {
    buttons: 0x1000,
    left_trigger: 255,
    right_trigger: 128,
    left_stick_x: i16::MIN,
    left_stick_y: i16::MAX,
    right_stick_x: -100,
    right_stick_y: 100,
};
type Runtime = SeatRuntime<RecordingSeatBackend>;
fn runtime(count: u8) -> (Runtime, RecordingSeatBackend) {
    let backend = RecordingSeatBackend::default();
    let mut runtime = SeatRuntime::boot(backend.clone()).unwrap();
    ok(&mut runtime, SeatRequest::ApplyCount { count });
    (runtime, backend)
}
fn ok(runtime: &mut Runtime, request: SeatRequest) -> SeatReply {
    let reply = runtime.coordinate(request, 10);
    assert_eq!(reply.failure, None);
    reply
}
fn begin(runtime: &mut Runtime) {
    ok(
        runtime,
        SeatRequest::BeginSession {
            launch_id: LAUNCH.into(),
        },
    );
    ok(
        runtime,
        SeatRequest::Route {
            launch_id: Some(LAUNCH.into()),
        },
    );
    runtime.bind(LAUNCH, TOKEN).unwrap();
}
fn connect(runtime: &mut Runtime, id: &str) -> Option<u8> {
    ok(
        runtime,
        SeatRequest::PhysicalConnected {
            device_id: id.into(),
            name: "Existing native name".into(),
            state: HELD,
        },
    )
    .slot
}
fn update(runtime: &mut Runtime, id: &str, state: GamepadState) {
    ok(
        runtime,
        SeatRequest::PhysicalState {
            device_id: id.into(),
            state,
        },
    );
}
fn envelope(frame: &str, token: &str) -> Vec<u8> {
    (format!(r#"{{"mirrorToken":"{token}","frame":{frame}}}"#) + "\n").into_bytes()
}
fn connected(controller: u8) -> String {
    format!(
        r#"{{"kind":"source-connected","launchId":"{LAUNCH}","controllerNumber":{controller}}}"#
    )
}
fn disconnected(controller: u8) -> String {
    format!(
        r#"{{"kind":"source-disconnected","launchId":"{LAUNCH}","controllerNumber":{controller},"reason":"gone"}}"#
    )
}
fn frame_state(controller: u8, state: GamepadState) -> String {
    format!(
        r#"{{"kind":"source-state","launchId":"{LAUNCH}","controllerNumber":{controller},"buttons":{},"leftTrigger":{},"rightTrigger":{},"leftStickX":{},"leftStickY":{},"rightStickX":{},"rightStickY":{}}}"#,
        state.buttons,
        state.left_trigger,
        state.right_trigger,
        state.left_stick_x,
        state.left_stick_y,
        state.right_stick_x,
        state.right_stick_y
    )
}
fn remote(runtime: &mut Runtime, controller: u8, state: GamepadState) -> MirrorOutcome {
    runtime.accept(&envelope(&frame_state(controller, state), TOKEN), 10)
}

#[test]
fn disconnect_without_optional_reason_uses_its_kind_and_neutralizes_the_seat() {
    let (mut runtime, backend) = runtime(4);
    begin(&mut runtime);
    assert_eq!(
        remote(&mut runtime, 0, GamepadState::neutral()),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(
        remote(&mut runtime, 0, HELD),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_ne!(backend.state(1), GamepadState::neutral());
    let disconnected =
        format!(r#"{{"kind":"source-disconnected","launchId":"{LAUNCH}","controllerNumber":0}}"#);
    assert_eq!(
        runtime.accept(&envelope(&disconnected, TOKEN), 20),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(backend.state(1), GamepadState::neutral());
    assert!(ok(&mut runtime, SeatRequest::Poll)
        .remote_sources
        .is_empty());
}

#[test]
fn startup_requires_count_reconciliation_and_authoritative_session() {
    let mut runtime = SeatRuntime::boot(RecordingSeatBackend::default()).unwrap();
    assert!(runtime.bind(LAUNCH, TOKEN).is_err());
    assert!(ok(&mut runtime, SeatRequest::Hello).recovery_required);
    assert_eq!(
        runtime
            .coordinate(
                SeatRequest::BeginSession {
                    launch_id: LAUNCH.into()
                },
                0
            )
            .failure,
        Some(SeatFailure::NotReady)
    );
    assert!(!ok(&mut runtime, SeatRequest::ApplyCount { count: 6 }).recovery_required);
    assert!(runtime.bind(LAUNCH, TOKEN).is_err());
    begin(&mut runtime);
}
#[test]
fn rearm_uses_existing_kernel_flat_for_every_stick_and_requires_button_trigger_release() {
    assert_eq!(GAMEPAD_STICK_FLAT, 4096);
    for axis in 0..4 {
        for value in [-4096, -97, 0, 121, 4096, -4097, 4097, i16::MIN, i16::MAX] {
            let mut state = GamepadState::neutral();
            match axis {
                0 => state.left_stick_x = value,
                1 => state.left_stick_y = value,
                2 => state.right_stick_x = value,
                _ => state.right_stick_y = value,
            }
            assert_eq!(
                state.is_neutral_for_rearm(),
                (-4096..=4096).contains(&value)
            );
        }
    }
    for state in [
        GamepadState {
            buttons: 1,
            ..GamepadState::neutral()
        },
        GamepadState {
            buttons: 0x1000,
            ..GamepadState::neutral()
        },
        GamepadState {
            left_trigger: 1,
            ..GamepadState::neutral()
        },
        GamepadState {
            right_trigger: 1,
            ..GamepadState::neutral()
        },
    ] {
        assert!(!state.is_neutral_for_rearm());
    }
}

#[test]
fn physical_drift_baseline_arms_but_above_flat_stick_keeps_route_barrier_closed() {
    let (mut runtime, probe) = runtime(4);
    begin(&mut runtime);
    let drift = GamepadState {
        left_stick_x: 93,
        left_stick_y: -141,
        right_stick_x: -37,
        right_stick_y: 86,
        ..GamepadState::neutral()
    };
    ok(
        &mut runtime,
        SeatRequest::PhysicalConnected {
            device_id: "drifting-pad".into(),
            name: "Native controller".into(),
            state: drift,
        },
    );
    assert_eq!(probe.state(1), GamepadState::neutral()); // baseline never writes
    update(&mut runtime, "drifting-pad", HELD);
    assert_eq!(probe.state(1), HELD);

    ok(
        &mut runtime,
        SeatRequest::Route {
            launch_id: Some(LAUNCH.into()),
        },
    );
    update(
        &mut runtime,
        "drifting-pad",
        GamepadState {
            right_stick_y: 4097,
            ..drift
        },
    );
    update(&mut runtime, "drifting-pad", HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
    update(&mut runtime, "drifting-pad", drift);
    assert_eq!(probe.state(1), GamepadState::neutral()); // rearm frame only arms
    let gameplay = GamepadState {
        buttons: 0x1000,
        ..drift
    };
    update(&mut runtime, "drifting-pad", gameplay);
    assert_eq!(probe.state(1), gameplay); // do not clamp even in-flat game values
}

#[test]
fn remote_rearm_accepts_drift_after_reset_without_changing_gameplay_axes() {
    let (mut runtime, probe) = runtime(4);
    begin(&mut runtime);
    let drift = GamepadState {
        left_stick_x: -123,
        left_stick_y: 64,
        right_stick_x: 94,
        right_stick_y: -81,
        ..GamepadState::neutral()
    };
    remote(&mut runtime, 0, drift);
    remote(&mut runtime, 0, HELD);
    assert_ne!(probe.state(1), GamepadState::neutral());
    assert_eq!(runtime.reset(LAUNCH), SeatResetOutcome::Accepted);
    remote(
        &mut runtime,
        0,
        GamepadState {
            left_stick_x: -4097,
            ..drift
        },
    );
    remote(&mut runtime, 0, HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
    remote(&mut runtime, 0, drift);
    assert_eq!(probe.state(1), GamepadState::neutral());
    remote(
        &mut runtime,
        0,
        GamepadState {
            buttons: 0x1000,
            ..drift
        },
    );
    assert_eq!(
        probe.state(1),
        GamepadState {
            buttons: 0x1000,
            left_stick_y: -drift.left_stick_y,
            right_stick_y: -drift.right_stick_y,
            ..drift
        }
    );
}

#[test]
fn strict_envelope_launch_and_token_are_required() {
    let (mut runtime, _) = runtime(4);
    begin(&mut runtime);
    assert_eq!(
        runtime.accept(&envelope(&connected(0), "bad"), 0),
        MirrorOutcome::Unauthorized
    );
    let extra = format!(
        r#"{{"mirrorToken":"{TOKEN}","frame":{},"extra":1}}"#,
        connected(0)
    ) + "\n";
    assert_eq!(runtime.accept(extra.as_bytes(), 0), MirrorOutcome::Invalid);
    let mut packet = envelope(&connected(0), TOKEN);
    packet.pop();
    assert_eq!(runtime.accept(&packet, 0), MirrorOutcome::Invalid);
    assert_eq!(
        runtime.accept(&envelope(&connected(0).replace(LAUNCH, OTHER), TOKEN), 0),
        MirrorOutcome::StaleLaunch
    );
    assert_eq!(
        runtime.accept(&envelope(&connected(16), TOKEN), 0),
        MirrorOutcome::Invalid
    );
    assert_eq!(
        runtime.accept(&vec![b' '; MAX_MIRROR_FRAME_BYTES + 1], 0),
        MirrorOutcome::Invalid
    );
    assert_eq!(runtime.accept(b"\n", 0), MirrorOutcome::Invalid);
}
#[test]
fn six_seats_mix_physical_and_remote_first_free_without_losing_overflow() {
    let (mut runtime, probe) = runtime(6);
    begin(&mut runtime);
    assert_eq!(connect(&mut runtime, "native-a"), Some(1));
    assert_eq!(
        runtime.accept(&envelope(&connected(9), TOKEN), 10),
        MirrorOutcome::Accepted { slot: 2 }
    );
    remote(&mut runtime, 9, GamepadState::neutral());
    assert_eq!(connect(&mut runtime, "native-b"), Some(3));
    for (controller, slot) in [(2, 4), (3, 5), (4, 6)] {
        assert_eq!(
            runtime.accept(&envelope(&connected(controller), TOKEN), 10),
            MirrorOutcome::Accepted { slot }
        );
        remote(&mut runtime, controller, GamepadState::neutral());
    }
    assert_eq!(connect(&mut runtime, "overflow-native"), None);
    assert_eq!(remote(&mut runtime, 5, HELD), MirrorOutcome::NoSeat);
    let reply = ok(&mut runtime, SeatRequest::Poll);
    assert_eq!(reply.remote_sources.len(), 5);
    assert_eq!(
        reply
            .remote_sources
            .iter()
            .find(|source| source.controller_number == 5)
            .unwrap()
            .slot,
        None
    );
    update(&mut runtime, "overflow-native", HELD);
    assert_eq!(probe.0.lock().unwrap().created, [1, 2, 3, 4, 5, 6]);
}
#[test]
fn baseline_routes_and_reconnect_require_neutral_before_gameplay() {
    let (mut runtime, probe) = runtime(4);
    connect(&mut runtime, "physical");
    update(&mut runtime, "physical", HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
    begin(&mut runtime);
    update(&mut runtime, "physical", HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
    update(&mut runtime, "physical", GamepadState::neutral());
    update(&mut runtime, "physical", HELD);
    assert_eq!(probe.state(1), HELD);
    ok(&mut runtime, SeatRequest::Route { launch_id: None });
    update(&mut runtime, "physical", HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
    ok(
        &mut runtime,
        SeatRequest::Route {
            launch_id: Some(LAUNCH.into()),
        },
    );
    update(&mut runtime, "physical", HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
    ok(
        &mut runtime,
        SeatRequest::PhysicalDisconnected {
            device_id: "physical".into(),
        },
    );
    assert_eq!(connect(&mut runtime, "physical"), Some(1));
    update(&mut runtime, "physical", HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
}
#[test]
fn mirror_stop_and_reset_do_not_end_game_reservations_or_release_physical_input() {
    let (mut runtime, probe) = runtime(2);
    begin(&mut runtime);
    connect(&mut runtime, "physical");
    update(&mut runtime, "physical", GamepadState::neutral());
    update(&mut runtime, "physical", HELD);
    remote(&mut runtime, 0, GamepadState::neutral());
    remote(&mut runtime, 0, HELD);
    assert_ne!(probe.state(2), GamepadState::neutral());
    assert_eq!(runtime.reset(OTHER), SeatResetOutcome::StaleLaunch);
    assert_eq!(runtime.reset(LAUNCH), SeatResetOutcome::Accepted);
    assert_eq!(probe.state(1), HELD);
    assert_eq!(probe.state(2), GamepadState::neutral());
    remote(&mut runtime, 0, HELD);
    assert_eq!(probe.state(2), GamepadState::neutral());
    runtime.unbind().unwrap();
    assert_eq!(
        ok(&mut runtime, SeatRequest::Poll).session.as_deref(),
        Some(LAUNCH)
    );
    assert_eq!(connect(&mut runtime, "replacement"), None);
    assert_eq!(remote(&mut runtime, 0, HELD), MirrorOutcome::StaleLaunch);
    ok(
        &mut runtime,
        SeatRequest::EndSession {
            launch_id: LAUNCH.into(),
        },
    );
    assert_eq!(connect(&mut runtime, "replacement"), Some(2));
    assert_eq!(probe.state(1), GamepadState::neutral());
    assert_eq!(probe.0.lock().unwrap().created, [1, 2, 3, 4]);
}
#[test]
fn disconnected_reservations_survive_pause_and_connected_assignments_survive_end() {
    let (mut runtime, _) = runtime(2);
    begin(&mut runtime);
    assert_eq!(connect(&mut runtime, "a"), Some(1));
    assert_eq!(connect(&mut runtime, "b"), Some(2));
    ok(
        &mut runtime,
        SeatRequest::PhysicalDisconnected {
            device_id: "a".into(),
        },
    );
    ok(&mut runtime, SeatRequest::Route { launch_id: None });
    assert_eq!(connect(&mut runtime, "c"), None);
    assert_eq!(connect(&mut runtime, "a"), Some(1));
    ok(
        &mut runtime,
        SeatRequest::PhysicalDisconnected {
            device_id: "a".into(),
        },
    );
    ok(
        &mut runtime,
        SeatRequest::EndSession {
            launch_id: LAUNCH.into(),
        },
    );
    assert_eq!(connect(&mut runtime, "b"), Some(2));
    assert_eq!(connect(&mut runtime, "c"), Some(1));
}
#[test]
fn full_remote_state_uses_linux_axis_orientation_and_masks_unsupported_buttons() {
    let (mut runtime, probe) = runtime(4);
    begin(&mut runtime);
    remote(&mut runtime, 0, GamepadState::neutral());
    remote(
        &mut runtime,
        0,
        GamepadState {
            buttons: u32::MAX,
            left_stick_y: i16::MIN,
            right_stick_y: 123,
            ..HELD
        },
    );
    let state = probe.state(1);
    assert_eq!(state.buttons, 0xf7ff);
    assert_eq!(state.left_stick_y, i16::MAX);
    assert_eq!(state.right_stick_y, -123);
    assert_eq!(state.left_trigger, 255);
    assert_eq!(state.right_trigger, 128);
    assert_eq!(invert_sunshine_axis(i16::MIN), i16::MAX);
}
#[test]
fn source_expiry_including_timestamp_zero_neutralizes_and_reports_loss() {
    let (mut runtime, probe) = runtime(4);
    begin(&mut runtime);
    runtime.accept(
        &envelope(&frame_state(0, GamepadState::neutral()), TOKEN),
        0,
    );
    runtime.accept(&envelope(&frame_state(0, HELD), TOKEN), 0);
    assert_ne!(probe.state(1), GamepadState::neutral());
    assert_eq!(runtime.expire_stale(STALE_SOURCE_TIMEOUT_MS).unwrap(), 1);
    assert_eq!(probe.state(1), GamepadState::neutral());
    let reply = ok(&mut runtime, SeatRequest::Poll);
    assert!(reply.remote_sources.is_empty());
    assert!(reply.remote_events.iter().any(|event| matches!(
        event,
        RemoteEvent::Disconnected {
            controller_number: 0,
            ..
        }
    )));
    remote(&mut runtime, 0, HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
}
#[test]
fn coordinator_loss_neutralizes_all_and_preserves_authoritative_session_until_recovered() {
    let (mut runtime, probe) = runtime(2);
    begin(&mut runtime);
    connect(&mut runtime, "physical");
    update(&mut runtime, "physical", GamepadState::neutral());
    update(&mut runtime, "physical", HELD);
    remote(&mut runtime, 0, GamepadState::neutral());
    remote(&mut runtime, 0, HELD);
    runtime.coordinator_lost().unwrap();
    assert_eq!(probe.state(1), GamepadState::neutral());
    assert_eq!(probe.state(2), GamepadState::neutral());
    let reply = ok(&mut runtime, SeatRequest::Hello);
    assert!(reply.recovery_required);
    assert_eq!(reply.session.as_deref(), Some(LAUNCH));
    assert!(runtime.bind(LAUNCH, TOKEN).is_err());
    assert_eq!(
        runtime
            .coordinate(SeatRequest::ApplyCount { count: 6 }, 20)
            .failure,
        Some(SeatFailure::Active)
    );
    ok(
        &mut runtime,
        SeatRequest::BeginSession {
            launch_id: LAUNCH.into(),
        },
    );
    assert_eq!(connect(&mut runtime, "replacement"), None);
    assert_eq!(connect(&mut runtime, "physical"), Some(1));
    update(&mut runtime, "physical", HELD);
    assert_eq!(probe.state(1), GamepadState::neutral());
}
#[test]
fn high_rate_physical_preserves_every_state_and_edge_without_retiring_quiet_source() {
    let (mut runtime, probe) = runtime(4);
    begin(&mut runtime);
    assert_eq!(connect(&mut runtime, "busy"), Some(1));
    assert_eq!(connect(&mut runtime, "quiet"), Some(2));
    update(&mut runtime, "busy", GamepadState::neutral());
    update(&mut runtime, "quiet", GamepadState::neutral());
    let start = probe.0.lock().unwrap().states.len();
    let mut expected = Vec::new();
    // Several translated axes/SYN frames per sample, exceeding the remote
    // budget within one second. Button edges straddle that old 240-frame cap.
    for index in 0..600u64 {
        let now = 11 + index;
        let state = GamepadState {
            buttons: if index % 2 == 0 { 0x1000 } else { 0 },
            left_stick_x: 10_000 + index as i16,
            left_stick_y: -12_000 - index as i16,
            right_stick_x: 8_000 + index as i16,
            right_stick_y: -9_000 - index as i16,
            ..GamepadState::neutral()
        };
        let reply = runtime.coordinate(
            SeatRequest::PhysicalState {
                device_id: "busy".into(),
                state,
            },
            now,
        );
        assert_eq!(reply.failure, None, "physical frame {index} was refused");
        assert_eq!(reply.slot, Some(1));
        assert!(!reply.recovery_required);
        expected.push((1, state));
        if index % 100 == 0 {
            let reply = runtime.coordinate(
                SeatRequest::PhysicalState {
                    device_id: "quiet".into(),
                    state: GamepadState::neutral(),
                },
                now,
            );
            assert_eq!(reply.failure, None);
            assert_eq!(reply.slot, Some(2));
        }
        assert_eq!(runtime.expire_stale(now).unwrap(), 0);
        assert_eq!(probe.state(2), GamepadState::neutral());
    }
    assert_eq!(
        &probe.0.lock().unwrap().states[start..],
        expected.as_slice()
    );
    // The quiet controller is still assigned and armed: its next press must
    // reach its original seat, not become a reconnect baseline or be lost.
    let reply = runtime.coordinate(
        SeatRequest::PhysicalState {
            device_id: "quiet".into(),
            state: HELD,
        },
        611,
    );
    assert_eq!(reply.failure, None);
    assert_eq!(reply.slot, Some(2));
    assert_eq!(probe.state(2), HELD);
    assert!(!runtime.faulted());
}

#[test]
fn remote_rate_limit_has_no_implicit_controller_number_seat_mapping() {
    assert_eq!(MAX_EVENTS_PER_SECOND, 240);
    let (mut runtime, _) = runtime(4);
    begin(&mut runtime);
    for _ in 0..MAX_EVENTS_PER_SECOND {
        assert_eq!(
            remote(&mut runtime, 15, GamepadState::neutral()),
            MirrorOutcome::Accepted { slot: 1 }
        );
    }
    assert_eq!(remote(&mut runtime, 15, HELD), MirrorOutcome::RateLimited);
    assert_eq!(
        runtime.accept(&envelope(&frame_state(15, HELD), TOKEN), 1010),
        MirrorOutcome::Accepted { slot: 1 }
    );
}
#[test]
fn remote_feedback_preserves_taps_and_fails_bounded_instead_of_losing_edges() {
    let (mut runtime, _) = runtime(4);
    begin(&mut runtime);
    remote(&mut runtime, 0, HELD);
    remote(&mut runtime, 0, GamepadState::neutral());
    let reply = ok(&mut runtime, SeatRequest::Poll);
    assert_eq!(reply.remote_events.len(), 2);
    assert!(
        matches!(&reply.remote_events[0], RemoteEvent::Connected { source } if source.state.buttons == HELD.buttons)
    );
    assert_eq!(reply.remote_sources[0].state, GamepadState::neutral());
    for index in 0..=MAX_REMOTE_EVENTS {
        remote(
            &mut runtime,
            1,
            if index % 2 == 0 {
                HELD
            } else {
                GamepadState::neutral()
            },
        );
    }
    assert!(runtime.feedback_failed());
    runtime.coordinator_lost().unwrap();
    assert!(!runtime.feedback_failed());
    assert!(ok(&mut runtime, SeatRequest::Hello).recovery_required);
}
#[test]
fn idle_count_apply_preserves_assignments_and_failure_never_permits_launch() {
    let (mut runtime, probe) = runtime(4);
    connect(&mut runtime, "physical");
    let reply = ok(&mut runtime, SeatRequest::ApplyCount { count: 6 });
    assert_eq!(reply.count, 6);
    assert_eq!(connect(&mut runtime, "physical"), Some(1));
    probe.0.lock().unwrap().fail_create = Some(7);
    let reply = runtime.coordinate(SeatRequest::ApplyCount { count: 8 }, 10);
    assert_eq!(reply.failure, Some(SeatFailure::Backend));
    assert!(reply.recovery_required);
    assert!(runtime.faulted());
    assert!(runtime.bind(LAUNCH, TOKEN).is_err());
    assert_eq!(
        runtime
            .coordinate(
                SeatRequest::BeginSession {
                    launch_id: LAUNCH.into()
                },
                10
            )
            .failure,
        Some(SeatFailure::Backend)
    );
}
#[test]
fn active_and_invalid_counts_do_not_recreate_devices() {
    let (mut runtime, probe) = runtime(4);
    assert_eq!(
        runtime
            .coordinate(SeatRequest::ApplyCount { count: 0 }, 10)
            .failure,
        Some(SeatFailure::Invalid)
    );
    begin(&mut runtime);
    assert_eq!(
        runtime
            .coordinate(SeatRequest::ApplyCount { count: 4 }, 10)
            .failure,
        Some(SeatFailure::Active)
    );
    ok(&mut runtime, SeatRequest::Route { launch_id: None });
    assert_eq!(
        runtime
            .coordinate(SeatRequest::ApplyCount { count: 6 }, 10)
            .failure,
        Some(SeatFailure::Active)
    );
    assert_eq!(probe.0.lock().unwrap().created, [1, 2, 3, 4]);
}
#[test]
fn failed_backend_write_poisoning_and_shutdown_release_devices() {
    let (mut runtime, probe) = runtime(4);
    begin(&mut runtime);
    connect(&mut runtime, "physical");
    update(&mut runtime, "physical", GamepadState::neutral());
    probe.0.lock().unwrap().fail_write = true;
    assert_eq!(
        runtime
            .coordinate(
                SeatRequest::PhysicalState {
                    device_id: "physical".into(),
                    state: HELD
                },
                10
            )
            .failure,
        Some(SeatFailure::Backend)
    );
    assert!(runtime.faulted());
    probe.0.lock().unwrap().fail_write = false;
    runtime.stop().unwrap();
    assert_eq!(probe.0.lock().unwrap().destroyed, [4, 3, 2, 1]);
    for slot in 1..=4 {
        assert_eq!(probe.state(slot), GamepadState::neutral());
    }
}
#[test]
fn explicit_remote_disconnect_is_launch_scoped_and_same_controller_reclaims() {
    let (mut runtime, probe) = runtime(4);
    begin(&mut runtime);
    remote(&mut runtime, 7, GamepadState::neutral());
    remote(&mut runtime, 7, HELD);
    assert_eq!(
        runtime.accept(&envelope(&disconnected(7), TOKEN), 10),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(probe.state(1), GamepadState::neutral());
    assert_eq!(
        remote(&mut runtime, 7, HELD),
        MirrorOutcome::Accepted { slot: 1 }
    );
    assert_eq!(probe.state(1), GamepadState::neutral());
    ok(
        &mut runtime,
        SeatRequest::EndSession {
            launch_id: LAUNCH.into(),
        },
    );
    ok(
        &mut runtime,
        SeatRequest::BeginSession {
            launch_id: OTHER.into(),
        },
    );
    runtime.bind(OTHER, TOKEN).unwrap();
    assert_eq!(remote(&mut runtime, 7, HELD), MirrorOutcome::StaleLaunch);
}
