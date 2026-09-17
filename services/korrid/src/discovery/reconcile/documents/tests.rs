use super::*;
use crate::config::{snapshot::ConfigSnapshotCoordinator, test_fixtures as readable};

type PublicationHook = Box<dyn FnMut(usize, &Path, &Path) -> Result<(), DiscoveryError>>;
thread_local! {
    static PUBLICATION_HOOK: std::cell::RefCell<Option<PublicationHook>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn publication_boundary(
    boundary: usize,
    root: &Path,
    private_root: &Path,
) -> Result<(), DiscoveryError> {
    PUBLICATION_HOOK.with_borrow_mut(|hook| match hook {
        Some(hook) => hook(boundary, root, private_root),
        None => Ok(()),
    })
}

#[test]
fn production_commit_journals_before_publication_and_recovers_minted_ids_at_every_boundary() {
    for boundary in 0..=3 {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let roms = tempfile::tempdir().unwrap();
        let discovery = DiscoveryCoordinator::for_tests(root.path(), private.path());
        discovery
            .add_location(roms.path(), &DiscoveryOptions::default())
            .unwrap();
        fs::write(roms.path().join("new.gba"), b"rom").unwrap();
        let expected = Documents::read(root.path()).unwrap();
        PUBLICATION_HOOK.set(Some(Box::new(move |at, root, private| {
            let journal = PrivateState::read(private)?
                .repair
                .pending_write
                .expect("journal precedes first publication");
            if at == 0 {
                assert_eq!(Documents::read(root)?, journal.expected);
            }
            if at == boundary {
                Err(DiscoveryError::Storage("interrupted publication".into()))
            } else {
                Ok(())
            }
        })));
        let result = discovery.rescan(&DiscoveryOptions::default());
        PUBLICATION_HOOK.set(None);
        assert!(matches!(result, Err(DiscoveryError::Storage(_))));
        let planned = PrivateState::read(private.path())
            .unwrap()
            .repair
            .pending_write
            .unwrap();
        assert_eq!(planned.expected, expected);
        let id = planned
            .candidate
            .validate()
            .unwrap()
            .games
            .keys()
            .next()
            .unwrap()
            .clone();
        let restarted = DiscoveryCoordinator::for_tests(root.path(), private.path());
        restarted.rescan(&DiscoveryOptions::default()).unwrap();
        assert_eq!(Documents::read(root.path()).unwrap(), planned.candidate);
        assert!(Documents::read(root.path())
            .unwrap()
            .validate()
            .unwrap()
            .games
            .contains_key(&id));
        assert!(!restarted.has_recovery_work());
        assert_eq!(
            PrivateState::read(private.path())
                .unwrap()
                .ownership
                .releases
                .len(),
            1
        );
    }
}

#[test]
fn external_edit_between_publications_retains_journal_and_pending_ownership() {
    let root = tempfile::tempdir().unwrap();
    let private_root = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    let discovery = DiscoveryCoordinator::for_tests(root.path(), private_root.path());
    discovery
        .add_location(roms.path(), &DiscoveryOptions::default())
        .unwrap();
    fs::write(roms.path().join("new.gba"), b"rom").unwrap();
    PUBLICATION_HOOK.set(Some(Box::new(|at, root, _| {
        if at == 2 {
            let path = root.join(GAMES_FILE_NAME);
            fs::write(
                &path,
                format!("{}# external edit\n", fs::read_to_string(&path).unwrap()),
            )
            .unwrap();
        }
        Ok(())
    })));
    let result = discovery.rescan(&DiscoveryOptions::default());
    PUBLICATION_HOOK.set(None);
    assert!(
        matches!(result, Err(DiscoveryError::Conflict)),
        "{result:?}"
    );
    let private = PrivateState::read(private_root.path()).unwrap();
    assert!(private.repair.pending_write.is_some());
    assert_eq!(private.repair.pending_ownership.len(), 1);
    assert!(private.ownership.releases.is_empty());
}

#[test]
fn settings_rejects_pending_publication_then_restart_recovers_and_save_succeeds() {
    use crate::config::settings::{self, SettingChange, SettingsError};
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let roms = tempfile::tempdir().unwrap();
    let lock = Arc::new(Mutex::new(()));
    let discovery =
        DiscoveryCoordinator::for_tests_with_write_lock(root.path(), private.path(), lock.clone());
    discovery
        .add_location(roms.path(), &DiscoveryOptions::default())
        .unwrap();
    let before = settings::read_with_registry_source(
        root.path(),
        &crate::plugin_policy::RegistrySource::Selected(Arc::new(
            crate::plugin_test_fixtures::installed(root.path()),
        )),
    )
    .unwrap();
    fs::write(roms.path().join("new.gba"), b"rom").unwrap();
    PUBLICATION_HOOK.set(Some(Box::new(|at, _, _| {
        if at == 2 {
            Err(DiscoveryError::Storage(
                "device publication interrupted".into(),
            ))
        } else {
            Ok(())
        }
    })));
    let result = discovery.rescan(&DiscoveryOptions::default());
    PUBLICATION_HOOK.set(None);
    assert!(result.is_err());
    let pending = Documents::read(root.path()).unwrap();
    let result = settings::update(
        root.path(),
        private.path(),
        &lock,
        &before.revision,
        SettingChange::DeviceName("saved name".into()),
    );
    assert!(matches!(result, Err(SettingsError::Conflict)), "{result:?}");
    assert_eq!(Documents::read(root.path()).unwrap(), pending);
    DiscoveryCoordinator::for_tests_with_write_lock(root.path(), private.path(), lock.clone())
        .rescan(&DiscoveryOptions::default())
        .unwrap();
    let recovered = settings::read_with_registry_source(
        root.path(),
        &crate::plugin_policy::RegistrySource::Selected(Arc::new(
            crate::plugin_test_fixtures::installed(root.path()),
        )),
    )
    .unwrap();
    let saved = settings::update_with_registry_source(
        root.path(),
        private.path(),
        &lock,
        &recovered.revision,
        SettingChange::DeviceName("saved name".into()),
        &crate::plugin_policy::RegistrySource::Selected(Arc::new(
            crate::plugin_test_fixtures::installed(root.path()),
        )),
    )
    .unwrap();
    assert_eq!(saved.device_name.as_deref(), Some("saved name"));
    discovery.rescan(&DiscoveryOptions::default()).unwrap();
    assert_eq!(
        Documents::read(root.path())
            .unwrap()
            .validate()
            .unwrap()
            .games
            .len(),
        1
    );
}

fn candidate() -> Documents {
    Documents {
        device: readable::gba_locations("roms", "wl4.gba", true),
        games: readable::gba_games(),
        releases: readable::gba_releases(),
    }
}

fn plan(private: &mut PrivateState, expected: Documents, candidate: Documents) {
    let games = parse_mapping(&candidate.games).unwrap();
    let releases = parse_mapping(&candidate.releases).unwrap();
    private.repair.pending_ownership.insert(
        ownership_key(readable::GBA_ID, readable::GBA_RELEASE),
        OwnedRelease {
            playable_id: readable::GBA_ID.into(),
            release_id: readable::GBA_RELEASE.into(),
            fingerprint: catalog_fingerprint(
                &games,
                &releases,
                readable::GBA_ID,
                readable::GBA_RELEASE,
            )
            .unwrap(),
        },
    );
    let device = parse_mapping(&candidate.device).unwrap();
    let location = device["locations"].as_mapping().unwrap()[readable::GBA_RELEASE]
        .as_sequence()
        .unwrap()[0]
        .as_mapping()
        .unwrap();
    private.repair.pending_locations.insert(
        location_key(readable::GBA_RELEASE, location),
        OwnedLocation {
            release_id: readable::GBA_RELEASE.into(),
            fingerprint: fingerprint_mapping(location),
        },
    );
    private.repair.pending_write = Some(PendingWrite {
        expected,
        candidate,
    });
}

#[test]
fn restart_recovers_every_readable_rename_boundary_and_pending_ownership() {
    for renamed in 0..=3 {
        let root = tempfile::tempdir().unwrap();
        let private_root = tempfile::tempdir().unwrap();
        ensure_fixed_files(root.path()).unwrap();
        let expected = Documents::read(root.path()).unwrap();
        let next = candidate();
        let mut private = PrivateState::default();
        plan(&mut private, expected, next.clone());
        private.write(private_root.path()).unwrap();
        for (name, contents) in next.files().into_iter().take(renamed) {
            fs::write(root.path().join(name), contents).unwrap();
        }
        let restarted = DiscoveryCoordinator::for_tests(root.path(), private_root.path());
        assert!(restarted.has_recovery_work());
        restarted.rescan(&DiscoveryOptions::default()).unwrap();
        assert_eq!(Documents::read(root.path()).unwrap(), next);
        let state = ConfigSnapshotCoordinator::new(root.path()).reload();
        assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
        assert!(state.snapshot.games.contains_key(readable::GBA_ID));
        let private = PrivateState::read(private_root.path()).unwrap();
        assert!(private.repair.pending_write.is_none());
        assert!(private.repair.pending_ownership.is_empty());
        assert!(private.repair.pending_locations.is_empty());
        assert_eq!(private.ownership.releases.len(), 1);
        assert_eq!(private.ownership.locations.len(), 1);
        restarted.rescan(&DiscoveryOptions::default()).unwrap();
        assert_eq!(Documents::read(root.path()).unwrap(), next);
    }
}

#[test]
fn a_conflict_in_any_document_blocks_commit_before_all_readable_writes() {
    for name in FILE_NAMES {
        let root = tempfile::tempdir().unwrap();
        let private_root = tempfile::tempdir().unwrap();
        ensure_fixed_files(root.path()).unwrap();
        let expected = Documents::read(root.path()).unwrap();
        fs::write(root.path().join(name), "{}\n# authored\n").unwrap();
        let authored = Documents::read(root.path()).unwrap();
        let error = expected
            .commit(
                candidate(),
                root.path(),
                private_root.path(),
                &mut PrivateState::default(),
            )
            .unwrap_err();
        assert!(matches!(error, DiscoveryError::Conflict));
        assert_eq!(Documents::read(root.path()).unwrap(), authored);
    }
}

#[test]
fn recovery_never_overwrites_an_external_edit_after_a_partial_commit() {
    let root = tempfile::tempdir().unwrap();
    let private_root = tempfile::tempdir().unwrap();
    ensure_fixed_files(root.path()).unwrap();
    let mut private = PrivateState::default();
    let next = candidate();
    plan(
        &mut private,
        Documents::read(root.path()).unwrap(),
        next.clone(),
    );
    private.write(private_root.path()).unwrap();
    fs::write(root.path().join(GAMES_FILE_NAME), &next.games).unwrap();
    fs::write(
        root.path().join(RELEASES_FILE_NAME),
        "{}\n# external edit\n",
    )
    .unwrap();
    let before = Documents::read(root.path()).unwrap();
    let error = DiscoveryCoordinator::for_tests(root.path(), private_root.path())
        .rescan(&DiscoveryOptions::default())
        .unwrap_err();
    assert!(matches!(error, DiscoveryError::Conflict));
    assert_eq!(Documents::read(root.path()).unwrap(), before);
    assert!(PrivateState::read(private_root.path())
        .unwrap()
        .repair
        .pending_write
        .is_some());
    // An explicit restoration to the expected bytes makes the same journal retryable.
    fs::write(root.path().join(RELEASES_FILE_NAME), "{}\n").unwrap();
    DiscoveryCoordinator::for_tests(root.path(), private_root.path())
        .rescan(&DiscoveryOptions::default())
        .unwrap();
    assert_eq!(Documents::read(root.path()).unwrap(), next);
}
