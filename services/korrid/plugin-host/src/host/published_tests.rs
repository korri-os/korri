//! Real repository bytes and configured first-party key. No invented release
//! record, ephemeral publisher, or trust bypass participates in this proof.
use super::*;

#[test]
#[ignore = "downloads the pinned actual release; combined product/catalog gate runs it"]
fn published_catalog_install_and_same_release_update() {
    let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("native Nix"));
    let curl = PathBuf::from(std::env::var_os("KORRI_IMAGE_CURL").expect("native curl"));
    let catalog = crate::catalog::Catalog::from_json(
        &fs::read(std::env::var_os("KORRI_IMAGE_CATALOG").expect("actual pinned catalog")).unwrap(),
    )
    .unwrap();
    let source =
        SourceUrl::parse(&std::env::var("KORRI_IMAGE_SOURCE").expect("immutable catalog URL"))
            .unwrap();
    let bindings = package::publisher_bindings(Path::new(
        &std::env::var("KORRI_IMAGE_BINDINGS").expect("product publisher bindings"),
    ))
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut host = Host::for_test(
        root.path(),
        nix,
        LifecyclePolicy::new(Vec::new()).unwrap(),
        TestRuntime::default(),
    )
    .unwrap();
    // Use the production checks and file storage, not TestRuntime's authority
    // shortcut. New selections stay Disabled, so no native effect is requested.
    host.test_runtime = None;
    // Disabled installation still removes old firewall rules. Represent the
    // initially empty tables with a real, configured no-output subprocess;
    // never query or change the build machine's firewall. Unit control stays
    // unavailable, so this test cannot accidentally activate a native service.
    let no_rules =
        PathBuf::from(std::env::var_os("KORRI_TEST_TRUE").expect("immutable empty-table command"));
    host.units.firewall.ipv4 = no_rules.clone();
    host.units.firewall.ipv6 = no_rules;
    host.publishers = bindings;
    let config = Configuration {
        curl,
        official: Some(source.clone()),
        ca_bundle: None,
    };
    let fetched = host.catalog(&config, &source).unwrap();
    assert_eq!(catalog.to_json().unwrap(), fetched.to_json().unwrap());
    for id in [
        "@korri:homebrewhub",
        "@korri:mgba",
        "@korri:ssh",
        "@korri:starter-pack",
    ] {
        let record = catalog
            .record(id, &catalog.latest[id], current_platform())
            .unwrap();
        let review = host
            .inspect_repository(&config, &source, id, &record.release_version)
            .unwrap();
        let approval = host.review_approval(&review).unwrap();
        assert!(host
            .install(review.clone(), &"0".repeat(64), SelectionIntent::Install)
            .is_err());
        assert!(host.receipt(id).unwrap().is_none());
        host.install(review, &approval, SelectionIntent::Install)
            .unwrap();
        let installed = host.receipt(id).unwrap().unwrap();
        assert_eq!(installed.package, Path::new(&record.store_path));
        assert!(matches!(installed.desired, Desired::Disabled));
        assert!(
            matches!(&installed.provenance, Provenance::Repository { source_url, archive_sha256, signed: true, .. }
            if source_url == source.as_str() && archive_sha256 == &record.archive_sha256)
        );
        let update = host
            .inspect_repository(&config, &source, id, &record.release_version)
            .unwrap();
        let approval = host.review_approval(&update).unwrap();
        host.install(update, &approval, SelectionIntent::Update { id })
            .unwrap();
        assert_eq!(host.receipt(id).unwrap().unwrap(), installed);
        host.release_download().unwrap();
    }
    // This is a same-release update, NOT a changed-output upgrade. There is no
    // published direction-compatible predecessor; do not fabricate one here.
}
