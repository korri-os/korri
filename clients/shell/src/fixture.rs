//! A fixture korrid for tests and workstation proofs: a real loopback server
//! that speaks contract section 6 and korrid's native input subscription,
//! with configurable behaviour. It is never shipped (feature `fixture`).
//!
//! It checks what korrid checks on this wire: the bearer capability on every
//! call and socket, the WebSocket `Origin`, declared preference keys and
//! sizes, and the ack before the next model push.

use crate::{catalog, engine, messages};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{
    handshake::server::{Request, Response},
    Message,
};

/// What the fixture does with a surface.
#[derive(Clone, Debug, Default)]
pub struct Behaviour {
    /// Refuse `app.surface.declare` for these surface ids.
    pub refuse_declare: Vec<String>,
    /// Refuse every `app.surface.request` whose tag is listed.
    pub refuse_requests: Vec<String>,
}

#[derive(Default)]
struct Surface {
    declared: Option<engine::Declaration>,
    /// `displays` from the last declare; korrid publishes `model.displays`
    /// only for `"all"`.
    displays: Option<engine::Displays>,
    stored: BTreeMap<String, String>,
    query: Option<Value>,
    from: Option<Value>,
    count: usize,
    focused: Option<String>,
    revision: u64,
}

struct State {
    base: Value,
    games: Vec<Value>,
    surfaces: BTreeMap<String, Surface>,
    behaviour: Behaviour,
    /// Every request received, in order: (surface, request).
    requests: Vec<(String, Value)>,
    /// Every revision acked: (surface, revision).
    acks: Vec<(String, String)>,
    /// Native input frames to send to subscribers.
    input: VecDeque<String>,
    /// Device frames every new subscription starts with, as korrid's snapshot.
    devices: Vec<String>,
    input_subscribers: usize,
    calls: Vec<String>,
    declarations: Vec<Value>,
    generation: u64,
}

pub struct Fixture {
    port: u16,
    capability: String,
    origin: String,
    shared: Arc<(Mutex<State>, Condvar)>,
}

const ORIGIN: &str = "http://127.0.0.1:9";

impl Fixture {
    /// Serve `model` (a `SurfaceModel` whose catalog is replaced by a cursor
    /// window over `games`).
    pub fn start(model: Value, games: Vec<Value>, behaviour: Behaviour) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let capability = format!(
            "{:064x}",
            std::process::id() as u128 * 0x9e37_79b9_7f4a_7c15 + port as u128
        );
        let shared = Arc::new((
            Mutex::new(State {
                base: model,
                games,
                surfaces: BTreeMap::new(),
                behaviour,
                requests: Vec::new(),
                acks: Vec::new(),
                input: VecDeque::new(),
                devices: Vec::new(),
                input_subscribers: 0,
                calls: Vec::new(),
                declarations: Vec::new(),
                generation: 0,
            }),
            Condvar::new(),
        ));
        let fixture = Self {
            port,
            capability: capability.clone(),
            origin: ORIGIN.to_owned(),
            shared: shared.clone(),
        };
        thread::Builder::new()
            .name("fixture-korrid".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let shared = shared.clone();
                    let capability = capability.clone();
                    thread::spawn(move || {
                        let _ = serve(stream, &shared, &capability);
                    });
                }
            })?;
        Ok(fixture)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn connection(&self) -> crate::config::Connection {
        crate::config::Connection::new(self.port, self.origin.clone(), self.capability.clone())
    }

    /// Write the capability where systemd's `LoadCredential` would.
    pub fn write_credential(&self, directory: &std::path::Path) -> io::Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.join("KORRID_RPC_CAPABILITY"))?;
        writeln!(file, "{}", self.capability)
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn requests(&self) -> Vec<(String, Value)> {
        self.shared.0.lock().unwrap().requests.clone()
    }

    pub fn acks(&self) -> Vec<(String, String)> {
        self.shared.0.lock().unwrap().acks.clone()
    }

    pub fn calls(&self) -> Vec<String> {
        self.shared.0.lock().unwrap().calls.clone()
    }

    /// Every `app.surface.declare` payload received, in order.
    pub fn declarations(&self) -> Vec<Value> {
        self.shared.0.lock().unwrap().declarations.clone()
    }

    /// The model the fixture would publish to `surface` now.
    pub fn model(&self, surface: &str) -> Value {
        model_for(&self.shared.0.lock().unwrap(), surface)
    }

    pub fn stored(&self, surface: &str) -> BTreeMap<String, String> {
        self.shared
            .0
            .lock()
            .unwrap()
            .surfaces
            .get(surface)
            .map(|s| s.stored.clone())
            .unwrap_or_default()
    }

    pub fn store(&self, surface: &str, key: &str, value: &str) {
        let mut state = self.shared.0.lock().unwrap();
        state
            .surfaces
            .entry(surface.to_owned())
            .or_default()
            .stored
            .insert(key.to_owned(), value.to_owned());
    }

    /// Change the published model for every surface and push it.
    pub fn publish(&self, change: impl FnOnce(&mut Value)) {
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        change(&mut state.base);
        for surface in state.surfaces.values_mut() {
            surface.revision += 1;
        }
        changed.notify_all();
    }

    pub fn set_overlay(&self, shown: bool) {
        self.publish(|model| {
            model["presentation"] = if shown {
                json!({"kind": "gameplay-overlay", "title": "Paused", "controls": [{"id": "resume", "label": "Resume", "enabled": true, "destructive": false, "dismissOnSuccess": true, "interaction": {"kind": "command"}}], "groups": []})
            } else {
                json!({"kind": "catalog"})
            };
        });
    }

    /// Queue native input frames (already JSON) for subscribers.
    pub fn send_input(&self, frames: &[Value]) {
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        for frame in frames {
            match frame["kind"].as_str() {
                Some("device-added") | Some("device-state-complete") => {
                    state.devices.push(frame.to_string())
                }
                Some("initialization-complete") => continue, // sent per subscription
                _ => {}
            }
            state.input.push_back(frame.to_string());
        }
        changed.notify_all();
    }

    pub fn input_subscribers(&self) -> usize {
        self.shared.0.lock().unwrap().input_subscribers
    }

    /// Wait until `test` holds, or panic after `timeout`.
    pub fn wait(&self, timeout: Duration, mut test: impl FnMut(&Self) -> bool) {
        let started = Instant::now();
        while !test(self) {
            assert!(
                started.elapsed() < timeout,
                "fixture korrid: condition not met in {timeout:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// One gamepad with a neutral baseline. It is part of every later
    /// subscription's snapshot, as korrid would send it.
    pub fn gamepad_frames(device: &str) -> Vec<Value> {
        vec![
            json!({"kind": "device-added", "device": {"deviceId": device, "class": "gamepad", "name": "Fixture pad", "capabilities": []}}),
            json!({"kind": "device-state-complete", "deviceId": device}),
        ]
    }

    /// A press and release of one gamepad key.
    pub fn press_frames(device: &str, code: u16) -> Vec<Value> {
        let key = |value: u8| json!({"kind": "input", "deviceId": device, "class": "gamepad", "type": 1, "code": code, "value": value, "timestamp": 0});
        let report = json!({"kind": "input", "deviceId": device, "class": "gamepad", "type": 0, "code": 0, "value": 0, "timestamp": 0});
        vec![key(1), report.clone(), key(0), report]
    }
}

fn model_for(state: &State, surface_id: &str) -> Value {
    let surface = state.surfaces.get(surface_id);
    let mut model = state.base.clone();
    let query = surface
        .and_then(|s| s.query.clone())
        .unwrap_or_else(catalog::default_query);
    let from = surface
        .and_then(|s| s.from.clone())
        .unwrap_or_else(|| json!({"_tag": "Start"}));
    let count = surface.map_or(catalog::DEFAULT_COUNT, |s| {
        if s.count == 0 {
            catalog::DEFAULT_COUNT
        } else {
            s.count
        }
    });
    model["catalog"] = catalog::window(&state.games, &query, &from, count);
    if surface.and_then(|s| s.displays) == Some(engine::Displays::All) {
        // One main display; the fixture has no compositor to read outputs from.
        model["displays"] = json!([{"id": "fixture", "main": true, "x": 0, "y": 0, "width": 640, "height": 480, "touch": false}]);
    }
    if let Some(id) = surface.and_then(|s| s.focused.as_deref()) {
        model["focusedGame"] = json!({"gameId": id, "actions": []});
    }
    model
}

/// Replay bytes already read before handing a stream to tungstenite.
struct Replay {
    prefix: io::Cursor<Vec<u8>>,
    stream: TcpStream,
}

impl Read for Replay {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.prefix.read(buffer)?;
        if read > 0 {
            return Ok(read);
        }
        self.stream.read(buffer)
    }
}

impl Write for Replay {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.stream.write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn serve(
    stream: TcpStream,
    shared: &Arc<(Mutex<State>, Condvar)>,
    capability: &str,
) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut head = Vec::new();
    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        head.extend_from_slice(line.as_bytes());
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(':') {
            headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let bearer = format!("Bearer {capability}");
    if headers
        .get("upgrade")
        .is_some_and(|u| u.eq_ignore_ascii_case("websocket"))
    {
        if headers.get("origin").map(String::as_str) != Some(ORIGIN) {
            let mut stream = stream;
            return stream.write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n");
        }
        let mut rest = reader.buffer().to_vec();
        head.append(&mut rest);
        let replay = Replay {
            prefix: io::Cursor::new(head),
            stream,
        };
        let mut socket =
            tungstenite::accept_hdr(replay, |_: &Request, response: Response| Ok(response))
                .map_err(io::Error::other)?;
        let Message::Text(first) = socket.read().map_err(io::Error::other)? else {
            return Ok(());
        };
        if first.as_str() != bearer {
            return Ok(());
        }
        let Message::Text(second) = socket.read().map_err(io::Error::other)? else {
            return Ok(());
        };
        let second: Value = serde_json::from_str(&second).map_err(io::Error::other)?;
        if second.get("_tag").and_then(Value::as_str) == Some("app.surface.watch") {
            let surface = second
                .pointer("/payload/surfaceId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            return watch(socket, shared, &surface);
        }
        if second.get("classes").is_some() {
            return input(socket, shared);
        }
        return Ok(());
    }
    let length: usize = headers
        .get("content-length")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let mut stream = stream;
    if headers.get("authorization").map(String::as_str) != Some(bearer.as_str()) {
        return stream.write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n");
    }
    let request: Value = serde_json::from_slice(&body).map_err(io::Error::other)?;
    let reply = rpc(shared, &request);
    let reply = reply.to_string();
    // Chunked, as hyper may send it, so the client's chunk reader is used.
    write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ntransfer-encoding: chunked\r\n\r\n{:x}\r\n{reply}\r\n0\r\n\r\n", reply.len())
}

fn ok(method: &str, payload: Value) -> Value {
    json!({"_tag": method, "outcome": {"_tag": "Ok", "payload": payload}})
}

fn err(method: &str, message: &str) -> Value {
    json!({"_tag": method, "outcome": {"_tag": "Err", "payload": {"_tag": "Invalid", "message": message}}})
}

fn rpc(shared: &Arc<(Mutex<State>, Condvar)>, request: &Value) -> Value {
    let (lock, changed) = &**shared;
    let mut state = lock.lock().unwrap();
    let method = request
        .get("_tag")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    state.calls.push(method.clone());
    let payload = request.get("payload").cloned().unwrap_or(Value::Null);
    let Some(surface_id) = payload
        .get("surfaceId")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return err(&method, "surfaceId required");
    };
    match method.as_str() {
        "app.surface.declare" => {
            state.declarations.push(payload.clone());
            if state.behaviour.refuse_declare.contains(&surface_id) {
                return err(&method, "declaration refused");
            }
            if let Some(key) = payload.as_object().and_then(|o| {
                o.keys()
                    .find(|k| !["surfaceId", "preferences", "displays"].contains(&k.as_str()))
            }) {
                return err(&method, &format!("unknown field {key}"));
            }
            let displays = match engine::check_displays(payload.get("displays")) {
                Ok(displays) => displays,
                Err(message) => return err(&method, &message),
            };
            match engine::check_declaration(payload.get("preferences").unwrap_or(&Value::Null)) {
                Ok(declaration) => {
                    let surface = state.surfaces.entry(surface_id).or_default();
                    surface.declared = Some(declaration);
                    surface.displays = displays;
                    ok(&method, Value::Null)
                }
                Err(message) => err(&method, &message),
            }
        }
        "app.surface.preferences" => {
            let surface = state.surfaces.entry(surface_id).or_default();
            let Some(declared) = &surface.declared else {
                return err(&method, "declare first");
            };
            let values = engine::effective_preferences(declared, &surface.stored);
            ok(&method, json!(values))
        }
        "app.surface.model" => {
            state.surfaces.entry(surface_id.clone()).or_default();
            let model = model_for(&state, &surface_id);
            ok(&method, model)
        }
        "app.surface.request" => {
            let request = payload.get("request").cloned().unwrap_or(Value::Null);
            state.requests.push((surface_id.clone(), request.clone()));
            let Some(tag) = messages::request_tag(&request).map(str::to_owned) else {
                return err(&method, "unknown request");
            };
            if state.behaviour.refuse_requests.contains(&tag) {
                return err(&method, "refused by the fixture");
            }
            let surface = state.surfaces.entry(surface_id).or_default();
            let mut publish = false;
            match tag.as_str() {
                "RememberPreference" => {
                    let (Some(key), Some(value)) = (
                        request.get("key").and_then(Value::as_str),
                        request.get("value").and_then(Value::as_str),
                    ) else {
                        return err(&method, "key and value required");
                    };
                    if !surface
                        .declared
                        .as_ref()
                        .is_some_and(|d| d.contains_key(key))
                        || value.len() > engine::MAX_PREFERENCE_BYTES
                    {
                        return err(&method, "undeclared key or oversized value");
                    }
                    surface.stored.insert(key.to_owned(), value.to_owned());
                }
                "MoveCatalog" => {
                    let count = request
                        .get("count")
                        .and_then(Value::as_u64)
                        .unwrap_or(catalog::DEFAULT_COUNT as u64);
                    if count == 0 || count > messages::MAX_CATALOG_COUNT {
                        return err(&method, "count must be 1..=200");
                    }
                    surface.query = request.get("query").cloned();
                    surface.from = request.get("from").cloned();
                    surface.count = count as usize;
                    publish = true;
                }
                "FocusGame" => {
                    surface.focused = request
                        .get("gameId")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    publish = true;
                }
                "DismissGameplayOverlay" => {
                    drop(state);
                    let mut state = lock.lock().unwrap();
                    state.base["presentation"] = json!({"kind": "catalog"});
                    for surface in state.surfaces.values_mut() {
                        surface.revision += 1;
                    }
                    changed.notify_all();
                    return ok(&method, Value::Null);
                }
                _ => {}
            }
            if publish {
                surface.revision += 1;
                changed.notify_all();
            }
            ok(&method, Value::Null)
        }
        _ => err(&method, "unknown method"),
    }
}

fn watch(
    mut socket: tungstenite::WebSocket<Replay>,
    shared: &Arc<(Mutex<State>, Condvar)>,
    surface_id: &str,
) -> io::Result<()> {
    let (lock, changed) = &**shared;
    socket
        .get_ref()
        .stream
        .set_read_timeout(Some(Duration::from_millis(20)))?;
    let mut sent: Option<u64> = None;
    let mut acked = true;
    loop {
        // Push when acked and the revision moved on.
        let push = {
            let mut state = lock.lock().unwrap();
            let revision = state
                .surfaces
                .entry(surface_id.to_owned())
                .or_default()
                .revision
                + 1;
            if acked && sent != Some(revision) {
                Some((revision, model_for(&state, surface_id)))
            } else {
                drop(
                    changed
                        .wait_timeout(state, Duration::from_millis(20))
                        .unwrap(),
                );
                None
            }
        };
        if let Some((revision, model)) = push {
            let frame = json!({"kind": "model", "revision": revision.to_string(), "model": model})
                .to_string();
            if socket.send(Message::Text(frame.into())).is_err() {
                return Ok(());
            }
            sent = Some(revision);
            acked = false;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let ack: Value = serde_json::from_str(&text).map_err(io::Error::other)?;
                let revision = ack
                    .get("revision")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if ack.get("kind").and_then(Value::as_str) != Some("ack")
                    || Some(revision.as_str()) != sent.map(|s| s.to_string()).as_deref()
                {
                    return Ok(()); // a wrong ack closes the watch, as korrid would
                }
                lock.lock()
                    .unwrap()
                    .acks
                    .push((surface_id.to_owned(), revision));
                acked = true;
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Ok(()),
        }
    }
}

fn input(
    mut socket: tungstenite::WebSocket<Replay>,
    shared: &Arc<(Mutex<State>, Condvar)>,
) -> io::Result<()> {
    let (lock, changed) = &**shared;
    let (snapshot, generation) = {
        let mut state = lock.lock().unwrap();
        state.input_subscribers += 1;
        state.generation += 1;
        state.input.clear();
        (state.devices.clone(), state.generation)
    };
    socket
        .get_ref()
        .stream
        .set_read_timeout(Some(Duration::from_millis(20)))?;
    let result = (|| -> io::Result<()> {
        for frame in snapshot {
            socket
                .send(Message::Text(frame.into()))
                .map_err(io::Error::other)?;
        }
        let complete =
            json!({"kind": "initialization-complete", "generation": generation.to_string()})
                .to_string();
        socket
            .send(Message::Text(complete.into()))
            .map_err(io::Error::other)?;
        loop {
            let frames: Vec<String> = {
                let mut state = lock.lock().unwrap();
                if state.input.is_empty() {
                    state = changed
                        .wait_timeout(state, Duration::from_millis(20))
                        .unwrap()
                        .0;
                }
                state.input.drain(..).collect()
            };
            for frame in frames {
                socket
                    .send(Message::Text(frame.into()))
                    .map_err(io::Error::other)?;
            }
            match socket.read() {
                Ok(Message::Close(_)) => return Ok(()),
                Ok(Message::Text(text)) => {
                    // A retire is answered as korrid answers it.
                    let message: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                    if message["kind"] == "retire" {
                        let retired =
                            json!({"kind": "retired", "generation": message["generation"]})
                                .to_string();
                        socket
                            .send(Message::Text(retired.into()))
                            .map_err(io::Error::other)?;
                    }
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return Ok(()),
            }
        }
    })();
    lock.lock().unwrap().input_subscribers -= 1;
    result
}
