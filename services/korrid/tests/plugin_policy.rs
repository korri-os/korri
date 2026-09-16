use korrid::plugin::{load_plugin_source, PluginRegistry};
use korrid::plugin_policy::{
    bundled_plugin_policy_layer, bundled_plugins, empty_user_plugin_policy_layer,
    resolve_enabled_plugin_ids, PluginPolicyLayer,
};

const CHECKPOINT_ANDROID_PLUGIN: &str = include_str!(
    "../../../docs/research/android-app-plugin-schema-checkpoint/android-app.plugin.ts"
);
const PRODUCTION_ANDROID_PLUGIN: &str = include_str!("../plugins/android-app.plugin.ts");

#[test]
fn native_service_exports_do_not_change_registry_data_contributions() {
    let source = "export const name='network'; export const services=['tailscaled']; export const systems={};";
    let plugin = load_plugin_source("@example", source).unwrap();
    assert_eq!(plugin.id(), "@example:network");
    let registry = PluginRegistry::new(vec![plugin], ["@example:network".to_owned()]).unwrap();
    assert_eq!(registry.registered_plugin_ids(), ["@example:network"]);
    assert!(registry.systems().is_empty());
    for invalid in ["null", "['../daemon']", "['daemon','daemon']"] {
        assert!(
            load_plugin_source("@example", &source.replace("['tailscaled']", invalid)).is_err()
        );
    }
}

#[test]
fn bundled_policy_enables_first_party_android_plugins_by_default() {
    let plugins = bundled_plugins().expect("bundled plugins should load");
    let enabled_ids = resolve_enabled_plugin_ids([
        bundled_plugin_policy_layer(),
        empty_user_plugin_policy_layer(),
    ]);
    let registry =
        PluginRegistry::new(plugins, enabled_ids).expect("bundled policy should register");

    assert_eq!(
        registry.registered_plugin_ids(),
        [
            "@korri:android-app",
            "@korri:mgba",
            "@korri:moonlight",
            "@korri:retroarch"
        ]
    );
    assert_eq!(
        registry.enabled_plugin_ids(),
        [
            "@korri:android-app",
            "@korri:mgba",
            "@korri:moonlight",
            "@korri:retroarch"
        ]
    );
    assert!(registry.providers().contains_key("@korri:android-app"));
    assert!(registry.providers().contains_key("@korri:mgba"));
    assert!(registry.providers().contains_key("@korri:retroarch"));
    assert!(registry
        .systems()
        .contains_key("@korri:android-app/android"));
    assert!(registry
        .runners()
        .contains_key("@korri:android-app/android-app"));
    assert!(registry.families().contains_key("@korri:retroarch"));
    assert!(registry.runners().contains_key("@korri:mgba/mgba"));
    assert_eq!(
        registry
            .file_release_discovery_claims()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["@korri:mgba/gba-files"]
    );
}

#[test]
fn later_user_policy_layer_disables_bundled_moonlight_normally() {
    let plugins = bundled_plugins().expect("bundled plugins should load");
    let enabled_ids = resolve_enabled_plugin_ids([
        bundled_plugin_policy_layer(),
        PluginPolicyLayer::from_enabled([("@korri:moonlight", false)]),
    ]);
    let registry = PluginRegistry::new(plugins, enabled_ids)
        .expect("disabled plugin should remain registered");

    assert_eq!(
        registry.registered_plugin_ids(),
        [
            "@korri:android-app",
            "@korri:mgba",
            "@korri:moonlight",
            "@korri:retroarch"
        ]
    );
    assert_eq!(
        registry.enabled_plugin_ids(),
        ["@korri:android-app", "@korri:mgba", "@korri:retroarch"]
    );
    assert!(registry.owns_registered_transport_id("@korri:moonlight/moonlight"));
    assert!(!registry
        .transports()
        .contains_key("@korri:moonlight/moonlight"));
    assert!(!registry
        .session_controls()
        .keys()
        .any(|id| id.starts_with("@korri:moonlight/")));
}

#[test]
fn disabling_mgba_withholds_its_runner_and_discovery_but_not_the_family() {
    let plugins = bundled_plugins().expect("bundled plugins should load");
    let enabled_ids = resolve_enabled_plugin_ids([
        bundled_plugin_policy_layer(),
        PluginPolicyLayer::from_enabled([("@korri:mgba", false)]),
    ]);
    let registry = PluginRegistry::new(plugins, enabled_ids)
        .expect("disabled mGBA plugin should remain registered");

    assert!(registry.families().contains_key("@korri:retroarch"));
    assert!(!registry.runners().contains_key("@korri:mgba/mgba"));
    assert!(registry
        .file_release_discovery_claims_for_extension("gba")
        .is_empty());
    assert!(registry.owns_registered_runner_id("@korri:mgba/mgba"));
}

#[test]
fn unknown_enabled_policy_override_is_rejected_by_registry() {
    let plugins = bundled_plugins().expect("bundled plugins should load");
    let enabled_ids = resolve_enabled_plugin_ids([
        bundled_plugin_policy_layer(),
        PluginPolicyLayer::from_enabled([("@korri:missing", true)]),
    ]);

    let error = PluginRegistry::new(plugins, enabled_ids)
        .expect_err("unknown enabled policy ids must not create phantom plugins");

    assert!(
        error
            .to_string()
            .contains("enabled plugin @korri:missing is not registered"),
        "unexpected error: {error}"
    );
}

#[test]
fn retired_launcher_checkpoint_is_not_a_supported_plugin_contract() {
    let production = load_plugin_source("@korri", PRODUCTION_ANDROID_PLUGIN)
        .expect("production Android plugin should load");
    assert_eq!(production.id(), "@korri:android-app");
    assert!(load_plugin_source("@korri", CHECKPOINT_ANDROID_PLUGIN).is_err());
}
