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
