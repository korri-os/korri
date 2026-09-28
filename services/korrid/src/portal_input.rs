//! Bounded, receive-only native input for the local portal.
//!
//! This router is deliberately NOT mounted by either production router yet.
//! A future local producer owns `PortalInputSource`; this seam specifies no IPC,
//! controller identity, pool assignment, or deployment configuration.
//! Device snapshots carry metadata, not held/neutral state. Queue deadlines do
//! not invalidate bytes already delivered to kernel/browser buffers; activation
//! and neutral-before-rearm still require an integrated consumer/source contract.

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
use tokio::{
    sync::{mpsc, watch},
    time::timeout,
};

use crate::{native_input::*, portal_access::PortalAccess};

/// Internal resource budgets, not persisted settings. Tests can use smaller
/// budgets. Device and queue counts are unrelated to virtual-gamepad seat count.
#[derive(Clone, Copy, Debug)]
pub struct PortalInputLimits {
    pub connections: usize,
    pub devices: usize,
    pub queued_events: usize,
    pub incoming_bytes: usize,
    pub event_bytes: usize,
    pub handshake_timeout: Duration,
    pub write_timeout: Duration,
    pub max_queue_age: Duration,
}

impl Default for PortalInputLimits {
    fn default() -> Self {
        Self {
            connections: 4,
            devices: 64,
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
}

struct Frame {
    text: Arc<str>,
    created: Instant,
}

struct Device {
    class: NativeInputDeviceClass,
    text: Arc<str>,
}

struct Subscription {
    classes: Vec<NativeInputDeviceClass>,
    queue: mpsc::Sender<Frame>,
}

struct Client {
    stop: watch::Sender<bool>,
    subscription: Option<Subscription>,
}

struct SourceState {
    connected: bool,
    devices: BTreeMap<String, Device>,
    clients: Vec<Option<Client>>,
}

struct Shared {
    limits: PortalInputLimits,
    state: Mutex<SourceState>,
}

/// The sole in-process producer. Dropping it clears metadata and disconnects
/// every socket, including sockets still authenticating. It is not cloneable.
/// Source loss is terminal for this router instance; producer reconnection is
/// deliberately not specified by this in-process seam.
pub struct PortalInputSource {
    shared: Arc<Shared>,
}

impl PortalInputSource {
    /// Publish current lifecycle/input, never a replay. Metadata is retained only
    /// until removal/source loss. Actions and input are never retained for a new
    /// subscriber. Full queues cancel the affected connection, not the producer.
    pub fn publish(&self, event: NativeInputEvent) -> Result<(), PublishError> {
        finite(&event)?;
        let text = encode(&event, self.shared.limits.event_bytes)?;
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        let class = match &event {
            NativeInputEvent::DeviceAdded(added) => {
                let device = &added.device;
                if state.devices.contains_key(&device.device_id) {
                    return Err(PublishError::DuplicateDevice);
                }
                if state.devices.len() >= self.shared.limits.devices {
                    return Err(PublishError::DeviceLimit);
                }
                state.devices.insert(
                    device.device_id.clone(),
                    Device {
                        class: device.class,
                        text: Arc::clone(&text),
                    },
                );
                device.class
            }
            NativeInputEvent::DeviceRemoved(removed) => {
                state
                    .devices
                    .remove(&removed.device_id)
                    .ok_or(PublishError::UnknownDevice)?
                    .class
            }
            NativeInputEvent::Input(input) => {
                if state
                    .devices
                    .get(&input.device_id)
                    .map(|device| device.class)
                    != Some(input.class)
                {
                    return Err(PublishError::UnknownDevice);
                }
                input.class
            }
            NativeInputEvent::Action(action) => action.class,
        };
        for client in state.clients.iter_mut().flatten() {
            let Some(subscription) = &client.subscription else {
                continue;
            };
            if subscription.classes.contains(&class)
                && subscription
                    .queue
                    .try_send(Frame {
                        text: Arc::clone(&text),
                        created: Instant::now(),
                    })
                    .is_err()
            {
                client.stop.send_replace(true);
                client.subscription = None;
            }
        }
        Ok(())
    }
}

impl Drop for PortalInputSource {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        state.connected = false;
        state.devices.clear();
        for client in state.clients.iter_mut().flatten() {
            client.stop.send_replace(true);
            client.subscription = None;
        }
    }
}

/// Construct the legacy root-path WS router and its bounded source. Mount only
/// on the local portal listener, never the LAN peer router. Exact Origin is
/// required before upgrade; the first text frame is `Bearer <capability>` and
/// the second is the legacy `{classes:[...]}` subscription. There is no auth ACK.
/// No capability is read from headers, query strings, or WS subprotocols here.
///
/// Integration must disable dependency payload logging: Tungstenite traces raw
/// messages and debugs close-frame content, which can contain the first-frame
/// credential. Current korrid installs no logger; this module logs nothing.
pub fn router(access: PortalAccess, limits: PortalInputLimits) -> (Router, PortalInputSource) {
    assert!(limits.connections > 0 && limits.devices > 0 && limits.queued_events > 0);
    assert!(limits.incoming_bytes > 0 && limits.event_bytes > 0);
    assert!(
        !limits.handshake_timeout.is_zero()
            && !limits.write_timeout.is_zero()
            && !limits.max_queue_age.is_zero()
    );
    let shared = Arc::new(Shared {
        limits,
        state: Mutex::new(SourceState {
            connected: true,
            devices: BTreeMap::new(),
            clients: (0..limits.connections).map(|_| None).collect(),
        }),
    });
    let app = Router::new().route("/", get(upgrade)).with_state(Endpoint {
        access,
        shared: Arc::clone(&shared),
    });
    (app, PortalInputSource { shared })
}

#[derive(Clone)]
struct Endpoint {
    access: PortalAccess,
    shared: Arc<Shared>,
}

// The reservation exists before upgrade and throughout authentication, so idle
// unauthenticated connections consume the same finite budget. Drop frees it on
// every exit, including a failed HTTP upgrade or a cancelled socket task.
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
        if !state.connected || *client.stop.borrow() {
            return None;
        }
        let (queue, receiver) = mpsc::channel(self.shared.limits.queued_events);
        let snapshot = state
            .devices
            .values()
            .filter(|device| subscription.classes.contains(&device.class))
            .map(|device| Frame {
                text: Arc::clone(&device.text),
                created: Instant::now(),
            })
            .collect();
        // Snapshot and live subscription share one lock with publication. A
        // replacement discards the previous queue; no old input is replayed.
        state.clients[self.slot].as_mut()?.subscription = Some(Subscription {
            classes: subscription.classes,
            queue,
        });
        Some((snapshot, receiver))
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
    let (connection, stop) = {
        let mut state = endpoint
            .shared
            .state
            .lock()
            .expect("native input mutex poisoned");
        if !state.connected {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        let slot = state
            .clients
            .iter()
            .position(Option::is_none)
            .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
        let (stop, receiver) = watch::channel(false);
        state.clients[slot] = Some(Client {
            stop,
            subscription: None,
        });
        (
            Connection {
                shared: Arc::clone(&endpoint.shared),
                slot,
            },
            receiver,
        )
    };
    let limits = endpoint.shared.limits;
    Ok(ws
        .max_message_size(limits.incoming_bytes)
        .max_frame_size(limits.incoming_bytes)
        .read_buffer_size(limits.incoming_bytes)
        .write_buffer_size(0)
        .max_write_buffer_size(limits.event_bytes.saturating_add(1024))
        .on_upgrade(move |socket| serve(socket, endpoint.access, connection, stop)))
}

async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
}

async fn handshake(
    socket: &mut WebSocket,
    access: &PortalAccess,
) -> Option<NativeInputSubscription> {
    let Message::Text(bearer) = socket.recv().await?.ok()? else {
        return None;
    };
    access.authorize_bearer(&bearer).ok()?;
    // Do not retain the credential through subscription handling.
    drop(bearer);
    let Message::Text(text) = socket.recv().await?.ok()? else {
        return None;
    };
    serde_json::from_str(&text).ok()
}

async fn send(
    socket: &mut WebSocket,
    frame: Frame,
    limits: PortalInputLimits,
    stop: &mut watch::Receiver<bool>,
) -> bool {
    let Some(remaining) = limits.max_queue_age.checked_sub(frame.created.elapsed()) else {
        return false;
    };
    tokio::select! {
        biased;
        _ = stopped(stop) => false,
        result = timeout(limits.write_timeout.min(remaining), socket.send(Message::Text(frame.text.as_ref().into()))) => {
            matches!(result, Ok(Ok(())))
        }
    }
}

async fn serve(
    mut socket: WebSocket,
    access: PortalAccess,
    connection: Connection,
    mut stop: watch::Receiver<bool>,
) {
    let limits = connection.shared.limits;
    let subscription = tokio::select! {
        biased;
        _ = stopped(&mut stop) => return,
        result = timeout(limits.handshake_timeout, handshake(&mut socket, &access)) => {
            match result { Ok(Some(subscription)) => subscription, _ => return }
        }
    };
    let Some((mut snapshot, mut queue)) = connection.subscribe(subscription) else {
        return;
    };
    loop {
        for frame in snapshot.drain(..) {
            if !send(&mut socket, frame, limits, &mut stop).await {
                return;
            }
        }
        tokio::select! {
            biased;
            _ = stopped(&mut stop) => return,
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    // No event/controller injection, extra fields, auth objects,
                    // or other browser commands are accepted. Never log text or
                    // serde/tungstenite errors, which may contain credentials.
                    let Ok(subscription) = serde_json::from_str::<NativeInputSubscription>(&text) else { return };
                    let Some((next_snapshot, next_queue)) = connection.subscribe(subscription) else { return };
                    snapshot = next_snapshot;
                    queue = next_queue;
                }
                Some(Ok(Message::Ping(_))) => {
                    // Tungstenite queues the protocol Pong; bound its flush too.
                    tokio::select! {
                        biased;
                        _ = stopped(&mut stop) => return,
                        result = timeout(limits.write_timeout, socket.flush()) => {
                            if !matches!(result, Ok(Ok(()))) { return; }
                        }
                    }
                }
                Some(Ok(Message::Pong(_))) => {},
                _ => return,
            },
            frame = queue.recv() => {
                let Some(frame) = frame else { return };
                if !send(&mut socket, frame, limits, &mut stop).await { return; }
            }
        }
    }
    // Every exit drops the socket without draining the application queue.
    // Bytes already accepted by TCP are outside this queue (see module docs).
}

fn finite(event: &NativeInputEvent) -> Result<(), PublishError> {
    let valid = match event {
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
    if valid {
        Ok(())
    } else {
        Err(PublishError::NonFinite)
    }
}

// Serialize into a bounded allocation, rather than allocating an arbitrarily
// large JSON string and checking only after the allocation already happened.
fn encode(event: &NativeInputEvent, limit: usize) -> Result<Arc<str>, PublishError> {
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
