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

mod support;

fn uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[test]
fn wrong_uid_is_refused_or_acl_unavailability_fails_closed() {
    let dir = support::scratch();
    let path = dir.path().join("control/viewer.sock");
    if support::refused_below_an_unmapped_root(&path) {
        return;
    }
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

// A different local user who can write an ancestor can replace the private
// directory by name. The viewer must refuse such a location before it creates
// anything. Only the owner of a parent can make it unsafe here, so the test
// needs no second UID.
#[test]
fn location_another_user_could_rewrite_is_refused_before_anything_is_created() {
    let root = support::scratch();
    if support::refused_below_an_unmapped_root(&root.path().join("control/viewer.sock")) {
        return;
    }
    let cases: [(&str, u32, &str); 5] = [
        ("other-writable", 0o707, "control/viewer.sock"),
        ("group-writable", 0o770, "control/viewer.sock"),
        ("sticky", 0o1777, "control/viewer.sock"),
        ("writable-grandparent", 0o777, "private/control/viewer.sock"),
        ("private", 0o700, "link/control/viewer.sock"),
    ];
    for (name, mode, socket) in cases {
        let parent = root.path().join(name);
        std::fs::create_dir(&parent).unwrap();
        std::fs::create_dir(parent.join("private")).unwrap();
        std::os::unix::fs::symlink("private", parent.join("link")).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(mode)).unwrap();
        let socket = parent.join(socket);
        let error = match Lifecycle::bind(&socket, "a", uid()) {
            Ok(_) => panic!("{name}: the viewer bound a socket in an unsafe location"),
            Err(error) => format!("{error:#}"),
        };
        let unsafe_ancestor = if name == "private" {
            parent.join("link")
        } else {
            parent.clone()
        };
        assert!(
            error.contains(&format!("ancestor {} is ", unsafe_ancestor.display())),
            "{name}: the refusal must name the unsafe ancestor: {error}"
        );
        assert!(
            std::fs::symlink_metadata(socket.parent().unwrap()).is_err()
                && std::fs::read_dir(parent.join("private"))
                    .unwrap()
                    .next()
                    .is_none(),
            "{name}: a refused location must stay untouched"
        );
    }
}

// A sticky directory lets another user own a name in it. The third review
// (#435) turned that into a socket under the attacker's directory, so no
// shared directory is a trusted ancestor, whoever owns it.
#[test]
fn shared_sticky_directory_is_not_a_trusted_ancestor() {
    use std::os::unix::fs::MetadataExt;
    let shared = Path::new("/tmp");
    let Ok(metadata) = std::fs::metadata(shared) else {
        return;
    };
    if metadata.uid() != 0 || metadata.mode() & 0o1002 != 0o1002 {
        return; // No root-owned sticky /tmp here, as in the Nix build sandbox.
    }
    // The viewer's own private directory inside it does not make it safe.
    let private = tempfile::tempdir_in(shared).unwrap();
    let socket = private.path().join("control/viewer.sock");
    let error = match Lifecycle::bind(&socket, "a", uid()) {
        Ok(_) => panic!("the viewer bound a socket below a shared sticky directory"),
        Err(error) => format!("{error:#}"),
    };
    assert!(error.contains("ancestor /tmp is writable"), "{error}");
    assert!(!socket.parent().unwrap().exists());
}

// Run explicitly as root. All three participants drop to distinct unprivileged
// UIDs. No account, service, network connection or shared file is changed.
#[test]
#[ignore = "requires root to create three real, unprivileged UID processes"]
fn separate_viewer_and_controller_uids_are_the_only_users_with_socket_access() {
    assert_eq!(uid(), 0, "run this proof as root");
    // As a launcher does it: root owns the path down from `/`, no other user
    // can write it, and the viewer owns the last directory. /tmp is refused.
    let directory = tempfile::Builder::new()
        .prefix("korri-viewer-proof.")
        .tempdir_in("/run")
        .unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let executable = directory.path().join("security-test");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let viewer_uid = 65534;
    let controller_uid = 65532;
    // The controller needs search access here. Nobody but the viewer can write.
    let runtime = directory.path().join("viewer");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o711)).unwrap();
    std::os::unix::fs::chown(&runtime, Some(viewer_uid), Some(viewer_uid)).unwrap();
    let socket = runtime.join("control/viewer.sock");
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

// #468: the EOPNOTSUPP arm of the test above never runs. The Nix build
// sandbox refuses at `/` first, and a login session's runtime directory has
// ACLs. ramfs has none. Run explicitly as root, like the proof above; the
// mount exists only in the child's own mount namespace.
#[test]
#[ignore = "requires root to mount a filesystem without POSIX ACLs"]
fn filesystem_without_acls_fails_closed_and_keeps_nothing_the_viewer_made() {
    use std::ffi::CString;
    assert_eq!(uid(), 0, "run this proof as root");
    let directory = tempfile::Builder::new()
        .prefix("korri-viewer-acl.")
        .tempdir_in("/run")
        .unwrap();
    let target = CString::new(directory.path().as_os_str().as_encoded_bytes()).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "acl_free_process", "--ignored", "--nocapture"])
        .env(
            "VIEWER_SECURITY_SOCKET",
            directory.path().join("control/viewer.sock"),
        );
    // SAFETY: only system calls on data prepared before fork.
    unsafe {
        command.pre_exec(move || {
            let private = libc::unshare(libc::CLONE_NEWNS) == 0
                && libc::mount(
                    std::ptr::null(),
                    c"/".as_ptr(),
                    std::ptr::null(),
                    libc::MS_REC | libc::MS_PRIVATE,
                    std::ptr::null(),
                ) == 0
                && libc::mount(
                    c"korri-viewer-acl".as_ptr(),
                    target.as_ptr(),
                    c"ramfs".as_ptr(),
                    0,
                    c"mode=0755".as_ptr().cast(),
                ) == 0;
            if private {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("test acl_free_process ... ok"),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "subprocess entry for the ACL-free filesystem proof"]
fn acl_free_process() {
    let path = std::env::var_os("VIEWER_SECURITY_SOCKET").expect("start through the ACL proof");
    let socket = Path::new(&path);
    let error = match Lifecycle::bind(socket, "a", 65532) {
        Ok(_) => panic!("the viewer published a socket on a filesystem without ACLs"),
        Err(error) => error,
    };
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .and_then(|e| e.raw_os_error()),
        Some(libc::EOPNOTSUPP),
        "{error:#}"
    );
    let mount = socket.parent().unwrap().parent().unwrap();
    assert!(
        std::fs::read_dir(mount).unwrap().next().is_none(),
        "the refused viewer must remove its socket and its directory"
    );
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
            runtime.wait_start().unwrap().expect("Start came first");
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
