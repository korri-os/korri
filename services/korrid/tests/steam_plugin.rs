//! Run the shipped Steam source through the same QuickJS harness as PPSSPP.
use korrid::{
    plugin::{load_plugin_source, PluginRegistry},
    script::{call_plugin_operation_ts, LAUNCH_PREPARE},
};

const STEAM_PLUGIN: &str = include_str!("../../../plugins/steam/plugin.ts");

fn input() -> serde_json::Value {
    serde_json::json!({
        "runnerId": "@korri:steam/steam",
        "program": "/nix/store/exact-steam/bin/steam-session",
        "ref": "401710",
        "accountRoot": "/storage/korri/users/default",
        "cacheRoot": "/storage/korri/cache",
        "files": {"steam-session": "/nix/store/exact-steam/bin/steam-session"},
        "overrides": {"settings": {}},
    })
}

#[test]
fn steam_declares_only_its_system_and_native_session_runner() {
    let plugin = load_plugin_source("@korri", STEAM_PLUGIN).unwrap();
    let registry = PluginRegistry::new(vec![plugin], ["@korri:steam".into()]).unwrap();
    let runner = &registry.runners()["@korri:steam/steam"];
    assert_eq!(runner.program.as_deref(), Some("steam-session"));
    assert_eq!(
        runner.systems.as_deref(),
        Some(["steam".to_string()].as_slice())
    );
    assert!(runner.family.is_none());
    assert!(runner.core.is_none());
    assert_eq!(registry.systems().len(), 1);
    assert!(
        registry
            .content_rules("steam")
            .unwrap()
            .extensions
            .is_empty(),
        "no discovery extensions"
    );
    assert!(registry.session_controls().is_empty());
}

#[test]
fn steam_prepares_the_session_command_ref_and_account_home_without_a_file() {
    let result =
        call_plugin_operation_ts(STEAM_PLUGIN, LAUNCH_PREPARE, &input().to_string()).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(
        plan,
        serde_json::json!({
            "command": "/nix/store/exact-steam/bin/steam-session",
            "args": ["401710"],
            "env": {"STEAM_HOME": "/storage/korri/users/default/steam"},
            "directories": ["/storage/korri/users/default/steam"],
            "envUnset": [],
        })
    );
}

#[test]
fn steam_refuses_a_missing_or_empty_provider_ref() {
    for reference in [None, Some(serde_json::json!(""))] {
        let mut input = input();
        input.as_object_mut().unwrap().remove("ref");
        if let Some(reference) = reference {
            input["ref"] = reference;
        }
        assert!(
            call_plugin_operation_ts(STEAM_PLUGIN, LAUNCH_PREPARE, &input.to_string()).is_err()
        );
    }
}

#[test]
fn steam_refuses_settings_and_raw_config_it_cannot_apply() {
    for overrides in [
        serde_json::json!({"settings": {"video_vsync": true}}),
        serde_json::json!({"config": {"append": "anything"}}),
        serde_json::json!({"config": {}}),
    ] {
        let mut input = input();
        input["overrides"] = overrides;
        assert!(
            call_plugin_operation_ts(STEAM_PLUGIN, LAUNCH_PREPARE, &input.to_string()).is_err()
        );
    }
}
