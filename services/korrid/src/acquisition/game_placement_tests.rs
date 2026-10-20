//! Non-clobbering placement, containment, startup cleanup and hash seeding.
use super::{
    download::STAGING_PREFIX,
    placement::{self, Placed},
    Failure,
};
use crate::discovery::{DiscoveryCoordinator, DiscoveryOptions};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn staged(directory: &Path, bytes: &[u8]) -> (std::path::PathBuf, String) {
    let path = directory.join(format!("{STAGING_PREFIX}test"));
    fs::write(&path, bytes).unwrap();
    (path, hex::encode(Sha256::digest(bytes)))
}

#[test]
fn placement_reuses_identical_bytes_and_never_clobbers_a_different_file() {
    let work = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let (path, digest) = staged(work.path(), b"new cart");
    fs::write(source.path().join("cart.p8.png"), b"someone else's cart").unwrap();

    let placed =
        placement::place_game(&path, &digest, source.path(), "", "cart.p8.png", 6).unwrap();
    // The compound system extension stays intact, so discovery still matches it.
    assert_eq!(placed.relative, "cart (1).p8.png");
    assert!(!placed.reused);
    assert_eq!(
        fs::read(source.path().join("cart.p8.png")).unwrap(),
        b"someone else's cart"
    );
    assert_eq!(fs::read(&placed.path).unwrap(), b"new cart");

    let again = placement::place_game(&path, &digest, source.path(), "", "cart.p8.png", 6).unwrap();
    assert_eq!(
        again,
        Placed {
            relative: "cart (1).p8.png".into(),
            path: placed.path.clone(),
            reused: true
        }
    );
    // No temp file is left in the target directory.
    let names: Vec<_> = fs::read_dir(source.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");

    // Legacy: twenty names at most.
    let crowded = tempfile::tempdir().unwrap();
    fs::write(crowded.path().join("rom.gb"), b"0").unwrap();
    for n in 1..20 {
        fs::write(crowded.path().join(format!("rom ({n}).gb")), format!("{n}")).unwrap();
    }
    assert_eq!(
        placement::place_game(&path, &digest, crowded.path(), "", "rom.gb", 2).err(),
        Some(Failure::NameTaken)
    );
}

#[test]
fn placement_stays_inside_the_source_root() {
    let work = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let (path, digest) = staged(work.path(), b"rom");
    std::os::unix::fs::symlink(outside.path(), source.path().join("escape")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("x.gb"), source.path().join("link.gb")).unwrap();
    for (directory, name) in [
        ("..", "rom.gb"),
        ("a/../..", "rom.gb"),
        ("escape", "rom.gb"),
        ("", "../rom.gb"),
        ("", "a/rom.gb"),
        ("", "rom\0.gb"),
        ("", ".gb"),
        ("", ".hidden.gb"),
    ] {
        assert_eq!(
            placement::place_game(&path, &digest, source.path(), directory, name, 2).err(),
            Some(Failure::UnsafePath),
            "{directory:?} {name:?}"
        );
    }
    // A symlink occupying the name is never followed or replaced.
    let placed = placement::place_game(&path, &digest, source.path(), "", "link.gb", 2).unwrap();
    assert_eq!(placed.relative, "link (1).gb");
    assert!(!outside.path().join("x.gb").exists());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    // A layout directory is created inside the root.
    let nested =
        placement::place_game(&path, &digest, source.path(), "Games/GB", "rom.gb", 2).unwrap();
    assert_eq!(nested.relative, "Games/GB/rom.gb");
}

#[test]
fn startup_removes_game_staging_files_only() {
    let root = tempfile::tempdir().unwrap();
    let staging = placement::game_staging(root.path(), "@test:shop", "@test:shop").unwrap();
    assert_eq!(staging, root.path().join("cache/shop/shop/.downloads"));
    assert_eq!(
        fs::metadata(&staging).unwrap().permissions().mode() & 0o777,
        0o700
    );
    fs::write(staging.join(format!("{STAGING_PREFIX}abc")), b"partial").unwrap();
    fs::write(
        staging.join(format!("{}.part", "a".repeat(64))),
        b"pinned partial",
    )
    .unwrap();
    placement::remove_game_staging(&staging).unwrap();
    let names: Vec<_> = fs::read_dir(&staging)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, [format!("{}.part", "a".repeat(64))]);
    placement::remove_game_staging(&root.path().join("missing")).unwrap();
}

/// The scan after a placement reads no file bytes: older files are cached
/// from earlier scans and the placed file is seeded with its write digest.
#[test]
fn a_seeded_placement_costs_the_next_scan_no_hashed_bytes() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let games = tempfile::tempdir().unwrap();
    let old = vec![7u8; 256 * 1024];
    fs::write(games.path().join("old.gba"), &old).unwrap();
    let writer = DiscoveryCoordinator::for_tests(root.path(), private.path());
    let options = DiscoveryOptions::default();
    let first = writer.pair(games.path().into(), "gba", &options).unwrap();
    assert_eq!(first.hashed_bytes, old.len() as u64);
    assert_eq!(writer.rescan(&options).unwrap().hashed_bytes, 0);

    let work = tempfile::tempdir().unwrap();
    let rom = vec![9u8; 128 * 1024];
    let (path, digest) = staged(work.path(), &rom);
    let placed = placement::place_game(&path, &digest, games.path(), "", "new.gba", 3).unwrap();
    let sha1 = hex::encode(sha1::Sha1::digest(&rom));
    assert!(writer.seed_hash(&placed.path, &[], &digest, &sha1).unwrap());

    let scan = writer.rescan(&options).unwrap();
    assert_eq!(scan.hashed_bytes, 0);
    let published = writer.published().unwrap();
    let key = format!("sha256:{digest}");
    assert!(
        published.locations.contains_key(&key),
        "{:?}",
        published.locations.keys()
    );
    assert!(published.library.values().any(|game| game
        .releases
        .iter()
        .any(|release| release.identity.as_ref().is_some_and(|i| i.value() == key))));

    // Control: an unseeded new file is hashed once by the scan.
    fs::write(games.path().join("other.gba"), vec![3u8; 64 * 1024]).unwrap();
    assert_eq!(writer.rescan(&options).unwrap().hashed_bytes, 64 * 1024);
}

#[test]
fn a_header_rule_that_strips_bytes_is_never_seeded_with_whole_file_digests() {
    let private = tempfile::tempdir().unwrap();
    let file = private.path().join("rom.nes");
    fs::write(&file, [b"NES\x1a".as_slice(), &[0u8; 28]].concat()).unwrap();
    let rule = crate::plugin::HeaderRule {
        size: 16,
        bytes: Some(crate::plugin::BytesRule {
            offset: 0,
            hex: "4e45531a".into(),
        }),
        remainder: None,
    };
    let mut cache = crate::discovery::identify::HashCache::default();
    assert!(!cache.seed(&file, &[rule], "a", "b").unwrap());
    assert!(cache.seed(&file, &[], "a", "b").unwrap());
}
