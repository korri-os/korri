//! The migration guard checks captured documents, never a mutable review root.
use std::{fs, process::Command};

#[test]
fn check_mode_reads_valid_and_invalid_documents_without_changing_files() {
    let root = tempfile::tempdir().unwrap();
    let device = root.path().join("device.yaml");
    let library = root.path().join("library.yaml");
    fs::write(
        &library,
        include_str!("../deploy/launch-settings-migration/fixtures/library.after.yaml"),
    )
    .unwrap();
    for (source, valid) in [
        (
            include_str!("../deploy/launch-settings-migration/fixtures/device.after.yaml")
                .to_string(),
            true,
        ),
        (
            "host: {preferences: {player-count: four}}\n".to_string(),
            false,
        ),
        ("host: {moonlight: {}}\n".to_string(), false),
    ] {
        fs::write(&device, source.as_bytes()).unwrap();
        let before_device = fs::read(&device).unwrap();
        let before_library = fs::read(&library).unwrap();
        let device_time = fs::metadata(&device).unwrap().modified().unwrap();
        let library_time = fs::metadata(&library).unwrap().modified().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_config_snapshot_probe"))
            .args(["--check"])
            .arg(&device)
            .arg(&library)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            valid,
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if valid {
            assert!(String::from_utf8_lossy(&output.stdout).contains("CONFIG VALID"));
        } else {
            assert!(String::from_utf8_lossy(&output.stderr).contains("CONFIG SNAPSHOT FAILED"));
        }
        assert_eq!(fs::read(&device).unwrap(), before_device);
        assert_eq!(fs::read(&library).unwrap(), before_library);
        assert_eq!(
            fs::metadata(&device).unwrap().modified().unwrap(),
            device_time
        );
        assert_eq!(
            fs::metadata(&library).unwrap().modified().unwrap(),
            library_time
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}

#[test]
fn check_mode_refuses_missing_files_and_missing_arguments_without_initialization() {
    let root = tempfile::tempdir().unwrap();
    for arguments in [
        vec!["--check".into()],
        vec![
            "--check".into(),
            root.path().join("device.yaml").into_os_string(),
            root.path().join("library.yaml").into_os_string(),
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_config_snapshot_probe"))
            .args(arguments)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
