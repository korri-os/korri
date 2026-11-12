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
            // The governor is one device value: only the device keeps it.
            let layer = if matches!(path, LaunchPath::CpuGovernor) {
                LaunchLayer::Device
            } else {
                layer.clone()
            };
            let place = LaunchPlace { layer, scope };
            if !snapshot
                .places
                .iter()
                .any(|candidate| candidate.place == place)
            {
                return Err(CliError::usage(
                    "That keep destination is not an available place for this game.",
                ));
            }
            let expected_revision =
                if matches!(place.layer, LaunchLayer::Device | LaunchLayer::System(_)) {
                    &snapshot.revisions.device
                } else {
                    &snapshot.revisions.games
                };
            result.push(LaunchSetRequest {
                clear: value.is_null(),
                value: (!value.is_null()).then_some(value),
                expected_revision: expected_revision.clone(),
                host: None,
                place,
                path,
            });
            Ok(())
        };
    // A shared value is kept for any runner, a runner-only value for its
    // runner. Nobody is asked for a scope (ADR 0066).
    let mut values = korrid::config::cascade::leaves(&launch.settings).0;
    values.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in values {
        add(
            LaunchPath::Setting { key },
            LaunchRunnerScope::Any,
            json!(value),
        )?;
    }
    let mut runners: Vec<_> = launch.plugin.iter().collect();
    runners.sort_by_key(|(id, _)| *id);
    for (runner, own) in runners {
        let mut values = korrid::config::cascade::leaves(own).0;
        values.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in values {
            add(
                LaunchPath::Setting {
                    key: format!("plugin.{key}"),
                },
                LaunchRunnerScope::Runner(runner.clone()),
                json!(value),
            )?;
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
    if let Some(governor) = launch.cpu.as_ref().and_then(|cpu| cpu.governor.as_ref()) {
        add(
            LaunchPath::CpuGovernor,
            LaunchRunnerScope::Any,
            json!(governor),
        )?;
    }
    let mut services: Vec<_> = launch.services.iter().collect();
    services.sort_by_key(|(id, _)| *id);
    for (id, service) in services {
        let mut values = korrid::config::cascade::leaves(&service.settings).0;
        values.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in values {
            add(
                LaunchPath::ServiceSetting {
                    id: id.clone(),
                    key,
                },
                LaunchRunnerScope::Any,
                json!(value),
            )?;
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
                {"place":{"layer":{"_tag":"Device"},"scope":{"_tag":"Any"}},"title":"Device","body":{},"editableKinds":["cpu"]},
                {"place":{"layer":{"_tag":"Game","id":"game"},"scope":{"_tag":"Any"}},"title":"Game","body":{},"editableKinds":["settings"]},
                {"place":{"layer":{"_tag":"Game","id":"game"},"scope":{"_tag":"Runner","id":"@test:runner/play"}},"title":"Runner","body":{},"editableKinds":["settings"]}
            ], "resolved":{"values":{}},"runners":[],"runnerResolved":[],"modifiers":[],"services":[],"profiles":[],"notices":[],"routeLayers":{"_tag":"Present"},"revisions":{"device":"d1","games":"g1"}
        })).unwrap()
    }
    #[test]
    fn keeps_shared_values_at_any_scope_and_runner_only_values_for_their_runner() {
        let launch: LaunchOverride = serde_json::from_value(json!({
            "settings":{"video":{"render-scale":4}},
            "plugin":{"@test:runner/play":{"quality":2}},
            "cpu":{"max":60,"governor":"performance"},
            "services":{"@korri:sunshine/sunshine":{"settings":{"video":{"max-bitrate-kbps":3000}}}}
        }))
        .unwrap();
        let writes = requests(&snapshot(), "game", &launch, "game").unwrap();
        assert_eq!(writes.len(), 5);
        let shared = writes
            .iter()
            .find(|w| matches!(&w.path, LaunchPath::Setting { key } if key == "video.render-scale"))
            .unwrap();
        assert_eq!(shared.place.scope, LaunchRunnerScope::Any);
        assert_eq!(shared.value, Some(json!(4)));
        assert_eq!(shared.expected_revision, "g1");
        let own = writes
            .iter()
            .find(|w| matches!(&w.path, LaunchPath::Setting { key } if key == "plugin.quality"))
            .unwrap();
        assert_eq!(
            own.place.scope,
            LaunchRunnerScope::Runner("@test:runner/play".into())
        );
        // The governor is one device value: the device keeps it, not the game.
        let governor = writes
            .iter()
            .find(|w| matches!(w.path, LaunchPath::CpuGovernor))
            .unwrap();
        assert_eq!(governor.place.layer, LaunchLayer::Device);
        assert_eq!(governor.expected_revision, "d1");
        assert!(writes.iter().any(|w| matches!(
            &w.path,
            LaunchPath::ServiceSetting { id, key }
                if id == "@korri:sunshine/sunshine" && key == "video.max-bitrate-kbps"
        ) && w.place.layer == LaunchLayer::Game("game".into())));
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
