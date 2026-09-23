use korri_plugin_host::{
    native_unit::{managed_unit_name, NativeUnit, NativeUnitKind},
    package,
};
use std::collections::BTreeMap;

const SUNSHINE: &str = include_str!("fixtures/streaming-host/korri-sunshine.service");
const CONTROL: &str =
    include_str!("fixtures/streaming-host/korri-sunshine-certificate-control.socket");
const SEAT: &str =
    include_str!("fixtures/streaming-host/korri-sunshine-input-seat-receiver.service");

#[test]
fn approved_three_unit_streaming_shape_is_admitted_without_translation() {
    let sunshine = NativeUnit::parse(SUNSHINE).unwrap();
    let control = NativeUnit::parse(CONTROL).unwrap();
    let seat = NativeUnit::parse(SEAT).unwrap();

    assert_eq!(sunshine.source, SUNSHINE);
    assert_eq!(control.source, CONTROL);
    assert_eq!(seat.source, SEAT);
    assert_eq!(sunshine.kind, NativeUnitKind::Service);
    assert_eq!(control.kind, NativeUnitKind::Socket);
    assert_eq!(seat.kind, NativeUnitKind::Service);
    assert_eq!(
        managed_unit_name("korri-sunshine", sunshine.kind).unwrap(),
        "korri-sunshine.service"
    );
    assert_eq!(
        managed_unit_name("korri-sunshine-certificate-control.socket", control.kind).unwrap(),
        "korri-sunshine-certificate-control.socket"
    );
    assert!(managed_unit_name("sunshine.socket", sunshine.kind).is_err());
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
        ("korri-sunshine.service".into(), sunshine),
        ("korri-sunshine-certificate-control.socket".into(), control),
        ("korri-sunshine-input-seat-receiver.service".into(), seat),
    ]));
    for named in [
        "korri-sunshine.service",
        "korri-sunshine-certificate-control.socket",
        "korri-sunshine-input-seat-receiver.service",
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
