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
        requires: vec![],
    }
}

#[test]
fn root_authority_requires_native_request_not_identity_and_default_isolation_is_unchanged() {
    let source = "[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-service/bin/run\n";
    let mut ordinary = report(source);
    ordinary.id = "@korri:ssh".into();
    let isolated = unit::hardening(&ordinary);
    for directive in [
        "DynamicUser=yes",
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
