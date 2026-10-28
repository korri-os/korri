use korrid::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
use std::{collections::BTreeMap, fs, path::Path};

fn package(root: &Path, name: &str, exports: &str, files: &[&str]) -> EnabledPackage {
    let package = root.join(name);
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        format!("export const name = '{name}'; {exports}"),
    )
    .unwrap();
    EnabledPackage {
        id: format!("@test:{name}"),
        files: files
            .iter()
            .map(|key| ((*key).into(), package.join(key)))
            .collect::<BTreeMap<_, _>>(),
        package,
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: Vec::new(),
        hosts: vec![],
    }
}

const CALLBACK: &str =
    "export const handlers = {'launch.prepare': function () { throw new Error('admission must not call launch'); }}";
const RENDER_CALLBACK: &str =
    "export const handlers = {'settings.render': function () { throw new Error('admission must not call render'); }}";

#[test]
fn native_runner_owns_program_and_core_in_one_package() {
    let root = tempfile::tempdir().unwrap();
    let source = "export const runners = {main: {family:'@korri:retroarch', program:'program', core:'core', systems:['gba']}};";
    let complete = package(
        root.path(),
        "mgba",
        &format!("{source} {CALLBACK}"),
        &["program", "core"],
    );
    PluginRegistry::from_installed(vec![complete]).unwrap();
    let missing = package(
        root.path(),
        "mgba",
        &format!("{source} {CALLBACK}"),
        &["program"],
    );
    assert!(PluginRegistry::from_installed(vec![missing])
        .unwrap_err()
        .to_string()
        .contains("core"));
}

#[test]
fn native_runners_allow_no_launch_handler_and_never_invoke_an_authored_handler() {
    for selector in [
        "systems:['gba']",
        "releases:['sha256:0000000000000000000000000000000000000000000000000000000000000000']",
    ] {
        let declaration =
            format!("export const runners = {{main: {{program:'program', {selector}}}}};");
        for callback in [
            "",
            "export const handlers = {};",
            "export const handlers = {'launch.prepare': 42};",
            RENDER_CALLBACK,
            CALLBACK,
        ] {
            let root = tempfile::tempdir().unwrap();
            let runner = package(
                root.path(),
                "kind",
                &format!("{declaration} {callback}"),
                &["program"],
            );
            let result = PluginRegistry::from_installed(vec![runner]);
            if callback != "export const handlers = {'launch.prepare': 42};" {
                assert!(result.is_ok(), "callback must not run: {result:?}");
            } else {
                assert!(result.unwrap_err().to_string().contains("launch"));
            }
        }
    }
}

#[test]
fn command_runners_do_not_require_a_native_launch_callback() {
    let root = tempfile::tempdir().unwrap();
    let command = package(
        root.path(),
        "command",
        "export const runners = {main: {command:'retroarch', systems:['gba']}};",
        &[],
    );
    PluginRegistry::from_installed(vec![command]).unwrap();
}

#[test]
fn settings_only_native_runners_require_callable_render_without_invocation() {
    for selectors in ["", ", systems:[]"] {
        let declaration =
            format!("export const runners = {{main: {{program:'program'{selectors}}}}};");
        for callback in [
            "",
            "export const handlers = {};",
            "export const handlers = {'settings.render': 42};",
            "export const handlers = {'settings.render': {}};",
            CALLBACK,
            RENDER_CALLBACK,
        ] {
            let root = tempfile::tempdir().unwrap();
            let runner = package(
                root.path(),
                "viewer",
                &format!("{declaration} {callback}"),
                &["program"],
            );
            let result = PluginRegistry::from_installed(vec![runner]);
            if callback == RENDER_CALLBACK {
                assert!(result.is_ok(), "render callback must not run: {result:?}");
            } else {
                assert!(result.unwrap_err().to_string().contains("settings.render"));
            }
        }
    }
}

#[test]
fn a_package_with_local_and_settings_only_runners_requires_only_render() {
    let declaration="export const runners = {local:{program:'program',systems:['gba']},viewer:{program:'program'}};";
    let both="export const handlers = {'launch.prepare':()=>{throw Error('launch must not run')},'settings.render':()=>{throw Error('render must not run')}};";
    for (callback, missing) in [
        (CALLBACK, Some("settings.render")),
        (RENDER_CALLBACK, None),
        (both, None),
    ] {
        let root = tempfile::tempdir().unwrap();
        let runner = package(
            root.path(),
            "mixed",
            &format!("{declaration} {callback}"),
            &["program"],
        );
        let result = PluginRegistry::from_installed(vec![runner]);
        if let Some(missing) = missing {
            assert!(result.unwrap_err().to_string().contains(missing));
        } else {
            assert!(
                result.is_ok(),
                "neither callback may run during admission: {result:?}"
            );
        }
    }
}
