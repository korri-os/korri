//! Keep explicitly requested one-launch values through the existing CAS setter.
use crate::error::CliError;
use korrid::launch_settings::{
    LaunchLayer, LaunchOverride, LaunchPath, LaunchPlace, LaunchRunnerScope, LaunchSetRequest,
    LaunchSnapshot,
};
use serde_json::{json, Value};

pub(crate) fn requests(
    snapshot: &LaunchSnapshot,
    game: &str,
    launch: &LaunchOverride,
    target: &str,
) -> Result<Vec<LaunchSetRequest>, CliError> {
    let layer = match target {
        "game" => LaunchLayer::Game(game.into()),
        "device" => LaunchLayer::Device,
        _ => {
            let (kind, id) = target
                .split_once(':')
                .map_or((target, None), |(kind, id)| (kind, Some(id)));
            let mut choices = Vec::new();
            for place in &snapshot.places {
                let matches = match &place.place.layer {
                    LaunchLayer::System(value) => {
                        kind == "system" && id.is_none_or(|id| id == value)
                    }
                    LaunchLayer::Release { release_id, .. } => {
                        kind == "release" && id.is_none_or(|id| id == release_id)
                    }
                    LaunchLayer::Profile { profile_id, .. } => {
                        kind == "profile" && id.is_none_or(|id| id == profile_id)
                    }
                    _ => false,
                };
                if matches && !choices.contains(&place.place.layer) {
                    choices.push(place.place.layer.clone());
                }
            }
            if choices.len() != 1 {
                return Err(CliError::usage("Keep needs game, device, system:ID, release:ID or profile:ID from launch-settings show."));
            }
            choices.remove(0)
        }
    };
    let mut result = Vec::new();
    let mut add =
        |path: LaunchPath, scope: LaunchRunnerScope, value: Value| -> Result<(), CliError> {
            let place = LaunchPlace {
                layer: layer.clone(),
                scope,
            };
            if !snapshot
                .places
                .iter()
                .any(|candidate| candidate.place == place)
            {
                return Err(CliError::usage(
                    "That keep destination is not an available place for this game.",
                ));
            }
            let expected_revision = if matches!(layer, LaunchLayer::Device | LaunchLayer::System(_))
            {
                &snapshot.revisions.device
            } else {
                &snapshot.revisions.games
            };
            result.push(LaunchSetRequest {
                place,
                path,
                clear: value.is_null(),
                value: (!value.is_null()).then_some(value),
                expected_revision: expected_revision.clone(),
            });
            Ok(())
        };
    if let Some(settings) = &launch.settings {
        let mut values = korrid::config::cascade::leaves(&settings.values).0;
        values.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in values {
            let scope = if korri_settings_treaty::SharedSetting::from_path(&key).is_some() {
                LaunchRunnerScope::Any
            } else {
                settings.scope.clone()
            };
            add(LaunchPath::Setting { key }, scope, json!(value))?;
        }
    }
    for (path, value) in [
        (LaunchPath::CpuMin, launch.cpu.as_ref().and_then(|v| v.min)),
        (LaunchPath::CpuMax, launch.cpu.as_ref().and_then(|v| v.max)),
        (LaunchPath::GpuMin, launch.gpu.as_ref().and_then(|v| v.min)),
        (LaunchPath::GpuMax, launch.gpu.as_ref().and_then(|v| v.max)),
    ] {
        if let Some(value) = value {
            add(path, LaunchRunnerScope::Any, json!(value))?;
        }
    }
    let mut modifiers: Vec<_> = launch.modifiers.iter().collect();
    modifiers.sort_by_key(|(id, _)| *id);
    for (id, modifier) in modifiers {
        if let Some(enabled) = modifier.enabled {
            add(
                LaunchPath::ModifierEnabled { id: id.clone() },
                LaunchRunnerScope::Any,
                json!(enabled),
            )?;
        }
        let mut values = korrid::config::cascade::leaves(&modifier.settings).0;
        values.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in values {
            add(
                LaunchPath::ModifierSetting {
                    id: id.clone(),
                    key,
                },
                LaunchRunnerScope::Any,
                json!(value),
            )?;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> LaunchSnapshot {
        serde_json::from_value(json!({
            "places":[
                {"place":{"layer":{"_tag":"Game","id":"game"},"scope":{"_tag":"Any"}},"title":"Game","body":{},"editableKinds":["settings"]},
                {"place":{"layer":{"_tag":"Game","id":"game"},"scope":{"_tag":"Runner","id":"@test:runner/play"}},"title":"Runner","body":{},"editableKinds":["settings"]}
            ], "resolved":{"values":{}},"runners":[],"runnerResolved":[],"modifiers":[],"services":[],"profiles":[],"notices":[],"routeLayers":{"_tag":"Present"},"revisions":{"device":"d1","games":"g1"}
        })).unwrap()
    }
    #[test]
    fn keeps_shared_values_at_any_scope_and_native_values_at_runner_scope() {
        let launch: LaunchOverride = serde_json::from_value(json!({"settings":{"scope":{"_tag":"Runner","id":"@test:runner/play"},"values":{"video":{"render-scale":4},"plugin":{"quality":2}}}})).unwrap();
        let writes = requests(&snapshot(), "game", &launch, "game").unwrap();
        assert_eq!(writes.len(), 2);
        let shared = writes
            .iter()
            .find(|w| matches!(&w.path, LaunchPath::Setting { key } if key == "video.render-scale"))
            .unwrap();
        assert_eq!(shared.place.scope, LaunchRunnerScope::Any);
        assert_eq!(shared.value, Some(json!(4)));
        assert_eq!(shared.expected_revision, "g1");
        let native = writes
            .iter()
            .find(|w| matches!(&w.path, LaunchPath::Setting { key } if key == "plugin.quality"))
            .unwrap();
        assert!(matches!(native.place.scope, LaunchRunnerScope::Runner(_)));
    }
    #[test]
    fn refuses_unobserved_destinations_before_launch() {
        assert!(requests(
            &snapshot(),
            "game",
            &LaunchOverride::default(),
            "profile:missing"
        )
        .is_err());
    }
}
