use super::*;
use std::{fs, time::Instant};

fn ready(feed: &FileChanges, after: u64) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let InputState::Ready(revision) = feed.current() {
            if revision > after {
                return revision;
            }
        }
        assert!(
            Instant::now() < deadline,
            "save input feed did not become ready: {:?}",
            feed.current()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn reading_unchanged_input_does_not_invalidate_but_atomic_replacement_does() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("library.yaml");
    fs::write(&path, "library: {}\n").unwrap();
    let feed = FileChanges::watch(vec![InputPath {
        path: path.clone(),
        recursive: false,
        filter: None,
    }])
    .unwrap();
    let revision = ready(&feed, 0);
    let changes = feed.subscribe();
    for _ in 0..20 {
        fs::read(&path).unwrap();
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(feed.current(), InputState::Ready(revision));
    assert!(!changes.has_changed().unwrap());
    let temp = root.path().join("replacement");
    fs::write(&temp, "library: {one: {}}\n").unwrap();
    fs::rename(temp, &path).unwrap();
    ready(&feed, revision);
}

#[test]
fn a_missing_save_directory_and_its_replacement_both_become_watched() {
    let root = tempfile::tempdir().unwrap();
    let saves = root.path().join("saves");
    let feed = FileChanges::watch(vec![InputPath {
        path: saves.clone(),
        recursive: true,
        filter: None,
    }])
    .unwrap();
    let revision = ready(&feed, 0);
    fs::create_dir(&saves).unwrap();
    let revision = ready(&feed, revision);
    fs::write(saves.join("record.json"), "{}").unwrap();
    let revision = ready(&feed, revision);
    fs::rename(&saves, root.path().join("previous")).unwrap();
    fs::create_dir(&saves).unwrap();
    let revision = ready(&feed, revision);
    // A separate write proves the feed watches the new inode, not just the
    // earlier rename/create events on the parent.
    std::thread::sleep(Duration::from_millis(100));
    let revision = ready(&feed, revision - 1);
    fs::write(saves.join("new-record.json"), "{}").unwrap();
    ready(&feed, revision);
}

#[test]
fn config_symlink_target_edits_and_retargeting_invalidate_the_projection() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let one = outside.path().join("one.yaml");
    let two = outside.path().join("two.yaml");
    fs::write(&one, "{}").unwrap();
    fs::write(&two, "{}").unwrap();
    let path = root.path().join("device.yaml");
    std::os::unix::fs::symlink(&one, &path).unwrap();
    let feed = FileChanges::watch(vec![InputPath {
        path: path.clone(),
        recursive: false,
        filter: None,
    }])
    .unwrap();
    let revision = ready(&feed, 0);
    fs::write(&one, "host: {}\n").unwrap();
    let revision = ready(&feed, revision);
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&two, &path).unwrap();
    ready(&feed, revision);
    std::thread::sleep(Duration::from_millis(100));
    let revision = ready(&feed, 0);
    fs::write(&two, "host: {}\n").unwrap();
    ready(&feed, revision);
}
