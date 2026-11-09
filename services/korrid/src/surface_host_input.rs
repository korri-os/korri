//! Bounded, receive-only native input for the local surface host.
//! Attachment captures metadata and current held state atomically. Before the
//! host freezes the shell, suspend() requires every subscriber to retire its
//! callbacks and clear input, then acknowledge the exact connection/request.
//! Queue age alone cannot invalidate TCP- or client-buffered messages.

use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode, Uri},
    response::Response,
    routing::get,
    Router,
};
use futures::SinkExt;
use serde::Serialize;
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::timeout,
};

use crate::{native_input::*, surface_host_access::SurfaceHostAccess};

/// The embedded native input socket carries small, latency-sensitive messages.
/// axum's TcpListener implementation does not change the kernel's Nagle default.
pub(crate) fn low_latency_listener(
    listener: tokio::net::TcpListener,
) -> impl axum::serve::Listener<Io = tokio::net::TcpStream, Addr = std::net::SocketAddr> {
    use axum::serve::ListenerExt;
    listener.tap_io(|socket| {
        if let Err(error) = socket.set_nodelay(true) {
            eprintln!("korrid: native input TCP_NODELAY failed: {error}");
        }
    })
}

#[cfg(test)]
mod latency_tests {
    use super::*;
    use axum::serve::Listener;

    #[tokio::test]
    async fn accepted_native_socket_disables_nagle() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut listener = listener;
        let (_client, (accepted, _)) = tokio::join!(
            tokio::net::TcpStream::connect(address),
            Listener::accept(&mut listener)
        );
        assert!(
            !accepted.nodelay().unwrap(),
            "default accepted TCP sockets still use Nagle"
        );
        let mut listener = low_latency_listener(listener);
        let (_client, (accepted, _)) =
            tokio::join!(tokio::net::TcpStream::connect(address), listener.accept());
        assert!(accepted.nodelay().unwrap());
    }
}

/// Internal resource budgets, not persisted settings or seat count.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceHostInputLimits {
    pub connections: usize,
    pub devices: usize,
    pub queued_events: usize,
    pub incoming_bytes: usize,
    pub event_bytes: usize,
    pub handshake_timeout: Duration,
    pub write_timeout: Duration,
    pub max_queue_age: Duration,
}

impl Default for SurfaceHostInputLimits {
    fn default() -> Self {
        Self {
            connections: 4,
            // Receiver admits 255 physical sources and controllerNumber 0..=15
            // for its one authenticated remote launch. Seat allocation does not
            // limit which connected sources can navigate the shell.
            devices: korri_input_contract::MAX_PHYSICAL_SOURCES + 16,
            queued_events: 64,
            incoming_bytes: 1024,
            event_bytes: 16 * 1024,
            handshake_timeout: Duration::from_secs(3),
            write_timeout: Duration::from_millis(250),
            max_queue_age: Duration::from_millis(250),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PublishError {
    #[error("native input event exceeds the byte limit")]
    TooLarge,
    #[error("native input device limit reached")]
    DeviceLimit,
    #[error("native input device is already present")]
    DuplicateDevice,
    #[error("native input device is absent or its class does not match")]
    UnknownDevice,
    #[error("native input contains a non-finite number")]
    NonFinite,
    #[error("native input exceeds Linux event bounds")]
    InvalidInput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SuspendError {
    #[error("native input is already suspended")]
    AlreadySuspended,
    #[error("native input subscriber did not acknowledge before the deadline")]
    Timeout,
    #[error("native input subscriber disconnected before acknowledging")]
    Disconnected,
}

struct Frame {
    text: Arc<str>,
    created: Instant,
}
impl Frame {
    fn new(text: Arc<str>) -> Self {
        Self {
            text,
            created: Instant::now(),
        }
    }
}

struct Device {
    class: NativeInputDeviceClass,
    text: Arc<str>,
    // Linux EV_KEY <= KEY_MAX and EV_ABS <= ABS_MAX bound this map.
    values: BTreeMap<(u16, u16), Arc<str>>,
}
struct Subscription {
    classes: Vec<NativeInputDeviceClass>,
    queue: mpsc::Sender<Frame>,
}
struct Suspension {
    request_id: String,
    acknowledgement: Option<oneshot::Sender<()>>,
}
struct Client {
    generation: String,
    stop: watch::Sender<bool>,
    control: watch::Sender<Option<NativeInputControl>>,
    subscription: Option<Subscription>,
    suspension: Option<Suspension>,
    released: bool,
}
struct SourceState {
    connected: bool,
    suspended: Option<String>,
    next_generation: u64,
    next_request: u64,
    devices: BTreeMap<String, Device>,
    clients: Vec<Option<Client>>,
    local_viewers: Vec<mpsc::Sender<Arc<str>>>,
}
struct Shared {
    limits: SurfaceHostInputLimits,
    state: Mutex<SourceState>,
    /// Gamepad presence and evidence for the surface model's `controllers`.
    controllers: Arc<crate::surface::controllers::Controllers>,
}

/// Sole producer (share through Arc when host lifecycle and ingress both use it).
/// Drop is terminal; reset clears a lost producer without replacing the router.
pub struct SurfaceHostInputSource {
    shared: Arc<Shared>,
}
impl SurfaceHostInputSource {
    /// In-process platform consumer of the SAME native frames and baseline.
    /// Queue overflow/producer loss closes it; a viewer must neutralize on EOF.
    pub fn subscribe_local_gamepads(&self) -> (Vec<Arc<str>>, mpsc::Receiver<Arc<str>>) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        let (sender, receiver) = mpsc::channel(self.shared.limits.queued_events);
        let mut baseline = Vec::new();
        for device in state
            .devices
            .values()
            .filter(|d| d.class == NativeInputDeviceClass::Gamepad)
        {
            baseline.push(Arc::clone(&device.text));
            baseline.extend(device.values.values().cloned());
            // The existing completion record distinguishes held baseline from
            // a real press. No second controller state is maintained here.
            let event: NativeInputEvent =
                serde_json::from_str(&device.text).expect("encoded device");
            if let NativeInputEvent::DeviceAdded(added) = event {
                baseline.push(
                    encode(
                        &NativeInputControl::DeviceStateComplete(NativeInputDeviceStateComplete {
                            kind: NativeInputDeviceStateCompleteKind::DeviceStateComplete,
                            device_id: added.device.device_id,
                        }),
                        self.shared.limits.event_bytes,
                    )
                    .expect("bounded completion"),
                );
            }
        }
        state.local_viewers.retain(|queue| !queue.is_closed());
        if state.connected && state.suspended.is_none() && state.local_viewers.is_empty() {
            state.local_viewers.push(sender);
        }
        (baseline, receiver)
    }

    /// A DeviceAdded without values declares a neutral arrival. Use publish_device
    /// with the complete current baseline for real controller arrivals.
    pub fn publish(&self, event: NativeInputEvent) -> Result<(), PublishError> {
        if let NativeInputEvent::DeviceAdded(added) = event {
            return self.publish_device(added.device, Vec::new());
        }
        validate(&event)?;
        let text = encode(&event, self.shared.limits.event_bytes)?;
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        let class = match &event {
            NativeInputEvent::DeviceRemoved(removed) => {
                state
                    .devices
                    .remove(&removed.device_id)
                    .ok_or(PublishError::UnknownDevice)?
                    .class
            }
            NativeInputEvent::Input(input) => {
                let device = state
                    .devices
                    .get_mut(&input.device_id)
                    .ok_or(PublishError::UnknownDevice)?;
                if device.class != input.class {
                    return Err(PublishError::UnknownDevice);
                }
                if input.input_type == 1.0 || input.input_type == 3.0 {
                    device.values.insert(
                        (input.input_type as u16, input.code as u16),
                        Arc::clone(&text),
                    );
                }
                input.class
            }
            NativeInputEvent::Action(action) => action.class,
            NativeInputEvent::DeviceAdded(_) => unreachable!(),
        };
        broadcast(&mut state, class, &[text]);
        drop(state);
        match &event {
            NativeInputEvent::DeviceRemoved(removed) => {
                self.shared.controllers.removed(&removed.device_id)
            }
            NativeInputEvent::Input(input) if input.class == NativeInputDeviceClass::Gamepad => {
                self.shared.controllers.input(&input.device_id)
            }
            _ => {}
        }
        Ok(())
    }

    /// Commit one measured controller sample atomically. Values are changed
    /// EV_KEY/EV_ABS fields, followed on the wire by canonical EV_SYN/SYN_REPORT.
    /// Subscriber attachment can see only the previous or next complete sample.
    /// The coordinator must use this API, not publish individual sample deltas.
    pub fn publish_frame(
        &self,
        device_id: &str,
        values: Vec<NativeInputInput>,
    ) -> Result<(), PublishError> {
        if values.len() > 832 {
            return Err(PublishError::InvalidInput);
        }
        let timestamp = values.last().map_or(0.0, |value| value.timestamp);
        let mut updates = BTreeMap::new();
        let mut frames = Vec::with_capacity(values.len() + 1);
        let mut classes = Vec::with_capacity(values.len());
        for value in values {
            if value.device_id != device_id {
                return Err(PublishError::UnknownDevice);
            }
            if value.input_type != 1.0 && value.input_type != 3.0 {
                return Err(PublishError::InvalidInput);
            }
            let event = NativeInputEvent::Input(value.clone());
            validate(&event)?;
            let text = encode(&event, self.shared.limits.event_bytes)?;
            if updates
                .insert(
                    (value.input_type as u16, value.code as u16),
                    Arc::clone(&text),
                )
                .is_some()
            {
                return Err(PublishError::InvalidInput);
            }
            classes.push(value.class);
            frames.push(text);
        }
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        let device = state
            .devices
            .get_mut(device_id)
            .ok_or(PublishError::UnknownDevice)?;
        let class = device.class;
        if classes.iter().any(|value| *value != class) {
            return Err(PublishError::UnknownDevice);
        }
        // Encode before committing, so validation/allocation failure cannot
        // leave a partially published baseline behind.
        frames.push(encode(
            &NativeInputEvent::Input(NativeInputInput {
                kind: NativeInputInputKind::Input,
                device_id: device_id.to_owned(),
                class,
                input_type: 0.0,
                code: 0.0,
                value: 0.0,
                timestamp,
            }),
            self.shared.limits.event_bytes,
        )?);
        device.values.extend(updates);
        korri_input_trace::record("publish", format_args!("fields={}", frames.len() - 1));
        broadcast(&mut state, class, &frames);
        drop(state);
        // A live sample after the baseline is evidence the pad was heard.
        if class == NativeInputDeviceClass::Gamepad {
            self.shared.controllers.input(device_id);
        }
        Ok(())
    }

    /// Metadata and complete initial state become visible in one lock. A live
    /// arrival is baseline too, never a series of synthetic button presses.
    pub fn publish_device(
        &self,
        device: NativeInputDeviceInfo,
        inputs: Vec<NativeInputInput>,
    ) -> Result<(), PublishError> {
        // Reject oversized lists before serializing. Linux permits 768 keys and
        // 64 absolute axes; neither this budget nor device count is seat count.
        if inputs.len() > 832 {
            return Err(PublishError::InvalidInput);
        }
        let added = NativeInputEvent::DeviceAdded(NativeInputDeviceAdded {
            kind: NativeInputDeviceAddedKind::DeviceAdded,
            device,
        });
        validate(&added)?;
        let NativeInputEvent::DeviceAdded(added_info) = &added else {
            unreachable!()
        };
        let info = &added_info.device;
        let text = encode(&added, self.shared.limits.event_bytes)?;
        let mut values = BTreeMap::new();
        for input in inputs {
            if input.device_id != info.device_id || input.class != info.class {
                return Err(PublishError::UnknownDevice);
            }
            if input.input_type != 1.0 && input.input_type != 3.0 {
                return Err(PublishError::InvalidInput);
            }
            let key = (input.input_type as u16, input.code as u16);
            let event = NativeInputEvent::Input(input);
            validate(&event)?;
            if values
                .insert(key, encode(&event, self.shared.limits.event_bytes)?)
                .is_some()
            {
                return Err(PublishError::InvalidInput);
            }
        }
        let complete = encode(
            &NativeInputControl::DeviceStateComplete(NativeInputDeviceStateComplete {
                kind: NativeInputDeviceStateCompleteKind::DeviceStateComplete,
                device_id: info.device_id.clone(),
            }),
            self.shared.limits.event_bytes,
        )?;
        let mut frames = vec![Arc::clone(&text)];
        frames.extend(values.values().cloned());
        frames.push(complete);
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        if state.devices.contains_key(&info.device_id) {
            return Err(PublishError::DuplicateDevice);
        }
        if state.devices.len() >= self.shared.limits.devices {
            return Err(PublishError::DeviceLimit);
        }
        state.devices.insert(
            info.device_id.clone(),
            Device {
                class: info.class,
                text,
                values,
            },
        );
        broadcast(&mut state, info.class, &frames);
        drop(state);
        if info.class == NativeInputDeviceClass::Gamepad {
            self.shared.controllers.added(&info.device_id, &info.name);
        }
        Ok(())
    }

    /// Explicit producer loss: current state and every attachment are retired.
    pub fn reset(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        state.devices.clear();
        state.local_viewers.clear();
        for client in state.clients.iter_mut().flatten() {
            client.stop.send_replace(true);
            client.subscription = None;
            client.suspension = None;
        }
        drop(state);
        self.shared.controllers.disconnected();
    }

    /// Stop publishing before requesting retirement. Success means all attached
    /// consumers cleared input and invalidated callbacks BEFORE the host freezes.
    /// On failure the source remains suspended; rollback must explicitly resume.
    pub async fn suspend(&self, deadline: Duration) -> Result<(), SuspendError> {
        let receivers = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("native input mutex poisoned");
            if state.suspended.is_some() {
                return Err(SuspendError::AlreadySuspended);
            }
            state.next_request = state
                .next_request
                .checked_add(1)
                .expect("native request counter exhausted");
            let request_id = state.next_request.to_string();
            state.suspended = Some(request_id.clone());
            state.local_viewers.clear();
            let mut receivers = Vec::new();
            for client in state.clients.iter_mut().flatten() {
                // Local retirement won the same mutex: it cleared surface host data
                // before asking for release and can no longer be selected.
                // Do not cancel the bounded retired reply currently being sent.
                if client.released {
                    continue;
                }
                if client.subscription.is_none() {
                    // A handshake without an installed subscription has received
                    // no input. Stop it so it cannot attach after the barrier.
                    client.stop.send_replace(true);
                    continue;
                }
                let (acknowledgement, receiver) = oneshot::channel();
                client.suspension = Some(Suspension {
                    request_id: request_id.clone(),
                    acknowledgement: Some(acknowledgement),
                });
                client
                    .control
                    .send_replace(Some(NativeInputControl::Suspend(NativeInputSuspend {
                        kind: NativeInputSuspendKind::Suspend,
                        generation: client.generation.clone(),
                        request_id: request_id.clone(),
                    })));
                // Publish the control before closing the queue: the serving
                // task must not mistake that queue closure for source loss.
                client.subscription = None;
                receivers.push(receiver);
            }
            receivers
        };
        match timeout(deadline, futures::future::try_join_all(receivers)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err(SuspendError::Disconnected),
            Err(_) => Err(SuspendError::Timeout),
        }
    }

    /// Call after thaw/return (or failed-transition rollback). Old connections
    /// stay retired; resume only asks them to authenticate a fresh attachment.
    pub fn resume(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        if state.suspended.take().is_none() {
            return;
        }
        for client in state.clients.iter_mut().flatten() {
            if let Some(suspension) = client.suspension.take() {
                client
                    .control
                    .send_replace(Some(NativeInputControl::Resume(NativeInputResume {
                        kind: NativeInputResumeKind::Resume,
                        generation: client.generation.clone(),
                        request_id: suspension.request_id,
                    })));
            }
        }
    }
}

fn broadcast(state: &mut SourceState, class: NativeInputDeviceClass, frames: &[Arc<str>]) {
    if state.suspended.is_some() {
        return;
    }
    if class == NativeInputDeviceClass::Gamepad {
        state.local_viewers.retain(|queue| {
            frames
                .iter()
                .all(|text| queue.try_send(Arc::clone(text)).is_ok())
        });
    }
    for client in state.clients.iter_mut().flatten() {
        let Some(subscription) = &client.subscription else {
            continue;
        };
        if !subscription.classes.contains(&class) {
            continue;
        }
        for text in frames {
            if subscription
                .queue
                .try_send(Frame::new(Arc::clone(text)))
                .is_err()
            {
                client.stop.send_replace(true);
                break;
            }
        }
    }
}

impl Drop for SurfaceHostInputSource {
    fn drop(&mut self) {
        self.reset();
        self.shared
            .state
            .lock()
            .expect("native input mutex poisoned")
            .connected = false;
    }
}

/// Local root-path WS only, never the LAN peer router. Exact Origin precedes
/// upgrade; first text frame is Bearer, second the unchanged legacy subscription.
/// No authority is read from URLs, subprotocols, or controller payloads. The
/// build disables dependency debug/trace logging of raw WS credentials.
pub fn router(
    access: SurfaceHostAccess,
    limits: SurfaceHostInputLimits,
) -> (Router, SurfaceHostInputSource) {
    router_with_surfaces(access, Arc::default(), limits)
}

/// The same socket also serves `app.surface.watch` (native surfaces contract,
/// section 6): a second frame naming it, instead of a native input
/// subscription, hands the authenticated connection to the hub in `surfaces`.
pub fn router_with_surfaces(
    access: SurfaceHostAccess,
    surfaces: crate::surface::SurfaceSlot,
    limits: SurfaceHostInputLimits,
) -> (Router, SurfaceHostInputSource) {
    assert!(limits.connections > 0 && limits.devices > 0 && limits.queued_events > 0);
    assert!(limits.incoming_bytes > 0 && limits.event_bytes > 0);
    assert!(
        !limits.handshake_timeout.is_zero()
            && !limits.write_timeout.is_zero()
            && !limits.max_queue_age.is_zero()
    );
    let shared = Arc::new(Shared {
        controllers: Arc::clone(&surfaces.controllers),
        limits,
        state: Mutex::new(SourceState {
            connected: true,
            suspended: None,
            next_generation: 0,
            next_request: 0,
            devices: BTreeMap::new(),
            local_viewers: Vec::new(),
            clients: (0..limits.connections).map(|_| None).collect(),
        }),
    });
    let app = Router::new().route("/", get(upgrade)).with_state(Endpoint {
        access,
        shared: Arc::clone(&shared),
        surfaces,
    });
    (app, SurfaceHostInputSource { shared })
}
#[derive(Clone)]
struct Endpoint {
    access: SurfaceHostAccess,
    shared: Arc<Shared>,
    surfaces: crate::surface::SurfaceSlot,
}

// Reserved before upgrade, including idle unauthenticated clients.
struct Connection {
    shared: Arc<Shared>,
    slot: usize,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.shared
            .state
            .lock()
            .expect("native input mutex poisoned")
            .clients[self.slot] = None;
    }
}
impl Connection {
    fn subscribe(
        &self,
        subscription: NativeInputSubscription,
    ) -> Option<(Vec<Frame>, mpsc::Receiver<Frame>)> {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        let client = state.clients[self.slot].as_ref()?;
        if !state.connected || state.suspended.is_some() || client.released || *client.stop.borrow()
        {
            return None;
        }
        let (queue, receiver) = mpsc::channel(self.shared.limits.queued_events);
        // At most devices * (metadata + 768 keys + 64 axes), plus completion.
        // Frames reference existing bounded state; live queues keep their
        // independent 64-frame budget and fail closed instead of growing.
        let mut snapshot = Vec::new();
        for device in state
            .devices
            .values()
            .filter(|device| subscription.classes.contains(&device.class))
        {
            snapshot.push(Frame::new(Arc::clone(&device.text)));
            snapshot.extend(
                device
                    .values
                    .values()
                    .map(|text| Frame::new(Arc::clone(text))),
            );
        }
        snapshot.push(Frame::new(
            encode(
                &NativeInputControl::InitializationComplete(NativeInputInitializationComplete {
                    kind: NativeInputInitializationCompleteKind::InitializationComplete,
                    generation: client.generation.clone(),
                }),
                self.shared.limits.event_bytes,
            )
            .ok()?,
        ));
        state.clients[self.slot].as_mut()?.subscription = Some(Subscription {
            classes: subscription.classes,
            queue,
        });
        Some((snapshot, receiver))
    }
    /// Serializes local attachment release against host suspend selection.
    /// None is invalid; false means suspend won and still needs its exact ACK.
    fn retire(&self, retirement: &NativeInputRetirement) -> Option<bool> {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        let client = state.clients[self.slot].as_mut()?;
        if client.generation != retirement.generation || *client.stop.borrow() {
            return None;
        }
        if client.suspension.is_some() {
            return Some(false);
        }
        client.released = true;
        client.subscription = None;
        Some(true)
    }

    fn acknowledge(&self, acknowledgement: NativeInputAcknowledgement) -> bool {
        let NativeInputAcknowledgement {
            generation,
            request_id,
            ..
        } = acknowledgement;
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        if state.suspended.as_ref() != Some(&request_id) {
            return false;
        }
        let Some(client) = state.clients[self.slot].as_mut() else {
            return false;
        };
        if client.generation != generation {
            return false;
        }
        let Some(suspension) = &mut client.suspension else {
            return false;
        };
        if suspension.request_id != request_id {
            return false;
        }
        let Some(sender) = suspension.acknowledgement.take() else {
            return false;
        };
        sender.send(()).is_ok()
    }
}

async fn upgrade(
    State(endpoint): State<Endpoint>,
    headers: HeaderMap,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    endpoint.access.authorize_socket_origin(&headers)?;
    if uri.query().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (connection, stop, control) = {
        let mut state = endpoint
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        if !state.connected || state.suspended.is_some() {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        let slot = state
            .clients
            .iter()
            .position(Option::is_none)
            .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
        let (stop, receiver) = watch::channel(false);
        let (control, lifecycle) = watch::channel(None);
        state.next_generation = state
            .next_generation
            .checked_add(1)
            .expect("native connection counter exhausted");
        state.clients[slot] = Some(Client {
            generation: state.next_generation.to_string(),
            stop,
            control,
            subscription: None,
            suspension: None,
            released: false,
        });
        (
            Connection {
                shared: Arc::clone(&endpoint.shared),
                slot,
            },
            receiver,
            lifecycle,
        )
    };
    let limits = endpoint.shared.limits;
    Ok(ws
        .max_message_size(limits.incoming_bytes)
        .max_frame_size(limits.incoming_bytes)
        .read_buffer_size(limits.incoming_bytes)
        .write_buffer_size(0)
        // A surface watch pushes a whole model in one message.
        .max_write_buffer_size(
            limits
                .event_bytes
                .saturating_add(1024)
                .max(SURFACE_MODEL_BYTES),
        )
        .on_upgrade(move |socket| {
            serve(
                socket,
                endpoint.access,
                endpoint.surfaces,
                connection,
                stop,
                control,
            )
        }))
}
async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
}
/// The largest whole model one surface watch push may carry.
const SURFACE_MODEL_BYTES: usize = 8 * 1024 * 1024;

enum Selection {
    Native(NativeInputSubscription),
    Surface(String),
}

async fn handshake(socket: &mut WebSocket, access: &SurfaceHostAccess) -> Option<Selection> {
    let Message::Text(bearer) = socket.recv().await?.ok()? else {
        return None;
    };
    access.authorize_bearer(&bearer).ok()?;
    drop(bearer);
    let Message::Text(text) = socket.recv().await?.ok()? else {
        return None;
    };
    if let Ok(crate::surface::SurfaceWatchSubscription::Watch(watch)) = serde_json::from_str(&text)
    {
        return Some(Selection::Surface(watch.surface_id));
    }
    serde_json::from_str(&text).ok().map(Selection::Native)
}
async fn send(
    socket: &mut WebSocket,
    frame: Frame,
    limits: SurfaceHostInputLimits,
    stop: &mut watch::Receiver<bool>,
) -> bool {
    let Some(remaining) = limits.max_queue_age.checked_sub(frame.created.elapsed()) else {
        return false;
    };
    // Decode only under the opt-in gate. Never print arbitrary socket text:
    // this function also carries baseline/control messages.
    let input = korri_input_trace::enabled()
        .then(|| serde_json::from_str::<NativeInputEvent>(&frame.text).ok())
        .flatten();
    if let Some(NativeInputEvent::Input(input)) = &input {
        korri_input_trace::record(
            "socket_write",
            format_args!(
                "type={} code={} value={} queue_us={}",
                input.input_type,
                input.code,
                input.value,
                frame.created.elapsed().as_micros()
            ),
        );
    }
    let sent = tokio::select! {
        biased;
        _ = stopped(stop) => false,
        result = timeout(limits.write_timeout.min(remaining), socket.send(Message::Text(frame.text.as_ref().into()))) => matches!(result, Ok(Ok(()))),
    };
    if let Some(NativeInputEvent::Input(input)) = &input {
        korri_input_trace::record(
            "socket_written",
            format_args!(
                "type={} code={} value={} ok={sent}",
                input.input_type, input.code, input.value
            ),
        );
    }
    sent
}
async fn serve(
    mut socket: WebSocket,
    access: SurfaceHostAccess,
    surfaces: crate::surface::SurfaceSlot,
    connection: Connection,
    mut stop: watch::Receiver<bool>,
    mut control: watch::Receiver<Option<NativeInputControl>>,
) {
    let limits = connection.shared.limits;
    let selection = tokio::select! {
        biased;
        _ = stopped(&mut stop) => return,
        result = timeout(limits.handshake_timeout, handshake(&mut socket, &access)) => match result { Ok(Some(selection)) => selection, _ => return },
    };
    let subscription = match selection {
        Selection::Native(subscription) => subscription,
        Selection::Surface(surface_id) => {
            // A watch receives no input: release the input slot first.
            drop(connection);
            if crate::surface::check_surface_id(&surface_id).is_err() {
                return;
            }
            let Some(hub) = surfaces.hub.get().cloned() else {
                return;
            };
            crate::surface::watch::serve(socket, hub, surface_id).await;
            return;
        }
    };
    let Some((snapshot, mut queue)) = connection.subscribe(subscription) else {
        return;
    };
    let mut snapshot = snapshot.into_iter();
    let mut streaming = true;
    loop {
        // Control is checked before each baseline/live frame. Already-buffered
        // bytes cannot be recalled; the surface host retirement ACK is the barrier.
        tokio::select! {
            biased;
            _ = stopped(&mut stop) => return,
            changed = control.changed() => {
                if changed.is_err() { return; }
                let Some(command) = control.borrow_and_update().clone() else { continue };
                streaming = false;
                snapshot = Vec::new().into_iter();
                queue.close();
                let resume = matches!(command, NativeInputControl::Resume(_));
                let Ok(text) = encode(&command, limits.event_bytes) else { return };
                if !send(&mut socket, Frame::new(text), limits, &mut stop).await { return; }
                if resume { return; }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(ack) = serde_json::from_str::<NativeInputAcknowledgement>(&text) {
                        if !connection.acknowledge(ack) { return; }
                    } else if let Ok(retirement) = serde_json::from_str::<NativeInputRetirement>(&text) {
                        match connection.retire(&retirement) {
                            None => return,
                            Some(false) => {}, // selected: retain exact suspend ACK channel
                            Some(true) => {
                                let reply = NativeInputControl::Retired(NativeInputRetired {
                                    kind: NativeInputRetiredKind::Retired,
                                    generation: retirement.generation,
                                });
                                let Ok(text) = encode(&reply, limits.event_bytes) else { return };
                                let _ = send(&mut socket, Frame::new(text), limits, &mut stop).await;
                                return; // released under mutex before either side closes
                            }
                        }
                    } else {
                        // Preserve subscription replacement; no input injection.
                        let Ok(subscription) = serde_json::from_str::<NativeInputSubscription>(&text) else { return };
                        let Some((next_snapshot, next_queue)) = connection.subscribe(subscription) else { return };
                        snapshot = next_snapshot.into_iter();
                        queue = next_queue;
                    }
                }
                Some(Ok(Message::Ping(_))) => {
                    tokio::select! {
                        biased;
                        _ = stopped(&mut stop) => return,
                        result = timeout(limits.write_timeout, socket.flush()) => if !matches!(result, Ok(Ok(()))) { return; },
                    }
                }
                Some(Ok(Message::Pong(_))) => {},
                _ => return,
            },
            frame = async { match snapshot.next() { Some(frame) => Some(frame), None => queue.recv().await } }, if streaming => {
                let Some(frame) = frame else { return };
                if !send(&mut socket, frame, limits, &mut stop).await { return; }
            }
        }
    }
}

fn validate(event: &NativeInputEvent) -> Result<(), PublishError> {
    let finite = match event {
        NativeInputEvent::Input(input) => {
            [input.input_type, input.code, input.value, input.timestamp]
                .iter()
                .all(|value| value.is_finite())
        }
        NativeInputEvent::DeviceAdded(added) => added.device.axes.as_ref().is_none_or(|axes| {
            axes.iter().all(|axis| {
                [axis.code, axis.minimum, axis.maximum]
                    .iter()
                    .all(|value| value.is_finite())
                    && axis.flat.is_none_or(f64::is_finite)
            })
        }),
        NativeInputEvent::DeviceRemoved(_) => true,
        NativeInputEvent::Action(action) => action.timestamp.is_finite(),
    };
    if !finite {
        return Err(PublishError::NonFinite);
    }
    if let NativeInputEvent::Input(input) = event {
        if !integer_between(input.input_type, 0.0, 0x1f as f64)
            || !integer_between(input.code, 0.0, 0x2ff as f64)
            || !integer_between(input.value, i32::MIN as f64, i32::MAX as f64)
            || (input.input_type == 1.0 && !integer_between(input.value, 0.0, 2.0))
            || (input.input_type == 3.0 && input.code > 0x3f as f64)
        {
            return Err(PublishError::InvalidInput);
        }
    }
    if let NativeInputEvent::DeviceAdded(added) = event {
        let device = &added.device;
        if device.device_id.is_empty()
            || device.device_id.len() > 1024
            || device.capabilities.len() > 64
            || device.capabilities.iter().any(|value| value.len() > 128)
        {
            return Err(PublishError::InvalidInput);
        }
        if let Some(axes) = &device.axes {
            let mut seen = std::collections::BTreeSet::new();
            if axes.len() > 64
                || axes.iter().any(|axis| {
                    !integer_between(axis.code, 0.0, 63.0) || !seen.insert(axis.code as u16)
                })
            {
                return Err(PublishError::InvalidInput);
            }
        }
    }
    Ok(())
}
fn integer_between(value: f64, min: f64, max: f64) -> bool {
    value.fract() == 0.0 && value >= min && value <= max
}

// Bounded allocation, not serialize-then-check. Never log payloads/errors.
fn encode(event: &impl Serialize, limit: usize) -> Result<Arc<str>, PublishError> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit - self.bytes.len() {
                return Err(io::Error::other("native input byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut output, event).map_err(|_| PublishError::TooLarge)?;
    Ok(String::from_utf8(output.bytes)
        .expect("JSON is UTF-8")
        .into())
}
