use super::*;
use crate::{
    file_changes::InputPath,
    saves::{store::NativeSaveStore, types::SnapshotRecord, SaveRepository},
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn record() -> SnapshotRecord {
    SnapshotRecord {
        parents: vec![],
        game_id: "game".into(),
        release_id: None,
        runner_id: "runner".into(),
        device: "22".repeat(32),
        ended_at: "2026-10-21T00:00:00.000Z".into(),
        duration_seconds: 1,
        interrupted: false,
        locations: vec![],
    }
}

#[tokio::test]
async fn a_different_person_never_reads_the_previous_persons_cached_marks() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("device.yaml");
    std::fs::write(&path, "{}").unwrap();
    let projection = SaveStatusProjection::new(vec![InputPath {
        path,
        recursive: false,
        filter: None,
    }])
    .unwrap();
    projection.ready().await.unwrap();
    projection
        .read("first person", || Ok(SaveStatus { games: vec![] }))
        .unwrap();
    let rebuilt = AtomicUsize::new(0);
    projection
        .read("second person", || {
            rebuilt.fetch_add(1, Ordering::SeqCst);
            Ok(SaveStatus { games: vec![] })
        })
        .unwrap();
    assert_eq!(rebuilt.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unchanged_requests_build_status_once() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("device.yaml");
    std::fs::write(&path, "{}").unwrap();
    let projection = SaveStatusProjection::new(vec![InputPath {
        path,
        recursive: false,
        filter: None,
    }])
    .unwrap();
    projection.ready().await.unwrap();
    let builds = AtomicUsize::new(0);
    for _ in 0..20 {
        projection
            .read("person", || {
                builds.fetch_add(1, Ordering::SeqCst);
                Ok(SaveStatus { games: vec![] })
            })
            .unwrap();
    }
    assert_eq!(builds.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_store_commit_invalidates_before_the_next_read_returns() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let person = "11".repeat(32);
    let mut record = record();
    let first = store.put_record(&person, &record).unwrap();
    let projection = SaveStatusProjection::new(vec![InputPath {
        path: store.status_input_root().to_owned(),
        recursive: true,
        filter: Some(NativeSaveStore::affects_status),
    }])
    .unwrap();
    projection.ready().await.unwrap();
    projection
        .read("person", || Ok(SaveStatus { games: vec![] }))
        .unwrap();
    record.parents.push(first);
    record.ended_at = "2026-10-21T00:01:00.000Z".into();
    store.put_record(&person, &record).unwrap();
    // No waiting for the filesystem callback: the producer must invalidate
    // before returning to a caller that can issue another status request.
    let rebuilt = AtomicUsize::new(0);
    projection
        .read("person", || {
            rebuilt.fetch_add(1, Ordering::SeqCst);
            Ok(SaveStatus { games: vec![] })
        })
        .unwrap();
    assert_eq!(rebuilt.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_commit_during_computation_is_not_cached_under_the_new_revision() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("device.yaml");
    std::fs::write(&path, "{}").unwrap();
    let projection = SaveStatusProjection::new(vec![InputPath {
        path: path.clone(),
        recursive: false,
        filter: None,
    }])
    .unwrap();
    projection.ready().await.unwrap();
    projection
        .read("person", || {
            crate::file_changes::committed(&path);
            Ok(SaveStatus { games: vec![] })
        })
        .unwrap();
    let rebuilt = AtomicUsize::new(0);
    projection
        .read("person", || {
            rebuilt.fetch_add(1, Ordering::SeqCst);
            Ok(SaveStatus { games: vec![] })
        })
        .unwrap();
    assert_eq!(rebuilt.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn peer_liveness_advisories_do_not_rebuild_save_marks() {
    let root = tempfile::tempdir().unwrap();
    let store = NativeSaveStore::new(root.path());
    let person = "11".repeat(32);
    store.put_record(&person, &record()).unwrap();
    let projection = SaveStatusProjection::new(vec![InputPath {
        path: store.status_input_root().to_owned(),
        recursive: true,
        filter: Some(NativeSaveStore::affects_status),
    }])
    .unwrap();
    projection.ready().await.unwrap();
    projection
        .read("person", || Ok(SaveStatus { games: vec![] }))
        .unwrap();
    store
        .remember_in_use(&person, "game", &"22".repeat(32), Some(1))
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    projection
        .read("person", || {
            panic!("advisory or temporary file invalidated save status")
        })
        .unwrap();
}
