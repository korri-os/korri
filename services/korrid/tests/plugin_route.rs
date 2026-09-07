#[path = "fixtures/readable.rs"]
mod readable;
use korrid::{
    config::{
        decode_config_documents,
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
fn copied_records() -> ConfigSnapshot {
    let device = format!("{ANDROID_DEVICE}\nproviders:\n  '@korri:android-app': {{title: Copied}}\nsystems:\n  android: {{title: Copied}}\nlaunchers:\n  '@korri:android-app/android-app':\n    plugin: '@korri:android-app'\n    command: android-app\n    systems: [android]\n");
    let mut device: serde_yaml::Value = serde_yaml::from_str(&device).unwrap();
    let mut games: serde_yaml::Value = serde_yaml::from_str(ANDROID_GAMES).unwrap();
    let mut releases: serde_yaml::Value = serde_yaml::from_str(ANDROID_RELEASES).unwrap();
    device["providers"]["@korri:other"] = serde_yaml::from_str("title: Other").unwrap();
    device["systems"]["other-system"] = serde_yaml::from_str("title: Other System").unwrap();
    device["launchers"]["@korri:other/android-app"] = serde_yaml::from_str(
        "plugin: '@korri:other'\ncommand: android-app\nsystems: [other-system]",
    )
    .unwrap();
    device["locations"]["@korri:other/package.name"] =
        serde_yaml::from_str("- provider: '@korri:other'\n  ref: package.name").unwrap();
    games["games"][OTHER_ID] =
        serde_yaml::from_str("title: Other Game\nreleases: ['@korri:other/package.name']").unwrap();
    releases["releases"]["@korri:other/package.name"] =
        serde_yaml::from_str(&format!("game: {OTHER_ID}\nsystem: other-system")).unwrap();
    decode_config_documents(
        &serde_yaml::to_string(&device).unwrap(),
        &serde_yaml::to_string(&games).unwrap(),
        &serde_yaml::to_string(&releases).unwrap(),
    )
    .unwrap()
}

#[test]
fn checkpoint_route_resolves_through_the_default_enabled_registry() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = checkpoint_snapshot(root.path());
    let registry = registry(true);
    let catalog = resolve_launchable_routes(root.path(), &snapshot, &registry, ["wl4"]);
    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    assert_eq!(catalog.routes.len(), 1);
    let route = resolve_route(root.path(), &snapshot, &registry, [], ANDROID_ID).unwrap();
    assert_eq!(route, catalog.routes[0]);
    assert_eq!(route.playable_id, ANDROID_ID);
    assert_eq!(route.title.as_deref(), Some("TMNT: Shredder's Revenge"));
    assert_eq!(route.release_id, ANDROID_RELEASE);
    assert_eq!(route.provider_id, "@korri:android-app");
    assert_eq!(route.system_id, "android");
    assert_eq!(route.system_title.as_deref(), Some("Android"));
    assert_eq!(route.launcher_id, "@korri:android-app/android-app");
    assert_eq!(route.launcher_kind, "@korri:android-app");
    assert_eq!(route.integration_token, "android-app");
}

#[test]
fn checkpoint_route_is_unavailable_when_the_plugin_is_disabled() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = checkpoint_snapshot(root.path());
    let catalog = resolve_launchable_routes(root.path(), &snapshot, &registry(false), []);
    assert!(catalog.routes.is_empty());
    assert_eq!(catalog.diagnostics.len(), 1);
    assert_eq!(
        catalog.diagnostics[0].playable_id.as_deref(),
        Some(ANDROID_ID)
    );
    assert_eq!(
        resolve_route(root.path(), &snapshot, &registry(false), [], ANDROID_ID)
            .unwrap_err()
            .code,
        RouteDiagnosticCode::LocalRouteUnavailable
    );
}

#[test]
fn disabled_registered_plugin_rejects_copied_records_without_blocking_device_routes() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = copied_records();
    let catalog = resolve_launchable_routes(root.path(), &snapshot, &registry(false), []);
    assert_eq!(catalog.routes.len(), 1);
    assert_eq!(catalog.routes[0].playable_id, OTHER_ID);
    assert_eq!(
        catalog.routes[0].flattened_target,
        "@korri:other:package.name"
    );
    assert_eq!(catalog.diagnostics.len(), 1);
    assert_eq!(
        catalog.diagnostics[0].code,
        RouteDiagnosticCode::LocalRouteUnavailable
    );
    let error =
        resolve_route(root.path(), &snapshot, &registry(false), [], ANDROID_ID).unwrap_err();
    assert!(!error.message.contains("process fallback"));
    assert_eq!(
        resolve_route(root.path(), &snapshot, &registry(false), [], OTHER_ID)
            .unwrap()
            .system_id,
        "other-system"
    );
}

#[test]
fn route_resolution_fails_closed_for_missing_system_and_unsupported_locations() {
    let root = tempfile::tempdir().unwrap();
    let cases = [
        (
            ANDROID_DEVICE.to_owned(),
            ANDROID_RELEASES.replace("system: android", "system: switch"),
            "no launcher supports system switch",
        ),
        (
            format!(
                "locations:\n  '{ANDROID_RELEASE}':\n    - value: https://example.invalid/tmnt\n"
            ),
            ANDROID_RELEASES.into(),
            "target kind url is not supported",
        ),
        ("{}".into(), ANDROID_RELEASES.into(), "no complete location"),
    ];
    for (device, releases, message) in cases {
        let snapshot = decode_config_documents(&device, ANDROID_GAMES, &releases).unwrap();
        let error =
            resolve_route(root.path(), &snapshot, &registry(true), [], ANDROID_ID).unwrap_err();
        assert_eq!(error.code, RouteDiagnosticCode::LocalRouteUnavailable);
        assert!(error.message.contains(message), "{}", error.message);
    }
}

#[test]
fn unavailable_provider_never_uses_a_registered_launcher_as_authority() {
    let root = tempfile::tempdir().unwrap();
    let mut snapshot = copied_records();
    snapshot.providers.remove("@korri:other");
    let error = resolve_route(root.path(), &snapshot, &registry(false), [], OTHER_ID).unwrap_err();
    assert!(error
        .message
        .contains("provider @korri:other is unavailable"));
}

#[test]
fn unsupported_launcher_command_never_falls_back_to_a_process() {
    let root = tempfile::tempdir().unwrap();
    let mut snapshot = copied_records();
    snapshot
        .launchers
        .get_mut("@korri:other/android-app")
        .unwrap()
        .command = Some(korrid::config::NonEmptyString("sh".into()));
    assert!(
        resolve_route(root.path(), &snapshot, &registry(false), [], OTHER_ID)
            .unwrap_err()
            .message
            .contains("command sh is not supported")
    );
}

#[test]
fn launcher_without_plugin_kind_never_uses_process_fallback() {
    let root = tempfile::tempdir().unwrap();
    let mut snapshot = copied_records();
    snapshot
        .launchers
        .get_mut("@korri:other/android-app")
        .unwrap()
        .plugin = None;
    assert!(
        resolve_route(root.path(), &snapshot, &registry(false), [], OTHER_ID)
            .unwrap_err()
            .message
            .contains("process fallback is not supported")
    );
}

#[test]
fn provider_ref_target_preserves_the_provider_identity_separator() {
    let root = tempfile::tempdir().unwrap();
    let route = resolve_route(
        root.path(),
        &checkpoint_snapshot(root.path()),
        &registry(true),
        [],
        ANDROID_ID,
    )
    .unwrap();
    assert_eq!(
        route.flattened_target,
        "@korri:android-app:com.playdigious.tmnt"
    );
    assert!(route
        .flattened_target
        .starts_with(&format!("{}:", route.provider_id)));
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
    assert_eq!(catalog.diagnostics.len(), 1);
    assert_eq!(
        catalog.diagnostics[0].code,
        RouteDiagnosticCode::LocalRouteCollision
    );
    assert!(catalog.diagnostics[0]
        .message
        .contains("static route remains active"));
}

#[test]
fn direct_dynamic_playable_collision_keeps_the_static_owner() {
    let root = tempfile::tempdir().unwrap();
    let error = resolve_route(
        root.path(),
        &checkpoint_snapshot(root.path()),
        &registry(true),
        [ANDROID_ID],
        ANDROID_ID,
    )
    .unwrap_err();
    assert_eq!(error.code, RouteDiagnosticCode::LocalRouteCollision);
    assert_eq!(error.playable_id.as_deref(), Some(ANDROID_ID));
}

#[test]
fn device_plugin_collisions_omit_only_affected_routes() {
    let root = tempfile::tempdir().unwrap();
    let catalog = resolve_launchable_routes(root.path(), &copied_records(), &registry(true), []);
    assert_eq!(catalog.routes.len(), 1);
    assert_eq!(catalog.routes[0].playable_id, OTHER_ID);
    assert_eq!(catalog.diagnostics.len(), 1);
    assert_eq!(
        catalog.diagnostics[0].code,
        RouteDiagnosticCode::LocalRouteCollision
    );
    assert_eq!(
        resolve_route(
            root.path(),
            &copied_records(),
            &registry(true),
            [],
            ANDROID_ID
        )
        .unwrap_err()
        .code,
        RouteDiagnosticCode::LocalRouteCollision
    );
}

#[test]
fn ambiguous_launchers_are_not_resolved_by_map_order() {
    let root = tempfile::tempdir().unwrap();
    let mut snapshot = copied_records();
    let launcher = snapshot.launchers["@korri:other/android-app"].clone();
    snapshot
        .launchers
        .insert("@korri:other/second".into(), launcher);
    assert!(
        resolve_route(root.path(), &snapshot, &registry(false), [], OTHER_ID)
            .unwrap_err()
            .message
            .contains("ambiguous launchers")
    );
}
