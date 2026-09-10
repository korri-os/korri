use crate as korrid;
use crate::config::test_fixtures as readable;
use std::{os::unix::fs::PermissionsExt, time::Duration};

use korrid::{
    local_server_capability, start_local_server_for_platform, stop_local_server,
    verify_local_launch_spec,
};
use reqwest::Client;
use serde_json::json;

const CHECKPOINT_LIBRARY: &str =
    include_str!("../../../docs/research/retroarch-plugin-route/catalog/games.yaml");

struct StopServer;

impl Drop for StopServer {
    fn drop(&mut self) {
        let _ = stop_local_server();
    }
}

async fn rpc(
    client: &Client,
    url: &str,
    capability: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    for _ in 0..20 {
        match client
            .post(url)
            .bearer_auth(capability)
            .json(&body)
            .send()
            .await
        {
            Ok(response) => return response.json::<serde_json::Value>().await.unwrap(),
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
    panic!("korrid RPC did not become reachable");
}

#[test]
fn protected_rpc_lists_and_launches_the_checkpoint_android_route_from_retained_config() {
    let _guard = crate::tests::embedded_server_guard();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(android_route_cases());
}

async fn android_route_cases() {
    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();

    let private = tempfile::tempdir().unwrap();
    std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let port = start_local_server_for_platform(
        "https://portal.example",
        root.path().to_str().expect("UTF-8 temp path"),
        private.path().to_str().expect("UTF-8 temp path"),
        crate::NativePlatform::EmbeddedAndroid,
    )
    .unwrap();
    let _stop = StopServer;
    let capability = local_server_capability().unwrap();
    let client = Client::new();
    let url = format!("http://127.0.0.1:{port}/rpc");

    let listed = rpc(
        &client,
        &url,
        &capability,
        json!({ "_tag": "app.local-games.list", "payload": {} }),
    )
    .await;
    assert_eq!(listed["outcome"]["_tag"], "Ok");
    assert_eq!(
        listed["outcome"]["payload"]["games"][0]["id"],
        "01K4J6K8Y00000000000000001"
    );
    assert_eq!(
        listed["outcome"]["payload"]["games"][0]["system"],
        "Android"
    );
    assert_eq!(
        listed["outcome"]["payload"]["games"][1]["id"],
        readable::GBA_ID
    );
    assert!(listed["outcome"]["payload"].get("failures").is_none());

    readable::write(
        root.path().join("catalog/games.yaml"),
        "library:\n  bad id:\n    releases: []\n",
    )
    .unwrap();
    let stale_list = rpc(
        &client,
        &url,
        &capability,
        json!({ "_tag": "app.local-games.list", "payload": {} }),
    )
    .await;
    assert_eq!(stale_list["outcome"]["_tag"], "Ok");
    assert_eq!(
        stale_list["outcome"]["payload"]["games"][0]["id"],
        "01K4J6K8Y00000000000000001"
    );
    assert_eq!(
        stale_list["outcome"]["payload"]["failures"][0]["code"],
        "LocalConfigReloadFailed"
    );
    assert!(stale_list["outcome"]["payload"]["failures"][0]["message"]
        .as_str()
        .unwrap()
        .contains("catalog/games.yaml"));
    assert!(!stale_list["outcome"]["payload"]["failures"][0]["message"]
        .as_str()
        .unwrap()
        .contains(root.path().to_str().unwrap()));

    let launched = rpc(
        &client,
        &url,
        &capability,
        json!({
            "_tag": "app.local-games.launch",
            "payload": { "gameId": "01K4J6K8Y00000000000000001" }
        }),
    )
    .await;
    assert_eq!(launched["outcome"]["_tag"], "Ok");
    let spec = &launched["outcome"]["payload"];
    assert_eq!(spec["launcherId"], "android-app");
    assert_eq!(spec["component"]["packageName"], "com.playdigious.tmnt");
    assert_eq!(spec["component"]["className"], "");
    assert_eq!(spec["extras"], json!({}));
    assert_eq!(spec["directories"], json!([]));
    assert_eq!(spec["files"], json!([]));
    assert!(verify_local_launch_spec(
        &serde_json::to_string(spec).unwrap()
    ));

    readable::write(root.path().join("catalog/games.yaml"), CHECKPOINT_LIBRARY).unwrap();
    let recovered = rpc(
        &client,
        &url,
        &capability,
        json!({ "_tag": "app.local-games.list", "payload": {} }),
    )
    .await;
    assert!(recovered["outcome"]["payload"].get("failures").is_none());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o000)).unwrap();
        let unauthorized_list = rpc(
            &client,
            &url,
            &capability,
            json!({ "_tag": "app.local-games.list", "payload": {} }),
        )
        .await;
        let unauthorized_launch = rpc(
            &client,
            &url,
            &capability,
            json!({
                "_tag": "app.local-games.launch",
                "payload": { "gameId": "01K4J6K8Y00000000000000001" }
            }),
        )
        .await;
        let unauthorized_absent_launch = rpc(
            &client,
            &url,
            &capability,
            json!({
                "_tag": "app.local-games.launch",
                "payload": { "gameId": "not-in-retained-snapshot" }
            }),
        )
        .await;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(unauthorized_list["outcome"]["_tag"], "Ok");
        assert_eq!(unauthorized_list["outcome"]["payload"]["games"], json!([]));
        assert_eq!(
            unauthorized_list["outcome"]["payload"]["failures"][0]["code"],
            "LocalConfigUnauthorized"
        );
        assert_eq!(unauthorized_launch["outcome"]["_tag"], "Err");
        assert_eq!(
            unauthorized_launch["outcome"]["payload"]["code"],
            "LocalConfigUnauthorized"
        );
        assert_eq!(unauthorized_absent_launch["outcome"]["_tag"], "Err");
        assert_eq!(
            unauthorized_absent_launch["outcome"]["payload"]["code"],
            "LocalGameNotFound"
        );
    }
}
