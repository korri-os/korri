//! The device's local portal can select a peer without giving peer calls a
//! second federation hop. Peer catalog and status always describe this holder.
use crate::authorization::AuthorizationContext;
use crate::linux_viewer::LinuxViewerError;
use crate::linux_viewer::{production_viewer, CertificateProvision, LocalSeats};
use crate::*;

/// Holder preparation and local peer selection share one admission fence.
/// The holder's own mutation lock still owns exact launch operations.
pub(crate) async fn serialize_selection<'a>(
    state: &'a AppState,
    request: &RpcRequest,
) -> Option<tokio::sync::MutexGuard<'a, ()>> {
    if matches!(
        request,
        RpcRequest::SessionPrepare(_)
            | RpcRequest::SessionStart(_)
            | RpcRequest::SelectedGameLaunch(_)
            | RpcRequest::InvitesStreamPlay(_)
    ) {
        Some(state.session_selection.lock().await)
    } else {
        None
    }
}

pub(crate) fn registry(
    host: &host::HostRuntime,
    resources: &FederationResources,
) -> upstreams::UpstreamRegistry {
    let registry = match host.route_root() {
        Ok(root) => upstreams::UpstreamRegistry::from_env_or_file(
            &root.join("upstreams.json"),
            resources.credentials.clone(),
        ),
        Err(_) => {
            upstreams::UpstreamRegistry::new_secure(Vec::new(), resources.credentials.clone())
        }
    }
    .with_federation(resources.directory.clone());
    let broker = registry.clone();
    let provision: CertificateProvision = Arc::new(move |host_uuid, certificate| {
        let broker = broker.clone();
        Box::pin(async move {
            broker
                .moonlight_certificate_provision(&host_uuid, &certificate)
                .await
                .map_err(|error| LinuxViewerError::new(error.to_string()))
        })
    });
    let input = host.clone();
    let local_seats: LocalSeats = Arc::new(move || {
        let input = input.clone();
        Box::pin(async move { input.occupied_local_seats().map_err(LinuxViewerError::new) })
    });
    registry.with_linux_viewer(production_viewer(provision, local_seats))
}

pub(crate) fn spawn_watch(peers: upstreams::UpstreamRegistry) {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(std::time::Duration::from_secs(1));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticks.tick().await;
            if peers.has_selected_session() {
                let _ = peers.session_status().await;
            }
        }
    });
}

pub(crate) async fn dispatch(
    state: &AppState,
    authorization: &AuthorizationContext,
    request: &RpcRequest,
) -> Option<RpcResponse> {
    let ServerMode::Host(host) = &state.mode else {
        return None;
    };
    let peers = state.remote.as_ref()?;
    if matches!(
        request,
        RpcRequest::SessionPrepare(_)
            | RpcRequest::SessionStart(_)
            | RpcRequest::SelectedGameLaunch(_)
    ) {
        if peers.has_selected_session() {
            let failure = RpcFailure {
                code: "ActiveSessionConflict".into(),
                message: "This device already has a selected session.".into(),
            };
            let outcome = game_routes::SelectedGameLaunchOutcome::Err(failure);
            return Some(match request {
                RpcRequest::SessionStart(_) => RpcResponse::SessionStart(outcome),
                RpcRequest::SelectedGameLaunch(_) => RpcResponse::SelectedGameLaunch(outcome),
                _ => RpcResponse::SessionPrepare(outcome),
            });
        }
        peers.clear_stream_play_end();
    }
    if !matches!(
        authorization,
        AuthorizationContext::LocalBrowser | AuthorizationContext::LocalUnixControl
    ) {
        return None;
    }
    if let Some(launch_id) = peers.stream_play_launch_id() {
        if let Some(response) = stream_player_control_refusal(&launch_id, request) {
            return Some(response);
        }
    }
    match request {
        RpcRequest::CatalogSnapshot(_) => {
            let local = host
                .catalog_snapshot_for(host.owner_public_key().as_deref())
                .await;
            let far = peers.catalog_snapshot().await;
            let outcome = match (local, far) {
                (Ok(mut local), Ok(mut far)) => {
                    local.games.append(&mut far.games);
                    if let Some(mut failures) = far.failures.take() {
                        local
                            .failures
                            .get_or_insert_with(Vec::new)
                            .append(&mut failures);
                    }
                    CatalogSnapshotOutcome::Ok(local)
                }
                (Ok(mut local), Err(error)) => {
                    local
                        .failures
                        .get_or_insert_with(Vec::new)
                        .push(CatalogHostFailure {
                            host: "Peers".into(),
                            code: error.code().into(),
                            message: error.to_string(),
                        });
                    CatalogSnapshotOutcome::Ok(local)
                }
                (Err(_), Ok(far)) => CatalogSnapshotOutcome::Ok(far),
                (Err(error), Err(_)) => CatalogSnapshotOutcome::Err(error),
            };
            Some(RpcResponse::CatalogSnapshot(outcome))
        }
        RpcRequest::SessionPrepare(request) if request.host.is_some() => {
            if request.host.as_deref() == host.local_label() {
                match peers.has_peer_label(request.host.as_deref().expect("selected label")) {
                    Ok(false) => return None,
                    other => {
                        return Some(RpcResponse::SessionPrepare(
                            game_routes::SelectedGameLaunchOutcome::Err(
                                other
                                    .err()
                                    .map(upstream_failure)
                                    .unwrap_or_else(|| RpcFailure {
                                        code: "UpstreamFailure".into(),
                                        message: "The selected device label is ambiguous.".into(),
                                    }),
                            ),
                        ))
                    }
                }
            }
            let outcome = match host.session_status().await {
                Ok(
                    host::control::HostSessionStatus::NoActive
                    | host::control::HostSessionStatus::Completed { .. }
                    | host::control::HostSessionStatus::Failed { .. },
                ) => peers
                    .prepare_stream(&request.game_id, request.host.as_deref())
                    .await
                    .map_err(upstream_failure),
                Ok(_) => Err(RpcFailure {
                    code: "ActiveSessionConflict".into(),
                    message: "This device already holds a live session.".into(),
                }),
                Err(error) => Err(error),
            };
            Some(RpcResponse::SessionPrepare(
                outcome
                    .map(game_routes::SelectedGameLaunchOutcome::Ok)
                    .unwrap_or_else(game_routes::SelectedGameLaunchOutcome::Err),
            ))
        }
        RpcRequest::SessionStatus(_)
            if peers.has_selected_session() || peers.stream_play_ended().is_some() =>
        {
            let mut outcome = if peers.has_selected_session() {
                session_status_outcome(peers.session_status().await)
            } else {
                SessionStatusOutcome::Ok(SessionStatus {
                    stream_play_launch_id: None,
                    active: None,
                    overlay: None,
                    pending_launches: None,
                    observation_failure: None,
                    stream_play_ended: None,
                })
            };
            if let SessionStatusOutcome::Ok(status) = &mut outcome {
                status.stream_play_launch_id = peers.stream_play_launch_id();
                status.stream_play_ended = peers.stream_play_ended();
            }
            Some(RpcResponse::SessionStatus(outcome))
        }
        RpcRequest::SessionStop(request) if peers.has_selected_session() => {
            Some(RpcResponse::SessionStop(session_stop_outcome(
                peers
                    .session_stop(
                        request.expected_launch_id.as_deref(),
                        request.force.unwrap_or(false),
                    )
                    .await,
            )))
        }
        RpcRequest::SessionFreeze(request) if peers.has_selected_session() => {
            Some(RpcResponse::SessionFreeze(session_freeze_outcome(
                peers
                    .session_freeze(request.expected_launch_id.as_deref())
                    .await,
            )))
        }
        RpcRequest::SessionThaw(request) if peers.has_selected_session() => {
            Some(RpcResponse::SessionThaw(session_freeze_outcome(
                peers
                    .session_thaw(request.expected_launch_id.as_deref())
                    .await,
            )))
        }
        RpcRequest::SessionControls(request) if peers.has_selected_session() => Some(
            RpcResponse::SessionControls(unavailable_remote_session_controls(
                session_status_outcome(peers.session_status().await),
                &request.launch_id,
            )),
        ),
        RpcRequest::SessionControlInvoke(request) if peers.has_selected_session() => Some(
            RpcResponse::SessionControlInvoke(unavailable_remote_session_control(
                session_status_outcome(peers.session_status().await),
                request,
            )),
        ),
        _ => None,
    }
}

/// A local viewer has no plugin controls, even if an old surface retained ids.
pub(crate) fn stream_player_control_refusal(
    launch_id: &str,
    request: &RpcRequest,
) -> Option<RpcResponse> {
    let expected = match request {
        RpcRequest::SessionControls(request) => &request.launch_id,
        RpcRequest::SessionControlInvoke(request) => &request.launch_id,
        _ => return None,
    };
    let failure = SessionControlFailure {
        reason: if expected == launch_id {
            SessionControlFailureReason::NotSessionStarter
        } else {
            SessionControlFailureReason::StaleSession
        },
        message: "A stream player cannot use the holding device's gameplay controls.".into(),
    };
    Some(match request {
        RpcRequest::SessionControls(_) => {
            RpcResponse::SessionControls(SessionControlsOutcome::Err(failure))
        }
        _ => RpcResponse::SessionControlInvoke(SessionControlInvokeOutcome::Err(failure)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        host::control::InMemoryLaunchUnitBackend,
        linux_viewer::test_backend::{LinuxViewerAction, TestLinuxStreamViewer},
    };
    use serde_json::json;

    fn holder(root: &Path, label: &str) -> host::HostRuntime {
        let config = root.join("host.toml");
        std::fs::write(&config, format!("label = \"{label}\"\n[[games]]\nid = \"{label}-game\"\ntitle = \"{label} Game\"\ncommand = [\"game\"]\n")).unwrap();
        let runtime = host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.join("private"),
            Arc::new(InMemoryLaunchUnitBackend::default()),
        );
        runtime.claim_save_owner();
        runtime
    }

    #[tokio::test]
    async fn host_portal_keeps_local_catalog_host_labels_launchable() {
        let root = tempfile::tempdir().unwrap();
        let runtime = holder(root.path(), "local");
        let (mut state, _) = crate::app_states(runtime);
        state.remote = Some(upstreams::UpstreamRegistry::new(Vec::new()));
        let request = serde_json::from_value(
            json!({"_tag":"app.session.prepare","payload":{"gameId":"local-game","host":"local"}}),
        )
        .unwrap();
        let response = crate::dispatch(&state, &AuthorizationContext::LocalBrowser, request)
            .await
            .unwrap();
        assert!(
            matches!(
                response,
                RpcResponse::SessionPrepare(game_routes::SelectedGameLaunchOutcome::Ok(_))
            ),
            "{response:?}"
        );
    }

    #[tokio::test]
    async fn host_portal_lists_peer_games_and_launches_through_the_injected_viewer() {
        let far_root = tempfile::tempdir().unwrap();
        let far = holder(far_root.path(), "far");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = crate::plain_host_routers_for_tests(far.clone()).0;
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let viewer = TestLinuxStreamViewer::default();
        let peers = upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
            "far",
            format!("http://{address}"),
        )])
        .with_linux_viewer(Arc::new(viewer.clone()));
        let local_root = tempfile::tempdir().unwrap();
        let local = holder(local_root.path(), "local");
        let (mut state, _) = crate::app_states(local);
        state.remote = Some(peers);
        let request =
            serde_json::from_value(json!({"_tag":"app.catalog.snapshot","payload":{}})).unwrap();
        let response = crate::dispatch(&state, &AuthorizationContext::LocalBrowser, request)
            .await
            .unwrap();
        let RpcResponse::CatalogSnapshot(CatalogSnapshotOutcome::Ok(catalog)) = response else {
            panic!("catalog failed")
        };
        assert_eq!(catalog.games.len(), 2);
        assert!(catalog
            .games
            .iter()
            .any(|game| game.id == "far-game" && !game.source.is_local));
        let request = serde_json::from_value(
            json!({"_tag":"app.session.prepare","payload":{"gameId":"far-game","host":"far"}}),
        )
        .unwrap();
        let response = crate::dispatch(&state, &AuthorizationContext::LocalBrowser, request)
            .await
            .unwrap();
        let RpcResponse::SessionPrepare(game_routes::SelectedGameLaunchOutcome::Ok(prepared)) =
            response
        else {
            panic!("prepare failed: {response:?}")
        };
        assert!(viewer
            .calls()
            .iter()
            .any(|call| call.action == LinuxViewerAction::Start
                && call.launch_id == prepared.session.launch_id));
        assert!(state.remote.as_ref().unwrap().has_selected_session());
        assert!(matches!(
            far.session_status().await.unwrap(),
            host::control::HostSessionStatus::Running { .. }
                | host::control::HostSessionStatus::FocusFailed { .. }
        ));
        server.abort();
    }
    #[tokio::test]
    async fn simultaneous_local_and_peer_launches_admit_exactly_one_selection() {
        let far_root = tempfile::tempdir().unwrap();
        let far = holder(far_root.path(), "far");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let entered = Arc::new(tokio::sync::Semaphore::new(0));
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (arrived, resume) = (entered.clone(), release.clone());
        let router = crate::plain_host_routers_for_tests(far)
            .0
            .layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let (arrived, resume, gate) = (arrived.clone(), resume.clone(), gate.clone());
                    async move {
                        if gate.swap(false, std::sync::atomic::Ordering::SeqCst) {
                            arrived.add_permits(1);
                            resume.acquire().await.unwrap().forget();
                        }
                        next.run(request).await
                    }
                },
            ));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let peers = upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
            "far",
            format!("http://{address}"),
        )]);
        let local_root = tempfile::tempdir().unwrap();
        let (mut state, _) = crate::app_states(holder(local_root.path(), "local"));
        state.remote = Some(peers);
        let remote_state = state.clone();
        let peer_state = state.clone();
        let remote = tokio::spawn(async move {
            crate::dispatch(&remote_state, &AuthorizationContext::LocalBrowser,
            serde_json::from_value(json!({"_tag":"app.session.prepare", "payload":{"gameId":"far-game","host":"far"}})).unwrap()).await.unwrap()
        });
        entered.acquire().await.unwrap().forget();
        let mut local = tokio::spawn(async move {
            crate::dispatch(&state, &AuthorizationContext::LocalBrowser,
            serde_json::from_value(json!({"_tag":"app.session.prepare", "payload":{"gameId":"local-game","host":"local"}})).unwrap()).await.unwrap()
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut local)
                .await
                .is_err()
        );
        release.add_permits(1);
        assert!(matches!(
            remote.await.unwrap(),
            RpcResponse::SessionPrepare(game_routes::SelectedGameLaunchOutcome::Ok(_))
        ));
        assert!(
            matches!(local.await.unwrap(), RpcResponse::SessionPrepare(game_routes::SelectedGameLaunchOutcome::Err(RpcFailure { code, .. })) if code == "ActiveSessionConflict")
        );
        let incoming = AuthorizationContext::Peer(authorization::Principal::OwnerDevice {
            device_public_key: "33".repeat(32),
            owner_public_key: "44".repeat(32),
            owner_statement: "verified".into(),
        });
        let response = crate::dispatch(
            &peer_state,
            &incoming,
            serde_json::from_value(json!({
                "_tag":"app.session.prepare", "payload":{"gameId":"local-game"}
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        assert!(
            matches!(response, RpcResponse::SessionPrepare(game_routes::SelectedGameLaunchOutcome::Err(RpcFailure { code, .. })) if code == "ActiveSessionConflict")
        );
        server.abort();
    }

    #[tokio::test]
    async fn idle_host_does_not_adopt_an_unselected_peers_live_session() {
        // Merely observing the directory is not selecting a session.
        let far_root = tempfile::tempdir().unwrap();
        let far = holder(far_root.path(), "far");
        let prepared = far.prepare("far-game", None).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let far_router = crate::plain_host_routers_for_tests(far).0;
        let server = tokio::spawn(async move { axum::serve(listener, far_router).await.unwrap() });
        let config = upstreams::UpstreamHostConfig::native("far", format!("http://{address}"));
        let peers = upstreams::UpstreamRegistry::new(vec![config]);
        assert!(peers.stream_play_launch_id().is_none());
        let local_root = tempfile::tempdir().unwrap();
        let (mut state, _) = crate::app_states(holder(local_root.path(), "local"));
        state.remote = Some(peers);
        let response = crate::dispatch(
            &state,
            &AuthorizationContext::LocalBrowser,
            serde_json::from_value(json!({"_tag":"app.session.status","payload":{}})).unwrap(),
        )
        .await
        .unwrap();
        assert!(
            matches!(response, RpcResponse::SessionStatus(SessionStatusOutcome::Err(RpcFailure { code, .. })) if code == "NoActiveSession")
        );
        assert_ne!(prepared.session.launch_id, "");
        server.abort();
    }
}
