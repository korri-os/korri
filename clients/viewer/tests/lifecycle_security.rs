// SPDX-License-Identifier: GPL-3.0-or-later
use korri_viewer::lifecycle::Lifecycle;
use korri_viewer_treaty::{
    ViewerCommand as C, ViewerDisplay, ViewerMessage as M, ViewerReply as R, ViewerRequest,
    read_message, write_message,
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

fn uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[test]
fn wrong_uid_is_refused_or_acl_unavailability_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/viewer.sock");
    let lifecycle = match Lifecycle::bind(&path, "a", uid() + 1) {
        Ok(lifecycle) => lifecycle,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .and_then(|e| e.raw_os_error())
                == Some(libc::EOPNOTSUPP) =>
        {
            // The Nix build filesystem has no POSIX ACLs. It must fail closed,
            // not publish a world-accessible socket to make the build pass.
            assert!(!path.exists());
            assert!(!path.parent().unwrap().exists());
            return;
        }
        Err(error) => panic!("{error:#}"),
    };
    let mut socket = UnixStream::connect(&path).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut byte = [0];
    assert_eq!(
        socket.read(&mut byte).unwrap(),
        0,
        "an unexpected peer must not be allowed to wait for Start"
    );
    assert!(
        !lifecycle.stop_requested(),
        "a refused peer must not consume the controller slot"
    );
    lifecycle.failed("test finished".into());
    lifecycle.drain();
}

// Run explicitly as root. All three participants drop to distinct unprivileged
// UIDs. No account, service, network connection or shared file is changed.
#[test]
#[ignore = "requires root to create three real, unprivileged UID processes"]
fn separate_viewer_and_controller_uids_are_the_only_users_with_socket_access() {
    assert_eq!(uid(), 0, "run this proof as root");
    let directory = tempfile::tempdir().unwrap();
    // The viewer must create its own private child; it cannot write a root-owned
    // 0755 test directory. Sticky mode protects the copied executable.
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o1777)).unwrap();
    let executable = directory.path().join("security-test");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let socket = directory.path().join("control/viewer.sock");
    let viewer_uid = 65534;
    let controller_uid = 65532;
    let mut viewer = helper(&executable, &socket, "viewer", viewer_uid, controller_uid)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    let mut stdout = BufReader::new(viewer.stdout.take().unwrap());
    while stdout.read_line(&mut ready).unwrap() != 0 {
        if ready.trim_end().ends_with("ready") {
            break;
        }
        ready.clear();
    }
    assert!(
        ready.trim_end().ends_with("ready"),
        "viewer must bind and grant controller access"
    );
    // Neither a third UID, nor the socket's owner acting as a controller, may
    // acquire this connection. Rejection must leave the real controller slot free.
    for (role, peer_uid) in [
        ("excluded", 65533),
        ("wrong-peer", viewer_uid),
        ("controller", controller_uid),
    ] {
        let output = helper(&executable, &socket, role, peer_uid, controller_uid)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{role}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(viewer.wait().unwrap().success());
    let mut log = String::new();
    viewer
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut log)
        .unwrap();
    assert!(
        log.contains("refused lifecycle peer UID 65534; expected 65532"),
        "{log}"
    );
    assert!(!socket.exists());
    assert!(!socket.parent().unwrap().exists());
}

fn helper(
    executable: &Path,
    socket: &Path,
    role: &str,
    peer_uid: u32,
    controller_uid: u32,
) -> Command {
    let mut command = Command::new(executable);
    command
        .args(["--exact", "credential_process", "--ignored", "--nocapture"])
        .env("VIEWER_SECURITY_ROLE", role)
        .env("VIEWER_SECURITY_SOCKET", socket)
        .env("VIEWER_SECURITY_CONTROLLER", controller_uid.to_string())
        .uid(peer_uid)
        .gid(peer_uid);
    command
}

#[test]
#[ignore = "subprocess entry for the explicit three-UID proof"]
fn credential_process() {
    let role = std::env::var("VIEWER_SECURITY_ROLE").expect("start through the three-UID proof");
    let path = std::env::var_os("VIEWER_SECURITY_SOCKET").unwrap();
    let path = Path::new(&path);
    let controller: u32 = std::env::var("VIEWER_SECURITY_CONTROLLER")
        .unwrap()
        .parse()
        .unwrap();
    match role.as_str() {
        "viewer" => {
            // Even an inherited permissive umask must never publish the socket
            // through a directory that an unrelated UID can traverse.
            unsafe {
                libc::umask(0);
            }
            let runtime = Lifecycle::bind(path, "a", controller).unwrap();
            println!("ready");
            std::io::stdout().flush().unwrap();
            runtime.wait_start().unwrap();
            runtime
                .connected(ViewerDisplay {
                    width: 640,
                    height: 480,
                })
                .unwrap();
            let until = std::time::Instant::now() + Duration::from_secs(5);
            while !runtime.stop_requested() {
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(Duration::from_millis(5));
            }
            runtime.stopped();
            runtime.drain();
        }
        "excluded" => {
            let error = UnixStream::connect(path).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        }
        "wrong-peer" => {
            let mut socket = UnixStream::connect(path).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut byte = [0];
            assert_eq!(socket.read(&mut byte).unwrap(), 0);
        }
        "controller" => {
            let mut socket = BufReader::new(UnixStream::connect(path).unwrap());
            socket
                .get_mut()
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            for command in [
                C::Start {
                    address: "127.0.0.1".into(),
                    display: None,
                },
                C::Stop,
            ] {
                write_message(
                    socket.get_mut(),
                    &ViewerRequest {
                        launch_id: "a".into(),
                        command,
                    },
                )
                .unwrap();
                loop {
                    match read_message(&mut socket).unwrap().unwrap() {
                        M::Reply(R::Ok) => break,
                        M::Event(_) => (),
                        message => panic!("unexpected reply: {message:?}"),
                    }
                }
            }
        }
        _ => panic!("unknown process role"),
    }
}
