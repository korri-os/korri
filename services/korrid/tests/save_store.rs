use korrid::saves::{store::NativeSaveStore, types::*, SaveRepository};

const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
fn record() -> SnapshotRecord {
    SnapshotRecord {
        parents: vec![],
        game_id: "gba/wario".into(),
        release_id: None,
        runner_id: "@korri:libretro/mgba".into(),
        device: PERSON.into(),
        ended_at: "2026-10-21T00:00:00.000Z".into(),
        duration_seconds: 10,
        interrupted: false,
        locations: vec![],
    }
}

#[test]
fn snapshot_round_trips_and_is_its_games_head() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let original = record();
    let id = store.put_record(PERSON, &original).unwrap();
    assert_eq!(store.records(PERSON, "gba/wario").unwrap(), vec![original]);
    assert_eq!(
        store.heads(PERSON).unwrap().games,
        vec![GameSaveHeads {
            game_id: "gba/wario".into(),
            heads: vec![id]
        }]
    );
}

#[test]
fn object_transfers_verify_bytes_before_confirmation_and_are_person_scoped() {
    use std::io::Read;
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let hash = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let mut snapshot = record();
    snapshot.locations = vec![SnapshotLocation {
        key: "saves".into(),
        kind: SaveKind::Save,
        files: vec![SaveFile {
            path: "wario.srm".into(),
            sha256: hash.into(),
            bytes: 3,
        }],
    }];
    let id = store.put_record(PERSON, &snapshot).unwrap();
    assert!(!store.complete(PERSON, "gba/wario", &id).unwrap());
    assert!(store.put_object(PERSON, hash, &mut &b"wrong"[..]).is_err());
    assert!(!store.complete(PERSON, "gba/wario", &id).unwrap());
    store.put_object(PERSON, hash, &mut &b"abc"[..]).unwrap();
    assert!(store.complete(PERSON, "gba/wario", &id).unwrap());
    let mut bytes = Vec::new();
    store
        .object(PERSON, hash)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"abc");
    assert!(store.object(&"1".repeat(64), hash).is_err());
}

#[test]
fn save_store_refuses_symlinks_in_private_paths() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("saves")).unwrap();
    assert!(NativeSaveStore::new(root.path())
        .put_record(PERSON, &record())
        .is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn different_progress_from_same_parent_requires_a_choice_not_the_latest_clock() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let base = store.put_record(PERSON, &record()).unwrap();
    let mut a = record();
    a.parents = vec![base.clone()];
    a.locations = vec![SnapshotLocation {
        key: "saves".into(),
        kind: SaveKind::Save,
        files: vec![SaveFile {
            path: "a.srm".into(),
            sha256: korrid::saves::record::hash(b"a"),
            bytes: 1,
        }],
    }];
    store.put_record(PERSON, &a).unwrap();
    let mut b = record();
    b.parents = vec![base];
    b.ended_at = "2026-10-20T00:00:00.000Z".into();
    b.locations = vec![SnapshotLocation {
        key: "saves".into(),
        kind: SaveKind::Save,
        files: vec![SaveFile {
            path: "b.srm".into(),
            sha256: korrid::saves::record::hash(b"b"),
            bytes: 1,
        }],
    }];
    store.put_record(PERSON, &b).unwrap();
    assert_eq!(
        store.reconcile(PERSON, "gba/wario").unwrap(),
        SaveMark::Split
    );
}

#[test]
fn matching_progress_and_screenshot_only_changes_join_identically_on_two_devices() {
    for screenshots in [false, true] {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        let stores = [
            NativeSaveStore::new(left.path()),
            NativeSaveStore::new(right.path()),
        ];
        let a = record();
        let mut b = record();
        b.device = "1".repeat(64);
        if screenshots {
            b.locations.push(SnapshotLocation {
                key: "screenshots".into(),
                kind: SaveKind::Screenshot,
                files: vec![SaveFile {
                    path: "frame.png".into(),
                    sha256: korrid::saves::record::hash(b"frame"),
                    bytes: 5,
                }],
            });
        }
        for store in &stores {
            store.put_record(PERSON, &a).unwrap();
            store.put_record(PERSON, &b).unwrap();
            if screenshots {
                store
                    .put_object(
                        PERSON,
                        &korrid::saves::record::hash(b"frame"),
                        &mut &b"frame"[..],
                    )
                    .unwrap();
            }
            assert_ne!(
                store.reconcile(PERSON, "gba/wario").unwrap(),
                SaveMark::Split
            );
        }
        assert_eq!(
            stores[0].heads(PERSON).unwrap(),
            stores[1].heads(PERSON).unwrap()
        );
        assert_eq!(stores[0].heads(PERSON).unwrap().games[0].heads.len(), 1);
        let joined = stores[0]
            .records(PERSON, "gba/wario")
            .unwrap()
            .into_iter()
            .find(|r| r.parents.len() == 2)
            .unwrap();
        assert!(!joined.interrupted);
        assert_eq!(joined.locations.len(), usize::from(screenshots));
    }
}

#[test]
fn choosing_and_restoring_append_snapshots_and_preserve_every_side() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let base = store.put_record(PERSON, &record()).unwrap();
    let mut a = record();
    a.parents = vec![base.clone()];
    a.runner_id = "runner-a".into();
    let a_id = store.put_record(PERSON, &a).unwrap();
    let mut b = a.clone();
    b.runner_id = "runner-b".into();
    let b_id = store.put_record(PERSON, &b).unwrap();
    let picked = store
        .choose(
            PERSON,
            "gba/wario",
            &a_id,
            PERSON,
            "2026-10-21T01:00:00.000Z",
        )
        .unwrap();
    let records = store.records(PERSON, "gba/wario").unwrap();
    let choice = records
        .iter()
        .find(|r| korrid::saves::record::snapshot_id(r).unwrap() == picked)
        .unwrap();
    let mut parents = vec![a_id, b_id];
    parents.sort();
    assert_eq!(choice.parents, parents);
    assert_eq!(choice.runner_id, "runner-a");
    assert_eq!(records.len(), 4);
    let restored = store
        .restore(
            PERSON,
            "gba/wario",
            &base,
            PERSON,
            "2026-10-21T02:00:00.000Z",
        )
        .unwrap();
    let records = store.records(PERSON, "gba/wario").unwrap();
    let restore = records
        .iter()
        .find(|r| korrid::saves::record::snapshot_id(r).unwrap() == restored)
        .unwrap();
    assert_eq!(restore.parents, vec![picked]);
    assert_eq!(restore.runner_id, record().runner_id);
    assert_eq!(records.len(), 5);
}

#[test]
fn boundary_capture_preserves_files_and_materializes_only_the_chosen_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let saves = tempfile::tempdir().unwrap();
    let folder = saves.path().join("saves");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("game.srm"), b"progress").unwrap();
    let locations = vec![SaveLocation {
        key: "saves".into(),
        kind: SaveKind::Save,
        path: folder.to_str().unwrap().into(),
    }];
    let captured = store.capture(PERSON, &locations, record()).unwrap();
    let id = store.put_record(PERSON, &captured).unwrap();
    std::fs::write(folder.join("game.srm"), b"later").unwrap();
    store
        .materialize(
            PERSON,
            "gba/wario",
            &id,
            saves.path().to_str().unwrap(),
            &locations,
        )
        .unwrap();
    assert_eq!(std::fs::read(folder.join("game.srm")).unwrap(), b"progress");
}

#[test]
fn retention_never_prunes_an_unconfirmed_snapshot_and_keeps_ancestry() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let mut parent = None;
    let mut first = None;
    for n in 0..12 {
        let content = format!("save {n}");
        let hash = korrid::saves::record::hash(content.as_bytes());
        let mut r = record();
        r.parents = parent.into_iter().collect();
        r.ended_at = format!("2026-10-21T00:00:{n:02}.000Z");
        r.locations = vec![SnapshotLocation {
            key: "saves".into(),
            kind: SaveKind::Save,
            files: vec![SaveFile {
                path: "game.srm".into(),
                sha256: hash.clone(),
                bytes: content.len() as u64,
            }],
        }];
        let id = store.put_record(PERSON, &r).unwrap();
        store
            .put_object(PERSON, &hash, &mut content.as_bytes())
            .unwrap();
        if first.is_none() {
            first = Some((id.clone(), hash));
        }
        parent = Some(id);
    }
    let (id, hash) = first.unwrap();
    store.prune(PERSON, "gba/wario", PERSON).unwrap();
    assert!(store.object(PERSON, &hash).is_ok());
    store
        .confirm(PERSON, "gba/wario", &id, &"1".repeat(64))
        .unwrap();
    store.prune(PERSON, "gba/wario", PERSON).unwrap();
    assert!(store.object(PERSON, &hash).is_err());
    assert_eq!(store.records(PERSON, "gba/wario").unwrap().len(), 12);
    assert_eq!(
        store.heads(PERSON).unwrap().games[0].heads,
        vec![parent.unwrap()]
    );
}

#[test]
fn an_incomplete_newer_head_does_not_block_continuing_the_last_local_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let local = store.put_record(PERSON, &record()).unwrap();
    let mut newer = record();
    newer.device = "1".repeat(64);
    newer.parents = vec![local.clone()];
    newer.locations = vec![SnapshotLocation {
        key: "saves".into(),
        kind: SaveKind::Save,
        files: vec![SaveFile {
            path: "game.srm".into(),
            sha256: korrid::saves::record::hash(b"elsewhere"),
            bytes: 9,
        }],
    }];
    store.put_record(PERSON, &newer).unwrap();
    assert_eq!(
        store.launch_base(PERSON, "gba/wario", PERSON).unwrap(),
        Some(local)
    );
    assert_eq!(
        store.in_use_warnings(PERSON, "gba/wario").unwrap()[0].device,
        "1".repeat(64)
    );
}

#[test]
fn picking_progress_keeps_both_screenshots_when_names_collide() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let mut ids = vec![];
    for bytes in [b"a", b"b"] {
        let hash = korrid::saves::record::hash(bytes);
        let mut side = record();
        side.locations = vec![
            SnapshotLocation {
                key: "saves".into(),
                kind: SaveKind::Save,
                files: vec![SaveFile {
                    path: "game.srm".into(),
                    sha256: hash.clone(),
                    bytes: 1,
                }],
            },
            SnapshotLocation {
                key: "screenshots".into(),
                kind: SaveKind::Screenshot,
                files: vec![SaveFile {
                    path: "frame.png".into(),
                    sha256: hash.clone(),
                    bytes: 1,
                }],
            },
        ];
        ids.push(store.put_record(PERSON, &side).unwrap());
        store
            .put_object(PERSON, &hash, &mut bytes.as_slice())
            .unwrap();
    }
    let chosen = store
        .choose(
            PERSON,
            "gba/wario",
            &ids[0],
            PERSON,
            "2026-10-21T01:00:00.000Z",
        )
        .unwrap();
    let record = store
        .records(PERSON, "gba/wario")
        .unwrap()
        .into_iter()
        .find(|r| korrid::saves::record::snapshot_id(r).unwrap() == chosen)
        .unwrap();
    let images = &record
        .locations
        .iter()
        .find(|l| l.kind == SaveKind::Screenshot)
        .unwrap()
        .files;
    assert_eq!(images.len(), 2);
    assert!(images
        .iter()
        .any(|f| f.path == "frame.png" && f.sha256 == korrid::saves::record::hash(b"b")));
    assert!(images.iter().any(|f| f.path
        == format!(
            "frame~{}.png",
            korrid::saves::record::hash(b"a")
                .strip_prefix("sha256:")
                .unwrap()
        )));
}

#[test]
fn retention_ranks_ancestry_not_device_clocks() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let mut parent = None;
    let mut hashes = Vec::new();
    for n in 0..12 {
        let content = format!("progress {n}");
        let hash = korrid::saves::record::hash(content.as_bytes());
        let mut snapshot = record();
        snapshot.parents = parent.into_iter().collect();
        // Every newer session claims an earlier clock time.
        snapshot.ended_at = format!("2026-10-21T00:00:{:02}.000Z", 11 - n);
        snapshot.locations = vec![SnapshotLocation {
            key: "saves".into(),
            kind: SaveKind::Save,
            files: vec![SaveFile {
                path: "game.srm".into(),
                sha256: hash.clone(),
                bytes: content.len() as u64,
            }],
        }];
        let id = store.put_record(PERSON, &snapshot).unwrap();
        store
            .put_object(PERSON, &hash, &mut content.as_bytes())
            .unwrap();
        store
            .confirm(PERSON, "gba/wario", &id, &"1".repeat(64))
            .unwrap();
        hashes.push(hash);
        parent = Some(id);
    }
    store.prune(PERSON, "gba/wario", PERSON).unwrap();
    assert!(
        store.object(PERSON, &hashes[0]).is_err(),
        "the oldest ancestor is pruned despite its later clock"
    );
    assert!(
        store.object(PERSON, &hashes[10]).is_ok(),
        "the recent ancestor stays despite its earlier clock"
    );
}
