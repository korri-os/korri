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
    let registry =
        plugin::PluginRegistry::from_installed(vec![plugin_installation::EnabledPackage {
            id: "@test:runtime".into(),
            package: package.clone(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: vec![],
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
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
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
    tokio::time::sleep(Duration::from_millis(25)).await;
    let failed = call("app.acquisition.status", json!({}), "acquisition-test")
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        failed["outcome"]["payload"]["artifacts"][0]["state"]["_tag"],
        "Failed"
    );
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
