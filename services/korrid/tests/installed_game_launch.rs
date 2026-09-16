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
use std::{collections::BTreeMap, fs, path::Path};

fn installed(root: &Path) -> PluginRegistry {
    let package = root.join("plugin-mgba");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        include_str!("../examples/libretro-core.plugin.ts"),
    )
    .unwrap();
    fs::write(
        package.join("retroarch.ts"),
        include_str!("../../../plugins/libretro/retroarch.ts"),
    )
    .unwrap();
    // The catalogue generates this module from the pinned program's source.
    fs::write(
        package.join("settings.ts"),
        "export const version = \"1.22.2\"\nexport const keys = {\n  video_vsync: \"Boolean\",\n}\n",
    )
    .unwrap();
    PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@korri:mgba".into(),
        package,
        files: BTreeMap::from([
            ("retroarch".into(), root.join("retroarch")),
            ("mgba".into(), root.join("mgba.so")),
            ("autoconfig".into(), root.join("autoconfig")),
        ]),
        entry: "plugin.ts".into(),
        sources: vec![
            "plugin.ts".into(),
            "retroarch.ts".into(),
            "settings.ts".into(),
        ],
    }])
    .unwrap()
}

fn setup(root: &Path) -> (korrid::config::ConfigSnapshot, PluginRegistry) {
    readable::combined(root);
    fs::create_dir_all(root.join("roms")).unwrap();
    fs::write(root.join("roms/wl4.gba"), b"rom").unwrap();
    let snapshot = (*ConfigSnapshotCoordinator::new(root).reload().snapshot).clone();
    (snapshot, installed(root))
}

#[test]
fn selected_runner_owns_frontend_core_callback_and_named_files() {
    let root = tempfile::tempdir().unwrap();
    let (snapshot, registry) = setup(root.path());
    let routes = korrid::game_routes::list(root.path(), &registry, readable::GBA_ID).unwrap();
    assert_eq!(routes.routes.len(), 1);
    let route = &routes.routes[0];
    assert_eq!(route.runner_id, "@korri:mgba/mgba");
    assert_eq!(route.family_id.as_deref(), Some("@korri:retroarch"));
    assert_eq!(
        route.runner_build,
        registry
            .installed_package("@korri:mgba/mgba")
            .unwrap()
            .package
            .display()
            .to_string()
    );

    let resolved = korrid::config::resolver::resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@korri:mgba/mgba"),
    )
    .unwrap();
    let spec = launch_route(root.path(), &snapshot, &registry, &resolved, None).unwrap();
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    assert_eq!(input.runner_id, "@korri:mgba/mgba");
    assert_eq!(input.family_id.as_deref(), Some("@korri:retroarch"));
    assert_eq!(
        input.program,
        root.path().join("retroarch").display().to_string()
    );
    assert_eq!(
        input.core_path.as_deref(),
        Some(root.path().join("mgba.so").to_str().unwrap())
    );
    assert_eq!(
        input.files["autoconfig"],
        root.path().join("autoconfig").display().to_string()
    );
}

#[test]
fn runner_callback_preserves_raw_native_override_semantics() {
    let root = tempfile::tempdir().unwrap();
    let (snapshot, registry) = setup(root.path());
    let route = korrid::config::resolver::resolve_linux_route(
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
    let spec = launch_route(root.path(), &snapshot, &registry, &route, Some(overrides)).unwrap();
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    let package = registry.installed_package("@korri:mgba/mgba").unwrap();
    let source =
        SourceSnapshot::package_plugin(&package.package, &package.entry, &package.sources).unwrap();
    let output = evaluate_snapshot(&source, &input).unwrap();
    assert_eq!(
        output.command,
        root.path().join("retroarch").display().to_string()
    );
    assert_eq!(
        output.args[0..3],
        [
            "--config",
            root.path()
                .join("users/default/retroarch.cfg")
                .to_str()
                .unwrap(),
            "-L"
        ]
    );
    assert!(output.files[0].content.contains("video_vsync = true"));
}

#[test]
fn cross_package_executable_reference_has_no_contract_shape() {
    let source = r#"
      export const name = 'bad';
      export const runners = { main: {
        id: '@test:bad/main', program: 'program', core: 'core',
        launcher: '@other:frontend/main', systems: ['gba']
      }};
      export const handlers = {'launch.prepare': function () { return {command:'/bin/false', args:[]}; }}
    "#;
    assert!(korrid::plugin::load_plugin_source("@test", source).is_err());
}
