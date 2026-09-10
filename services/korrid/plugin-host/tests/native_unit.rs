use korri_plugin_host::native_unit::NativeUnit;

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
fn unknown_directives_and_systemd_execution_escape_hatches_fail_by_name() {
    for directive in [
        "User=daemon",
        "ExecStartPre=/bin/true",
        "Environment=LD_PRELOAD=evil",
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
    for value in ["+", "!", "-", "@"] {
        assert!(NativeUnit::parse(&format!("{UNIT}ExecStartPre={value}/nix/store/00000000000000000000000000000000-daemon/bin/run\n")).is_err());
    }
}
