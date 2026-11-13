// SPDX-License-Identifier: GPL-3.0-or-later
use korri_viewer::lifecycle::Lifecycle;
use korri_viewer_treaty::{
    ViewerCommand as C, ViewerDisplay, ViewerEvent as E, ViewerMessage as M, ViewerReply as R,
    ViewerRequest, ViewerStatus,
};
use korri_viewer_treaty::{read_message, write_message};
use std::{io::BufReader, os::unix::net::UnixStream, time::Duration};

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
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
    let lifecycle = Lifecycle::bind(&path, "launch-a").unwrap();
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
        let start = runtime.wait_start().unwrap();
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
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
    std::fs::write(&path, "keep").unwrap();
    assert!(Lifecycle::bind(&path, "a").is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "keep");
}

#[test]
fn startup_failure_returns_typed_error_and_stopped_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
    let lifecycle = Lifecycle::bind(&path, "a").unwrap();
    let mut socket = BufReader::new(UnixStream::connect(&path).unwrap());
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let runtime = lifecycle.clone();
    let failed = std::thread::spawn(move || {
        runtime.wait_start().unwrap();
        runtime.failed("certificate rejected".into());
    });
    assert!(
        matches!(request(&mut socket, "a", C::Start { address: "127.0.0.1".into(), display: None }), R::Error { message } if message == "certificate rejected")
    );
    failed.join().unwrap();
}

#[test]
fn real_binary_reports_missing_pin_over_socket_and_exits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
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
