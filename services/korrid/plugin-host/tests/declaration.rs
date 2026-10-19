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

#[test]
fn modifier_data_is_retained_without_adding_effect_authority() {
    let declaration = Declaration::evaluate("@example", "export const name='wrapper'; export const modifiers={main:{id:'@example:wrapper/main'}}; export const handlers={'launch.compose': input => input.plan};").unwrap();
    assert_eq!(
        declaration.modifiers.as_ref().unwrap()["main"]["id"],
        "@example:wrapper/main"
    );
    assert!(declaration.services.is_empty());
}

#[test]
fn steam_discovery_data_survives_admission_but_grants_no_native_effects() {
    let source = include_str!("fixtures/steam.plugin.ts");
    let declaration = Declaration::evaluate("@korri", source).unwrap();
    assert_eq!(
        declaration.discovery.as_ref().unwrap()[0].id,
        "@korri:steam/installed-apps"
    );
    assert!(declaration.services.is_empty());
    assert!(Declaration::evaluate("@thirdparty", source).is_err());
    assert!(Declaration::evaluate(
        "@korri",
        &source.replace("@korri:steam/installed-apps", "@korri:steam/other")
    )
    .is_err());
}
