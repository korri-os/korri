//! Exercise the production HTTP/capability boundary and actual cache placement.
use crate::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, time::Duration};

#[tokio::test]
async fn local_rpc_download_retry_and_launch_gate_share_cache_truth() {
    let root = tempfile::tempdir().unwrap();
    plugin_test_fixtures::write_gba_library(root.path());
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let package = root.path().join("plugin");
    fs::create_dir(&package).unwrap();
    let digest = hex::encode(Sha256::digest(b"runtime"));
    fs::write(package.join("plugin.ts"), format!(r#"
        export const name = 'runtime';
        export const title = 'Runtime';
        export const systems = {{gba:{{id:'gba', content:{{extensions:['gba']}}}}}};
        export const artifacts = {{payload:{{id:'@test:runtime/payload', version:'v1', files:[{{url:'file:///private', sha256:'sha256:{digest}', bytes:7}}]}}}};
        export const runners = {{run:{{id:'@test:runtime/run', program:'run', systems:['gba'], artifacts:['@test:runtime/payload']}}}};
        export const handlers = {{'launch.prepare': input => ({{command:input.program, args:[input.artifacts.payload]}})}};
    "#)).unwrap();
    fs::write(
        package.join("manifest.json"),
        json!({"entry":"plugin.ts","sources":["plugin.ts"],"files":{"run":package.join("run")}})
            .to_string(),
    )
    .unwrap();
    let registry =
        plugin::PluginRegistry::from_installed(vec![plugin_installation::EnabledPackage {
            id: "@test:runtime".into(),
            package: package.clone(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: vec![],
            hosts: vec![],
            files: BTreeMap::from([("run".into(), package.join("run"))]),
        }])
        .unwrap();
    let config = root.path().join("host.toml");
    fs::write(&config, "label = \"acquisition\"\ngames = []\n").unwrap();
    let runtime = host::HostRuntime::from_paths_with_backend(
        &config,
        Some(root.path().into()),
        root.path().join("private"),
        Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
    )
    .with_route_registry(root.path().into(), registry);
    runtime.claim_save_owner();
    let app = secure_host_routers(
        runtime,
        &root.path().join("private"),
        Some(PortalAccess::new(
            "acquisition-test",
            "http://127.0.0.1:8099",
            PortalPermission::LocalSessions,
        )),
    )
    .0;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/rpc", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let client = reqwest::Client::new();
    let call = |method: &'static str, payload: Value, token: &'static str| {
        let client = client.clone();
        let endpoint = endpoint.clone();
        async move {
            client
                .post(endpoint)
                .bearer_auth(token)
                .header("Origin", "http://127.0.0.1:8099")
                .json(&json!({"_tag":method, "payload":payload}))
                .send()
                .await
                .unwrap()
        }
    };
    assert_eq!(
        call("app.acquisition.status", json!({}), "wrong")
            .await
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let status = call(
        "app.acquisition.status",
        json!({"gameId":"wario-land-4"}),
        "acquisition-test",
    )
    .await
    .json::<Value>()
    .await
    .unwrap();
    assert_eq!(
        status["outcome"]["payload"]["artifacts"][0]["state"]["_tag"],
        "Missing"
    );
    let reserved = call(
        "app.session.reserve",
        json!({"gameId":"wario-land-4"}),
        "acquisition-test",
    )
    .await
    .json::<Value>()
    .await
    .unwrap();
    let launch_id = &reserved["outcome"]["payload"]["launchId"];
    let refused = call(
        "app.session.start",
        json!({"gameId":"wario-land-4", "expectedLaunchId":launch_id}),
        "acquisition-test",
    )
    .await
    .json::<Value>()
    .await
    .unwrap();
    assert_eq!(
        refused["outcome"]["payload"]["code"], "ArtifactsNotReady",
        "{refused}"
    );
    let start = json!({"artifactIds":["@test:runtime/payload"]});
    let started = call("app.acquisition.start", start.clone(), "acquisition-test")
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let failed = call("app.acquisition.status", json!({}), "acquisition-test")
                .await
                .json::<Value>()
                .await
                .unwrap();
            if failed["outcome"]["payload"]["artifacts"][0]["state"]["_tag"] == "Failed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let downloads = root.path().join("cache/runtime/payload/.downloads");
    fs::write(downloads.join(&digest), b"runtime").unwrap();
    call("app.acquisition.start", start, "acquisition-test").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let value = call("app.acquisition.status", json!({}), "acquisition-test")
                .await
                .json::<Value>()
                .await
                .unwrap();
            if value["outcome"]["payload"]["artifacts"][0]["state"]["_tag"] == "Ready" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        fs::read(root.path().join("cache/runtime/payload/v1/private")).unwrap(),
        b"runtime"
    );
    let reserved = call(
        "app.session.reserve",
        json!({"gameId":"wario-land-4"}),
        "acquisition-test",
    )
    .await
    .json::<Value>()
    .await
    .unwrap();
    let started = call("app.session.start", json!({"gameId":"wario-land-4", "expectedLaunchId":reserved["outcome"]["payload"]["launchId"]}), "acquisition-test").await.json::<Value>().await.unwrap();
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    server.abort();
}

#[tokio::test]
async fn acquisition_status_resolves_the_same_explicit_runner_as_session_start() {
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    async fn call(app: &Router, tag: &str, payload: Value) -> Value {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .extension(axum::extract::ConnectInfo(
                        "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
                    ))
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer acquisition-test")
                    .header("Origin", "http://127.0.0.1:8099")
                    .body(Body::from(
                        json!({"_tag":tag, "payload":payload}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        serde_json::from_slice(&body).unwrap()
    }

    let root = tempfile::tempdir().unwrap();
    plugin_test_fixtures::write_gba_library(root.path());
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let mut packages = Vec::new();
    for name in ["first", "second"] {
        let package = root.path().join(name);
        fs::create_dir(&package).unwrap();
        fs::write(package.join("plugin.ts"), format!(r#"
            export const name = '{name}';
            export const systems = {{gba:{{id:'gba',content:{{extensions:['gba']}}}}}};
            export const artifacts = {{payload:{{id:'@test:{name}/payload',version:'v1',files:[{{url:'https://example.com/{name}',sha256:'sha256:{}'}}]}}}};
            export const runners = {{run:{{id:'@test:{name}/run',program:'run',systems:['gba'],artifacts:['@test:{name}/payload']}}}};
            export const handlers = {{'launch.prepare': input => ({{command:input.program,args:[]}})}};
        "#, "a".repeat(64))).unwrap();
        packages.push(plugin_installation::EnabledPackage {
            id: format!("@test:{name}"),
            package: package.clone(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: vec![],
            hosts: vec![],
            files: BTreeMap::from([("run".into(), package.join("run"))]),
        });
    }
    let registry = plugin::PluginRegistry::from_installed(packages).unwrap();
    let config = root.path().join("host.toml");
    fs::write(&config, "label = \"acquisition\"\ngames = []\n").unwrap();
    let runtime = host::HostRuntime::from_paths_with_backend(
        &config,
        Some(root.path().into()),
        root.path().join("private"),
        Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
    )
    .with_route_registry(root.path().into(), registry);
    runtime.claim_save_owner();
    let app = secure_host_routers(
        runtime,
        &root.path().join("private"),
        Some(PortalAccess::new(
            "acquisition-test",
            "http://127.0.0.1:8099",
            PortalPermission::LocalSessions,
        )),
    )
    .0;

    for name in ["first", "second"] {
        let status = call(
            &app,
            "app.acquisition.status",
            json!({"gameId":"wario-land-4", "runnerId":format!("@test:{name}/run")}),
        )
        .await;
        assert_eq!(status["outcome"]["_tag"], "Ok", "{status}");
        let artifacts = status["outcome"]["payload"]["artifacts"]
            .as_array()
            .unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0]["id"], format!("@test:{name}/payload"));
    }
    // Omitted selection and an unavailable explicit selection fail identically
    // at both entry points, before readiness is checked or anything launches.
    for runner in [None, Some("@test:absent/run")] {
        let mut payload = json!({"gameId":"wario-land-4"});
        if let Some(runner) = runner {
            payload["runnerId"] = json!(runner);
        }
        let status = call(&app, "app.acquisition.status", payload.clone()).await;
        assert_eq!(status["outcome"]["_tag"], "Err", "{status}");
        let reservation = call(
            &app,
            "app.session.reserve",
            json!({"gameId":"wario-land-4"}),
        )
        .await;
        assert_eq!(reservation["outcome"]["_tag"], "Ok", "{reservation}");
        payload["expectedLaunchId"] = reservation["outcome"]["payload"]["launchId"].clone();
        let start = call(&app, "app.session.start", payload).await;
        assert_eq!(status["outcome"], start["outcome"]);
    }
}

/// The six game acquisition methods decode at the production `/rpc` boundary
/// with the portal capability. A device with no installed library answers
/// each with `AcquisitionUnavailable`, and discovery with
/// `OperationUnsupported`, never a panic.
#[tokio::test]
async fn game_acquisition_methods_decode_and_need_a_device_library() {
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("host.toml");
    fs::write(&config, "label = \"acquisition\"\ngames = []\n").unwrap();
    // ADR 0017: a host with no installed library root has no discovery and
    // no store; one with a library runs both (provider_tests.rs).
    let runtime = host::HostRuntime::from_paths_with_backend(
        &config,
        None,
        root.path().join("private"),
        Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
    );
    let app = secure_host_routers(
        runtime,
        &root.path().join("private"),
        Some(PortalAccess::new(
            "acquisition-test",
            "http://127.0.0.1:8099",
            PortalPermission::LocalSessions,
        )),
    )
    .0;
    for (tag, payload) in [
        ("app.acquisition.providers", json!({})),
        (
            "app.acquisition.search",
            json!({"query":"2048","providerIds":["@test:shop"]}),
        ),
        (
            "app.acquisition.details",
            json!({"providerId":"@test:shop","id":"2048"}),
        ),
        (
            "app.acquisition.acquire",
            json!({"providerId":"@test:shop","id":"2048"}),
        ),
        ("app.acquisition.acquire-status", json!({"jobId":"x"})),
        ("app.acquisition.acquire-cancel", json!({"jobId":"x"})),
        ("app.library.folders.snapshot", json!({})),
        ("app.attention.list", json!({})),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .extension(axum::extract::ConnectInfo(
                        "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
                    ))
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer acquisition-test")
                    .header("Origin", "http://127.0.0.1:8099")
                    .body(Body::from(
                        json!({"_tag":tag, "payload":payload}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{tag}");
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["outcome"]["_tag"], "Err", "{tag}: {body}");
        let code = if tag.starts_with("app.acquisition.") {
            "AcquisitionUnavailable"
        } else {
            "OperationUnsupported"
        };
        assert_eq!(body["outcome"]["payload"]["code"], code, "{tag}");
    }
}
