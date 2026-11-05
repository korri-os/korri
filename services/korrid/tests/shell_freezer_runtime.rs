//! VM entry point for the production host router, launcher, compositor and
//! shell watcher. No launch/freezer/compositor implementations are replaced.
//! The VM supplies native units, Sway, policy, and the existing host.toml format.
use korrid::surface_host_access::{SurfaceHostAccess, SurfaceHostPermission};

#[tokio::test]
#[ignore = "runs as korrid.service in the shell freezer NixOS VM"]
async fn serve_native_shell_freezer_runtime() {
    let (router, _) = korrid::host_routers_with_storage_and_private(
        "/etc/freezer-host.toml",
        None::<std::path::PathBuf>,
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
