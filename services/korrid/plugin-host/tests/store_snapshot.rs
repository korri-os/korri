//! Boot restore reads every enabled plugin's store metadata with one Nix
//! call, not three per plugin. On an RK3566 each call costs 0.3-0.4 s.
//!
//! A stand-in `nix` records each invocation and prints canned metadata.
use korri_plugin_host::package::{nix32_sha256, StoreSnapshot};
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    path::PathBuf,
    sync::{Mutex, MutexGuard, PoisonError},
};

const A: &str = "/nix/store/0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-plugin-a";
const B: &str = "/nix/store/0bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-plugin-b";
const LIB: &str = "/nix/store/0ccccccccccccccccccccccccccccccc-lib";
const LIBC: &str = "/nix/store/0ddddddddddddddddddddddddddddddd-libc";
const EMPTY: &str = "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=";

/// Tests run on parallel threads. If one test forks to run its stand-in while
/// another still holds its own stand-in open for writing, the forked child
/// inherits that descriptor until it execs, and the second exec fails with
/// ETXTBSY ("Text file busy"). An aarch64 CI build hit exactly that. Each test
/// that writes and runs a stand-in holds this lock for the whole time.
static STAND_IN: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    STAND_IN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn stand_in(directory: &Path, metadata: &str) -> (PathBuf, PathBuf) {
    let log = directory.join("calls");
    let json = directory.join("metadata.json");
    fs::write(&json, metadata).unwrap();
    let nix = directory.join("nix");
    fs::write(
        &nix,
        format!(
            "#!/bin/sh\necho \"$*\" >> {}\ncat {}\n",
            log.display(),
            json.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&nix, fs::Permissions::from_mode(0o755)).unwrap();
    (nix, log)
}

fn info(references: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "ca": null, "deriver": null, "narHash": EMPTY, "narSize": 8,
        "references": references, "registrationTime": 1,
        "signatures": ["publisher-1:c2lnbmF0dXJl"], "ultimate": false
    })
}

#[test]
fn nix32_matches_nix_for_a_known_hash() {
    // `nix hash convert --hash-algo sha256 --to nix32` on the RG353M, Nix 2.31.2.
    assert_eq!(
        nix32_sha256(EMPTY).unwrap(),
        "0mdqa9w1p6cmli6976v4wi0sw9r4p5prkj7lzfd1877wk11c9c73"
    );
    assert!(nix32_sha256("sha512-AAAA").is_err());
    assert!(nix32_sha256("sha256-AAAA").is_err());
}

#[test]
fn one_query_answers_closure_and_metadata_for_every_package() {
    let _serial = serial();
    let directory = tempfile::tempdir().unwrap();
    let metadata = serde_json::json!({
        A: info(&[A, LIB]),
        B: info(&[LIBC]),
        LIB: info(&[LIBC]),
        LIBC: info(&[LIBC]),
    });
    let (nix, log) = stand_in(directory.path(), &metadata.to_string());

    let snapshot = StoreSnapshot::query(&nix, &[Path::new(A), Path::new(B)]).unwrap();

    let calls = fs::read_to_string(&log).unwrap();
    assert_eq!(calls.lines().count(), 1, "{calls}");
    assert!(calls.contains("path-info --json --recursive"), "{calls}");
    assert!(calls.contains(A) && calls.contains(B), "{calls}");
    assert_eq!(
        snapshot.closure(Path::new(A)).unwrap(),
        BTreeSet::from([A, LIB, LIBC].map(PathBuf::from))
    );
    assert_eq!(
        snapshot.closure(Path::new(B)).unwrap(),
        BTreeSet::from([B, LIBC].map(PathBuf::from))
    );
    assert_eq!(
        snapshot.fingerprint(Path::new(B)).unwrap(),
        format!("1;{B};sha256:0mdqa9w1p6cmli6976v4wi0sw9r4p5prkj7lzfd1877wk11c9c73;8;{LIBC}")
    );
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
}

#[test]
fn a_reference_missing_from_the_metadata_is_refused() {
    let _serial = serial();
    let directory = tempfile::tempdir().unwrap();
    let metadata = serde_json::json!({ A: info(&[A, LIB]) });
    let (nix, _) = stand_in(directory.path(), &metadata.to_string());

    let snapshot = StoreSnapshot::query(&nix, &[Path::new(A)]).unwrap();

    assert!(snapshot.closure(Path::new(A)).is_err());
    assert!(snapshot.closure(Path::new(B)).is_err());
}

/// A store path for the `index`th generated dependency. Its hash part uses
/// Nix's base-32 alphabet, so it passes store-path validation.
fn dependency(index: usize) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdfghijklmnpqrsvwxyz";
    let mut hash = [b'0'; 32];
    let mut rest = index;
    for slot in hash.iter_mut().rev() {
        *slot = ALPHABET[rest % 32];
        rest /= 32;
    }
    format!(
        "/nix/store/{}-dependency-{index}",
        std::str::from_utf8(&hash).unwrap()
    )
}

#[test]
fn closure_metadata_larger_than_one_mebibyte_is_accepted() {
    let _serial = serial();
    // About 700 bytes per path made the former 1 MiB helper cap a hidden
    // limit near 1,500 paths. Plugins have no size limit.
    let directory = tempfile::tempdir().unwrap();
    let dependencies: Vec<String> = (0..3000).map(dependency).collect();
    let mut root_references: Vec<&str> = dependencies.iter().map(String::as_str).collect();
    root_references.extend([A, LIBC]);
    let mut metadata = serde_json::Map::new();
    metadata.insert(A.into(), info(&root_references));
    metadata.insert(LIBC.into(), info(&[LIBC]));
    for path in &dependencies {
        metadata.insert(path.clone(), info(&[path.as_str(), LIBC]));
    }
    let metadata = serde_json::Value::Object(metadata).to_string();
    assert!(metadata.len() > 1024 * 1024, "{} bytes", metadata.len());
    let (nix, _) = stand_in(directory.path(), &metadata);

    let snapshot = StoreSnapshot::query(&nix, &[Path::new(A)]).unwrap();

    let closure = snapshot.closure(Path::new(A)).unwrap();
    assert_eq!(closure.len(), dependencies.len() + 2);
    assert!(closure.contains(Path::new(&dependencies[2999])));
}
