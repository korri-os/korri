//! The device's local shell can select a peer without giving peer calls a
//! second federation hop. Peer catalog and status always describe this holder.
use crate::authorization::AuthorizationContext;
use crate::linux_viewer::LinuxViewerError;
use crate::linux_viewer::{installed_viewer, CertificateProvision, LocalSeats};
use crate::*;

/// Unreserved preparation and peer selection share one admission fence.
/// Reserved starts take it in their native worker, after cancellable routing.
pub(crate) async fn serialize_selection(
    state: &AppState,
    request: &RpcRequest,
) -> Option<tokio::sync::OwnedMutexGuard<()>> {
    if matches!(
        request,
        RpcRequest::SessionPrepare(_)
            | RpcRequest::SelectedGameLaunch(_)
            | RpcRequest::InvitesNetplay(_)
            | RpcRequest::InvitesStreamPlay(_)
    ) {
        Some(state.session_selection.clone().lock_owned().await)
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
    let settings_host = host.clone();
    let settings_registry = registry.clone();
    let settings: crate::linux_viewer::ViewerSettings = Arc::new(move |plugin, display| {
        let root = settings_host
            .route_root()
            .map_err(|e| LinuxViewerError::new(e.message))?;
        let plugins = settings_host
            .route_registry()
            .map_err(|e| LinuxViewerError::new(e.message))?;
        crate::linux_viewer::render_settings(
            root,
            &plugins,
            plugin.runner(),
            settings_registry.stream_play_game_id().as_deref(),
            display,
        )
    });
    let selection = registry.clone();
    let viewer = host.viewer_with_local_input(
        installed_viewer(provision, local_seats, settings),
        Arc::new(move || selection.has_selected_session()),
    );
    registry.with_linux_viewer(viewer)
}

/// Polls the selected streamed session so its end stops this device's viewer.
pub(crate) fn spawn_watch(
    peers: upstreams::UpstreamRegistry,
    on_failure: impl Fn(String) + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(std::time::Duration::from_secs(1));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let signal = peers.local_viewer().change_signal();
        loop {
            tokio::select! {
                _ = ticks.tick() => {},
                _ = async {
                    if let Some(signal) = &signal { signal.notified().await; }
                    else { std::future::pending::<()>().await; }
                } => {},
            }
            if peers.has_selected_session() {
                if let Err(error) = peers.session_status().await {
                    if error.code() == "HostLaunchFailed" {
                        on_failure(error.to_string());
                    }
                }
            }
        }
    })
}

/// The launch settings and runner choices of a streamed game live on the
/// device that runs it (ADR 0066). A local caller names that holder in `host`;
/// this korrid asks it and returns its answer. None: the request names no
/// holder, or is not one of these calls.
pub(crate) async fn ask_holder(
    peers: &upstreams::UpstreamRegistry,
    request: &RpcRequest,
) -> Option<RpcResponse> {
    use crate::launch_settings::{LaunchSettingsSnapshotOutcome, LaunchSettingsWriteOutcome};
    macro_rules! forward {
        ($variant:ident, $asked:expr, $refused:path) => {{
            let holder = $asked.holder.clone()?;
            let mut asked = $asked.clone();
            asked.holder = None;
            Some(
                match peers
                    .holder_call(&holder, RpcRequest::$variant(asked))
                    .await
                {
                    Ok(response @ RpcResponse::$variant(_)) => response,
                    Ok(_) => RpcResponse::$variant($refused(RpcFailure {
                        code: "UpstreamWire".into(),
                        message: format!("{holder} answered a different request"),
                    })),
                    // The holder refused this device, not the request: only a
                    // device of its owner may read or change its settings.
                    Err(upstreams::UpstreamError::Http(403)) => {
                        RpcResponse::$variant($refused(RpcFailure {
                            code: "PermissionDenied".into(),
                            message: format!(
                                "This device cannot change the settings of {holder}. Only a device of its owner can."
                            ),
                        }))
                    }
                    Err(error) => RpcResponse::$variant($refused(upstream_failure(error))),
                },
            )
        }};
    }
    match request {
        RpcRequest::LaunchSettingsSnapshot(asked) => forward!(
            LaunchSettingsSnapshot,
            asked,
            LaunchSettingsSnapshotOutcome::Err
        ),
        RpcRequest::LaunchSettingsSet(asked) => {
            forward!(LaunchSettingsSet, asked, LaunchSettingsWriteOutcome::Err)
        }
        RpcRequest::LaunchProfileSave(asked) => {
            forward!(LaunchProfileSave, asked, LaunchSettingsWriteOutcome::Err)
        }
        RpcRequest::LaunchProfileRemove(asked) => {
            forward!(LaunchProfileRemove, asked, LaunchSettingsWriteOutcome::Err)
        }
        RpcRequest::LaunchProfileSelect(asked) => {
            forward!(LaunchProfileSelect, asked, LaunchSettingsWriteOutcome::Err)
        }
        RpcRequest::GameRoutes(asked) => {
            forward!(GameRoutes, asked, game_routes::GameRoutesOutcome::Err)
        }
        _ => None,
    }
}

/// A local caller, not a peer: only it may send this korrid to another device.
pub(crate) fn is_local_caller(authorization: &AuthorizationContext) -> bool {
    matches!(
        authorization,
        AuthorizationContext::LocalSurfaceHost
            | AuthorizationContext::LocalUnixControl
            | AuthorizationContext::LocalCli
    )
}

/// The label a request names when it is another device's. This device's own
/// label names no holder, unless a peer carries the same label.
fn named_holder<'a>(
    host: &host::HostRuntime,
    peers: &upstreams::UpstreamRegistry,
    label: Option<&'a str>,
) -> Option<&'a str> {
    let label = label?;
    if Some(label) == host.local_label() && matches!(peers.has_peer_label(label), Ok(false)) {
        return None;
    }
    Some(label)
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
    if !is_local_caller(authorization) {
        return None;
    }
    if let Some(launch_id) = peers.stream_play_launch_id() {
        if let Some(response) = stream_player_control_refusal(&launch_id, request) {
            return Some(response);
        }
    }
    let holder_of_request = match request {
        RpcRequest::LaunchSettingsSnapshot(asked) => asked.holder.as_deref(),
        RpcRequest::LaunchSettingsSet(asked) => asked.holder.as_deref(),
        RpcRequest::LaunchProfileSave(asked) => asked.holder.as_deref(),
        RpcRequest::LaunchProfileRemove(asked) => asked.holder.as_deref(),
        RpcRequest::LaunchProfileSelect(asked) => asked.holder.as_deref(),
        RpcRequest::GameRoutes(asked) => asked.holder.as_deref(),
        _ => None,
    };
    if named_holder(host, peers, holder_of_request).is_some() {
        return ask_holder(peers, request).await;
    }
    match request {
        RpcRequest::CatalogSnapshot(_) => {
            let local = host
                .catalog_snapshot_for(host.owner_public_key().as_deref())
                .await;
            let streamed = peers.catalog_snapshot().await;
            let outcome = match (local, streamed) {
                (Ok(mut local), Ok(mut streamed)) => {
                    local.games.append(&mut streamed.games);
                    if let Some(mut failures) = streamed.failures.take() {
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
                (Err(_), Ok(streamed)) => CatalogSnapshotOutcome::Ok(streamed),
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
                    .prepare_streamed_launch(
                        &request.game_id,
                        request.host.as_deref(),
                        upstreams::StreamedLaunch {
                            runner_id: None,
                            launch: request.launch.clone(),
                        },
                    )
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
        // The runner the person chose for a streamed game (issue #401): the
        // holder's owner-device launch, with this device's viewer lifecycle.
        RpcRequest::SelectedGameLaunch(request)
            if named_holder(host, peers, request.holder.as_deref()).is_some() =>
        {
            let outcome = match host.session_status().await {
                Ok(
                    host::control::HostSessionStatus::NoActive
                    | host::control::HostSessionStatus::Completed { .. }
                    | host::control::HostSessionStatus::Failed { .. },
                ) => peers
                    .prepare_streamed_launch(
                        &request.game_id,
                        request.holder.as_deref(),
                        upstreams::StreamedLaunch {
                            runner_id: Some(request.runner_id.clone()),
                            launch: request.launch.clone(),
                        },
                    )
                    .await
                    .map_err(upstream_failure),
                Ok(_) => Err(RpcFailure {
                    code: "ActiveSessionConflict".into(),
                    message: "This device already holds a live session.".into(),
                }),
                Err(error) => Err(error),
            };
            Some(RpcResponse::SelectedGameLaunch(
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
                    netplay: None,
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
        RpcRequest::SessionEnd(request) if peers.has_selected_session() => {
            Some(RpcResponse::SessionEnd(session_stop_outcome(
                peers
                    .session_stop(
                        request.expected_launch_id.as_deref(),
                        request.force.unwrap_or(false),
                    )
                    .await,
            )))
        }
        RpcRequest::SessionLeave(request) if peers.has_selected_session() => {
            Some(RpcResponse::SessionLeave(session_freeze_outcome(
                peers
                    .session_freeze(request.expected_launch_id.as_deref())
                    .await,
            )))
        }
        RpcRequest::SessionResume(request) if peers.has_selected_session() => {
            Some(RpcResponse::SessionResume(session_freeze_outcome(
                peers
                    .session_thaw(request.expected_launch_id.as_deref())
                    .await,
            )))
        }
        RpcRequest::SessionControls(request) if peers.has_selected_session() => Some(
            RpcResponse::SessionControls(peers.session_controls(&request.launch_id).await),
        ),
        RpcRequest::SessionControlInvoke(request) if peers.has_selected_session() => Some(
            RpcResponse::SessionControlInvoke(peers.invoke_session_control(request.clone()).await),
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
#[path = "gameplay_controls_tests.rs"]
mod gameplay_controls_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        host::control::InMemoryLaunchUnitBackend,
        linux_viewer::test_backend::{LinuxViewerAction, TestLinuxStreamViewer},
    };
    use serde_json::json;

    fn host_runtime(root: &Path, label: &str) -> host::HostRuntime {
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
    async fn host_shell_keeps_local_catalog_host_labels_launchable() {
        let root = tempfile::tempdir().unwrap();
        let runtime = host_runtime(root.path(), "local");
        let (mut state, _) = crate::app_states(runtime);
        state.remote = Some(upstreams::UpstreamRegistry::new(Vec::new()));
        let request = serde_json::from_value(
            json!({"_tag":"app.session.prepare","payload":{"gameId":"local-game","host":"local"}}),
        )
        .unwrap();
        let response = crate::dispatch(&state, &AuthorizationContext::LocalSurfaceHost, request)
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
    async fn host_shell_lists_peer_games_and_launches_through_the_injected_viewer() {
        let holder_root = tempfile::tempdir().unwrap();
        let holder = host_runtime(holder_root.path(), "holder");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = crate::plain_host_routers_for_tests(holder.clone()).0;
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let viewer = TestLinuxStreamViewer::default();
        let peers = upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
            "holder",
            format!("http://{address}"),
        )])
        .with_linux_viewer(Arc::new(viewer.clone()));
        let local_root = tempfile::tempdir().unwrap();
        let local = host_runtime(local_root.path(), "local");
        let (mut state, _) = crate::app_states(local);
        state.remote = Some(peers);
        let request =
            serde_json::from_value(json!({"_tag":"app.catalog.snapshot","payload":{}})).unwrap();
        let response = crate::dispatch(&state, &AuthorizationContext::LocalSurfaceHost, request)
            .await
            .unwrap();
        let RpcResponse::CatalogSnapshot(CatalogSnapshotOutcome::Ok(catalog)) = response else {
            panic!("catalog failed")
        };
        assert_eq!(catalog.games.len(), 2);
        assert!(catalog
            .games
            .iter()
            .any(|game| game.id == "holder-game" && !game.source.is_local));
        let request = serde_json::from_value(
            json!({"_tag":"app.session.prepare","payload":{"gameId":"holder-game","host":"holder"}}),
        )
        .unwrap();
        let response = crate::dispatch(&state, &AuthorizationContext::LocalSurfaceHost, request)
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
        let controls = crate::dispatch(
            &state,
            &AuthorizationContext::LocalSurfaceHost,
            RpcRequest::SessionControls(SessionControlsRequest {
                launch_id: prepared.session.launch_id.clone(),
            }),
        )
        .await
        .unwrap();
        assert!(
            matches!(controls, RpcResponse::SessionControls(SessionControlsOutcome::Ok(ref controls)) if controls.groups.is_empty()),
            "a game without plugin controls must report an empty list: {controls:?}"
        );
        assert!(matches!(
            holder.session_status().await.unwrap(),
            host::control::HostSessionStatus::Running { .. }
                | host::control::HostSessionStatus::FocusFailed { .. }
        ));
        server.abort();
    }
    /// A holder with an installed library. Its device keeps its own CPU limit.
    fn library_holder(root: &Path) -> host::HostRuntime {
        std::fs::write(root.join("device.yaml"), "host: {cpu: {max: 77}}\n").unwrap();
        let config = root.join("host.toml");
        std::fs::write(&config, "label = \"holder\"\ngames = []\n").unwrap();
        let runtime = host::HostRuntime::from_paths_with_backend(
            &config,
            Some(root.into()),
            root.join("private"),
            Arc::new(InMemoryLaunchUnitBackend::default()),
        )
        .with_route_registry(
            root.into(),
            crate::plugin::PluginRegistry::from_installed(Vec::new()).unwrap(),
        );
        runtime.claim_save_owner();
        runtime
    }

    /// This device with one native peer named "holder".
    async fn playing_device(
        holder: host::HostRuntime,
        local_root: &Path,
    ) -> (AppState, TestLinuxStreamViewer, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = crate::plain_host_routers_for_tests(holder).0;
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let viewer = TestLinuxStreamViewer::default();
        let peers = upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
            "holder",
            format!("http://{address}"),
        )])
        .with_linux_viewer(Arc::new(viewer.clone()));
        let (mut state, _) = crate::app_states(host_runtime(local_root, "local"));
        state.remote = Some(peers);
        (state, viewer, server)
    }

    async fn ask(
        state: &AppState,
        who: &AuthorizationContext,
        request: serde_json::Value,
    ) -> serde_json::Value {
        let response = crate::dispatch(state, who, serde_json::from_value(request).unwrap())
            .await
            .unwrap();
        serde_json::to_value(response).unwrap()["outcome"].clone()
    }

    #[tokio::test]
    async fn launch_settings_that_name_a_holder_are_read_from_and_written_to_that_device() {
        let holder_root = tempfile::tempdir().unwrap();
        let local_root = tempfile::tempdir().unwrap();
        let (state, _, server) =
            playing_device(library_holder(holder_root.path()), local_root.path()).await;
        let shell = AuthorizationContext::LocalSurfaceHost;
        // Read: the holder's own values, not this device's.
        let read = ask(
            &state,
            &shell,
            json!({"_tag":"app.launch-settings.snapshot","payload":{"holder":"holder"}}),
        )
        .await;
        assert_eq!(read["_tag"], "Ok", "{read}");
        assert_eq!(
            read["payload"]["resolved"]["values"]["cpu.max"]["value"],
            77
        );
        let own = ask(
            &state,
            &shell,
            json!({"_tag":"app.launch-settings.snapshot","payload":{}}),
        )
        .await;
        assert_ne!(own["payload"]["resolved"]["values"]["cpu.max"]["value"], 77);
        // Write: on the holder, from the revision it gave.
        let written = ask(
            &state,
            &shell,
            json!({"_tag":"app.launch-settings.set","payload":{
                "holder":"holder",
                "place":{"layer":{"_tag":"Device"},"scope":{"_tag":"Any"}},
                "path":{"_tag":"CpuMax"},
                "value":55,
                "expectedRevision":read["payload"]["revisions"]["device"],
            }}),
        )
        .await;
        assert_eq!(written["_tag"], "Ok", "{written}");
        assert!(
            std::fs::read_to_string(holder_root.path().join("device.yaml"))
                .unwrap()
                .contains("max: 55")
        );
        assert!(!local_root.path().join("device.yaml").exists());
        // The holder's runners are asked of the holder too (issue #401).
        let routes = ask(
            &state,
            &shell,
            json!({"_tag":"app.local-games.routes","payload":{"gameId":"absent","holder":"holder"}}),
        )
        .await;
        assert_eq!(routes["_tag"], "Err");
        assert_ne!(
            routes["payload"]["code"], "OperationUnsupported",
            "{routes}"
        );
        // A peer cannot send this korrid on to another device: it gets this
        // device's own answer.
        let peer = AuthorizationContext::Peer(crate::authorization::Principal::OwnerDevice {
            device_public_key: "d".repeat(64),
            owner_statement: String::new(),
            owner_public_key: "o".repeat(64),
        });
        let hop = ask(
            &state,
            &peer,
            json!({"_tag":"app.launch-settings.snapshot","payload":{"holder":"holder"}}),
        )
        .await;
        assert_ne!(hop["payload"]["resolved"]["values"]["cpu.max"]["value"], 55);
        // A holder that does not answer is said to be unreachable.
        server.abort();
        let _ = server.await;
        let gone = ask(
            &state,
            &shell,
            json!({"_tag":"app.launch-settings.snapshot","payload":{"holder":"holder"}}),
        )
        .await;
        assert_eq!(gone["_tag"], "Err");
        assert_eq!(gone["payload"]["code"], "UpstreamUnreachable", "{gone}");
    }

    /// A device that only plays, as the phone's korrid: brain mode with one
    /// native peer named "holder".
    async fn playing_brain(
        holder: host::HostRuntime,
        local_root: &Path,
    ) -> (AppState, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = crate::plain_host_routers_for_tests(holder).0;
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let peers = upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
            "holder",
            format!("http://{address}"),
        )])
        .with_linux_viewer(Arc::new(TestLinuxStreamViewer::default()));
        let (state, _) = crate::brain_app_state(
            "brain-test-capability",
            "http://127.0.0.1:8099",
            local_root,
            local_root.join("private"),
            crate::plugin_policy::RegistrySource::Selected(Arc::new(
                crate::plugin::PluginRegistry::from_installed(Vec::new()).unwrap(),
            )),
            crate::config::snapshot::ConfigSnapshotCoordinator::new(local_root),
            crate::discovery::FolderSelectionGrantStore::default(),
            Some(peers),
            None,
            None,
        );
        (state, server)
    }

    #[tokio::test]
    async fn a_device_that_only_plays_reaches_the_holder_for_a_streamed_games_settings() {
        let holder_root = tempfile::tempdir().unwrap();
        let local_root = tempfile::tempdir().unwrap();
        let (state, server) =
            playing_brain(library_holder(holder_root.path()), local_root.path()).await;
        let shell = AuthorizationContext::LocalSurfaceHost;
        let read = ask(
            &state,
            &shell,
            json!({"_tag":"app.launch-settings.snapshot","payload":{"holder":"holder"}}),
        )
        .await;
        assert_eq!(read["_tag"], "Ok", "{read}");
        assert_eq!(
            read["payload"]["resolved"]["values"]["cpu.max"]["value"],
            77
        );
        let written = ask(
            &state,
            &shell,
            json!({"_tag":"app.launch-settings.set","payload":{
                "holder":"holder",
                "place":{"layer":{"_tag":"Device"},"scope":{"_tag":"Any"}},
                "path":{"_tag":"CpuMax"},
                "value":55,
                "expectedRevision":read["payload"]["revisions"]["device"],
            }}),
        )
        .await;
        assert_eq!(written["_tag"], "Ok", "{written}");
        assert!(
            std::fs::read_to_string(holder_root.path().join("device.yaml"))
                .unwrap()
                .contains("max: 55")
        );
        for (call, payload) in [
            (
                "app.local-games.routes",
                json!({"gameId":"absent","holder":"holder"}),
            ),
            (
                "app.launch-settings.profile.select",
                json!({"gameId":"absent","holder":"holder","expectedRevision":"r"}),
            ),
        ] {
            let asked = ask(&state, &shell, json!({"_tag":call,"payload":payload})).await;
            assert_ne!(
                asked["payload"]["code"], "OperationUnsupported",
                "{call}: {asked}"
            );
        }
        // Without a holder the call is about this device, which has no
        // installed Linux routes: the refusal it always gave.
        let own = ask(
            &state,
            &shell,
            json!({"_tag":"app.launch-settings.set","payload":{
                "place":{"layer":{"_tag":"Device"},"scope":{"_tag":"Any"}},
                "path":{"_tag":"CpuMax"},
                "value":55,
                "expectedRevision":"r",
            }}),
        )
        .await;
        assert_eq!(own["payload"]["code"], "OperationUnsupported", "{own}");
        // A peer never sends this korrid on to another device.
        let peer = AuthorizationContext::Peer(crate::authorization::Principal::OwnerDevice {
            device_public_key: "d".repeat(64),
            owner_statement: String::new(),
            owner_public_key: "o".repeat(64),
        });
        let hop = ask(
            &state,
            &peer,
            json!({"_tag":"app.launch-settings.snapshot","payload":{"holder":"holder"}}),
        )
        .await;
        assert_eq!(hop["payload"]["code"], "OperationUnsupported", "{hop}");
        // The chosen runner travels with the streamed launch (issue #401).
        let launched = ask(
            &state,
            &shell,
            json!({"_tag":"app.local-games.launch.selected","payload":{
                "gameId":"absent","holder":"holder","runnerId":"@test:runner/main"}}),
        )
        .await;
        assert_eq!(launched["_tag"], "Err");
        assert_ne!(
            launched["payload"]["code"], "OperationUnsupported",
            "{launched}"
        );
        server.abort();
    }

    #[tokio::test]
    async fn a_holder_that_refuses_this_device_says_only_its_owners_devices_may_change_it() {
        // A holder that does not take this device as one of its owner's.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = axum::Router::new().route(
            "/rpc",
            axum::routing::post(|| async { axum::http::StatusCode::FORBIDDEN }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let peers = upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
            "holder",
            format!("http://{address}"),
        )]);
        let request = serde_json::from_value(
            json!({"_tag":"app.launch-settings.snapshot","payload":{"gameId":"g","holder":"holder"}}),
        )
        .unwrap();
        let response = ask_holder(&peers, &request)
            .await
            .expect("a holder is named");
        let refused = serde_json::to_value(response).unwrap()["outcome"]["payload"].clone();
        assert_eq!(refused["code"], "PermissionDenied", "{refused}");
        assert_eq!(
            refused["message"],
            "This device cannot change the settings of holder. Only a device of its owner can."
        );
        // A request that names no holder is not this function's to answer.
        let own = serde_json::from_value(
            json!({"_tag":"app.launch-settings.snapshot","payload":{"gameId":"g"}}),
        )
        .unwrap();
        assert!(ask_holder(&peers, &own).await.is_none());
        server.abort();
    }

    #[tokio::test]
    async fn a_streamed_launch_carries_its_values_and_its_chosen_runner_to_the_holder() {
        let holder_root = tempfile::tempdir().unwrap();
        let local_root = tempfile::tempdir().unwrap();
        let holder = host_runtime(holder_root.path(), "holder");
        let (state, viewer, server) = playing_device(holder.clone(), local_root.path()).await;
        let shell = AuthorizationContext::LocalSurfaceHost;
        // The fixture holder runs a host command game, which takes no values
        // and no runner choice. Its refusal proves both reached its launch.
        for request in [
            json!({"_tag":"app.session.prepare","payload":{
                "gameId":"holder-game","host":"holder","launch":{"cpu":{"max":50}}}}),
            json!({"_tag":"app.local-games.launch.selected","payload":{
                "gameId":"holder-game","holder":"holder","runnerId":"@test:runner/main"}}),
        ] {
            let refused = ask(&state, &shell, request).await;
            assert_eq!(refused["_tag"], "Err", "{refused}");
            assert!(
                refused["payload"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("installed runner choices or overrides"),
                "{refused}"
            );
        }
        assert!(
            viewer.calls().is_empty(),
            "a refused launch starts no viewer"
        );
        assert!(!state.remote.as_ref().unwrap().has_selected_session());
        // The same launch without values starts, as before.
        let started = ask(
            &state,
            &shell,
            json!({"_tag":"app.session.prepare","payload":{"gameId":"holder-game","host":"holder"}}),
        )
        .await;
        assert_eq!(started["_tag"], "Ok", "{started}");
        // A device that is not the owner's cannot send values to this holder.
        let (holder_state, _) = crate::app_states(holder);
        let pass = AuthorizationContext::Peer(crate::authorization::Principal::Household {
            device_public_key: "d".repeat(64),
            owner_statement: String::new(),
            person_pass: crate::authorization::PersonPass {
                event_id: String::new(),
                event_json: String::new(),
                expires_at: u64::MAX,
                scopes: vec![crate::authorization::Scope::StreamLaunch],
                person_public_key: "p".repeat(64),
            },
        });
        let with_values: RpcRequest =
            serde_json::from_value(json!({"_tag":"app.session.prepare","payload":{
            "gameId":"holder-game","launch":{"cpu":{"max":50}}}}))
            .unwrap();
        assert!(crate::dispatch(&holder_state, &pass, with_values)
            .await
            .is_err());
        server.abort();
    }

    #[tokio::test]
    async fn simultaneous_local_and_peer_launches_admit_exactly_one_selection() {
        let holder_root = tempfile::tempdir().unwrap();
        let holder = host_runtime(holder_root.path(), "holder");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let entered = Arc::new(tokio::sync::Semaphore::new(0));
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (arrived, resume) = (entered.clone(), release.clone());
        let router =
            crate::plain_host_routers_for_tests(holder)
                .0
                .layer(axum::middleware::from_fn(
                    move |request: axum::extract::Request, next: axum::middleware::Next| {
                        let (arrived, resume, gate) =
                            (arrived.clone(), resume.clone(), gate.clone());
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
            "holder",
            format!("http://{address}"),
        )]);
        let local_root = tempfile::tempdir().unwrap();
        let (mut state, _) = crate::app_states(host_runtime(local_root.path(), "local"));
        state.remote = Some(peers);
        let remote_state = state.clone();
        let peer_state = state.clone();
        let remote = tokio::spawn(async move {
            crate::dispatch(&remote_state, &AuthorizationContext::LocalSurfaceHost,
            serde_json::from_value(json!({"_tag":"app.session.prepare", "payload":{"gameId":"holder-game","host":"holder"}})).unwrap()).await.unwrap()
        });
        entered.acquire().await.unwrap().forget();
        let mut local = tokio::spawn(async move {
            crate::dispatch(&state, &AuthorizationContext::LocalSurfaceHost,
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
        let holder_root = tempfile::tempdir().unwrap();
        let holder = host_runtime(holder_root.path(), "holder");
        let prepared = holder.prepare("holder-game", None, None).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let holder_router = crate::plain_host_routers_for_tests(holder).0;
        let server =
            tokio::spawn(async move { axum::serve(listener, holder_router).await.unwrap() });
        let config = upstreams::UpstreamHostConfig::native("holder", format!("http://{address}"));
        let peers = upstreams::UpstreamRegistry::new(vec![config]);
        assert!(peers.stream_play_launch_id().is_none());
        let local_root = tempfile::tempdir().unwrap();
        let (mut state, _) = crate::app_states(host_runtime(local_root.path(), "local"));
        state.remote = Some(peers);
        let response = crate::dispatch(
            &state,
            &AuthorizationContext::LocalSurfaceHost,
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

    #[derive(Debug)]
    struct SelectedSessionFocus;
    impl crate::host::CompositorControl for SelectedSessionFocus {
        fn tree(&self) -> Result<String, String> {
            Ok(r#"{"nodes":[{"id":2,"pid":9100,"focused":true}]}"#.into())
        }
        fn focus(&self, _: i64) -> Result<(), String> {
            Ok(())
        }
    }

    struct SelectedSessionFixture {
        state: AppState,
        holder: host::HostRuntime,
        viewer: TestLinuxStreamViewer,
        units: Arc<InMemoryLaunchUnitBackend>,
        launch: String,
        server: tokio::task::JoinHandle<()>,
        _roots: Vec<tempfile::TempDir>,
    }
    impl Drop for SelectedSessionFixture {
        fn drop(&mut self) {
            self.server.abort();
        }
    }
    impl SelectedSessionFixture {
        async fn new() -> Self {
            let holder_root = tempfile::tempdir().unwrap();
            let config = holder_root.path().join("host.toml");
            std::fs::write(&config, "label = \"holder\"\n[[games]]\nid = \"streamed-game\"\ntitle = \"Streamed Game\"\ncommand = [\"game\"]\n").unwrap();
            let units = Arc::new(InMemoryLaunchUnitBackend::default());
            let holder = host::HostRuntime::from_paths_with_backend(
                &config,
                None,
                holder_root.path().join("private"),
                units.clone(),
            )
            .with_compositor(Arc::new(SelectedSessionFocus));
            holder.claim_save_owner();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let router = crate::plain_host_routers_for_tests(holder.clone()).0;
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let viewer = TestLinuxStreamViewer::default();
            let peers =
                upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
                    "holder",
                    format!("http://{address}"),
                )])
                .with_linux_viewer(Arc::new(viewer.clone()));
            let local_root = tempfile::tempdir().unwrap();
            let (mut state, _) = crate::app_states(host_runtime(local_root.path(), "local"));
            state.remote = Some(peers);
            let response = crate::dispatch(
                &state,
                &AuthorizationContext::LocalSurfaceHost,
                RpcRequest::SessionPrepare(SessionPrepareRequest {
                    viewer_display: None,
                    game_id: "streamed-game".into(),
                    host: Some("holder".into()),
                    launch: None,
                }),
            )
            .await
            .unwrap();
            let RpcResponse::SessionPrepare(game_routes::SelectedGameLaunchOutcome::Ok(prepared)) =
                response
            else {
                panic!("selected preparation failed: {response:?}")
            };
            let launch = prepared.session.launch_id;
            units.set_window_pids(&launch, [9100].into());
            Self {
                state,
                holder,
                viewer,
                units,
                launch,
                server,
                _roots: vec![holder_root, local_root],
            }
        }
        async fn call(
            &self,
            context: &AuthorizationContext,
            tag: &str,
            expected: Option<&str>,
        ) -> serde_json::Value {
            let payload = expected
                .map(|id| json!({"expectedLaunchId":id}))
                .unwrap_or_else(|| json!({}));
            let response = crate::dispatch(
                &self.state,
                context,
                serde_json::from_value(json!({"_tag":tag,"payload":payload})).unwrap(),
            )
            .await
            .unwrap();
            serde_json::to_value(response).unwrap()
        }
    }

    #[tokio::test]
    async fn completed_holder_resume_clears_selection_through_native_rpc() {
        let f = SelectedSessionFixture::new().await;
        f.viewer
            .seed(&f.launch, crate::linux_viewer::LinuxViewerStatus::Stopped);
        f.units.complete_live();
        // Exercise the real holder's terminal status encoding, not the peer fixture.
        assert!(
            matches!(crate::host_session_status_outcome(f.holder.session_status().await),
            SessionStatusOutcome::Err(RpcFailure { code, .. }) if code == "SessionCompleted")
        );
        let response = f
            .call(
                &AuthorizationContext::LocalSurfaceHost,
                "app.session.resume",
                Some(&f.launch),
            )
            .await;
        assert_eq!(
            response["outcome"]["payload"]["code"], "NoActiveSession",
            "{response}"
        );
        assert!(!f.state.remote.as_ref().unwrap().has_selected_session());
        let response = f
            .call(
                &AuthorizationContext::LocalSurfaceHost,
                "app.session.status",
                None,
            )
            .await;
        assert!(
            response["outcome"]["payload"]["active"].is_null(),
            "{response}"
        );
    }

    #[tokio::test]
    async fn selected_stream_local_cli_has_surface_host_and_inputd_status_control_parity() {
        for context in [
            AuthorizationContext::LocalSurfaceHost,
            AuthorizationContext::LocalUnixControl,
            AuthorizationContext::LocalCli,
        ] {
            let f = SelectedSessionFixture::new().await;
            let status = f.call(&context, "app.session.status", None).await;
            assert_eq!(status["outcome"]["_tag"], "Ok", "{context:?}: {status}");
            assert_eq!(status["outcome"]["payload"]["active"]["launchId"], f.launch);
            assert_eq!(
                status["outcome"]["payload"]["active"]["gameId"],
                "streamed-game"
            );
            let before = f.viewer.calls();
            for tag in ["app.session.leave", "app.session.resume", "app.session.end"] {
                for (expected, code) in [
                    (None, "ExpectedLaunchIdRequired"),
                    (
                        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                        "StaleLaunchIdentity",
                    ),
                ] {
                    let refused = f.call(&context, tag, expected).await;
                    assert_eq!(
                        refused["outcome"]["payload"]["code"], code,
                        "{context:?} {tag}: {refused}"
                    );
                }
            }
            assert_eq!(
                f.viewer.calls(),
                before,
                "stale/missing identity must not touch the viewer"
            );
            let leave = f.call(&context, "app.session.leave", Some(&f.launch)).await;
            assert_eq!(leave["outcome"]["_tag"], "Ok", "{context:?}: {leave}");
            assert_eq!(leave["outcome"]["payload"]["launchId"], f.launch);
            assert_eq!(leave["outcome"]["payload"]["state"], "frozen");
            assert!(matches!(
                f.holder.session_status().await.unwrap(),
                host::control::HostSessionStatus::Frozen { .. }
            ));
            let resume = f
                .call(&context, "app.session.resume", Some(&f.launch))
                .await;
            assert_eq!(resume["outcome"]["_tag"], "Ok", "{context:?}: {resume}");
            assert_eq!(resume["outcome"]["payload"]["state"], "running");
            let end = f.call(&context, "app.session.end", Some(&f.launch)).await;
            assert_eq!(
                end["outcome"]["payload"]["phase"], "ended",
                "{context:?}: {end}"
            );
            for action in [
                LinuxViewerAction::Background,
                LinuxViewerAction::ResumeAndFocus,
                LinuxViewerAction::Stop,
            ] {
                assert!(f
                    .viewer
                    .calls()
                    .iter()
                    .any(|call| call.action == action && call.launch_id == f.launch));
            }
            assert!(!f.state.remote.as_ref().unwrap().has_selected_session());
        }
    }

    #[tokio::test]
    async fn selected_stream_local_cli_cut_does_not_forward_peer_or_guest_controls() {
        use crate::authorization::{PersonPass, Principal, Scope};
        let f = SelectedSessionFixture::new().await;
        let before = f.viewer.calls();
        for principal in [
            Principal::OwnerDevice {
                device_public_key: "33".repeat(32),
                owner_public_key: "44".repeat(32),
                owner_statement: "verified".into(),
            },
            Principal::Guest {
                device_public_key: "55".repeat(32),
                owner_statement: "verified".into(),
                person_pass: PersonPass {
                    event_id: "event".into(),
                    event_json: "verified".into(),
                    expires_at: u64::MAX,
                    scopes: vec![Scope::StreamLaunch],
                    person_public_key: "66".repeat(32),
                },
            },
        ] {
            let peer = AuthorizationContext::Peer(principal);
            let status = f.call(&peer, "app.session.status", None).await;
            assert_eq!(
                status["outcome"]["payload"]["code"], "NoActiveSession",
                "{status}"
            );
            for tag in ["app.session.leave", "app.session.resume", "app.session.end"] {
                let response = f.call(&peer, tag, Some(&f.launch)).await;
                assert_eq!(
                    response["outcome"]["payload"]["code"], "NoActiveSession",
                    "{response}"
                );
            }
        }
        let unknown = AuthorizationContext::Peer(Principal::Unknown {
            device_public_key: "77".repeat(32),
        });
        assert!(crate::dispatch(
            &f.state,
            &unknown,
            RpcRequest::SessionStatus(SessionStatusRequest {})
        )
        .await
        .is_err());
        assert_eq!(f.viewer.calls(), before);
        assert!(f.state.remote.as_ref().unwrap().has_selected_session());
        assert!(matches!(
            f.holder.session_status().await.unwrap(),
            host::control::HostSessionStatus::Running { .. }
                | host::control::HostSessionStatus::FocusFailed { .. }
        ));
    }
}
