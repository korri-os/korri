use korri_plugin_host::{native_unit::NativeUnit, package};
use std::collections::BTreeMap;

const SUNSHINE: &str = include_str!("fixtures/streaming-host/sunshine.service");
const CONTROL: &str = include_str!("fixtures/streaming-host/korri-certificate-control.socket");
const SEAT: &str = include_str!("fixtures/streaming-host/korri-input-seat-receiver.service");

#[test]
fn current_generated_streaming_host_units_are_admitted_without_translation() {
    let sunshine = NativeUnit::parse(SUNSHINE).unwrap();
    let control = NativeUnit::parse(CONTROL).unwrap();
    let seat = NativeUnit::parse(SEAT).unwrap();

    assert_eq!(sunshine.source, SUNSHINE);
    assert_eq!(control.source, CONTROL);
    assert_eq!(seat.source, SEAT);
    assert!(sunshine
        .executables
        .iter()
        .any(|path| path == "/run/wrappers/bin/sunshine"));
    assert!(sunshine
        .executables
        .iter()
        .any(|path| path.ends_with("-korri-wait-for-compositor")));
    assert!(sunshine
        .privileged_directives
        .iter()
        .any(|directive| directive
            .contains("mpp_drm_dev=/dev/dri/by-path/platform-display-subsystem-card")));
    assert!(seat
        .privileged_directives
        .iter()
        .any(|directive| directive == "RuntimeDirectory=korri-input-seat"));
    assert_eq!(seat.capabilities, ["CAP_CHOWN"]);
    assert_eq!(seat.devices, ["/dev/uinput rw"]);

    let warning = package::authority_warning(&BTreeMap::from([
        ("sunshine.service".into(), sunshine),
        ("korri-certificate-control.socket".into(), control),
        ("korri-input-seat-receiver.service".into(), seat),
    ]));
    for named in [
        "sunshine.service",
        "korri-certificate-control.socket",
        "korri-input-seat-receiver.service",
        "CAP_SETPCAP",
        "CAP_SYS_ADMIN",
        "CAP_CHOWN",
        "/dev/uinput rw",
        "NoNewPrivileges=false",
        "ExecStart=/run/wrappers/bin/sunshine",
        "ExecStartPre=+/nix/store/",
        "mpp_drm_dev=/dev/dri/by-path/platform-display-subsystem-card",
        "all host devices (PrivateDevices=false)",
        "RuntimeDirectory=korri-input-seat",
        "RestrictAddressFamilies=AF_UNIX",
        "ListenSequentialPacket=/run/korri-certificate-control/control.sock",
    ] {
        assert!(warning.contains(named), "{named}: {warning}");
    }
}
