#[path = "fixtures/readable.rs"]
mod readable;
use korrid::config::{
    cascade::{self, LauncherConfig},
    decode_config_documents,
};

#[test]
fn settings_only_runtime_overrides_decode_without_executable_redeclaration() {
    // Grounding: approved runtimes[fullRuntimeId].launchers[fullLauncherId]
    // example; these are the three existing snapshot.rs documents.
    let snapshot = decode_config_documents(
        "runtimes:\n  '@korri:mgba/mgba':\n    launchers:\n      '@korri:retroarch/retroarch':\n        settings: {video_vsync: false}\n",
        "{}", "{}",
    ).unwrap();
    korrid::config::classify_snapshot_support(&snapshot).unwrap();
    assert_eq!(snapshot.runtimes.len(), 1);
    assert!(decode_config_documents(
        "runtimes: {core: {kind: libretro-core, path: /core.so}}",
        "{}",
        "{}"
    )
    .is_err());
}

#[test]
fn cascade_is_per_instance_and_raw_blocks_are_last_wins() {
    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    let mut snapshot = (*korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot)
        .clone();
    let id = "@simon:build/retroarch";
    let config = |n: i32| {
        format!("launchers: {{'{id}': {{settings: {{level: {n}}}, config: {{append: 'layer = {n}'}}}}}} ")
    };
    snapshot
        .launchers
        .insert(id.into(), serde_yaml::from_str(&config(1)).unwrap());
    snapshot.launchers.insert(
        "@korri:retroarch/retroarch".into(),
        serde_yaml::from_str(
            "launchers: {'@simon:build/retroarch': {settings: {kind_leak: true}}}",
        )
        .unwrap(),
    );
    snapshot
        .systems
        .insert("gba".into(), serde_yaml::from_str(&config(2)).unwrap());
    snapshot.runtimes.insert(
        "@simon:build/mgba".into(),
        serde_yaml::from_str(&config(3)).unwrap(),
    );
    snapshot.games.get_mut(readable::GBA_ID).unwrap().launchers =
        serde_yaml::from_str::<korrid::config::RuntimePayload>(&config(4))
            .unwrap()
            .launchers;
    let route = korrid::config::resolver::ResolvedRoute {
        playable_id: readable::GBA_ID.into(),
        title: None,
        release_id: String::new(),
        identity: None,
        provider_id: "@simon:build".into(),
        system_id: "gba".into(),
        system_title: None,
        launcher_id: id.into(),
        launcher_kind: "@korri:retroarch/retroarch".into(),
        integration_token: String::new(),
        flattened_target: String::new(),
        android_component: None,
        linux_launcher: None,
        file_target: None,
        runtime: Some(korrid::config::resolver::ResolvedRuntime {
            id: "@simon:build/mgba".into(),
            kind: "libretro-core".into(),
            app: id.into(),
            path: String::new(),
        }),
    };
    let result = cascade::resolve(&snapshot, &route, None);
    assert_eq!(
        serde_json::to_value(&result.settings).unwrap(),
        serde_json::json!({"level":4})
    );
    assert_eq!(result.config.unwrap().append.as_deref(), Some("layer = 4"));
    let overrides: LauncherConfig =
        serde_yaml::from_str("settings: {level: 5}\nconfig: {prepend: 'extra = true'}").unwrap();
    let result = cascade::resolve(&snapshot, &route, Some(&overrides));
    assert_eq!(
        serde_json::to_value(&result.settings).unwrap(),
        serde_json::json!({"level":5})
    );
    assert_eq!(
        result.config.as_ref().unwrap().prepend.as_deref(),
        Some("extra = true")
    );
    assert_eq!(result.config.unwrap().append.as_deref(), Some("layer = 4"));
}

#[test]
fn source_checked_types_omit_unsupported_settings_with_build_and_version() {
    use korrid::launcher::typed_settings::*;
    let settings =
        serde_json::from_value(serde_json::json!({"supported":true,"wrong":false,"absent":1}))
            .unwrap();
    let keys = std::collections::HashMap::from([
        ("supported".into(), SettingType::Boolean),
        ("wrong".into(), SettingType::String),
    ]);
    let (accepted, warnings) = validate(
        settings,
        "@korri:retroarch/retroarch",
        "/nix/store/exact-build",
        Some(SourceCheckedSettings {
            version: "1.22.2",
            build: "/nix/store/exact-build",
            keys: &keys,
        }),
    );
    assert_eq!(accepted.len(), 1);
    let (accepted, mismatched) = validate(
        accepted,
        "@korri:retroarch/retroarch",
        "/nix/store/other-build",
        Some(SourceCheckedSettings {
            version: "1.22.2",
            build: "/nix/store/exact-build",
            keys: &keys,
        }),
    );
    assert!(accepted.is_empty());
    assert_eq!(mismatched.len(), 1);
    assert_eq!(warnings.len(), 2);
    assert!(warnings
        .iter()
        .all(|warning| warning.message.contains("1.22.2")
            && warning.message.contains("/nix/store/exact-build")
            && warning.message.contains(&warning.setting)));
}
