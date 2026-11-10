use super::typed_settings::{describe_runner, validate_runner, SettingScope};
use crate::config::cascade::{leaf, LaunchSettingValue};
use crate::script::source::SourceSnapshot;
use serde_json::json;

fn source() -> SourceSnapshot {
    SourceSnapshot::plugin(r#"
      export const name = "settings-test";
      export const title = "Settings test";
      export const handlers = {
        "settings.describe": () => ({ shared: ["video.render-scale"], revision: "1", schema: {
          type: "object", properties: { diagnostic: {type:"boolean"} }
        }}),
        "settings.validate": ({values}) => ({valid: true, diagnostics:
          values.video?.["render-scale"] === 2.5 ? [{code:"rounded", severity:"warning", path:["video","render-scale"], message:"Requested 2.5x; applying 2x"}] : []
        })
      }
    "#).unwrap()
}

#[test]
fn authoritative_schema_keeps_unsupported_paths_visible_and_native_paths_scoped() {
    let description = describe_runner(&source(), "@test:wii/game", "build").unwrap();
    let video = &description.schema["properties"]["video"]["properties"];
    assert_eq!(video["render-scale"]["x-korri-supported"], true);
    assert_eq!(video["fps-limit"]["x-korri-supported"], false);
    assert_eq!(
        description.scopes["video.render-scale"],
        SettingScope::Device
    );
    assert_eq!(description.scopes["input.rumble"], SettingScope::Shared);
    assert!(description.schema["properties"]["plugin"]["properties"]
        .get("diagnostic")
        .is_some());
}

#[test]
fn skipped_and_rounded_settings_each_report_one_warning_without_rewriting_authored_values() {
    let authored =
        json!({"video":{"render-scale":2.5,"fps-limit":60},"plugin":{"diagnostic":true}});
    let mut effective = serde_json::from_value(authored.clone()).unwrap();
    let checked = validate_runner(&source(), &mut effective, "@test:wii/game", "build").unwrap();
    assert!(leaf(&effective, "video.fps-limit").is_none());
    assert!(matches!(
        leaf(&effective, "plugin.diagnostic"),
        Some(LaunchSettingValue::Boolean(true))
    ));
    assert_eq!(checked.warnings.len(), 2);
    assert_eq!(checked.skipped.len(), 1);
    assert_eq!(authored["video"]["fps-limit"], 60);
    assert!(checked
        .warnings
        .iter()
        .any(|w| w.setting == "video.render-scale"));
    assert!(checked
        .warnings
        .iter()
        .any(|w| w.setting == "video.fps-limit"));
}

#[test]
fn invalid_shared_values_are_skipped_even_when_plugin_validator_accepts_everything() {
    for value in [json!(0), json!(-1), json!("4x")] {
        let mut settings = serde_json::from_value(json!({"video":{"render-scale":value}})).unwrap();
        let checked = validate_runner(&source(), &mut settings, "@test:wii/game", "build").unwrap();
        assert!(leaf(&settings, "video.render-scale").is_none());
        assert_eq!(checked.warnings.len(), 1);
    }
}
