//! Bounded native i3/Sway IPC. A failed exchange discards its connection.
//!
//! Reads can reconnect on the next observation. Commands are never replayed:
//! losing their reply does not mean the compositor did not apply them.

use std::{
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixStream,
    },
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAGIC: &[u8; 6] = b"i3-ipc";
const MAX_PAYLOAD: usize = 4 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct SocketStamp {
    device: u64,
    inode: u64,
    owner: u32,
    group: u32,
    mode: u32,
    changed: (i64, i64),
}

impl SocketStamp {
    fn read(path: &std::path::Path) -> io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        if !metadata.file_type().is_socket() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Sway path is not a socket",
            ));
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            owner: metadata.uid(),
            group: metadata.gid(),
            mode: metadata.mode(),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}

#[derive(Debug)]
pub(super) struct SwayIpc {
    socket: PathBuf,
    timeout: Duration,
    stream: Mutex<Option<(SocketStamp, UnixStream)>>,
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
        // An established Unix connection survives chmod and ACL revocation.
        // Reconnect when socket authority changes so the kernel checks the
        // current permissions, just as it did for each former helper process.
        let stamp = SocketStamp::read(&self.socket)?;
        if connection
            .as_ref()
            .is_some_and(|(previous, _)| previous != &stamp)
        {
            *connection = None;
        }
        if connection.is_none() {
            let stream = self.connect(deadline)?;
            if SocketStamp::read(&self.socket)? != stamp {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Sway socket changed during connection",
                ));
            }
            *connection = Some((stamp, stream));
        }
        let result = exchange(
            &mut connection.as_mut().expect("connected").1,
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
    fn changed_socket_permissions_retire_an_existing_connection() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sway.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            reply(&mut first, 4, b"{}");
            // Keep the first connection alive; permission changes must still
            // cause a new connection, not reuse its earlier authorization.
            let (mut second, _) = listener.accept().unwrap();
            reply(&mut second, 4, b"{\"new\":true}");
        });
        let control = SwayIpc::new(path.clone(), Duration::from_secs(1)).unwrap();
        assert_eq!(control.request(4, b"").unwrap(), "{}");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
        assert_eq!(control.request(4, b"").unwrap(), "{\"new\":true}");
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
    fn a_silent_compositor_cannot_block_forever() {
        let (mut client, _server) = UnixStream::pair().unwrap();
        let started = Instant::now();
        assert!(exchange(&mut client, started + Duration::from_millis(30), 4, b"").is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
