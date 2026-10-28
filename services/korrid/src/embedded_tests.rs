//! Exercise the same public startup used by the Android JNI edge.
use crate::*;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    let port = start_local_server(
        "https://portal.example",
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

#[test]
fn embedded_peer_transport_reads_and_releases_its_platform_network_source() {
    let _guard = crate::tests::embedded_server_guard();
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    let mut environment = federation::reach::Environment::linux();
    environment.interfaces = Arc::new(move || {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    });
    start_local_server(
        "https://portal.example",
        readable.path().to_str().unwrap(),
        private.path().to_str().unwrap(),
        EmbeddedPlatform {
            reach: Some(environment),
            ..EmbeddedPlatform::default()
        },
    )
    .unwrap();
    let observed = tokio::runtime::Runtime::new().unwrap().block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while reads.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
    });
    stop_local_server().unwrap();
    assert!(
        observed.is_ok(),
        "embedded peer transport never read its platform interface source"
    );
    let stopped = reads.load(Ordering::SeqCst);
    std::thread::sleep(federation::interfaces::ADDRESS_POLL + std::time::Duration::from_millis(50));
    assert_eq!(
        reads.load(Ordering::SeqCst),
        stopped,
        "platform callbacks survived server shutdown"
    );
}
