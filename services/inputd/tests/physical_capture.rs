use evdev::InputEvent;
use korri_input_contract::{GamepadState, RemoteEvent, RemoteSource, SeatRequest};
use korri_inputd::{
    action_catalog::{ActionId, ActionRoutes, DispatchMode},
    capture::{apply_event, CaptureRuntime},
    dbus::{DBUS_INPUT_MEMBER, DBUS_TARGET_INTERFACE},
    devices::{parse_proc_bus_input_devices, CaptureProvider, DeviceDescriptor, OpenedCapture},
    physical_sources::{identify_sources, IdentifiedSource, PhysicalSource, PhysicalSources},
    source_topology::SysfsSourceClassifier,
};
use std::{collections::BTreeMap, io, path::Path};
use tokio::sync::mpsc;

fn identities() -> Vec<IdentifiedSource> {
    let devices = parse_proc_bus_input_devices(
        include_str!("fixtures/proc-bus-input/one-virtual-one-raw.txt"),
        Path::new("/dev/input"),
    );
    (0..2)
        .map(|index| {
            let mut target = devices[0].clone();
            target.path = format!("/dev/input/event{}", 10 + index).into();
            target.device_number = Some(10 + index);
            let mut raw = devices[2].clone();
            raw.path = format!("/dev/input/event{}", 4 + index * 3).into();
            raw.unique_id = Some(format!("controller-{index}"));
            IdentifiedSource {
                device_id: format!("controller-{index}"),
                source: PhysicalSource {
                    composite_path: format!("/org/shadowblip/InputPlumber/CompositeDevice{index}"),
                    source_device_paths: vec![raw.path.to_str().unwrap().into()],
                    target_path: format!(
                        "/org/shadowblip/InputPlumber/devices/target/gamepad{index}"
                    ),
                    dbus_paths: vec![format!(
                        "/org/shadowblip/InputPlumber/devices/target/dbus{index}"
                    )],
                    descriptor: target,
                },
                raw,
            }
        })
        .collect()
}
#[derive(Default)]
struct Provider {
    opened: BTreeMap<std::path::PathBuf, OpenedCapture>,
    opens: usize,
}
impl Provider {
    fn add(
        &mut self,
        identity: &IdentifiedSource,
        initial: GamepadState,
    ) -> mpsc::Sender<io::Result<InputEvent>> {
        let (tx, rx) = mpsc::channel(32);
        let events = futures_util::stream::unfold(rx, |mut rx| async {
            rx.recv().await.map(|event| (event, rx))
        });
        self.opened.insert(
            identity.source.descriptor.path.clone(),
            OpenedCapture {
                descriptor: identity.source.descriptor.clone(),
                initial,
                events: Box::pin(events),
            },
        );
        tx
    }
}
impl CaptureProvider for Provider {
    fn enumerate_capture(&mut self) -> io::Result<Vec<DeviceDescriptor>> {
        unreachable!("stream tests supply authenticated discovery snapshots")
    }
    fn open_capture(&mut self, expected: &DeviceDescriptor) -> io::Result<OpenedCapture> {
        self.opens += 1;
        self.opened
            .remove(&expected.path)
            .ok_or_else(|| io::Error::other("not present"))
    }
}
fn runtime() -> CaptureRuntime {
    let mut runtime = CaptureRuntime::with_action_routes(ActionRoutes::default());
    runtime.set_dbus_owner(Some(":1.42"));
    runtime
}
async fn event(
    runtime: &mut CaptureRuntime,
    tx: &mpsc::Sender<io::Result<InputEvent>>,
    kind: u16,
    code: u16,
    value: i32,
) {
    tx.send(Ok(InputEvent::new(kind, code, value)))
        .await
        .unwrap();
    runtime.next_evdev_actions().await.unwrap();
}
fn dbus(
    runtime: &mut CaptureRuntime,
    sender: &str,
    index: usize,
    capability: &str,
    value: f64,
) -> Vec<korri_inputd::runtime::RuntimeAction> {
    let path = format!("/org/shadowblip/InputPlumber/devices/target/dbus{index}");
    let message = zbus::Message::signal(path.as_str(), DBUS_TARGET_INTERFACE, DBUS_INPUT_MEMBER)
        .unwrap()
        .sender(sender)
        .unwrap()
        .build(&(capability, value))
        .unwrap();
    runtime.handle_dbus_message(&message)
}

#[tokio::test]
async fn captures_two_identical_normalized_targets_independently_with_one_whole_state_route() {
    let ids = identities();
    assert_eq!(
        ids[0].source.descriptor.stable_identity(),
        ids[1].source.descriptor.stable_identity()
    );
    let mut provider = Provider::default();
    let one = provider.add(&ids[0], GamepadState::neutral());
    let two = provider.add(&ids[1], GamepadState::neutral());
    let mut runtime = runtime();
    runtime
        .reconcile_sources(&mut provider, ids.clone())
        .unwrap();
    assert_eq!(provider.opens, 2);
    assert_eq!(runtime.take_updates().unwrap().len(), 2);
    runtime.reconcile_sources(&mut provider, ids).unwrap();
    assert_eq!(
        provider.opens, 2,
        "unchanged discovery must not recreate sources"
    );
    event(&mut runtime, &one, 1, 0x130, 1).await;
    event(&mut runtime, &two, 3, 0, 12000).await;
    assert!(
        runtime.take_updates().unwrap().is_empty(),
        "only complete SYN_REPORT frames leave capture"
    );
    event(&mut runtime, &one, 0, 0, 0).await;
    event(&mut runtime, &two, 0, 0, 0).await;
    let updates = runtime.take_updates().unwrap();
    assert_eq!(
        updates,
        vec![
            SeatRequest::PhysicalState {
                device_id: "controller-0".into(),
                state: GamepadState {
                    buttons: 0x1000,
                    ..GamepadState::neutral()
                }
            },
            SeatRequest::PhysicalState {
                device_id: "controller-1".into(),
                state: GamepadState {
                    left_stick_x: 12000,
                    ..GamepadState::neutral()
                }
            },
        ]
    );
    event(&mut runtime, &one, 0, 0, 0).await;
    assert!(
        runtime.take_updates().unwrap().is_empty(),
        "no duplicate browser or old virtual-target route"
    );
}

#[tokio::test]
async fn initial_held_state_is_a_baseline_and_quiet_sources_refresh_the_complete_state() {
    let ids = identities();
    let held = GamepadState {
        buttons: 0x1100,
        left_trigger: 90,
        right_stick_y: -15000,
        ..GamepadState::neutral()
    };
    let mut provider = Provider::default();
    let _tx = provider.add(&ids[0], held);
    let mut runtime = runtime();
    runtime
        .reconcile_sources(&mut provider, vec![ids[0].clone()])
        .unwrap();
    assert!(
        matches!(&runtime.take_updates().unwrap()[0], SeatRequest::PhysicalConnected { state, .. } if *state == held)
    );
    runtime.heartbeat();
    assert!(
        matches!(&runtime.take_updates().unwrap()[0], SeatRequest::PhysicalState { state, .. } if *state == held)
    );
    assert!(dbus(&mut runtime, ":1.42", 0, "ui_guide", 1.0).is_empty());
    assert!(
        dbus(&mut runtime, ":1.42", 0, "ui_guide", 0.0).is_empty(),
        "held baseline must not fire shortcuts"
    );
}

#[tokio::test]
async fn source_loss_and_syn_dropped_revoke_only_that_source_then_requery_baseline() {
    let ids = identities();
    let mut provider = Provider::default();
    let one = provider.add(&ids[0], GamepadState::neutral());
    let two = provider.add(&ids[1], GamepadState::neutral());
    let mut runtime = runtime();
    runtime
        .reconcile_sources(&mut provider, ids.clone())
        .unwrap();
    runtime.take_updates().unwrap();
    event(&mut runtime, &one, 0, 3, 0).await;
    assert_eq!(
        runtime.take_updates().unwrap(),
        vec![SeatRequest::PhysicalDisconnected {
            device_id: "controller-0".into()
        }]
    );
    assert!(runtime.has_open_target());
    let held = GamepadState {
        buttons: 0x2000,
        ..GamepadState::neutral()
    };
    let _reopened = provider.add(&ids[0], held);
    runtime
        .reconcile_sources(&mut provider, ids.clone())
        .unwrap();
    assert!(
        matches!(&runtime.take_updates().unwrap()[0], SeatRequest::PhysicalConnected { state, .. } if *state == held)
    );
    drop(two);
    assert!(runtime.next_evdev_actions().await.unwrap().is_none());
    assert_eq!(
        runtime.take_updates().unwrap(),
        vec![SeatRequest::PhysicalDisconnected {
            device_id: "controller-1".into()
        }]
    );
}

#[tokio::test]
async fn owner_replacement_revokes_all_captures_and_old_dbus_shortcut_authority() {
    let ids = identities();
    let mut provider = Provider::default();
    let _one = provider.add(&ids[0], GamepadState::neutral());
    let _two = provider.add(&ids[1], GamepadState::neutral());
    let mut runtime = runtime();
    runtime.reconcile_sources(&mut provider, ids).unwrap();
    runtime.take_updates().unwrap();
    runtime.set_dbus_owner(Some(":1.43"));
    assert!(!runtime.has_open_target());
    assert_eq!(runtime.take_updates().unwrap().len(), 2);
    assert!(dbus(&mut runtime, ":1.42", 0, "ui_volume_up", 1.0).is_empty());
    assert!(dbus(&mut runtime, ":1.43", 0, "ui_volume_up", 1.0).is_empty());
}

#[tokio::test]
async fn renumbered_target_reconnects_with_same_raw_identity_and_open_fd_is_still_checked() {
    let mut ids = identities();
    let mut provider = Provider::default();
    let _first = provider.add(&ids[0], GamepadState::neutral());
    let mut runtime = runtime();
    runtime
        .reconcile_sources(&mut provider, vec![ids[0].clone()])
        .unwrap();
    runtime.take_updates().unwrap();
    ids[0].source.descriptor.path = "/dev/input/event80".into();
    ids[0].source.descriptor.device_number = Some(80);
    let _second = provider.add(&ids[0], GamepadState::neutral());
    runtime
        .reconcile_sources(&mut provider, vec![ids[0].clone()])
        .unwrap();
    let updates = runtime.take_updates().unwrap();
    assert!(
        matches!(&updates[0], SeatRequest::PhysicalDisconnected { device_id } if device_id == "controller-0")
    );
    assert!(
        matches!(&updates[1], SeatRequest::PhysicalConnected { device_id, .. } if device_id == "controller-0")
    );
    runtime.transport_lost();
    let _third = provider.add(&ids[0], GamepadState::neutral());
    provider
        .opened
        .get_mut(&ids[0].source.descriptor.path)
        .unwrap()
        .descriptor
        .device_number = Some(81);
    assert!(runtime
        .reconcile_sources(&mut provider, vec![ids[0].clone()])
        .is_err());
    assert!(!runtime.has_open_target());
    assert!(runtime.take_updates().unwrap().is_empty());
}

#[tokio::test]
async fn shortcut_policies_are_source_scoped_and_wrong_dbus_paths_have_no_authority() {
    let ids = identities();
    let mut provider = Provider::default();
    let _one = provider.add(&ids[0], GamepadState::neutral());
    let _two = provider.add(&ids[1], GamepadState::neutral());
    let mut runtime = runtime();
    runtime.reconcile_sources(&mut provider, ids).unwrap();
    assert!(dbus(&mut runtime, ":1.99", 0, "ui_volume_up", 1.0).is_empty());
    assert!(dbus(&mut runtime, ":1.42", 9, "ui_volume_up", 1.0).is_empty());
    assert!(dbus(&mut runtime, ":1.42", 0, "ui_guide", 1.0).is_empty());
    assert!(
        dbus(&mut runtime, ":1.42", 1, "ui_right", 1.0).is_empty(),
        "controllers cannot combine a chord"
    );
    let volume = dbus(&mut runtime, ":1.42", 1, "ui_volume_up", 1.0);
    assert_eq!(volume[0].id, ActionId::VolumeUp);
    // The full same-source chord works on either controller, regardless of the
    // game/portal route (which this capture runtime no longer owns).
    let matched = dbus(&mut runtime, ":1.42", 0, "ui_right", 1.0);
    assert!(!matched.is_empty());
}

#[tokio::test]
async fn exact_stop_still_requires_a_complete_authenticated_same_controller_dbus_chord() {
    let ids = identities();
    let mut provider = Provider::default();
    let _one = provider.add(&ids[0], GamepadState::neutral());
    let _two = provider.add(&ids[1], GamepadState::neutral());
    let mut runtime = runtime();
    runtime.reconcile_sources(&mut provider, ids).unwrap();
    for controller in [0, 1] {
        for capability in ["ui_l1", "ui_r1", "ui_option", "ui_select"] {
            dbus(&mut runtime, ":1.42", controller, capability, 0.0);
        }
    }
    for capability in ["ui_l1", "ui_r1"] {
        assert!(dbus(&mut runtime, ":1.42", 0, capability, 1.0).is_empty());
    }
    for capability in ["ui_option", "ui_select"] {
        assert!(dbus(&mut runtime, ":1.42", 1, capability, 1.0).is_empty());
    }
    assert!(dbus(&mut runtime, ":1.42", 0, "ui_option", 1.0).is_empty());
    let actions = dbus(&mut runtime, ":1.42", 0, "ui_select", 1.0);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id, ActionId::KillCurrentGame);
    assert_eq!(actions[0].dispatch_mode, DispatchMode::ExactStop);
}

fn remote(buttons: u32) -> RemoteSource {
    RemoteSource {
        launch_id: "launch-test".into(),
        controller_number: 0,
        state: GamepadState {
            buttons,
            ..GamepadState::neutral()
        },
        slot: None,
    }
}
#[test]
fn remote_shortcuts_work_for_overflow_but_never_gain_authenticated_dbus_exact_stop() {
    let mut runtime = runtime();
    runtime.handle_remote_event(RemoteEvent::Connected { source: remote(0) });
    // Home tap still opens the panel, including a connected overflow source.
    runtime.handle_remote_event(RemoteEvent::State {
        source: remote(0x0400),
    });
    let released = runtime.handle_remote_event(RemoteEvent::State { source: remote(0) });
    assert!(released
        .iter()
        .any(|action| action.id == ActionId::SystemPanel));
    let mut actions = Vec::new();
    for buttons in [0, 0x0100, 0x0300, 0x0310, 0x0330, 0] {
        actions.extend(runtime.handle_remote_event(RemoteEvent::State {
            source: remote(buttons),
        }));
    }
    assert!(actions
        .iter()
        .all(|action| action.dispatch_mode != DispatchMode::ExactStop));
    runtime.handle_remote_event(RemoteEvent::Disconnected {
        launch_id: "launch-test".into(),
        controller_number: 0,
    });
    assert!(runtime
        .handle_remote_event(RemoteEvent::State {
            source: remote(0x0400)
        })
        .is_empty());
}

#[test]
fn legacy_uniq_then_phys_is_derived_from_mapped_raw_source_and_duplicates_or_unknown_fail() {
    let root = tempfile::tempdir().unwrap();
    for event in [4, 7] {
        let caps = root
            .path()
            .join(format!("class/input/event{event}/device/capabilities"));
        std::fs::create_dir_all(&caps).unwrap();
        std::fs::write(caps.join("key"), "f00000000 0 0 0 7cdb000000400000 0 0 0 0").unwrap();
        std::fs::write(caps.join("abs"), "30003f").unwrap();
    }
    let classifier = SysfsSourceClassifier::new(root.path());
    let ids = identities();
    let mut devices = ids
        .iter()
        .map(|source| source.raw.clone())
        .collect::<Vec<_>>();
    let sources = PhysicalSources {
        owner: ":1.42".into(),
        controllers: ids.iter().map(|source| source.source.clone()).collect(),
    };
    assert_eq!(
        identify_sources(&sources, &devices, &classifier)
            .unwrap()
            .iter()
            .map(|source| source.device_id.as_str())
            .collect::<Vec<_>>(),
        vec!["controller-0", "controller-1"]
    );
    devices[0].unique_id = None;
    assert_eq!(
        identify_sources(&sources, &devices, &classifier).unwrap()[0].device_id,
        "usb-0000:01:00.0-1/input0"
    );
    devices[1].unique_id = None;
    assert!(
        identify_sources(&sources, &devices, &classifier).is_err(),
        "duplicate phys never merges distinct controllers"
    );
    devices[1].unique_id = Some("controller-1".into());
    devices[0].physical_path = None;
    assert!(
        identify_sources(&sources, &devices, &classifier).is_err(),
        "eventN must not become reconnect identity"
    );
    devices[0].unique_id = Some("controller-0".into());
    std::fs::remove_file(
        root.path()
            .join("class/input/event4/device/capabilities/key"),
    )
    .unwrap();
    assert!(
        identify_sources(&sources, &devices, &classifier).is_err(),
        "single Unknown is not identity proof"
    );
}

#[tokio::test]
async fn centered_nonzero_axes_arm_initial_and_released_physical_baselines_without_rewriting_values(
) {
    for held in [false, true] {
        let ids = identities();
        let centered = GamepadState {
            left_stick_x: 4096,
            left_stick_y: -4096,
            right_stick_x: 71,
            right_stick_y: -23,
            ..GamepadState::neutral()
        };
        let initial = GamepadState {
            buttons: if held { 0x1000 } else { 0 },
            ..centered
        };
        let mut provider = Provider::default();
        let tx = provider.add(&ids[0], initial);
        let mut runtime = runtime();
        runtime
            .reconcile_sources(&mut provider, vec![ids[0].clone()])
            .unwrap();
        assert!(
            matches!(&runtime.take_updates().unwrap()[0], SeatRequest::PhysicalConnected { state, .. } if *state == initial)
        );
        if held {
            assert!(dbus(&mut runtime, ":1.42", 0, "ui_volume_up", 1.0).is_empty());
            event(&mut runtime, &tx, 1, 0x130, 0).await;
            event(&mut runtime, &tx, 0, 0, 0).await;
            assert!(
                matches!(&runtime.take_updates().unwrap()[0], SeatRequest::PhysicalState { state, .. } if *state == centered)
            );
        }
        assert_eq!(
            dbus(&mut runtime, ":1.42", 0, "ui_volume_up", 1.0)[0].id,
            ActionId::VolumeUp
        );
        runtime.heartbeat();
        assert!(
            matches!(&runtime.take_updates().unwrap()[0], SeatRequest::PhysicalState { state, .. } if *state == centered)
        );
    }
}

#[tokio::test]
async fn deflected_stick_or_held_trigger_must_release_before_physical_rearm() {
    for (code, initial) in [
        (
            0,
            GamepadState {
                left_stick_x: 4097,
                ..GamepadState::neutral()
            },
        ),
        (
            2,
            GamepadState {
                left_trigger: 1,
                ..GamepadState::neutral()
            },
        ),
    ] {
        let ids = identities();
        let mut provider = Provider::default();
        let tx = provider.add(&ids[0], initial);
        let mut runtime = runtime();
        runtime
            .reconcile_sources(&mut provider, vec![ids[0].clone()])
            .unwrap();
        assert!(dbus(&mut runtime, ":1.42", 0, "ui_volume_up", 1.0).is_empty());
        event(&mut runtime, &tx, 3, code, if code == 0 { 123 } else { 0 }).await;
        event(&mut runtime, &tx, 0, 0, 0).await;
        assert_eq!(
            dbus(&mut runtime, ":1.42", 0, "ui_volume_up", 1.0)[0].id,
            ActionId::VolumeUp
        );
    }
}

#[test]
fn remote_centered_nonzero_axes_rearm_only_after_held_baseline_releases() {
    for held in [false, true] {
        let centered = GamepadState {
            left_stick_x: 4096,
            right_stick_y: -4096,
            ..GamepadState::neutral()
        };
        let mut runtime = runtime();
        let source = |state| RemoteSource { state, ..remote(0) };
        runtime.handle_remote_event(RemoteEvent::Connected {
            source: source(GamepadState {
                buttons: if held { 0x1000 } else { 0 },
                ..centered
            }),
        });
        if held {
            assert!(runtime
                .handle_remote_event(RemoteEvent::State {
                    source: source(GamepadState {
                        buttons: 0x1400,
                        ..centered
                    })
                })
                .is_empty());
            assert!(runtime
                .handle_remote_event(RemoteEvent::State {
                    source: source(centered)
                })
                .is_empty());
        }
        runtime.handle_remote_event(RemoteEvent::State {
            source: source(GamepadState {
                buttons: 0x0400,
                ..centered
            }),
        });
        let actions = runtime.handle_remote_event(RemoteEvent::State {
            source: source(centered),
        });
        assert!(actions
            .iter()
            .any(|action| action.id == ActionId::SystemPanel));
    }
}

#[test]
fn post_open_validation_rejects_raw_replacement_and_same_owner_provider_remapping() {
    use korri_inputd::capture::validate_after_open;
    let root = tempfile::tempdir().unwrap();
    for event in [4, 7] {
        let caps = root
            .path()
            .join(format!("class/input/event{event}/device/capabilities"));
        std::fs::create_dir_all(&caps).unwrap();
        std::fs::write(caps.join("key"), "f00000000 0 0 0 7cdb000000400000 0 0 0 0").unwrap();
        std::fs::write(caps.join("abs"), "30003f").unwrap();
    }
    let classifier = SysfsSourceClassifier::new(root.path());
    let ids = identities();
    let devices = ids
        .iter()
        .map(|source| source.raw.clone())
        .collect::<Vec<_>>();
    let before = PhysicalSources {
        owner: ":1.42".into(),
        controllers: ids.iter().map(|source| source.source.clone()).collect(),
    };
    assert!(validate_after_open(&before, &before, &ids, &devices, &classifier).is_ok());
    let mut raw_replaced = devices.clone();
    raw_replaced[0].unique_id = Some("replacement-controller".into());
    assert!(validate_after_open(&before, &before, &ids, &raw_replaced, &classifier).is_err());
    for mutation in 0..4 {
        let mut after = before.clone();
        match mutation {
            0 => after.owner = ":1.43".into(),
            1 => {
                after.controllers[0].source_device_paths =
                    before.controllers[1].source_device_paths.clone()
            }
            2 => after.controllers[0].descriptor = before.controllers[1].descriptor.clone(),
            _ => after.controllers[0].dbus_paths = before.controllers[1].dbus_paths.clone(),
        }
        assert!(validate_after_open(&before, &after, &ids, &devices, &classifier).is_err());
    }
}

#[test]
fn normalized_state_preserves_signed_axes_triggers_and_hat_without_wrapping() {
    let mut state = GamepadState::neutral();
    for (code, value) in [
        (0, -32768),
        (1, 32767),
        (2, 255),
        (3, 20000),
        (4, -10000),
        (5, 128),
        (16, -1),
        (17, 1),
    ] {
        apply_event(&mut state, 3, code, value).unwrap();
    }
    assert_eq!(
        state,
        GamepadState {
            buttons: 6,
            left_trigger: 255,
            right_trigger: 128,
            left_stick_x: -32768,
            left_stick_y: 32767,
            right_stick_x: 20000,
            right_stick_y: -10000
        }
    );
    assert!(apply_event(&mut state, 3, 2, 256).is_err());
    assert!(apply_event(&mut state, 3, 0, 32768).is_err());
    assert!(apply_event(&mut state, 3, 16, -2).is_err());
}

fn topology() -> Vec<DeviceDescriptor> {
    parse_proc_bus_input_devices(
        include_str!("fixtures/proc-bus-input/one-virtual-one-raw.txt"),
        Path::new("/dev/input"),
    )
}

// A settled pass lets the 1 Hz reconcile skip its DBus round trips, which
// otherwise hold the event loop and delay every controller read behind them.
#[tokio::test]
async fn unchanged_topology_after_a_captured_pass_is_settled_until_any_change() {
    let ids = identities();
    let mut provider = Provider::default();
    let _one = provider.add(&ids[0], GamepadState::neutral());
    let mut runtime = runtime();
    let devices = topology();
    assert!(
        !runtime.topology_settled(&devices),
        "no pass has settled yet"
    );
    runtime
        .reconcile_sources(&mut provider, vec![ids[0].clone()])
        .unwrap();
    runtime.record_settled_topology(devices.clone());
    assert!(runtime.topology_settled(&devices));
    let mut added = devices.clone();
    added.push(devices[0].clone());
    added.last_mut().unwrap().path = "/dev/input/event99".into();
    assert!(
        !runtime.topology_settled(&added),
        "a new input device needs a full pass"
    );
    assert!(
        !runtime.topology_settled(&devices[1..]),
        "a removed input device needs a full pass"
    );
}

#[tokio::test]
async fn nothing_settles_without_an_open_capture() {
    let mut runtime = runtime();
    let devices = topology();
    runtime.record_settled_topology(devices.clone());
    assert!(!runtime.topology_settled(&devices));
}

#[tokio::test]
async fn source_loss_owner_change_and_transport_loss_unsettle_the_topology() {
    let ids = identities();
    let devices = topology();
    for case in 0..5 {
        let mut provider = Provider::default();
        let one = provider.add(&ids[0], GamepadState::neutral());
        let mut runtime = runtime();
        runtime
            .reconcile_sources(&mut provider, vec![ids[0].clone()])
            .unwrap();
        runtime.record_settled_topology(devices.clone());
        match case {
            0 => event(&mut runtime, &one, 0, 3, 0).await, // SYN_DROPPED
            1 => {
                drop(one);
                assert!(runtime.next_evdev_actions().await.unwrap().is_none());
            }
            2 => runtime.set_dbus_owner(Some(":1.43")),
            3 => runtime.transport_lost(),
            _ => runtime.source_ambiguous(),
        }
        assert!(
            !runtime.topology_settled(&devices),
            "case {case} must force a full pass"
        );
    }
}
