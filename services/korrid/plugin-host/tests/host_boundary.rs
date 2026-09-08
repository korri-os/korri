use korri_plugin_host::{package, process, storage};
use std::{
    ffi::CString,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};

#[test]
fn imports_accept_only_exact_output_paths() {
    package::validate_store_path(Path::new(
        "/nix/store/00000000000000000000000000000000-package",
    ))
    .unwrap();
    for path in [
        "github:example/plugin",
        "/tmp/plugin",
        "/nix/store/not-a-hash",
        "/nix/store/00000000000000000000000000000000-package.drv",
        "/nix/store/00000000000000000000000000000000-package/bin/tool",
    ] {
        assert!(
            package::validate_store_path(Path::new(path)).is_err(),
            "{path}"
        );
    }
}

#[test]
fn receipts_are_private_regular_files_and_never_follow_links() {
    let root = tempfile::tempdir().unwrap();
    let receipt = root.path().join("selection.json");
    storage::write_json(&receipt, &vec!["committed"]).unwrap();
    assert_eq!(
        storage::read_json::<Vec<String>>(&receipt)
            .unwrap()
            .unwrap(),
        ["committed"]
    );
    fs::set_permissions(&receipt, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(storage::read_json::<Vec<String>>(&receipt).is_err());
    fs::set_permissions(&receipt, fs::Permissions::from_mode(0o600)).unwrap();
    let link = root.path().join("link");
    symlink(&receipt, &link).unwrap();
    assert!(storage::read_json::<Vec<String>>(&link).is_err());
    let pipe = root.path().join("pipe");
    let name = CString::new(pipe.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(storage::read_json::<Vec<String>>(&pipe).is_err());
}

#[test]
fn one_mutation_owns_the_lock_and_interrupted_writes_do_not_replace_committed_data() {
    let root = tempfile::tempdir().unwrap();
    let lock = root.path().join("lock");
    let held = storage::lock(&lock).unwrap();
    assert!(storage::lock(&lock).is_err());
    drop(held);
    storage::lock(&lock).unwrap();
    let receipt = root.path().join("selection.json");
    storage::write_json(&receipt, &"old").unwrap();
    fs::write(receipt.with_extension("new"), "interrupted").unwrap();
    assert_eq!(
        storage::read_json::<String>(&receipt).unwrap().unwrap(),
        "old"
    );
    storage::write_json(&receipt, &"new").unwrap();
    assert_eq!(
        storage::read_json::<String>(&receipt).unwrap().unwrap(),
        "new"
    );
}

#[test]
fn helper_execution_has_time_output_and_exit_boundaries() {
    let shell = std::env::var("SHELL").expect("test runner supplies its shell");
    let shell = Path::new(&shell);
    let start = Instant::now();
    assert!(
        process::run(shell, ["-c", "sleep 5"], Duration::from_millis(30))
            .unwrap_err()
            .contains("deadline")
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    let output = process::run(
        shell,
        ["-c", "printf result; exit 7"],
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(output.stdout, "result");
    assert!(!output.success);
    assert!(process::run(
        shell,
        ["-c", "while :; do printf '%01000d' 0; done"],
        Duration::from_secs(10)
    )
    .unwrap_err()
    .contains("exceeded 1 MiB"));
}
