//! ADR 0035: a signed peer catalog is local, never another federation fan-out.
use crate::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Serving {
    origin: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Serving {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn serve(router: Router) -> Serving {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Serving { origin, task }
}

#[tokio::test]
async fn signed_peer_catalog_does_not_query_the_brains_federation() {
    peer_catalog_case(false).await;
}

#[tokio::test]
async fn signed_peer_catalog_preserves_own_source_runner_identity_and_stats() {
    peer_catalog_case(true).await;
}

async fn peer_catalog_case(has_local_game: bool) {
    const NOW: u64 = 1_700_000_000;
    let upstream_root = tempfile::tempdir().unwrap();
    let config = upstream_root.path().join("host.toml");
    std::fs::write(&config, "label = \"remote\"\n[[games]]\nid = \"wl4\"\ntitle = \"Wario Land 4\"\ncommand = [\"game\"]\n").unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    let upstream = serve(host_router_with_in_memory_units(&config).layer(
        axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    next.run(request).await
                }
            },
        ),
    ))
    .await;
    let registry = upstreams::UpstreamRegistry::new(vec![upstreams::UpstreamHostConfig::native(
        "remote",
        upstream.origin.clone(),
    )]);
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let caller_private = tempfile::tempdir().unwrap();
    let owner = nostr::key::Keys::generate();
    let owner_secret = owner.secret_key().to_secret_hex();
    let local_plugins = if has_local_game {
        std::fs::create_dir_all(readable.path().join("roms")).unwrap();
        std::fs::write(readable.path().join("roms/wl4.gba"), b"rom").unwrap();
        plugin_test_fixtures::write_gba_library(readable.path());
        play_log::PlayLogRepository::new(private.path())
            .record_session(
                plugin_test_fixtures::GBA_ID,
                "2026-09-04T18:30:00Z",
                12,
                None,
            )
            .unwrap();
        plugin_test_fixtures::installed_pair(readable.path())
    } else {
        plugin::PluginRegistry::new(Vec::new(), Vec::new()).unwrap()
    };
    let identity = peer_rpc::test_owned_identity(private.path(), &"05".repeat(32), &owner_secret);
    let device_key = identity.device_public_key().unwrap().to_owned();
    let (state, ()) = brain_app_state(
        "catalog-test-capability",
        "https://portal.example",
        readable.path(),
        private.path(),
        plugin_policy::RegistrySource::Selected(Arc::new(local_plugins)),
        config::snapshot::ConfigSnapshotCoordinator::new(readable.path()),
        discovery::FolderSelectionGrantStore::default(),
        Some(registry.clone()),
        None,
        None,
    );
    let portal = portal_router_from_state(state.clone());
    let peer = peer_rpc::PeerRpcServer::new_at(state, private.path(), NOW).unwrap();
    let peer = serve(peer.router(Some(portal))).await;
    let credentials =
        peer_rpc::test_owned_credentials(caller_private.path(), &"06".repeat(32), &owner_secret);
    let client = upstream_native::NativeClient::new_secure_at(
        peer.origin.clone(),
        device_key.clone(),
        credentials.clone(),
        NOW,
        "11".repeat(32),
        "22".repeat(32),
    );
    let catalog = client.catalog_snapshot().await.unwrap();
    if has_local_game {
        assert_eq!(catalog.games.len(), 1);
        let game = &catalog.games[0];
        assert_eq!(game.id, plugin_test_fixtures::GBA_ID);
        assert_eq!(game.title, "Wario Land 4");
        assert_eq!(
            game.source.device_public_key.as_deref(),
            Some(device_key.as_str())
        );
        assert!(game.source.is_local);
        assert!(game.supports_runner_selection);
        assert!(game.identity.is_some());
        assert_eq!(
            game.play_stats.as_ref().unwrap().total_playtime_seconds,
            12.0
        );
        let guest_root = tempfile::tempdir().unwrap();
        let guest_owner = nostr::key::Keys::generate();
        let guest = peer_rpc::test_owned_credentials(
            guest_root.path(),
            &"07".repeat(32),
            &guest_owner.secret_key().to_secret_hex(),
        );
        use nostr::{
            event::{EventBuilder, FinalizeEvent, Kind, Tag},
            types::Timestamp,
        };
        let pass = EventBuilder::new(Kind::Custom(authorization::PERSON_PASS_EVENT_KIND), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.person-pass:{}", "11".repeat(32))]).unwrap(),
                Tag::parse(["device", &guest.public_key().unwrap()]).unwrap(),
                Tag::parse(["tier", "guest"]).unwrap(),
                Tag::parse(["expires", &(NOW + 120).to_string()]).unwrap(),
                Tag::parse(["scope", authorization::STREAM_LAUNCH_SCOPE]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(NOW))
            .finalize(&owner)
            .unwrap()
            .as_json();
        let guest = guest.with_person_pass(Some(pass));
        let guest_catalog = upstream_native::NativeClient::new_secure_at(
            peer.origin.clone(),
            device_key.clone(),
            guest,
            NOW,
            "33".repeat(32),
            "44".repeat(32),
        )
        .catalog_snapshot()
        .await
        .unwrap();
        assert_eq!(guest_catalog.games.len(), 1);
        assert!(
            guest_catalog.games[0].play_stats.is_none(),
            "guest received the device owner's play history"
        );
    } else {
        assert!(
            catalog.games.is_empty(),
            "a stream-only device returned a peer's games"
        );
    }
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "signed peer read fanned out into the federation"
    );

    // A local selection must remain private to the local caller. Peers still
    // address this device's holder, even while its viewer is playing elsewhere.
    let prepared = registry
        .prepare_stream("wl4", Some("remote"))
        .await
        .unwrap();
    reads.store(0, Ordering::SeqCst);

    // Peer session calls also address only this device's own holder.
    // Each signed call consumes its own replay nonce.
    let status_client = upstream_native::NativeClient::new_secure_at(
        peer.origin.clone(),
        device_key.clone(),
        credentials.clone(),
        NOW,
        "55".repeat(32),
        "66".repeat(32),
    );
    let status = status_client.session_status().await.unwrap();
    assert!(matches!(
        status,
        upstream::UpstreamSessionStatus::SessionStatus { active: None }
    ));
    assert_eq!(reads.load(Ordering::SeqCst), 0, "peer status fanned out");
    let prepare_client = upstream_native::NativeClient::new_secure_at(
        peer.origin.clone(),
        device_key.clone(),
        credentials,
        NOW,
        "77".repeat(32),
        "88".repeat(32),
    );
    assert!(matches!(
        prepare_client.prepare_stream("wl4").await,
        Err(upstreams::UpstreamError::Tagged { code, .. }) if code == "OperationUnsupported"
    ));
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "peer prepare was forwarded"
    );
    let local_status = registry.session_status().await.unwrap();
    assert!(
        matches!(local_status, upstream::UpstreamSessionStatus::SessionStatus { active: Some(active) }
        if active.launch_id == prepared.session.launch_id)
    );

    assert_eq!(
        reads.load(Ordering::SeqCst),
        1,
        "local selected status must still read its holder"
    );
    reads.store(0, Ordering::SeqCst);

    // The same brain still aggregates for its own portal.
    let local: serde_json::Value = reqwest::Client::new()
        .post(format!("{}/rpc", peer.origin))
        .bearer_auth("catalog-test-capability")
        .json(&serde_json::json!({"_tag":"app.catalog.snapshot","payload":{}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        local["outcome"]["payload"]["games"][0]["title"],
        "Wario Land 4"
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}
