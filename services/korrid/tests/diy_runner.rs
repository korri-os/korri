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
        requires: Vec::new(),
        hosts: vec![],
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

    assert_eq!(
        registry.content_rules("psp").unwrap().extensions,
        ["iso", "cso", "pbp"]
    );

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
        "artifacts": {},
        "cacheRoot": "/storage/korri/cache",
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
        "artifacts": {},
        "cacheRoot": "/storage/korri/cache",
        "files": { "ppsspp": "/nix/store/exact-ppsspp/bin/ppsspp" },
    });

    // Provider-backed content is not a PSP image.
    let mut without_content = base.clone();
    without_content
        .as_object_mut()
        .unwrap()
        .remove("contentPath");
    without_content["ref"] = serde_json::json!("401710");
    assert!(
        call_plugin_operation_ts(PPSSPP_PLUGIN, LAUNCH_PREPARE, &without_content.to_string())
            .is_err()
    );

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

const HELLO_PLUGIN: &str = include_str!("fixtures/hello/plugin.ts");

fn hello_input() -> korrid::launcher::plugin_launch::PluginLaunchInput {
    serde_json::from_value(serde_json::json!({
        "runnerId": "@author:hello/hello",
        "program": "/nix/store/exact-hello/bin/hello",
        "contentPath": "/games/world.hello",
        "accountRoot": "/accounts/person",
        "saveRoot": "/accounts/person/saves/hello",
        "cacheRoot": "/cache",
        "files": {},
        "artifacts": {},
    }))
    .unwrap()
}

#[test]
fn hello_needs_no_authored_ids_or_launch_handler() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("plugin.ts"), HELLO_PLUGIN).unwrap();
    fs::write(
        root.path().join("manifest.json"),
        r#"{"entry":"plugin.ts","sources":["plugin.ts"]}"#,
    )
    .unwrap();
    let registry = PluginRegistry::from_installed(vec![EnabledPackage {
        id: "@author:hello".into(),
        package: root.path().to_owned(),
        files: BTreeMap::from([("hello".into(), root.path().join("hello"))]),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: vec![],
        hosts: vec![],
    }])
    .unwrap();
    assert_eq!(
        registry.runners()["@author:hello/hello"].id,
        "@author:hello/hello"
    );
    assert_eq!(registry.systems()["@author:hello/hello"].id, "hello");
    assert_eq!(
        registry.content_rules("hello").unwrap().extensions,
        ["hello"]
    );

    // Exercise the same preparation command and private handoff as a real launch.
    let command = vec![
        "korrid".into(),
        "plugin-prepare".into(),
        root.path().join("plugin.ts").display().to_string(),
        serde_json::to_string(&hello_input()).unwrap(),
        r#"{"modifiers":[]}"#.into(),
        "test-release".into(),
    ];
    let prepared = korrid::launcher::plugin_launch::prepare_command(&command).unwrap();
    assert_eq!(prepared[1], "plugin-launch");
    let plan = korrid::launcher::plugin_launch::prepared_plan(&prepared).unwrap();
    assert_eq!(plan.command, "/nix/store/exact-hello/bin/hello");
    assert_eq!(plan.args, ["/games/world.hello"]);
    assert!(plan.env.is_empty());
    assert!(plan.env_unset.is_empty());
    assert!(plan.cwd.is_none());
    assert!(plan.directories.is_empty());
    assert!(plan.files.is_empty());
    assert!(plan.save_locations.is_empty());
}

#[test]
fn hello_default_plan_runs_through_the_real_plugin_run_cli() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("plugin.ts"), HELLO_PLUGIN).unwrap();
    fs::write(
        root.path().join("manifest.json"),
        r#"{"entry":"plugin.ts","sources":["plugin.ts"]}"#,
    )
    .unwrap();
    let program = root.path().join("hello");
    fs::write(&program, "#!/bin/sh\nprintf '%s\\n' \"$#:$1\"\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let mut input = hello_input();
    input.program = program.display().to_string();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_korrid"))
        .env_clear()
        .arg("plugin-run")
        .arg(root.path().join("plugin.ts"))
        .arg(serde_json::to_string(&input).unwrap())
        .arg(r#"{"modifiers":[]}"#)
        .arg("hello-test-release")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        "1:/games/world.hello\n"
    );
}

#[test]
fn hello_refuses_provider_references_settings_and_raw_config() {
    use korrid::launcher::plugin_launch::evaluate;
    let mut input = hello_input();
    input.content_path = None;
    input.provider_ref = Some("world".into());
    assert_eq!(evaluate(HELLO_PLUGIN, &input).unwrap_err(),
        "runner @author:hello/hello has no launch.prepare handler, so it cannot start provider references");

    // Even settings a settings.validate handler would discard must fail.
    for settings in [
        serde_json::json!({"speed": 2}),
        serde_json::json!({"speed": null}),
    ] {
        let mut input = hello_input();
        input.overrides =
            Some(serde_json::from_value(serde_json::json!({"settings": settings})).unwrap());
        assert_eq!(
            evaluate(HELLO_PLUGIN, &input).unwrap_err(),
            "runner @author:hello/hello has no launch.prepare handler, so it takes no settings"
        );
    }
    for config in [
        serde_json::json!({}),
        serde_json::json!({"append": "speed=2"}),
    ] {
        let mut input = hello_input();
        input.overrides =
            Some(serde_json::from_value(serde_json::json!({"config": config})).unwrap());
        assert_eq!(evaluate(HELLO_PLUGIN, &input).unwrap_err(),
            "runner @author:hello/hello has no launch.prepare handler, so it takes no config override");
    }
    let mut input = hello_input();
    input.provider_ref = Some("world".into());
    input.overrides = Some(Default::default());
    assert!(evaluate(HELLO_PLUGIN, &input).is_ok());
}

#[test]
fn an_authored_launch_handler_keeps_its_plan_and_failures() {
    use korrid::launcher::plugin_launch::evaluate;
    let source = format!("{HELLO_PLUGIN} export const handlers={{'launch.prepare':input=>({{command:input.program,args:['custom',input.contentPath],env:{{CUSTOM:'yes'}},cwd:'/custom'}})}};");
    let plan = evaluate(&source, &hello_input()).unwrap();
    assert_eq!(plan.args, ["custom", "/games/world.hello"]);
    assert_eq!(plan.env["CUSTOM"], "yes");
    assert_eq!(plan.cwd.as_deref(), Some("/custom"));
    let source = format!("{HELLO_PLUGIN} export const handlers={{'launch.prepare':()=>{{throw Error('authored failure')}}}};");
    assert!(evaluate(&source, &hello_input())
        .unwrap_err()
        .contains("authored failure"));
}

#[test]
fn authored_runner_and_system_ids_fail_both_source_and_json_admission() {
    use korrid::plugin::{decode_plugin_declaration, load_plugin_source};
    for (kind, key) in [("runner", "hello"), ("system", "hello")] {
        for id in [
            serde_json::json!("hello"),
            serde_json::json!("@author:hello/hello"),
            serde_json::json!(null),
            serde_json::json!(42),
        ] {
            let records = format!("{kind}s");
            let mut declaration = serde_json::json!({"name": "hello"});
            declaration[&records] = serde_json::json!({key: {"id": id}});
            let expected = format!("{kind} {key}: id is derived; remove it");
            assert!(
                decode_plugin_declaration("@author", &declaration.to_string())
                    .unwrap_err()
                    .to_string()
                    .contains(&expected)
            );
            let source = format!(
                "export const name='hello'; export const {records}={};",
                declaration[&records]
            );
            assert!(load_plugin_source("@author", &source)
                .unwrap_err()
                .to_string()
                .contains(&expected));
            // plugin-host also uses this evaluator before its declaration checks.
            assert!(korrid::script::eval_plugin_ts(&source)
                .unwrap_err()
                .contains(&expected));
        }
    }
}
