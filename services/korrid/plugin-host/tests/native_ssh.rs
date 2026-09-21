use korri_plugin_host::{package, provenance::Provenance};
use std::path::PathBuf;

#[test]
#[ignore = "requires built SSH plugin and real build-machine Nix; no VM"]
fn packaged_native_ssh_requests_root_authority_before_any_service_effect() {
    let nix = PathBuf::from(std::env::var_os("KORRI_TEST_NIX").expect("Nix executable"));
    let selected =
        PathBuf::from(std::env::var_os("KORRI_TEST_SSH_PACKAGE").expect("built SSH plugin"));
    let report = package::load(
        &nix,
        &selected,
        Provenance::RawCache {
            cache_url: "https://cache.example.test".into(),
        },
    )
    .unwrap();
    assert_eq!(report.id, "@korri:ssh");
    assert_eq!(report.policy, package::ROOT_POLICY);
    assert!(report.warning.starts_with("NATIVE AUTHORITY REQUEST:"));
    assert!(report.warning.contains("DEVICE-WIDE ROOT AUTHORITY:"));
    assert!(report.warning.contains("User=root"));
    assert!(report
        .warning
        .contains("limited to this exact plugin build"));
    assert_eq!(report.ports.allowed_tcp_ports, [2222]);
    assert!(report.ports.allowed_udp_ports.is_empty());
    let native = &report.native_units["sshd"];
    assert_eq!(native.user.as_deref(), Some("root"));
    assert_eq!(native.executables.len(), 2);
    assert!(native
        .executables
        .contains(&report.files["prepare"].display().to_string()));
    assert!(native
        .executables
        .contains(&report.files["start"].display().to_string()));
    assert!(report.unit_configuration.contains(package::ROOT_POLICY));
    assert!(report.unit_configuration.contains("DynamicUser=no"));
    assert!(!report.unit_configuration.contains("DynamicUser=yes"));
    assert_eq!(report.approval.len(), 64);
    let start = std::fs::read_to_string(&report.files["start"]).unwrap();
    assert!(start.contains("$STATE_DIRECTORY/ssh_host_ed25519_key"));
    assert!(start.contains("$RUNTIME_DIRECTORY/sshd.pid"));
    assert!(start.contains(&format!("-f {}", report.files["config"].display())));
    assert!(!start.contains("-f /etc/ssh/sshd_config"));
    assert!(
        !report.files.contains_key("upstream-report"),
        "build-side audit must not ship upstream boot units"
    );
}
