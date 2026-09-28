#![cfg(target_os = "linux")]

use korri_inputd::input_seat::{
    GamepadState, SeatFailure, SeatReply, SeatRequest, COORDINATION_VERSION, MAX_COORDINATION_BYTES,
};
use serde_json::Value;
use std::{
    fs,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt, PermissionsExt},
            net::UnixDatagram,
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
        let child = spawn_receiver(root.path(), expected_uid, None);
        wait_for(root.path().join("control.sock").as_path());
        Self { child, root }
    }
    fn path(&self, name: &str) -> std::path::PathBuf {
        self.root.path().join(name)
    }
}

fn spawn_receiver(root: &Path, expected_uid: u32, notify_socket: Option<&Path>) -> Child {
    spawn_receiver_with_failure(root, expected_uid, notify_socket, None)
}
fn spawn_receiver_with_failure(
    root: &Path,
    expected_uid: u32,
    notify_socket: Option<&Path>,
    fail_slot: Option<u8>,
) -> Child {
    let gid = unsafe { libc::getgid() };
    let mut command = Command::new(env!("CARGO_BIN_EXE_korri-input-seat-receiver"));
    command
        .args([
            "--runtime-dir",
            root.to_str().unwrap(),
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
        .stderr(Stdio::piped());
    if let Some(slot) = fail_slot {
        command.args(["--dry-run-fail-create-slot", &slot.to_string()]);
    }
    // Do not inherit systemd's socket from the test runner.
    command.env_remove("NOTIFY_SOCKET");
    if let Some(socket) = notify_socket {
        command.env("NOTIFY_SOCKET", socket);
    }
    command.spawn().unwrap()
}
impl Drop for Receiver {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn ready_notification_follows_boot_and_listening_control_socket() {
    let root = tempfile::tempdir().unwrap();
    let notify_path = root.path().join("notify.sock");
    let notify = UnixDatagram::bind(&notify_path).unwrap();
    notify
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    // Dry-run executes the same default pool boot path without /dev/uinput.
    let mut child = spawn_receiver(root.path(), unsafe { libc::getuid() }, Some(&notify_path));
    let mut message = [0u8; 128];
    let count = notify.recv(&mut message).unwrap();
    assert_eq!(&message[..count], b"READY=1\nSTATUS=Ready");
    let control_path = root.path().join("control.sock");
    assert!(fs::symlink_metadata(&control_path)
        .unwrap()
        .file_type()
        .is_socket());
    let _control = connect(&control_path);
    assert!(child.try_wait().unwrap().is_none());
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn failed_socket_initialization_never_notifies_ready() {
    let root = tempfile::tempdir().unwrap();
    let notify_path = root.path().join("notify.sock");
    let notify = UnixDatagram::bind(&notify_path).unwrap();
    notify
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    // Boot completes, but an unsafe control path must fail before READY=1.
    fs::write(root.path().join("control.sock"), b"not a socket").unwrap();
    let mut child = spawn_receiver(root.path(), unsafe { libc::getuid() }, Some(&notify_path));
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "receiver did not exit on initialization failure"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert!(!status.success());
    assert!(
        notify.recv(&mut [0u8; 128]).is_err(),
        "failed startup sent a notification"
    );
}

#[test]
fn exact_start_mirror_and_stop_lifecycle() {
    let mut receiver = Receiver::start(unsafe { libc::getuid() });
    let coordinator = initialize(&receiver, 4, LAUNCH);
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

    success(
        &coordinator,
        SeatRequest::EndSession {
            launch_id: LAUNCH.into(),
        },
    );
    success(
        &coordinator,
        SeatRequest::BeginSession {
            launch_id: OTHER.into(),
        },
    );
    let next = connect(&receiver.path("control.sock"));
    send(next.as_raw_fd(), &request(1, OTHER));
    assert_eq!(receive(next.as_raw_fd(), 3), [1, 0, 0]);
    let next_sidecar: Value = serde_json::from_slice(&fs::read(&sidecar_path).unwrap()).unwrap();
    assert_eq!(next_sidecar["launchId"], OTHER);
    assert_eq!(next_sidecar["generation"], 2);
    assert_ne!(next_sidecar["mirrorToken"], token);
    send(next.as_raw_fd(), &request(2, OTHER));
    assert_eq!(receive(next.as_raw_fd(), 3), [1, 0, 0]);
}

#[test]
fn idle_mirror_connection_cannot_block_exact_stop() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let _coordinator = initialize(&receiver, 4, LAUNCH);
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
    let _coordinator = initialize(&receiver, 4, LAUNCH);
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

fn coordinate(fd: &OwnedFd, request: SeatRequest) -> SeatReply {
    let mut packet = vec![COORDINATION_VERSION];
    packet.extend(serde_json::to_vec(&request).unwrap());
    send(fd.as_raw_fd(), &packet);
    let response = receive(fd.as_raw_fd(), MAX_COORDINATION_BYTES + 1);
    assert_eq!(response.first(), Some(&COORDINATION_VERSION));
    serde_json::from_slice(&response[1..]).unwrap()
}
fn success(fd: &OwnedFd, request: SeatRequest) -> SeatReply {
    let reply = coordinate(fd, request);
    assert_eq!(reply.failure, None);
    reply
}
fn hello(receiver: &Receiver) -> OwnedFd {
    let fd = connect(&receiver.path("control.sock"));
    success(&fd, SeatRequest::Hello);
    fd
}
fn initialize(receiver: &Receiver, count: u8, launch: &str) -> OwnedFd {
    let fd = hello(receiver);
    assert!(!success(&fd, SeatRequest::ApplyCount { count }).recovery_required);
    success(
        &fd,
        SeatRequest::BeginSession {
            launch_id: launch.into(),
        },
    );
    fd
}
fn physical(fd: &OwnedFd, id: &str) -> SeatReply {
    success(
        fd,
        SeatRequest::PhysicalConnected {
            device_id: id.into(),
            name: "Native producer".into(),
            state: GamepadState::neutral(),
        },
    )
}
fn start_mirror(receiver: &Receiver) -> (OwnedFd, String) {
    let fd = connect(&receiver.path("control.sock"));
    send(fd.as_raw_fd(), &request(1, LAUNCH));
    assert_eq!(receive(fd.as_raw_fd(), 3), [1, 0, 0]);
    let sidecar: Value =
        serde_json::from_slice(&fs::read(receiver.path("sunshine-active-launch.json")).unwrap())
            .unwrap();
    (fd, sidecar["mirrorToken"].as_str().unwrap().to_owned())
}
fn remote_connect(receiver: &Receiver, token: &str, controller: u8) {
    let fd = connect(&receiver.path("sunshine-input-seat.sock"));
    let frame = format!(
        r#"{{"mirrorToken":"{token}","frame":{{"kind":"source-connected","launchId":"{LAUNCH}","controllerNumber":{controller}}}}}"#
    ) + "\n";
    send(fd.as_raw_fd(), frame.as_bytes());
    drop(fd);
    // Source-connected contains no baseline. Report a real full state before
    // expecting this source in native feedback (never fabricate neutral there).
    let fd = connect(&receiver.path("sunshine-input-seat.sock"));
    let frame = format!(
        r#"{{"mirrorToken":"{token}","frame":{{"kind":"source-state","launchId":"{LAUNCH}","controllerNumber":{controller},"buttons":0,"leftTrigger":0,"rightTrigger":0,"leftStickX":0,"leftStickY":0,"rightStickX":0,"rightStickY":0}}}}"#
    ) + "\n";
    send(fd.as_raw_fd(), frame.as_bytes());
}
fn remote_count(fd: &OwnedFd, count: usize) -> SeatReply {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let reply = success(fd, SeatRequest::Poll);
        if reply.remote_sources.len() == count {
            return reply;
        }
        assert!(Instant::now() < deadline, "remote feedback did not arrive");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn old_peer_cannot_launch_without_count_and_session_reconciliation() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let old = connect(&receiver.path("control.sock"));
    send(old.as_raw_fd(), &request(1, LAUNCH));
    assert_eq!(receive(old.as_raw_fd(), 3), [1, 1, 4]);
    let coordinator = hello(&receiver);
    assert!(success(&coordinator, SeatRequest::Poll).recovery_required);
    success(&coordinator, SeatRequest::ApplyCount { count: 6 });
    let early = connect(&receiver.path("control.sock"));
    send(early.as_raw_fd(), &request(1, LAUNCH));
    assert_eq!(receive(early.as_raw_fd(), 3), [1, 1, 4]);
    let duplicate = connect(&receiver.path("control.sock"));
    assert_eq!(
        coordinate(&duplicate, SeatRequest::Hello).failure,
        Some(SeatFailure::Active)
    );
    assert_eq!(success(&coordinator, SeatRequest::Poll).count, 6);
    assert!(!receiver.path("sunshine-active-launch.json").exists());
}
#[test]
fn process_mixed_sources_share_six_seats_and_overflow_still_has_feedback() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let coordinator = initialize(&receiver, 6, LAUNCH);
    assert_eq!(physical(&coordinator, "existing-native-a").slot, Some(1));
    let (_lease, token) = start_mirror(&receiver);
    remote_connect(&receiver, &token, 9);
    assert_eq!(
        remote_count(&coordinator, 1).remote_sources[0].slot,
        Some(2)
    );
    for (id, slot) in [("b", 3), ("c", 4), ("d", 5), ("e", 6)] {
        assert_eq!(physical(&coordinator, id).slot, Some(slot));
    }
    assert_eq!(physical(&coordinator, "overflow").slot, None);
    remote_connect(&receiver, &token, 3);
    let reply = remote_count(&coordinator, 2);
    assert_eq!(
        reply
            .remote_sources
            .iter()
            .find(|source| source.controller_number == 3)
            .unwrap()
            .slot,
        None
    );
    // Authentication failure cannot allocate or publish a remote source.
    remote_connect(&receiver, &"b".repeat(64), 7);
    thread::sleep(Duration::from_millis(30));
    assert_eq!(
        success(&coordinator, SeatRequest::Poll)
            .remote_sources
            .len(),
        2
    );
    assert_eq!(
        coordinate(&coordinator, SeatRequest::ApplyCount { count: 4 }).failure,
        Some(SeatFailure::Active)
    );
    success(&coordinator, SeatRequest::Route { launch_id: None });
    assert_eq!(
        coordinate(&coordinator, SeatRequest::ApplyCount { count: 4 }).failure,
        Some(SeatFailure::Active)
    );
}
#[test]
fn process_lease_end_is_not_session_end() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let coordinator = initialize(&receiver, 1, LAUNCH);
    let (lease, token) = start_mirror(&receiver);
    remote_connect(&receiver, &token, 0);
    remote_count(&coordinator, 1);
    drop(lease);
    wait_absent(&receiver.path("sunshine-input-seat.sock"));
    let reply = remote_count(&coordinator, 0);
    assert_eq!(reply.session.as_deref(), Some(LAUNCH));
    assert_eq!(physical(&coordinator, "replacement").slot, None);
    success(
        &coordinator,
        SeatRequest::EndSession {
            launch_id: LAUNCH.into(),
        },
    );
    assert_eq!(physical(&coordinator, "replacement").slot, Some(1));
    assert_eq!(
        success(&coordinator, SeatRequest::ApplyCount { count: 6 }).count,
        6
    );
}
#[test]
fn process_coordinator_disconnect_requires_recovery_without_clearing_game_reservations() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let coordinator = initialize(&receiver, 1, LAUNCH);
    assert_eq!(physical(&coordinator, "physical").slot, Some(1));
    let (_lease, _) = start_mirror(&receiver);
    drop(coordinator);
    wait_absent(&receiver.path("sunshine-active-launch.json"));
    let recovery = hello(&receiver);
    let reply = success(&recovery, SeatRequest::Poll);
    assert!(reply.recovery_required);
    assert_eq!(reply.session.as_deref(), Some(LAUNCH));
    assert_eq!(
        coordinate(&recovery, SeatRequest::ApplyCount { count: 6 }).failure,
        Some(SeatFailure::Active)
    );
    success(
        &recovery,
        SeatRequest::BeginSession {
            launch_id: LAUNCH.into(),
        },
    );
    assert_eq!(physical(&recovery, "replacement").slot, None);
    assert_eq!(physical(&recovery, "physical").slot, Some(1));
}
#[test]
fn process_coordinator_heartbeat_timeout_revokes_only_input_authority() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let coordinator = initialize(&receiver, 1, LAUNCH);
    physical(&coordinator, "held-source");
    let (_lease, _) = start_mirror(&receiver);
    // Leave the authenticated connection open, but stop servicing it.
    wait_absent(&receiver.path("sunshine-active-launch.json"));
    let recovered = hello(&receiver);
    let reply = success(&recovered, SeatRequest::Poll);
    assert!(reply.recovery_required);
    assert_eq!(reply.session.as_deref(), Some(LAUNCH));
    drop(coordinator);
}
#[test]
fn process_source_loss_expires_without_ending_authoritative_session() {
    let receiver = Receiver::start(unsafe { libc::getuid() });
    let coordinator = initialize(&receiver, 1, LAUNCH);
    let (_lease, token) = start_mirror(&receiver);
    remote_connect(&receiver, &token, 0);
    remote_count(&coordinator, 1);
    let reply = remote_count(&coordinator, 0); // polls also keep the coordinator alive
    assert_eq!(reply.session.as_deref(), Some(LAUNCH));
    assert_eq!(physical(&coordinator, "replacement").slot, None);
    success(
        &coordinator,
        SeatRequest::EndSession {
            launch_id: LAUNCH.into(),
        },
    );
    assert_eq!(physical(&coordinator, "replacement").slot, Some(1));
}
#[test]
fn process_count_creation_failure_is_negative_ack_then_fatal_not_launchable() {
    let root = tempfile::tempdir().unwrap();
    let child = spawn_receiver_with_failure(root.path(), unsafe { libc::getuid() }, None, Some(6));
    wait_for(&root.path().join("control.sock"));
    let mut receiver = Receiver { child, root };
    let coordinator = hello(&receiver);
    let reply = coordinate(&coordinator, SeatRequest::ApplyCount { count: 6 });
    assert_eq!(reply.failure, Some(SeatFailure::Backend));
    assert!(reply.recovery_required);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = receiver.child.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!receiver.path("sunshine-active-launch.json").exists());
    assert!(!receiver.path("control.sock").exists());
}
#[test]
fn process_restart_reports_unreconciled_pool_and_requires_current_authority() {
    let mut receiver = Receiver::start(unsafe { libc::getuid() });
    let coordinator = initialize(&receiver, 6, LAUNCH);
    physical(&coordinator, "physical");
    let (_lease, _) = start_mirror(&receiver);
    unsafe {
        libc::kill(receiver.child.id() as i32, libc::SIGTERM);
    }
    assert!(receiver.child.wait().unwrap().success());
    drop(coordinator);
    receiver.child = spawn_receiver(receiver.root.path(), unsafe { libc::getuid() }, None);
    wait_for(&receiver.path("control.sock"));
    let coordinator = hello(&receiver);
    let reply = success(&coordinator, SeatRequest::Poll);
    assert!(reply.recovery_required);
    assert_eq!(reply.count, 4);
    assert_eq!(reply.session, None);
    assert_eq!(
        coordinate(
            &coordinator,
            SeatRequest::BeginSession {
                launch_id: LAUNCH.into()
            }
        )
        .failure,
        Some(SeatFailure::NotReady)
    );
    // The host must resolve the old game before reconciling a new pool; the
    // receiver cannot reconstruct reservations destroyed by process death.
    success(&coordinator, SeatRequest::ApplyCount { count: 6 });
    success(
        &coordinator,
        SeatRequest::BeginSession {
            launch_id: OTHER.into(),
        },
    );
    assert_eq!(physical(&coordinator, "physical").slot, Some(1));
}
#[test]
fn malformed_and_oversized_coordinator_frames_revoke_mirror_and_require_recovery() {
    for packet in [
        vec![COORDINATION_VERSION, b'{'],
        vec![COORDINATION_VERSION; MAX_COORDINATION_BYTES + 1],
    ] {
        let receiver = Receiver::start(unsafe { libc::getuid() });
        let coordinator = initialize(&receiver, 4, LAUNCH);
        let (_lease, _) = start_mirror(&receiver);
        send(coordinator.as_raw_fd(), &packet);
        wait_absent(&receiver.path("sunshine-active-launch.json"));
        let recovered = hello(&receiver);
        assert!(success(&recovered, SeatRequest::Poll).recovery_required);
    }
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
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    assert!(
        unsafe { libc::poll(&mut descriptor, 1, 3000) } > 0,
        "receiver reply timed out"
    );
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
