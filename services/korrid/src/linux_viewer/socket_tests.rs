use super::*;
use korri_viewer_treaty::{
    read_message, write_message, ViewerCommand as C, ViewerEvent as E, ViewerMessage as M,
    ViewerReply as R, ViewerRequest, ViewerStatus as S,
};
use std::{io::BufReader, os::unix::net::UnixListener, path::PathBuf, sync::Arc};

const LAUNCH: &str = "0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn socket_viewer_uses_exact_launch_typed_events_and_no_unit_observation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
    let listener = UnixListener::bind(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let server = std::thread::spawn(move || {
        let mut socket = BufReader::new(listener.accept().unwrap().0);
        for expected in [
            "Start",
            "Snapshot",
            "Background",
            "Status",
            "ResumeAndFocus",
            "Resize",
            "Apply",
            "Notice",
            "Stop",
        ] {
            let request: ViewerRequest = read_message(&mut socket).unwrap().unwrap();
            assert_eq!(request.launch_id, LAUNCH);
            let reply = match (expected, request.command) {
                ("Start", C::Start { address, .. }) => {
                    assert_eq!(address, "127.0.0.1");
                    R::Ok
                }
                ("Snapshot", C::Snapshot) => R::Snapshot {
                    snapshot: Some(Default::default()),
                },
                ("Background", C::Background) | ("ResumeAndFocus", C::ResumeAndFocus) => R::Ok,
                ("Status", C::Status) => R::Status(S::Backgrounded),
                (
                    "Resize",
                    C::Resize {
                        connection,
                        display,
                    },
                ) => {
                    assert_eq!(connection, "connection-1");
                    assert_eq!(display.width, 640);
                    R::Error {
                        message: "Live resolution is unavailable".into(),
                    }
                }
                ("Apply", C::Apply { control, value }) => {
                    assert_eq!(control, "fit");
                    assert!(
                        matches!(value, Some(korri_surface_treaty::SurfaceGameplayControlValue::Choice { value }) if value == "fit")
                    );
                    R::Error {
                        message: "Local viewer control unavailable".into(),
                    }
                }
                ("Notice", C::Notice { message }) => {
                    assert_eq!(message, "notice");
                    R::Error {
                        message: "Viewer notice unavailable".into(),
                    }
                }
                ("Stop", C::Stop) => {
                    write_message(socket.get_mut(), &M::Event(E::Status(S::Stopped))).unwrap();
                    R::Ok
                }
                pair => panic!("unexpected request: {pair:?}"),
            };
            write_message(socket.get_mut(), &M::Reply(reply)).unwrap();
        }
    });
    let focused = Arc::new(std::sync::Mutex::new(Vec::new()));
    let effects = focused.clone();
    let stopped = Arc::new(std::sync::Mutex::new(Vec::new()));
    let cleanup = stopped.clone();
    let viewer = SocketLinuxStreamViewer::new(
        Arc::new(|| None),
        unsafe { libc::geteuid() },
        Arc::new(move |_, _, _| {
            let path = path.clone();
            Box::pin(async move { Ok(path) })
        }),
        Arc::new(move |launch, background| {
            effects.lock().unwrap().push((launch, background));
            Box::pin(async { Ok(()) })
        }),
        Arc::new(move |launch| {
            cleanup.lock().unwrap().push(launch);
            Box::pin(async { Ok(()) })
        }),
    );
    viewer.start(LAUNCH, "127.0.0.1", None).await.unwrap();
    assert!(viewer.snapshot(LAUNCH).await.unwrap().is_some());
    viewer.background(LAUNCH).await.unwrap();
    assert_eq!(
        viewer.status(LAUNCH).await.unwrap(),
        LinuxViewerStatus::Backgrounded
    );
    viewer.resume_and_focus(LAUNCH).await.unwrap();
    assert_eq!(
        viewer
            .resize(
                LAUNCH,
                "connection-1",
                crate::launcher::plugin_launch::PluginDisplay {
                    width: 640,
                    height: 480
                }
            )
            .await
            .unwrap_err()
            .to_string(),
        "Live resolution is unavailable"
    );
    assert!(viewer
        .apply(
            LAUNCH,
            "fit",
            Some(korri_surface_treaty::SurfaceGameplayControlValue::Choice {
                value: "fit".into()
            })
        )
        .await
        .is_err());
    assert!(viewer.notice(LAUNCH, "notice").await.is_err());
    let changed = viewer.change_signal().unwrap();
    assert!(stopped.lock().unwrap().is_empty());
    viewer.stop(LAUNCH).await.unwrap();
    // korrid still confirms that the unit ended after the viewer's Stop reply.
    assert_eq!(*stopped.lock().unwrap(), vec![LAUNCH.to_owned()]);
    tokio::time::timeout(std::time::Duration::from_secs(1), changed.notified())
        .await
        .unwrap();
    assert_eq!(
        viewer.status(LAUNCH).await.unwrap(),
        LinuxViewerStatus::Stopped
    );
    assert_eq!(
        *focused.lock().unwrap(),
        vec![
            (LAUNCH.into(), false),
            (LAUNCH.into(), true),
            (LAUNCH.into(), false)
        ]
    );
    assert!(viewer
        .background("ffffffffffffffffffffffffffffffff")
        .await
        .is_err());
    server.join().unwrap();
}

#[tokio::test]
async fn unexpected_eof_is_not_reported_as_a_confirmed_stop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
    let listener = UnixListener::bind(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let server = std::thread::spawn(move || {
        let mut socket = BufReader::new(listener.accept().unwrap().0);
        let _: ViewerRequest = read_message(&mut socket).unwrap().unwrap();
        // Close during Start, without any reply or Stopped event.
    });
    let cleaned = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cleanup = cleaned.clone();
    let viewer = SocketLinuxStreamViewer::new(
        Arc::new(|| None),
        unsafe { libc::geteuid() },
        Arc::new(move |_, _, _| {
            let path = path.clone();
            Box::pin(async move { Ok(path) })
        }),
        Arc::new(|_, _| Box::pin(async { Ok(()) })),
        Arc::new(move |launch| {
            assert_eq!(launch, LAUNCH);
            cleanup.store(true, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        }),
    );
    assert!(viewer.start(LAUNCH, "127.0.0.1", None).await.is_err());
    assert!(cleaned.load(std::sync::atomic::Ordering::SeqCst));
    server.join().unwrap();
}

#[tokio::test]
async fn cancelled_request_cannot_confirm_a_later_request() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
    let listener = UnixListener::bind(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let received = Arc::new(tokio::sync::Notify::new());
    let began = received.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut socket = BufReader::new(listener.accept().unwrap().0);
        let _: ViewerRequest = read_message(&mut socket).unwrap().unwrap();
        write_message(socket.get_mut(), &M::Reply(R::Ok)).unwrap();
        let request: ViewerRequest = read_message(&mut socket).unwrap().unwrap();
        assert!(matches!(request.command, C::Snapshot));
        began.notify_one();
        wait.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let _ = write_message(socket.get_mut(), &M::Reply(R::Snapshot { snapshot: None }));
    });
    let viewer = Arc::new(SocketLinuxStreamViewer::new(
        Arc::new(|| None),
        unsafe { libc::geteuid() },
        Arc::new(move |_, _, _| {
            let path = path.clone();
            Box::pin(async move { Ok(path) })
        }),
        Arc::new(|_, _| Box::pin(async { Ok(()) })),
        Arc::new(|_| Box::pin(async { Ok(()) })),
    ));
    viewer.start(LAUNCH, "127.0.0.1", None).await.unwrap();
    let asking = viewer.clone();
    let task = tokio::spawn(async move { asking.snapshot(LAUNCH).await });
    received.notified().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    assert!(viewer.status(LAUNCH).await.is_err());
    server.join().unwrap();
}

#[tokio::test]
async fn socket_owner_and_peer_must_match_the_launchers_expected_viewer_uid() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
    let listener = UnixListener::bind(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let server = std::thread::spawn(move || {
        let mut socket = BufReader::new(listener.accept().unwrap().0);
        assert!(
            read_message::<ViewerRequest>(&mut socket)
                .unwrap()
                .is_none(),
            "an unexpected viewer must receive no commands"
        );
    });
    let viewer = SocketLinuxStreamViewer::new(
        Arc::new(|| None),
        unsafe { libc::geteuid() } + 1,
        Arc::new(move |_, _, _| {
            let path = path.clone();
            Box::pin(async move { Ok(path) })
        }),
        Arc::new(|_, _| Box::pin(async { panic!("an unexpected viewer must not get focus") })),
        Arc::new(|_| Box::pin(async { Ok(()) })),
    );
    assert!(viewer.start(LAUNCH, "127.0.0.1", None).await.is_err());
    server.join().unwrap();
}

#[tokio::test]
async fn stop_after_a_lost_connection_still_ends_the_unit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.sock");
    let listener = UnixListener::bind(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let server = std::thread::spawn(move || {
        let mut socket = BufReader::new(listener.accept().unwrap().0);
        let _: ViewerRequest = read_message(&mut socket).unwrap().unwrap();
        write_message(socket.get_mut(), &M::Reply(R::Ok)).unwrap();
        // The viewer goes away without a Stopped event.
    });
    let stopped = Arc::new(std::sync::Mutex::new(Vec::new()));
    let cleanup = stopped.clone();
    let viewer = SocketLinuxStreamViewer::new(
        Arc::new(|| None),
        unsafe { libc::geteuid() },
        Arc::new(move |_, _, _| {
            let path = path.clone();
            Box::pin(async move { Ok(path) })
        }),
        Arc::new(|_, _| Box::pin(async { Ok(()) })),
        Arc::new(move |launch| {
            cleanup.lock().unwrap().push(launch);
            Box::pin(async { Ok(()) })
        }),
    );
    let changed = viewer.change_signal().unwrap();
    viewer.start(LAUNCH, "127.0.0.1", None).await.unwrap();
    server.join().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), changed.notified())
        .await
        .unwrap();
    assert!(
        viewer.status(LAUNCH).await.is_err(),
        "a lost viewer is not a stopped one"
    );
    viewer.stop(LAUNCH).await.unwrap();
    assert_eq!(*stopped.lock().unwrap(), vec![LAUNCH.to_owned()]);
    assert_eq!(
        viewer.status(LAUNCH).await.unwrap(),
        LinuxViewerStatus::Stopped
    );
}

fn mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// The viewer makes its directory 0700 before the unit's ExecStartPost grants
/// korrid search access. A connect in between gets EACCES.
fn ungranted_socket() -> (tempfile::TempDir, PathBuf, PathBuf, UnixListener) {
    let dir = tempfile::tempdir().unwrap();
    let control = dir.path().join("control");
    std::fs::create_dir(&control).unwrap();
    let path = control.join("viewer.sock");
    let listener = UnixListener::bind(&path).unwrap();
    mode(&path, 0o600);
    mode(&control, 0o000);
    (dir, control, path, listener)
}

fn plain_viewer(path: PathBuf) -> SocketLinuxStreamViewer {
    SocketLinuxStreamViewer::new(
        Arc::new(|| None),
        unsafe { libc::geteuid() },
        Arc::new(move |_, _, _| {
            let path = path.clone();
            Box::pin(async move { Ok(path) })
        }),
        Arc::new(|_, _| Box::pin(async { Ok(()) })),
        Arc::new(|_| Box::pin(async { Ok(()) })),
    )
}

#[tokio::test]
async fn a_socket_korrid_may_not_yet_search_is_retried_until_its_grant_arrives() {
    // Root ignores directory modes, so the refusal cannot be made there.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let (_dir, control, path, listener) = ungranted_socket();
    let server = std::thread::spawn(move || {
        let mut socket = BufReader::new(listener.accept().unwrap().0);
        let request: ViewerRequest = read_message(&mut socket).unwrap().unwrap();
        assert!(matches!(request.command, C::Start { .. }));
        write_message(socket.get_mut(), &M::Reply(R::Ok)).unwrap();
    });
    let granted = control.clone();
    let grant = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        mode(&granted, 0o700);
    });
    plain_viewer(path)
        .start(LAUNCH, "127.0.0.1", None)
        .await
        .unwrap();
    grant.join().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn a_socket_that_stays_unreachable_fails_at_the_deadline_with_its_last_error() {
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let (_dir, control, path, _listener) = ungranted_socket();
    let deadline = std::time::Duration::from_millis(300);
    let viewer = plain_viewer(path).with_connect_deadline(deadline);
    let started = std::time::Instant::now();
    let refused = viewer
        .start(LAUNCH, "127.0.0.1", None)
        .await
        .unwrap_err()
        .to_string();
    assert!(started.elapsed() >= deadline, "gave up early: {refused}");
    assert!(refused.contains("Permission denied"), "{refused}");
    mode(&control, 0o700);
}
