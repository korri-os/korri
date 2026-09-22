#![cfg(target_os = "linux")]

use serde_json::Value;
use std::{
    fs,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, PermissionsExt},
        },
    },
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const LAUNCH: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "fedcba9876543210fedcba9876543210";

struct Receiver {
    child: Child,
    root: tempfile::TempDir,
}
impl Receiver {
    fn start(expected_uid: u32) -> Self {
        let root = tempfile::tempdir().unwrap();
        let gid = unsafe { libc::getgid() };
        let child = Command::new(env!("CARGO_BIN_EXE_korri-input-seat-receiver"))
            .args([
                "--runtime-dir",
                root.path().to_str().unwrap(),
                "--control-uid",
                &expected_uid.to_string(),
                "--control-gid",
                &gid.to_string(),
                "--sunshine-uid",
                &unsafe { libc::getuid() }.to_string(),
                "--sunshine-gid",
                &gid.to_string(),
                "--event-gid",
                &gid.to_string(),
                "--dry-run",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        wait_for(root.path().join("control.sock").as_path());
        Self { child, root }
    }
    fn path(&self, name: &str) -> std::path::PathBuf {
        self.root.path().join(name)
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn exact_start_mirror_and_stop_lifecycle() {
    let mut receiver = Receiver::start(unsafe { libc::getuid() });
    let control = connect(&receiver.path("control.sock"));
    send(control.as_raw_fd(), &request(1, LAUNCH));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 0, 0]);
    wait_for(&receiver.path("sunshine-active-launch.json"));
    wait_for(&receiver.path("sunshine-input-seat.sock"));
    let sidecar_path = receiver.path("sunshine-active-launch.json");
    let sidecar: Value = serde_json::from_slice(&fs::read(&sidecar_path).unwrap()).unwrap();
    let sidecar_metadata = fs::metadata(&sidecar_path).unwrap();
    assert_eq!(sidecar_metadata.permissions().mode() & 0o777, 0o640);
    assert_eq!(sidecar_metadata.gid(), unsafe { libc::getgid() });
    assert_eq!(
        fs::metadata(receiver.path("control.sock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o660
    );
    assert_eq!(
        fs::metadata(receiver.path("sunshine-input-seat.sock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o660
    );
    assert_eq!(sidecar.as_object().unwrap().len(), 3);
    assert_eq!(sidecar["launchId"], LAUNCH);
    assert_eq!(sidecar["generation"], 1);
    let token = sidecar["mirrorToken"].as_str().unwrap();
    assert_eq!(token.len(), 64);

    let mirror = connect(&receiver.path("sunshine-input-seat.sock"));
    let frame = format!(
        r#"{{"mirrorToken":"{token}","frame":{{"kind":"source-connected","launchId":"{LAUNCH}","controllerNumber":0}}}}"#
    ) + "\n";
    send(mirror.as_raw_fd(), frame.as_bytes());

    send(control.as_raw_fd(), &request(3, OTHER));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 1, 5]);
    assert!(receiver.path("sunshine-active-launch.json").exists());
    send(control.as_raw_fd(), &request(3, LAUNCH));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 0, 0]);

    let extra = connect(&receiver.path("control.sock"));
    send(extra.as_raw_fd(), &request(1, OTHER));
    assert_eq!(receive(extra.as_raw_fd(), 3), [1, 1, 3]);

    send(control.as_raw_fd(), &request(2, OTHER));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 1, 5]);
    assert!(receiver.path("sunshine-active-launch.json").exists());
    send(control.as_raw_fd(), &request(2, LAUNCH));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 0, 0]);
    wait_absent(&receiver.path("sunshine-active-launch.json"));
    wait_absent(&receiver.path("sunshine-input-seat.sock"));
    assert!(receiver.child.try_wait().unwrap().is_none());
}

#[test]
fn idle_mirror_connection_cannot_block_exact_stop() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let control = connect(&receiver.path("control.sock"));
    send(control.as_raw_fd(), &request(1, LAUNCH));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 0, 0]);
    let _idle_mirror = connect(&receiver.path("sunshine-input-seat.sock"));

    let started = Instant::now();
    send(control.as_raw_fd(), &request(2, LAUNCH));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 0, 0]);
    assert!(started.elapsed() < Duration::from_secs(1));
    wait_absent(&receiver.path("sunshine-active-launch.json"));
}

#[test]
fn lease_eof_removes_sidecar_and_mirror_socket() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let control = connect(&receiver.path("control.sock"));
    send(control.as_raw_fd(), &request(1, LAUNCH));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 0, 0]);
    wait_for(&receiver.path("sunshine-active-launch.json"));
    drop(control);
    wait_absent(&receiver.path("sunshine-active-launch.json"));
    wait_absent(&receiver.path("sunshine-input-seat.sock"));
}

#[test]
fn wrong_control_peer_is_rejected() {
    let receiver = Receiver::start(unsafe { libc::getuid() } + 1);
    let control = connect(&receiver.path("control.sock"));
    send(control.as_raw_fd(), &request(1, LAUNCH));
    assert_eq!(receive(control.as_raw_fd(), 3), [1, 1, 2]);
    assert!(!receiver.path("sunshine-active-launch.json").exists());
}

fn request(operation: u8, launch: &str) -> [u8; 34] {
    let mut value = [0u8; 34];
    value[0] = 1;
    value[1] = operation;
    value[2..].copy_from_slice(launch.as_bytes());
    value
}

fn connect(path: &Path) -> OwnedFd {
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
    assert!(fd >= 0);
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let bytes = path.as_os_str().as_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, source) in address.sun_path.iter_mut().zip(bytes.iter().copied()) {
        *target = source as libc::c_char;
    }
    let length = (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t;
    assert_eq!(
        unsafe {
            libc::connect(
                fd.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                length,
            )
        },
        0
    );
    fd
}

fn send(fd: RawFd, bytes: &[u8]) {
    assert_eq!(
        unsafe { libc::send(fd, bytes.as_ptr().cast(), bytes.len(), libc::MSG_NOSIGNAL) },
        bytes.len() as isize
    );
}

fn receive(fd: RawFd, length: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; length];
    let count = unsafe { libc::recv(fd, bytes.as_mut_ptr().cast(), bytes.len(), 0) };
    assert!(count >= 0);
    bytes.truncate(count as usize);
    bytes
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(path.exists(), "missing {}", path.display());
}
fn wait_absent(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!path.exists(), "still present {}", path.display());
}
