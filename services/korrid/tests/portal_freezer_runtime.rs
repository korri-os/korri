//! VM entry point for the production host router, launcher, compositor and
//! portal watcher. No launch/freezer/compositor implementations are replaced.
//! The VM supplies native units, Sway, policy, and the existing host.toml format.
use korrid::portal_access::{PortalAccess, PortalPermission};

#[tokio::test]
#[ignore = "runs as korrid.service in the portal freezer NixOS VM"]
async fn serve_native_portal_freezer_runtime() {
    let (router, _) = korrid::host_routers_with_storage_and_private(
        "/etc/freezer-host.toml",
        None::<std::path::PathBuf>,
        "/var/lib/korrid",
        Some(PortalAccess::new(
            "freezer-vm-only",
            "http://127.0.0.1:8099",
            PortalPermission::LocalSessions,
        )),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:43117")
        .await
        .unwrap();
    axum::serve(listener, router).await.unwrap();
}
