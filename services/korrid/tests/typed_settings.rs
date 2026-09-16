#[path = "fixtures/readable.rs"]
mod readable;

use korrid::{
    config::{resolver::resolve_linux_route, snapshot::ConfigSnapshotCoordinator},
    launcher::{
        linux_plugin::launch_route,
        plugin_launch::{evaluate, PluginLaunchInput, PluginLaunchOverrides},
        typed_settings::PackagedSettings,
    },
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
};
use std::{collections::BTreeMap, fs, path::Path};

fn package(root: &Path, name: &str, source: &str) -> EnabledPackage {
    let package = root.join(name);
    fs::create_dir(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    EnabledPackage {
        id: format!("@korri:{name}"),
        package,
        files: BTreeMap::new(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: vec![],
    }
}

fn evidence(package: &mut EnabledPackage, program: &str, version: &str, keys: serde_json::Value) {
    let path = package.package.join("settings.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "program": program, "version": version, "keys": keys,
        }))
        .unwrap(),
    )
    .unwrap();
    package.files.insert("retroarch-settings".into(), path);
}

fn input(settings: serde_json::Value) -> PluginLaunchInput {
    serde_json::from_value(serde_json::json!({
        "launcherId": "@korri:retroarch/retroarch",
        "launcherKind": "@korri:retroarch/retroarch", "runtimeId": "@korri:mgba/mgba",
        "program": "/program", "runtimePath": "/core", "contentPath": "/rom",
        "accountRoot": "/account", "files": {"autoconfig":"/autoconfig"},
        "overrides": {"settings":settings},
    }))
    .unwrap()
}

#[test]
fn callback_collapses_native_assignments_without_relaxing_reserved_keys() {
    let source = include_str!("../../../plugins/retroarch/plugin.ts");
    let mut input = input(serde_json::json!({
        "video_vsync":false, "video_driver":"vulkan", "audio_volume":-3.5, "audio_device":"device\\path",
    }));
    input.overrides.as_mut().unwrap().config = Some(
        serde_json::from_value(serde_json::json!({
            "prepend":"video_vsync = true", "append":"video_vsync = false",
        }))
        .unwrap(),
    );
    let content = evaluate(source, &input).unwrap().files.remove(0).content;
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
        let mut typed = input.clone();
        typed.overrides =
            Some(serde_json::from_value(serde_json::json!({"settings":{key:false}})).unwrap());
        assert!(evaluate(source, &typed).is_err(), "typed {key}");
        let mut raw = input.clone();
        raw.overrides = Some(
            serde_json::from_value(
                serde_json::json!({"config":{"append":format!("{key} = false")}}),
            )
            .unwrap(),
        );
        assert!(evaluate(source, &raw).is_err(), "raw {key}");
    }
}

#[test]
fn callback_rejects_strings_that_cannot_round_trip_through_native_cfg() {
    let source = include_str!("../../../plugins/retroarch/plugin.ts");
    for value in ["device \"quoted\"", "a\nb", "a\rb", "a\0b"] {
        let input = input(serde_json::json!({"audio_device":value}));
        assert!(evaluate(source, &input).is_err(), "{value:?}");
    }
    let input = input(serde_json::json!({"audio_device":"hw:\"quoted\""}));
    let content = evaluate(source, &input).unwrap().files.remove(0).content;
    assert!(content.contains("audio_device = hw:\"quoted\"\n"));
}

#[test]
fn installed_instances_use_their_own_evidence_and_exact_program_before_the_kind_callback() {
    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    fs::create_dir(root.path().join("roms")).unwrap();
    fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    let snapshot = ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot;
    let mut kind = package(
        root.path(),
        "retroarch",
        include_str!("../../../plugins/retroarch/plugin.ts"),
    );
    kind.files
        .insert("retroarch".into(), "/default-program".into());
    kind.files.insert("autoconfig".into(), "/autoconfig".into());
    evidence(
        &mut kind,
        "/default-program",
        "default-version",
        serde_json::json!({"video_vsync":"Boolean", "audio_volume":"Number"}),
    );
    let mut instance = package(
        root.path(),
        "alternate",
        r#"
        export const name = "alternate";
        export const systems = { gba: { id: "gba", title: "Game Boy Advance" } };
        export const launchers = { retroarch: { id: "@korri:alternate/retroarch", kind: "@korri:retroarch/retroarch", program: "retroarch" } };
        export const runtimes = { core: { id: "@korri:alternate/core", kind: "libretro-core", launcher: "@korri:alternate/retroarch", path: "core", supports: {systems:["gba"]} } };
    "#,
    );
    instance.requires.push(kind.package.clone());
    instance
        .files
        .insert("retroarch".into(), "/alternate-program".into());
    instance.files.insert("core".into(), "/core".into());
    let overrides: PluginLaunchOverrides = serde_json::from_value(serde_json::json!({
        "settings":{"audio_volume":-3,"video_vsync":false,"absent_key":1},
    }))
    .unwrap();
    for (program, expected_accepted) in [("/alternate-program", 1), ("/default-program", 0)] {
        evidence(
            &mut instance,
            program,
            "alternate-version",
            serde_json::json!({"audio_volume":"Number", "video_vsync":"Number"}),
        );
        let registry =
            PluginRegistry::from_installed(vec![kind.clone(), instance.clone()]).unwrap();
        let route = resolve_linux_route(
            root.path(),
            &snapshot,
            &registry,
            readable::GBA_ID,
            Some("@korri:alternate/core"),
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
        let input: PluginLaunchInput = serde_json::from_str(&spec.command[3]).unwrap();
        assert_eq!(
            input.overrides.as_ref().unwrap().settings.len(),
            expected_accepted
        );
        assert_eq!(spec.warnings.len(), 3 - expected_accepted);
        for warning in &spec.warnings {
            assert_eq!(warning.build, instance.package.display().to_string());
            assert_eq!(warning.launcher_id, "@korri:alternate/retroarch");
            assert!(warning.message.contains("alternate-version"));
            assert!(!warning.message.contains("default-version"));
            assert!(warning.message.contains(&warning.setting));
        }
        let output = evaluate(&fs::read_to_string(&spec.command[2]).unwrap(), &input).unwrap();
        assert_eq!(output.command, "/alternate-program");
        assert_eq!(
            output.files[0].content.contains("audio_volume = -3\n"),
            expected_accepted == 1
        );
        assert!(!output.files[0].content.contains("video_vsync ="));
        assert!(!output.files[0].content.contains("absent_key"));
    }
    instance.files.remove("retroarch-settings");
    let registry = PluginRegistry::from_installed(vec![kind, instance]).unwrap();
    let route = resolve_linux_route(
        root.path(),
        &snapshot,
        &registry,
        readable::GBA_ID,
        Some("@korri:alternate/core"),
    )
    .unwrap();
    let spec = launch_route(root.path(), &snapshot, &registry, &route, Some(overrides)).unwrap();
    assert_eq!(spec.warnings.len(), 3);
    assert!(spec
        .warnings
        .iter()
        .all(|warning| warning.message.contains("source metadata absent")));
}

#[test]
fn malformed_packaged_evidence_is_an_error_not_setting_authority() {
    let root = tempfile::tempdir().unwrap();
    let mut package = package(root.path(), "invalid", "export const name = 'invalid';");
    let path = package.package.join("settings.json");
    package
        .files
        .insert("retroarch-settings".into(), path.clone());
    for artifact in [
        serde_json::json!({"program":"/program", "version":"1", "keys":{"video_vsync":"integer"}}),
        serde_json::json!({"program":"/program", "version":"", "keys":{}}),
        serde_json::json!({"program":"/program", "version":"1", "keys":{}, "since":"1"}),
    ] {
        fs::write(&path, serde_json::to_vec(&artifact).unwrap()).unwrap();
        assert!(PackagedSettings::read(&package, "retroarch").is_err());
    }
}

#[test]
#[ignore = "build .#korri-plugin-retroarch and set KORRI_TEST_RETROARCH_PACKAGE"]
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
        id: "@korri:retroarch".into(),
        package,
        files: manifest.files,
        entry: manifest.entry,
        sources: manifest.sources,
        requires: vec![],
    };
    let build = package.package.display().to_string();
    let program = package.files["retroarch"].display().to_string();
    let evidence = PackagedSettings::read(&package, "retroarch")
        .unwrap()
        .unwrap();
    let mut input = input(
        serde_json::json!({"video_vsync":false,"audio_volume":-3.5,"audio_device":"device\\path","absent_key":true,"config_save_on_exit":true}),
    );
    let (accepted, warnings) = evidence.validate(
        input.overrides.take().unwrap().settings,
        &input.launcher_id,
        &build,
        &program,
    );
    assert_eq!(accepted.len(), 3);
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
    input.overrides = Some(PluginLaunchOverrides {
        settings: accepted,
        config: None,
    });
    let output = evaluate(
        &fs::read_to_string(package.package.join("plugin.ts")).unwrap(),
        &input,
    )
    .unwrap();
    let bytes = output.files[0].content.as_bytes();
    for line in [
        "video_vsync = \"false\"\n",
        "audio_volume = -3.5\n",
        "audio_device = \"device\\path\"\n",
        "config_save_on_exit = \"false\"\n",
    ] {
        assert!(
            bytes
                .windows(line.len())
                .any(|window| window == line.as_bytes()),
            "{line}"
        );
    }
    assert!(!output.files[0].content.contains("absent_key"));
}
