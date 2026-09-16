use korrid::{
    plugin::{
        load_plugin_source, PluginRegistry, SessionControlDeclarationInteraction,
        SessionControlEffect,
    },
    plugin_installation::EnabledPackage,
};
use std::{collections::BTreeMap, fs, path::Path};

const ANDROID_PLUGIN: &str = include_str!("../plugins/android-app.plugin.ts");
const MGBA_PLUGIN: &str = include_str!("../../../plugins/mgba/android/plugin.ts");
const RETROARCH_PLUGIN: &str = include_str!("../../../plugins/retroarch/android/plugin.ts");
const MOONLIGHT_PLUGIN: &str = include_str!("../../../plugins/moonlight/plugin.ts");

const LINUX_RETROARCH_PLUGIN: &str = include_str!("../../../plugins/retroarch/plugin.ts");
const GENERATED_CORE: &str = include_str!("../examples/libretro-core.plugin.ts");
const RETROARCH_HELPER: &str = include_str!("../../../plugins/libretro/retroarch.ts");

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

fn install(root: &Path, id: &str, source: &str, files: &[&str]) -> EnabledPackage {
    let package = root.join(id.replace(['@', ':'], "-"));
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    let mut sources = vec!["plugin.ts".to_owned()];
    if source.contains("./retroarch") {
        fs::write(package.join("retroarch.ts"), RETROARCH_HELPER).unwrap();
        // The catalogue generates this module from the pinned program's own
        // source; the helper answers settings.describe and validate from it.
        fs::write(
            package.join("settings.ts"),
            "export const version = \"1.22.2\"\nexport const keys = {\n  video_vsync: \"Boolean\",\n}\n",
        )
        .unwrap();
        sources.push("retroarch.ts".to_owned());
        sources.push("settings.ts".to_owned());
    }
    EnabledPackage {
        id: id.to_owned(),
        package,
        files: files
            .iter()
            .map(|key| ((*key).to_owned(), root.join(format!("{id}-{key}"))))
            .collect::<BTreeMap<_, _>>(),
        entry: "plugin.ts".into(),
        sources,
    }
}

/// The generator emits one shape for every catalogue entry, and
/// plugins/libretro/example-check.nix holds the committed mGBA example to that
/// shape. Deriving the second core from it keeps one source of truth for what a
/// generated plugin looks like. The identifiers match the real snes9x2010
/// catalogue entry.
fn second_generated_core() -> String {
    GENERATED_CORE
        .replace("mgba", "snes9x2010")
        .replace("gba", "snes")
        .replace("extensions: [\"snes\"]", "extensions: [\"sfc\"]")
        .replace("mGBA", "Snes9x 2010")
        .replace("Game Boy Advance", "Super Nintendo")
}

/// The catalogue's whole promise: install two cores of one family and both work.
/// This broke on hardware once, so it is held by a test and not by a device.
#[test]
fn two_cores_of_one_family_coexist() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let registry = PluginRegistry::from_installed(vec![
        install(root, "@korri:retroarch", LINUX_RETROARCH_PLUGIN, &[]),
        install(
            root,
            "@korri:mgba",
            GENERATED_CORE,
            &["retroarch", "autoconfig", "mgba"],
        ),
        install(
            root,
            "@korri:snes9x2010",
            &second_generated_core(),
            &["retroarch", "autoconfig", "snes9x2010"],
        ),
    ])
    .unwrap();

    // One family record, declared once, shared by both runners.
    assert_eq!(
        registry.families()["@korri:retroarch"].title.as_deref(),
        Some("RetroArch")
    );
    for runner_id in ["@korri:mgba/mgba", "@korri:snes9x2010/snes9x2010"] {
        let runner = &registry.runners()[runner_id];
        assert_eq!(runner.family.as_deref(), Some("@korri:retroarch"));
        assert_eq!(runner.program.as_deref(), Some("retroarch"));
    }
    // Each core resolves its own frontend and its own core file from its own
    // package. Two installed cores share no file, so neither can break the other.
    registry.native_runner("@korri:mgba/mgba").unwrap();
    registry
        .native_runner("@korri:snes9x2010/snes9x2010")
        .unwrap();
    assert_ne!(
        registry
            .installed_file("@korri:mgba/mgba", "retroarch")
            .unwrap(),
        registry
            .installed_file("@korri:snes9x2010/snes9x2010", "retroarch")
            .unwrap()
    );

    // Discovery keeps each system pointed at its own core.
    assert_eq!(
        registry.file_release_discovery_claims_for_extension(".GBA")[0].runners,
        ["@korri:mgba/mgba"]
    );
    assert_eq!(
        registry.file_release_discovery_claims_for_extension(".SFC")[0].runners,
        ["@korri:snes9x2010/snes9x2010"]
    );

    // Both cores reach the family's controls; neither holds a right the other lacks.
    for (control_id, effect) in [
        ("@korri:mgba/open-menu", SessionControlEffect::RetroarchOpenMenu),
        ("@korri:mgba/quit", SessionControlEffect::RetroarchQuit),
        (
            "@korri:snes9x2010/open-menu",
            SessionControlEffect::RetroarchOpenMenu,
        ),
        (
            "@korri:snes9x2010/quit",
            SessionControlEffect::RetroarchQuit,
        ),
    ] {
        assert_eq!(registry.session_controls()[control_id].effect, effect);
    }
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
