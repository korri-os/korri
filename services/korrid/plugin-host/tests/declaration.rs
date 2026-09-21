use korri_plugin_host::declaration::Declaration;

const PLUGIN: &str = "export const name = 'network'; export const title = 'Network'; export const services = ['daemon'];";

#[test]
fn unknown_plugin_identity_selects_its_named_native_units() {
    let declaration = Declaration::evaluate(
        "@example",
        &PLUGIN.replace("['daemon']", "['daemon', 'control.socket', 'seat']"),
    )
    .unwrap();
    assert_eq!(declaration.id(), "@example:network");
    assert_eq!(declaration.services, ["daemon", "control.socket", "seat"]);
}

#[test]
fn native_unit_requests_are_bounded_to_the_three_unit_streaming_case() {
    assert!(Declaration::evaluate(
        "@example",
        "export const name = 'three'; export const services = ['one', 'two', 'three'];"
    )
    .is_ok());
    let error = Declaration::evaluate(
        "@example",
        "export const name = 'four'; export const services = ['one', 'two', 'three', 'four'];",
    )
    .unwrap_err();
    assert!(error.contains("at most three native units"), "{error}");
}

#[test]
fn only_the_callers_publisher_can_supply_identity() {
    assert_eq!(
        Declaration::evaluate("@owner", PLUGIN).unwrap().id(),
        "@owner:network"
    );
    assert!(Declaration::evaluate(
        "@owner",
        &format!("{PLUGIN}\nexport const namespace = '@example';")
    )
    .is_err());
    assert!(Declaration::evaluate("not-a-namespace", PLUGIN).is_err());
}

#[test]
fn service_names_are_not_a_second_systemd_language() {
    for value in [
        "[{Type:'exec',ExecStart:['bin/run']}]",
        "['../run']",
        "['daemon','daemon']",
        "null",
    ] {
        assert!(Declaration::evaluate("@example", &PLUGIN.replace("['daemon']", value)).is_err());
    }
    assert!(Declaration::evaluate(
        "@example",
        "export const name='old'; export const daemons=[];"
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
fn empty_service_packages_remain_valid_for_the_game_only_cutover() {
    assert!(
        Declaration::evaluate("@example", &PLUGIN.replace("'network'", "'../../other'")).is_err()
    );
    assert!(Declaration::evaluate(
        "@example",
        "export const name = 'empty'; export const services = []; "
    )
    .unwrap()
    .services
    .is_empty());
    assert!(
        Declaration::evaluate("@example", "export const name = 'empty';")
            .unwrap()
            .services
            .is_empty()
    );
}
