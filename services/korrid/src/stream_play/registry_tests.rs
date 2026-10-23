#[tokio::test]
async fn stream_player_pause_end_and_local_leave_never_control_the_holder() {
    use crate::linux_viewer::test_backend::{TestLinuxStreamViewer, LinuxViewerAction};
    let (url, far) = exact_native_server(false).await;
    let config = UpstreamHostConfig::native("far", url);
    let key = config.device_public_key.clone().unwrap();
    let viewer = TestLinuxStreamViewer::default();
    let registry = UpstreamRegistry::new(vec![config]).with_linux_viewer(Arc::new(viewer.clone()));
    assert_eq!(registry.stream_play(&key, &far.launch_id).await.unwrap_err().code(), "NoActiveSession");
    far.set_phase("running");
    assert_eq!(registry.stream_play(&key, "wrong").await.unwrap_err().code(), "LaunchMismatch");
    let active = registry.stream_play(&key, &far.launch_id).await.unwrap();
    assert_eq!(active.stream_players.len(), 1);
    far.set_phase("frozen");
    registry.session_status().await.unwrap();
    assert!(viewer.calls().iter().any(|call| call.action == LinuxViewerAction::Background));
    far.set_phase("running");
    registry.session_status().await.unwrap();
    assert!(viewer.calls().iter().any(|call| call.action == LinuxViewerAction::ResumeAndFocus));
    registry.session_freeze(Some(&far.launch_id)).await.unwrap();
    assert_eq!(far.active_phase.lock().unwrap().as_deref(), Some("running"));
    assert!(!registry.has_selected_session());
    registry.stream_play(&key, &far.launch_id).await.unwrap();
    registry.session_stop(Some(&far.launch_id), true).await.unwrap();
    assert_eq!(far.active_phase.lock().unwrap().as_deref(), Some("running"));
    registry.stream_play(&key, &far.launch_id).await.unwrap();
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
