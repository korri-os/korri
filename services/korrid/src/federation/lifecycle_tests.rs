use crate::*;
use nostr::{event::{EventBuilder, FinalizeEvent, Kind, Tag}, key::Keys, types::Timestamp};
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

#[test]
fn binding_refreshes_existing_clients_without_rotating_port_and_stop_joins_blocked_relay() {
    let _serialized = super::embedded_server_guard();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
    struct Stop;
    impl Drop for Stop { fn drop(&mut self) { let _ = stop_local_server(); } }
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let peer_root = tempfile::tempdir().unwrap();
    let owner = Keys::generate();
    let peer = peer_rpc::test_owned_identity(peer_root.path(), &"61".repeat(32), &owner.secret_key().to_secret_hex());
    let config = readable.path().join("host.toml");
    fs::write(&config, "label = \"Peer\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n").unwrap();
    let host_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host_address = host_listener.local_addr().unwrap();
    let runtime = host::HostRuntime::from_paths_with_backend(&config, None, peer_root.path().to_owned(), Arc::new(host::control::InMemoryLaunchUnitBackend::default()));
    let router = secure_host_routers(runtime, peer_root.path(), None).0;
    let host = tokio::spawn(async move { axum::serve(host_listener, router).await.unwrap(); });
    fs::write(readable.path().join("upstreams.json"), serde_json::json!([{"kind":"native", "label":"Peer", "baseUrl":format!("http://{host_address}"), "devicePublicKey":peer.device_public_key(), "moonlightAddress":"peer:47989"}]).to_string()).unwrap();
    let relay_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    fs::write(readable.path().join("device.yaml"), format!("host:\n  relays:\n    - ws://{}\n", relay_listener.local_addr().unwrap())).unwrap();
    let port = start_local_server_for_platform("https://portal.example", readable.path().to_str().unwrap(), private.path().to_str().unwrap(), NativePlatform::Standalone).unwrap();
    let _stop = Stop;
    let capability = local_server_capability().unwrap();
    let (credentials, registry) = {
        let slot = server_slot().lock().unwrap();
        let server = slot.as_ref().unwrap();
        (server.federation.credentials.clone(), server.upstream.clone())
    };
    assert!(matches!(credentials.identity_snapshot().unwrap().state(), identity::IdentityState::Unowned { .. }));
    let now = peer_rpc::unix_time();
    let template = local_owner_binding_template(now).unwrap();
    let device = credentials.identity_snapshot().unwrap().device_public_key().unwrap().to_owned();
    let event = EventBuilder::new(Kind::Custom(30078), "").tags([
        Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
        Tag::parse(["device", &device]).unwrap(), Tag::parse(["status", "owned"]).unwrap(),
    ]).custom_created_at(Timestamp::from(now)).finalize(&owner).unwrap();
    apply_local_owner_binding(&template, &owner.public_key().to_hex(), &event.as_json()).unwrap();
    assert!(matches!(credentials.identity_snapshot().unwrap().state(), identity::IdentityState::Owned { .. }));
    assert_eq!(local_server_port(), Some(port));
    assert_eq!(local_server_capability(), Some(capability.clone()));
    assert_eq!(registry.catalog_snapshot().await.unwrap().games.len(), 1);
    let response = reqwest::Client::new().post(format!("http://127.0.0.1:{port}/rpc")).bearer_auth(&capability).json(&serde_json::json!({"_tag":"app.catalog.snapshot", "payload":{}})).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    // The relay accepts TCP but never answers the WebSocket handshake.
    let (_blocked, _) = tokio::time::timeout(Duration::from_secs(2), relay_listener.accept()).await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(2), tokio::task::spawn_blocking(stop_local_server)).await.unwrap().unwrap().unwrap();
    assert_eq!(local_server_port(), None);
    host.abort();
    let _ = host.await;
    });
}

#[tokio::test]
async fn inbound_rpc_and_directory_use_the_same_revocation_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let caller_root = tempfile::tempdir().unwrap();
    let owner = Keys::generate();
    let local = peer_rpc::test_owned_identity(root.path(), &"62".repeat(32), &owner.secret_key().to_secret_hex());
    let caller = peer_rpc::test_owned_credentials(caller_root.path(), &"63".repeat(32), &owner.secret_key().to_secret_hex());
    let caller_identity = caller.identity_snapshot().unwrap();
    let device = caller_identity.device_public_key().unwrap();
    let config = root.path().join("host.toml");
    fs::write(&config, "label = \"Peer\"\n").unwrap();
    let resources = FederationResources::open(root.path()).unwrap();
    resources.directory.apply_membership(&resources.directory.begin_work().unwrap(), &[caller_identity.owner_statement_json().unwrap()]).unwrap();
    let runtime = host::HostRuntime::from_paths_with_backend(&config, None, root.path().to_owned(), Arc::new(host::control::InMemoryLaunchUnitBackend::default()));
    let router = secure_host_routers_with_federation(runtime, root.path(), resources.clone(), None).0;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let host = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
    let client = upstream_native::NativeClient::new_secure(format!("http://{address}"), local.device_public_key().unwrap().into(), caller);
    client.catalog_snapshot().await.unwrap();
    let event = EventBuilder::new(Kind::Custom(30078), "").tags([
        Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
        Tag::parse(["device", device]).unwrap(), Tag::parse(["status", "revoked"]).unwrap(),
    ]).custom_created_at(Timestamp::from(peer_rpc::unix_time())).finalize(&owner).unwrap();
    resources.directory.apply_membership(&resources.directory.begin_work().unwrap(), &[event.as_json()]).unwrap();
    assert!(client.catalog_snapshot().await.is_err());
    assert!(resources.directory.snapshot().unwrap().is_empty());
    host.abort();
    let _ = host.await;
}
