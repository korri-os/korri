use korri_plugin_host::{
    declaration::Declaration, native_unit::NativeUnit, package, provenance::Provenance, unit,
};

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
        unit: String::new(),
        state_directory: String::new(),
        runtime_directory: String::new(),
        unit_configuration: String::new(),
        declaration: Declaration::evaluate(
            "@example",
            "export const name = 'not-ssh'; export const services = ['daemon'];",
        )
        .unwrap(),
        native_unit: Some(NativeUnit::parse(source).unwrap()),
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
        format!("{}\n{}", privileged.native_unit.unwrap().source, policy)
    );
}
