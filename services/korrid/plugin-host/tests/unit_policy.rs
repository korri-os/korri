use korri_plugin_host::{
    declaration::Declaration, native_unit::NativeUnit, package, provenance::Provenance, unit,
};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path};

fn report(source: &str) -> package::Report {
    package::Report {
        id: "@example:not-ssh".into(),
        package: "/nix/store/00000000000000000000000000000000-service".into(),
        provenance: Provenance::RawCache {
            cache_url: "file:///cache".into(),
        },
        approval: String::new(),
        policy: package::BASE_POLICY,
        warning: String::new(),
        unit: Some(String::new()),
        state_directory: String::new(),
        runtime_directory: String::new(),
        unit_configuration: String::new(),
        declaration: Declaration::evaluate(
            "@example",
            "export const name = 'not-ssh'; export const services = ['daemon'];",
        )
        .unwrap(),
        native_units: BTreeMap::from([("daemon".into(), NativeUnit::parse(source).unwrap())]),
        ports: Default::default(),
        packages: Default::default(),
        files: Default::default(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
    }
}

#[test]
#[ignore = "requires actual pinned systemd-analyze; no service is started"]
fn pinned_systemd_preserves_approved_root_and_applies_unprivileged_reset() {
    let analyze = std::env::var("KORRI_TEST_SYSTEMD_ANALYZE").expect("pinned systemd-analyze");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authority.service");
    let source = format!("[Service]\nType=exec\nExecStart={analyze} --version\n");
    let verify = |text: &str| {
        std::fs::write(&path, text).unwrap();
        let result = std::process::Command::new(&analyze)
            .env("SYSTEMD_LOG_LEVEL", "debug")
            .args(["verify", path.to_str().unwrap()])
            .output()
            .unwrap();
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.status.success(), "{output}");
        output
    };
    // The actual systemd parser ignores the NBSP assignment, retaining root.
    let ambiguous = format!("{source}User=root\nUser\u{00a0}=\nDynamicUser=yes\n");
    let output = verify(&ambiguous);
    assert!(output.contains("User: root\n"), "{output}");
    assert!(output.contains("DynamicUser: yes\n"), "{output}");
    assert!(NativeUnit::parse(&ambiguous).is_err());
    // Defense in depth: the host drop-in must clear even a retained root.
    let output = verify(&format!(
        "{ambiguous}\n{}",
        unit::hardening(&report(&source))
    ));
    assert!(!output.contains("User: root\n"), "{output}");
    assert!(output.contains("DynamicUser: yes\n"), "{output}");
    let output = verify(&unit::render(&report(&format!("{source}User=root\n"))).unwrap());
    assert!(output.contains("User: root\n"), "{output}");
    assert!(output.contains("DynamicUser: no\n"), "{output}");

    let explicit = report(&format!(
        "{source}RestrictAddressFamilies=AF_UNIX\nRuntimeDirectory=korri-input-seat\nRuntimeDirectoryMode=0711\n"
    ));
    let output = verify(&unit::render(&explicit).unwrap());
    assert!(
        output.contains("RuntimeDirectory: korri-input-seat\n"),
        "{output}"
    );
    assert!(output.contains("RuntimeDirectoryMode: 0711\n"), "{output}");
    let security = std::process::Command::new(&analyze)
        .args([
            "security",
            "--offline=yes",
            "--json=short",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        security.status.success(),
        "{}{}",
        String::from_utf8_lossy(&security.stdout),
        String::from_utf8_lossy(&security.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&security.stdout).unwrap();
    let restricted = |field: &str| {
        rows.iter()
            .find(|row| row["json_field"] == field)
            .and_then(|row| row["set"].as_bool())
    };
    assert_eq!(restricted("RestrictAddressFamilies_AF_UNIX"), Some(false));
    assert_eq!(
        restricted("RestrictAddressFamilies_AF_INET_INET6"),
        Some(true)
    );
    assert_eq!(restricted("RestrictAddressFamilies_AF_NETLINK"), Some(true));
    assert_eq!(restricted("RestrictAddressFamilies_AF_PACKET"), Some(true));

    let closed = report(&format!(
        "{source}DevicePolicy=closed\nPrivateDevices=false\n"
    ));
    let output = verify(&unit::render(&closed).unwrap());
    assert!(output.contains("PrivateDevices: no\n"), "{output}");
    assert!(output.contains("DevicePolicy: closed\n"), "{output}");
}

#[test]
fn root_authority_requires_native_request_not_identity_and_default_isolation_is_unchanged() {
    let source = "[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\n";
    let mut ordinary = report(source);
    ordinary.id = "@korri:ssh".into();
    let isolated = unit::hardening(&ordinary);
    for directive in [
        "User=\nDynamicUser=yes",
        "NoNewPrivileges=yes",
        "ProtectSystem=strict",
        "ProtectHome=yes",
        "DevicePolicy=closed",
        "CapabilityBoundingSet=\nCapabilityBoundingSet=\n",
    ] {
        assert!(isolated.contains(directive), "{directive}");
    }
    let privileged = report(&format!("{source}User=root\n"));
    let policy = unit::hardening(&privileged);
    assert!(policy.contains(package::ROOT_POLICY));
    assert!(policy.contains("User=root\nDynamicUser=no\n"));
    assert!(policy.contains("StateDirectoryMode=0700"));
    assert!(policy.contains("RuntimeDirectoryMode=0700"));
    assert!(policy.contains("KillMode=control-group"));
    for directive in [
        "NoNewPrivileges",
        "ProtectHome",
        "ProtectSystem",
        "DevicePolicy",
        "CapabilityBoundingSet",
    ] {
        assert!(!policy.contains(directive), "{directive}");
    }
    assert_eq!(
        unit::render(&privileged).unwrap(),
        format!(
            "# native unit: daemon\n{}\n{}",
            privileged.native_units["daemon"].source, policy
        )
    );
}

#[test]
fn explicit_address_family_request_is_not_widened_by_the_host_policy() {
    let explicit = report("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\nRestrictAddressFamilies=AF_UNIX\n");
    let policy = unit::hardening(&explicit);
    assert!(policy.contains("RestrictAddressFamilies=\nRestrictAddressFamilies=AF_UNIX\n"));
    assert!(!policy.contains("AF_INET"), "{policy}");
    assert!(!policy.contains("AF_INET6"), "{policy}");
    assert!(!policy.contains("AF_NETLINK"), "{policy}");

    let default = unit::hardening(&report("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\n"));
    assert!(default.contains(
        "RestrictAddressFamilies=\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK\n"
    ));
}

#[test]
fn real_seat_receiver_runtime_directory_and_mode_survive_the_host_policy() {
    let seat = report(include_str!(
        "fixtures/streaming-host/korri-sunshine-input-seat-receiver.service"
    ));
    let policy = unit::hardening(&seat);
    assert!(policy.contains(
        "RuntimeDirectory=\nRuntimeDirectory=korri-input-seat\nRuntimeDirectoryMode=0711\n"
    ));
    assert!(!policy.contains(&format!(
        "RuntimeDirectory={}\n",
        package::unit_name(&seat.id)
    )));
    let effective = unit::render(&seat).unwrap();
    assert!(effective.contains("--runtime-dir /run/korri-input-seat"));
    assert!(effective.contains("RuntimeDirectory=korri-input-seat"));
}

#[test]
fn explicit_closed_device_policy_is_not_widened_by_private_devices_false() {
    let closed = report("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\nDevicePolicy=closed\nPrivateDevices=false\n");
    let policy = unit::hardening(&closed);
    assert!(policy.contains("DevicePolicy=closed\n"), "{policy}");
    assert!(!policy.contains("DevicePolicy=auto"), "{policy}");
}

#[test]
fn current_kms_streaming_request_widens_device_policy_only_for_that_unit() {
    let streaming = report(include_str!("fixtures/streaming-host/korri-sunshine.service"));
    let policy = unit::hardening(&streaming);
    assert!(policy.contains("DevicePolicy=auto"));
    assert!(policy.contains("NoNewPrivileges=no"));
    assert!(policy.contains("CapabilityBoundingSet=CAP_SETPCAP CAP_SYS_ADMIN"));
}

#[test]
fn one_plugins_native_request_grants_no_authority_to_another_plugin() {
    let privileged = report("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\nCapabilityBoundingSet=CAP_SYS_ADMIN\nDeviceAllow=/dev/dri/card0 rw\nNoNewPrivileges=false\n");
    let mut ordinary = report("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\n");
    ordinary.id = "@other:ordinary".into();

    let privileged_policy = unit::hardening(&privileged);
    assert!(privileged_policy.contains("CAP_SYS_ADMIN"));
    assert!(privileged_policy.contains("/dev/dri/card0 rw"));
    let ordinary_policy = unit::hardening(&ordinary);
    assert!(!ordinary_policy.contains("CAP_SYS_ADMIN"));
    assert!(!ordinary_policy.contains("/dev/dri/card0 rw"));
    assert!(ordinary_policy.contains("NoNewPrivileges=yes"));
}

fn write_executable(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn assert_inactive_purge(native_unit_count: usize) {
    let temporary = tempfile::tempdir().unwrap();
    let unit_directory = temporary.path().join("systemd");
    let state_directory = temporary.path().join("state");
    let log = temporary.path().join("systemctl.log");
    fs::create_dir(&unit_directory).unwrap();
    fs::create_dir(&state_directory).unwrap();
    fs::write(state_directory.join("retained-data"), "retained").unwrap();

    let systemctl = temporary.path().join("systemctl");
    write_executable(
        &systemctl,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = clean ]; then rm -rf '{}'; fi\n",
            log.display(),
            state_directory.display()
        ),
    );
    let firewall = temporary.path().join("iptables");
    write_executable(&firewall, "#!/bin/sh\nexit 0\n");

    let mut report = report("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\n");
    report.unit = None;
    report.state_directory = state_directory.display().to_string();
    if native_unit_count == 0 {
        report.native_units.clear();
    } else {
        report.native_units.insert(
            "other".into(),
            NativeUnit::parse("[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/other\n").unwrap(),
        );
    }
    assert_eq!(report.native_units.len(), native_unit_count);

    let units = unit::Units {
        systemctl,
        unit_directory,
        ownership_directory: temporary.path().join("unit-ownership"),
        firewall: korri_plugin_host::firewall::Firewall {
            ipv4: firewall.clone(),
            ipv6: firewall,
        },
    };
    let cleanup_unit = unit::purge_unit_name(&report);
    units.purge_inactive(&report).unwrap();

    let commands = fs::read_to_string(log).unwrap();
    assert!(
        commands
            .lines()
            .any(|line| line == format!("clean --what=state {cleanup_unit}")),
        "{commands}"
    );
    assert_eq!(
        commands
            .lines()
            .filter(|line| *line == "daemon-reload")
            .count(),
        2,
        "{commands}"
    );
    assert!(!state_directory.exists());
    assert!(!units.path(&report.id).exists());
}

#[test]
fn inactive_purge_executes_cleanup_for_zero_and_multiple_native_units() {
    assert_inactive_purge(0);
    assert_inactive_purge(2);
}

#[test]
fn effective_configuration_names_every_unit_and_its_widened_authority() {
    let mut report = report("[Service]\nType=exec\nUser=korri\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\nCapabilityBoundingSet=CAP_SYS_ADMIN\nDeviceAllow=/dev/dri/card0 rw\nNoNewPrivileges=false\nPrivateTmp=false\nProtectHome=read-only\n");
    report.declaration = Declaration::evaluate(
        "@example",
        "export const name = 'streaming'; export const services = ['streaming', 'control.socket'];",
    )
    .unwrap();
    report.native_units.insert(
        "control.socket".into(),
        NativeUnit::parse("[Socket]\nAccept=false\nListenSequentialPacket=/run/streaming/control.sock\nFileDescriptorName=certificate-control\nSocketUser=root\nSocketGroup=korri\nSocketMode=0660\nDirectoryMode=0751\nRemoveOnStop=true\nNonBlocking=true\nService=streaming.service\n")
            .unwrap(),
    );

    let effective = unit::render(&report).unwrap();
    for named in [
        "# native unit: control.socket",
        "# native unit: daemon",
        "CAP_SYS_ADMIN",
        "/dev/dri/card0 rw",
        "NoNewPrivileges=false",
        "User=korri\nDynamicUser=no",
        "PrivateTmp=no",
        "ProtectHome=read-only",
        "ListenSequentialPacket=/run/streaming/control.sock",
        package::BASE_POLICY,
    ] {
        assert!(effective.contains(named), "{named}");
    }
}
