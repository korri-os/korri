//! Production multi-controller capture. No uinput writer or browser route lives here.
use std::{
    collections::{BTreeMap, VecDeque},
    future::poll_fn,
    io,
    task::Poll,
    time::Instant,
};

use korri_input_contract::{GamepadState, RemoteEvent, SeatRequest};

use crate::{
    action_catalog::{ActionId, ActionRoutes, DispatchMode},
    activity::ControllerActivity,
    dbus::{map_capability, DbusSignalSource, DBUS_INPUT_MEMBER, DBUS_TARGET_INTERFACE},
    devices::{validate_opened_descriptor, CaptureProvider, DeviceDescriptor, OpenedCapture},
    physical_sources::{
        discover_physical_sources, identify_sources, IdentifiedSource, PhysicalSources,
    },
    runtime::{
        map_evdev, InputSource, Policies, ReadyTarget, RecoveryReason, RuntimeAction, RuntimeState,
    },
    source_topology::SysfsSourceClassifier,
};

// Inverse of input_seat_uinput's existing Linux-oriented state encoder.
const BUTTONS: &[(u16, u32)] = &[
    (0x13b, 0x0010),
    (0x13a, 0x0020),
    (0x13d, 0x0040),
    (0x13e, 0x0080),
    (0x136, 0x0100),
    (0x137, 0x0200),
    (0x13c, 0x0400),
    (0x130, 0x1000),
    (0x131, 0x2000),
    (0x133, 0x4000),
    (0x134, 0x8000),
];
const MAX_UPDATES: usize = 1024;

pub fn apply_event(
    state: &mut GamepadState,
    event_type: u16,
    code: u16,
    value: i32,
) -> io::Result<()> {
    let invalid = || io::Error::other("normalized controller value outside bounds");
    match event_type {
        1 => {
            if !(0..=2).contains(&value) {
                return Err(invalid());
            }
            if let Some((_, mask)) = BUTTONS.iter().find(|(key, _)| *key == code) {
                if value == 0 {
                    state.buttons &= !mask;
                } else {
                    state.buttons |= mask;
                }
            }
        }
        3 => match code {
            0 => state.left_stick_x = i16::try_from(value).map_err(|_| invalid())?,
            1 => state.left_stick_y = i16::try_from(value).map_err(|_| invalid())?,
            2 => state.left_trigger = u8::try_from(value).map_err(|_| invalid())?,
            3 => state.right_stick_x = i16::try_from(value).map_err(|_| invalid())?,
            4 => state.right_stick_y = i16::try_from(value).map_err(|_| invalid())?,
            5 => state.right_trigger = u8::try_from(value).map_err(|_| invalid())?,
            16 | 17 => {
                if !(-1..=1).contains(&value) {
                    return Err(invalid());
                }
                let (negative, positive) = if code == 16 { (4, 8) } else { (1, 2) };
                state.buttons &= !(negative | positive);
                state.buttons |= if value < 0 {
                    negative
                } else if value > 0 {
                    positive
                } else {
                    0
                };
            }
            _ => {}
        },
        _ => {}
    }
    Ok(())
}

/// Both snapshots are read from the live provider, on opposite sides of open.
/// This detects observed replacement; DBus and procfs offer no atomic topology
/// transaction, so an unobserved change-and-change-back remains unprovable.
pub fn validate_after_open(
    before: &PhysicalSources,
    after: &PhysicalSources,
    identities: &[IdentifiedSource],
    devices: &[DeviceDescriptor],
    classifier: &SysfsSourceClassifier,
) -> io::Result<()> {
    if before != after
        || identify_sources(after, devices, classifier).ok().as_deref() != Some(identities)
    {
        return Err(io::Error::other(
            "physical identity or provider mapping changed during open",
        ));
    }
    Ok(())
}

struct CapturedSource {
    identity: IdentifiedSource,
    opened: OpenedCapture,
    state: GamepadState,
    published: GamepadState,
    policies: Policies,
    armed: bool,
}
struct RemoteShortcutSource {
    state: GamepadState,
    policies: Policies,
    armed: bool,
}

pub struct CaptureRuntime {
    state: RuntimeState,
    owner: Option<String>,
    sources: BTreeMap<String, CapturedSource>,
    remote: BTreeMap<(String, u8), RemoteShortcutSource>,
    updates: VecDeque<SeatRequest>,
    routes: ActionRoutes,
    activity: Option<ControllerActivity>,
    started: Instant,
    poll_cursor: usize,
    overflow: bool,
}

impl CaptureRuntime {
    pub fn with_action_routes(routes: ActionRoutes) -> Self {
        Self {
            state: RuntimeState::Recovering {
                reason: RecoveryReason::ProviderUnavailable,
            },
            owner: None,
            sources: BTreeMap::new(),
            remote: BTreeMap::new(),
            updates: VecDeque::new(),
            routes,
            activity: None,
            started: Instant::now(),
            poll_cursor: 0,
            overflow: false,
        }
    }
    pub fn enable_activity(&mut self) {
        self.activity = Some(ControllerActivity::default());
    }
    pub fn state(&self) -> &RuntimeState {
        &self.state
    }
    pub fn dbus_owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }
    pub fn has_open_target(&self) -> bool {
        !self.sources.is_empty()
    }
    pub fn set_dbus_owner(&mut self, owner: Option<&str>) {
        let owner = owner
            .filter(|owner| zbus::names::UniqueName::try_from(*owner).is_ok())
            .map(str::to_owned);
        if self.owner != owner {
            self.close_sources();
            self.owner = owner;
            self.state = RuntimeState::Recovering {
                reason: RecoveryReason::ProviderUnavailable,
            };
        }
    }
    fn queue(&mut self, update: SeatRequest) {
        if self.updates.len() == MAX_UPDATES {
            self.overflow = true;
        } else {
            self.updates.push_back(update);
        }
    }
    pub fn take_updates(&mut self) -> io::Result<Vec<SeatRequest>> {
        if self.overflow {
            return Err(io::Error::other("physical producer queue overflow"));
        }
        Ok(self.updates.drain(..).collect())
    }
    pub fn transport_lost(&mut self) {
        self.close_sources();
        self.remote.clear();
        self.updates.clear();
        self.overflow = false;
        self.state = RuntimeState::Recovering {
            reason: RecoveryReason::TargetRoutingFailed,
        };
    }
    pub fn source_missing(&mut self) {
        self.close_sources();
        self.state = RuntimeState::Missing { raw_gamepads: 0 };
    }
    pub fn source_ambiguous(&mut self) {
        self.close_sources();
        self.state = RuntimeState::Recovering {
            reason: RecoveryReason::SourceTopologyAmbiguous,
        };
    }
    fn remove(&mut self, id: &str) {
        if self.sources.remove(id).is_some() {
            self.queue(SeatRequest::PhysicalDisconnected {
                device_id: id.to_owned(),
            });
            self.state = match self.sources.values().next() {
                Some(source) => RuntimeState::Ready {
                    target: ReadyTarget {
                        identity: source.opened.descriptor.stable_identity(),
                        path: source.opened.descriptor.path.clone(),
                    },
                },
                None => RuntimeState::Recovering {
                    reason: RecoveryReason::EventStreamLost,
                },
            };
        }
    }
    fn close_sources(&mut self) {
        for id in self.sources.keys().cloned().collect::<Vec<_>>() {
            self.remove(&id);
        }
        if let Some(activity) = &mut self.activity {
            activity.reset();
        }
    }
    pub async fn reconcile(
        &mut self,
        provider: &mut impl CaptureProvider,
        dbus: &DbusSignalSource,
    ) {
        let queued_before = self.updates.len();
        let result = async {
            let devices = provider
                .enumerate_capture()
                .map_err(|_| io::Error::other("enumeration failed"))?;
            let classifier = SysfsSourceClassifier::system();
            let discovered = discover_physical_sources(dbus.connection(), &devices, &classifier)
                .await
                .map_err(|_| io::Error::other("physical discovery failed"))?;
            if self.owner.as_deref() != Some(discovered.owner.as_str()) {
                return Err(io::Error::other("physical owner changed during discovery"));
            }
            let identities = identify_sources(&discovered, &devices, &classifier)
                .map_err(|_| io::Error::other("physical reconnect identity rejected"))?;
            self.reconcile_sources(provider, identities.clone())?;
            let after_open = provider.enumerate_capture()?;
            // Re-read provider mappings too: using `discovered` here would
            // recheck raw descriptors against a stale composite/target map.
            let rediscovered =
                discover_physical_sources(dbus.connection(), &after_open, &classifier)
                    .await
                    .map_err(|_| io::Error::other("physical rediscovery failed after open"))?;
            validate_after_open(
                &discovered,
                &rediscovered,
                &identities,
                &after_open,
                &classifier,
            )?;
            // Opening may run after the discovery pass. Revalidate provider
            // authority before queued source facts can leave this runtime.
            if dbus.current_owner().await.ok().flatten().as_deref() != self.owner.as_deref() {
                return Err(io::Error::other("physical owner changed during open"));
            }
            Ok::<_, io::Error>(())
        }
        .await;
        if let Err(error) = result {
            self.updates.truncate(queued_before);
            tracing::warn!(event = "inputd_capture_reconcile_failed", error = %error, "physical capture failed closed");
            self.source_ambiguous();
            // A failed pass may have removed an old source before a later open
            // failed. Retire the complete producer authority, not a partial diff.
            self.overflow = true;
        }
    }
    pub fn reconcile_sources(
        &mut self,
        provider: &mut impl CaptureProvider,
        identities: Vec<IdentifiedSource>,
    ) -> io::Result<()> {
        if self.owner.is_none() {
            self.close_sources();
            return Err(io::Error::other("missing provider authority"));
        }
        let mut desired = BTreeMap::new();
        for identity in identities {
            if desired
                .insert(identity.device_id.clone(), identity)
                .is_some()
            {
                self.source_ambiguous();
                return Err(io::Error::other("duplicate physical reconnect identity"));
            }
        }
        for id in self.sources.keys().cloned().collect::<Vec<_>>() {
            if desired.get(&id) != self.sources.get(&id).map(|source| &source.identity) {
                self.remove(&id);
            }
        }
        for (id, identity) in desired {
            if self.sources.contains_key(&id) {
                continue;
            }
            let opened = provider.open_capture(&identity.source.descriptor)?;
            validate_opened_descriptor(&identity.source.descriptor, &opened.descriptor)
                .map_err(|_| io::Error::other("actual opened descriptor mismatch"))?;
            let state = opened.initial;
            self.queue(SeatRequest::PhysicalConnected {
                device_id: id.clone(),
                name: identity.raw.name.clone(),
                state,
            });
            self.sources.insert(
                id,
                CapturedSource {
                    identity,
                    opened,
                    state,
                    published: state,
                    policies: Policies::new(self.routes),
                    armed: state.is_neutral_for_rearm(),
                },
            );
        }
        self.state = match self.sources.values().next() {
            Some(source) => RuntimeState::Ready {
                target: ReadyTarget {
                    identity: source.opened.descriptor.stable_identity(),
                    path: source.opened.descriptor.path.clone(),
                },
            },
            None => RuntimeState::Missing { raw_gamepads: 0 },
        };
        Ok(())
    }
    pub fn heartbeat(&mut self) {
        let states = self
            .sources
            .iter()
            .map(|(id, source)| (id.clone(), source.published))
            .collect::<Vec<_>>();
        for (device_id, state) in states {
            self.queue(SeatRequest::PhysicalState { device_id, state });
        }
    }
    pub fn advance_actions(&mut self) -> Vec<RuntimeAction> {
        let now = self.started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        if self
            .activity
            .as_mut()
            .is_some_and(|activity| activity.take_due(now))
        {
            vec![RuntimeAction {
                id: ActionId::ControllerActivity,
                dispatch_mode: DispatchMode::Direct,
            }]
        } else {
            Vec::new()
        }
    }
    pub async fn next_evdev_actions(&mut self) -> io::Result<Option<Vec<RuntimeAction>>> {
        if self.sources.is_empty() {
            return Ok(None);
        }
        // Poll each descriptor once per wake, rotating first priority. Dropping
        // this future in select! does not remove streams or consumed events.
        let (id, event) = poll_fn(|cx| {
            let ids = self.sources.keys().cloned().collect::<Vec<_>>();
            for offset in 0..ids.len() {
                let index = (self.poll_cursor + offset) % ids.len();
                let id = &ids[index];
                if let Poll::Ready(event) = self
                    .sources
                    .get_mut(id)
                    .unwrap()
                    .opened
                    .events
                    .as_mut()
                    .poll_next(cx)
                {
                    self.poll_cursor = (index + 1) % ids.len();
                    return Poll::Ready((id.clone(), event));
                }
            }
            Poll::Pending
        })
        .await;
        match event {
            Some(Ok(event)) if !(event.event_type().0 == 0 && event.code() == 3) => {
                let source = self.sources.get_mut(&id).unwrap();
                if let Err(error) = apply_event(
                    &mut source.state,
                    event.event_type().0,
                    event.code(),
                    event.value(),
                ) {
                    self.remove(&id);
                    return Err(error);
                }
                if let Some(activity) = &mut self.activity {
                    activity.evdev(event.event_type().0, event.code(), event.value());
                }
                let mut actions = Vec::new();
                // The immutable profile emits these controls on DBus too.
                // Handle their shortcuts only on that authenticated path, not
                // twice. X/Back remain evdev-only; never promote their authority.
                if source.armed
                    && event.event_type().0 == 1
                    && matches!(event.code(), 0x116 | 0x133)
                {
                    if let Some(input) = map_evdev(1, event.code(), event.value()) {
                        actions = source.policies.handle(InputSource::Evdev, input, 0);
                    }
                }
                if event.event_type().0 == 0 && event.code() == 0 {
                    if !source.armed && source.state.is_neutral_for_rearm() {
                        source.armed = true;
                        source.policies.reset();
                    }
                    let state = source.state;
                    if state != source.published {
                        source.published = state;
                        self.queue(SeatRequest::PhysicalState {
                            device_id: id,
                            state,
                        });
                    }
                }
                Ok(Some(actions))
            }
            lost => {
                // SYN_DROPPED has no trustworthy deltas. Revoke this source,
                // close/ungrab, then reconcile/reopen and read a fresh complete
                // ioctl baseline. Never replay events up to the next SYN_REPORT.
                self.remove(&id);
                if self.sources.is_empty() {
                    self.state = RuntimeState::Recovering {
                        reason: RecoveryReason::EventStreamLost,
                    };
                }
                match lost {
                    Some(Err(error)) => Err(error),
                    _ => Ok(None),
                }
            }
        }
    }
    pub fn handle_dbus_message(&mut self, message: &zbus::Message) -> Vec<RuntimeAction> {
        let header = message.header();
        if header.sender().map(|sender| sender.as_str()) != self.owner.as_deref()
            || header.interface().map(|value| value.as_str()) != Some(DBUS_TARGET_INTERFACE)
            || header.member().map(|value| value.as_str()) != Some(DBUS_INPUT_MEMBER)
        {
            return Vec::new();
        }
        let Some(path) = header.path().map(|path| path.as_str()) else {
            return Vec::new();
        };
        let Some(source) = self.sources.values_mut().find(|source| {
            source
                .identity
                .source
                .dbus_paths
                .iter()
                .any(|target| target == path)
        }) else {
            return Vec::new();
        };
        let Ok((capability, value)) = message.body().deserialize::<(String, f64)>() else {
            return Vec::new();
        };
        let Some(input) = map_capability(&capability, value) else {
            return Vec::new();
        };
        if let Some(activity) = &mut self.activity {
            activity.semantic(input);
        }
        if !source.armed {
            return Vec::new();
        }
        source
            .policies
            .handle(InputSource::AuthenticatedDbus, input, 0)
    }
    pub fn handle_remote_event(&mut self, event: RemoteEvent) -> Vec<RuntimeAction> {
        match event {
            RemoteEvent::Connected { source } => {
                self.remote.insert(
                    (source.launch_id, source.controller_number),
                    RemoteShortcutSource {
                        state: source.state,
                        policies: Policies::new(self.routes),
                        armed: source.state.is_neutral_for_rearm(),
                    },
                );
                Vec::new()
            }
            RemoteEvent::Disconnected {
                launch_id,
                controller_number,
            } => {
                self.remote.remove(&(launch_id, controller_number));
                Vec::new()
            }
            RemoteEvent::State { source } => {
                let Some(current) = self
                    .remote
                    .get_mut(&(source.launch_id, source.controller_number))
                else {
                    return Vec::new();
                };
                let old = current.state;
                current.state = source.state;
                if let Some(activity) = &mut self.activity {
                    for (code, before, after) in [
                        (
                            0,
                            i32::from(old.left_stick_x),
                            i32::from(source.state.left_stick_x),
                        ),
                        (
                            1,
                            i32::from(old.left_stick_y),
                            i32::from(source.state.left_stick_y),
                        ),
                        (
                            2,
                            i32::from(old.left_trigger),
                            i32::from(source.state.left_trigger),
                        ),
                        (
                            3,
                            i32::from(old.right_stick_x),
                            i32::from(source.state.right_stick_x),
                        ),
                        (
                            4,
                            i32::from(old.right_stick_y),
                            i32::from(source.state.right_stick_y),
                        ),
                        (
                            5,
                            i32::from(old.right_trigger),
                            i32::from(source.state.right_trigger),
                        ),
                    ] {
                        if before != after {
                            activity.evdev(3, code, after);
                        }
                    }
                }
                if !current.armed {
                    if source.state.is_neutral_for_rearm() {
                        current.armed = true;
                        current.policies.reset();
                    }
                    return Vec::new();
                }
                let mut actions = Vec::new();
                for (code, mask) in BUTTONS {
                    if old.buttons & mask != source.state.buttons & mask {
                        if let Some(input) =
                            map_evdev(1, *code, i32::from(source.state.buttons & mask != 0))
                        {
                            if let Some(activity) = &mut self.activity {
                                activity.semantic(input);
                            }
                            actions.extend(current.policies.handle(InputSource::Remote, input, 0));
                        }
                    }
                }
                for (code, negative, positive) in [(16, 4, 8), (17, 1, 2)] {
                    let hat = |state: GamepadState| {
                        i32::from(state.buttons & positive != 0)
                            - i32::from(state.buttons & negative != 0)
                    };
                    if hat(old) != hat(source.state) {
                        let input = map_evdev(3, code, hat(source.state)).unwrap();
                        if let Some(activity) = &mut self.activity {
                            activity.semantic(input);
                        }
                        actions.extend(current.policies.handle(InputSource::Remote, input, 0));
                    }
                }
                actions
            }
        }
    }
}
