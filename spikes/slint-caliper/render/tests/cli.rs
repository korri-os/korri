#[test]
fn native_cli_contract() {
    let status = std::process::Command::new("python3")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/smoke.py"))
        .arg(env!("CARGO_BIN_EXE_slint-caliper-render"))
        .status()
        .expect("python3 runs the spike CLI checks");
    assert!(status.success());
}
