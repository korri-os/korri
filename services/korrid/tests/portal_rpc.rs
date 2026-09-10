use axum::Router;
use korrid::portal_access::{PortalAccess, PortalPermission};
use korrid::{host_routers_with_storage_and_private, router_with_capability_and_roots};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

const ORIGIN: &str = "http://127.0.0.1:8099";
const TOKEN: &str = "portal-contract-token";

struct RunningPortal {
    address: String,
    server: tokio::task::JoinHandle<()>,
    _root: tempfile::TempDir,
}

impl RunningPortal {
    async fn start(host: bool) -> Self {
        Self::with_host_access(host, Some(PortalPermission::LocalSessions)).await
    }

    async fn with_host_access(host: bool, permission: Option<PortalPermission>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let private = root.path().join("private");
        let app: Router = if host {
            let config = root.path().join("host.toml");
            std::fs::write(&config, "label = \"rg353m\"\n").unwrap();
            host_routers_with_storage_and_private(
                &config,
                // This access-control fixture has only host.toml games. Native
                // catalog tests supply approved installed selections separately.
                None::<std::path::PathBuf>,
                &private,
                permission.map(|permission| PortalAccess::new(TOKEN, ORIGIN, permission)),
            )
            .0
        } else {
            router_with_capability_and_roots(TOKEN, ORIGIN, root.path().join("storage"), &private)
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            address,
            server,
            _root: root,
        }
    }

    async fn rpc(
        &self,
        token: Option<&str>,
        origin: Option<&str>,
        method: &str,
    ) -> reqwest::Response {
        self.request(token, origin, json!({"_tag": method, "payload": {}}))
            .await
    }

    async fn request(
        &self,
        token: Option<&str>,
        origin: Option<&str>,
        body: Value,
    ) -> reqwest::Response {
        // Prove rejection concerns a real, typed RPC rather than invalid JSON.
        serde_json::from_value::<korrid::RpcRequest>(body.clone()).unwrap();
        let mut request = Client::new()
            .post(format!("{}/rpc", self.address))
            .json(&body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(origin) = origin {
            request = request.header("Origin", origin);
        }
        request.send().await.unwrap()
    }
}

impl Drop for RunningPortal {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[tokio::test]
async fn shared_portal_rpc_authenticates_the_same_health_request_in_both_runtimes() {
    for host in [false, true] {
        let portal = RunningPortal::start(host).await;
        for token in [None, Some("incorrect")] {
            assert_eq!(
                portal
                    .rpc(token, Some(ORIGIN), "system.health")
                    .await
                    .status(),
                StatusCode::UNAUTHORIZED,
                "host={host}"
            );
        }
        let response = portal.rpc(Some(TOKEN), Some(ORIGIN), "system.health").await;
        assert_eq!(response.status(), StatusCode::OK, "host={host}");
        assert_eq!(response.headers()["access-control-allow-origin"], ORIGIN);
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"_tag":"system.health","outcome":{"_tag":"Ok","payload":{"version":"korrid-v0"}}})
        );
    }
}

#[tokio::test]
async fn shared_portal_rpc_rejects_a_foreign_origin_even_with_the_token() {
    for host in [false, true] {
        let portal = RunningPortal::start(host).await;
        let response = portal
            .rpc(
                Some(TOKEN),
                Some("https://foreign.example"),
                "system.health",
            )
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "host={host}");
        assert!(response
            .headers()
            .get("access-control-allow-origin")
            .is_none());
    }
}

#[tokio::test]
async fn shared_portal_rpc_reads_the_actual_linux_catalog_without_android_fixtures() {
    let portal = RunningPortal::start(true).await;
    let response = portal
        .rpc(Some(TOKEN), Some(ORIGIN), "app.catalog.snapshot")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"_tag":"app.catalog.snapshot","outcome":{"_tag":"Ok","payload":{"games":[]}}})
    );
}

#[tokio::test]
async fn shared_portal_rpc_scoped_permissions_allow_only_initial_reads_and_local_sessions() {
    for permission in [PortalPermission::ReadOnly, PortalPermission::LocalSessions] {
        let portal = RunningPortal::with_host_access(true, Some(permission)).await;
        for method in [
            "app.catalog.snapshot",
            "system.health",
            "app.local-games.list",
            "system.settings.snapshot",
            "app.discovery.snapshot",
            "app.moonlight.resolve",
            "app.session.status",
        ] {
            let response = portal.rpc(Some(TOKEN), Some(ORIGIN), method).await;
            assert_eq!(response.status(), StatusCode::OK, "{method}");
            assert_eq!(response.json::<Value>().await.unwrap()["_tag"], method);
        }
        for body in [
            json!({"_tag":"app.moonlight.launch.prepare","payload":{"hostUuid":"h","appId":1}}),
            json!({"_tag":"app.moonlight.launch.cancel","payload":{"launchId":"l"}}),
            json!({"_tag":"app.moonlight.certificate.attest","payload":{"hostUuid":"h"}}),
            json!({"_tag":"app.moonlight.certificate.provision","payload":{"hostUuid":"h","clientCertificate":"c"}}),
            json!({"_tag":"app.moonlight.certificate.revoke","payload":{"hostUuid":"h","clientCertificate":"c"}}),
            json!({"_tag":"app.session.prepare","payload":{"gameId":"g"}}),
            json!({"_tag":"app.session.prepare","payload":{"gameId":"g","host":"peer"}}),
            json!({"_tag":"app.session.prepare","payload":{"gameId":"g","host":"rg353m"}}),
            json!({"_tag":"app.session.prepare","payload":{"gameId":"g","host":""}}),
            json!({"_tag":"app.session.stop","payload":{}}),
            json!({"_tag":"app.session.freeze","payload":{}}),
            json!({"_tag":"app.session.thaw","payload":{}}),
            json!({"_tag":"app.source.status","payload":{"devicePublicKey":"k"}}),
            json!({"_tag":"app.session.controls","payload":{"launchId":"l"}}),
            json!({"_tag":"app.session.control.invoke","payload":{"launchId":"l","controlId":"c"}}),
            json!({"_tag":"app.local-games.launch","payload":{"gameId":"g"}}),
            json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt":"r"}}),
            json!({"_tag":"app.discovery.removeLocation","payload":{"locationId":"l"}}),
            json!({"_tag":"app.discovery.rescan","payload":{}}),
            json!({"_tag":"system.settings.update","payload":{"expectedRevision":"r","settingId":"s","value":"v"}}),
            json!({"_tag":"system.settings.steamgriddbCredential.set","payload":{"token":"t"}}),
            json!({"_tag":"system.settings.steamgriddbCredential.clear","payload":{}}),
        ] {
            let method = body["_tag"].as_str().unwrap().to_owned();
            let local_session = (method == "app.session.prepare"
                && body["payload"].get("host").is_none())
                || method == "app.session.stop";
            let allowed = permission == PortalPermission::LocalSessions && local_session;
            let response = portal.request(Some(TOKEN), Some(ORIGIN), body).await;
            assert_eq!(
                response.status(),
                if allowed {
                    StatusCode::OK
                } else {
                    StatusCode::FORBIDDEN
                },
                "{permission:?}: {method}"
            );
            if allowed {
                // HTTP 200 proves dispatch, not launch: this portal has no configured game.
                let body = response.json::<Value>().await.unwrap();
                assert_eq!(body["_tag"], method);
                assert_eq!(body["outcome"]["_tag"], "Err");
                assert_eq!(
                    body["outcome"]["payload"]["code"],
                    if method == "app.session.prepare" {
                        "HostGameNotFound"
                    } else {
                        "ExpectedLaunchIdRequired"
                    }
                );
            }
        }
    }
}

#[tokio::test]
async fn shared_portal_rpc_local_sessions_still_require_credentials_and_the_exact_origin() {
    let portal = RunningPortal::start(true).await;
    for body in [
        json!({"_tag":"app.session.prepare","payload":{"gameId":"g"}}),
        json!({"_tag":"app.session.stop","payload":{"expectedLaunchId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}),
    ] {
        for token in [None, Some("incorrect")] {
            assert_eq!(
                portal
                    .request(token, Some(ORIGIN), body.clone())
                    .await
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        let response = portal
            .request(Some(TOKEN), Some("https://foreign.example"), body.clone())
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(response
            .headers()
            .get("access-control-allow-origin")
            .is_none());
        // Native callers may omit Origin, but must still hold the capability.
        let response = portal.request(Some(TOKEN), None, body).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json::<Value>().await.unwrap()["outcome"]["_tag"],
            "Err"
        );
    }
}

#[tokio::test]
async fn shared_portal_rpc_full_permission_still_dispatches_commands_on_both_runtimes() {
    for host in [false, true] {
        let portal = RunningPortal::with_host_access(host, Some(PortalPermission::Full)).await;
        let response = portal
            .request(
                Some(TOKEN),
                Some(ORIGIN),
                json!({
                    "_tag":"app.local-games.launch", "payload":{"gameId":"not-configured"}
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK, "host={host}");
        assert_eq!(
            response.json::<Value>().await.unwrap()["outcome"]["_tag"],
            "Err"
        );
    }
}

#[tokio::test]
async fn shared_portal_rpc_preserves_encrypted_peer_routing_and_default_plaintext_rejection() {
    let disabled = RunningPortal::with_host_access(true, None).await;
    assert_eq!(
        disabled
            .rpc(Some(TOKEN), Some(ORIGIN), "system.health")
            .await
            .status(),
        StatusCode::UPGRADE_REQUIRED
    );
    let enabled = RunningPortal::start(true).await;
    for portal in [&disabled, &enabled] {
        let response = Client::new()
            .post(format!("{}/peer-rpc", portal.address))
            .bearer_auth(TOKEN)
            .json(&json!({"_tag":"system.health","payload":{}}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_client_error());
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn shared_portal_rpc_cors_preflight_has_the_same_exact_origin_policy() {
    for host in [false, true] {
        let portal = RunningPortal::start(host).await;
        for origin in [ORIGIN, "https://foreign.example"] {
            let response = Client::new()
                .request(reqwest::Method::OPTIONS, format!("{}/rpc", portal.address))
                .header("Origin", origin)
                .header("Access-Control-Request-Method", "POST")
                .header(
                    "Access-Control-Request-Headers",
                    "authorization,content-type",
                )
                .send()
                .await
                .unwrap();
            assert!(response.status().is_success());
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .map(|value| value.to_str().unwrap()),
                (origin == ORIGIN).then_some(ORIGIN)
            );
        }
        assert_eq!(
            portal
                .rpc(Some(TOKEN), None, "system.health")
                .await
                .status(),
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn shared_portal_rpc_browser_permission_does_not_change_private_control_authority() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("host.toml");
    std::fs::write(&config, "label = \"rg353m\"\n").unwrap();
    let (_, control) = host_routers_with_storage_and_private(
        &config,
        None::<std::path::PathBuf>,
        root.path().join("private"),
        Some(PortalAccess::new(TOKEN, ORIGIN, PortalPermission::ReadOnly)),
    );
    let path = root.path().join("control.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, control).await.unwrap() });
    let response = Client::builder()
        .unix_socket(path)
        .build()
        .unwrap()
        .post("http://localhost/rpc")
        .json(&json!({"_tag":"app.session.stop","payload":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["outcome"]["_tag"],
        "Err"
    );
    server.abort();
}
