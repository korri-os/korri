use korri_plugin_host::{declaration::Declaration, script::call_plugin_launch_ts};

#[test]
fn game_packages_preserve_the_shipped_declaration_exports_without_a_service() {
    for (name, source) in [
        (
            "retroarch",
            include_str!("../../plugins/retroarch.plugin.ts"),
        ),
        ("mgba", include_str!("../../plugins/mgba.plugin.ts")),
        (
            "moonlight",
            include_str!("../../plugins/moonlight.plugin.ts"),
        ),
    ] {
        let evaluated: serde_json::Value =
            serde_json::from_str(&korri_plugin_host::script::eval_plugin_ts(source).unwrap())
                .unwrap();
        let declaration = Declaration::evaluate("@korri", source).unwrap();
        assert_eq!(declaration.id(), format!("@korri:{name}"));
        assert!(declaration.services.is_empty());
        let retained = serde_json::to_value(declaration).unwrap();
        for (key, value) in evaluated.as_object().unwrap() {
            assert_eq!(&retained[key], value, "{name} export {key}");
        }
    }
}

#[test]
fn native_kind_admission_requires_launch_but_does_not_call_it() {
    let declaration = "export const name = 'game';
        export const launchers = {game: {id:'@example:game/game', kind:'@example:game/game', program:'game'}};";
    for callback in ["", "export const launch = 42;"] {
        let error = Declaration::evaluate("@example", &format!("{declaration} {callback}"))
            .expect_err("native kind without a callable launch must fail admission");
        assert!(error.contains("launch"), "{error}");
    }
    Declaration::evaluate(
        "@example",
        &format!("{declaration} export function launch() {{ throw new Error('must not run'); }}"),
    )
    .unwrap();
}

#[test]
fn empty_data_exports_are_not_silently_changed_to_absent_exports() {
    for export in [
        "providers",
        "systems",
        "launchers",
        "runtimes",
        "transports",
        "sessionControls",
        "discovery",
        "android",
        "config",
    ] {
        let declaration = Declaration::evaluate(
            "@example",
            &format!("export const name='game'; export const {export}={{}};"),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(declaration).unwrap()[export],
            serde_json::json!({}),
            "{export}"
        );
        for invalid in ["null", "[]", "'text'"] {
            assert!(
                Declaration::evaluate(
                    "@example",
                    &format!("export const name='game'; export const {export}={invalid};")
                )
                .is_err(),
                "{export}={invalid}"
            );
        }
    }
}

#[test]
fn host_admission_preserves_data_and_never_invokes_the_launch_callback() {
    let source = "export const name = 'game';
        export const config = {launchers: {}};
        export const android = {packageName: 'org.example.game'};
        export function launch(input) { return {command: input.program, args: [input.runtime.id]}; }";
    let declaration = Declaration::evaluate("@example", source).unwrap();
    let data = serde_json::to_value(declaration).unwrap();
    assert_eq!(data["config"], serde_json::json!({"launchers": {}}));
    assert_eq!(data["android"]["packageName"], "org.example.game");
    assert!(data.get("launch").is_none());
    let launch: serde_json::Value = serde_json::from_str(
        &call_plugin_launch_ts(
            source,
            r#"{"program":"/approved/program","runtime":{"id":"@example:game/core"}}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        launch,
        serde_json::json!({"command":"/approved/program", "args":["@example:game/core"]})
    );
    let never_call = "export const name = 'game'; export function launch() { throw new Error('not during approval'); }";
    assert!(Declaration::evaluate("@example", never_call).is_ok());
}
