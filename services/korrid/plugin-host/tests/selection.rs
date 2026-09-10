use korri_plugin_host::{
    selection::{Desired, Receipt, SelectionStore},
    storage,
};
use std::{fs, path::PathBuf};

fn receipt() -> Receipt {
    serde_json::from_str(include_str!("fixtures/selection.json")).unwrap()
}

fn update(old: &Receipt, name: &str) -> Receipt {
    old.select(
        PathBuf::from(format!(
            "/nix/store/00000000000000000000000000000000-{name}"
        )),
        old.provenance.clone(),
        format!("{name}-approval"),
    )
    .unwrap()
}

#[test]
fn update_and_restore_swap_exact_approvals_and_preserve_current_intent() {
    for desired in [Desired::Disabled, Desired::Enabled] {
        let mut first = receipt();
        first.desired = desired.clone();
        let second = update(&first, "second");
        assert_eq!(second.previous.as_ref().unwrap().package, first.package);
        assert_eq!(second.previous.as_ref().unwrap().approval, first.approval);
        let restored = second.rollback().unwrap();
        assert_eq!(restored.package, first.package);
        assert_eq!(restored.provenance, first.provenance);
        assert_eq!(restored.approval, first.approval);
        assert_eq!(restored.desired, desired);
        assert_eq!(restored.rollback().unwrap(), second);
    }
    assert!(receipt().rollback().unwrap_err().contains("previous"));
}

#[test]
fn previous_is_bounded_and_enable_or_reapproval_does_not_erase_it() {
    let first = receipt();
    let second = update(&first, "second");
    let third = update(&second, "third");
    assert_eq!(third.previous.as_ref().unwrap().package, second.package);
    assert!(!serde_json::to_string(&third)
        .unwrap()
        .contains("current-approval"));
    assert_eq!(update(&second, "second"), second);
    let mut disabled = second.clone();
    disabled.desired = Desired::Disabled;
    assert_eq!(disabled.rollback().unwrap().desired, Desired::Disabled);
    let mut removed = second;
    removed.desired = Desired::Removed { purge: true };
    assert!(removed.rollback().is_err());
    assert!(removed
        .select(first.package, first.provenance, first.approval)
        .is_err());
}

#[test]
fn source_switch_and_restore_keep_the_exact_provenance() {
    let first = receipt();
    let mut other_source = first.provenance.clone();
    if let korri_plugin_host::provenance::Provenance::RawCache { cache_url } = &mut other_source {
        *cache_url = "https://other.example".into();
    }
    let second = first
        .select(
            first.package.clone(),
            other_source.clone(),
            "new-source-approval".into(),
        )
        .unwrap();
    assert_eq!(second.provenance, other_source);
    assert_eq!(second.rollback().unwrap().provenance, first.provenance);
}

#[test]
fn interrupted_update_keeps_both_committed_builds_until_atomic_commit() {
    for committed in [false, true] {
        for roots_written in 0..=2 {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("selection.json");
            let roots = directory.path().join("roots");
            storage::directory(&roots).unwrap();
            let store = SelectionStore::new(path.clone(), roots.clone());
            let first = receipt();
            let second = update(&first, "second");
            store.stage(&second.package).unwrap();
            store.commit(&second).unwrap();
            let third = update(&second, "third");
            store.stage(&third.package).unwrap();
            assert_eq!(fs::read_link(roots.join("active")).unwrap(), second.package);
            assert_eq!(
                fs::read_link(roots.join("previous")).unwrap(),
                first.package
            );
            assert_eq!(fs::read_link(roots.join("pending")).unwrap(), third.package);
            // The durable receipt is the commit point. Exercise each following
            // root repair boundary, as a power cut would leave it.
            if committed {
                storage::write_json(&path, &third).unwrap();
                if roots_written >= 1 {
                    storage::root_link(&roots.join("previous"), &second.package).unwrap();
                }
                if roots_written >= 2 {
                    storage::root_link(&roots.join("active"), &third.package).unwrap();
                }
            }
            let recovered = store.read().unwrap().unwrap();
            assert_eq!(
                recovered,
                if committed {
                    third.clone()
                } else {
                    second.clone()
                }
            );
            store.settle(&recovered).unwrap();
            assert_eq!(
                fs::read_link(roots.join("active")).unwrap(),
                recovered.package
            );
            assert_eq!(
                fs::read_link(roots.join("previous")).unwrap(),
                recovered.previous.unwrap().package
            );
            assert!(!roots.join("pending").exists());
            assert_eq!(fs::read_dir(&roots).unwrap().count(), 2);
            store.remove().unwrap();
            assert!(store.read().unwrap().is_none());
            assert_eq!(fs::read_dir(&roots).unwrap().count(), 0);
        }
    }
}

#[test]
fn failed_receipt_commit_keeps_current_previous_and_candidate_rooted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selection.json");
    let roots = directory.path().join("roots");
    storage::directory(&roots).unwrap();
    let store = SelectionStore::new(path.clone(), roots.clone());
    let first = receipt();
    let second = update(&first, "second");
    store.stage(&second.package).unwrap();
    store.commit(&second).unwrap();
    let third = update(&second, "third");
    store.stage(&third.package).unwrap();
    fs::create_dir(path.with_extension("new")).unwrap();
    assert!(store.commit(&third).is_err());
    assert_eq!(store.read().unwrap().unwrap(), second);
    for (name, package) in [
        ("active", &second.package),
        ("previous", &first.package),
        ("pending", &third.package),
    ] {
        assert_eq!(&fs::read_link(roots.join(name)).unwrap(), package);
    }
}

#[test]
fn recovery_of_a_committed_swap_roots_previous_before_replacing_active() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selection.json");
    let roots = directory.path().join("roots");
    storage::directory(&roots).unwrap();
    let store = SelectionStore::new(path.clone(), roots.clone());
    let second = update(&receipt(), "second");
    store.commit(&second).unwrap();
    let mut disabled = second.clone();
    disabled.desired = Desired::Disabled;
    store.commit(&disabled).unwrap();
    let restored = disabled.rollback().unwrap();
    store.stage(&restored.package).unwrap();
    storage::write_json(&path, &restored).unwrap();
    // A write failure at active must already have pinned the new previous.
    fs::create_dir(roots.join("active.new")).unwrap();
    assert!(store.settle(&restored).is_err());
    assert_eq!(
        fs::read_link(roots.join("previous")).unwrap(),
        second.package
    );
    assert_eq!(
        fs::read_link(roots.join("pending")).unwrap(),
        restored.package
    );
    fs::remove_dir(roots.join("active.new")).unwrap();
    store.settle(&store.read().unwrap().unwrap()).unwrap();
    assert_eq!(
        fs::read_link(roots.join("active")).unwrap(),
        restored.package
    );
    assert_eq!(fs::read_dir(&roots).unwrap().count(), 2);
    assert_eq!(store.read().unwrap().unwrap().desired, Desired::Disabled);
}

#[test]
fn interrupted_root_temporaries_and_completed_removal_do_not_retain_extra_builds() {
    let directory = tempfile::tempdir().unwrap();
    let roots = directory.path().join("roots");
    storage::directory(&roots).unwrap();
    let store = SelectionStore::new(directory.path().join("selection.json"), roots.clone());
    let first = receipt();
    store.commit(&first).unwrap();
    for name in ["active.new", "previous.new", "pending.new"] {
        std::os::unix::fs::symlink("/discarded", roots.join(name)).unwrap();
    }
    store.settle(&first).unwrap();
    assert_eq!(fs::read_dir(&roots).unwrap().count(), 1);
    fs::remove_file(directory.path().join("selection.json")).unwrap();
    store.remove().unwrap();
    assert_eq!(fs::read_dir(&roots).unwrap().count(), 0);
}

#[test]
fn receipts_without_the_explicit_previous_slot_and_recursive_history_are_rejected() {
    let mut value = serde_json::to_value(receipt()).unwrap();
    value.as_object_mut().unwrap().remove("previous");
    assert!(serde_json::from_value::<Receipt>(value).is_err());
    let second = update(&receipt(), "second");
    let mut value = serde_json::to_value(second).unwrap();
    value["previous"]["previous"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<Receipt>(value).is_err());
}
