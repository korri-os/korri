//! Browser authority for the existing `/rpc` contract on either runtime.

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};

use crate::RpcRequest;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortalPermission {
    Full,
    ReadOnly,
    LocalSessions,
}

impl PortalPermission {
    fn permits(self, request: &RpcRequest) -> bool {
        let initial_read = match request {
            RpcRequest::GameRoutes(_)
            | RpcRequest::CatalogSnapshot(_)
            | RpcRequest::Health(_)
            | RpcRequest::LocalGamesList(_)
            | RpcRequest::SettingsSnapshot(_)
            | RpcRequest::IdentityStatus(_)
            | RpcRequest::DiscoverySnapshot(_)
            | RpcRequest::PeerList(_)
            | RpcRequest::SessionStatus(_)
            | RpcRequest::SessionControls(_) => true,
            RpcRequest::GameRunnerSet(_)
            | RpcRequest::SelectedGameLaunch(_)
            | RpcRequest::MoonlightCertificateAttest(_)
            | RpcRequest::MoonlightCertificateProvision(_)
            | RpcRequest::MoonlightCertificateRevoke(_)
            | RpcRequest::SessionPrepare(_)
            | RpcRequest::SessionControlInvoke(_)
            | RpcRequest::SessionStop(_)
            | RpcRequest::SessionFreeze(_)
            | RpcRequest::SessionThaw(_)
            | RpcRequest::SourceStatus(_)
            | RpcRequest::DiscoveryRegisterReceipt(_)
            | RpcRequest::DiscoveryRemoveLocation(_)
            | RpcRequest::DiscoveryRescan(_)
            | RpcRequest::SettingsUpdate(_)
            | RpcRequest::SteamGridDbCredentialSet(_)
            | RpcRequest::SteamGridDbCredentialClear(_)
            | RpcRequest::IdentityBackupExport(_)
            | RpcRequest::IdentityLocalSwitch(_)
            | RpcRequest::IdentityNip46Switch(_)
            | RpcRequest::IdentityRetiredExport(_)
            | RpcRequest::IdentityRetiredDelete(_) => false,
        };
        match self {
            Self::Full => true,
            Self::ReadOnly => initial_read,
            Self::LocalSessions => {
                initial_read
                    || match request {
                        RpcRequest::SessionPrepare(request) => request.host.is_none(),
                        // The existing stop contract has no peer selector. The host
                        // executor enforces expectedLaunchId before changing a session.
                        RpcRequest::SessionStop(_)
                        | RpcRequest::SessionFreeze(_)
                        | RpcRequest::SessionThaw(_)
                        | RpcRequest::SessionControlInvoke(_)
                        | RpcRequest::SelectedGameLaunch(_) => true,
                        _ => false,
                    }
            }
        }
    }
}

/// Configuration from the existing token and portal-origin producers. The
/// capability is intentionally absent from Debug output and wire serialization.
#[derive(Clone)]
pub struct PortalAccess {
    capability: String,
    allowed_origins: Vec<HeaderValue>,
    permission: PortalPermission,
}

impl PortalAccess {
    pub fn new(capability: &str, allowed_origin: &str, permission: PortalPermission) -> Self {
        assert!(
            !capability.is_empty(),
            "portal capability must not be empty"
        );
        let origin: HeaderValue = allowed_origin
            .parse()
            .expect("allowed portal origin must be a valid header value");
        Self {
            capability: capability.into(),
            allowed_origins: vec![origin],
            permission,
        }
    }

    pub(crate) fn allowed_origins(&self) -> &[HeaderValue] {
        &self.allowed_origins
    }

    pub(crate) fn authorize(
        &self,
        headers: &HeaderMap,
        request: &RpcRequest,
    ) -> Result<(), StatusCode> {
        let expected = format!("Bearer {}", self.capability);
        let mut authorizations = headers.get_all(header::AUTHORIZATION).iter();
        if authorizations.next().and_then(|value| value.to_str().ok()) != Some(expected.as_str())
            || authorizations.next().is_some()
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
        // CORS alone hides a response; it does not prevent a request's effects.
        // A native caller can omit Origin, but must still possess the token.
        let mut origins = headers.get_all(header::ORIGIN).iter();
        if let Some(origin) = origins.next() {
            if !self.allowed_origins.contains(origin) || origins.next().is_some() {
                return Err(StatusCode::FORBIDDEN);
            }
        }
        if !self.permission.permits(request) {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
        Router,
    };
    use serde_json::{json, Value};
    use std::{
        net::UdpSocket,
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };
    use tower::ServiceExt;

    const TOKEN: &str = "portal-test-token";
    const ORIGIN: &str = "http://127.0.0.1:8099";

    #[test]
    fn local_sessions_allow_only_local_prepare_and_exact_session_controls() {
        for host in [None, Some("peer"), Some("rg353m"), Some("")] {
            let request = RpcRequest::SessionPrepare(crate::SessionPrepareRequest {
                game_id: "neverball".into(),
                host: host.map(str::to_owned),
            });
            assert_eq!(
                PortalPermission::LocalSessions.permits(&request),
                host.is_none()
            );
            assert!(!PortalPermission::ReadOnly.permits(&request));
            assert!(PortalPermission::Full.permits(&request));
        }
        // Stop has no host selector; the host executor requires an exact launch ID.
        let stop = RpcRequest::SessionStop(crate::SessionStopRequest {
            force: Some(true),
            expected_launch_id: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
        });
        assert!(PortalPermission::LocalSessions.permits(&stop));
        assert!(!PortalPermission::ReadOnly.permits(&stop));
        assert!(PortalPermission::Full.permits(&stop));

        for request in [
            RpcRequest::SessionFreeze(crate::SessionFreezeRequest {
                expected_launch_id: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
            }),
            RpcRequest::SessionThaw(crate::SessionThawRequest {
                expected_launch_id: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
            }),
            RpcRequest::SessionControlInvoke(crate::SessionControlInvokeRequest {
                launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                control_id: "@korri:mgba/open-menu".into(),
                value: None,
            }),
        ] {
            assert!(PortalPermission::LocalSessions.permits(&request));
            assert!(!PortalPermission::ReadOnly.permits(&request));
            assert!(PortalPermission::Full.permits(&request));
        }
    }

    #[test]
    fn peer_list_reads_authenticated_peer_state_under_every_permission() {
        // The handler only reads the federation directory, and authorization
        // already limits it to the owner's own devices.
        let request = RpcRequest::PeerList(crate::PeerListRequest {});
        assert!(PortalPermission::ReadOnly.permits(&request));
        assert!(PortalPermission::LocalSessions.permits(&request));
        assert!(PortalPermission::Full.permits(&request));
    }

    #[test]
    fn local_sessions_reject_duplicate_authorization_and_origin_headers() {
        let access = PortalAccess::new(TOKEN, ORIGIN, PortalPermission::LocalSessions);
        let request = RpcRequest::SessionPrepare(crate::SessionPrepareRequest {
            game_id: "neverball".into(),
            host: None,
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {TOKEN}").parse().unwrap(),
        );
        headers.insert(header::ORIGIN, ORIGIN.parse().unwrap());
        assert_eq!(access.authorize(&headers, &request), Ok(()));
        let mut duplicate = headers.clone();
        duplicate.append(
            header::AUTHORIZATION,
            headers[header::AUTHORIZATION].clone(),
        );
        assert_eq!(
            access.authorize(&duplicate, &request),
            Err(StatusCode::UNAUTHORIZED)
        );
        for origin in [ORIGIN, "https://foreign.example"] {
            let mut duplicate = headers.clone();
            duplicate.append(header::ORIGIN, origin.parse().unwrap());
            assert_eq!(
                access.authorize(&duplicate, &request),
                Err(StatusCode::FORBIDDEN)
            );
        }
    }

    async fn rpc(app: &Router, method: &str, payload: Value) -> Value {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                    .header(header::ORIGIN, ORIGIN)
                    .body(Body::from(
                        json!({"_tag":method,"payload":payload}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{method}");
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn installed_routes_persist_choices_and_launch_explicit_runner_through_rpc() {
        let root = tempfile::tempdir().unwrap();
        crate::config::test_fixtures::gba(root.path());
        std::fs::create_dir(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let registry = crate::plugin_test_fixtures::installed(root.path());
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"route-device\"\ngames = []\n").unwrap();
        let private = root.path().join("private");
        let backend = Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default());
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let control_port = socket.local_addr().unwrap().port();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let observed_commands = Arc::clone(&commands);
        let control_backend = Arc::clone(&backend);
        let control_server = thread::spawn(move || {
            let mut quit_count = 0;
            for reply in [
                Some("GET_STATUS PLAYING gba,wl4.gba,crc32=1a2b3c4d\n"),
                None,
                Some("GET_STATUS PAUSED gba,wl4.gba,crc32=1a2b3c4d\n"),
                None,
                None,
            ] {
                let mut request = [0_u8; 128];
                let (length, peer) = socket.recv_from(&mut request).unwrap();
                let command = std::str::from_utf8(&request[..length])
                    .unwrap()
                    .trim()
                    .to_owned();
                observed_commands
                    .lock()
                    .unwrap()
                    .push(format!("{}:{command}", control_backend.thaw_count()));
                if command == "QUIT" {
                    quit_count += 1;
                    if quit_count == 2 {
                        control_backend.complete_live();
                    }
                }
                if let Some(reply) = reply {
                    socket.send_to(reply.as_bytes(), peer).unwrap();
                }
            }
        });
        let runtime = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            Some(root.path().into()),
            private.clone(),
            backend.clone(),
        )
        .with_route_registry(root.path().into(), registry.clone())
        .with_retroarch_control_port(control_port);
        let (app, _) = crate::secure_host_routers(
            runtime,
            &private,
            Some(PortalAccess::new(TOKEN, ORIGIN, PortalPermission::Full)),
        );
        let game_id = crate::config::test_fixtures::GBA_ID;
        let list = rpc(&app, "app.local-games.routes", json!({"gameId":game_id})).await;
        let listed = &list["outcome"]["payload"];
        assert_eq!(
            listed["selection"],
            json!({"_tag":"Selected","runnerId":"@korri:mgba/mgba"}),
            "{list}"
        );
        assert_eq!(listed["routes"][0]["runnerId"], "@korri:mgba/mgba");
        let set = json!({"scope":{"_tag":"Game","id":game_id},"runnerId":"@missing:build/core","expectedRevision":listed["revisions"]["games"]});
        let saved = rpc(&app, "app.local-games.runner.set", set.clone()).await;
        assert_eq!(saved["outcome"]["_tag"], "Ok", "{saved}");
        let conflict = rpc(&app, "app.local-games.runner.set", set).await;
        assert_eq!(conflict["outcome"]["payload"]["code"], "SettingsConflict");
        let list = rpc(&app, "app.local-games.routes", json!({"gameId":game_id})).await;
        assert_eq!(list["outcome"]["payload"]["selection"]["_tag"], "Choose");
        assert_eq!(
            list["outcome"]["payload"]["gameRunner"],
            "@missing:build/core"
        );
        let prepare = rpc(&app, "app.session.prepare", json!({"gameId":game_id})).await;
        assert_eq!(prepare["outcome"]["_tag"], "Err");
        let launch = rpc(&app, "app.local-games.launch.selected", json!({"gameId":game_id,"runnerId":"@korri:mgba/mgba","overrides":{"settings":{"video_vsync":false,"absent_key":1}}})).await;
        assert_eq!(launch["outcome"]["_tag"], "Ok", "{launch}");
        assert_eq!(launch["outcome"]["payload"]["session"]["gameId"], game_id);
        let launch_id = launch["outcome"]["payload"]["session"]["launchId"]
            .as_str()
            .unwrap();
        let controls = rpc(&app, "app.session.controls", json!({"launchId":launch_id})).await;
        assert_eq!(controls["outcome"]["_tag"], "Ok", "{controls}");
        assert_eq!(controls["outcome"]["payload"]["launchId"], launch_id);
        assert_eq!(
            controls["outcome"]["payload"]["groups"][0]["id"],
            "@korri:mgba"
        );
        assert_eq!(
            controls["outcome"]["payload"]["groups"][0]["controls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|control| control["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["@korri:mgba/open-menu", "@korri:mgba/quit"]
        );
        let frozen = rpc(
            &app,
            "app.session.freeze",
            json!({"expectedLaunchId":launch_id}),
        )
        .await;
        assert_eq!(frozen["outcome"]["_tag"], "Ok", "{frozen}");
        assert_eq!(backend.thaw_count(), 0);
        let invoked = rpc(
            &app,
            "app.session.control.invoke",
            json!({"launchId":launch_id,"controlId":"@korri:mgba/open-menu"}),
        )
        .await;
        assert_eq!(invoked["outcome"]["_tag"], "Ok", "{invoked}");
        assert_eq!(invoked["outcome"]["payload"]["launchId"], launch_id);
        assert_eq!(backend.thaw_count(), 1);
        let open_menu_still_frozen = rpc(&app, "app.session.status", json!({})).await;
        assert_eq!(
            open_menu_still_frozen["outcome"]["payload"]["active"],
            json!({"launchId":launch_id,"gameId":game_id,"phase":"frozen"}),
            "a control executed after thaw must restore the exact frozen state"
        );
        let ignored_quit = rpc(
            &app,
            "app.session.control.invoke",
            json!({"launchId":launch_id,"controlId":"@korri:mgba/quit"}),
        )
        .await;
        assert_eq!(
            ignored_quit["outcome"]["payload"]["reason"], "Unavailable",
            "{ignored_quit}"
        );
        assert_eq!(
            backend.thaw_count(),
            2,
            "the unanswered one-way execution crossed the thaw boundary exactly once"
        );
        let still_frozen = rpc(&app, "app.session.status", json!({})).await;
        assert_eq!(
            still_frozen["outcome"]["payload"]["active"],
            json!({"launchId":launch_id,"gameId":game_id,"phase":"frozen"})
        );
        let unknown = rpc(
            &app,
            "app.session.control.invoke",
            json!({"launchId":launch_id,"controlId":"@korri:mgba/not-declared"}),
        )
        .await;
        assert_eq!(unknown["outcome"]["payload"]["reason"], "UnknownControl");
        let stale = rpc(
            &app,
            "app.session.control.invoke",
            json!({"launchId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","controlId":"@korri:mgba/open-menu"}),
        )
        .await;
        assert_eq!(stale["outcome"]["payload"]["reason"], "StaleSession");
        // A fresh korrid recovers the route from the live transient unit's
        // authoritative launch metadata, not from the durable session schema
        // or process-local bookkeeping.
        drop(app);
        let recovered = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            Some(root.path().into()),
            private.clone(),
            backend,
        )
        .with_route_registry(root.path().into(), registry)
        .with_retroarch_control_port(control_port);
        let (app, _) = crate::secure_host_routers(
            recovered,
            &private,
            Some(PortalAccess::new(TOKEN, ORIGIN, PortalPermission::Full)),
        );
        let recovered_controls =
            rpc(&app, "app.session.controls", json!({"launchId":launch_id})).await;
        assert_eq!(
            recovered_controls["outcome"]["_tag"], "Ok",
            "{recovered_controls}"
        );
        assert_eq!(
            recovered_controls["outcome"]["payload"]["groups"][0]["controls"][0]["id"],
            "@korri:mgba/open-menu"
        );
        let returned = rpc(
            &app,
            "app.session.thaw",
            json!({"expectedLaunchId":launch_id}),
        )
        .await;
        assert_eq!(returned["outcome"]["_tag"], "Ok", "{returned}");
        // The runner accepts video_vsync and reports the key it cannot apply.
        // korrid carries that report to the portal without owning the table.
        assert_eq!(
            launch["outcome"]["payload"]["warnings"][0]["setting"],
            "absent_key"
        );
        assert_eq!(
            launch["outcome"]["payload"]["warnings"][1],
            serde_json::Value::Null
        );
        let resumed = rpc(&app, "app.session.prepare", json!({"gameId":game_id})).await;
        assert_eq!(resumed["outcome"]["_tag"], "Ok", "{resumed}");
        assert_eq!(
            resumed["outcome"]["payload"], launch["outcome"]["payload"]["session"],
            "ordinary prepare must resume the explicit launch despite the stale preference"
        );
        let repeated = rpc(
            &app,
            "app.local-games.launch.selected",
            json!({"gameId":game_id,"runnerId":"@korri:mgba/mgba"}),
        )
        .await;
        assert_eq!(
            repeated["outcome"]["payload"]["code"], "ActiveSessionConflict",
            "explicit runner launch must not claim it changed an already running route"
        );
        let list = rpc(&app, "app.local-games.routes", json!({"gameId":game_id})).await;
        assert_eq!(
            list["outcome"]["payload"]["gameRunner"], "@missing:build/core",
            "explicit launch must not rewrite preference"
        );

        let quit = rpc(
            &app,
            "app.session.control.invoke",
            json!({"launchId":launch_id,"controlId":"@korri:mgba/quit"}),
        )
        .await;
        assert_eq!(quit["outcome"]["_tag"], "Ok", "{quit}");
        control_server.join().unwrap();
        assert_eq!(
            *commands.lock().unwrap(),
            [
                "1:GET_STATUS",
                "1:MENU_TOGGLE",
                "1:GET_STATUS",
                "2:QUIT",
                "3:QUIT"
            ]
        );
        let status = rpc(&app, "app.session.status", json!({})).await;
        assert_eq!(status["outcome"]["payload"]["code"], "SessionCompleted");
    }

    #[tokio::test]
    async fn ordinary_prepare_resumes_explicit_runner_despite_ambiguous_routes() {
        let root = tempfile::tempdir().unwrap();
        crate::config::test_fixtures::gba(root.path());
        std::fs::create_dir(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let registry = crate::plugin_test_fixtures::installed(root.path());
        let mgba = registry
            .installed_package("@korri:mgba/mgba")
            .unwrap()
            .clone();
        // A second runner uses the same installed declaration and payload.
        let source = std::fs::read_to_string(mgba.package.join("plugin.ts")).unwrap();
        std::fs::write(
            mgba.package.join("plugin.ts"),
            format!("{source}\nrunners.other = {{ ...runners.mgba, id: '@korri:mgba/other' }};\n"),
        )
        .unwrap();
        let registry = crate::plugin::PluginRegistry::from_installed(vec![mgba]).unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"route-device\"\ngames = []\n").unwrap();
        let private = root.path().join("private");
        let runtime = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            Some(root.path().into()),
            private.clone(),
            Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        )
        .with_route_registry(root.path().into(), registry);
        let (app, _) = crate::secure_host_routers(
            runtime,
            &private,
            Some(PortalAccess::new(TOKEN, ORIGIN, PortalPermission::Full)),
        );
        let game_id = crate::config::test_fixtures::GBA_ID;
        let routes = rpc(&app, "app.local-games.routes", json!({"gameId":game_id})).await;
        assert_eq!(
            routes["outcome"]["payload"]["routes"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(routes["outcome"]["payload"]["selection"]["_tag"], "Choose");
        let prepare = rpc(&app, "app.session.prepare", json!({"gameId":game_id})).await;
        assert_eq!(prepare["outcome"]["_tag"], "Err", "{prepare}");
        let launch = rpc(
            &app,
            "app.local-games.launch.selected",
            json!({"gameId":game_id,"runnerId":"@korri:mgba/mgba"}),
        )
        .await;
        assert_eq!(launch["outcome"]["_tag"], "Ok", "{launch}");
        let resumed = rpc(&app, "app.session.prepare", json!({"gameId":game_id})).await;
        assert_eq!(resumed["outcome"]["_tag"], "Ok", "{resumed}");
        assert_eq!(
            resumed["outcome"]["payload"],
            launch["outcome"]["payload"]["session"]
        );
        let switched = rpc(
            &app,
            "app.local-games.launch.selected",
            json!({"gameId":game_id,"runnerId":"@korri:mgba/other"}),
        )
        .await;
        assert_eq!(
            switched["outcome"]["payload"]["code"],
            "ActiveSessionConflict"
        );
    }

    #[test]
    fn route_reads_and_explicit_launch_do_not_grant_preference_write_permission() {
        let request = |tag: &str, payload| {
            serde_json::from_value::<RpcRequest>(json!({"_tag":tag,"payload":payload})).unwrap()
        };
        let list = request("app.local-games.routes", json!({"gameId":"game"}));
        let launch = request(
            "app.local-games.launch.selected",
            json!({"gameId":"game","runnerId":"@korri:mgba/mgba"}),
        );
        let set = request(
            "app.local-games.runner.set",
            json!({"scope":{"_tag":"System","id":"gba"},"runnerId":null,"expectedRevision":"r"}),
        );
        assert!(PortalPermission::ReadOnly.permits(&list));
        assert!(!PortalPermission::ReadOnly.permits(&launch));
        assert!(PortalPermission::LocalSessions.permits(&launch));
        assert!(!PortalPermission::LocalSessions.permits(&set));
        assert!(PortalPermission::Full.permits(&set));
    }

    #[tokio::test]
    async fn local_sessions_prepare_and_exact_stop_use_the_existing_host_executor() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        // Same HostConfig producer as the existing host router tests in lib.rs.
        std::fs::write(&config, "label = \"rg353m\"\n[[games]]\nid = \"neverball\"\ntitle = \"Neverball\"\ncommand = [\"neverball\"]\n").unwrap();
        let private = root.path().join("private");
        let runtime = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            private.clone(),
            Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        );
        let (app, _) = crate::secure_host_routers(
            runtime,
            &private,
            Some(PortalAccess::new(
                TOKEN,
                ORIGIN,
                PortalPermission::LocalSessions,
            )),
        );
        let prepared = rpc(&app, "app.session.prepare", json!({"gameId":"neverball"})).await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok", "{prepared}");
        assert_eq!(prepared["outcome"]["payload"]["gameId"], "neverball");
        let launch_id = prepared["outcome"]["payload"]["launchId"].as_str().unwrap();
        assert_eq!(launch_id.len(), 32);
        let repeated = rpc(&app, "app.session.prepare", json!({"gameId":"neverball"})).await;
        assert_eq!(repeated, prepared);
        let controls = rpc(&app, "app.session.controls", json!({"launchId":launch_id})).await;
        assert_eq!(controls["outcome"]["_tag"], "Ok", "{controls}");
        assert_eq!(controls["outcome"]["payload"]["launchId"], launch_id);
        assert_eq!(controls["outcome"]["payload"]["groups"], json!([]));
        for (payload, code) in [
            (json!({}), "ExpectedLaunchIdRequired"),
            (
                json!({"expectedLaunchId":"stale-launch-id","force":true}),
                "StaleLaunchIdentity",
            ),
        ] {
            let rejected = rpc(&app, "app.session.stop", payload).await;
            assert_eq!(rejected["outcome"]["_tag"], "Err");
            assert_eq!(rejected["outcome"]["payload"]["code"], code);
            let status = rpc(&app, "app.session.status", json!({})).await;
            assert_eq!(status["outcome"]["_tag"], "Ok");
            assert_eq!(
                status["outcome"]["payload"]["active"]["launchId"],
                launch_id
            );
        }
        let stopped = rpc(
            &app,
            "app.session.stop",
            json!({"expectedLaunchId":launch_id}),
        )
        .await;
        assert_eq!(
            stopped["outcome"],
            json!({"_tag":"Ok","payload":{"phase":"stopped"}})
        );
        let status = rpc(&app, "app.session.status", json!({})).await;
        assert_eq!(status["outcome"]["_tag"], "Err");
        assert_eq!(status["outcome"]["payload"]["code"], "SessionCompleted");
    }
}
