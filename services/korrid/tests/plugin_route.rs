#[path = "fixtures/readable.rs"]
mod readable;
use korrid::{
    config::{
        resolver::{resolve_launchable_routes, resolve_route, RouteDiagnosticCode},
        snapshot::ConfigSnapshotCoordinator,
        ConfigSnapshot,
    },
    plugin::PluginRegistry,
    plugin_policy::{
        bundled_plugin_policy_layer, bundled_plugins, resolve_enabled_plugin_ids, PluginPolicyLayer,
    },
};
use readable::*;

fn checkpoint_snapshot(root: &std::path::Path) -> std::sync::Arc<ConfigSnapshot> {
    android(root);
    let state = ConfigSnapshotCoordinator::new(root).reload();
    assert_eq!(state.diagnostic, None);
    state.snapshot
}

fn registry(enabled: bool) -> PluginRegistry {
    PluginRegistry::new(
        bundled_plugins().unwrap(),
        resolve_enabled_plugin_ids([
            bundled_plugin_policy_layer(),
            PluginPolicyLayer::from_enabled([("@korri:android-app", enabled)]),
        ]),
    )
    .unwrap()
}

#[test]
fn checkpoint_route_resolves_through_one_android_runner() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = checkpoint_snapshot(root.path());
    let registry = registry(true);
    let catalog = resolve_launchable_routes(root.path(), &snapshot, &registry, ["wl4"]);
    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    let route = resolve_route(root.path(), &snapshot, &registry, [], ANDROID_ID).unwrap();
    assert_eq!(route.runner_id, "@korri:android-app/android-app");
    assert_eq!(route.family_id.as_deref(), Some("@korri:android-app"));
    assert_eq!(route.integration_token, "android-app");
    assert_eq!(
        route.flattened_target,
        "@korri:android-app:com.playdigious.tmnt"
    );
}

#[test]
fn disabled_runner_is_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = checkpoint_snapshot(root.path());
    let error =
        resolve_route(root.path(), &snapshot, &registry(false), [], ANDROID_ID).unwrap_err();
    assert_eq!(error.code, RouteDiagnosticCode::LocalRouteUnavailable);
    assert_eq!(error.playable_id.as_deref(), Some(ANDROID_ID));
}

#[test]
fn dynamic_playable_collision_keeps_the_static_owner() {
    let root = tempfile::tempdir().unwrap();
    let catalog = resolve_launchable_routes(
        root.path(),
        &checkpoint_snapshot(root.path()),
        &registry(true),
        [ANDROID_ID],
    );
    assert!(catalog.routes.is_empty());
    assert_eq!(
        catalog.diagnostics[0].code,
        RouteDiagnosticCode::LocalRouteCollision
    );
}

#[test]
fn unsupported_target_does_not_invent_a_process_runner() {
    let root = tempfile::tempdir().unwrap();
    let mut device: serde_yaml::Value = serde_yaml::from_str(ANDROID_DEVICE).unwrap();
    device["locations"][ANDROID_RELEASE] =
        serde_yaml::from_str("- value: https://example.invalid/game").unwrap();
    let snapshot = korrid::config::decode_config_documents(
        &serde_yaml::to_string(&device).unwrap(),
        ANDROID_GAMES,
        ANDROID_RELEASES,
    )
    .unwrap();
    let error = resolve_route(root.path(), &snapshot, &registry(true), [], ANDROID_ID).unwrap_err();
    assert_eq!(error.code, RouteDiagnosticCode::LocalRouteUnavailable);
}
