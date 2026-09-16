//! The runner owns its settings schema, so these tests run the real RetroArch
//! helper against a generated schema module exactly as a device would.
#[path = "fixtures/readable.rs"]
mod readable;

use korrid::{
    config::{resolver::resolve_linux_route, snapshot::ConfigSnapshotCoordinator},
    launcher::{
        linux_plugin::launch_route,
        plugin_launch::{evaluate_snapshot, PluginLaunchInput, PluginLaunchOverrides},
        typed_settings::validate,
    },
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    script::source::SourceSnapshot,
};
use std::{collections::BTreeMap, fs, path::Path};

/// The build-time generator writes this module from the pinned program's own
/// source. The shape is reproduced here, never a second key table for korrid.
fn settings_module(keys: serde_json::Value) -> String {
    let entries: String = keys
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, kind)| format!("  {key}: {kind},\n"))
        .collect();
    format!("export const version = \"1.22.2\"\nexport const keys = {{\n{entries}}}\n")
}

fn checked_keys() -> serde_json::Value {
    serde_json::json!({
        "video_vsync": "Boolean",
        "video_driver": "String",
        "audio_volume": "Number",
        "audio_device": "String",
        "config_save_on_exit": "Boolean",
    })
}

fn package(root: &Path, name: &str, source: &str) -> EnabledPackage {
    let package = root.join(name);
    fs::create_dir(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    let sources = if source.contains("./retroarch") {
        fs::write(
            package.join("retroarch.ts"),
            include_str!("../../../plugins/libretro/retroarch.ts"),
        )
        .unwrap();
        fs::write(package.join("settings.ts"), settings_module(checked_keys())).unwrap();
        vec![
            "plugin.ts".into(),
            "retroarch.ts".into(),
            "settings.ts".into(),
        ]
    } else {
        vec!["plugin.ts".into()]
    };
    EnabledPackage {
        id: format!("@korri:{name}"),
        package,
        files: BTreeMap::new(),
        entry: "plugin.ts".into(),
        sources,
    }
}

/// One core plugin exactly as the catalogue generates it: an entry that
/// re-exports the shared helper, the helper, and the checked schema beside it.
fn core_snapshot(root: &Path, keys: serde_json::Value) -> SourceSnapshot {
    fs::write(
        root.join("plugin.ts"),
        format!(
            "export const name = 'fixture';\n{}",
            include_str!("../examples/libretro-core.plugin.ts")
                .lines()
                .filter(|line| !line.starts_with("export const name"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .unwrap();
    fs::write(
        root.join("retroarch.ts"),
        include_str!("../../../plugins/libretro/retroarch.ts"),
    )
    .unwrap();
    fs::write(root.join("settings.ts"), settings_module(keys)).unwrap();
    SourceSnapshot::package_plugin(
        root,
        "plugin.ts",
        &[
            "plugin.ts".to_owned(),
            "retroarch.ts".to_owned(),
            "settings.ts".to_owned(),
        ],
    )
    .unwrap()
}

fn input(settings: serde_json::Value) -> PluginLaunchInput {
    serde_json::from_value(serde_json::json!({
        "runnerId": "@korri:mgba/mgba",
        "familyId": "@korri:retroarch",
        "program": "/program",
        "corePath": "/core",
        "contentPath": "/rom",
        "accountRoot": "/account",
        "files": {"autoconfig":"/autoconfig"},
        "overrides": {"settings":settings},
    }))
    .unwrap()
}

#[test]
fn callback_collapses_native_assignments_without_relaxing_reserved_keys() {
    let root = tempfile::tempdir().unwrap();
    let source = core_snapshot(root.path(), checked_keys());
    let mut input = input(serde_json::json!({
        "video_vsync":false, "video_driver":"vulkan", "audio_volume":-3.5, "audio_device":"device\\path",
    }));
    input.overrides.as_mut().unwrap().config = Some(
        serde_json::from_value(serde_json::json!({
            "prepend":"video_vsync = true", "append":"video_vsync = false",
        }))
        .unwrap(),
    );
    let content = evaluate_snapshot(&source, &input)
        .unwrap()
        .files
        .remove(0)
        .content;
    assert!(content.contains("video_vsync = false\n"));
    assert_eq!(content.matches("video_vsync =").count(), 1);
    assert!(content.contains("video_driver = \"vulkan\"\n"));
    assert_eq!(content.matches("video_driver =").count(), 1);
    assert!(content.contains("audio_volume = -3.5\n"));
    assert!(content.contains("audio_device = \"device\\path\"\n"));
    for key in [
        "config_save_on_exit",
        "kiosk_mode_enable",
        "menu_driver",
        "cheevos_token",
        "netplay_password",
    ] {
        // A reserved key keeps its protection: the schema may name it, and the
        // runner still refuses to take its value from authored settings.
        let mut typed = input.clone();
        typed.overrides = Some(
            serde_json::from_value(serde_json::json!({"settings":{key:"sentinel-value"}})).unwrap(),
        );
        let rendered = evaluate_snapshot(&source, &typed).unwrap().files.remove(0);
        assert!(!rendered.content.contains("sentinel-value"), "typed {key}");
        let mut raw = input.clone();
        raw.overrides = Some(
            serde_json::from_value(
                serde_json::json!({"config":{"append":format!("{key} = false")}}),
            )
            .unwrap(),
        );
        assert!(evaluate_snapshot(&source, &raw).is_err(), "raw {key}");
    }
}

#[test]
fn callback_rejects_strings_that_cannot_round_trip_through_native_cfg() {
    let root = tempfile::tempdir().unwrap();
    let source = core_snapshot(root.path(), checked_keys());
    for value in ["device \"quoted\"", "a\nb", "a\rb", "a\0b"] {
        let input = input(serde_json::json!({"audio_device":value}));
        assert!(evaluate_snapshot(&source, &input).is_err(), "{value:?}");
    }
    let input = input(serde_json::json!({"audio_device":"hw:\"quoted\""}));
    let content = evaluate_snapshot(&source, &input)
        .unwrap()
        .files
        .remove(0)
        .content;
    assert!(content.contains("audio_device = hw:\"quoted\"\n"));
}

#[test]
fn the_runner_describes_its_own_schema_and_validates_against_it() {
    let root = tempfile::tempdir().unwrap();
    let source = core_snapshot(root.path(), checked_keys());
    let described: serde_json::Value = serde_json::from_str(
        &korrid::script::call_plugin_operation_snapshot(
            &source,
            korrid::script::SETTINGS_DESCRIBE,
            "{}",
        )
        .unwrap(),
    )
    .unwrap();
    // The described schema is this build's own evidence, carrying the revision
    // it is valid for, and it never offers a key korrid reserves.
    assert_eq!(described["revision"], "1.22.2");
    assert_eq!(
        described["schema"]["properties"]["video_vsync"]["type"],
        "boolean"
    );
    assert_eq!(
        described["schema"]["properties"]["audio_volume"]["type"],
        "number"
    );
    assert_eq!(described["schema"]["additionalProperties"], false);
    assert!(described["schema"]["properties"]
        .get("config_save_on_exit")
        .is_none());

    let settings = serde_json::from_value(serde_json::json!({
        "video_vsync": false,
        "video_driver": 7,
        "absent_key": true,
    }))
    .unwrap();
    let warnings = validate(
        &source,
        &settings,
        "@korri:mgba/mgba",
        "/nix/store/exact-build",
    )
    .unwrap();
    let reported: Vec<&str> = warnings
        .iter()
        .map(|warning| warning.setting.as_str())
        .collect();
    assert_eq!(reported, vec!["absent_key", "video_driver"]);
    assert!(warnings
        .iter()
        .all(|warning| warning.message.contains("1.22.2")
            && warning.build == "/nix/store/exact-build"));
}

#[test]
fn an_unsupported_setting_is_reported_once_and_never_reaches_the_native_config() {
    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let snapshot = ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot;
    let mut runner = package(
        root.path(),
        "mgba",
        include_str!("../examples/libretro-core.plugin.ts"),
    );
    runner
        .files
        .insert("retroarch".into(), "/exact-program".into());
    runner.files.insert("mgba".into(), "/exact-core".into());
    runner
        .files
        .insert("autoconfig".into(), "/autoconfig".into());
    let overrides: PluginLaunchOverrides = serde_json::from_value(serde_json::json!({
        "settings":{"audio_volume":-3,"video_vsync":false,"absent_key":1}
    }))
    .unwrap();
    let registry = PluginRegistry::from_installed(vec![runner.clone()]).unwrap();
    let route = resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@korri:mgba/mgba"),
    )
    .unwrap();
    let spec = launch_route(
        root.path(),
        &snapshot,
        &registry,
        &route,
        Some(overrides.clone()),
    )
    .unwrap();
    // korrid hands the runner exactly what the person authored, and reports
    // the one key this build cannot apply.
    let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
    assert_eq!(input.overrides.as_ref().unwrap().settings.len(), 3);
    assert_eq!(spec.warnings.len(), 1);
    assert_eq!(spec.warnings[0].setting, "absent_key");
    assert_eq!(spec.warnings[0].runner_id, "@korri:mgba/mgba");
    assert_eq!(spec.warnings[0].build, runner.package.display().to_string());

    // The runner leaves it out of the native configuration it renders.
    let source =
        SourceSnapshot::package_plugin(&runner.package, &runner.entry, &runner.sources).unwrap();
    let content = evaluate_snapshot(&source, &input)
        .unwrap()
        .files
        .remove(0)
        .content;
    assert!(content.contains("audio_volume = -3\n"));
    assert!(content.contains("video_vsync = \"false\"\n"));
    assert!(!content.contains("absent_key"));
}

#[test]
#[ignore = "build .#korri-plugin-mgba and set KORRI_TEST_RETROARCH_PACKAGE"]
fn packaged_source_evidence_reaches_the_packaged_callback_configuration_bytes() {
    #[derive(serde::Deserialize)]
    struct Manifest {
        files: BTreeMap<String, std::path::PathBuf>,
        entry: String,
        sources: Vec<String>,
    }
    let package = std::path::PathBuf::from(
        std::env::var_os("KORRI_TEST_RETROARCH_PACKAGE").expect("built RetroArch package"),
    );
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(package.join("manifest.json")).unwrap()).unwrap();
    let package = EnabledPackage {
        id: "@korri:mgba".into(),
        package,
        files: manifest.files,
        entry: manifest.entry,
        sources: manifest.sources,
    };
    let build = package.package.display().to_string();
    let program = package.files["retroarch"].display().to_string();
    let source =
        SourceSnapshot::package_plugin(&package.package, &package.entry, &package.sources).unwrap();
    let mut input = input(
        serde_json::json!({"video_vsync":false,"audio_volume":-3.5,"audio_device":"device\\path","absent_key":true,"config_save_on_exit":true}),
    );
    let warnings = validate(
        &source,
        &input.overrides.as_ref().unwrap().settings,
        &input.runner_id,
        &build,
    )
    .unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(warnings
        .iter()
        .all(|warning| warning.build == build && warning.message.contains("1.22.2")));
    input.program = program;
    input.files = package
        .files
        .iter()
        .map(|(key, path)| (key.clone(), path.display().to_string()))
        .collect();
    let output = evaluate_snapshot(&source, &input).unwrap();
    for line in [
        "video_vsync = \"false\"\n",
        "audio_volume = -3.5\n",
        "audio_device = \"device\\path\"\n",
        "config_save_on_exit = \"false\"\n",
    ] {
        assert!(output.files[0].content.contains(line), "{line}");
    }
    assert!(!output.files[0].content.contains("absent_key"));
}
