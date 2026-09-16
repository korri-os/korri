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
    }
}

const CALLBACK: &str =
    "export const handlers = {'launch.prepare': function () { throw new Error('admission must not call launch'); }}";

#[test]
fn native_runner_owns_program_and_core_in_one_package() {
    let root = tempfile::tempdir().unwrap();
    let source = "export const runners = {main: {id:'@test:mgba/main', family:'@korri:retroarch', program:'program', core:'core', systems:['gba']}};";
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
fn native_runners_require_a_callable_launch_prepare_handler_without_invocation() {
    let declaration = "export const runners = {main: {id:'@test:kind/main', program:'program'}};";
    for callback in [
        "",
        "export const handlers = {};",
        "export const handlers = {'launch.prepare': 42};",
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
        if callback == CALLBACK {
            assert!(result.is_ok(), "callback must not run: {result:?}");
        } else {
            assert!(result.unwrap_err().to_string().contains("launch"));
        }
    }
}

#[test]
fn android_runners_do_not_require_a_native_launch_callback() {
    let root = tempfile::tempdir().unwrap();
    let android = package(
        root.path(),
        "android",
        "export const runners = {main: {id:'@test:android/main', command:'retroarch', systems:['gba'], android:{packageName:'com.korri.retroarch', className:'com.retroarch.browser.retroactivity.RetroActivityFuture'}}};",
        &[],
    );
    PluginRegistry::from_installed(vec![android]).unwrap();
}
