//! The device's local portal can select a peer without giving peer calls a
//! second federation hop. Peer catalog and status always describe this holder.
use crate::authorization::AuthorizationContext;
use crate::*;

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
    // Production viewer composition is completed with the occupied-local-seat
    // read. Tests retain the registry's injected TestLinuxStreamViewer.
    registry
}

pub(crate) async fn dispatch(
    state: &AppState,
    authorization: &AuthorizationContext,
    request: &RpcRequest,
) -> Option<RpcResponse> {
    if !matches!(
        authorization,
        AuthorizationContext::LocalBrowser | AuthorizationContext::LocalUnixControl
    ) {
        return None;
    }
    let ServerMode::Host(host) = &state.mode else {
        return None;
    };
    let peers = state.remote.as_ref()?;
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
        RpcRequest::SessionStatus(_) if peers.has_selected_session() => Some(
            RpcResponse::SessionStatus(session_status_outcome(peers.session_status().await)),
        ),
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
}
