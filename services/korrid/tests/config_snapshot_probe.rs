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

#[test]
fn yaml_integer_scalar_domain_is_i64_min_through_u64_max() {
    use korrid::config::decode_config_documents;
    for (literal, accepted) in [
        ("-9223372036854775808", true),
        ("18446744073709551615", true),
        ("-9223372036854775809", false),
        ("18446744073709551616", false),
    ] {
        let source=format!("host:\n  runners:\n    '@korri:example/main':\n      settings: {{large: {literal}}}\n  modifiers:\n    '@korri:example/main':\n      settings: {{large: {literal}}}\nsystems:\n  gba:\n    metadata: {{large: {literal}}}\n");
        let decoded = decode_config_documents(&source, "{}");
        assert_eq!(decoded.is_ok(), accepted, "{literal}: {decoded:?}");
        if let Ok(config) = decoded {
            let expected: serde_json::Value = serde_json::from_str(literal).unwrap();
            let host = config.host.as_ref().unwrap();
            assert_eq!(
                serde_json::to_value(&host.runners["@korri:example/main"].settings["large"])
                    .unwrap(),
                expected
            );
            assert_eq!(
                serde_json::to_value(&host.modifiers["@korri:example/main"].settings["large"])
                    .unwrap(),
                expected
            );
            assert_eq!(
                config.systems["gba"].metadata.as_ref().unwrap()["large"],
                expected
            );
        }
    }
}

#[test]
fn scalar_fixture_preserves_serde_yaml_leading_zero_strings_and_integer_types() {
    use korrid::config::{cascade::LaunchSettingValue, decode_config_documents};
    let before = include_str!("../deploy/launch-settings-migration/fixtures/scalars.before.yaml");
    // Moving the known top-level runner record does not change its scalars.
    let before_at_host = format!(
        "host:\n{}\n",
        before
            .lines()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let original = decode_config_documents(&before_at_host, "{}").unwrap();
    let migrated = decode_config_documents(
        include_str!("../deploy/launch-settings-migration/fixtures/scalars.after.yaml"),
        "{}",
    )
    .unwrap();
    assert_eq!(migrated, original);
    let settings = &migrated.host.as_ref().unwrap().runners["@korri:example/main"].settings;
    for (key, expected) in [
        ("leading-zero", "0755"),
        ("signed-leading-zero", "-0755"),
        ("positive-leading-zero", "+0755"),
        ("separated-digits", "1_000"),
        ("separated-float", "1_0.5"),
    ] {
        assert_eq!(settings[key], LaunchSettingValue::String(expected.into()));
    }
    let signed = serde_json::to_value(&settings["signed-min"]).unwrap();
    let unsigned = serde_json::to_value(&settings["unsigned-max"]).unwrap();
    assert_eq!(signed.as_i64(), Some(i64::MIN));
    assert_eq!(unsigned.as_u64(), Some(u64::MAX));
}
