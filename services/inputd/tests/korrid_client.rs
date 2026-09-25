use std::{sync::Arc, time::Duration};

use korri_inputd::{
    korrid_client::{
        ExactPanelOutcome, ExactStopOutcome, KorridClient, LocalControlError, LocalControlLimits,
        SessionStatus,
    },
    virtual_targets::InputOwner,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
    sync::Mutex,
};

const LAUNCH_ID: &str = "0123456789abcdef0123456789abcdef";

fn limits(status_attempts: usize) -> LocalControlLimits {
    LocalControlLimits {
        operation_timeout: Duration::from_millis(500),
        status_attempts,
        retry_delay: Duration::from_millis(5),
        max_response_bytes: 4096,
    }
}

async fn reply(stream: &mut tokio::net::UnixStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(), body
    );
    stream.write_all(response.as_bytes()).await.unwrap();
}

async fn request_body(stream: &mut tokio::net::UnixStream) -> serde_json::Value {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        assert!(head.len() < 4096, "request headers exceeded test limit");
        stream.read_exact(&mut byte).await.unwrap();
        head.push(byte[0]);
    }
    let length = std::str::from_utf8(&head)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .trim()
        .parse::<usize>()
        .unwrap();
    let mut body = vec![0_u8; length];
    stream.read_exact(&mut body).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn status_ok(phase: &str) -> String {
    serde_json::json!({
        "_tag": "app.session.status",
        "outcome": {
            "_tag": "Ok",
            "payload": { "active": { "launchId": LAUNCH_ID, "phase": phase } }
        }
    })
    .to_string()
}

fn status_error(code: &str) -> String {
    serde_json::json!({
        "_tag": "app.session.status",
        "outcome": { "_tag": "Err", "payload": { "code": code, "message": "test" } }
    })
    .to_string()
}

#[test]
fn restart_owner_is_derived_from_exact_live_session_phase() {
    assert_eq!(
        SessionStatus::Running {
            launch_id: LAUNCH_ID.into(),
        }
        .input_owner(),
        InputOwner::Game
    );
    for status in [
        SessionStatus::Frozen {
            launch_id: LAUNCH_ID.into(),
        },
        SessionStatus::FocusFailed {
            launch_id: LAUNCH_ID.into(),
        },
        SessionStatus::Stopping {
            launch_id: LAUNCH_ID.into(),
        },
        SessionStatus::NoActive,
        SessionStatus::Completed,
        SessionStatus::RecoveryBlocked,
    ] {
        assert_eq!(status.input_owner(), InputOwner::Portal);
    }
}

#[tokio::test]
async fn status_reads_the_exact_running_launch() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(
            request_body(&mut stream).await,
            serde_json::json!({"_tag":"app.session.status", "payload":{}})
        );
        let mut next = [0_u8; 1];
        assert!(
            tokio::time::timeout(Duration::from_millis(25), stream.read(&mut next))
                .await
                .is_err(),
            "the client must not half-close before reading the response"
        );
        reply(&mut stream, &status_ok("running")).await;
    });

    assert_eq!(
        KorridClient::with_limits(path, limits(1))
            .status()
            .await
            .unwrap(),
        SessionStatus::Running {
            launch_id: LAUNCH_ID.into()
        }
    );
    server.await.unwrap();
}

#[tokio::test]
async fn exact_panel_freezes_running_and_thaws_frozen_without_retargeting() {
    for (phase, tag, state, expected) in [
        (
            "running",
            "app.session.freeze",
            "frozen",
            ExactPanelOutcome::Opened,
        ),
        (
            "frozen",
            "app.session.thaw",
            "running",
            ExactPanelOutcome::Returned,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        let server = tokio::spawn(async move {
            let (mut status, _) = listener.accept().await.unwrap();
            observed.lock().await.push(request_body(&mut status).await);
            reply(&mut status, &status_ok(phase)).await;
            drop(status);
            let (mut mutation, _) = listener.accept().await.unwrap();
            observed
                .lock()
                .await
                .push(request_body(&mut mutation).await);
            reply(
                &mut mutation,
                &serde_json::json!({
                    "_tag": tag,
                    "outcome": { "_tag": "Ok", "payload": {
                        "launchId": LAUNCH_ID, "state": state, "changed": true
                    }}
                })
                .to_string(),
            )
            .await;
        });

        assert_eq!(
            KorridClient::with_limits(path, limits(1))
                .toggle_panel_exact()
                .await
                .unwrap(),
            expected,
        );
        server.await.unwrap();
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1]["_tag"], tag);
        assert_eq!(requests[1]["payload"]["expectedLaunchId"], LAUNCH_ID);
    }
}

#[tokio::test]
async fn refused_portal_focus_rolls_back_to_the_exact_running_launch() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut status, _) = listener.accept().await.unwrap();
        assert_eq!(
            request_body(&mut status).await["_tag"],
            "app.session.status"
        );
        reply(&mut status, &status_ok("running")).await;
        drop(status);
        let (mut freeze, _) = listener.accept().await.unwrap();
        let request = request_body(&mut freeze).await;
        assert_eq!(request["_tag"], "app.session.freeze");
        assert_eq!(request["payload"]["expectedLaunchId"], LAUNCH_ID);
        reply(
            &mut freeze,
            &serde_json::json!({
                "_tag": "app.session.freeze",
                "outcome": { "_tag": "Ok", "payload": {
                    "launchId": LAUNCH_ID, "state": "frozen", "changed": true
                }}
            })
            .to_string(),
        )
        .await;
        drop(freeze);
        let (mut rollback, _) = listener.accept().await.unwrap();
        let request = request_body(&mut rollback).await;
        assert_eq!(request["_tag"], "app.session.thaw");
        assert_eq!(request["payload"]["expectedLaunchId"], LAUNCH_ID);
        reply(
            &mut rollback,
            &serde_json::json!({
                "_tag": "app.session.thaw",
                "outcome": { "_tag": "Ok", "payload": {
                    "launchId": LAUNCH_ID, "state": "running", "changed": false
                }}
            })
            .to_string(),
        )
        .await;
    });

    assert_eq!(
        KorridClient::with_limits(path, limits(1))
            .toggle_panel_exact_with(|| async { false })
            .await
            .unwrap(),
        ExactPanelOutcome::LeaveRefused,
    );
    server.await.unwrap();
}

#[tokio::test]
async fn failed_leave_rollback_without_proven_game_focus_stays_portal_owned() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut status, _) = listener.accept().await.unwrap();
        assert_eq!(
            request_body(&mut status).await["_tag"],
            "app.session.status"
        );
        reply(&mut status, &status_ok("running")).await;
        drop(status);

        let (mut freeze, _) = listener.accept().await.unwrap();
        let request = request_body(&mut freeze).await;
        assert_eq!(request["_tag"], "app.session.freeze");
        assert_eq!(request["payload"]["expectedLaunchId"], LAUNCH_ID);
        reply(
            &mut freeze,
            &serde_json::json!({
                "_tag": "app.session.freeze",
                "outcome": { "_tag": "Ok", "payload": {
                    "launchId": LAUNCH_ID, "state": "frozen", "changed": true
                }}
            })
            .to_string(),
        )
        .await;
        drop(freeze);

        let (mut thaw, _) = listener.accept().await.unwrap();
        let request = request_body(&mut thaw).await;
        assert_eq!(request["_tag"], "app.session.thaw");
        assert_eq!(request["payload"]["expectedLaunchId"], LAUNCH_ID);
        reply(
            &mut thaw,
            r#"{"_tag":"app.session.thaw","outcome":{"_tag":"Err","payload":{"code":"HostFocusFailed","message":"no unique exact-launch window could be focused"}}}"#,
        )
        .await;
    });

    let outcome = KorridClient::with_limits(path, limits(1))
        .toggle_panel_exact_with(|| async { false })
        .await
        .unwrap();
    assert_eq!(outcome, ExactPanelOutcome::FocusFailed);
    server.await.unwrap();
}

#[tokio::test]
async fn replacement_before_exact_freeze_never_enters_portal_or_retargets() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut status, _) = listener.accept().await.unwrap();
        assert_eq!(
            request_body(&mut status).await["_tag"],
            "app.session.status"
        );
        reply(&mut status, &status_ok("running")).await;
        drop(status);
        let (mut freeze, _) = listener.accept().await.unwrap();
        let request = request_body(&mut freeze).await;
        assert_eq!(request["_tag"], "app.session.freeze");
        assert_eq!(request["payload"]["expectedLaunchId"], LAUNCH_ID);
        reply(
            &mut freeze,
            r#"{"_tag":"app.session.freeze","outcome":{"_tag":"Err","payload":{"code":"SelectedRemoteSessionReplaced","message":"launch was replaced"}}}"#,
        )
        .await;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err(),
            "a refused exact freeze must not trigger another request"
        );
    });

    let entered_portal = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let callback_flag = Arc::clone(&entered_portal);
    assert_eq!(
        KorridClient::with_limits(path, limits(1))
            .toggle_panel_exact_with(|| async move {
                callback_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .await
            .unwrap(),
        ExactPanelOutcome::LeaveRefused,
    );
    assert!(!entered_portal.load(std::sync::atomic::Ordering::SeqCst));
    server.await.unwrap();
}

#[tokio::test]
async fn home_on_recovered_focus_failure_refreezes_exact_launch_and_recreates_intent() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&requests);
    let server = tokio::spawn(async move {
        let (mut status, _) = listener.accept().await.unwrap();
        observed.lock().await.push(request_body(&mut status).await);
        reply(&mut status, &status_ok("frozen")).await;
        drop(status);
        let (mut thaw, _) = listener.accept().await.unwrap();
        observed.lock().await.push(request_body(&mut thaw).await);
        reply(&mut thaw, r#"{"_tag":"app.session.thaw","outcome":{"_tag":"Err","payload":{"code":"HostFocusFailed","message":"focus refused"}}}"#).await;
        drop(thaw);
        let (mut status, _) = listener.accept().await.unwrap();
        observed.lock().await.push(request_body(&mut status).await);
        reply(&mut status, &status_ok("focus-failed")).await;
        drop(status);
        let (mut freeze, _) = listener.accept().await.unwrap();
        let request = request_body(&mut freeze).await;
        observed.lock().await.push(request.clone());
        assert_eq!(request["payload"]["expectedLaunchId"], LAUNCH_ID);
        reply(
            &mut freeze,
            &serde_json::json!({
                "_tag": "app.session.freeze",
                "outcome": { "_tag": "Ok", "payload": {
                    "launchId": LAUNCH_ID, "state": "frozen", "changed": true
                }}
            })
            .to_string(),
        )
        .await;
    });

    let client = KorridClient::with_limits(path, limits(1));
    assert_eq!(
        client.toggle_panel_exact().await.unwrap(),
        ExactPanelOutcome::FocusFailed,
    );
    assert_eq!(
        client.toggle_panel_exact().await.unwrap(),
        ExactPanelOutcome::Opened,
    );
    server.await.unwrap();
    let requests = requests.lock().await;
    assert_eq!(
        requests
            .iter()
            .map(|request| request["_tag"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "app.session.status",
            "app.session.thaw",
            "app.session.status",
            "app.session.freeze",
        ]
    );
}

#[tokio::test]
async fn rapid_home_presses_serialize_status_and_freezer_mutations() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&requests);
    let (first_status_seen_tx, first_status_seen_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut first_status_seen_tx = Some(first_status_seen_tx);
        for (index, (phase, tag, state)) in [
            ("running", "app.session.freeze", "frozen"),
            ("frozen", "app.session.thaw", "running"),
        ]
        .into_iter()
        .enumerate()
        {
            let (mut status, _) = listener.accept().await.unwrap();
            observed.lock().await.push(request_body(&mut status).await);
            if index == 0 {
                first_status_seen_tx.take().unwrap().send(()).unwrap();
            }
            reply(&mut status, &status_ok(phase)).await;
            drop(status);
            let (mut mutation, _) = listener.accept().await.unwrap();
            observed
                .lock()
                .await
                .push(request_body(&mut mutation).await);
            reply(
                &mut mutation,
                &serde_json::json!({
                    "_tag": tag,
                    "outcome": { "_tag": "Ok", "payload": {
                        "launchId": LAUNCH_ID, "state": state, "changed": true
                    }}
                })
                .to_string(),
            )
            .await;
        }
    });

    let client = Arc::new(KorridClient::with_limits(path, limits(1)));
    let first_client = Arc::clone(&client);
    let first = tokio::spawn(async move { first_client.toggle_panel_exact().await.unwrap() });
    first_status_seen_rx.await.unwrap();
    let second_client = Arc::clone(&client);
    let second = tokio::spawn(async move { second_client.toggle_panel_exact().await.unwrap() });

    assert_eq!(first.await.unwrap(), ExactPanelOutcome::Opened);
    assert_eq!(second.await.unwrap(), ExactPanelOutcome::Returned);
    server.await.unwrap();
    let tags = requests
        .lock()
        .await
        .iter()
        .map(|request| request["_tag"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        tags,
        [
            "app.session.status",
            "app.session.freeze",
            "app.session.status",
            "app.session.thaw",
        ]
    );
}

#[tokio::test]
async fn rapid_home_serializes_the_delayed_portal_focus_before_game_return() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let server_events = Arc::clone(&events);
    let (focus_entered_tx, focus_entered_rx) = tokio::sync::oneshot::channel();
    let (release_focus_tx, release_focus_rx) = tokio::sync::oneshot::channel();
    let (second_started_tx, second_started_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut status, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut status).await;
        reply(&mut status, &status_ok("running")).await;
        drop(status);
        let (mut freeze, _) = listener.accept().await.unwrap();
        let request = request_body(&mut freeze).await;
        assert_eq!(request["_tag"], "app.session.freeze");
        server_events.lock().await.push("freeze");
        reply(
            &mut freeze,
            &serde_json::json!({
                "_tag": "app.session.freeze",
                "outcome": { "_tag": "Ok", "payload": {
                    "launchId": LAUNCH_ID, "state": "frozen", "changed": true
                }}
            })
            .to_string(),
        )
        .await;
        drop(freeze);
        second_started_rx.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err(),
            "the second Home escaped while the first portal focus was pending"
        );

        let (mut status, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut status).await;
        reply(&mut status, &status_ok("frozen")).await;
        drop(status);
        let (mut thaw, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut thaw).await;
        server_events.lock().await.push("game-focus");
        reply(
            &mut thaw,
            &serde_json::json!({
                "_tag": "app.session.thaw",
                "outcome": { "_tag": "Ok", "payload": {
                    "launchId": LAUNCH_ID, "state": "running", "changed": true
                }}
            })
            .to_string(),
        )
        .await;
    });

    let client = Arc::new(KorridClient::with_limits(path, limits(1)));
    let first_client = Arc::clone(&client);
    let focus_events = Arc::clone(&events);
    let first = tokio::spawn(async move {
        first_client
            .toggle_panel_exact_with(|| async move {
                focus_events.lock().await.push("portal-focus-start");
                focus_entered_tx.send(()).unwrap();
                release_focus_rx.await.unwrap();
                focus_events.lock().await.push("portal-focus-end");
                true
            })
            .await
            .unwrap()
    });
    focus_entered_rx.await.unwrap();

    let second_client = Arc::clone(&client);
    let second = tokio::spawn(async move {
        second_started_tx.send(()).unwrap();
        second_client.toggle_panel_exact().await.unwrap()
    });
    tokio::time::sleep(Duration::from_millis(75)).await;
    release_focus_tx.send(()).unwrap();

    assert_eq!(first.await.unwrap(), ExactPanelOutcome::Opened);
    assert_eq!(second.await.unwrap(), ExactPanelOutcome::Returned);
    server.await.unwrap();
    assert_eq!(
        *events.lock().await,
        [
            "freeze",
            "portal-focus-start",
            "portal-focus-end",
            "game-focus"
        ]
    );
}

#[tokio::test]
async fn exact_stop_uses_the_observed_launch_id_and_mutates_once() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&requests);
    let server = tokio::spawn(async move {
        let (mut status, _) = listener.accept().await.unwrap();
        observed.lock().await.push(request_body(&mut status).await);
        reply(&mut status, &status_ok("running")).await;
        drop(status);
        let (mut stop, _) = listener.accept().await.unwrap();
        observed.lock().await.push(request_body(&mut stop).await);
        reply(
            &mut stop,
            r#"{"_tag":"app.session.stop","outcome":{"_tag":"Ok","payload":{"phase":"stopped"}}}"#,
        )
        .await;
    });

    assert_eq!(
        KorridClient::with_limits(path, limits(2))
            .stop_active_exact()
            .await
            .unwrap(),
        ExactStopOutcome::Completed
    );
    server.await.unwrap();
    let requests = requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1]["_tag"], "app.session.stop");
    assert_eq!(requests[1]["payload"]["expectedLaunchId"], LAUNCH_ID);
}

#[tokio::test]
async fn stopping_returns_already_stopping_without_a_stop_mutation() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut status, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut status).await;
        reply(&mut status, &status_ok("stopping")).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    });

    assert_eq!(
        KorridClient::with_limits(path, limits(1))
            .stop_active_exact()
            .await
            .unwrap(),
        ExactStopOutcome::AlreadyStopping
    );
    server.await.unwrap();
}

#[tokio::test]
async fn terminal_and_recovery_statuses_never_send_a_stop() {
    for (code, expected) in [
        ("NoActiveSession", ExactStopOutcome::NoActive),
        ("SelectedRemoteSessionReplaced", ExactStopOutcome::NoActive),
        ("HostRecoveryBlocked", ExactStopOutcome::RecoveryBlocked),
        ("SessionCompleted", ExactStopOutcome::Completed),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (mut status, _) = listener.accept().await.unwrap();
            let _ = request_body(&mut status).await;
            reply(&mut status, &status_error(code)).await;
            assert!(
                tokio::time::timeout(Duration::from_millis(50), listener.accept())
                    .await
                    .is_err()
            );
        });

        assert_eq!(
            KorridClient::with_limits(path, limits(1))
                .stop_active_exact()
                .await
                .unwrap(),
            expected
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn stop_outcomes_distinguish_stale_and_already_stopping() {
    for (stop_body, expected) in [
        (
            r#"{"_tag":"app.session.stop","outcome":{"_tag":"Err","payload":{"code":"StaleLaunchIdentity","message":"stale"}}}"#,
            ExactStopOutcome::StaleIdentity,
        ),
        (
            r#"{"_tag":"app.session.stop","outcome":{"_tag":"Err","payload":{"code":"SelectedRemoteSessionReplaced","message":"replaced"}}}"#,
            ExactStopOutcome::StaleIdentity,
        ),
        (
            r#"{"_tag":"app.session.stop","outcome":{"_tag":"Ok","payload":{"phase":"pending"}}}"#,
            ExactStopOutcome::AlreadyStopping,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (mut status, _) = listener.accept().await.unwrap();
            let _ = request_body(&mut status).await;
            reply(&mut status, &status_ok("running")).await;
            drop(status);
            let (mut stop, _) = listener.accept().await.unwrap();
            let _ = request_body(&mut stop).await;
            reply(&mut stop, stop_body).await;
        });

        assert_eq!(
            KorridClient::with_limits(path, limits(1))
                .stop_active_exact()
                .await
                .unwrap(),
            expected
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn read_only_status_retries_but_stop_is_never_retried() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut first_status, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut first_status).await;
        drop(first_status);
        let (mut second_status, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut second_status).await;
        reply(&mut second_status, &status_ok("running")).await;
        drop(second_status);
        let (mut stop, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut stop).await;
        drop(stop);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });

    let error = KorridClient::with_limits(path, limits(2))
        .stop_active_exact()
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LocalControlError::InvalidHttpResponse | LocalControlError::Read(_)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn response_framing_requires_one_exact_bounded_content_length() {
    for response in [
        "HTTP/1.1 200 OK\r\nconnection: close\r\n\r\n{}".to_owned(),
        "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nContent-Length: 2\r\n\r\n{}".to_owned(),
        "HTTP/1.1 200 OK\r\ncontent-length: nope\r\n\r\n{}".to_owned(),
        "HTTP/1.1 200 OK\r\ncontent-length: 3\r\n\r\n{}".to_owned(),
        "HTTP/1.1 200 OK\r\ncontent-length: 1\r\n\r\n{}".to_owned(),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = request_body(&mut stream).await;
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        assert!(matches!(
            KorridClient::with_limits(path, limits(1)).status().await,
            Err(LocalControlError::InvalidHttpResponse)
        ));
        server.await.unwrap();
    }

    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 99999\r\n\r\n")
            .await
            .unwrap();
    });
    assert!(matches!(
        KorridClient::with_limits(path, limits(1)).status().await,
        Err(LocalControlError::ResponseTooLarge)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn response_size_is_bounded() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = request_body(&mut stream).await;
        stream.write_all(&vec![b'x'; 1024]).await.unwrap();
    });
    let mut bounded = limits(1);
    bounded.max_response_bytes = 128;

    assert!(matches!(
        KorridClient::with_limits(path, bounded).status().await,
        Err(LocalControlError::ResponseTooLarge)
    ));
    server.await.unwrap();
}
