use korrid::plugin::{
    load_plugin_source, PluginRegistry, SessionControlDeclarationInteraction, SessionControlEffect,
};

const ANDROID_PLUGIN: &str = include_str!("../plugins/android-app.plugin.ts");
const MGBA_PLUGIN: &str = include_str!("../../../plugins/mgba/android/plugin.ts");
const RETROARCH_PLUGIN: &str = include_str!("../../../plugins/retroarch/android/plugin.ts");
const MOONLIGHT_PLUGIN: &str = include_str!("../../../plugins/moonlight/plugin.ts");

#[test]
fn publisher_identity_comes_from_composition_not_module_source() {
    let source = "export const name = 'clock'; export const title = 'Clock';";
    let plugin = load_plugin_source("@owner", source).unwrap();
    assert_eq!(plugin.id(), "@owner:clock");
    assert!(load_plugin_source(
        "@owner",
        &format!("{source} export const namespace = '@korri';")
    )
    .is_err());
}

#[test]
fn android_application_is_one_runner() {
    let plugin = load_plugin_source("@korri", ANDROID_PLUGIN).unwrap();
    let registry = PluginRegistry::new(vec![plugin], ["@korri:android-app".into()]).unwrap();
    let runner = &registry.runners()["@korri:android-app/android-app"];
    assert_eq!(runner.family.as_deref(), Some("@korri:android-app"));
    assert_eq!(runner.command.as_deref(), Some("android-app"));
    assert_eq!(runner.systems.as_deref(), Some(&["android".into()][..]));
}

#[test]
fn retroarch_is_a_scoped_family_not_an_executable_dependency() {
    let plugin = load_plugin_source("@korri", RETROARCH_PLUGIN).unwrap();
    let registry = PluginRegistry::new(vec![plugin], ["@korri:retroarch".into()]).unwrap();
    assert_eq!(
        registry.families()["@korri:retroarch"].title.as_deref(),
        Some("RetroArch")
    );
    assert!(registry.runners().is_empty());
}

#[test]
fn mgba_owns_its_android_runner_discovery_and_controls() {
    let mgba = load_plugin_source("@korri", MGBA_PLUGIN).unwrap();
    let registry = PluginRegistry::new(vec![mgba], ["@korri:mgba".into()]).unwrap();
    let runner = &registry.runners()["@korri:mgba/mgba"];
    assert_eq!(runner.family.as_deref(), Some("@korri:retroarch"));
    assert_eq!(runner.command.as_deref(), Some("retroarch"));
    assert_eq!(
        runner.core.as_deref(),
        Some("/data/data/com.korri.retroarch/cores/mgba_libretro_android.so")
    );
    assert_eq!(
        runner.android.as_ref().unwrap().package_name,
        "com.korri.retroarch"
    );
    let claim = &registry.file_release_discovery_claims_for_extension(".GBA")[0];
    assert_eq!(claim.runners, ["@korri:mgba/mgba"]);
    assert_eq!(
        registry.session_controls()["@korri:mgba/open-menu"].effect,
        SessionControlEffect::RetroarchOpenMenu
    );
}

#[test]
fn discovery_runner_order_is_explicit_and_unknown_ids_fail() {
    let source = MGBA_PLUGIN.replace(
        "runners: [\"@korri:mgba/mgba\"]",
        "runners: [\"@korri:missing/runner\", \"@korri:mgba/mgba\"]",
    );
    let mgba = load_plugin_source("@korri", &source).unwrap();
    assert!(PluginRegistry::new(vec![mgba], ["@korri:mgba".into()]).is_err());
}

#[test]
fn disabled_plugins_reserve_runner_identities_without_enabling_them() {
    let mgba = load_plugin_source("@korri", MGBA_PLUGIN).unwrap();
    let registry = PluginRegistry::new(vec![mgba], Vec::new()).unwrap();
    assert!(registry.runners().is_empty());
    assert!(registry.owns_registered_runner_id("@korri:mgba/mgba"));
    assert!(registry.owns_registered_system_id("gba"));
}

#[test]
fn moonlight_remains_a_transport_with_its_controls() {
    let plugin = load_plugin_source("@korri", MOONLIGHT_PLUGIN).unwrap();
    let registry = PluginRegistry::new(vec![plugin], ["@korri:moonlight".into()]).unwrap();
    let transport = &registry.transports()["@korri:moonlight/moonlight"];
    assert_eq!(
        transport.android.as_ref().unwrap().implementation.as_str(),
        "artemis"
    );
    let interaction = &registry.session_controls()["@korri:moonlight/mouse-mode"].interaction;
    assert!(matches!(
        interaction,
        SessionControlDeclarationInteraction::Choice { options } if options.len() == 6
    ));
}

/// A stranger may join any family, but naming one is a claim on an id every
/// other plugin resolves through. Only the owning publisher may make it.
#[test]
fn only_the_owning_namespace_may_name_a_family() {
    let declare = |family_id: &str| {
        format!(
            "export const name = 'ra'; \
             export const families = {{ ra: {{ id: '{family_id}', title: 'Mine' }} }};"
        )
    };
    assert!(load_plugin_source("@korri", &declare("@korri:retroarch")).is_ok());
    assert!(load_plugin_source("@alice", &declare("@korri:retroarch")).is_err());
    assert!(load_plugin_source("@alice", &declare("@alice:retroarch")).is_ok());

    // Joining stays open: a stranger's runner still references @korri:retroarch.
    let joiner = "export const name = 'ppsspp'; \
         export const systems = { psp: { id: 'psp', title: 'PSP' } }; \
         export const runners = { ppsspp: { id: '@alice:ppsspp/ppsspp', \
             family: '@korri:retroarch', command: 'ppsspp', systems: ['psp'] } };";
    let plugin = load_plugin_source("@alice", joiner).unwrap();
    let registry = PluginRegistry::new(vec![plugin], ["@alice:ppsspp".into()]).unwrap();
    assert_eq!(
        registry.runners()["@alice:ppsspp/ppsspp"].family.as_deref(),
        Some("@korri:retroarch")
    );
}

#[test]
fn malformed_runner_shapes_fail_without_legacy_fields() {
    for source in [
        MGBA_PLUGIN.replace("id: \"@korri:mgba/mgba\"", "id: \"@korri:mgba/other\""),
        MGBA_PLUGIN.replace("runners: [", "runtimes: ["),
        MGBA_PLUGIN.replace("owner: { kind: \"runner\"", "owner: { kind: \"runtime\""),
    ] {
        assert!(load_plugin_source("@korri", &source).is_err());
    }
}
