#[path = "fixtures/readable.rs"]
mod readable;
use korrid::config::decode_config_documents;

const MODIFIER: &str = "@test:wrapper/wrapper";

#[test]
fn modifier_records_decode_at_the_device_runner_system_and_library_layers() {
    let record = "{'@test:wrapper/wrapper': {enabled: true, settings: {width: 1240}, config: {append: '--flag'}}}";
    let device = format!("modifiers: {record}\nrunners: {{'@korri:mgba/mgba': {{modifiers: {record}}}}}\nsystems: {{gba: {{modifiers: {record}}}}}");
    let library = format!("{}    modifiers: {record}\n", readable::gba_library());
    decode_config_documents(&device, &library).unwrap();
}

#[test]
fn modifiers_refuse_runner_subrecords_and_unsafe_config_parts() {
    for part in ["prepend", "replace"] {
        let record = format!("{{'{MODIFIER}': {{config: {{{part}: '--flag'}}}}}}");
        assert!(decode_config_documents(&format!("modifiers: {record}"), "{}").is_err());
        assert!(decode_config_documents(
            &format!("runners: {{core: {{modifiers: {record}}}}}"),
            "{}"
        )
        .is_err());
        assert!(decode_config_documents(
            &format!("systems: {{gba: {{modifiers: {record}}}}}"),
            "{}"
        )
        .is_err());
        assert!(decode_config_documents(
            "{}",
            &format!("{}    modifiers: {record}\n", readable::gba_library())
        )
        .is_err());
    }
    let record = format!("{{'{MODIFIER}': {{enabled: true}}}}");
    assert!(decode_config_documents(
        &format!("systems:\n  gba:\n    runners:\n      core:\n        modifiers: {record}"),
        "{}"
    )
    .is_err());
    assert!(decode_config_documents(
        "{}",
        &format!(
            "{}    runners: {{core: {{modifiers: {record}}}}}\n",
            readable::gba_library()
        )
    )
    .is_err());
}

#[test]
fn legacy_launch_is_an_unknown_field_in_host_profiles_and_releases() {
    for device in ["host: {launch: {}}", "profiles: {local: {launch: {}}}"] {
        let error = decode_config_documents(device, "{}")
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown field `launch`"), "{error}");
    }
    let error = decode_config_documents(
        "{}",
        &format!("{}        launch: {{}}\n", readable::gba_library()),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown field `launch`"), "{error}");
}

fn route() -> korrid::config::resolver::ResolvedRoute {
    korrid::config::resolver::ResolvedRoute {
        playable_id: readable::GBA_ID.into(),
        title: None,
        release_id: String::new(),
        identity: None,
        provider_id: "@korri:mgba".into(),
        system_id: "gba".into(),
        system_title: None,
        runner_id: "@korri:mgba/mgba".into(),
        family_id: None,
        integration_token: String::new(),
        flattened_target: String::new(),
        linux_runner: None,
        core_path: None,
        file_target: None,
    }
}

#[test]
fn modifiers_fold_each_key_in_both_directions_without_enabling_absent_records() {
    use korrid::config::modifiers::{resolve, ModifierConfigs};
    let layer = |enabled: &str, key: &str, n: u8| -> ModifierConfigs {
        serde_yaml::from_str(&format!("'{MODIFIER}': {{{enabled}settings: {{shared: {n}, {key}: {n}}}, config: {{append: 'layer {n}'}}}}\n'@test:wrapper/off': {{settings: {{value: true}}}}" )).unwrap()
    };
    let mut snapshot = decode_config_documents(
        "runners: {'@korri:mgba/mgba': {}}\nsystems: {gba: {}}",
        &readable::gba_library(),
    )
    .unwrap();
    snapshot.modifiers = layer("enabled: true, ", "device", 1);
    let check = |snapshot: &korrid::config::ConfigSnapshot, enabled, level| {
        let records = resolve(snapshot, &route());
        assert_eq!(records[MODIFIER].enabled, Some(enabled));
        assert_eq!(records["@test:wrapper/off"].enabled, None);
        assert_eq!(
            serde_json::to_value(&records[MODIFIER].settings).unwrap()["shared"],
            level
        );
        assert_eq!(
            records[MODIFIER].config.as_ref().unwrap().append,
            Some(format!("layer {level}"))
        );
    };
    check(&snapshot, true, 1);
    snapshot
        .runners
        .get_mut("@korri:mgba/mgba")
        .unwrap()
        .modifiers = layer("enabled: false, ", "runner", 2);
    check(&snapshot, false, 2);
    snapshot.systems.get_mut("gba").unwrap().modifiers = layer("enabled: true, ", "system", 3);
    check(&snapshot, true, 3);
    snapshot
        .library
        .get_mut(readable::GBA_ID)
        .unwrap()
        .modifiers = layer("enabled: false, ", "game", 4);
    check(&snapshot, false, 4);
    let folded = resolve(&snapshot, &route());
    assert_eq!(
        serde_json::to_value(&folded[MODIFIER].settings).unwrap(),
        serde_json::json!({"shared":4,"device":1,"runner":2,"system":3,"game":4})
    );
    snapshot
        .library
        .get_mut(readable::GBA_ID)
        .unwrap()
        .modifiers = layer("", "game", 4);
    check(&snapshot, true, 4);
}

#[test]
fn coordinator_retains_device_key_order_and_rejects_invalid_modifier_ids() {
    let root = tempfile::tempdir().unwrap();
    readable::combined(root.path());
    let device = std::fs::read_to_string(root.path().join("device.yaml")).unwrap();
    std::fs::write(root.path().join("device.yaml"), format!("{device}\nmodifiers:\n  '@test:z/main': {{enabled: true}}\n  '@test:a/main': {{enabled: true}}\n")).unwrap();
    let loaded = korrid::config::snapshot::ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(loaded.diagnostic.is_none(), "{:?}", loaded.diagnostic);
    assert_eq!(
        loaded
            .snapshot
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
        assert!(decode_config_documents(&format!("modifiers: {{'{id}': {{}}}}"), "{}").is_err());
    }
    assert!(
        decode_config_documents("modifiers: {'@test:p/m': {}, '@test:p/m': {}}", "{}").is_err()
    );
    assert!(decode_config_documents("{}", "modifiers: {}").is_err());
}
