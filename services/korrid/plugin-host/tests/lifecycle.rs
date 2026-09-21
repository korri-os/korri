use korri_plugin_host::{
    lifecycle::{LifecyclePolicy, RequiredPlugin},
    selection::{Desired, Receipt, SelectionStore},
    software_cleanup::SoftwareCleanup,
    storage,
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

fn receipt() -> Receipt {
    serde_json::from_str(include_str!("fixtures/selection.json")).unwrap()
}

fn updated(old: &Receipt) -> Receipt {
    old.select(
        PathBuf::from("/nix/store/00000000000000000000000000000000-updated"),
        old.provenance.clone(),
        "updated-approval".into(),
    )
    .unwrap()
}

#[test]
fn caller_without_authoritative_policy_cannot_remove_any_plugin() {
    let error = LifecyclePolicy::removal_unavailable()
        .check_removal("@example:clock")
        .unwrap_err();
    assert!(
        error.contains("no authoritative lifecycle policy"),
        "{error}"
    );
}

#[test]
fn required_behavior_refuses_removal_by_name_without_treating_other_plugins_as_required() {
    let required = RequiredPlugin::new("@example:clock", "portal accepts local input").unwrap();
    let policy = LifecyclePolicy::new(vec![required]).unwrap();

    let error = policy.check_removal("@example:clock").unwrap_err();
    assert!(error.contains("portal accepts local input"), "{error}");
    assert!(error.contains("@example:clock"), "{error}");
    policy.check_removal("@example:optional").unwrap();

    for invalid in ["", "  ", "line\nbreak"] {
        assert!(RequiredPlugin::new("@example:clock", invalid).is_err());
    }
}

#[test]
fn removal_releases_both_selections_before_cleanup_and_keeps_the_receipt_until_cleanup_succeeds() {
    let directory = tempfile::tempdir().unwrap();
    let roots = directory.path().join("roots");
    storage::directory(&roots).unwrap();
    let receipt_path = directory.path().join("selection.json");
    let store = SelectionStore::new(receipt_path, roots.clone());
    let mut selected = updated(&receipt());
    selected.desired = Desired::Removed { purge: false };
    store.commit(&selected).unwrap();

    store.release_software().unwrap();

    assert_eq!(store.read().unwrap(), Some(selected));
    assert!(store.software_released().unwrap());
    assert_eq!(fs::read_dir(&roots).unwrap().count(), 0);

    store.finish_removal().unwrap();
    assert!(store.read().unwrap().is_none());
}

#[test]
fn declining_an_update_keeps_the_current_and_previous_selections() {
    let directory = tempfile::tempdir().unwrap();
    let roots = directory.path().join("roots");
    storage::directory(&roots).unwrap();
    let store = SelectionStore::new(directory.path().join("selection.json"), roots.clone());
    let current = updated(&receipt());
    store.commit(&current).unwrap();
    let candidate = current
        .select(
            PathBuf::from("/nix/store/00000000000000000000000000000000-declined"),
            current.provenance.clone(),
            "declined-approval".into(),
        )
        .unwrap();

    // Inspection can stage the exact candidate for review, but declining its
    // permissions never writes the atomic receipt commit point.
    store.stage(&candidate.package).unwrap();
    store.settle(&store.read().unwrap().unwrap()).unwrap();

    assert_eq!(store.read().unwrap(), Some(current.clone()));
    assert_eq!(
        fs::read_link(roots.join("active")).unwrap(),
        current.package
    );
    assert_eq!(
        fs::read_link(roots.join("previous")).unwrap(),
        current.previous.unwrap().package
    );
    assert!(!roots.join("pending").exists());
}

#[test]
fn software_cleanup_failure_is_reported_and_can_be_retried() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("calls");
    let helper = directory.path().join("nix");
    fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nif [ ! -e {} ]; then touch {}; exit 42; fi\n",
            marker.display(),
            directory.path().join("retry").display(),
            directory.path().join("retry").display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let cleanup = SoftwareCleanup::new(helper);

    let error = cleanup.reclaim().unwrap_err();
    assert!(error.contains("storage cleanup failed"), "{error}");
    cleanup.reclaim().unwrap();
    assert_eq!(
        fs::read_to_string(marker).unwrap(),
        "--extra-experimental-features nix-command store gc\n--extra-experimental-features nix-command store gc\n"
    );
}
