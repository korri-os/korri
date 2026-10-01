//! Boot restore trusts the store contents the image or install already
//! verified (owner decision 2026-09-30); install and enable still rehash.
//!
//! A stand-in `nix` records each invocation and fails every one, so the test
//! sees which checks run first without touching a real store.
use korri_plugin_host::package::{verify_publisher, PublisherBinding, StoreContents};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path};

const PACKAGE: &str = "/nix/store/0123456789abcdfghijklmnpqrsvwxyz-korri-plugin";

fn invocations(contents: StoreContents) -> (String, String) {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("calls");
    let nix = directory.path().join("nix");
    fs::write(
        &nix,
        format!(
            "#!/bin/sh\necho \"$*\" >> {}\necho refused >&2\nexit 1\n",
            log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&nix, fs::Permissions::from_mode(0o755)).unwrap();
    let bindings: BTreeMap<String, PublisherBinding> = BTreeMap::new();
    let error = verify_publisher(&nix, Path::new(PACKAGE), None, &bindings, contents)
        .expect_err("the stand-in nix refuses everything");
    (fs::read_to_string(&log).unwrap_or_default(), error)
}

#[test]
fn install_rehashes_the_package_contents() {
    let (calls, error) = invocations(StoreContents::Rehash);
    assert!(calls.contains("store verify --no-trust"), "{calls}");
    assert!(error.contains("refused"), "{error}");
}

#[test]
fn boot_restore_does_not_rehash_the_package_contents() {
    let (calls, _) = invocations(StoreContents::TrustRegistered);
    assert!(!calls.contains("store verify"), "{calls}");
}
