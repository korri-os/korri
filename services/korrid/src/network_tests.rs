use super::*;
use std::sync::Mutex;

struct InMemoryNetworkManager {
    snapshot: NetworkSnapshot,
    reason: Option<u32>,
    passwords: Mutex<Vec<bool>>,
}
impl NetworkManager for InMemoryNetworkManager {
    fn read(&self) -> BoxFuture<'_, Result<NetworkSnapshot, NetworkFailure>> {
        Box::pin(async { Ok(self.snapshot.clone()) })
    }
    fn scan(&self) -> BoxFuture<'_, Result<(), NetworkFailure>> {
        Box::pin(async { Ok(()) })
    }
    fn connect<'a>(
        &'a self,
        _: &'a str,
        _: bool,
        password: Option<&'a str>,
    ) -> BoxFuture<'a, Result<(), NetworkFailure>> {
        Box::pin(async move {
            self.passwords.lock().unwrap().push(password.is_some());
            match self.reason {
                Some(reason) => Err(NetworkFailure::state_reason(reason)),
                None => Ok(()),
            }
        })
    }
}
fn service(reason: Option<u32>) -> NetworkService {
    NetworkService::new(Arc::new(InMemoryNetworkManager {
        snapshot: NetworkSnapshot {
            connection: NetworkConnection::Offline,
            wifi: Wifi::Available {
                scanning: false,
                networks: vec![WifiNetwork {
                    id: "ap".into(),
                    ssid: "Home".into(),
                    strength_percent: 80,
                    secured: true,
                    known: false,
                }],
                attempt: WifiAttempt::Idle,
            },
        },
        reason,
        passwords: Mutex::new(vec![]),
    }))
}
#[tokio::test]
async fn activation_failure_publishes_safe_copy_and_password_rejection() {
    for reason in 0..15 {
        let service = service(Some(reason));
        let result = service.connect("ap", false, Some("test-passphrase")).await;
        let NetworkStatusOutcome::Ok(NetworkSnapshot {
            wifi:
                Wifi::Available {
                    attempt:
                        WifiAttempt::Failed {
                            message,
                            password_rejected,
                            ssid,
                        },
                    ..
                },
            ..
        }) = result
        else {
            panic!("failure is a model fact")
        };
        assert_eq!(ssid, "Home");
        assert_eq!(password_rejected, (7..=11).contains(&reason));
        assert!(!message.is_empty());
        assert!(!message.contains("test-passphrase"));
    }
}
#[tokio::test]
async fn hidden_network_and_scan_complete_through_the_same_service() {
    let service = service(None);
    assert!(matches!(service.scan().await, NetworkStatusOutcome::Ok(_)));
    assert!(matches!(
        service.connect("Hidden", true, None).await,
        NetworkStatusOutcome::Ok(_)
    ));
}
#[test]
fn request_debug_never_contains_password() {
    let request: crate::RpcRequest = serde_json::from_value(serde_json::json!({"_tag":"system.wifi.connect","payload":{"networkId":"ap","password":"test-passphrase"}})).unwrap();
    assert!(!format!("{request:?}").contains("test-passphrase"));
    let request: crate::RpcRequest = serde_json::from_value(serde_json::json!({"_tag":"system.wifi.connectHidden","payload":{"ssid":"hidden","password":"test-passphrase"}})).unwrap();
    assert!(!format!("{request:?}").contains("test-passphrase"));
}

#[tokio::test]
async fn network_and_onboarding_rpc_round_trip_uses_portal_authority_and_real_marker() {
    let local = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let (mut state, ()) = crate::brain_app_state(
        "onboarding-test-capability",
        "http://portal.test",
        local.path(),
        private.path(),
        crate::plugin_policy::RegistrySource::Installed,
        crate::config::snapshot::ConfigSnapshotCoordinator::new(local.path()),
        crate::discovery::FolderSelectionGrantStore::default(),
        None,
        None,
        None,
    );
    state.network = service(Some(7));
    let app = crate::portal_router_from_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/rpc", listener.local_addr().unwrap());
    let task = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let denied = client
        .post(&url)
        .json(&serde_json::json!({"_tag":"system.onboarding.finish", "payload":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert!(matches!(
        crate::onboarding::status(private.path()),
        crate::onboarding::OnboardingStatusOutcome::Ok(crate::onboarding::OnboardingStatus {
            needed: true
        })
    ));
    for (tag, payload) in [
        ("system.network.status", serde_json::json!({})),
        ("system.wifi.scan", serde_json::json!({})),
        (
            "system.wifi.connect",
            serde_json::json!({"networkId":"ap","password":"test-passphrase"}),
        ),
        (
            "system.wifi.connectHidden",
            serde_json::json!({"ssid":"Hidden","password":"test-passphrase"}),
        ),
        ("system.onboarding.status", serde_json::json!({})),
        ("system.onboarding.finish", serde_json::json!({})),
    ] {
        let response = client
            .post(&url)
            .bearer_auth("onboarding-test-capability")
            .json(&serde_json::json!({"_tag": tag, "payload":payload}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{tag}");
        let text = response.text().await.unwrap();
        assert!(!text.contains("test-passphrase"));
        let response: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(response["_tag"], tag);
        assert_eq!(response["outcome"]["_tag"], "Ok");
    }
    assert!(matches!(
        crate::onboarding::status(private.path()),
        crate::onboarding::OnboardingStatusOutcome::Ok(crate::onboarding::OnboardingStatus {
            needed: false
        })
    ));
    task.abort();
}

#[tokio::test]
async fn local_sessions_wifi_scan_reaches_network_manager_through_host_http_router() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("host.toml");
    std::fs::write(&config, "label = \"wifi-device\"\ngames = []\n").unwrap();
    let private = root.path().join("private");
    let runtime = crate::host::HostRuntime::from_paths_with_backend(
        &config,
        None,
        private.clone(),
        Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
    );
    let (mut lan, _) = crate::app_states(runtime);
    lan.network = service(None);
    let portal = crate::portal_router_for(
        &lan,
        Some(crate::PortalAccess::new(
            "wifi-test-capability",
            "http://portal.test",
            crate::PortalPermission::LocalSessions,
        )),
    );
    let app = crate::peer_rpc::PeerRpcServer::new(lan, &private)
        .unwrap()
        .router(portal);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/rpc", listener.local_addr().unwrap());
    let task = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth("wifi-test-capability")
        .header("Origin", "http://portal.test")
        .json(&serde_json::json!({"_tag":"system.wifi.scan","payload":{}}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    task.abort();
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let response: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["_tag"], "system.wifi.scan");
    assert_eq!(response["outcome"]["_tag"], "Ok", "{response}");
    assert_eq!(
        response["outcome"]["payload"]["wifi"]["payload"]["networks"][0]["ssid"],
        "Home"
    );
}
