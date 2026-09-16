#[path = "fixtures/readable.rs"]
mod readable;
use korrid::{
    config::{resolver, snapshot::ConfigSnapshotCoordinator},
    launcher::{self, FileProvisionMode},
    plugin::{load_plugin_source, PluginRegistry},
    plugin_policy::{self, PluginPolicyLayer},
};

fn state(root: &std::path::Path) -> korrid::config::snapshot::ConfigSnapshotState {
    readable::gba(root);
    ConfigSnapshotCoordinator::new(root).reload()
}

fn registry(mgba: bool, retroarch: bool) -> PluginRegistry {
    let plugins = plugin_policy::bundled_plugins().unwrap();
    let enabled = plugin_policy::resolve_enabled_plugin_ids([
        plugin_policy::bundled_plugin_policy_layer(),
        PluginPolicyLayer::from_enabled([("@korri:mgba", mgba), ("@korri:retroarch", retroarch)]),
    ]);
    PluginRegistry::new(plugins, enabled).unwrap()
}

#[test]
fn android_mgba_is_one_runner_with_a_scoped_retroarch_family() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let state = state(root.path());
    let registry = registry(true, true);
    let route = resolver::resolve_route(
        root.path(),
        &state.snapshot,
        &registry,
        [],
        readable::GBA_ID,
    )
    .unwrap();
    assert_eq!(route.runner_id, "@korri:mgba/mgba");
    assert_eq!(route.family_id.as_deref(), Some("@korri:retroarch"));
    assert_eq!(
        route.core_path.as_deref(),
        Some("/data/data/com.korri.retroarch/cores/mgba_libretro_android.so")
    );
    assert_eq!(
        route.android_component.as_ref().unwrap().package_name,
        "com.korri.retroarch"
    );
    let spec = launcher::launch_game(
        root.path(),
        readable::GBA_ID,
        FileProvisionMode::Deferred,
        &state,
        &registry,
        50000,
    )
    .unwrap();
    assert_eq!(spec.runner_id, "retroarch");
    assert_eq!(
        spec.extras["LIBRETRO"],
        "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so"
    );
}

#[test]
fn family_enablement_does_not_supply_or_replace_runner_code() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let state = state(root.path());
    let without_mgba = registry(false, true);
    assert!(without_mgba.families().contains_key("@korri:retroarch"));
    assert!(!without_mgba.runners().contains_key("@korri:mgba/mgba"));
    assert!(resolver::resolve_route(
        root.path(),
        &state.snapshot,
        &without_mgba,
        [],
        readable::GBA_ID,
    )
    .is_err());
    let without_family_plugin = registry(true, false);
    assert!(without_family_plugin
        .runners()
        .contains_key("@korri:mgba/mgba"));
    assert!(resolver::resolve_route(
        root.path(),
        &state.snapshot,
        &without_family_plugin,
        [],
        readable::GBA_ID,
    )
    .is_ok());
}

#[test]
fn linux_does_not_fall_back_to_bundled_android_runner() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let state = state(root.path());
    assert!(resolver::resolve_route_for_platform(
        root.path(),
        &state.snapshot,
        &registry(true, true),
        [],
        readable::GBA_ID,
        resolver::RoutePlatform::Linux,
    )
    .is_err());
}

#[test]
fn malformed_android_runner_fields_fail_at_declaration_admission() {
    let source = plugin_policy::MGBA_PLUGIN_SOURCE;
    for malformed in [
        source.replace("family: \"@korri:retroarch\"", "family: \"retroarch\""),
        source.replace(
            "packageName: \"com.korri.retroarch\"",
            "packageName: \"bad/name\"",
        ),
        source.replace("command: \"retroarch\"", "program: \"retroarch\""),
    ] {
        assert!(load_plugin_source("@korri", &malformed).is_err());
    }
}
