//! The shell's side of the shell ↔ korrid wire (contract section 6).
//!
//! HTTP: `POST http://127.0.0.1:PORT/rpc`, `Authorization: Bearer CAP`, body
//! `{"_tag": method, "payload": {...}}`, reply `{"_tag": method, "outcome":
//! {"_tag": "Ok", "payload": ...} | {"_tag": "Err", "payload": RpcFailure}}`.
//!
//! | Method | Payload | Ok payload |
//! | --- | --- | --- |
//! | `app.surface.declare` | `{surfaceId, preferences}` | unit |
//! | `app.surface.preferences` | `{surfaceId}` | `Record<string, string>` |
//! | `app.surface.model` | `{surfaceId}` | `SurfaceModel` |
//! | `app.surface.request` | `{surfaceId, request}` | unit |
//!
//! Watch: a WebSocket to `ws://127.0.0.1:PORT/` with `Origin: <portal origin>`.
//! First frame `Bearer CAP`, second `{"_tag":"app.surface.watch","payload":
//! {"surfaceId":…}}`. korrid pushes `{"kind":"model","revision":"N","model":…}`
//! and sends nothing more until the shell replies `{"kind":"ack","revision":"N"}`.
//!
//! Nothing here logs a payload, a failure body or the capability.

use crate::config::Connection;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fmt,
    io::{self, BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};
use tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, Message};

/// Largest HTTP reply or model push accepted. A model carries a catalog
/// window of at most 200 games, possibly with data-URL covers.
pub const MAX_REPLY_BYTES: usize = 64 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);
const RECONNECT_INITIAL: Duration = Duration::from_millis(250);
const RECONNECT_MAX: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum KorridError {
    /// Transport or protocol trouble; the body is never included.
    Unavailable(String),
    /// korrid answered with an `Err` outcome (`RpcFailure`).
    Refused(Value),
}

impl fmt::Display for KorridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(f, "korrid unavailable: {reason}"),
            // RpcFailure is korrid's own typed error; its tag is enough here.
            Self::Refused(failure) => write!(
                f,
                "korrid refused: {}",
                failure
                    .get("_tag")
                    .and_then(Value::as_str)
                    .unwrap_or("error")
            ),
        }
    }
}

impl std::error::Error for KorridError {}

type Result<T> = std::result::Result<T, KorridError>;

fn unavailable(reason: impl fmt::Display) -> KorridError {
    KorridError::Unavailable(reason.to_string())
}

#[derive(Clone, Debug)]
pub struct Korrid {
    connection: Connection,
}

impl Korrid {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn declare(&self, surface: &str, preferences: &Value) -> Result<()> {
        self.call(
            "app.surface.declare",
            json!({"surfaceId": surface, "preferences": preferences}),
        )?;
        Ok(())
    }

    pub fn preferences(&self, surface: &str) -> Result<BTreeMap<String, String>> {
        let payload = self.call("app.surface.preferences", json!({"surfaceId": surface}))?;
        serde_json::from_value(payload).map_err(|_| unavailable("preferences are not a string map"))
    }

    /// The current `SurfaceModel`, as JSON text.
    pub fn model(&self, surface: &str) -> Result<String> {
        let payload = self.call("app.surface.model", json!({"surfaceId": surface}))?;
        if !payload.is_object() {
            return Err(unavailable("model is not an object"));
        }
        Ok(payload.to_string())
    }

    pub fn request(&self, surface: &str, request: &Value) -> Result<()> {
        self.call(
            "app.surface.request",
            json!({"surfaceId": surface, "request": request}),
        )?;
        Ok(())
    }

    fn call(&self, method: &str, payload: Value) -> Result<Value> {
        let body = json!({"_tag": method, "payload": payload}).to_string();
        let reply = post(&self.connection, &body)?;
        let reply: Value =
            serde_json::from_slice(&reply).map_err(|_| unavailable("reply is not JSON"))?;
        if reply.get("_tag").and_then(Value::as_str) != Some(method) {
            return Err(unavailable("reply names another method"));
        }
        let outcome = reply
            .get("outcome")
            .ok_or_else(|| unavailable("reply has no outcome"))?;
        let payload = outcome.get("payload").cloned().unwrap_or(Value::Null);
        match outcome.get("_tag").and_then(Value::as_str) {
            Some("Ok") => Ok(payload),
            Some("Err") => Err(KorridError::Refused(payload)),
            _ => Err(unavailable("outcome is neither Ok nor Err")),
        }
    }

    /// Watch one surface's model. Pushes arrive on `events` in order; the
    /// caller acks each revision through the returned handle.
    pub fn watch(&self, surface: &str, events: Box<dyn Fn(WatchEvent) + Send>) -> Watch {
        let (acks, ack_receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let connection = self.connection.clone();
        let surface = surface.to_owned();
        let thread = thread::Builder::new()
            .name(format!("korrid-watch-{surface}"))
            .spawn(move || watch_loop(&connection, &surface, &*events, &ack_receiver, &thread_stop))
            .expect("spawn the watch thread");
        Watch {
            acks,
            stop,
            thread: Some(thread),
        }
    }
}

pub fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// One HTTP/1.1 request on its own connection. korrid is local; keep-alive
/// buys nothing measurable here.
fn post(connection: &Connection, body: &str) -> Result<Vec<u8>> {
    let mut stream = TcpStream::connect_timeout(&loopback(connection.port), CONNECT_TIMEOUT)
        .map_err(unavailable)?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(unavailable)?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(unavailable)?;
    let head = format!(
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        connection.port,
        connection.bearer().as_str(),
        body.len()
    );
    let head = zeroize::Zeroizing::new(head);
    stream.write_all(head.as_bytes()).map_err(unavailable)?;
    stream.write_all(body.as_bytes()).map_err(unavailable)?;
    read_response(BufReader::new(stream))
}

fn read_response(mut reader: impl BufRead) -> Result<Vec<u8>> {
    let mut line = String::new();
    reader.read_line(&mut line).map_err(unavailable)?;
    let status: u16 = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| unavailable("bad status line"))?;
    let mut length = None;
    let mut chunked = false;
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(unavailable)? == 0 {
            return Err(unavailable("headers ended early"));
        }
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if line.len() > 8192 {
            return Err(unavailable("header too long"));
        }
        if let Some((name, value)) = header.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("content-length") {
                length = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| unavailable("bad length"))?,
                );
            } else if name.eq_ignore_ascii_case("transfer-encoding")
                && value.eq_ignore_ascii_case("chunked")
            {
                chunked = true;
            }
        }
    }
    if status != 200 {
        return Err(unavailable(format!("HTTP {status}")));
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            line.clear();
            reader.read_line(&mut line).map_err(unavailable)?;
            let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or(""), 16)
                .map_err(|_| unavailable("bad chunk"))?;
            if size == 0 {
                break;
            }
            if body.len() + size > MAX_REPLY_BYTES {
                return Err(unavailable("reply too large"));
            }
            let start = body.len();
            body.resize(start + size, 0);
            reader.read_exact(&mut body[start..]).map_err(unavailable)?;
            let mut crlf = [0; 2];
            reader.read_exact(&mut crlf).map_err(unavailable)?;
        }
    } else if let Some(length) = length {
        if length > MAX_REPLY_BYTES {
            return Err(unavailable("reply too large"));
        }
        body.resize(length, 0);
        reader.read_exact(&mut body).map_err(unavailable)?;
    } else {
        reader
            .take(MAX_REPLY_BYTES as u64 + 1)
            .read_to_end(&mut body)
            .map_err(unavailable)?;
        if body.len() > MAX_REPLY_BYTES {
            return Err(unavailable("reply too large"));
        }
    }
    Ok(body)
}

#[derive(Debug)]
pub enum WatchEvent {
    /// A model push. Ack `revision` once it is applied.
    Model { revision: String, model: String },
    /// The watch socket reconnected after a loss; pushes may have been missed.
    Reconnected,
}

/// A running watch. Dropping it closes the socket and joins the thread.
pub struct Watch {
    acks: mpsc::Sender<String>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Watch {
    pub fn ack(&self, revision: &str) {
        let _ = self.acks.send(revision.to_owned());
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub type Socket = tungstenite::WebSocket<TcpStream>;

/// Open an authenticated root WebSocket: `Origin` header, then the bearer
/// frame. Used by the watch and by native input.
pub fn open_socket(connection: &Connection, max_message: usize) -> io::Result<Socket> {
    let stream = TcpStream::connect_timeout(&loopback(connection.port), CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.set_nodelay(true)?;
    let mut request = format!("ws://127.0.0.1:{}/", connection.port)
        .into_client_request()
        .map_err(io::Error::other)?;
    request.headers_mut().insert(
        "Origin",
        connection
            .origin
            .parse()
            .map_err(|_| io::Error::other("origin header"))?,
    );
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(max_message);
    config.max_frame_size = Some(max_message);
    let (mut socket, _) = tungstenite::client::client_with_config(request, stream, Some(config))
        .map_err(io::Error::other)?;
    socket
        .send(Message::Text(connection.bearer().as_str().into()))
        .map_err(io::Error::other)?;
    socket.get_ref().set_read_timeout(Some(POLL))?;
    Ok(socket)
}

fn watch_loop(
    connection: &Connection,
    surface: &str,
    events: &dyn Fn(WatchEvent),
    acks: &mpsc::Receiver<String>,
    stop: &AtomicBool,
) {
    let mut delay = RECONNECT_INITIAL;
    let mut first = true;
    while !stop.load(Ordering::SeqCst) {
        if let Ok(mut socket) = open_socket(connection, MAX_REPLY_BYTES) {
            let subscribe =
                json!({"_tag": "app.surface.watch", "payload": {"surfaceId": surface}}).to_string();
            if socket.send(Message::Text(subscribe.into())).is_ok() {
                if !first {
                    events(WatchEvent::Reconnected);
                }
                first = false;
                delay = RECONNECT_INITIAL;
                serve_watch(&mut socket, events, acks, stop);
            }
            let _ = socket.close(None);
        }
        // Wait out the backoff in short steps so a drop stays prompt.
        let mut waited = Duration::ZERO;
        while waited < delay && !stop.load(Ordering::SeqCst) {
            thread::sleep(POLL);
            waited += POLL;
        }
        delay = (delay * 2).min(RECONNECT_MAX);
    }
}

fn serve_watch(
    socket: &mut Socket,
    events: &dyn Fn(WatchEvent),
    acks: &mpsc::Receiver<String>,
    stop: &AtomicBool,
) {
    let mut pending: Option<String> = None;
    while !stop.load(Ordering::SeqCst) {
        while let Ok(revision) = acks.try_recv() {
            if pending.as_deref() == Some(revision.as_str()) {
                pending = None;
                let ack = json!({"kind": "ack", "revision": revision}).to_string();
                if socket.send(Message::Text(ack.into())).is_err() {
                    return;
                }
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let Ok(push) = serde_json::from_str::<Value>(&text) else {
                    return;
                };
                let (Some("model"), Some(revision), Some(model)) = (
                    push.get("kind").and_then(Value::as_str),
                    push.get("revision").and_then(Value::as_str),
                    push.get("model").filter(|m| m.is_object()),
                ) else {
                    return;
                };
                if pending.is_some()
                    || !revision.bytes().all(|b| b.is_ascii_digit())
                    || revision.is_empty()
                {
                    return; // korrid must wait for the ack; a second push breaks the wire
                }
                pending = Some(revision.to_owned());
                events(WatchEvent::Model {
                    revision: revision.to_owned(),
                    model: model.to_string(),
                });
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_length_and_chunked_replies_and_refuses_errors() {
        let plain = b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nhello".as_slice();
        assert_eq!(read_response(plain).unwrap(), b"hello");
        let chunked = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n3\r\nhel\r\n2\r\nlo\r\n0\r\n\r\n".as_slice();
        assert_eq!(read_response(chunked).unwrap(), b"hello");
        let denied = b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n".as_slice();
        assert!(
            matches!(read_response(denied), Err(KorridError::Unavailable(r)) if r == "HTTP 401")
        );
    }
}
