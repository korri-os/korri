use korri_plugin_host::declaration::Declaration;

const PLUGIN: &str = r#"
export const name = 'network';
export const title = 'Network';
export const daemons = [{Type: 'notify', ExecStart: ['bin/daemon', '--state=${STATE_DIRECTORY}/state'], ExecStopPost: ['bin/daemon', '--cleanup'], CapabilityBoundingSet: ['CAP_NET_ADMIN', 'CAP_NET_RAW']}];
"#;

#[test]
fn unknown_plugin_identity_declares_a_network_daemon() {
    let declaration = Declaration::evaluate("@example", PLUGIN).unwrap();
    assert_eq!(declaration.id(), "@example:network");
    assert!(declaration.host_network_admin());
}

#[test]
fn only_the_callers_publisher_can_supply_identity() {
    let declaration = Declaration::evaluate("@owner", PLUGIN).unwrap();
    assert_eq!(declaration.id(), "@owner:network");
    assert!(Declaration::evaluate(
        "@owner",
        &format!("{PLUGIN}\nexport const namespace = '@example';")
    )
    .is_err());
    assert!(Declaration::evaluate("not-a-namespace", PLUGIN).is_err());
}

#[test]
fn unsupported_permissions_and_executable_paths_are_rejected() {
    for replacement in ["CAP_SYS_ADMIN", "CAP_SETUID"] {
        assert!(
            Declaration::evaluate("@example", &PLUGIN.replace("CAP_NET_ADMIN", replacement))
                .is_err()
        );
    }
    for replacement in ["../daemon", "/bin/daemon", "bin/../daemon"] {
        assert!(
            Declaration::evaluate("@example", &PLUGIN.replace("bin/daemon", replacement)).is_err()
        );
    }
    assert!(Declaration::evaluate(
        "@example",
        &PLUGIN.replace("Type: 'notify'", "User: 'root', Type: 'notify'")
    )
    .is_err());
}

#[test]
fn external_source_cannot_hang_or_exhaust_the_host() {
    assert!(Declaration::evaluate("@example", "while (true) {}").is_err());
    assert!(Declaration::evaluate(
        "@example",
        "const a=[]; while(true) a.push(new Array(100000).fill(1))"
    )
    .is_err());
    assert!(Declaration::evaluate("@example", &" ".repeat(128 * 1024 + 1)).is_err());
}

#[test]
fn identity_and_daemon_bounds_are_checked() {
    assert!(
        Declaration::evaluate("@example", &PLUGIN.replace("'network'", "'../../other'")).is_err()
    );
    assert!(Declaration::evaluate(
        "@example",
        "export const name = 'y'; export const title = 'Y'; export const daemons = [];"
    )
    .is_err());
}
