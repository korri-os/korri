//! VM entry point for the production host router, launcher, compositor and
//! shell watcher. No launch/freezer/compositor implementations are replaced.
//! The VM supplies native units, Sway, policy, and the existing host.toml format.
use korrid::surface_host_access::{SurfaceHostAccess, SurfaceHostPermission};

#[tokio::test]
#[ignore = "runs as korrid.service in the shell freezer NixOS VM"]
async fn serve_native_shell_freezer_runtime() {
    assert!(
        std::path::Path::new("/etc/freezer-host.toml").is_file(),
        "this harness requires the freezer VM configuration"
    );
    // Use the real signed owner-binding producer. The VM owns no human data,
    // and restarts keep this binding rather than changing the person each time.
    use korrid::identity::{DeviceIdentity, IdentityState, OwnerStatementStatus};
    use nostr::prelude::{EventBuilder, FinalizeEvent, Keys, Kind, Tag, Timestamp};
    let mut identity =
        DeviceIdentity::load_or_create(std::path::Path::new("/var/lib/korrid")).unwrap();
    if matches!(identity.state(), IdentityState::Unowned { .. }) {
        let owner = Keys::generate();
        let created_at = Timestamp::now();
        let template = identity
            .owner_statement_template(OwnerStatementStatus::Owned, created_at.as_secs())
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&template).unwrap();
        let tags: Vec<Vec<String>> = serde_json::from_value(value["tags"].clone()).unwrap();
        let kind = u16::try_from(value["kind"].as_u64().unwrap()).unwrap();
        let event = EventBuilder::new(Kind::Custom(kind), value["content"].as_str().unwrap())
            .tags(tags.into_iter().map(|tag| Tag::parse(tag).unwrap()))
            .custom_created_at(created_at)
            .finalize(&owner)
            .unwrap();
        identity
            .apply_signed_owner_binding(&template, &owner.public_key().to_hex(), &event.as_json())
            .unwrap();
    }
    let (router, _) = korrid::host_routers_with_storage_and_private(
        "/etc/freezer-host.toml",
        Some(std::path::PathBuf::from("/var/lib/korri")),
        "/var/lib/korrid",
        Some(SurfaceHostAccess::new(
            "freezer-vm-only",
            "http://127.0.0.1:8099",
            SurfaceHostPermission::LocalSessions,
        )),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:43117")
        .await
        .unwrap();
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();
}
