//! Real builder outputs and registered Nix closures. These tests write only the
//! build-machine store. They never change device trust or launch an emulator.
use korri_plugin_host::{package, provenance::Provenance};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn fixture(name: &str) -> PathBuf {
    fs::canonicalize(
        PathBuf::from(env::var_os("KORRI_TEST_DEPENDENCY_FIXTURES").expect("built builder check"))
            .join(name),
    )
    .unwrap()
}
fn nix() -> PathBuf {
    env::var_os("KORRI_PUBLISH_NIX")
        .expect("immutable Nix executable")
        .into()
}
fn origin(cache: &str) -> Provenance {
    Provenance::RawCache {
        cache_url: cache.into(),
    }
}
fn load(name: &str) -> Result<package::Report, String> {
    package::load_graph(
        &nix(),
        None,
        &fixture(name),
        origin("file:///games-cache"),
        |path| {
            Ok(origin(if package::manifest_namespace(path)? == "@games" {
                "file:///games-cache"
            } else {
                "file:///runtime-cache"
            }))
        },
    )
}

#[test]
#[ignore = "requires builder dependency fixtures and real build-machine Nix"]
fn pack_requires_an_independent_runner_with_its_own_sources_files_and_approval() {
    let runner = fixture("runner");
    let inspected = load("pack").unwrap();
    assert_eq!(inspected.id, "@games:starter-pack");
    assert_eq!(inspected.requires.as_slice(), std::slice::from_ref(&runner));
    assert!(inspected.files.is_empty());
    assert!(inspected.packages.is_empty());
    let dependency = &inspected.brings[0].report;
    assert_eq!(dependency.id, "@runtime:fake08");
    assert_eq!(dependency.package, runner);
    assert_eq!(dependency.sources, ["launch.ts", "plugin.ts"]);
    assert_eq!(dependency.entry, "plugin.ts");
    assert!(dependency.files["fake08"].is_file());
    assert_eq!(dependency.provenance, origin("file:///runtime-cache"));
    assert_eq!(
        dependency.approval,
        package::load(&nix(), &runner, dependency.provenance.clone())
            .unwrap()
            .approval
    );
    let changed_cache = package::load_graph(
        &nix(),
        None,
        &fixture("pack"),
        origin("file:///games-cache"),
        |_| Ok(origin("file:///another-runtime-cache")),
    )
    .unwrap();
    assert_ne!(changed_cache.approval, inspected.approval);
    let transitive = load("transitive").unwrap();
    assert_eq!(
        transitive
            .brings
            .iter()
            .map(|dep| dep.report.id.as_str())
            .collect::<Vec<_>>(),
        ["@runtime:fake08", "@games:starter-pack"]
    );
    assert_eq!(transitive.brings[1].report.approval, inspected.approval);
    assert_eq!(transitive.brings[1].report.provenance, inspected.provenance);
    assert!(transitive
        .brings
        .iter()
        .all(|dep| dep.report.brings.is_empty()));
    assert_ne!(changed_cache.brings[0].report.approval, dependency.approval);
    assert!(
        package::load_for_seed(&fixture("pack"), origin("file:///games-cache"))
            .err()
            .unwrap()
            .contains("independent publisher")
    );
}

#[test]
#[ignore = "requires builder dependency fixtures and real build-machine Nix"]
fn every_dependency_discloses_current_multiunit_native_authority() {
    let inspected = load("nativePack").unwrap();
    let dependency = &inspected.brings[0].report;
    assert_eq!(dependency.native_units.len(), 2);
    assert!(dependency.warning.contains("CAP_NET_RAW"));
    assert!(dependency.warning.contains("DEVICE-WIDE ROOT AUTHORITY"));
    assert_eq!(dependency.policy, package::ROOT_POLICY);
    assert_eq!(
        dependency.approval,
        package::load(&nix(), &fixture("native"), dependency.provenance.clone())
            .unwrap()
            .approval
    );
    assert!(inspected.native_units.is_empty());
    assert!(inspected.brings[0]
        .report
        .unit_configuration
        .contains("User=root"));
}

#[test]
#[ignore = "requires builder dependency fixtures and real build-machine Nix"]
fn cycles_conflicting_versions_outside_closure_and_borrowed_repository_identity_are_refused() {
    assert!(load("conflict")
        .err()
        .unwrap()
        .contains("conflicting exact versions"));
    assert!(load("cycle").err().unwrap().contains("cycle"));
    let parent = Provenance::Repository {
        source_url: "https://games.example/catalog.json".into(),
        plugin_id: "@games:starter-pack".into(),
        release_version: "1".into(),
        platform: korri_plugin_host::provenance::current_platform().into(),
        archive_sha256: "a".repeat(64),
    };
    assert!(
        package::load_graph(&nix(), None, &fixture("pack"), parent.clone(), |_| Ok(
            parent.clone()
        ))
        .is_err()
    );
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("plugin.ts"),
        "export const name = 'outside';",
    )
    .unwrap();
    fs::write(root.path().join("manifest.json"), serde_json::to_vec(&serde_json::json!({
        "publisher": {"namespace":"@games"}, "entry":"plugin.ts", "sources":["plugin.ts"], "requires":[fixture("runner")],
    })).unwrap()).unwrap();
    // add-path has no registered references, unlike the builder's runCommand.
    let output = Command::new(nix())
        .args([
            "--extra-experimental-features",
            "nix-command",
            "store",
            "add-path",
        ])
        .arg(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = String::from_utf8(output.stdout).unwrap();
    let error = package::load_graph(
        &nix(),
        None,
        Path::new(path.trim()),
        origin("file:///games-cache"),
        |_| Ok(origin("file:///runtime-cache")),
    )
    .err()
    .unwrap();
    assert!(error.contains("outside the immutable closure"), "{error}");
}
