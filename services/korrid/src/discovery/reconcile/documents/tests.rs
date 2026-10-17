use super::*;

fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Documents, Documents) {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    ensure_fixed_files(root.path()).unwrap();
    let current = Documents::read(root.path()).unwrap();
    let candidate = Documents { device: "host: {title: desk}\n".into(), library: format!("library:\n  game:\n    title: Game\n    releases:\n      - id: gba\n        system: gba\n        identity: {{kind: hash, value: 'sha256:{}'}}\n", "a".repeat(64)) };
    (root, private, current, candidate)
}
#[test]
fn conflicting_external_edit_is_never_overwritten() {
    let (root, private, current, candidate) = fixture();
    fs::write(
        root.path().join(DEVICE_FILE_NAME),
        "host: {title: edited}\n",
    )
    .unwrap();
    let mut state = PrivateState::default();
    assert!(matches!(
        current.commit(candidate, root.path(), private.path(), &mut state),
        Err(DiscoveryError::Conflict)
    ));
    assert_eq!(
        fs::read_to_string(root.path().join(LIBRARY_FILE_NAME)).unwrap(),
        current.library
    );
}
#[test]
fn recovery_finishes_each_partial_publication_and_its_private_provenance() {
    for written in 0..=2 {
        let (root, private, current, candidate) = fixture();
        let ownership = OwnershipJournal {
            created_storages: BTreeSet::new(),
            origins: BTreeMap::from([("sha256:a".into(), BTreeSet::from(["gba".into()]))]),
            paths: BTreeMap::from([(
                "gba\nGame.gba".into(),
                PathRecord {
                    identity: "sha256:a".into(),
                    files: vec!["Game.gba".into()],
                },
            )]),
        };
        let state = PrivateState {
            repair: RepairJournal {
                pending_write: Some(PendingWrite {
                    expected: current.clone(),
                    candidate: candidate.clone(),
                    ownership: ownership.clone(),
                }),
            },
            ..PrivateState::default()
        };
        state.write(private.path()).unwrap();
        for (name, bytes) in candidate.files().into_iter().take(written) {
            fs::write(root.path().join(name), bytes).unwrap();
        }
        let mut restarted = PrivateState::read(private.path()).unwrap();
        assert!(recover(root.path(), private.path(), &mut restarted).unwrap());
        assert_eq!(Documents::read(root.path()).unwrap(), candidate);
        assert_eq!(restarted.ownership, ownership);
        assert!(restarted.repair.pending_write.is_none());
    }
}
#[test]
fn recovery_stops_on_an_external_edit_and_keeps_the_repair_plan() {
    let (root, private, current, candidate) = fixture();
    let mut state = PrivateState {
        repair: RepairJournal {
            pending_write: Some(PendingWrite {
                expected: current,
                candidate,
                ownership: OwnershipJournal::default(),
            }),
        },
        ..PrivateState::default()
    };
    state.write(private.path()).unwrap();
    fs::write(
        root.path().join(DEVICE_FILE_NAME),
        "host: {title: edited}\n",
    )
    .unwrap();
    assert!(matches!(
        recover(root.path(), private.path(), &mut state),
        Err(DiscoveryError::Conflict)
    ));
    assert!(PrivateState::read(private.path())
        .unwrap()
        .repair
        .pending_write
        .is_some());
}
#[test]
fn unchanged_publication_preserves_comments_order_and_file_times() {
    let (root, private, _, _) = fixture();
    fs::write(
        root.path().join(DEVICE_FILE_NAME),
        "# comment\nhost:\n  title: desk\n",
    )
    .unwrap();
    fs::write(root.path().join(LIBRARY_FILE_NAME), "# empty library\n{}\n").unwrap();
    let current = Documents::read(root.path()).unwrap();
    let snapshot = current.validate().unwrap();
    let candidate = current.with_snapshot(&snapshot, &snapshot).unwrap();
    current
        .commit(
            candidate,
            root.path(),
            private.path(),
            &mut PrivateState::default(),
        )
        .unwrap();
    assert_eq!(Documents::read(root.path()).unwrap(), current);
    assert!(!state_file(private.path(), REPAIR_FILE).exists());
}
