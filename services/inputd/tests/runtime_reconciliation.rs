use std::{
    collections::VecDeque,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use evdev::{EventType, InputEvent, SynchronizationCode};
use futures_util::stream;
use korri_input_core::controls::{Control, ControlTransition};
use korri_inputd::{
    action_catalog::{ActionId, ActionRoutes, DispatchMode, Trigger, ACTION_CATALOG},
    dbus::{Signal, DBUS_INPUT_MEMBER, DBUS_TARGET_INTERFACE, DBUS_TARGET_PATH},
    devices::{
        parse_proc_bus_input_devices, resolve_target, DeviceCapabilities, DeviceClass,
        DeviceDescriptor, DeviceInputId, OpenedTarget, TargetProvider, TargetResolution,
        XB360_TARGET_NAME,
    },
    runtime::{RecoveryReason, Runtime, RuntimeState},
    virtual_targets::{InputOwner, TargetRouter},
};

#[derive(Default)]
struct RoutedEvents {
    game: Vec<InputEvent>,
    portal: Vec<InputEvent>,
}

struct SharedRouter {
    owner: InputOwner,
    routed: Arc<Mutex<RoutedEvents>>,
}

impl TargetRouter for SharedRouter {
    fn set_owner(&mut self, owner: InputOwner) {
        self.owner = owner;
    }

    fn owner(&self) -> InputOwner {
        self.owner
    }

    fn route(&mut self, event: InputEvent) -> io::Result<()> {
        let mut routed = self.routed.lock().unwrap();
        match self.owner {
            InputOwner::Game => routed.game.push(event),
            InputOwner::Portal => routed.portal.push(event),
        }
        Ok(())
    }
}

struct InMemoryTargetProvider {
    devices: Vec<DeviceDescriptor>,
    opened: VecDeque<io::Result<DeviceDescriptor>>,
    end_stream_on_open: bool,
    drop_sync_on_open: bool,
    events_on_open: Vec<io::Result<InputEvent>>,
    routed: Arc<Mutex<RoutedEvents>>,
    enumerate_calls: usize,
    open_calls: usize,
}

impl InMemoryTargetProvider {
    fn with(devices: Vec<DeviceDescriptor>) -> Self {
        Self {
            devices,
            opened: VecDeque::new(),
            end_stream_on_open: false,
            drop_sync_on_open: false,
            events_on_open: Vec::new(),
            routed: Arc::new(Mutex::new(RoutedEvents::default())),
            enumerate_calls: 0,
            open_calls: 0,
        }
    }

    fn open_as(&mut self, descriptor: io::Result<DeviceDescriptor>) {
        self.opened.push_back(descriptor);
    }

    fn end_stream_on_open(&mut self) {
        self.end_stream_on_open = true;
    }

    fn drop_sync_on_open(&mut self) {
        self.drop_sync_on_open = true;
    }

    fn route_on_open(&mut self, events: impl IntoIterator<Item = InputEvent>) {
        self.events_on_open = events.into_iter().map(Ok).collect();
    }
}

impl TargetProvider for InMemoryTargetProvider {
    fn enumerate(&mut self) -> io::Result<Vec<DeviceDescriptor>> {
        self.enumerate_calls += 1;
        Ok(self.devices.clone())
    }

    fn open(&mut self, expected: &DeviceDescriptor) -> io::Result<OpenedTarget> {
        self.open_calls += 1;
        let descriptor = self
            .opened
            .pop_front()
            .unwrap_or_else(|| Ok(expected.clone()))?;
        let events = if self.end_stream_on_open {
            self.end_stream_on_open = false;
            Box::pin(stream::empty()) as korri_inputd::devices::InputEventStream
        } else if self.drop_sync_on_open {
            self.drop_sync_on_open = false;
            Box::pin(stream::iter([Ok(InputEvent::new(
                EventType::SYNCHRONIZATION.0,
                SynchronizationCode::SYN_DROPPED.0,
                0,
            ))])) as korri_inputd::devices::InputEventStream
        } else if !self.events_on_open.is_empty() {
            Box::pin(stream::iter(std::mem::take(&mut self.events_on_open)))
                as korri_inputd::devices::InputEventStream
        } else {
            Box::pin(stream::pending()) as korri_inputd::devices::InputEventStream
        };
        Ok(OpenedTarget {
            descriptor,
            events,
            router: Box::new(SharedRouter {
                owner: InputOwner::Portal,
                routed: Arc::clone(&self.routed),
            }),
        })
    }
}

fn target(node: &str) -> DeviceDescriptor {
    DeviceDescriptor {
        path: PathBuf::from(format!("/dev/input/{node}")),
        name: XB360_TARGET_NAME.to_owned(),
        physical_path: None,
        unique_id: None,
        sysfs_path: Some("/devices/virtual/input/input20".to_owned()),
        input_id: DeviceInputId {
            bus: 3,
            vendor: 0x045e,
            product: 0x028e,
            version: 0x0001,
        },
        capabilities: DeviceCapabilities::inputplumber_xb360(),
        class: DeviceClass::Gamepad,
        device_number: Some(100),
    }
}

fn raw(node: &str) -> DeviceDescriptor {
    DeviceDescriptor {
        path: PathBuf::from(format!("/dev/input/{node}")),
        name: "Xbox Wireless Controller".to_owned(),
        physical_path: Some("usb-0000:01".to_owned()),
        unique_id: None,
        sysfs_path: Some("/devices/pci/usb/input/input9".to_owned()),
        input_id: DeviceInputId::default(),
        capabilities: DeviceCapabilities::default(),
        class: DeviceClass::Gamepad,
        device_number: Some(9),
    }
}

fn ready_runtime(provider: &mut InMemoryTargetProvider) -> Runtime {
    ready_runtime_with_routes(provider, ActionRoutes::default())
}

fn ready_runtime_with_routes(
    provider: &mut InMemoryTargetProvider,
    routes: ActionRoutes,
) -> Runtime {
    let mut runtime = Runtime::with_action_routes(routes);
    runtime.set_dbus_owner(Some(":1.42"));
    runtime.reconcile(provider);
    assert!(matches!(runtime.state(), RuntimeState::Ready { .. }));
    runtime
}

fn send_dbus(
    runtime: &mut Runtime,
    control: Control,
    transition: ControlTransition,
) -> Vec<korri_inputd::runtime::RuntimeAction> {
    send_dbus_at(runtime, control, transition, 0)
}

fn send_dbus_at(
    runtime: &mut Runtime,
    control: Control,
    transition: ControlTransition,
    now_ms: u64,
) -> Vec<korri_inputd::runtime::RuntimeAction> {
    let capability = match control {
        Control::Home => "ui_guide",
        Control::L1 => "ui_l1",
        Control::R1 => "ui_r1",
        Control::Start => "ui_option",
        Control::Select => "ui_select",
        _ => panic!("test helper has no DBus capability for {control:?}"),
    };
    let owner = runtime.dbus_owner().expect("DBus owner").to_owned();
    runtime.handle_dbus_signal_at(
        &Signal {
            sender: &owner,
            path: DBUS_TARGET_PATH,
            interface: DBUS_TARGET_INTERFACE,
            member: DBUS_INPUT_MEMBER,
            capability,
            value: if transition == ControlTransition::Pressed {
                1.0
            } else {
                0.0
            },
        },
        now_ms,
    )
}

fn release_destructive(runtime: &mut Runtime) {
    for control in [Control::L1, Control::R1, Control::Start, Control::Select] {
        assert!(send_dbus(runtime, control, ControlTransition::Released).is_empty());
    }
}

#[tokio::test]
async fn normalized_events_route_only_to_the_current_owner() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event7")]);
    provider.route_on_open([
        InputEvent::new(EventType::KEY.0, 0x130, 1),
        InputEvent::new(EventType::KEY.0, 0x130, 0),
    ]);
    let routed = Arc::clone(&provider.routed);
    let mut runtime = ready_runtime(&mut provider);

    assert_eq!(runtime.input_owner(), InputOwner::Portal);
    runtime.next_evdev_actions().await.unwrap();
    runtime.set_input_owner(InputOwner::Game);
    runtime.next_evdev_actions().await.unwrap();

    let routed = routed.lock().unwrap();
    assert_eq!(routed.portal.len(), 1);
    assert_eq!(routed.portal[0].value(), 1);
    assert_eq!(routed.game.len(), 1);
    assert_eq!(routed.game[0].value(), 0);
}

#[tokio::test]
async fn selected_owner_survives_source_reopen_without_persisted_state() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event7")]);
    let routed = Arc::clone(&provider.routed);
    let mut runtime = ready_runtime(&mut provider);
    runtime.set_input_owner(InputOwner::Game);

    provider.devices = vec![target("event8")];
    provider.route_on_open([InputEvent::new(EventType::KEY.0, 0x130, 1)]);
    runtime.reconcile(&mut provider);
    runtime.next_evdev_actions().await.unwrap();

    let routed = routed.lock().unwrap();
    assert_eq!(routed.game.len(), 1);
    assert!(routed.portal.is_empty());
}

#[tokio::test]
async fn ownership_switch_does_not_replay_inactive_target_events() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event7")]);
    provider.route_on_open([InputEvent::new(EventType::KEY.0, 0x130, 1)]);
    let routed = Arc::clone(&provider.routed);
    let mut runtime = ready_runtime(&mut provider);

    runtime.set_input_owner(InputOwner::Game);
    runtime.set_input_owner(InputOwner::Portal);
    runtime.next_evdev_actions().await.unwrap();

    let routed = routed.lock().unwrap();
    assert!(routed.game.is_empty());
    assert_eq!(routed.portal.len(), 1);
}

#[test]
fn proc_fixture_resolves_only_the_exact_virtual_target() {
    let fixture = include_str!("fixtures/proc-bus-input/one-virtual-one-raw.txt");
    let devices = parse_proc_bus_input_devices(fixture, std::path::Path::new("/fixture/input"));

    assert_eq!(devices.len(), 4);
    let TargetResolution::Found(selected) = resolve_target(&devices) else {
        panic!("fixture must resolve exactly one normalized target");
    };
    assert_eq!(selected.path, PathBuf::from("/fixture/input/event10"));
    assert_eq!(selected.input_id.bus, 3);
    assert_eq!(selected.input_id.vendor, 0x045e);
    assert_eq!(selected.input_id.product, 0x028e);
    assert_eq!(selected.input_id.version, 0x0001);
    assert_eq!(selected.physical_path, None);
    assert_eq!(selected.unique_id, None);
    assert_eq!(
        selected.capabilities,
        DeviceCapabilities::inputplumber_xb360()
    );
    assert!(!devices[1].is_validated_inputplumber_xb360());
    assert_eq!(devices[2].unique_id, None);
}

#[test]
fn selects_one_virtual_xb360_and_never_falls_back_to_raw_gamepads() {
    let virtual_target = target("event10");
    let mut provider = InMemoryTargetProvider::with(vec![raw("event4"), virtual_target.clone()]);
    let runtime = ready_runtime(&mut provider);

    assert_eq!(
        runtime.state(),
        &RuntimeState::Ready {
            target: korri_inputd::runtime::ReadyTarget {
                identity: virtual_target.stable_identity(),
                path: virtual_target.path,
            }
        }
    );
    assert_eq!(provider.open_calls, 1);

    let mut raw_only = InMemoryTargetProvider::with(vec![raw("event4")]);
    let mut runtime = Runtime::default();
    runtime.set_dbus_owner(Some(":1.42"));
    runtime.reconcile(&mut raw_only);
    assert_eq!(runtime.state(), &RuntimeState::Missing { raw_gamepads: 1 });
    assert_eq!(raw_only.open_calls, 0);
}

#[test]
fn duplicated_non_destructive_controls_dispatch_once_per_chord_lifecycle() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    let mut runtime = ready_runtime(&mut provider);

    assert!(send_dbus(&mut runtime, Control::Home, ControlTransition::Pressed).is_empty());
    let first = runtime.handle_evdev(1, 0x136, 1);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, ActionId::WorkspacePrev);
    assert_eq!(first[0].dispatch_mode, DispatchMode::Direct);
    assert!(send_dbus(&mut runtime, Control::L1, ControlTransition::Pressed).is_empty());

    assert!(runtime.handle_evdev(1, 0x136, 0).is_empty());
    assert!(send_dbus(&mut runtime, Control::L1, ControlTransition::Released).is_empty());
    assert!(send_dbus(&mut runtime, Control::Home, ControlTransition::Released).is_empty());

    assert!(send_dbus(&mut runtime, Control::Home, ControlTransition::Pressed).is_empty());
    let second = send_dbus(&mut runtime, Control::L1, ControlTransition::Pressed);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, ActionId::WorkspacePrev);
}

fn press_control(
    runtime: &mut Runtime,
    control: Control,
) -> Vec<korri_inputd::runtime::RuntimeAction> {
    match control {
        Control::DpadLeft => runtime.handle_evdev(3, 16, -1),
        Control::DpadRight => runtime.handle_evdev(3, 16, 1),
        Control::DpadUp => runtime.handle_evdev(3, 17, -1),
        Control::DpadDown => runtime.handle_evdev(3, 17, 1),
        _ => runtime.handle_evdev(
            1,
            match control {
                Control::Home => 0x13c,
                Control::L1 => 0x136,
                Control::R1 => 0x137,
                Control::Start => 0x13b,
                Control::Select => 0x13a,
                Control::L3 => 0x13d,
                Control::R3 => 0x13e,
                Control::Back => 0x116,
                Control::X => 0x133,
                Control::VolumeUp => 0x73,
                Control::VolumeDown => 0x72,
                Control::DpadUp | Control::DpadDown | Control::DpadLeft | Control::DpadRight => {
                    unreachable!()
                }
            },
            1,
        ),
    }
}

#[test]
fn legacy_controller_action_matrix_is_reachable_and_catalog_typed() {
    for entry in ACTION_CATALOG {
        let routes = match entry.trigger {
            Trigger::ConfiguredBackTap => ActionRoutes {
                back_tap: Some(entry.id),
            },
            _ => ActionRoutes::default(),
        };
        let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
        let mut runtime = ready_runtime_with_routes(&mut provider, routes);
        let mut emitted = Vec::new();
        match entry.trigger {
            Trigger::Tap(control) => {
                emitted.extend(press_control(&mut runtime, control));
                let code = if control == Control::Home {
                    0x13c
                } else {
                    0x116
                };
                emitted.extend(runtime.handle_evdev(1, code, 0));
            }
            Trigger::Press(control) => emitted.extend(press_control(&mut runtime, control)),
            Trigger::Chords(chords) | Trigger::ChordsAndConfiguredBackTap(chords) => {
                let chord = chords[0];
                if entry.dispatch_mode == DispatchMode::ExactStop {
                    for control in chord.controls {
                        let _ = send_dbus(&mut runtime, *control, ControlTransition::Released);
                    }
                    for control in chord.controls {
                        emitted.extend(send_dbus_at(
                            &mut runtime,
                            *control,
                            ControlTransition::Pressed,
                            0,
                        ));
                    }
                } else {
                    for control in chord.controls {
                        emitted.extend(press_control(&mut runtime, *control));
                    }
                }
            }
            Trigger::ConfiguredBackTap => {
                emitted.extend(press_control(&mut runtime, Control::Back));
                emitted.extend(runtime.handle_evdev(1, 0x116, 0));
            }
            Trigger::Unsupported => {
                assert!(emitted.is_empty());
                continue;
            }
        }
        assert!(
            emitted.iter().any(|action| action.id == entry.id),
            "catalog route was unreachable: {}",
            entry.id
        );
        assert!(emitted.iter().all(|action| {
            ACTION_CATALOG
                .iter()
                .any(|known| known.id == action.id && known.dispatch_mode == action.dispatch_mode)
        }));
    }
}

#[test]
fn reconciliation_is_finite_and_opens_at_most_one_validated_target_per_tick() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    let mut runtime = Runtime::default();
    runtime.set_dbus_owner(Some(":1.42"));

    runtime.reconcile(&mut provider);
    runtime.reconcile(&mut provider);

    assert_eq!(provider.enumerate_calls, 2);
    assert_eq!(provider.open_calls, 1);
}

#[test]
fn ambiguity_closes_the_stream_clears_held_state_and_recovers_after_hotplug() {
    let first = target("event10");
    let mut provider = InMemoryTargetProvider::with(vec![first.clone()]);
    let mut runtime = ready_runtime(&mut provider);
    assert!(runtime.handle_evdev(1, 0x13c, 1).is_empty());

    let mut second = target("event11");
    second.device_number = Some(101);
    provider.devices.push(second);
    runtime.reconcile(&mut provider);
    assert!(matches!(runtime.state(), RuntimeState::Ambiguous { .. }));
    assert!(runtime.handle_evdev(1, 0x136, 1).is_empty());

    provider.devices = vec![first];
    runtime.reconcile(&mut provider);
    assert!(matches!(runtime.state(), RuntimeState::Ready { .. }));
    assert!(runtime.handle_evdev(1, 0x136, 1).is_empty());
}

#[test]
fn missing_source_overrides_a_stale_target_and_clears_held_state() {
    let first = target("event10");
    let mut provider = InMemoryTargetProvider::with(vec![first]);
    let mut runtime = ready_runtime(&mut provider);
    assert!(runtime.handle_evdev(1, 0x13c, 1).is_empty());

    runtime.source_missing();
    assert_eq!(runtime.state(), &RuntimeState::Missing { raw_gamepads: 0 });
    assert!(!runtime.has_open_target());
    assert!(runtime.handle_evdev(1, 0x136, 1).is_empty());

    runtime.reconcile(&mut provider);
    assert!(matches!(runtime.state(), RuntimeState::Ready { .. }));
    assert!(runtime.handle_evdev(1, 0x136, 1).is_empty());
}

#[test]
fn ambiguous_source_topology_fails_closed_and_clears_held_state() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    let mut runtime = ready_runtime(&mut provider);
    assert!(runtime.handle_evdev(1, 0x13c, 1).is_empty());

    runtime.source_ambiguous();
    assert_eq!(
        runtime.state(),
        &RuntimeState::Recovering {
            reason: RecoveryReason::SourceTopologyAmbiguous
        }
    );
    assert!(!runtime.has_open_target());
    assert!(runtime.handle_evdev(1, 0x136, 1).is_empty());
}

#[test]
fn renumbered_target_reopens_without_carrying_held_controls() {
    let first = target("event10");
    let mut provider = InMemoryTargetProvider::with(vec![first]);
    let mut runtime = ready_runtime(&mut provider);
    assert!(runtime.handle_evdev(1, 0x13c, 1).is_empty());

    let mut renumbered = target("event14");
    renumbered.device_number = Some(114);
    provider.devices = vec![renumbered.clone()];
    runtime.reconcile(&mut provider);

    assert_eq!(provider.open_calls, 2);
    assert_eq!(
        runtime.state(),
        &RuntimeState::Ready {
            target: korri_inputd::runtime::ReadyTarget {
                identity: renumbered.stable_identity(),
                path: renumbered.path,
            }
        }
    );
    assert!(runtime.handle_evdev(1, 0x136, 1).is_empty());
}

#[test]
fn unreadable_required_target_is_retried_but_irrelevant_devices_are_not_opened() {
    let required = target("event10");
    let mut provider = InMemoryTargetProvider::with(vec![raw("event2"), required]);
    provider.open_as(Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "fixture denial",
    )));
    let mut runtime = Runtime::default();
    runtime.set_dbus_owner(Some(":1.42"));
    runtime.reconcile(&mut provider);

    assert_eq!(
        runtime.state(),
        &RuntimeState::Recovering {
            reason: RecoveryReason::RequiredTargetUnreadable
        }
    );
    assert_eq!(provider.open_calls, 1);

    runtime.reconcile(&mut provider);
    assert!(matches!(runtime.state(), RuntimeState::Ready { .. }));
    assert_eq!(provider.open_calls, 2);
}

#[test]
fn descriptor_provenance_rejects_path_replacement_between_enumeration_and_open() {
    let expected = target("event10");
    let mut replacement = expected.clone();
    replacement.sysfs_path = Some("/devices/pci/usb/input/input9".to_owned());
    let mut provider = InMemoryTargetProvider::with(vec![expected]);
    provider.open_as(Ok(replacement));
    let mut runtime = Runtime::default();
    runtime.set_dbus_owner(Some(":1.42"));

    runtime.reconcile(&mut provider);

    assert_eq!(
        runtime.state(),
        &RuntimeState::Recovering {
            reason: RecoveryReason::DescriptorChangedAfterOpen
        }
    );
}

#[tokio::test]
async fn stream_loss_clears_state_and_requires_reconciliation_and_release_before_destructive_input()
{
    let required = target("event10");
    let mut provider = InMemoryTargetProvider::with(vec![required]);
    provider.end_stream_on_open();
    let mut runtime = ready_runtime(&mut provider);
    release_destructive(&mut runtime);
    for control in [Control::L1, Control::R1, Control::Start] {
        send_dbus(&mut runtime, control, ControlTransition::Pressed);
    }

    assert!(matches!(runtime.next_evdev_actions().await, Ok(None)));
    assert_eq!(
        runtime.state(),
        &RuntimeState::Recovering {
            reason: RecoveryReason::EventStreamLost
        }
    );
    runtime.reconcile(&mut provider);
    assert!(send_dbus(&mut runtime, Control::Select, ControlTransition::Pressed).is_empty());
    release_destructive(&mut runtime);
    for control in [Control::L1, Control::R1, Control::Start] {
        send_dbus(&mut runtime, control, ControlTransition::Pressed);
    }
    let actions = send_dbus_at(
        &mut runtime,
        Control::Select,
        ControlTransition::Pressed,
        1_000,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].dispatch_mode, DispatchMode::ExactStop);
}

#[test]
fn exact_stop_fires_on_activation_once_and_suppresses_repeats_until_release() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    let mut runtime = ready_runtime(&mut provider);
    release_destructive(&mut runtime);
    for control in [Control::L1, Control::R1, Control::Start] {
        assert!(send_dbus_at(&mut runtime, control, ControlTransition::Pressed, 1_000).is_empty());
    }
    let first = send_dbus_at(
        &mut runtime,
        Control::Select,
        ControlTransition::Pressed,
        1_000,
    );
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, ActionId::KillCurrentGame);
    assert_eq!(first[0].dispatch_mode, DispatchMode::ExactStop);
    assert!(send_dbus_at(
        &mut runtime,
        Control::Select,
        ControlTransition::Pressed,
        1_001,
    )
    .is_empty());
    assert!(runtime.advance_actions_at(4_000).is_empty());
    assert!(send_dbus_at(
        &mut runtime,
        Control::Select,
        ControlTransition::Released,
        4_001,
    )
    .is_empty());
    let second = send_dbus_at(
        &mut runtime,
        Control::Select,
        ControlTransition::Pressed,
        4_002,
    );
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, ActionId::KillCurrentGame);
}

#[test]
fn provider_owner_change_closes_target_and_requires_reconciliation() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    let mut runtime = ready_runtime(&mut provider);
    release_destructive(&mut runtime);

    runtime.set_dbus_owner(Some(":1.99"));
    assert_eq!(
        runtime.state(),
        &RuntimeState::Recovering {
            reason: RecoveryReason::ProviderUnavailable
        }
    );
    assert!(!runtime.has_open_target());
    assert!(send_dbus(&mut runtime, Control::Home, ControlTransition::Pressed).is_empty());

    runtime.reconcile(&mut provider);
    assert!(matches!(runtime.state(), RuntimeState::Ready { .. }));
    assert_eq!(provider.open_calls, 2);
    for control in [Control::L1, Control::R1, Control::Start, Control::Select] {
        assert!(send_dbus(&mut runtime, control, ControlTransition::Pressed).is_empty());
    }
    release_destructive(&mut runtime);
    for control in [Control::L1, Control::R1, Control::Start] {
        assert!(send_dbus(&mut runtime, control, ControlTransition::Pressed).is_empty());
    }
    let actions = send_dbus(&mut runtime, Control::Select, ControlTransition::Pressed);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].dispatch_mode, DispatchMode::ExactStop);
}

#[test]
fn owner_lookup_failure_leaves_runtime_inert_until_a_bounded_retry_succeeds() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    let mut runtime = ready_runtime(&mut provider);

    runtime.set_dbus_owner(None);
    runtime.reconcile(&mut provider);

    assert_eq!(
        runtime.state(),
        &RuntimeState::Recovering {
            reason: RecoveryReason::ProviderUnavailable
        }
    );
    assert!(!runtime.has_open_target());
    assert_eq!(provider.open_calls, 1);
    assert!(runtime.handle_evdev(1, 0x13c, 1).is_empty());

    runtime.set_dbus_owner(Some(":1.99"));
    runtime.reconcile(&mut provider);
    assert!(matches!(runtime.state(), RuntimeState::Ready { .. }));
    assert_eq!(provider.open_calls, 2);
}

#[test]
fn same_owner_refresh_does_not_close_target_or_clear_held_state() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    let mut runtime = ready_runtime(&mut provider);
    assert!(send_dbus(&mut runtime, Control::Home, ControlTransition::Pressed).is_empty());

    runtime.set_dbus_owner(Some(":1.42"));
    runtime.reconcile(&mut provider);

    assert!(runtime.has_open_target());
    assert_eq!(provider.open_calls, 1);
    let actions = send_dbus(&mut runtime, Control::L1, ControlTransition::Pressed);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id, ActionId::WorkspacePrev);
}

#[tokio::test]
async fn syn_dropped_closes_target_and_clears_stale_pressed_state() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    provider.drop_sync_on_open();
    let mut runtime = ready_runtime(&mut provider);
    assert!(runtime.handle_evdev(1, 0x13c, 1).is_empty());

    assert!(matches!(runtime.next_evdev_actions().await, Ok(Some(actions)) if actions.is_empty()));
    assert_eq!(
        runtime.state(),
        &RuntimeState::Recovering {
            reason: RecoveryReason::EventStreamLost
        }
    );
    assert!(!runtime.has_open_target());

    runtime.reconcile(&mut provider);
    assert!(runtime.handle_evdev(1, 0x136, 1).is_empty());
}

#[test]
fn evdev_only_and_mixed_sources_cannot_complete_destructive_input() {
    let mut provider = InMemoryTargetProvider::with(vec![target("event10")]);
    let mut runtime = ready_runtime(&mut provider);
    release_destructive(&mut runtime);

    for code in [0x136, 0x137, 0x13b, 0x13a] {
        assert!(runtime.handle_evdev(1, code, 1).is_empty());
    }
    runtime.event_stream_lost();
    runtime.reconcile(&mut provider);
    release_destructive(&mut runtime);
    send_dbus(&mut runtime, Control::L1, ControlTransition::Pressed);
    send_dbus(&mut runtime, Control::R1, ControlTransition::Pressed);
    assert!(runtime.handle_evdev(1, 0x13b, 1).is_empty());
    assert!(runtime.handle_evdev(1, 0x13a, 1).is_empty());
}
