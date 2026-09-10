#[path = "fixtures/readable.rs"]
mod readable;
use std::path::Path;

use korrid::{
    config::{resolver, snapshot::ConfigSnapshotCoordinator},
    launcher::{self, FileProvisionMode},
    plugin::{load_plugin_source, PluginRegistry},
    plugin_policy::{
        bundled_plugin_policy_layer, bundled_plugins, resolve_enabled_plugin_ids,
        PluginPolicyLayer, ANDROID_APP_PLUGIN_SOURCE, MGBA_PLUGIN_SOURCE, RETROARCH_PLUGIN_SOURCE,
    },
};

fn state(root: &Path) -> korrid::config::snapshot::ConfigSnapshotState {
    readable::combined(root);
    ConfigSnapshotCoordinator::new(root).reload()
}

fn registry(retroarch_enabled: bool, mgba_enabled: bool) -> PluginRegistry {
    let layers = [
        bundled_plugin_policy_layer(),
        PluginPolicyLayer::from_enabled([
            ("@korri:retroarch", retroarch_enabled),
            ("@korri:mgba", mgba_enabled),
        ]),
    ];
    PluginRegistry::new(
        bundled_plugins().unwrap(),
        resolve_enabled_plugin_ids(layers),
    )
    .unwrap()
}

fn registry_from_sources(retroarch_source: &str, mgba_source: &str) -> PluginRegistry {
    PluginRegistry::new(
        vec![
            load_plugin_source("@korri", ANDROID_APP_PLUGIN_SOURCE).unwrap(),
            load_plugin_source("@korri", retroarch_source).unwrap(),
            load_plugin_source("@korri", mgba_source).unwrap(),
        ],
        resolve_enabled_plugin_ids([
            bundled_plugin_policy_layer(),
            PluginPolicyLayer::from_enabled([("@korri:moonlight", false)]),
        ]),
    )
    .unwrap()
}

fn state_from_docs(
    root: &Path,
    config: &str,
    library: &str,
) -> korrid::config::snapshot::ConfigSnapshotState {
    readable::install(
        root,
        &format!("{config}\n{library}"),
        &readable::gba_games(),
        &readable::gba_releases(),
    );
    ConfigSnapshotCoordinator::new(root).reload()
}

fn explicit_storage_config(storage_id: &str, storage_root: &Path) -> String {
    format!(
        "host:\n  title: usu\nstorage:\n  {storage_id}:\n    root: {}\n",
        storage_root.display()
    )
}

fn explicit_storage_library(storage_id: &str, target_path: &str, discovery: bool) -> String {
    readable::gba_locations(storage_id, target_path, discovery)
}

#[test]
fn explicit_storage_root_file_target_launches_through_retroarch() {
    let korri_root = tempfile::tempdir().unwrap();
    let selected_root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(selected_root.path().join("gba")).unwrap();
    std::fs::write(selected_root.path().join("gba/wl4.gba"), b"rom").unwrap();
    let state = state_from_docs(
        korri_root.path(),
        &explicit_storage_config("selected-gba", selected_root.path()),
        &explicit_storage_library("selected-gba", "gba/wl4.gba", false),
    );
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);

    let route = resolver::resolve_route(
        korri_root.path(),
        &state.snapshot,
        &registry(true, true),
        [],
        readable::GBA_ID,
    )
    .expect("explicit storage route");
    assert_eq!(
        route.file_target.as_ref().unwrap().storage_id,
        "selected-gba"
    );

    let spec = launcher::launch_game(
        korri_root.path(),
        readable::GBA_ID,
        FileProvisionMode::Deferred,
        &state,
        &registry(true, true),
        50000,
    )
    .expect("explicit storage launch spec");

    assert_eq!(
        spec.extras.get("ROM").map(String::as_str),
        Some(
            selected_root
                .path()
                .join("gba/wl4.gba")
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
        )
    );
    assert_eq!(
        spec.authorized_content_root.as_deref(),
        Some(
            selected_root
                .path()
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
        )
    );
    assert!(spec
        .directories
        .iter()
        .all(|path| path.starts_with(korri_root.path().to_str().unwrap())));
    assert!(spec
        .files
        .iter()
        .all(|file| file.path.starts_with(korri_root.path().to_str().unwrap())));
}

#[test]
fn discovery_metadata_on_file_targets_does_not_change_launch_resolution() {
    let korri_root = tempfile::tempdir().unwrap();
    let selected_root = tempfile::tempdir().unwrap();
    std::fs::write(selected_root.path().join("wl4.gba"), b"rom").unwrap();
    let config = explicit_storage_config("selected-gba", selected_root.path());
    let without_discovery = state_from_docs(
        korri_root.path(),
        &config,
        &explicit_storage_library("selected-gba", "wl4.gba", false),
    );
    let with_discovery = state_from_docs(
        korri_root.path(),
        &config,
        &explicit_storage_library("selected-gba", "wl4.gba", true),
    );

    let plain = resolver::resolve_route(
        korri_root.path(),
        &without_discovery.snapshot,
        &registry(true, true),
        [],
        readable::GBA_ID,
    )
    .expect("plain file target");
    let discovered = resolver::resolve_route(
        korri_root.path(),
        &with_discovery.snapshot,
        &registry(true, true),
        [],
        readable::GBA_ID,
    )
    .expect("discovery metadata file target");

    assert_eq!(discovered.flattened_target, plain.flattened_target);
    assert_eq!(discovered.file_target, plain.file_target);
}

#[test]
fn explicit_storage_root_failures_are_route_diagnostics() {
    let korri_root = tempfile::tempdir().unwrap();
    let missing_root = korri_root.path().join("missing-selected-root");
    let file_root = tempfile::NamedTempFile::new().unwrap();

    for (storage_id, config, expected) in [
        (
            "absent-storage",
            explicit_storage_config("selected-gba", korri_root.path()),
            "storage absent-storage is unavailable",
        ),
        (
            "selected-gba",
            "host:\n  title: usu\nstorage:\n  selected-gba:\n    root: relative/path\n".to_owned(),
            "storage selected-gba root is not absolute",
        ),
        (
            "selected-gba",
            explicit_storage_config("selected-gba", &missing_root),
            "storage selected-gba root is unavailable",
        ),
        (
            "selected-gba",
            explicit_storage_config("selected-gba", file_root.path()),
            "storage selected-gba root is not a directory",
        ),
    ] {
        let state = state_from_docs(
            korri_root.path(),
            &config,
            &explicit_storage_library(storage_id, "wl4.gba", false),
        );
        let error = resolver::resolve_route(
            korri_root.path(),
            &state.snapshot,
            &registry(true, true),
            [],
            readable::GBA_ID,
        )
        .expect_err("invalid storage root should produce a route diagnostic");
        assert!(
            error.message.contains(expected),
            "expected {expected:?}, got {:?}",
            error.message
        );
    }
}

#[cfg(unix)]
#[test]
fn explicit_storage_rejects_parent_and_symlink_target_escapes() {
    use std::os::unix::fs::symlink;

    let korri_root = tempfile::tempdir().unwrap();
    let selected_root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("escaped.gba"), b"rom").unwrap();
    symlink(
        outside.path().join("escaped.gba"),
        selected_root.path().join("linked.gba"),
    )
    .unwrap();

    for (target_path, expected) in [
        ("../escaped.gba", "file target is unsafe"),
        ("linked.gba", "escapes storage selected-gba root"),
    ] {
        let state = state_from_docs(
            korri_root.path(),
            &explicit_storage_config("selected-gba", selected_root.path()),
            &explicit_storage_library("selected-gba", target_path, false),
        );
        let error = resolver::resolve_route(
            korri_root.path(),
            &state.snapshot,
            &registry(true, true),
            [],
            readable::GBA_ID,
        )
        .expect_err("escaped target should produce a route diagnostic");
        assert!(
            error.message.contains(expected),
            "expected {expected:?}, got {:?}",
            error.message
        );
    }
}

#[test]
fn composes_retroarch_launcher_with_mgba_runtime_from_independent_plugins() {
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
    .expect("composed RetroArch and mGBA route");

    assert_eq!(route.launcher_id, "@korri:retroarch/retroarch");
    assert_eq!(
        route.identity,
        Some(korrid::GameIdentity::Hash(
            "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414".into(),
        ))
    );
    assert_eq!(route.integration_token, "retroarch");
    assert_eq!(
        route.android_component.unwrap().package_name,
        "com.korri.retroarch"
    );
    let runtime = route.runtime.expect("mGBA plugin runtime");
    assert_eq!(runtime.id, "@korri:mgba/mgba");
    assert_eq!(runtime.kind, "libretro-core");
    assert_eq!(runtime.app, "@korri:retroarch/retroarch");
    assert_eq!(
        runtime.path,
        "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so"
    );
    let target = route.file_target.expect("file target");
    assert_eq!(target.storage_id, "roms");
    assert_eq!(target.path, "wl4.gba");
}

#[test]
fn linux_never_falls_back_to_bundled_android_plugins() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let state = state(root.path());

    let route = resolver::resolve_route_for_platform(
        root.path(),
        &state.snapshot,
        &registry(true, true),
        [],
        readable::GBA_ID,
        resolver::RoutePlatform::Linux,
    )
    .unwrap_err();
    assert!(route.message.contains("no installed Linux route"));
}

#[test]
fn materializes_the_existing_launch_treaty_from_composed_plugin_records() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();

    let spec = launcher::launch_game(
        root.path(),
        readable::GBA_ID,
        FileProvisionMode::Deferred,
        &state,
        &registry(true, true),
        50000,
    )
    .expect("plugin-backed launch spec");

    assert_eq!(spec.launcher_id, "retroarch");
    assert_eq!(spec.component.package_name, "com.korri.retroarch");
    assert_eq!(
        spec.component.class_name,
        "com.retroarch.browser.retroactivity.RetroActivityFuture"
    );
    assert_eq!(
        spec.extras.get("LIBRETRO").map(String::as_str),
        Some("/data/data/com.korri.retroarch/cores/mgba_libretro_android.so")
    );
}

#[test]
fn launch_spec_and_config_follow_values_from_both_plugins() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let retroarch_source = RETROARCH_PLUGIN_SOURCE
        .replace("com.korri.retroarch", "dev.korri.sentinel")
        .replace(
            "com.retroarch.browser.retroactivity.RetroActivityFuture",
            "dev.korri.sentinel.SentinelActivity",
        );
    let mgba_source = MGBA_PLUGIN_SOURCE.replace(
        "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so",
        "/plugin/sentinel/cores/sentinel_libretro.so",
    );

    let spec = launcher::launch_game(
        root.path(),
        readable::GBA_ID,
        FileProvisionMode::Deferred,
        &state,
        &registry_from_sources(&retroarch_source, &mgba_source),
        50000,
    )
    .expect("sentinel plugin-backed launch spec");

    assert_eq!(spec.component.package_name, "dev.korri.sentinel");
    assert_eq!(
        spec.component.class_name,
        "dev.korri.sentinel.SentinelActivity"
    );
    assert_eq!(
        spec.extras.get("LIBRETRO").map(String::as_str),
        Some("/plugin/sentinel/cores/sentinel_libretro.so")
    );
    assert!(spec.files[0]
        .content
        .contains("libretro_directory = \"/plugin/sentinel/cores\""));
}

#[test]
fn disabling_retroarch_withholds_only_its_launcher_identity() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    let registry = registry(false, true);

    assert!(registry.owns_registered_launcher_id("@korri:retroarch/retroarch"));
    assert!(registry.owns_registered_runtime_id("@korri:mgba/mgba"));
    assert!(!registry
        .launchers()
        .contains_key("@korri:retroarch/retroarch"));
    assert!(registry.runtimes().contains_key("@korri:mgba/mgba"));

    let error = resolver::resolve_route(
        root.path(),
        &state.snapshot,
        &registry,
        [],
        readable::GBA_ID,
    )
    .expect_err("disabled launcher plugin route");
    assert!(error.message.contains("no launcher supports system gba"));
}

#[test]
fn disabling_mgba_withholds_only_its_system_and_runtime_identities() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    let registry = registry(true, false);

    assert!(registry
        .launchers()
        .contains_key("@korri:retroarch/retroarch"));
    assert!(registry.owns_registered_system_id("gba"));
    assert!(registry.owns_registered_runtime_id("@korri:mgba/mgba"));
    assert!(!registry.runtimes().contains_key("@korri:mgba/mgba"));

    let error = resolver::resolve_route(
        root.path(),
        &state.snapshot,
        &registry,
        [],
        readable::GBA_ID,
    )
    .expect_err("disabled core plugin route");
    assert!(error.message.contains("no launcher supports system gba"));
}

#[test]
fn rejects_a_runtime_declared_for_a_different_launcher() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    let incompatible = MGBA_PLUGIN_SOURCE.replace(
        "app: \"@korri:retroarch/retroarch\"",
        "app: \"@korri:other/launcher\"",
    );
    let registry = registry_from_sources(RETROARCH_PLUGIN_SOURCE, &incompatible);

    let error = resolver::resolve_route(
        root.path(),
        &state.snapshot,
        &registry,
        [],
        readable::GBA_ID,
    )
    .expect_err("runtime app mismatch");
    assert!(error.message.contains("no launcher supports system gba"));
}

#[test]
fn rejects_an_mgba_runtime_that_does_not_support_the_release_system() {
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    let unsupported = MGBA_PLUGIN_SOURCE.replace("systems: [\"gba\"]", "systems: [\"gb\"]");
    let registry = registry_from_sources(RETROARCH_PLUGIN_SOURCE, &unsupported);

    let error = resolver::resolve_route(
        root.path(),
        &state.snapshot,
        &registry,
        [],
        readable::GBA_ID,
    )
    .expect_err("runtime system mismatch");
    assert!(error.message.contains("no launcher supports system gba"));
}

#[test]
fn rejects_an_mgba_runtime_that_is_not_a_libretro_core() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let state = state(root.path());
    let incompatible = MGBA_PLUGIN_SOURCE.replace("kind: \"libretro-core\"", "kind: \"tool\"");
    let registry = registry_from_sources(RETROARCH_PLUGIN_SOURCE, &incompatible);

    let error = resolver::resolve_route(
        root.path(),
        &state.snapshot,
        &registry,
        [],
        readable::GBA_ID,
    )
    .expect_err("incompatible runtime kind");
    assert!(error
        .message
        .contains("has kind tool, expected libretro-core"));
}
