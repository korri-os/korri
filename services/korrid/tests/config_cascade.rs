#[path = "fixtures/readable.rs"]
mod readable;
use korrid::config::{
    cascade::{self, RunnerConfig},
    decode_config_documents,
};

#[test]
fn settings_only_family_and_runner_overrides_decode_without_executable_redeclaration() {
    let snapshot = decode_config_documents(
        "families:\n  '@korri:retroarch':\n    settings: {video-vsync: false}\nrunners:\n  '@korri:mgba/mgba':\n    settings: {video-vsync: true}\n",
        "{}",
        "{}",
    )
    .unwrap();
    korrid::config::classify_snapshot_support(&snapshot).unwrap();
    assert_eq!(snapshot.families.len(), 1);
    assert_eq!(snapshot.runners.len(), 1);
    assert!(
        decode_config_documents("runners: {core: {program: /bin/program}}", "{}", "{}").is_err()
    );
}

#[test]
fn family_then_system_then_runner_then_game_then_override_is_explainable() {
    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    let mut snapshot = (*korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path())
        .reload()
        .snapshot)
        .clone();
    let runner = "@korri:mgba/mgba";
    let family = "@korri:retroarch";
    let config = |n: i32| -> RunnerConfig {
        serde_yaml::from_str(&format!(
            "settings: {{level: {n}}}\nconfig: {{append: 'layer = {n}'}}"
        ))
        .unwrap()
    };
    snapshot.families.insert(family.into(), config(1));
    snapshot.runners.insert(runner.into(), config(3));
    let system = snapshot
        .systems
        .entry("gba".into())
        .or_insert_with(|| serde_yaml::from_str("{}").unwrap());
    system.families.insert(family.into(), config(2));
    system.runners.insert(runner.into(), config(4));
    let game = snapshot.games.get_mut(readable::GBA_ID).unwrap();
    game.families.insert(family.into(), config(5));
    game.runners.insert(runner.into(), config(6));
    let route = korrid::config::resolver::ResolvedRoute {
        playable_id: readable::GBA_ID.into(),
        title: None,
        release_id: String::new(),
        identity: None,
        provider_id: family.into(),
        system_id: "gba".into(),
        system_title: None,
        runner_id: runner.into(),
        family_id: Some(family.into()),
        integration_token: String::new(),
        flattened_target: String::new(),
        android_component: None,
        linux_runner: None,
        core_path: None,
        file_target: None,
    };
    let result = cascade::resolve(&snapshot, &route, None);
    assert_eq!(
        serde_json::to_value(&result.settings).unwrap(),
        serde_json::json!({"level":6})
    );
    assert_eq!(result.config.unwrap().append.as_deref(), Some("layer = 6"));
    let overrides: RunnerConfig =
        serde_yaml::from_str("settings: {level: 7}\nconfig: {prepend: 'extra = true'}").unwrap();
    let result = cascade::resolve(&snapshot, &route, Some(&overrides));
    assert_eq!(
        serde_json::to_value(&result.settings).unwrap(),
        serde_json::json!({"level":7})
    );
    assert_eq!(
        result.config.as_ref().unwrap().prepend.as_deref(),
        Some("extra = true")
    );
    assert_eq!(result.config.unwrap().append.as_deref(), Some("layer = 6"));
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
        "@korri:mgba/mgba",
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
        "@korri:mgba/mgba",
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
}
