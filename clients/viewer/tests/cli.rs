use std::process::Command;

#[test]
fn missing_pin_fails_before_connecting() {
    let keys = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .env_remove("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .args([
            "stream",
            "-app",
            "Korri Stream",
            "-platform",
            "x11_vaapi",
            "-mapping",
            "/unused/mapping",
            "-keydir",
            keys.path().to_str().unwrap(),
            "-windowed",
            "-autowindowresize",
            "-viewonly",
            "-verbose",
            "-width",
            "1920",
            "-height",
            "1080",
            "-fps",
            "60",
            "-codec",
            "h264",
            "-bitrate",
            "10000",
            "-port",
            "9",
            "127.0.0.1",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("KORRI_MOONLIGHT_SERVER_CERTIFICATE"));
}

#[test]
fn unsupported_input_is_not_silently_ignored() {
    let output = Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .args(["stream", "-input", "/dev/input/event0", "127.0.0.1"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("input"));
}

#[test]
fn accepts_rendered_pc_settings_without_connecting_without_a_pin() {
    let output = Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .env_remove("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .args([
            "stream",
            "-app",
            "Korri Stream",
            "-platform",
            "x11_vaapi",
            "-keydir",
            "/unused",
            "-windowed",
            "-autowindowresize",
            "-viewonly",
            "-width",
            "1920",
            "-height",
            "1080",
            "-fps",
            "60",
            "-bitrate",
            "10000",
            "-codec",
            "h265",
            "-hdr",
            "-rotate",
            "90",
            "-nosops",
            "-nounsupported",
            "-surround",
            "5.1",
            "-localaudio",
            "-nomouseemulation",
            "-remote",
            "no",
            "-packetsize",
            "1024",
            "-debug",
            "127.0.0.1",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("KORRI_MOONLIGHT_SERVER_CERTIFICATE"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// The binary's licence, as --help names it (ADR 0059). With nightjar-core,
// which is GPL-3.0-only (vendor/README.md), the binary is GPL-3.0-only: the
// PC package builds it (#391). Without it the first line stays
// "korri-viewer 0.1.0 (GPL-3.0-or-later)", which the aarch64 check reads.
#[test]
fn help_names_the_binarys_licence() {
    let output = Command::new(env!("CARGO_BIN_EXE_korri-viewer"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    let first = help.lines().next().unwrap();
    if cfg!(feature = "nightjar") {
        assert_eq!(
            first,
            "korri-viewer 0.1.0 (GPL-3.0-only: GPL-3.0-or-later code with nightjar-core, which is GPL-3.0-only)"
        );
    } else {
        assert_eq!(first, "korri-viewer 0.1.0 (GPL-3.0-or-later)");
    }
}
