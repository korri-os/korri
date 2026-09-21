use korri_plugin_host::native_unit::{DevicePolicy, NativeUnit};

const UNIT: &str = "[Unit]\nDescription=Network daemon\n[Service]\nType=notify\nExecStart=/nix/store/00000000000000000000000000000000-daemon/bin/run --state=${STATE_DIRECTORY}/state\nCapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW\nDeviceAllow=/dev/net/tun rw\n";

#[test]
fn native_systemd_grammar_preserves_requests_and_resets() {
    let source = UNIT.replace(
        "CAP_NET_ADMIN CAP_NET_RAW",
        "CAP_NET_ADMIN \\\n# continued comment\n CAP_NET_RAW",
    );
    let unit = NativeUnit::parse(&source).unwrap();
    assert_eq!(unit.capabilities, ["CAP_NET_ADMIN", "CAP_NET_RAW"]);
    assert_eq!(unit.devices, ["/dev/net/tun rw"]);
    let reset = NativeUnit::parse(&format!(
        "{UNIT}CapabilityBoundingSet=\nCapabilityBoundingSet=CAP_NET_RAW\nDeviceAllow=\n"
    ))
    .unwrap();
    assert_eq!(reset.capabilities, ["CAP_NET_RAW"]);
    assert!(reset.devices.is_empty());
}

#[test]
fn exact_address_family_and_runtime_directory_requests_are_preserved() {
    let source = format!(
        "{UNIT}RestrictAddressFamilies=AF_UNIX\nRuntimeDirectory=korri-input-seat\nRuntimeDirectoryMode=0711\n"
    );
    let unit = NativeUnit::parse(&source).unwrap();
    assert_eq!(unit.address_families.as_ref().unwrap(), &["AF_UNIX"]);
    assert_eq!(unit.runtime_directory.as_deref(), Some("korri-input-seat"));
    assert_eq!(unit.runtime_directory_mode.as_deref(), Some("0711"));
    for authority in [
        "RestrictAddressFamilies=AF_UNIX",
        "RuntimeDirectory=korri-input-seat",
        "RuntimeDirectoryMode=0711",
    ] {
        assert!(
            unit.privileged_directives
                .iter()
                .any(|directive| directive == authority),
            "{authority}: {:?}",
            unit.privileged_directives
        );
    }
}

#[test]
fn unknown_address_families_and_runtime_directories_fail_by_name() {
    for (request, name) in [
        ("RestrictAddressFamilies=AF_PACKET", "AF_PACKET"),
        ("RuntimeDirectory=unapproved-runtime", "unapproved-runtime"),
        ("RuntimeDirectoryMode=0711", "RuntimeDirectoryMode"),
    ] {
        let error = NativeUnit::parse(&format!("{UNIT}{request}\n")).unwrap_err();
        assert!(error.contains(name), "{name}: {error}");
    }
}

#[test]
fn explicit_closed_device_policy_survives_private_devices_false() {
    let source = "[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-daemon/bin/run\n";
    let unit = NativeUnit::parse(&format!(
        "{source}DevicePolicy=closed\nPrivateDevices=false\n"
    ))
    .unwrap();
    assert_eq!(unit.device_policy, Some(DevicePolicy::Closed));
    assert_eq!(unit.effective_device_policy(), DevicePolicy::Closed);
    assert!(unit.devices.is_empty());
    for authority in ["DevicePolicy=closed", "PrivateDevices=false"] {
        assert!(
            unit.privileged_directives
                .iter()
                .any(|directive| directive == authority),
            "{authority}: {:?}",
            unit.privileged_directives
        );
    }
    let inferred = NativeUnit::parse(&format!("{source}PrivateDevices=false\n")).unwrap();
    assert_eq!(inferred.device_policy, None);
    assert_eq!(inferred.effective_device_policy(), DevicePolicy::Auto);
}

#[test]
fn streaming_authority_is_preserved_as_an_explicit_approval_request() {
    let source = "[Unit]\nDescription=Streaming host\n[Service]\nType=simple\nExecStart=/nix/store/00000000000000000000000000000000-daemon/bin/run\nNoNewPrivileges=false\nPrivatePIDs=false\nPrivateDevices=false\nCapabilityBoundingSet=CAP_SETPCAP CAP_SYS_ADMIN\nDeviceAllow=/dev/dri/card0 rw\nDeviceAllow=/dev/dri/renderD128 rw\nDeviceAllow=/dev/uinput rw\nDeviceAllow=/dev/uhid rw\n";
    let unit = NativeUnit::parse(source).unwrap();
    assert_eq!(unit.capabilities, ["CAP_SETPCAP", "CAP_SYS_ADMIN"]);
    assert_eq!(
        unit.devices,
        [
            "/dev/dri/card0 rw",
            "/dev/dri/renderD128 rw",
            "/dev/uinput rw",
            "/dev/uhid rw"
        ]
    );
    assert_eq!(
        unit.privileged_directives,
        [
            "NoNewPrivileges=false",
            "PrivatePIDs=false",
            "PrivateDevices=false"
        ]
    );
}

#[test]
fn streaming_service_keeps_native_identity_environment_and_host_path_requests() {
    let source = "[Unit]\nDescription=Streaming host\nAfter=control.socket network-online.target\nRequires=control.socket\n[Service]\nType=simple\nUser=korri\nGroup=streaming-seat\nSupplementaryGroups=video render\nWorkingDirectory=/home/korri\nEnvironment=WAYLAND_DISPLAY=wayland-1 XDG_RUNTIME_DIR=/run/user/1000\nSockets=control.socket\nExecCondition=+/nix/store/00000000000000000000000000000000-daemon/bin/check\nExecStartPre=+/nix/store/00000000000000000000000000000000-daemon/bin/wait\nExecStart=/nix/store/00000000000000000000000000000000-daemon/bin/run\nPrivateTmp=false\nProtectHome=read-only\nReadWritePaths=/home/korri/.config /tmp\nInaccessiblePaths=/run/korri-control\n";
    let unit = NativeUnit::parse(source).unwrap();
    assert_eq!(unit.user.as_deref(), Some("korri"));
    assert_eq!(unit.executables.len(), 3);
    for authority in [
        "User=korri",
        "Group=streaming-seat",
        "SupplementaryGroups=video render",
        "Environment=WAYLAND_DISPLAY=wayland-1 XDG_RUNTIME_DIR=/run/user/1000",
        "ExecCondition=+/nix/store/00000000000000000000000000000000-daemon/bin/check",
        "PrivateTmp=false",
        "ProtectHome=read-only",
        "ReadWritePaths=/home/korri/.config /tmp",
        "InaccessiblePaths=/run/korri-control",
    ] {
        assert!(
            unit.privileged_directives
                .iter()
                .any(|request| request == authority),
            "{authority:?} missing from {:?}",
            unit.privileged_directives
        );
    }
}

#[test]
fn native_socket_authority_is_named_without_translating_its_configuration() {
    let source = "[Unit]\nDescription=Certificate control\nBefore=streaming.service\n[Socket]\nAccept=false\nListenSequentialPacket=/run/streaming/control.sock\nFileDescriptorName=certificate-control\nSocketUser=root\nSocketGroup=korri\nSocketMode=0660\nDirectoryMode=0751\nRemoveOnStop=true\nNonBlocking=true\nService=streaming.service\n";
    let unit = NativeUnit::parse(source).unwrap();
    assert_eq!(unit.source, source);
    assert!(unit.executables.is_empty());
    for authority in [
        "Before=streaming.service",
        "ListenSequentialPacket=/run/streaming/control.sock",
        "SocketUser=root",
        "SocketGroup=korri",
        "Service=streaming.service",
    ] {
        assert!(
            unit.privileged_directives
                .iter()
                .any(|request| request == authority),
            "{authority:?} missing from {:?}",
            unit.privileged_directives
        );
    }
}

#[test]
fn unknown_directives_and_systemd_execution_escape_hatches_fail_by_name() {
    for directive in [
        "User=Bad User",
        "ExecStartPre=/bin/true",
        "EnvironmentFile=/etc/environment",
        "AmbientCapabilities=CAP_SYS_ADMIN",
        "ProtectSystem=no",
        "BindPaths=/",
        "DeviceAllow=/dev/sda rw",
        "LoadCredential=authkey:/etc/shadow",
        "CapabilityBoundingSet=~CAP_NET_ADMIN",
        "CapabilityBoundingSet=CAP_\\x4eET_ADMIN",
        "DeviceAllow=\\x2fdev/net/tun rw",
        "LoadCredential=\\x61uthkey:tailscale-authkey",
    ] {
        let error = NativeUnit::parse(&format!("{UNIT}{directive}\n")).unwrap_err();
        assert!(
            error.contains(directive.split('=').next().unwrap()),
            "{error}"
        );
    }
    for program in [
        "+/bin/run",
        "!/bin/run",
        "@/bin/run",
        "-/bin/run",
        "/bin/run",
        "/nix/store/00000000000000000000000000000000-daemon/../run",
    ] {
        assert!(
            NativeUnit::parse(&UNIT.replace(
                "/nix/store/00000000000000000000000000000000-daemon/bin/run",
                program
            ))
            .is_err(),
            "{program}"
        );
    }
}

#[test]
fn unnamed_directive_device_and_capability_are_refused_by_exact_name() {
    for (request, name) in [
        ("UnapprovedDirective=value", "UnapprovedDirective"),
        ("DeviceAllow=/dev/sda rw", "/dev/sda"),
        ("CapabilityBoundingSet=CAP_SYS_BOOT", "CAP_SYS_BOOT"),
    ] {
        let error = NativeUnit::parse(&format!("{UNIT}{request}\n")).unwrap_err();
        assert!(error.contains(name), "{name}: {error}");
    }
}

#[test]
fn bare_carriage_returns_cannot_hide_systemd_directives() {
    for prefix in ["# hidden", "; hidden", "Description=Network daemon"] {
        let source = format!("[Unit]\n{prefix}\r[Service]\rUser=root\rExecStartPre=+/nix/store/00000000000000000000000000000000-daemon/bin/run\n{UNIT}");
        let error = NativeUnit::parse(&source).unwrap_err();
        assert!(error.contains("bare carriage return"), "{error}");
    }
    for source in [
        format!("{UNIT}# trailing\r"),
        UNIT.replace("Type=notify", "Type=\rnotify"),
    ] {
        assert!(NativeUnit::parse(&source).is_err(), "{source:?}");
    }
}

#[test]
fn unicode_whitespace_cannot_reset_or_hide_native_authority() {
    for whitespace in [
        '\u{00a0}', '\u{1680}', '\u{2003}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{3000}',
    ] {
        for directive in [
            format!("User{whitespace}="),
            format!("{whitespace}User="),
            format!("User={whitespace}"),
            format!("# comment{whitespace}User="),
        ] {
            let source = format!("{UNIT}User=root\n{directive}\n");
            assert!(NativeUnit::parse(&source).is_err(), "{source:?}");
        }
    }
}

#[test]
fn crlf_line_endings_preserve_the_approved_source() {
    let source = format!("# comment\n{UNIT}").replace('\n', "\r\n");
    let unit = NativeUnit::parse(&source).unwrap();
    assert_eq!(unit.source, source);
    assert_eq!(unit.capabilities, ["CAP_NET_ADMIN", "CAP_NET_RAW"]);
}

#[test]
fn malformed_and_ambiguous_grammar_fails_closed() {
    for source in [
        "[Service]\nExecStart=\n",
        "[Service]\nExecStart=/bin/run\n",
        "[Socket]\nListenStream=22\n",
        "[Service]\nType=exec\nBroken",
        "[Service]\nType=exec\\",
        "[Service]\nType=exec\0",
        "[Service]\nType=exec\nExecStart=\"unterminated",
    ] {
        assert!(NativeUnit::parse(source).is_err(), "{source:?}");
    }
    for arg in ["%n", "$OTHER", "${HOME}", "\\q", "; /bin/sh"] {
        assert!(NativeUnit::parse(&format!("{UNIT}ExecStopPost=/nix/store/00000000000000000000000000000000-daemon/bin/run {arg}\n")).is_err(), "{arg}");
    }
}

#[test]
fn quotes_escapes_and_native_credentials_are_validated_not_translated() {
    let source = format!("{UNIT}ExecStopPost=\"/nix/store/00000000000000000000000000000000-daemon/bin/run\" \"two words\" \\x61\nLoadCredential=authkey:tailscale-authkey\n");
    let unit = NativeUnit::parse(&source).unwrap();
    assert_eq!(unit.credentials, ["authkey:tailscale-authkey"]);
    assert_eq!(unit.source, source);
}

#[test]
fn native_root_request_and_pre_start_commands_are_explicit_and_resettable() {
    let source = format!("{UNIT}User=root\nExecStartPre=/nix/store/00000000000000000000000000000000-daemon/bin/keygen ${{STATE_DIRECTORY}}\n");
    let unit = NativeUnit::parse(&source).unwrap();
    assert_eq!(unit.user.as_deref(), Some("root"));
    assert_eq!(unit.executables.len(), 2);
    assert_eq!(unit.source, source);
    let reset = NativeUnit::parse(&format!("{source}User=\nExecStartPre=\n")).unwrap();
    assert_eq!(reset.user, None);
    assert_eq!(reset.executables.len(), 1);
    assert_eq!(NativeUnit::parse(UNIT).unwrap().user, None);
    assert!(NativeUnit::parse(&format!(
        "{UNIT}ExecStartPre=+/nix/store/00000000000000000000000000000000-daemon/bin/run\n"
    ))
    .is_ok());
    let quoted = NativeUnit::parse(&format!(
        "{UNIT}ExecStartPre=\"+/nix/store/00000000000000000000000000000000-daemon/bin/run\"\n"
    ))
    .unwrap();
    assert!(quoted
        .privileged_directives
        .iter()
        .any(|directive| directive.starts_with("ExecStartPre=")));
    for value in ["!", "-", "@"] {
        assert!(NativeUnit::parse(&format!("{UNIT}ExecStartPre={value}/nix/store/00000000000000000000000000000000-daemon/bin/run\n")).is_err());
    }
}
