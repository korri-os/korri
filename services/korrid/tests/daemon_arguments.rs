use std::process::Command;

#[test]
fn unknown_help_and_flags_exit_usage_without_state_or_listeners() {
    let root = tempfile::tempdir().unwrap();
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    for argument in ["unknown", "help", "--help", "-h", "--version", "--address"] {
        let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
            .arg(argument)
            .env_clear()
            .env("HOME", root.path())
            .env("KORRID_PRIVATE_STATE_ROOT", root.path().join("private"))
            .env("KORRID_ADDRESS", reserved.local_addr().unwrap().to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{argument}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage: korrid"));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn removed_catalog_commands_exit_usage_without_state_or_listeners() {
    let root = tempfile::tempdir().unwrap();
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    for args in [
        vec!["catalog", "pair", "/games", "gba"],
        vec!["catalog", "unpair", "gba"],
        vec!["catalog", "rescan"],
        vec!["catalog", "--help"],
        vec!["identity", "show"],
        vec!["identity", "switch"],
        vec!["identity", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
            .args(&args)
            .env_clear()
            .env("HOME", root.path())
            .env("KORRID_STORAGE_ROOT", root.path().join("readable"))
            .env("KORRID_PRIVATE_STATE_ROOT", root.path().join("private"))
            .env("KORRID_ADDRESS", reserved.local_addr().unwrap().to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn existing_internal_helpers_still_dispatch_before_daemon_startup() {
    for (argument, expected) in [
        ("plugin-run", 1),
        ("plugin-launch", 1),
        ("unpack-archive", 1),
        ("catalog", 2),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
            .arg(argument)
            .env_clear()
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected), "{argument}");
        assert!(!String::from_utf8_lossy(&output.stderr)
            .contains("KORRID_LOCAL_SIGNER_SOCKET must be set"));
    }
}
