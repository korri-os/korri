use korrid::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
use std::{collections::BTreeMap, fs, path::Path};

fn package(root: &Path, name: &str, exports: &str) -> EnabledPackage {
    let package = root.join(name);
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        format!("export const name = '{name}'; {exports}"),
    )
    .unwrap();
    EnabledPackage {
        id: format!("@test:{name}"),
        files: BTreeMap::from([("program".into(), package.join("program"))]),
        package,
        requires: vec![],
    }
}

const KIND: &str = "export const launchers = {main: {id:'@test:kind/main', kind:'@test:kind/main', program:'program'}};";
const CALLBACK: &str =
    "export function launch() { throw new Error('admission must not call launch'); }";

#[test]
fn native_kind_references_require_the_referenced_packages_exact_selection() {
    check_exact_selection("export const launchers = {main: {id:'@test:consumer/main', kind:'@test:kind/main', program:'program'}};");
}

#[test]
fn native_runtime_references_require_the_referenced_packages_exact_selection() {
    check_exact_selection("export const runtimes = {main: {id:'@test:consumer/main', kind:'emulator', launcher:'@test:kind/main', path:'program'}};");
}

fn check_exact_selection(exports: &str) {
    for pin in [None, Some("unrelated"), Some("old-kind"), Some("kind")] {
        let root = tempfile::tempdir().unwrap();
        let kind = package(root.path(), "kind", &format!("{KIND} {CALLBACK}"));
        let unrelated = package(root.path(), "unrelated", "");
        let mut consumer = package(root.path(), "consumer", exports);
        if let Some(pin) = pin {
            consumer.requires.push(root.path().join(pin));
        }
        let result = PluginRegistry::from_installed(vec![kind, unrelated, consumer]);
        if pin == Some("kind") {
            assert!(result.is_ok(), "exact dependency: {result:?}");
        } else {
            let error = result.expect_err("unbound native reference must not be admitted");
            assert!(error.to_string().contains("@test:consumer"), "{error}");
        }
    }
}

#[test]
fn native_self_kinds_require_a_callable_top_level_launch_without_invocation() {
    for callback in ["", "export const launch = 42;", CALLBACK] {
        let root = tempfile::tempdir().unwrap();
        let kind = package(root.path(), "kind", &format!("{KIND} {callback}"));
        let result = PluginRegistry::from_installed(vec![kind]);
        if callback == CALLBACK {
            assert!(result.is_ok(), "callback must not run: {result:?}");
        } else {
            let error = result.expect_err("a native kind must export a callable launch");
            assert!(error.to_string().contains("launch"), "{error}");
        }
    }
}

#[test]
fn android_launchers_do_not_require_a_native_launch_callback() {
    let root = tempfile::tempdir().unwrap();
    let android = package(
        root.path(),
        "android",
        "export const launchers = {main: {id:'@test:android/main', command:'retroarch', android:{packageName:'com.korri.retroarch', className:'com.retroarch.browser.retroactivity.RetroActivityFuture'}}};",
    );
    PluginRegistry::from_installed(vec![android]).unwrap();
}
