//! A runner's launch input names the files of the plugins its package exactly
//! requires, under `<pluginId>/<key>`, beside its own bare keys. These tests
//! drive the real path: real packages, a real route and the real
//! `launch_route` call, then the runner's own `launch.prepare`.
#[path = "fixtures/readable.rs"]
mod readable;

use korrid::{
    config::{resolver::resolve_linux_route, snapshot::ConfigSnapshotCoordinator},
    launcher::{
        linux_plugin::launch_route,
        plugin_launch::{evaluate_snapshot, PluginLaunchInput},
        LaunchError,
    },
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    script::source::SourceSnapshot,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

const RUNNER_ID: &str = "@korri:mgba/mgba";

/// The runner reports what it received, so the test proves the callback sees
/// the same keys korrid put into the launch input.
const RUNNER: &str = r#"
export const name = "mgba"
export const systems = { gba: { id: "gba", title: "Game Boy Advance", content: { extensions: ["gba"] } } }
export const runners = { mgba: { id: "@korri:mgba/mgba", program: "retroarch", core: "mgba", systems: ["gba"] } }
export const handlers = {
  "launch.prepare": input => ({
    command: input.program,
    args: [input.files["@korri:fex/fex-unpack"], input.cacheRoot],
  }),
}
"#;

struct Fixture {
    root: tempfile::TempDir,
    fex: EnabledPackage,
    proton: EnabledPackage,
    unrelated: EnabledPackage,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        readable::combined(root.path());
        fs::create_dir_all(root.path().join("roms")).unwrap();
        fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let fex = package(root.path(), "fex", &["fex-unpack", "config"]);
        let proton = package(root.path(), "proton", &["cachyos-unpack"]);
        let unrelated = package(root.path(), "unrelated", &["secret"]);
        Self {
            root,
            fex,
            proton,
            unrelated,
        }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn runner(&self, requires: Vec<PathBuf>) -> EnabledPackage {
        let package = self.path().join("plugin-mgba");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("plugin.ts"), RUNNER).unwrap();
        EnabledPackage {
            id: "@korri:mgba".into(),
            package,
            files: BTreeMap::from([
                ("retroarch".into(), self.path().join("retroarch")),
                ("mgba".into(), self.path().join("mgba.so")),
            ]),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires,
            hosts: vec![],
        }
    }

    fn launch(
        &self,
        installed: Vec<EnabledPackage>,
    ) -> Result<(PluginRegistry, PluginLaunchInput), LaunchError> {
        let snapshot = (*ConfigSnapshotCoordinator::new(self.path())
            .reload()
            .snapshot)
            .clone();
        let registry = PluginRegistry::from_installed(installed).unwrap();
        let route = resolve_linux_route(
            self.path(),
            &snapshot,
            &registry,
            readable::GBA_ID,
            Some(RUNNER_ID),
        )
        .unwrap();
        let spec = launch_route(
            self.path(),
            &snapshot,
            &registry,
            &route,
            None,
            None,
            "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
        )?;
        let input = serde_json::from_str(&spec.command[3]).unwrap();
        Ok((registry, input))
    }
}

/// A plugin with no contributions that ships only named native files.
fn package(root: &Path, name: &str, keys: &[&str]) -> EnabledPackage {
    let package = root.join(format!("plugin-{name}"));
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        format!("export const name = \"{name}\"\n"),
    )
    .unwrap();
    EnabledPackage {
        id: format!("@korri:{name}"),
        package,
        files: keys
            .iter()
            .map(|key| ((*key).to_owned(), root.join(format!("{name}-{key}"))))
            .collect(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: Vec::new(),
        hosts: vec![],
    }
}

fn text(path: PathBuf) -> String {
    path.display().to_string()
}

#[test]
fn a_runner_receives_each_required_plugins_files_under_its_plugin_id() {
    let fixture = Fixture::new();
    let runner = fixture.runner(vec![
        fixture.fex.package.clone(),
        fixture.proton.package.clone(),
    ]);
    let (registry, input) = fixture
        .launch(vec![
            runner,
            fixture.fex.clone(),
            fixture.proton.clone(),
            fixture.unrelated.clone(),
        ])
        .unwrap();
    let root = fixture.path();
    // Own files keep their bare keys; required files are qualified. The
    // installed and enabled plugin the runner does not require stays hidden.
    assert_eq!(
        input.files,
        HashMap::from([
            ("retroarch".into(), text(root.join("retroarch"))),
            ("mgba".into(), text(root.join("mgba.so"))),
            (
                "@korri:fex/fex-unpack".into(),
                text(root.join("fex-fex-unpack"))
            ),
            ("@korri:fex/config".into(), text(root.join("fex-config"))),
            (
                "@korri:proton/cachyos-unpack".into(),
                text(root.join("proton-cachyos-unpack"))
            ),
        ])
    );
    assert_eq!(input.cache_root, text(root.join("cache")));
    assert_eq!(
        input.account_root,
        text(root.join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"))
    );

    let package = registry.installed_package(RUNNER_ID).unwrap();
    let source =
        SourceSnapshot::package_plugin(&package.package, &package.entry, &package.sources).unwrap();
    let plan = evaluate_snapshot(&source, &input).unwrap();
    assert_eq!(
        plan.args,
        [text(root.join("fex-fex-unpack")), text(root.join("cache"))]
    );
    assert!(
        !root.join("cache").exists(),
        "launch planning has no effects"
    );
}

#[test]
fn a_missing_or_disabled_required_plugin_refuses_the_launch() {
    let fixture = Fixture::new();
    let runner = fixture.runner(vec![
        fixture.fex.package.clone(),
        fixture.proton.package.clone(),
    ]);
    // The host projects only enabled plugins, so a disabled one is absent
    // exactly like one that was never installed.
    let outcome = fixture.launch(vec![runner, fixture.fex.clone(), fixture.unrelated.clone()]);
    let Err(LaunchError::RouteUnavailable(message)) = outcome else {
        panic!("a missing required plugin must refuse the launch, got {outcome:?}");
    };
    assert_eq!(
        message,
        format!(
            "{RUNNER_ID} requires plugin package {}, which is not installed and enabled",
            fixture.proton.package.display()
        )
    );
    assert!(!fixture.path().join("users").exists());
    assert!(!fixture.path().join("cache").exists());
}

#[test]
fn a_runner_without_requirements_sees_only_its_own_files() {
    let fixture = Fixture::new();
    let runner = fixture.runner(Vec::new());
    let (_, input) = fixture
        .launch(vec![runner, fixture.fex.clone(), fixture.unrelated.clone()])
        .unwrap();
    let mut keys: Vec<_> = input.files.keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["mgba", "retroarch"]);
    assert_eq!(input.cache_root, text(fixture.path().join("cache")));
}
