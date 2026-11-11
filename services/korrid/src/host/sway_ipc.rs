//! Bounded native i3/Sway IPC. A failed exchange discards its connection.
//!
//! Reads can reconnect on the next observation. Commands are never replayed:
//! losing their reply does not mean the compositor did not apply them.

use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAGIC: &[u8; 6] = b"i3-ipc";
const MAX_PAYLOAD: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub(super) struct SwayIpc {
    socket: PathBuf,
    timeout: Duration,
    stream: Mutex<Option<UnixStream>>,
}

impl SwayIpc {
    pub(super) fn new(socket: PathBuf, timeout: Duration) -> io::Result<Self> {
        if !socket.is_absolute() || timeout.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute Sway socket and positive timeout required",
            ));
        }
        Ok(Self {
            socket,
            timeout,
            stream: Mutex::new(None),
        })
    }

    /// Subscribe before reading the initial snapshot, so changes during that
    /// read remain queued. Events never share the command connection.
    pub(super) fn subscribe(&self) -> io::Result<SwaySubscription> {
        let deadline = Instant::now() + self.timeout;
        let mut stream = self.connect(deadline)?;
        let response = exchange(
            &mut stream,
            deadline,
            2,
            br#"["window","workspace","output","shutdown"]"#,
        )?;
        #[derive(serde::Deserialize)]
        struct Accepted {
            success: bool,
        }
        let accepted: Accepted = serde_json::from_str(&response)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if !accepted.success {
            return Err(io::Error::other("Sway refused the subscription"));
        }
        stream.set_read_timeout(None)?;
        Ok(SwaySubscription {
            stream,
            timeout: self.timeout,
        })
    }

    fn connect(&self, deadline: Instant) -> io::Result<UnixStream> {
        let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
        socket.connect_timeout(
            &socket2::SockAddr::unix(&self.socket)?,
            remaining(deadline)?,
        )?;
        Ok(socket.into())
    }

    pub(super) fn request(&self, kind: u32, payload: &[u8]) -> io::Result<String> {
        if payload.len() > MAX_PAYLOAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Sway request exceeds limit",
            ));
        }
        let mut connection = self
            .stream
            .lock()
            .map_err(|_| io::Error::other("Sway connection lock poisoned"))?;
        let deadline = Instant::now() + self.timeout;
        if connection.is_none() {
            *connection = Some(self.connect(deadline)?);
        }
        let result = exchange(
            connection.as_mut().expect("connected"),
            deadline,
            kind,
            payload,
        );
        if result.is_err() {
            *connection = None;
        }
        result
    }
}

/// Blocks without periodic wakeups while Sway has no events. The owner shuts
/// down a cloned socket when it stops the worker. EOF invalidates cached facts.
#[derive(Debug)]
pub(super) struct SwaySubscription {
    stream: UnixStream,
    timeout: Duration,
}

impl SwaySubscription {
    pub(super) fn cancellation_socket(&self) -> io::Result<UnixStream> {
        self.stream.try_clone()
    }

    pub(super) fn next_event(&mut self) -> io::Result<()> {
        let mut header = [0; 14];
        self.stream.set_read_timeout(None)?;
        self.stream.read_exact(&mut header[..1])?;
        let deadline = Instant::now() + self.timeout;
        read_exact(&mut self.stream, &mut header[1..], deadline)?;
        let length = u32::from_le_bytes(header[6..10].try_into().expect("length field")) as usize;
        let kind = u32::from_le_bytes(header[10..14].try_into().expect("type field"));
        if &header[..6] != MAGIC || length > MAX_PAYLOAD || kind & (1 << 31) == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid Sway event frame",
            ));
        }
        let mut payload = vec![0; length];
        read_exact(&mut self.stream, &mut payload, deadline)?;
        serde_json::from_slice::<serde_json::Value>(&payload)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(())
    }
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Sway IPC deadline exceeded"))
}

fn read_exact(stream: &mut UnixStream, mut output: &mut [u8], deadline: Instant) -> io::Result<()> {
    while !output.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(output) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Sway IPC closed",
                ))
            }
            Ok(count) => output = &mut output[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn exchange(
    stream: &mut UnixStream,
    deadline: Instant,
    kind: u32,
    payload: &[u8],
) -> io::Result<String> {
    let mut request = Vec::with_capacity(14 + payload.len());
    request.extend_from_slice(MAGIC);
    request.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    request.extend_from_slice(&kind.to_le_bytes());
    request.extend_from_slice(payload);
    let mut rest = request.as_slice();
    while !rest.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        match stream.write(rest) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Sway IPC write stopped",
                ))
            }
            Ok(count) => rest = &rest[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) => return Err(error),
        }
    }
    let mut header = [0; 14];
    read_exact(stream, &mut header, deadline)?;
    let length = u32::from_le_bytes(header[6..10].try_into().expect("length field")) as usize;
    let reply_kind = u32::from_le_bytes(header[10..14].try_into().expect("type field"));
    if &header[..6] != MAGIC || reply_kind != kind || length > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Sway IPC reply header",
        ));
    }
    let mut reply = vec![0; length];
    read_exact(stream, &mut reply, deadline)?;
    String::from_utf8(reply).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::net::UnixListener, thread};

    fn reply(stream: &mut UnixStream, kind: u32, bytes: &[u8]) {
        let mut request = [0; 14];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request[..6], MAGIC);
        assert_eq!(u32::from_le_bytes(request[10..].try_into().unwrap()), kind);
        let mut body = vec![0; u32::from_le_bytes(request[6..10].try_into().unwrap()) as usize];
        stream.read_exact(&mut body).unwrap();
        stream.write_all(MAGIC).unwrap();
        stream
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .unwrap();
        stream.write_all(&kind.to_le_bytes()).unwrap();
        stream.write_all(bytes).unwrap();
    }

    #[test]
    fn one_connection_serves_multiple_observations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sway.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            reply(&mut stream, 4, b"{\"nodes\":[]}");
            reply(&mut stream, 3, b"[]");
        });
        let control = SwayIpc::new(path, Duration::from_secs(1)).unwrap();
        assert_eq!(control.request(4, b"").unwrap(), "{\"nodes\":[]}");
        assert_eq!(control.request(3, b"").unwrap(), "[]");
        server.join().unwrap();
    }

    #[test]
    fn next_observation_reconnects_after_a_broken_reply() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sway.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let mut request = [0; 14];
            first.read_exact(&mut request).unwrap();
            drop(first);
            let (mut second, _) = listener.accept().unwrap();
            reply(&mut second, 4, b"{}");
        });
        let control = SwayIpc::new(path, Duration::from_secs(1)).unwrap();
        assert!(control.request(4, b"").is_err());
        assert_eq!(control.request(4, b"").unwrap(), "{}");
        server.join().unwrap();
    }

    #[test]
    fn oversized_and_mismatched_replies_are_rejected() {
        for (length, kind) in [(MAX_PAYLOAD as u32 + 1, 4u32), (0, 3u32)] {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            let task = thread::spawn(move || {
                let mut request = [0; 14];
                server.read_exact(&mut request).unwrap();
                server.write_all(MAGIC).unwrap();
                server.write_all(&length.to_le_bytes()).unwrap();
                server.write_all(&kind.to_le_bytes()).unwrap();
            });
            let error =
                exchange(&mut client, Instant::now() + Duration::from_secs(1), 4, b"").unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            task.join().unwrap();
        }
    }

    #[test]
    fn a_command_with_a_lost_reply_is_not_replayed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sway.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut header = [0; 14];
            stream.read_exact(&mut header).unwrap();
            let mut command =
                vec![0; u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize];
            stream.read_exact(&mut command).unwrap();
            assert_eq!(command, b"[con_id=1] focus");
            drop(stream);
            listener
        });
        let control = SwayIpc::new(path, Duration::from_secs(1)).unwrap();
        assert!(control.request(0, b"[con_id=1] focus").is_err());
        let listener = server.join().unwrap();
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn fragmented_frames_are_read_completely() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let task = thread::spawn(move || {
            let mut request = [0; 14];
            server.read_exact(&mut request).unwrap();
            let mut frame = MAGIC.to_vec();
            frame.extend_from_slice(&2u32.to_le_bytes());
            frame.extend_from_slice(&4u32.to_le_bytes());
            frame.extend_from_slice(b"{}");
            for byte in frame {
                server.write_all(&[byte]).unwrap();
            }
        });
        assert_eq!(
            exchange(&mut client, Instant::now() + Duration::from_secs(1), 4, b"").unwrap(),
            "{}"
        );
        task.join().unwrap();
    }

    #[test]
    fn subscription_receives_changes_and_shutdown_cancels_an_idle_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sway.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (release, hold) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            reply(&mut stream, 2, br#"{"success":true}"#);
            stream.write_all(MAGIC).unwrap();
            stream.write_all(&2u32.to_le_bytes()).unwrap();
            stream.write_all(&((1u32 << 31) | 3).to_le_bytes()).unwrap();
            stream.write_all(b"{}").unwrap();
            hold.recv().unwrap();
        });
        let control = SwayIpc::new(path, Duration::from_secs(1)).unwrap();
        let mut subscription = control.subscribe().unwrap();
        subscription.next_event().unwrap();
        let cancel = subscription.cancellation_socket().unwrap();
        let reader = thread::spawn(move || subscription.next_event());
        cancel.shutdown(std::net::Shutdown::Both).unwrap();
        assert!(reader.join().unwrap().is_err());
        release.send(()).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn a_refused_subscription_is_not_an_observation_feed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sway.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            reply(&mut stream, 2, br#"{"success":false}"#);
        });
        let control = SwayIpc::new(path, Duration::from_secs(1)).unwrap();
        assert!(control.subscribe().is_err());
        server.join().unwrap();
    }

    #[test]
    fn a_partial_event_has_a_deadline() {
        let (client, mut server) = UnixStream::pair().unwrap();
        server.write_all(b"i").unwrap();
        let mut subscription = SwaySubscription {
            stream: client,
            timeout: Duration::from_millis(30),
        };
        let started = Instant::now();
        assert!(subscription.next_event().is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_silent_compositor_cannot_block_forever() {
        let (mut client, _server) = UnixStream::pair().unwrap();
        let started = Instant::now();
        assert!(exchange(&mut client, started + Duration::from_millis(30), 4, b"").is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
