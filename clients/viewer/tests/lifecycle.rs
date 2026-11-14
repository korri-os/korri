// SPDX-License-Identifier: GPL-3.0-or-later
use korri_viewer::lifecycle::Lifecycle;
use korri_viewer_treaty::{
    ViewerCommand as C, ViewerDisplay, ViewerEvent as E, ViewerMessage as M, ViewerReply as R,
    ViewerRequest, ViewerStatus,
};
use korri_viewer_treaty::{read_message, write_message};
use std::{
    io::BufReader,
    net::TcpListener,
    os::unix::net::UnixStream,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

mod support;

fn request(socket: &mut BufReader<UnixStream>, launch: &str, command: C) -> R {
    write_message(
        socket.get_mut(),
        &ViewerRequest {
            launch_id: launch.into(),
            command,
        },
    )
    .unwrap();
    loop {
        match read_message(socket).unwrap().unwrap() {
            M::Reply(reply) => return reply,
            M::Event(_) => (),
        }
    }
}

#[test]
fn exact_launch_connected_events_and_confirmed_stop_over_real_socket() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let lifecycle = Lifecycle::bind(&path, "launch-a", unsafe { libc::geteuid() }).unwrap();
    let mut socket = BufReader::new(UnixStream::connect(&path).unwrap());
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert!(matches!(
        request(&mut socket, "launch-b", C::Status),
        R::Error { .. }
    ));
    let runtime = lifecycle.clone();
    let running = std::thread::spawn(move || {
        let start = runtime.wait_start().unwrap().expect("Start came first");
        assert_eq!(start.address, "127.0.0.1");
        runtime
            .connected(ViewerDisplay {
                width: 1280,
                height: 720,
            })
            .unwrap();
        while !runtime.stop_requested() {
            std::thread::sleep(Duration::from_millis(1));
        }
        runtime.stopped();
    });
    assert!(matches!(
        request(
            &mut socket,
            "launch-a",
            C::Start {
                address: "127.0.0.1".into(),
                display: Some(ViewerDisplay {
                    width: 1920,
                    height: 1080
                })
            }
        ),
        R::Ok
    ));
    assert!(
        matches!(request(&mut socket, "launch-a", C::Snapshot), R::Snapshot { snapshot: Some(s) } if s.connected == Some(true) && s.shape.as_ref().unwrap().encoded.width == 1280)
    );
    assert!(matches!(
        request(&mut socket, "launch-a", C::Background),
        R::Ok
    ));
    assert!(matches!(
        request(&mut socket, "launch-a", C::Status),
        R::Status(ViewerStatus::Backgrounded)
    ));
    assert!(matches!(
        request(&mut socket, "launch-a", C::ResumeAndFocus),
        R::Ok
    ));
    for command in [
        C::Resize {
            connection: "old".into(),
            display: ViewerDisplay {
                width: 640,
                height: 480,
            },
        },
        C::Apply {
            control: "unknown".into(),
            value: None,
        },
        C::Notice {
            message: "notice".into(),
        },
    ] {
        assert!(matches!(
            request(&mut socket, "launch-a", command),
            R::Error { .. }
        ));
    }
    write_message(
        socket.get_mut(),
        &ViewerRequest {
            launch_id: "launch-a".into(),
            command: C::Stop,
        },
    )
    .unwrap();
    let mut stopped = false;
    loop {
        match read_message(&mut socket).unwrap().unwrap() {
            M::Event(E::Status(ViewerStatus::Stopped)) => stopped = true,
            M::Reply(R::Ok) => break,
            _ => (),
        }
    }
    assert!(
        stopped,
        "Stop must not acknowledge before the runtime finishes"
    );
    running.join().unwrap();
}

#[test]
fn existing_path_is_not_deleted() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    std::fs::create_dir(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "keep").unwrap();
    assert!(Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "keep");
}

#[test]
fn socket_name_too_long_for_its_pinned_address_fails_closed() {
    let dir = support::scratch();
    // The address is /proc/self/fd/<descriptor>/<name> in 108 bytes.
    let path = dir.path().join("control").join("s".repeat(100));
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    assert!(Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).is_err());
    assert!(
        !path.parent().unwrap().exists(),
        "a refused bind must remove the directory it created"
    );
}

#[test]
fn startup_failure_returns_typed_error_and_stopped_event() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let lifecycle = Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).unwrap();
    let mut socket = BufReader::new(UnixStream::connect(&path).unwrap());
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let runtime = lifecycle.clone();
    let failed = std::thread::spawn(move || {
        runtime.wait_start().unwrap().expect("Start came first");
        runtime.failed("certificate rejected".into());
    });
    assert!(
        matches!(request(&mut socket, "a", C::Start { address: "127.0.0.1".into(), display: None }), R::Error { message } if message == "certificate rejected")
    );
    failed.join().unwrap();
}

#[test]
fn real_binary_reports_missing_pin_over_socket_and_exits() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .args([
            "--lifecycle-socket",
            path.to_str().unwrap(),
            "--launch-id",
            "a",
            "--controller-uid",
            &unsafe { libc::geteuid() }.to_string(),
            "stream",
            "-keydir",
            dir.path().to_str().unwrap(),
            "127.0.0.1",
        ])
        .env_remove("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(2);
    let mut socket = loop {
        match UnixStream::connect(&path) {
            Ok(socket) => break BufReader::new(socket),
            Err(_) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(5))
            }
            error => panic!("viewer socket did not start: {error:?}"),
        }
    };
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert!(
        matches!(request(&mut socket, "a", C::Start { address: "127.0.0.1:47989".into(), display: None }), R::Error { message } if message.contains("KORRI_MOONLIGHT_SERVER_CERTIFICATE is required"))
    );
    assert!(!child.wait().unwrap().success());
    assert!(
        !path.exists(),
        "the exited process must remove its own socket"
    );
}

#[test]
fn start_address_preserves_port_and_ipv6_and_rejects_url_injection() {
    use korri_viewer::{lifecycle::Start, options::Options};
    let mut options =
        Options::parse(["stream", "-keydir", "/tmp/keys", "localhost"].map(str::to_owned)).unwrap();
    for (address, host, port) in [
        ("127.0.0.1:48000", "127.0.0.1", 48000),
        ("[::1]:48001", "::1", 48001),
        ("localhost", "localhost", 47989),
        ("localhost:80", "localhost", 80),
    ] {
        Start {
            address: address.into(),
            display: None,
        }
        .configure(&mut options)
        .unwrap();
        assert_eq!((&*options.host, options.port), (host, port));
    }
    for address in [
        "name@localhost",
        "localhost/path",
        "localhost?x=1",
        "localhost#x",
        "localhost:0",
    ] {
        assert!(
            Start {
                address: address.into(),
                display: None
            }
            .configure(&mut options)
            .is_err()
        );
    }
}

#[test]
fn shutdown_closes_a_partial_reader_and_removes_the_owned_socket() {
    use std::io::{Read, Write};
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let lifecycle = Lifecycle::bind(&path, "a", unsafe { libc::geteuid() }).unwrap();
    let mut socket = BufReader::new(UnixStream::connect(&path).unwrap());
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    // A reply proves the server accepted this controller before shutdown.
    assert!(matches!(
        request(&mut socket, "a", C::Status),
        R::Error { .. }
    ));
    socket.get_mut().write_all(b"{\"launchId\":").unwrap();
    lifecycle.failed("startup deadline".into());
    lifecycle.drain();
    assert!(
        !path.exists(),
        "shutdown must unlink its socket even while the runtime handle exists"
    );
    let mut remaining = Vec::new();
    socket
        .read_to_end(&mut remaining)
        .expect("shutdown must release the blocked socket reader");
}

#[test]
fn real_binary_removes_socket_after_idle_controller_startup_timeout() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .args([
            "--lifecycle-socket",
            path.to_str().unwrap(),
            "--launch-id",
            "a",
            "--controller-uid",
            &unsafe { libc::geteuid() }.to_string(),
            "stream",
            "-keydir",
            dir.path().to_str().unwrap(),
            "127.0.0.1",
        ])
        .env_remove("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(2);
    let _idle_controller = loop {
        match UnixStream::connect(&path) {
            Ok(socket) => break socket,
            Err(_) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(5))
            }
            error => {
                let _ = child.kill();
                panic!("viewer socket did not start: {error:?}");
            }
        }
    };
    let until = std::time::Instant::now() + Duration::from_secs(35);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= until {
            child.kill().unwrap();
            panic!("viewer did not honor its startup deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(!status.success());
    assert!(
        !path.exists(),
        "the idle controller must not keep the exited viewer's socket path alive"
    );
}

/// The real process, as a launcher starts it. Nothing in it can connect a
/// stream without a certificate pin, so it waits for Start or fails after it.
fn lifecycle_viewer(path: &Path, keys: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_korri-viewer"));
    command
        .args([
            "--lifecycle-socket",
            path.to_str().unwrap(),
            "--launch-id",
            "a",
            "--controller-uid",
            &unsafe { libc::geteuid() }.to_string(),
            "stream",
            "-keydir",
            keys.to_str().unwrap(),
            "127.0.0.1",
        ])
        .env_remove("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .stderr(Stdio::null());
    command
}

/// A controller that the viewer has accepted: its first request has a reply.
fn accepted_controller(path: &Path, child: &mut Child) -> BufReader<UnixStream> {
    let until = Instant::now() + Duration::from_secs(2);
    let socket = loop {
        match UnixStream::connect(path) {
            Ok(socket) => break socket,
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
            error => {
                let _ = child.kill();
                panic!("viewer socket did not start: {error:?}");
            }
        }
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut socket = BufReader::new(socket);
    assert!(matches!(
        request(&mut socket, "a", C::Status),
        R::Error { .. }
    ));
    socket
}

fn signal(child: &Child, signal: libc::c_int) {
    assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, signal) }, 0);
}

fn exit_within(child: &mut Child, limit: Duration) -> ExitStatus {
    let until = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= until {
            child.kill().unwrap();
            panic!("viewer did not exit within {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Every message until the viewer closes the controller's connection.
fn remaining_messages(socket: &mut BufReader<UnixStream>) -> Vec<M> {
    std::iter::from_fn(|| read_message(socket).unwrap()).collect()
}

// #468: a signal before Start used to kill the process with its default
// action and leave the socket and its private directory on disk.
#[test]
fn real_binary_stops_and_removes_socket_and_directory_on_a_signal_before_start() {
    for number in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP, libc::SIGQUIT] {
        let dir = support::scratch();
        let path = dir.path().join("control/viewer.sock");
        if support::refused_below_an_unmapped_root(&path) {
            return;
        }
        let mut child = lifecycle_viewer(&path, dir.path()).spawn().unwrap();
        let mut controller = accepted_controller(&path, &mut child);

        signal(&child, number);

        let status = exit_within(&mut child, Duration::from_secs(5));
        assert!(
            status.success(),
            "signal {number}: a stop before Start is not a failure: {status:?}"
        );
        assert!(
            remaining_messages(&mut controller)
                .iter()
                .any(|m| matches!(m, M::Event(E::Status(ViewerStatus::Stopped)))),
            "signal {number}: the controller must see Stopped"
        );
        assert!(!path.exists(), "signal {number}: socket left behind");
        assert!(
            !path.parent().unwrap().exists(),
            "signal {number}: private directory left behind"
        );
    }
}

// The main thread is busy with Start's first request and cannot clean up
// until that returns. The signal must not wait for it.
#[test]
fn real_binary_removes_socket_at_once_on_a_signal_while_connecting_after_start() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
    let keys = dir.path().join("keys");
    korri_viewer::session::Identity::load_or_generate(&keys).unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let port = server.local_addr().unwrap().port();
    let mut child = lifecycle_viewer(&path, &keys)
        .env(
            "KORRI_MOONLIGHT_SERVER_CERTIFICATE",
            keys.join("client.pem"),
        )
        .spawn()
        .unwrap();
    let mut controller = accepted_controller(&path, &mut child);
    write_message(
        controller.get_mut(),
        &ViewerRequest {
            launch_id: "a".into(),
            command: C::Start {
                address: format!("127.0.0.1:{port}"),
                display: None,
            },
        },
    )
    .unwrap();
    // The viewer's first request after Start. Hold it without an answer.
    let until = Instant::now() + Duration::from_secs(5);
    let held = loop {
        match server.accept() {
            Ok((socket, _)) => break socket,
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
            error => {
                let _ = child.kill();
                panic!("viewer did not connect after Start: {error:?}");
            }
        }
    };

    signal(&child, libc::SIGTERM);

    let until = Instant::now() + Duration::from_secs(2);
    while path.parent().unwrap().exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !path.parent().unwrap().exists(),
        "the socket and its directory must go at once"
    );
    assert!(
        child.try_wait().unwrap().is_none(),
        "removal must not wait for the main thread"
    );
    drop(held);
    let messages = remaining_messages(&mut controller);
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, M::Reply(R::Error { .. }))),
        "Start must still get its reply: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, M::Event(E::Status(ViewerStatus::Stopped)))),
        "{messages:?}"
    );
    exit_within(&mut child, Duration::from_secs(10));
    assert!(!path.parent().unwrap().exists());
}

// The signal may arrive before, during or after the socket's creation. None
// of those moments may leave the socket or its directory behind.
#[test]
fn a_signal_at_any_moment_of_startup_leaves_no_socket_or_directory() {
    let dir = support::scratch();
    if support::refused_below_an_unmapped_root(&dir.path().join("control/viewer.sock")) {
        return;
    }
    // Step the delay until several signals arrive after the socket exists.
    // Earlier ones end the process before it makes anything.
    let mut handled = 0;
    let mut attempt = 0u64;
    while handled < 5 {
        assert!(
            attempt < 500,
            "no signal reached the viewer after its socket existed"
        );
        let path = dir.path().join(format!("control-{attempt}/viewer.sock"));
        let mut child = lifecycle_viewer(&path, dir.path()).spawn().unwrap();
        std::thread::sleep(Duration::from_micros(attempt * 200));
        signal(&child, libc::SIGTERM);
        let status = exit_within(&mut child, Duration::from_secs(5));
        handled += usize::from(status.success());
        assert!(
            !path.parent().unwrap().exists(),
            "attempt {attempt} ({status:?}) left {} behind",
            path.parent().unwrap().display()
        );
        attempt += 1;
    }
}

#[test]
fn lifecycle_socket_requires_an_explicit_controller_uid() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .args([
            "--lifecycle-socket",
            path.to_str().unwrap(),
            "--launch-id",
            "a",
            "stream",
            "-keydir",
            dir.path().to_str().unwrap(),
            "127.0.0.1",
        ])
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= until {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("viewer accepted a lifecycle socket with no controller UID");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--controller-uid"));
    assert!(!path.exists());
}
