use korri_plugin_host::{native_unit::NativeUnit, package};
use std::collections::BTreeMap;

#[test]
fn approval_report_names_each_units_widened_authority_before_approval() {
    let units = BTreeMap::from([
        (
            "streaming".into(),
            NativeUnit::parse("[Service]\nType=simple\nExecStart=/nix/store/00000000000000000000000000000000-streaming/bin/run\nNoNewPrivileges=false\nRestrictAddressFamilies=AF_UNIX\nRuntimeDirectory=korri-input-seat\nRuntimeDirectoryMode=0711\nCapabilityBoundingSet=CAP_SETPCAP CAP_SYS_ADMIN\nDeviceAllow=/dev/dri/card0 rw\nDeviceAllow=/dev/uinput rw\n").unwrap(),
        ),
        (
            "control.socket".into(),
            NativeUnit::parse("[Socket]\nAccept=false\nListenSequentialPacket=/run/streaming/control.sock\nFileDescriptorName=certificate-control\nSocketUser=root\nSocketGroup=korri\nSocketMode=0660\nDirectoryMode=0751\nRemoveOnStop=true\nNonBlocking=true\nService=streaming.service\n").unwrap(),
        ),
    ]);

    let report = package::authority_warning(&units);
    for named in [
        "control.socket",
        "streaming",
        "NoNewPrivileges=false",
        "RestrictAddressFamilies=AF_UNIX",
        "RuntimeDirectory=korri-input-seat",
        "RuntimeDirectoryMode=0711",
        "ListenSequentialPacket=/run/streaming/control.sock",
        "CAP_SETPCAP",
        "CAP_SYS_ADMIN",
        "/dev/dri/card0 rw",
        "/dev/uinput rw",
    ] {
        assert!(report.contains(named), "{named}: {report}");
    }
}

#[test]
fn approval_report_preserves_explicit_closed_device_policy() {
    let units = BTreeMap::from([(
        "closed".into(),
        NativeUnit::parse("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-closed/bin/run\nDevicePolicy=closed\nPrivateDevices=false\n").unwrap(),
    )]);

    let warning = package::authority_warning(&units);
    assert!(warning.contains("DevicePolicy=closed"), "{warning}");
    assert!(warning.contains("PrivateDevices=false"), "{warning}");
    assert!(warning.contains("devices [none]"), "{warning}");
    assert!(!warning.contains("all host devices"), "{warning}");
}

#[test]
fn approval_report_does_not_copy_authority_to_another_plugins_units() {
    let privileged = BTreeMap::from([(
        "streaming".into(),
        NativeUnit::parse("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-streaming/bin/run\nCapabilityBoundingSet=CAP_SYS_ADMIN\nDeviceAllow=/dev/dri/card0 rw\n").unwrap(),
    )]);
    let ordinary = BTreeMap::from([(
        "ordinary".into(),
        NativeUnit::parse("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-ordinary/bin/run\n").unwrap(),
    )]);

    assert!(package::authority_warning(&privileged).contains("CAP_SYS_ADMIN"));
    let warning = package::authority_warning(&ordinary);
    assert!(!warning.contains("CAP_SYS_ADMIN"));
    assert!(!warning.contains("/dev/dri/card0"));
}
