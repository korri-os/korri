//! korrid's native input subscription, as the portal uses it
//! (`clients/portal/src/input/native-connection.ts` and `native-decoder.ts`):
//! one authenticated root WebSocket, `{"classes":["gamepad"]}`, the same
//! decoder bounds, the same suspend / resume / retire lifecycle and the same
//! reconnect backoff. The shell only receives: it never injects input, never
//! reads `/dev/input` and needs no privilege.

use crate::{config::Connection, korrid::open_socket, input::mapper::Axis};
use serde_json::{json, Value};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};
use tungstenite::Message;

const RECONNECT_INITIAL: Duration = Duration::from_millis(250);
const RECONNECT_MAX: Duration = Duration::from_secs(5);
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const POLL: Duration = Duration::from_millis(20);

/// What the connection delivers to the shell, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeEvent {
    /// Held state must be cleared (lost socket, suspend, retire).
    Disconnected,
    Initialized,
    DeviceInitialized(String),
    DeviceAdded { id: String, gamepad: bool, axes: Vec<Axis> },
    DeviceRemoved(String),
    Input { device: String, gamepad: bool, kind: u32, code: u16, value: i64 },
}

/// The decoded wire message (`NativeInputEvent | NativeInputControl`).
#[derive(Clone, Debug, PartialEq)]
pub enum Decoded {
    InitializationComplete { generation: String },
    Retired { generation: String },
    DeviceStateComplete { device: String },
    Suspend { generation: String, request: String },
    Resume { generation: String, request: String },
    Event(NativeEvent),
    /// `action` (system): the host's, never acted on here, as in the portal.
    Action,
}

const CLASSES: [&str; 6] = ["gamepad", "keyboard", "mouse", "touch", "system", "unknown"];

fn keys_only(object: &serde_json::Map<String, Value>, allowed: &[&str]) -> bool {
    object.keys().all(|k| allowed.contains(&k.as_str()))
}
fn identity(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    let valid = (1..=20).contains(&text.len()) && text.as_bytes()[0] != b'0' && text.bytes().all(|b| b.is_ascii_digit());
    valid.then(|| text.to_owned())
}
fn id(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    (1..=1024).contains(&text.len()).then(|| text.to_owned())
}
fn integer(value: Option<&Value>, min: i64, max: i64) -> Option<i64> {
    let number = value?.as_f64()?;
    (number.fract() == 0.0 && number >= min as f64 && number <= max as f64).then_some(number as i64)
}
fn class(value: Option<&Value>) -> Option<&str> {
    let text = value?.as_str()?;
    CLASSES.contains(&text).then_some(text)
}

/// Runtime validation of korrid's Rust-owned treaty, the decoder's rules.
pub fn decode(text: &str) -> Option<Decoded> {
    let value: Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    let decoded = match object.get("kind")?.as_str()? {
        "initialization-complete" if keys_only(object, &["kind", "generation"]) => Decoded::InitializationComplete { generation: identity(object.get("generation"))? },
        "retired" if keys_only(object, &["kind", "generation"]) => Decoded::Retired { generation: identity(object.get("generation"))? },
        "device-state-complete" if keys_only(object, &["kind", "deviceId"]) => Decoded::DeviceStateComplete { device: id(object.get("deviceId"))? },
        "suspend" if keys_only(object, &["kind", "generation", "requestId"]) => Decoded::Suspend {
            generation: identity(object.get("generation"))?,
            request: identity(object.get("requestId"))?,
        },
        "resume" if keys_only(object, &["kind", "generation", "requestId"]) => Decoded::Resume {
            generation: identity(object.get("generation"))?,
            request: identity(object.get("requestId"))?,
        },
        "device-added" if keys_only(object, &["kind", "device"]) => {
            let device = object.get("device")?.as_object()?;
            if !keys_only(device, &["deviceId", "class", "name", "capabilities", "axes"]) {
                return None;
            }
            let device_id = id(device.get("deviceId"))?;
            let gamepad = class(device.get("class"))? == "gamepad";
            device.get("name")?.as_str()?;
            let capabilities = device.get("capabilities")?.as_array()?;
            if capabilities.len() > 64 || !capabilities.iter().all(|c| c.as_str().is_some_and(|c| c.len() <= 128)) {
                return None;
            }
            let mut axes = Vec::new();
            if let Some(list) = device.get("axes") {
                let list = list.as_array()?;
                if list.len() > 64 {
                    return None;
                }
                for axis in list {
                    let axis = axis.as_object()?;
                    if !keys_only(axis, &["code", "minimum", "maximum", "flat"]) {
                        return None;
                    }
                    let code = integer(axis.get("code"), 0, 63)? as u16;
                    if axes.iter().any(|a: &Axis| a.code == code) {
                        return None;
                    }
                    let finite = |v: Option<&Value>| v.and_then(Value::as_f64).filter(|n| n.is_finite());
                    let flat = match axis.get("flat") {
                        None => None,
                        Some(v) => Some(finite(Some(v))?),
                    };
                    axes.push(Axis { code, minimum: finite(axis.get("minimum"))?, maximum: finite(axis.get("maximum"))?, flat });
                }
            }
            Decoded::Event(NativeEvent::DeviceAdded { id: device_id, gamepad, axes })
        }
        "device-removed" if keys_only(object, &["kind", "deviceId"]) => Decoded::Event(NativeEvent::DeviceRemoved(id(object.get("deviceId"))?)),
        "input" if keys_only(object, &["kind", "deviceId", "class", "type", "code", "value", "timestamp"]) => {
            let kind = integer(object.get("type"), 0, 31)?;
            let code = integer(object.get("code"), 0, if kind == 3 { 63 } else { 767 })?;
            let (min, max) = if kind == 1 { (0, 2) } else { (-2_147_483_648, 2_147_483_647) };
            let value = integer(object.get("value"), min, max)?;
            object.get("timestamp")?.as_f64().filter(|t| t.is_finite())?;
            Decoded::Event(NativeEvent::Input {
                device: id(object.get("deviceId"))?,
                gamepad: class(object.get("class"))? == "gamepad",
                kind: kind as u32,
                code: code as u16,
                value,
            })
        }
        "action" if keys_only(object, &["kind", "class", "action", "timestamp"]) => {
            class(object.get("class"))?;
            if object.get("action")?.as_str()? != "system" || !object.get("timestamp")?.as_f64()?.is_finite() {
                return None;
            }
            Decoded::Action
        }
        _ => return None,
    };
    Some(decoded)
}

enum Command {
    Active(bool),
}

/// The running subscription. Dropping it closes the socket.
pub struct NativeInput {
    commands: mpsc::Sender<Command>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl NativeInput {
    pub fn start(connection: Connection, active: bool, events: Box<dyn Fn(NativeEvent) + Send>) -> Self {
        let (commands, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("korrid-native-input".into())
            .spawn(move || run(&connection, active, &*events, &receiver, &thread_stop))
            .expect("spawn the native input thread");
        Self { commands, stop, thread: Some(thread) }
    }

    /// The shell lost or regained focus (`setActive` in the portal).
    pub fn set_active(&self, active: bool) {
        let _ = self.commands.send(Command::Active(active));
    }
}

impl Drop for NativeInput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct Attachment {
    retired: bool,
    release_requested: bool,
    generation: Option<String>,
    request: Option<String>,
}

enum End {
    /// Reconnect after the backoff.
    Lost,
    /// Reconnect at once with a fresh baseline (resume).
    Fresh,
}

fn run(connection: &Connection, mut active: bool, events: &dyn Fn(NativeEvent), commands: &mpsc::Receiver<Command>, stop: &AtomicBool) {
    let mut delay = RECONNECT_INITIAL;
    while !stop.load(Ordering::SeqCst) {
        while let Ok(Command::Active(value)) = commands.try_recv() {
            active = value;
        }
        if !active {
            thread::sleep(POLL);
            continue;
        }
        let mut connected = false;
        let end = match open_socket(connection, MAX_MESSAGE_BYTES) {
            Ok(mut socket) => {
                let end = if socket.send(Message::Text(json!({"classes": ["gamepad"]}).to_string().into())).is_ok() {
                    serve(&mut socket, &mut active, &mut connected, &mut delay, events, commands, stop)
                } else {
                    End::Lost
                };
                let _ = socket.close(None);
                end
            }
            Err(_) => End::Lost,
        };
        if connected {
            events(NativeEvent::Disconnected);
        }
        if let End::Lost = end {
            let mut waited = Duration::ZERO;
            while waited < delay && !stop.load(Ordering::SeqCst) {
                thread::sleep(POLL);
                waited += POLL;
            }
            delay = (delay * 2).min(RECONNECT_MAX);
        }
    }
}

fn serve(
    socket: &mut crate::korrid::Socket,
    active: &mut bool,
    connected: &mut bool,
    delay: &mut Duration,
    events: &dyn Fn(NativeEvent),
    commands: &mpsc::Receiver<Command>,
    stop: &AtomicBool,
) -> End {
    let mut attachment = Attachment::default();
    let send = |socket: &mut crate::korrid::Socket, value: Value| socket.send(Message::Text(value.to_string().into())).is_ok();
    while !stop.load(Ordering::SeqCst) {
        while let Ok(Command::Active(value)) = commands.try_recv() {
            *active = value;
        }
        if !*active && !attachment.retired {
            // Retire locally first, then ask korrid to release this generation.
            attachment.retired = true;
            if std::mem::take(connected) {
                events(NativeEvent::Disconnected);
            }
        }
        if attachment.retired && !attachment.release_requested && attachment.request.is_none() {
            if let Some(generation) = attachment.generation.clone() {
                attachment.release_requested = true;
                if !send(socket, json!({"kind": "retire", "generation": generation})) {
                    return End::Lost;
                }
            }
        }
        let text = match socket.read() {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => return End::Lost,
            Ok(_) => return End::Lost,
            Err(tungstenite::Error::Io(error)) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
            Err(_) => return End::Lost,
        };
        if text.len() > MAX_MESSAGE_BYTES {
            return End::Lost;
        }
        // Never include a frame or decoder error in diagnostics.
        let Some(message) = decode(&text) else {
            return End::Lost;
        };
        match message {
            Decoded::Suspend { generation, request } => {
                if attachment.request.is_some() || attachment.generation.as_ref().is_some_and(|g| *g != generation) {
                    return End::Lost;
                }
                attachment.generation = Some(generation.clone());
                attachment.request = Some(request.clone());
                attachment.retired = true;
                if std::mem::take(connected) {
                    events(NativeEvent::Disconnected);
                }
                if !send(socket, json!({"kind": "suspended", "generation": generation, "requestId": request})) {
                    return End::Lost;
                }
            }
            Decoded::Retired { generation } => {
                if !attachment.retired || !attachment.release_requested || attachment.generation.as_deref() != Some(generation.as_str()) {
                    return End::Lost;
                }
                return End::Lost;
            }
            Decoded::Resume { generation, request } => {
                if !attachment.retired || attachment.generation.as_deref() != Some(generation.as_str()) || attachment.request.as_deref() != Some(request.as_str()) {
                    return End::Lost;
                }
                return End::Fresh; // never reactivate; reconnect with a baseline
            }
            Decoded::InitializationComplete { generation } => {
                if attachment.generation.is_some() {
                    return End::Lost;
                }
                attachment.generation = Some(generation);
                if attachment.retired {
                    continue; // the release request goes out at the loop top
                }
                *delay = RECONNECT_INITIAL;
                *connected = true;
                events(NativeEvent::Initialized);
            }
            _ if attachment.retired => {}
            Decoded::DeviceStateComplete { device } => {
                *delay = RECONNECT_INITIAL;
                events(NativeEvent::DeviceInitialized(device));
            }
            Decoded::Event(event) => {
                *delay = RECONNECT_INITIAL;
                *connected = true;
                events(event);
            }
            Decoded::Action => {}
        }
    }
    End::Lost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_within_the_portal_bounds() {
        assert_eq!(decode(r#"{"kind":"initialization-complete","generation":"1"}"#), Some(Decoded::InitializationComplete { generation: "1".into() }));
        assert_eq!(decode(r#"{"kind":"initialization-complete","generation":"01"}"#), None);
        assert_eq!(decode(r#"{"kind":"input","deviceId":"pad","class":"gamepad","type":1,"code":304,"value":1,"timestamp":0}"#), Some(Decoded::Event(NativeEvent::Input { device: "pad".into(), gamepad: true, kind: 1, code: 304, value: 1 })));
        assert_eq!(decode(r#"{"kind":"input","deviceId":"pad","class":"gamepad","type":1,"code":304,"value":3,"timestamp":0}"#), None);
        assert_eq!(decode(r#"{"kind":"input","deviceId":"pad","class":"gamepad","type":3,"code":64,"value":3,"timestamp":0}"#), None);
        assert_eq!(decode(r#"{"kind":"input","deviceId":"pad","class":"gamepad","type":1,"code":304,"value":1,"timestamp":0,"extra":1}"#), None);
        assert!(matches!(decode(r#"{"kind":"device-added","device":{"deviceId":"pad","class":"gamepad","name":"Pad","capabilities":[],"axes":[{"code":0,"minimum":-1,"maximum":1}]}}"#), Some(Decoded::Event(NativeEvent::DeviceAdded { gamepad: true, .. }))));
        assert_eq!(decode(r#"{"kind":"device-added","device":{"deviceId":"pad","class":"gamepad","name":"Pad","capabilities":[],"axes":[{"code":0,"minimum":-1,"maximum":1},{"code":0,"minimum":-1,"maximum":1}]}}"#), None);
        assert_eq!(decode(r#"{"kind":"action","class":"system","action":"system","timestamp":1}"#), Some(Decoded::Action));
        assert_eq!(decode("[]"), None);
    }
}
