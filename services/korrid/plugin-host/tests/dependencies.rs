//! Real builder outputs and registered Nix closures. These tests write only the
//! build-machine store. They never change device trust or launch an emulator.
use korri_plugin_host::{package, provenance::Provenance};
use std::{env, fs, path::PathBuf, process::Command};

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
    // Image digests grant no closure authority. Runtime must still refuse an
    // exact selected dependency that is present but not a registered reference.
    let selected = [PathBuf::from(path.trim()), fixture("runner")];
    let seeded = package::load_graph_for_seed(&selected, &bindings()).unwrap();
    let outside = seeded
        .iter()
        .find(|report| report.package == selected[0])
        .unwrap();
    let error = package::load_graph(
        &nix(),
        None,
        &outside.package,
        outside.provenance.clone(),
        |path| {
            Ok(seeded
                .iter()
                .find(|report| report.package == path)
                .unwrap()
                .provenance
                .clone())
        },
    )
    .err()
    .unwrap();
    assert!(error.contains("outside the immutable closure"), "{error}");
}

fn bindings() -> package::PublisherBindings {
    [
        ("@games", "file:///games-cache"),
        ("@runtime", "file:///runtime-cache"),
    ]
    .into_iter()
    .map(|(namespace, cache)| {
        (
            namespace.into(),
            package::PublisherBinding {
                public_key: "seed-fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
                cache_url: cache.into(),
            },
        )
    })
    .collect()
}

#[test]
#[ignore = "requires builder dependency fixtures and real build-machine Nix"]
fn image_graph_receipts_equal_runtime_graphs_with_transitive_and_cross_publisher_dependencies() {
    use korri_plugin_host::selection::{Desired, Receipt};
    // Deliberately supply parents first. Seeding must return dependency order.
    let selected = [fixture("transitive"), fixture("runner"), fixture("pack")];
    let directory = tempfile::tempdir().unwrap();
    let publishers = directory.path().join("publishers.json");
    fs::write(&publishers, serde_json::to_vec(&bindings()).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_korri-plugin"))
        .arg("seed-graph")
        .arg(publishers)
        .args(&selected)
        // No host tools, state or administrator access are needed for seeding.
        .env_remove("KORRI_PLUGIN_NIX")
        .env_remove("KORRI_PLUGIN_SYSTEMCTL")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipts: Vec<Receipt> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        receipts.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        [
            "@runtime:fake08",
            "@games:starter-pack",
            "@games:collection"
        ]
    );
    for receipt in &receipts {
        let runtime = package::load_graph(
            &nix(),
            None,
            &receipt.package,
            receipt.provenance.clone(),
            |path| {
                Ok(receipts
                    .iter()
                    .find(|r| r.package == path)
                    .unwrap()
                    .provenance
                    .clone())
            },
        )
        .unwrap();
        assert_eq!(receipt.approval, runtime.approval);
        assert_eq!(receipt.id, runtime.id);
        assert_eq!(receipt.desired, Desired::Enabled);
        assert!(receipt.previous.is_none());
        assert_eq!(
            receipt.provenance,
            origin(if receipt.id.starts_with("@runtime:") {
                "file:///runtime-cache"
            } else {
                "file:///games-cache"
            })
        );
        for dependency in &runtime.brings {
            let seeded = receipts
                .iter()
                .find(|r| r.package == dependency.report.package)
                .unwrap();
            assert_eq!(seeded.approval, dependency.report.approval);
            assert_eq!(seeded.provenance, dependency.report.provenance);
        }
    }
    let seeded = package::load_graph_for_seed(&selected, &bindings()).unwrap();
    assert_eq!(
        seeded.iter().map(|r| &r.approval).collect::<Vec<_>>(),
        receipts.iter().map(|r| &r.approval).collect::<Vec<_>>()
    );
    let leaf = package::load_for_seed(&fixture("runner"), origin("file:///runtime-cache")).unwrap();
    assert_eq!(leaf.approval, receipts[0].approval);
    assert!(package::load_for_seed(&fixture("transitive"), origin("file:///games-cache")).is_err());

    // Provenance is digest-bound at every ancestor, not just the leaf.
    let mut changed_bindings = bindings();
    changed_bindings.get_mut("@runtime").unwrap().cache_url = "file:///another-cache".into();
    let changed = package::load_graph_for_seed(&selected, &changed_bindings).unwrap();
    for (before, after) in seeded.iter().zip(changed) {
        assert_ne!(before.approval, after.approval);
    }
}

#[test]
#[ignore = "requires builder dependency fixtures and real build-machine Nix"]
fn image_graph_refuses_unknown_missing_wrong_exact_cycle_conflict_and_unbound_cache() {
    let error = |names: &[&str], publishers: &package::PublisherBindings| {
        package::load_graph_for_seed(
            &names.iter().map(|name| fixture(name)).collect::<Vec<_>>(),
            publishers,
        )
        .err()
        .expect("must refuse invalid image graph")
    };
    let publishers = bindings();
    assert!(error(&["pack"], &publishers).contains("missing exact selected dependency"));
    assert!(
        error(&["pack", "secondRunner"], &publishers).contains("missing exact selected dependency")
    );
    assert!(error(&["pack", "runner", "secondRunner"], &publishers)
        .contains("conflicting exact versions"));
    assert!(error(&["conflict", "runner", "secondRunner"], &publishers)
        .contains("conflicting exact versions"));
    assert!(error(&["cycle"], &publishers).contains("cycle"));
    assert!(error(&["runner", "runner"], &publishers).contains("duplicate selected"));
    let mut unknown = publishers.clone();
    unknown.remove("@runtime");
    assert!(error(&["pack", "runner"], &unknown).contains("publisher @runtime is not bound"));
    let mut invalid = publishers.clone();
    invalid.get_mut("@runtime").unwrap().cache_url = "nixpkgs#fake08".into();
    assert!(error(&["pack", "runner"], &invalid).contains("binary cache"));
    let mut invalid_key = publishers.clone();
    invalid_key.get_mut("@runtime").unwrap().public_key = "label-only".into();
    assert!(error(&["pack", "runner"], &invalid_key).contains("full Nix public key"));
    // Runtime authority refuses a receipt claiming the parent's cache, even
    // though the image contains the exact dependency output.
    assert!(package::verify_publisher(
        &nix(),
        &fixture("runner"),
        Some("file:///games-cache"),
        &publishers,
        package::StoreContents::TrustRegistered
    )
    .unwrap_err()
    .contains("bound to cache file:///runtime-cache"));
    assert!(package::verify_publisher(
        &nix(),
        &fixture("runner"),
        Some("file:///runtime-cache"),
        &unknown,
        package::StoreContents::TrustRegistered
    )
    .unwrap_err()
    .contains("not bound"));
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("publishers.json");
    fs::write(&file, serde_json::to_vec(&publishers).unwrap()).unwrap();
    for names in [
        vec!["pack"],
        vec!["pack", "secondRunner"],
        vec!["cycle"],
        vec!["conflict", "runner", "secondRunner"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_korri-plugin"))
            .arg("seed-graph")
            .arg(&file)
            .args(names.into_iter().map(fixture))
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            output.stdout.is_empty(),
            "no partial receipt array on refusal"
        );
    }
}
