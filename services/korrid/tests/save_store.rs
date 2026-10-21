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
