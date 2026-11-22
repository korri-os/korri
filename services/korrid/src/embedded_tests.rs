//! Exercise the same public startup that the Android app uses.
use crate::*;

#[test]
fn embedded_stream_client_has_an_empty_local_catalog_without_linux_plugins() {
    let _guard = crate::tests::embedded_server_guard();
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            let _ = stop_local_server();
        }
    }
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let port = start_local_shell_server(
        readable.path().to_str().unwrap(),
        private.path().to_str().unwrap(),
        EmbeddedPlatform::default(),
    )
    .unwrap();
    let _stop = Stop;
    let capability = local_server_capability().unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = reqwest::Client::new();
        for tag in ["app.local-games.list", "app.catalog.snapshot"] {
            let response: serde_json::Value = client
                .post(format!("http://127.0.0.1:{port}/rpc"))
                .bearer_auth(&capability)
                .json(&serde_json::json!({"_tag": tag, "payload": {}}))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(response["outcome"]["_tag"], "Ok", "{tag}: {response}");
            assert_eq!(
                response["outcome"]["payload"]["games"]
                    .as_array()
                    .unwrap()
                    .len(),
                0
            );
        }
    });
}
