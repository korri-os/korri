#[path = "fixtures/readable.rs"]
mod readable;
use korrid::config::{decode_config_documents, modifiers::ModifierConfigs};
use korrid::launch_settings::resolve_launch;
const MODIFIER: &str = "@test:wrapper/wrapper";
const RUNNER: &str = "@korri:mgba/mgba";

#[test]
fn modifier_records_decode_on_every_layer_and_runner_place() {
    let record="{'@test:wrapper/wrapper': {enabled: true, settings: {width: 1240}, config: {append: '--flag'}}}";
    let device=format!("host: {{modifiers: {record}, runners: {{'{RUNNER}': {{modifiers: {record}}}}}}}\nsystems: {{gba: {{modifiers: {record}, runners: {{'{RUNNER}': {{modifiers: {record}}}}}}}}}");
    let library = format!(
        "{}    modifiers: {record}\n    runners: {{'{RUNNER}': {{modifiers: {record}}}}}\n",
        readable::gba_library()
    );
    decode_config_documents(&device, &library).unwrap();
}
#[test]
fn modifier_config_refuses_prepend_replace_and_removed_top_level_shapes() {
    for part in ["prepend", "replace"] {
        let record = format!("{{'{MODIFIER}': {{config: {{{part}: '--flag'}}}}}}");
        for device in [
            format!("host: {{modifiers: {record}}}"),
            format!("host: {{runners: {{'{RUNNER}': {{modifiers: {record}}}}}}}"),
            format!("systems: {{gba: {{modifiers: {record}}}}}"),
        ] {
            assert!(decode_config_documents(&device, "{}").is_err());
        }
        assert!(decode_config_documents(
            "{}",
            &format!("{}    modifiers: {record}\n", readable::gba_library())
        )
        .is_err());
    }
    for device in [
        "profiles: {}",
        "modifiers: {}",
        "runners: {}",
        "families: {}",
        "hooks: {}",
    ] {
        assert!(decode_config_documents(device, "{}").is_err());
    }
}
#[test]
fn removed_launch_field_is_unknown_on_host_profile_and_release() {
    for (device, library) in [
        ("host: {launch: {}}".to_string(), "{}".to_string()),
        (
            "{}".into(),
            format!(
                "{}    profiles: {{local: {{title: Local, launch: {{}}}}}}\n",
                readable::gba_library()
            ),
        ),
        (
            "{}".into(),
            format!("{}        launch: {{}}\n", readable::gba_library()),
        ),
    ] {
        let error = decode_config_documents(&device, &library)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown field `launch`"), "{error}");
    }
}
#[test]
fn modifiers_fold_in_the_same_order_as_every_other_kind_and_keep_absence() {
    let layer = |enabled: &str, key: &str, n: u8| -> ModifierConfigs {
        serde_yaml::from_str(&format!("'{MODIFIER}': {{{enabled}settings: {{shared: {n}, {key}: {n}}}, config: {{append: 'layer {n}'}}}}\n'@test:wrapper/off': {{settings: {{value: true}}}}" )).unwrap()
    };
    let mut snapshot = decode_config_documents(
        &format!("host: {{runners: {{'{RUNNER}': {{}}}}}}\nsystems: {{gba: {{}}}}"),
        &readable::gba_library(),
    )
    .unwrap();
    let route = korrid::config::resolver::ResolvedRoute {
        playable_id: readable::GBA_ID.into(),
        title: None,
        release_id: readable::GBA_RELEASE.into(),
        identity: None,
        provider_id: "@korri:mgba".into(),
        system_id: "gba".into(),
        system_title: None,
        runner_id: RUNNER.into(),
        family_id: None,
        integration_token: String::new(),
        flattened_target: String::new(),
        linux_runner: None,
        core_path: None,
        file_target: None,
        provider_target: None,
        artifact_target: None,
        package_target: None,
        cache_target: None,
    };
    snapshot.host.as_mut().unwrap().modifiers = layer("enabled: true, ", "device", 1);
    snapshot.systems.get_mut("gba").unwrap().modifiers = layer("enabled: false, ", "system", 2);
    snapshot
        .host
        .as_mut()
        .unwrap()
        .runners
        .get_mut(RUNNER)
        .unwrap()
        .modifiers = layer("enabled: true, ", "runner", 3);
    let resolved = resolve_launch(&snapshot, &route, None, None).unwrap();
    assert_eq!(resolved.modifiers[MODIFIER].enabled, Some(true));
    assert_eq!(resolved.modifiers["@test:wrapper/off"].enabled, None);
    snapshot
        .library
        .get_mut(readable::GBA_ID)
        .unwrap()
        .modifiers = layer("enabled: false, ", "game", 4);
    let resolved = resolve_launch(&snapshot, &route, None, None).unwrap();
    assert_eq!(resolved.modifiers[MODIFIER].enabled, Some(false));
    assert_eq!(
        serde_json::to_value(&resolved.modifiers[MODIFIER].settings).unwrap(),
        serde_json::json!({"shared":4,"device":1,"system":2,"runner":3,"game":4})
    );
    assert_eq!(
        resolved.modifiers[MODIFIER]
            .config
            .as_ref()
            .unwrap()
            .append
            .as_deref(),
        Some("layer 4")
    );
    snapshot
        .library
        .get_mut(readable::GBA_ID)
        .unwrap()
        .modifiers = layer("", "game", 4);
    assert_eq!(
        resolve_launch(&snapshot, &route, None, None)
            .unwrap()
            .modifiers[MODIFIER]
            .enabled,
        Some(true)
    );
}
#[test]
fn coordinator_keeps_authored_modifier_order_and_rejects_bad_ids() {
    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    let device = std::fs::read_to_string(root.path().join("device.yaml")).unwrap();
    let mut value: serde_yaml::Value = serde_yaml::from_str(&device).unwrap();
    value["host"]["modifiers"] =
        serde_yaml::from_str("'@test:z/main': {enabled: true}\n'@test:a/main': {enabled: true}")
            .unwrap();
    std::fs::write(
        root.path().join("device.yaml"),
        serde_yaml::to_string(&value).unwrap(),
    )
    .unwrap();
    let loaded = korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(loaded.diagnostic.is_none(), "{:?}", loaded.diagnostic);
    assert_eq!(
        loaded
            .snapshot
            .host
            .as_ref()
            .unwrap()
            .modifiers
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["@test:z/main", "@test:a/main"]
    );
    for id in [
        "unscoped",
        "@test:plugin",
        "@test:plugin/",
        "@test:plugin/a/b",
    ] {
        assert!(
            decode_config_documents(&format!("host: {{modifiers: {{'{id}': {{}}}}}}"), "{}")
                .is_err()
        );
    }
    assert!(decode_config_documents(
        "host: {modifiers: {'@test:p/m': {}, '@test:p/m': {}}}",
        "{}"
    )
    .is_err());
}
