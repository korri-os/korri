#[tokio::test]
async fn stream_player_pause_end_and_local_leave_never_control_the_holder() {
    use crate::linux_viewer::test_backend::{TestLinuxStreamViewer, LinuxViewerAction};
    let (url, far) = exact_native_server(false).await;
    let config = UpstreamHostConfig::native("far", url);
    let key = config.device_public_key.clone().unwrap();
    let viewer = TestLinuxStreamViewer::default();
    let registry = UpstreamRegistry::new(vec![config]).with_linux_viewer(Arc::new(viewer.clone()));
    assert_eq!(registry.stream_play(&key, &far.launch_id, "Pocket Mini").await.unwrap_err().code(), "NoActiveSession");
    far.set_phase("running");
    assert_eq!(registry.stream_play(&key, "wrong", "Pocket Mini").await.unwrap_err().code(), "LaunchMismatch");
    let active = registry.stream_play(&key, &far.launch_id, "Pocket Mini").await.unwrap();
    assert_eq!(active.stream_players.len(), 1);
    assert_eq!(active.stream_players[0].label, "Pocket Mini");
    for tag in ["app.session.controls", "app.session.control.invoke"] {
        let payload = if tag == "app.session.controls" { serde_json::json!({"launchId":far.launch_id}) }
            else { serde_json::json!({"launchId":far.launch_id,"controlId":"plugin/action"}) };
        let request = serde_json::from_value(serde_json::json!({"_tag":tag,"payload":payload})).unwrap();
        let response = crate::stream_play::device::stream_player_control_refusal(&far.launch_id, &request).unwrap();
        assert_eq!(serde_json::to_value(response).unwrap()["outcome"]["payload"]["reason"], "NotSessionStarter");
    }

    assert_eq!(registry.stream_play_launch_id().as_deref(), Some(far.launch_id.as_str()));
    far.set_phase("frozen");
    registry.session_status().await.unwrap();
    assert!(viewer.calls().iter().any(|call| call.action == LinuxViewerAction::Background));
    far.set_phase("running");
    registry.session_status().await.unwrap();
    assert!(viewer.calls().iter().any(|call| call.action == LinuxViewerAction::ResumeAndFocus));
    registry.session_freeze(Some(&far.launch_id)).await.unwrap();
    assert_eq!(far.active_phase.lock().unwrap().as_deref(), Some("running"));
    assert!(!registry.has_selected_session());
    assert!(registry.stream_play_launch_id().is_none());
    registry.stream_play(&key, &far.launch_id, "Pocket Mini").await.unwrap();
    registry.session_stop(Some(&far.launch_id), true).await.unwrap();
    assert_eq!(far.active_phase.lock().unwrap().as_deref(), Some("running"));
    registry.stream_play(&key, &far.launch_id, "Pocket Mini").await.unwrap();
    *far.active_phase.lock().unwrap() = None;
    registry.session_status().await.unwrap();
    assert!(!registry.has_selected_session());
    let ended = registry.stream_play_ended().unwrap();
    assert_eq!(ended.launch_id, far.launch_id);
    assert!(matches!(ended.cause, crate::stream_play::StreamPlayEndCause::SessionEnded));
    assert!(viewer.calls().iter().any(|call| call.action == LinuxViewerAction::Stop));
}

#[tokio::test]
async fn stream_player_lost_device_stops_its_viewer_and_keeps_the_end_cause() {
    use crate::linux_viewer::test_backend::{TestLinuxStreamViewer, LinuxViewerAction};
    let config = UpstreamHostConfig::native("far", "http://127.0.0.1:9".into());
    let key = config.device_public_key.clone().unwrap();
    let viewer = TestLinuxStreamViewer::default();
    let registry = UpstreamRegistry::new(vec![config]).with_linux_viewer(Arc::new(viewer.clone()));
    viewer.start("exact", "far:47989").await.unwrap();
    registry.set_selected(SelectedRemoteSession {
        participation: Participation::StreamPlayer { game_id: Some("game".into()), game_title: Some("Game".into()), device_label: "far".into() },
        route: SelectedRemoteRoute::Native { device_public_key: key },
        launch_id: "exact".into(),
        viewer: Some(SelectedViewer { moonlight_address: "far:47989".into(), confirmed_live: true }),
    });
    registry.session_status().await.unwrap();
    assert!(matches!(registry.stream_play_ended().unwrap().cause, crate::stream_play::StreamPlayEndCause::DeviceUnreachable));
    assert!(!registry.has_selected_session());
    assert!(viewer.calls().iter().any(|call| call.action == LinuxViewerAction::Stop && call.launch_id == "exact"));
    assert_eq!(registry.stream_play_ended().unwrap().device_label, "far");
}

#[tokio::test]
async fn invite_poll_reads_only_same_owner_live_sessions_without_selecting_them() {
    let (url, far) = exact_native_server(false).await;
    let config = UpstreamHostConfig::native("far", url);
    let key = config.device_public_key.clone().unwrap();
    let registry = UpstreamRegistry::new(vec![config]);
    far.set_phase("running");
    assert!(registry.invite_sessions(&BTreeSet::new()).await.unwrap().is_empty());
    let invites = registry.invite_sessions(&BTreeSet::from([key.clone()])).await.unwrap();
    assert_eq!(invites.len(), 1);
    assert_eq!(invites[0].device_public_key, key);
    assert_eq!(invites[0].session.launch_id, far.launch_id);
    assert!(!registry.has_selected_session());
    *far.active_phase.lock().unwrap() = None;
    assert!(registry.invite_sessions(&BTreeSet::from([key])).await.unwrap().is_empty());
}

#[tokio::test]
async fn stream_play_prepare_rejects_unknown_device_labels_and_invalid_peer_origins() {
    use crate::linux_viewer::test_backend::TestLinuxStreamViewer;
    let (url, far) = exact_native_server(false).await;
    let viewer = TestLinuxStreamViewer::default();
    let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("far", url)])
        .with_linux_viewer(Arc::new(viewer.clone()));
    for selector in ["", "unknown", "http://peer/path"] {
        assert!(registry.prepare_stream("game", Some(selector)).await.is_err());
        assert!(!registry.has_selected_session());
        assert!(viewer.calls().is_empty());
        assert!(far.active_phase.lock().unwrap().is_none());
    }
    // Selectors are labels. Endpoint origins are validated separately when
    // the registry resolves that label, before any native launch can run.
    let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native(
        "far", "http://127.0.0.1:9/path".into(),
    )]).with_linux_viewer(Arc::new(viewer.clone()));
    assert!(matches!(registry.prepare_stream("game", Some("far")).await, Err(UpstreamError::Wire(_))));
    assert!(viewer.calls().is_empty());
}
