use korrid::plugin::{
    decode_plugin_declaration, load_plugin_source, PluginRegistry,
    SessionControlDeclarationInteraction, SessionControlEffect,
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
    assert!(
        decode_plugin_declaration("@owner", r#"{"name":"clock","namespace":"@korri"}"#).is_err()
    );
}

#[test]
fn enabled_android_plugin_announces_its_legacy_contributions() {
    let plugin =
        load_plugin_source("@korri", ANDROID_PLUGIN).expect("checkpoint plugin should load");
    let registry = PluginRegistry::new(vec![plugin], vec!["@korri:android-app".to_owned()])
        .expect("checkpoint plugin should register");

    assert_eq!(registry.registered_plugin_ids(), ["@korri:android-app"]);
    assert_eq!(registry.enabled_plugin_ids(), ["@korri:android-app"]);
    assert_eq!(
        registry
            .providers()
            .get("@korri:android-app")
            .map(|record| record.id.as_str()),
        Some("@korri:android-app")
    );
    assert_eq!(
        registry
            .systems()
            .get("@korri:android-app/android")
            .map(|record| record.id.as_str()),
        Some("android")
    );
    assert_eq!(
        registry
            .launchers()
            .get("@korri:android-app/android-app")
            .map(|record| record.id.as_str()),
        Some("@korri:android-app/android-app")
    );
}

#[test]
fn enabled_retroarch_plugin_announces_only_its_launcher_component() {
    let plugin =
        load_plugin_source("@korri", RETROARCH_PLUGIN).expect("RetroArch plugin should load");
    let registry = PluginRegistry::new(vec![plugin], vec!["@korri:retroarch".to_owned()])
        .expect("RetroArch plugin should register");

    let launcher = registry
        .launchers()
        .get("@korri:retroarch/retroarch")
        .expect("plugin launcher");
    assert_eq!(
        launcher
            .android
            .as_ref()
            .map(|android| android.package_name.as_str()),
        Some("com.korri.retroarch")
    );
    assert!(launcher.systems.is_none());
    assert!(registry.systems().is_empty());
    assert!(registry.runtimes().is_empty());
}

#[test]
fn enabled_mgba_plugin_announces_its_system_and_runtime() {
    let retroarch =
        load_plugin_source("@korri", RETROARCH_PLUGIN).expect("RetroArch plugin should load");
    let mgba = load_plugin_source("@korri", MGBA_PLUGIN).expect("mGBA plugin should load");
    let registry = PluginRegistry::new(
        vec![retroarch, mgba],
        vec!["@korri:retroarch".to_owned(), "@korri:mgba".to_owned()],
    )
    .expect("mGBA plugin should register");

    assert_eq!(
        registry
            .systems()
            .get("@korri:mgba/gba")
            .map(|system| system.id.as_str()),
        Some("gba")
    );
    let runtime = registry
        .runtimes()
        .get("@korri:mgba/mgba")
        .expect("plugin runtime");
    assert_eq!(runtime.kind, "libretro-core");
    assert_eq!(runtime.app.as_deref(), Some("@korri:retroarch/retroarch"));
    assert_eq!(
        runtime.path,
        "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so"
    );
}

#[test]
fn enabled_moonlight_plugin_declares_artemis_streaming_and_the_full_control_inventory() {
    let plugin =
        load_plugin_source("@korri", MOONLIGHT_PLUGIN).expect("Moonlight plugin should load");
    let registry = PluginRegistry::new(vec![plugin], vec!["@korri:moonlight".to_owned()])
        .expect("Moonlight plugin should register");

    assert_eq!(registry.registered_plugin_ids(), ["@korri:moonlight"]);
    let transport = registry
        .transports()
        .get("@korri:moonlight/moonlight")
        .expect("stable Moonlight transport");
    let android = transport.android.as_ref().expect("Android implementation");
    assert_eq!(android.implementation.as_str(), "artemis");
    assert_eq!(android.sunshine_app, "Korri Stream");
    let mouse_modes = match &registry
        .session_controls()
        .get("@korri:moonlight/mouse-mode")
        .expect("mouse mode control")
        .interaction
    {
        SessionControlDeclarationInteraction::Choice { options } => options,
        other => panic!("expected mouse choices, got {other:?}"),
    };
    assert_eq!(
        mouse_modes
            .iter()
            .map(|option| (option.value.as_str(), option.label.as_str()))
            .collect::<Vec<_>>(),
        [
            ("0", "Multi touch"),
            ("1", "Absolute touch"),
            ("2", "Track pad(Natural/Double tap to drag)"),
            ("3", "Track pad(Gaming/Long press to drag)"),
            ("4", "Disabled"),
            ("5", "Absolute touch (left/right click swapped)"),
        ]
    );
    let local_cursor = registry
        .session_controls()
        .get("@korri:moonlight/local-cursor")
        .expect("local cursor control");
    assert!(local_cursor.label.contains("physical mouse needed"));
    assert_eq!(
        registry
            .session_controls()
            .get("@korri:moonlight/sgsr-edge-threshold")
            .expect("SGSR edge threshold control")
            .effect,
        SessionControlEffect::MoonlightSetSgsrEdgeThreshold
    );
    assert_eq!(
        registry
            .session_controls()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "@korri:moonlight/disconnect",
            "@korri:moonlight/face-button-flip",
            "@korri:moonlight/fill",
            "@korri:moonlight/floating-menu",
            "@korri:moonlight/full-keyboard",
            "@korri:moonlight/hud",
            "@korri:moonlight/keyboard",
            "@korri:moonlight/keyboard-controller",
            "@korri:moonlight/local-cursor",
            "@korri:moonlight/mouse-mode",
            "@korri:moonlight/pan-zoom",
            "@korri:moonlight/picture-in-picture",
            "@korri:moonlight/quit-host",
            "@korri:moonlight/restore-stream-bitrate",
            "@korri:moonlight/restore-stream-fps",
            "@korri:moonlight/restore-stream-resolution",
            "@korri:moonlight/rotate-screen",
            "@korri:moonlight/rumble",
            "@korri:moonlight/sgsr-edge-threshold",
            "@korri:moonlight/sgsr-sharpness",
            "@korri:moonlight/stream-bitrate",
            "@korri:moonlight/stream-fps",
            "@korri:moonlight/stream-width",
            "@korri:moonlight/touch-sensitivity",
        ]
    );
}

#[test]
fn plugin_system_aliases_follow_the_readable_system_shape() {
    let plugin = decode_plugin_declaration("@korri",
        r#"{"name":"other","systems":{"gba":{"id":"gba","title":"Game Boy Advance","aliases":["game-boy-advance"]}}}"#,
    )
    .expect("system aliases should decode like readable systems");
    let registry = PluginRegistry::new(vec![plugin], vec!["@korri:other".to_owned()])
        .expect("plugin should register");

    assert_eq!(
        registry
            .systems()
            .get("@korri:other/gba")
            .and_then(|system| system.aliases.as_deref()),
        Some(&["game-boy-advance".to_owned()][..])
    );
}

#[test]
fn enabled_mgba_plugin_announces_gba_file_release_discovery_claim() {
    let retroarch =
        load_plugin_source("@korri", RETROARCH_PLUGIN).expect("RetroArch plugin should load");
    let mgba = load_plugin_source("@korri", MGBA_PLUGIN).expect("mGBA plugin should load");
    let registry = PluginRegistry::new(
        vec![retroarch, mgba],
        vec!["@korri:retroarch".to_owned(), "@korri:mgba".to_owned()],
    )
    .expect("mGBA discovery claim should register");

    let claims = registry.file_release_discovery_claims_for_extension(".GBA");
    assert_eq!(claims.len(), 1);
    let claim = claims[0];
    assert_eq!(claim.id, "@korri:mgba/gba-files");
    assert_eq!(claim.extensions, vec!["gba".to_owned()]);
    assert_eq!(claim.system, "gba");
    assert_eq!(claim.launcher, "@korri:retroarch/retroarch");
    assert_eq!(claim.runtime.as_deref(), Some("@korri:mgba/mgba"));
    assert!(registry
        .file_release_discovery_claims_for_extension("nes")
        .is_empty());
}

#[test]
fn malformed_discovery_claims_fail_explicitly() {
    let malformed_at_decode = [
        MGBA_PLUGIN.replace(
            "id: \"@korri:mgba/gba-files\"",
            "id: \"@korri:retroarch/gba-files\"",
        ),
        MGBA_PLUGIN.replace("extensions: [\"gba\"]", "extensions: [\"gba\", \".GBA\"]"),
        MGBA_PLUGIN.replace("extensions: [\"gba\"]", "extensions: []"),
        MGBA_PLUGIN.replace(
            "extensions: [\"gba\"]",
            "extensions: [\"gba\"], discover: () => []",
        ),
    ];
    for source in malformed_at_decode {
        assert!(
            load_plugin_source("@korri", &source).is_err(),
            "malformed discovery declaration unexpectedly loaded: {source}"
        );
    }

    for source in [
        MGBA_PLUGIN.replace("system: \"gba\"", "system: \"gb\""),
        MGBA_PLUGIN.replace(
            "launcher: \"@korri:retroarch/retroarch\"",
            "launcher: \"@korri:missing/retroarch\"",
        ),
        MGBA_PLUGIN.replace(
            "runtime: \"@korri:mgba/mgba\"",
            "runtime: \"@korri:missing/mgba\"",
        ),
    ] {
        let retroarch =
            load_plugin_source("@korri", RETROARCH_PLUGIN).expect("RetroArch plugin should load");
        let mgba = load_plugin_source("@korri", &source).expect("plugin shape should decode");
        PluginRegistry::new(
            vec![retroarch, mgba],
            vec!["@korri:retroarch".to_owned(), "@korri:mgba".to_owned()],
        )
        .expect_err("unknown discovery route identities must reject the registry");
    }
}

#[test]
fn malformed_moonlight_android_implementation_is_rejected_strictly() {
    for source in [
        MOONLIGHT_PLUGIN.replace("implementation: \"artemis\"", "implementation: \"other\""),
        MOONLIGHT_PLUGIN.replace("sunshineApp: \"Korri Stream\"", "sunshineApp: \"\""),
        MOONLIGHT_PLUGIN.replace(
            "sunshineApp: \"Korri Stream\"",
            "sunshineApp: \"Korri Stream\", unknown: true",
        ),
        MOONLIGHT_PLUGIN.replace("android: {", "android: null, ignored: {"),
    ] {
        load_plugin_source("@korri", &source).expect_err("invalid Artemis declaration must fail");
    }
}

#[test]
fn disabled_plugins_reserve_only_their_own_contribution_identities() {
    let retroarch = PluginRegistry::new(
        vec![load_plugin_source("@korri", RETROARCH_PLUGIN).unwrap()],
        Vec::new(),
    )
    .expect("RetroArch plugin should register");
    assert!(retroarch.owns_registered_launcher_id("@korri:retroarch/retroarch"));
    assert!(!retroarch.owns_registered_runtime_id("@korri:mgba/mgba"));
    assert!(!retroarch.owns_registered_system_id("gba"));

    let mgba = PluginRegistry::new(
        vec![load_plugin_source("@korri", MGBA_PLUGIN).unwrap()],
        Vec::new(),
    )
    .expect("mGBA plugin should register");
    assert!(mgba.owns_registered_runtime_id("@korri:mgba/mgba"));
    assert!(mgba.owns_registered_system_id("gba"));
    assert!(!mgba.owns_registered_launcher_id("@korri:retroarch/retroarch"));
}

#[test]
fn malformed_runtime_and_android_launcher_fields_are_rejected() {
    let invalid_sources = [
        MGBA_PLUGIN.replace("id: \"@korri:mgba/mgba\"", "id: \"@korri:mgba/wrong\""),
        MGBA_PLUGIN.replace("kind: \"libretro-core\"", "kind: \"\""),
        MGBA_PLUGIN.replace("app: \"@korri:retroarch/retroarch\"", "app: \"\""),
        MGBA_PLUGIN.replace(
            "path: \"/data/data/com.korri.retroarch/cores/mgba_libretro_android.so\"",
            "path: \"relative/mgba.so\"",
        ),
        MGBA_PLUGIN.replace(
            "path: \"/data/data/com.korri.retroarch/cores/mgba_libretro_android.so\"",
            "path: \"/cores/bad\\\"name.so\"",
        ),
        RETROARCH_PLUGIN.replace(
            "packageName: \"com.korri.retroarch\"",
            "packageName: \"com.korri/bad\"",
        ),
        MGBA_PLUGIN.replace("supports: {", "unknownRuntimeField: true, supports: {"),
    ];

    for source in invalid_sources {
        assert!(
            load_plugin_source("@korri", &source).is_err(),
            "malformed declaration unexpectedly loaded: {source}"
        );
    }
}

#[test]
fn disabled_plugin_announces_no_contributions_but_reserves_its_record_identities() {
    let plugin =
        load_plugin_source("@korri", ANDROID_PLUGIN).expect("checkpoint plugin should load");
    let registry = PluginRegistry::new(vec![plugin], Vec::new()).expect("plugin should register");

    assert_eq!(registry.registered_plugin_ids(), ["@korri:android-app"]);
    assert!(registry.enabled_plugin_ids().is_empty());
    assert!(registry.providers().is_empty());
    assert!(registry.systems().is_empty());
    assert!(registry.launchers().is_empty());
    assert!(registry.owns_registered_provider_id("@korri:android-app"));
    assert!(registry.owns_registered_system_id("android"));
    assert!(registry.owns_registered_launcher_id("@korri:android-app/android-app"));
    assert!(!registry.owns_registered_provider_id("@korri:other"));
    assert!(!registry.owns_registered_system_id("other-system"));
    assert!(!registry.owns_registered_launcher_id("@korri:other/android-app"));
}

#[test]
fn implicit_own_provider_is_normalized_with_its_provider_id() {
    let plugin = load_plugin_source(
        "@korri",
        r#"
export const name = "android-app";
export const title = "Android";
"#,
    )
    .expect("plugin should load");
    let registry = PluginRegistry::new(vec![plugin], vec!["@korri:android-app".to_owned()])
        .expect("plugin should register");

    let provider = registry
        .providers()
        .get("@korri:android-app")
        .expect("own provider should be synthesized");
    assert_eq!(provider.id, "@korri:android-app");
    assert_eq!(provider.title.as_deref(), Some("Android"));
}

#[test]
fn provider_contribution_keys_supply_the_readable_provider_id() {
    let plugin = load_plugin_source(
        "@korri",
        r#"
export const name = "android-app";
export const providers = {
                "@korri:android-app": { title: "Android applications" },
              };
"#,
    )
    .expect("provider contribution key should supply its id");
    let registry = PluginRegistry::new(vec![plugin], vec!["@korri:android-app".to_owned()])
        .expect("plugin should register");

    let provider = registry
        .providers()
        .get("@korri:android-app")
        .expect("provider should be announced");
    assert_eq!(provider.id, "@korri:android-app");
    assert_eq!(provider.title.as_deref(), Some("Android applications"));
}

#[test]
fn non_json_fields_fail_before_json_stringification_can_erase_them() {
    let error = load_plugin_source(
        "@korri",
        r#"
export const name = "android-app";
export const providers = {
                "@korri:android-app": {
                  title: "Android",
                  unsupported: undefined,
                },
              };
"#,
    )
    .expect_err("undefined fields must not disappear before strict decoding");

    assert!(
        error.to_string().contains("not JSON data"),
        "unexpected error: {error}"
    );
}

#[test]
fn accessors_and_to_json_cannot_change_the_validated_declaration() {
    let accessor_error = load_plugin_source(
        "@korri",
        r#"
        let reads = 0
        const provider = { title: "Android" }
        Object.defineProperty(provider, "unsupported", {
          enumerable: true,
          get() { reads += 1; return reads === 1 ? true : undefined },
        })

export const name = "android-app";
export const providers = { "@korri:android-app": provider };
"#,
    )
    .expect_err("an accessor must be captured once before strict decoding");
    assert!(
        accessor_error.to_string().contains("unsupported"),
        "unexpected error: {accessor_error}"
    );

    let to_json_error = load_plugin_source(
        "@korri",
        r#"
        const invalidTitle = Object.create({ toJSON() { return "Android" } })

export const name = "android-app";
export const title = invalidTitle;
"#,
    )
    .expect_err("custom JSON serialization must not replace declaration values");
    assert!(
        to_json_error.to_string().contains("not JSON data"),
        "unexpected error: {to_json_error}"
    );
}

#[test]
fn non_plain_objects_cannot_collapse_into_empty_declaration_maps() {
    for source in [
        r#"
export const name = "android-app";
export const systems = new Boolean(false);
"#,
        r#"
export const name = "android-app";
export const systems = new Date();
"#,
        r#"
export const name = "android-app";
export const providers = new Map();
"#,
        r#"const value = new Date(); Object.setPrototypeOf(value, Object.prototype)
export const name = "android-app";
export const systems = value;
"#,
    ] {
        let error = load_plugin_source("@korri", source)
            .expect_err("non-plain objects must not become empty declaration maps");
        assert!(
            error.to_string().contains("not JSON data"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn malformed_provider_contributions_fail_instead_of_disappearing() {
    let error = load_plugin_source(
        "@korri",
        r#"
export const name = "android-app";
export const providers = {
                "@korri:android-app": {
                  id: "@korri:android-app",
                  title: "Android",
                  unsupported: true,
                },
              };
"#,
    )
    .expect_err("unsupported provider fields must fail explicitly");

    assert!(
        error.to_string().contains("unsupported"),
        "unexpected error: {error}"
    );
}

#[test]
fn contribution_identity_mismatches_are_rejected() {
    let declarations = [
        r#"{"name":"android-app","providers":{"android-app":{"title":"Android"}}}"#,
        r#"{"name":"android-app","providers":{"@korri:android-app":{"id":"@korri:other"}}}"#,
        r#"{"name":"android-app","systems":{"android":{"id":"switch"}}}"#,
        r#"{"name":"android-app","launchers":{"android-app":{"id":"@korri:other/android-app"}}}"#,
    ];

    for declaration in declarations {
        decode_plugin_declaration("@korri", declaration)
            .expect_err("contribution identities must agree with their registry keys");
    }
}

#[test]
fn empty_keys_and_malformed_launcher_fields_are_rejected() {
    let declarations = [
        r#"{"name":"android-app","systems":{"":{"id":""}}}"#,
        r#"{"name":"android-app","launchers":{"":{"id":"@korri:android-app/"}}}"#,
        r#"{"name":"android-app","launchers":{"android-app":{"id":"@korri:android-app/android-app","plugin":"android-app"}}}"#,
        r#"{"name":"android-app","launchers":{"android-app":{"id":"@korri:android-app/android-app","command":""}}}"#,
    ];

    for declaration in declarations {
        decode_plugin_declaration("@korri", declaration)
            .expect_err("malformed contribution values must fail explicitly");
    }
}

#[test]
fn explicit_nulls_do_not_pass_as_absent_legacy_fields() {
    let declarations = [
        r#"{"name":"android-app","title":null}"#,
        r#"{"name":"android-app","providers":{"@korri:android-app":{"title":null}}}"#,
        r#"{"name":"android-app","systems":{"android":{"id":"android","title":null}}}"#,
        r#"{"name":"android-app","launchers":{"android-app":{"id":"@korri:android-app/android-app","plugin":null}}}"#,
        r#"{"name":"android-app","launchers":{"android-app":{"id":"@korri:android-app/android-app","command":null}}}"#,
        r#"{"name":"android-app","launchers":{"android-app":{"id":"@korri:android-app/android-app","systems":null}}}"#,
    ];

    for declaration in declarations {
        let error = decode_plugin_declaration("@korri", declaration)
            .expect_err("explicit null must not be normalized to an absent field");
        assert!(
            error.to_string().contains("invalid plugin declaration"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn unknown_enabled_plugin_ids_are_rejected() {
    let plugin = load_plugin_source("@korri", ANDROID_PLUGIN).expect("plugin should load");

    let error = PluginRegistry::new(vec![plugin], vec!["@korri:missing".to_owned()])
        .expect_err("unknown enabled plugin ids must fail");

    assert!(
        error
            .to_string()
            .contains("enabled plugin @korri:missing is not registered"),
        "unexpected error: {error}"
    );
}

#[test]
fn multiple_plugins_announce_only_when_each_is_enabled() {
    let android = load_plugin_source("@korri", ANDROID_PLUGIN).expect("Android plugin should load");
    let other = decode_plugin_declaration(
        "@korri",
        r#"{"name":"other","systems":{"other":{"id":"other"}}}"#,
    )
    .expect("second plugin should decode");

    let both = PluginRegistry::new(
        vec![android.clone(), other.clone()],
        vec!["@korri:android-app".to_owned(), "@korri:other".to_owned()],
    )
    .expect("both plugins should register");
    assert_eq!(
        both.registered_plugin_ids(),
        ["@korri:android-app", "@korri:other"]
    );
    assert!(both.providers().get("@korri:android-app").is_some());
    assert!(both.providers().get("@korri:other").is_some());
    assert!(both.systems().get("@korri:other/other").is_some());

    let android_only =
        PluginRegistry::new(vec![android, other], vec!["@korri:android-app".to_owned()])
            .expect("disabled second plugin should remain registered");
    assert!(android_only.providers().get("@korri:android-app").is_some());
    assert!(android_only.providers().get("@korri:other").is_none());
    assert!(android_only.systems().get("@korri:other/other").is_none());
}

#[test]
fn strict_session_control_declarations_reserve_disabled_identities() {
    let source = r#"
export const name = "retroarch";
export const launchers = {
              retroarch: {
                id: "@korri:retroarch/retroarch",
                plugin: "@korri:retroarch",
                command: "retroarch",
              },
            };
export const sessionControls = {
            openMenu: {
              order: 0,
              id: "@korri:retroarch/open-menu",
              owner: { kind: "launcher", id: "@korri:retroarch/retroarch" },
              label: "Open RetroArch menu",
              interaction: { kind: "command" },
              effect: "@korri:retroarch/open-menu",
              dismissOnSuccess: true,
            },
          };
"#;
    let plugin =
        load_plugin_source("@korri", source).expect("session-control declaration should load");

    let disabled = PluginRegistry::new(vec![plugin.clone()], Vec::new())
        .expect("disabled plugin should remain registered");
    assert!(disabled.owns_registered_session_control_id("@korri:retroarch/open-menu"));
    assert!(disabled.session_controls().is_empty());

    let enabled = PluginRegistry::new(vec![plugin], vec!["@korri:retroarch".to_owned()])
        .expect("enabled plugin should announce its control");
    let control = enabled
        .session_controls()
        .get("@korri:retroarch/open-menu")
        .expect("enabled control");
    assert_eq!(control.plugin_id, "@korri:retroarch");
    assert_eq!(control.local_id, "openMenu");
    assert_eq!(control.label, "Open RetroArch menu");
}

#[test]
fn malformed_session_controls_and_arbitrary_effect_payloads_are_rejected() {
    let valid = r#"
export const name = "moonlight";
export const transports = {
              moonlight: { id: "@korri:moonlight/moonlight" },
            };
export const sessionControls = {
            sharpness: {
              order: 0,
              id: "@korri:moonlight/sharpness",
              owner: { kind: "transport", id: "@korri:moonlight/moonlight" },
              label: "Sharpness",
              interaction: { kind: "range", min: 0, max: 100, step: 5 },
              effect: "@korri:moonlight/set-sgsr-sharpness",
            },
          };
"#;

    let malformed = [
        valid.replace("order: 0,", ""),
        valid.replace("order: 0", "order: -1"),
        valid.replace("order: 0", "order: 65536"),
        valid.replace("step: 5", "step: 0"),
        valid.replace("min: 0, max: 100", "min: 101, max: 100"),
        valid.replace("label: \"Sharpness\"", "label: \"\""),
        valid.replace(
            "effect: \"@korri:moonlight/set-sgsr-sharpness\"",
            "effect: \"\"",
        ),
        valid.replace(
            "effect: \"@korri:moonlight/set-sgsr-sharpness\"",
            "effect: { process: \"sh\", args: [\"-c\", \"id\"] }",
        ),
        valid.replace(
            "effect: \"@korri:moonlight/set-sgsr-sharpness\"",
            "effect: { url: \"https://example.invalid\" }",
        ),
        valid.replace(
            "effect: \"@korri:moonlight/set-sgsr-sharpness\"",
            "effect: { intent: \"android.intent.action.VIEW\" }",
        ),
        valid.replace(
            "effect: \"@korri:moonlight/set-sgsr-sharpness\"",
            "effect: { socket: \"127.0.0.1:55355\" }",
        ),
        valid.replace(
            "effect: \"@korri:moonlight/set-sgsr-sharpness\"",
            "effect: { javaMethod: \"finish\" }",
        ),
        valid.replace("step: 5", "step: 5, unknown: true"),
        valid.replace(
            "owner: { kind: \"transport\", id: \"@korri:moonlight/moonlight\" }",
            "owner: { kind: \"transport\", id: \"@korri:moonlight/moonlight\", unknown: true }",
        ),
        valid.replace(
            "id: \"@korri:moonlight/sharpness\"",
            "pluginId: \"@korri:moonlight\", id: \"@korri:moonlight/sharpness\"",
        ),
        valid.replace(
            "id: \"@korri:moonlight/sharpness\"",
            "localId: \"sharpness\", id: \"@korri:moonlight/sharpness\"",
        ),
        valid.replace("label: \"Sharpness\"", "label: null"),
        valid.replace("kind: \"range\"", "kind: \"slider\""),
        valid.replace(
            "owner: { kind: \"transport\", id: \"@korri:moonlight/moonlight\" }",
            "owner: { kind: \"transport\", id: \"@korri:retroarch/retroarch\" }",
        ),
        valid.replace(
            "effect: \"@korri:moonlight/set-sgsr-sharpness\"",
            "effect: \"@korri:retroarch/open-menu\"",
        ),
        valid.replace(
            "id: \"@korri:moonlight/sharpness\"",
            "id: \"@korri:retroarch/open-menu\"",
        ),
    ];

    for source in malformed {
        load_plugin_source("@korri", &source)
            .expect_err("unsafe session-control declaration must fail");
    }
}

#[test]
fn malformed_choice_and_duplicate_session_control_identities_are_rejected() {
    let empty_options = r#"
export const name = "moonlight";
export const transports = { moonlight: { id: "@korri:moonlight/moonlight" } };
export const sessionControls = {
            mouse: {
              order: 0,
              id: "@korri:moonlight/mouse",
              owner: { kind: "transport", id: "@korri:moonlight/moonlight" },
              label: "Mouse",
              interaction: { kind: "choice", options: [] },
              effect: "@korri:moonlight/set-mouse-mode",
            },
          };
"#;
    load_plugin_source("@korri", empty_options).expect_err("choice controls need options");
    load_plugin_source("@korri", &empty_options.replace(
        "options: []",
        "options: [{ value: \"direct\", label: \"Direct\" }, { value: \"direct\", label: \"Again\" }]",
    ))
    .expect_err("choice option identities must be unique");

    let duplicate_global_id = r#"
export const name = "moonlight";
export const transports = { moonlight: { id: "@korri:moonlight/moonlight" } };
export const sessionControls = {
            first: {
              order: 0,
              id: "@korri:moonlight/shared",
              owner: { kind: "transport", id: "@korri:moonlight/moonlight" },
              label: "First",
              interaction: { kind: "command" },
              effect: "@korri:moonlight/set-local-cursor",
            },
            second: {
              order: 1,
              id: "@korri:moonlight/shared",
              owner: { kind: "transport", id: "@korri:moonlight/moonlight" },
              label: "Second",
              interaction: { kind: "command" },
              effect: "@korri:moonlight/set-local-cursor",
            },
          };
"#;
    let plugin = load_plugin_source("@korri", duplicate_global_id)
        .expect("duplicate global ids are a registry-level collision");
    PluginRegistry::new(vec![plugin], vec!["@korri:moonlight".to_owned()])
        .expect_err("duplicate global session-control ids must fail registry construction");

    load_plugin_source(
        "@korri",
        &duplicate_global_id
            .replacen("@korri:moonlight/shared", "@korri:moonlight/first", 1)
            .replacen("@korri:moonlight/shared", "@korri:moonlight/second", 1)
            .replace("order: 1", "order: 0"),
    )
    .expect_err("session-control order collisions for one contributor must fail");
}

#[test]
fn duplicate_plugin_ids_are_rejected() {
    let first = load_plugin_source("@korri", ANDROID_PLUGIN).expect("first plugin should load");
    let second = load_plugin_source("@korri", ANDROID_PLUGIN).expect("second plugin should load");

    let error = PluginRegistry::new(vec![first, second], vec!["@korri:android-app".to_owned()])
        .expect_err("duplicate plugin ids must fail");

    assert!(
        error
            .to_string()
            .contains("duplicate plugin id @korri:android-app"),
        "unexpected error: {error}"
    );
}
