use super::*;
use korri_viewer_treaty::{
    read_message, write_message, ViewerCommand as C, ViewerEvent as E, ViewerMessage as M,
    ViewerReply as R, ViewerRequest, ViewerStatus as S,
};
use std::{io::BufReader, os::unix::net::UnixListener, sync::Arc};

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
    let viewer = SocketLinuxStreamViewer::new(
        None,
        unsafe { libc::geteuid() },
        Arc::new(move |_, _, _| {
            let path = path.clone();
            Box::pin(async move { Ok(path) })
        }),
        Arc::new(move |launch, background| {
            effects.lock().unwrap().push((launch, background));
            Box::pin(async { Ok(()) })
        }),
        Arc::new(|_| Box::pin(async { Ok(()) })),
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
    viewer.stop(LAUNCH).await.unwrap();
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
        None,
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
        None,
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
        None,
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
