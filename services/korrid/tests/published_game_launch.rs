//! Opt-in acceptance for unchanged publisher outputs, not rebuilt fixtures.
//! Run with KORRI_PUBLISHED_MGBA=/nix/store/<exact-output> and --ignored.
//! Trust/approval/closure checks belong to published-vm-test.nix. This test
//! proves admission, route construction and launch.prepare, not gameplay.
#[path = "fixtures/readable.rs"]
mod readable;

use korrid::{
    config::snapshot::ConfigSnapshotCoordinator,
    launcher::{
        linux_plugin::launch_route,
        plugin_launch::{evaluate_snapshot, PluginLaunchInput, PluginLaunchOverrides},
    },
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    script::source::SourceSnapshot,
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

// Read only fields owned by the real producer. This is not another manifest
// format: unrelated native-unit and publication fields remain producer-owned.
#[derive(Deserialize)]
struct Manifest {
    publisher: Publisher,
    entry: String,
    sources: Vec<String>,
    files: BTreeMap<String, PathBuf>,
}
#[derive(Deserialize)]
struct Publisher {
    namespace: String,
}

#[test]
#[ignore = "requires the exact published x86_64 mGBA package and its downloaded closure"]
fn published_mgba_route_and_launch_prepare() {
    let package = PathBuf::from(
        std::env::var_os("KORRI_PUBLISHED_MGBA")
            .expect("set KORRI_PUBLISHED_MGBA to the unchanged published store output"),
    );
    assert_eq!(
        package.parent().unwrap(),
        std::path::Path::new("/nix/store")
    );
    assert_eq!(fs::canonicalize(&package).unwrap(), package);
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(package.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.publisher.namespace, "@korri");
    for name in ["retroarch", "mgba", "autoconfig", "retroarch-settings"] {
        let file = &manifest.files[name];
        assert!(file.starts_with("/nix/store"), "{name}: {file:?}");
        assert!(file.exists(), "published named file is absent: {file:?}");
    }
    let registry = PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@korri:mgba".into(),
        package: package.clone(),
        files: manifest.files.clone(),
        entry: manifest.entry.clone(),
        sources: manifest.sources.clone(),
    }])
    .expect("current Core must admit the published declaration and source graph");

    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    fs::create_dir_all(root.path().join("roms")).unwrap();
    // Storage resolution needs a file, not an emulator-valid ROM. No process
    // executes this marker and this test makes no gameplay claim.
    fs::write(root.path().join("roms/wl4.gba"), b"route-only marker").unwrap();
    let snapshot = (*ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot)
        .clone();
    let routes = korrid::game_routes::list(root.path(), &registry, readable::GBA_ID).unwrap();
    assert_eq!(routes.routes.len(), 1);
    assert_eq!(routes.routes[0].runner_id, "@korri:mgba/mgba");
    assert_eq!(
        routes.routes[0].family_id.as_deref(),
        Some("@korri:retroarch")
    );
    assert_eq!(routes.routes[0].runner_build, package.display().to_string());
    let resolved = korrid::config::resolver::resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@korri:mgba/mgba"),
    )
    .unwrap();
    let overrides: PluginLaunchOverrides = serde_json::from_value(serde_json::json!({
        "config": {"append": "video_vsync = true"}
    }))
    .unwrap();
    let spec = launch_route(
        root.path(),
        &snapshot,
        &registry,
        &resolved,
        Some(overrides),
    )
    .expect("current Core must construct a route against the published runner");
    assert_eq!(spec.command[1], "plugin-launch");
    assert_eq!(
        spec.command[2],
        package.join(&manifest.entry).display().to_string()
    );
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    assert_eq!(input.runner_id, "@korri:mgba/mgba");
    assert_eq!(input.family_id.as_deref(), Some("@korri:retroarch"));
    assert_eq!(
        input.program,
        manifest.files["retroarch"].display().to_string()
    );
    assert_eq!(input.core_path.as_deref(), manifest.files["mgba"].to_str());
    for (name, file) in &manifest.files {
        assert_eq!(input.files[name], file.display().to_string());
    }
    let source =
        SourceSnapshot::package_plugin(&package, &manifest.entry, &manifest.sources).unwrap();
    // Use the actual current named-operation dispatch. A host-only change to
    // LAUNCH_PREPARE must fail this same test with these same package bytes.
    let output = evaluate_snapshot(&source, &input)
        .expect("current launch.prepare dispatch must evaluate the published callback");
    assert_eq!(output.command, input.program);
    let config = root.path().join("users/default/retroarch.cfg");
    assert_eq!(
        output.args,
        vec![
            "--config".to_string(),
            config.display().to_string(),
            "-L".to_string(),
            input.core_path.unwrap(),
            root.path().join("roms/wl4.gba").display().to_string(),
        ]
    );
    let native_config = output
        .files
        .iter()
        .find(|file| file.path == config.display().to_string())
        .expect("published callback must declare its native configuration file");
    for setting in [
        "video_vsync = true",
        "menu_driver = \"null\"",
        "kiosk_mode_enable = \"true\"",
        "config_save_on_exit = \"false\"",
        input.files["autoconfig"].as_str(),
    ] {
        assert!(native_config.content.contains(setting), "missing {setting}");
    }
}
