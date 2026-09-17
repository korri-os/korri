//! The DIY proof: a hand-written runner that borrows nothing.
//!
//! The catalogue's cores share a family, a generated plugin.ts and a launch
//! helper. If any of that were load-bearing, a plugin written by hand would not
//! work. PPSSPP is written by hand, joins no family, imports no helper, and is
//! built by the same builder as a generated core.

use korrid::{
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    script::{call_plugin_operation_ts, LAUNCH_PREPARE},
};
use std::{collections::BTreeMap, fs, path::Path};

const PPSSPP_PLUGIN: &str = include_str!("../../../plugins/ppsspp/plugin.ts");

fn installed(root: &Path) -> PluginRegistry {
    let package = root.join("plugin-ppsspp");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), PPSSPP_PLUGIN).unwrap();
    PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@korri:ppsspp".into(),
        package,
        // One file key, named by the runner's own program field. No frontend
        // package, no core, nothing from another plugin.
        files: BTreeMap::from([("ppsspp".into(), root.join("ppsspp"))]),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
    }])
    .unwrap()
}

#[test]
fn a_hand_written_runner_needs_no_family_and_no_helper() {
    let root = tempfile::tempdir().unwrap();
    let registry = installed(root.path());

    let runner = &registry.runners()["@korri:ppsspp/ppsspp"];
    assert_eq!(runner.family, None);
    assert_eq!(runner.core, None);
    assert_eq!(runner.program.as_deref(), Some("ppsspp"));
    registry.native_runner("@korri:ppsspp/ppsspp").unwrap();

    // No RetroArch family record is registered, and nothing asked for one.
    assert!(registry.families().is_empty());

    let claim = &registry.file_release_discovery_claims_for_extension(".ISO")[0];
    assert_eq!(claim.runners, ["@korri:ppsspp/ppsspp"]);

    // Honest about what it does not get: effects are a closed first-party
    // vocabulary, so a DIY runner declares no session controls at all.
    assert!(registry.session_controls().is_empty());
}

#[test]
fn the_hand_written_runner_prepares_its_own_launch() {
    let input = serde_json::json!({
        "runnerId": "@korri:ppsspp/ppsspp",
        "program": "/nix/store/exact-ppsspp/bin/ppsspp",
        "contentPath": "/storage/games/psp/wipeout.iso",
        "accountRoot": "/storage/korri/accounts/simon",
        "files": { "ppsspp": "/nix/store/exact-ppsspp/bin/ppsspp" },
    })
    .to_string();
    let plan: serde_json::Value = serde_json::from_str(
        &call_plugin_operation_ts(PPSSPP_PLUGIN, LAUNCH_PREPARE, &input).unwrap(),
    )
    .unwrap();

    assert_eq!(plan["command"], "/nix/store/exact-ppsspp/bin/ppsspp");
    assert_eq!(
        plan["args"],
        serde_json::json!(["--fullscreen", "/storage/games/psp/wipeout.iso"])
    );
    // The account root is korrid's fact; the layout inside it is the runner's.
    assert_eq!(
        plan["directories"],
        serde_json::json!(["/storage/korri/accounts/simon/ppsspp"])
    );
    assert_eq!(
        plan["env"]["XDG_CONFIG_HOME"],
        "/storage/korri/accounts/simon/ppsspp"
    );
}

#[test]
fn the_hand_written_runner_refuses_what_it_has_not_implemented() {
    let base = serde_json::json!({
        "runnerId": "@korri:ppsspp/ppsspp",
        "program": "/nix/store/exact-ppsspp/bin/ppsspp",
        "contentPath": "/storage/games/psp/wipeout.iso",
        "accountRoot": "/storage/korri/accounts/simon",
        "files": { "ppsspp": "/nix/store/exact-ppsspp/bin/ppsspp" },
    });

    // A core path means the caller mistook this for a libretro frontend.
    let mut with_core = base.clone();
    with_core["corePath"] = serde_json::json!("/nix/store/exact/mgba_libretro.so");
    assert!(
        call_plugin_operation_ts(PPSSPP_PLUGIN, LAUNCH_PREPARE, &with_core.to_string()).is_err()
    );

    // Authored settings this runner cannot apply are refused, not dropped.
    let mut with_settings = base.clone();
    with_settings["overrides"] = serde_json::json!({ "settings": { "video_vsync": true } });
    assert!(
        call_plugin_operation_ts(PPSSPP_PLUGIN, LAUNCH_PREPARE, &with_settings.to_string())
            .is_err()
    );

    // Raw RetroArch-style config has no meaning here either.
    let mut with_config = base;
    with_config["overrides"] = serde_json::json!({ "config": { "append": "video_vsync = true" } });
    assert!(
        call_plugin_operation_ts(PPSSPP_PLUGIN, LAUNCH_PREPARE, &with_config.to_string()).is_err()
    );
}
