use super::*;
use crate::{
    config::{
        resolver::{self, ResolvedRoute},
        settings::{self, SettingsError},
        ConfigSnapshot,
    },
    plugin::PluginRegistry,
};
use std::path::Path;

fn values(limits: Option<&config::Limits>) -> LaunchSettingsValues {
    LaunchSettingsValues {
        min: limits.and_then(|v| v.min),
        max: limits.and_then(|v| v.max),
    }
}
fn layer(
    scope: LaunchSettingsScope,
    title: String,
    cpu: Option<&config::Limits>,
    gpu: Option<&config::Limits>,
) -> LaunchSettingsLayer {
    LaunchSettingsLayer {
        scope,
        title,
        cpu: values(cpu),
        gpu: values(gpu),
        governor: None,
    }
}
pub fn layers(
    snapshot: &ConfigSnapshot,
    game_id: Option<&str>,
    route: Option<&ResolvedRoute>,
    facts: Option<&LaunchDeviceFacts>,
) -> Result<Vec<LaunchSettingsLayer>, SettingsError> {
    let host = snapshot.host.as_ref();
    let cpu = host.and_then(|h| h.cpu.as_ref());
    let gpu = host.and_then(|h| h.gpu.as_ref());
    let defaults = facts.map(|f| &f.defaults);
    let mut device = layer(LaunchSettingsScope::Device, "Device".into(), None, gpu);
    device.cpu = LaunchSettingsValues {
        min: cpu.and_then(|c| c.min).or(defaults.map(|d| d.cpu.min)),
        max: cpu.and_then(|c| c.max).or(defaults.map(|d| d.cpu.max)),
    };
    device.gpu.min = device.gpu.min.or(defaults.map(|d| d.gpu.min));
    device.gpu.max = device.gpu.max.or(defaults.map(|d| d.gpu.max));
    device.governor = cpu
        .and_then(|c| c.governor.as_ref())
        .map(|g| g.0.clone())
        .or_else(|| defaults.map(|d| d.governor.clone()));
    let mut result = vec![device];
    if let Some(id) = game_id {
        let game = snapshot
            .library
            .get(id)
            .ok_or_else(|| SettingsError::Invalid(format!("unknown game {id}")))?;
        // The same route resolver used at launch supplies release/system/runner.
        // A game with no selected route can still edit its game and profiles.
        // ResolvedRoute.release_id is the content identity, not the catalog record ID.
        let release = route.and_then(|r| {
            game.releases.iter().find(|v| {
                v.identity
                    .as_ref()
                    .is_some_and(|identity| identity.value() == r.release_id)
            })
        });
        if let Some(route) = route {
            if !route.system_id.is_empty() {
                let system = snapshot.systems.get(&route.system_id);
                result.push(layer(
                    LaunchSettingsScope::System(route.system_id.clone()),
                    route
                        .system_title
                        .clone()
                        .unwrap_or_else(|| route.system_id.clone()),
                    system.and_then(|v| v.cpu.as_ref()),
                    system.and_then(|v| v.gpu.as_ref()),
                ));
            }
            let runner = snapshot.runners.get(&route.runner_id);
            result.push(layer(
                LaunchSettingsScope::Runner(route.runner_id.clone()),
                route.runner_id.clone(),
                runner.and_then(|v| v.cpu.as_ref()),
                runner.and_then(|v| v.gpu.as_ref()),
            ));
        }
        result.push(layer(
            LaunchSettingsScope::Game(id.into()),
            game.title.clone().unwrap_or_else(|| id.into()),
            game.cpu.as_ref(),
            game.gpu.as_ref(),
        ));
        if let Some(release) = release {
            result.push(layer(
                LaunchSettingsScope::Release {
                    game_id: id.into(),
                    release_id: release.id.0.clone(),
                },
                release.id.0.clone(),
                release.inheritable.cpu.as_ref(),
                release.inheritable.gpu.as_ref(),
            ));
        }
        if let Some(pick) = game.profile.as_ref() {
            let profile = game.profiles.get(&pick.0).ok_or_else(|| {
                SettingsError::Invalid(format!("unknown selected profile {}", pick.0))
            })?;
            result.push(layer(
                LaunchSettingsScope::Profile {
                    game_id: id.into(),
                    profile_id: pick.0.clone(),
                },
                profile.title.0.clone(),
                profile.cpu.as_ref(),
                profile.gpu.as_ref(),
            ));
        }
    }
    Ok(result)
}

/// Resolve independently, then move only the less-specific side of a conflict.
pub fn resolve(layers: &[LaunchSettingsLayer]) -> LaunchSettingsResolved {
    fn range(layers: &[LaunchSettingsLayer], cpu: bool) -> LaunchResolvedRange {
        let mut min = (
            0,
            LaunchResolvedValue {
                value: 0,
                source: LaunchSettingsScope::Device,
                adjusted: false,
            },
        );
        let mut max = (
            0,
            LaunchResolvedValue {
                value: 100,
                source: LaunchSettingsScope::Device,
                adjusted: false,
            },
        );
        for (index, layer) in layers.iter().enumerate() {
            let values = if cpu { &layer.cpu } else { &layer.gpu };
            if let Some(value) = values.min {
                min = (
                    index,
                    LaunchResolvedValue {
                        value,
                        source: layer.scope.clone(),
                        adjusted: false,
                    },
                );
            }
            if let Some(value) = values.max {
                max = (
                    index,
                    LaunchResolvedValue {
                        value,
                        source: layer.scope.clone(),
                        adjusted: false,
                    },
                );
            }
        }
        if min.1.value > max.1.value {
            // At equal specificity a min is the floor: move max to meet it.
            if min.0 >= max.0 {
                max.1.value = min.1.value;
                max.1.adjusted = true;
            } else {
                min.1.value = max.1.value;
                min.1.adjusted = true;
            }
        }
        LaunchResolvedRange {
            min: min.1,
            max: max.1,
        }
    }
    LaunchSettingsResolved {
        cpu: range(layers, true),
        gpu: range(layers, false),
        governor: layers.first().and_then(|l| l.governor.clone()),
    }
}

pub fn snapshot(
    root: &Path,
    registry: Option<&PluginRegistry>,
    game_id: Option<&str>,
    client: &helper::Client,
) -> Result<LaunchSettingsSnapshot, SettingsError> {
    let (config, revisions) = settings::runner_choice_snapshot(root)?;
    let device_facts = client.describe().ok();
    let (route, route_layers) = match (game_id, registry) {
        (None, _) => (None, LaunchSettingsRouteLayers::Present),
        (Some(id), Some(registry)) => {
            match resolver::linux_route_candidates(root, &config, registry, id) {
                Err(error) => (None, LaunchSettingsRouteLayers::Unavailable(error.message)),
                Ok(candidates) => match resolver::linux_route_selection(&config, &candidates, None)
                {
                    Ok(resolver::LinuxRouteSelection::Selected(id)) => (
                        candidates.into_iter().find(|r| r.runner_id == id),
                        LaunchSettingsRouteLayers::Present,
                    ),
                    Ok(_) => (None, LaunchSettingsRouteLayers::NeedsRunnerChoice),
                    Err(error) => (None, LaunchSettingsRouteLayers::Unavailable(error.message)),
                },
            }
        }
        (Some(_), None) => (
            None,
            LaunchSettingsRouteLayers::Unavailable(
                "installed runner registry is unavailable".into(),
            ),
        ),
    };
    let layers = layers(&config, game_id, route.as_ref(), device_facts.as_ref())?;
    let game = game_id.and_then(|id| config.library.get(id));
    Ok(LaunchSettingsSnapshot {
        route_layers,
        resolved: resolve(&layers),
        layers,
        profiles: game
            .map(|g| {
                g.profiles
                    .iter()
                    .map(|(id, p)| LaunchSettingsProfile {
                        id: id.clone(),
                        title: p.title.0.clone(),
                        cpu: values(p.cpu.as_ref()),
                        gpu: values(p.gpu.as_ref()),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        selected_profile: game.and_then(|g| g.profile.as_ref()).map(|p| p.0.clone()),
        device_facts,
        revisions,
    })
}

pub fn apply_device(root: &Path, client: &helper::Client) -> Result<(), String> {
    let (snapshot, _) = settings::runner_choice_snapshot(root).map_err(|e| e.to_string())?;
    let facts = client.describe()?;
    let layers = layers(&snapshot, None, None, Some(&facts)).map_err(|e| e.to_string())?;
    let resolved = resolve(&layers);
    client.apply_device(&LaunchDeviceDefaults {
        cpu: LaunchPercentRange {
            min: resolved.cpu.min.value,
            max: resolved.cpu.max.value,
        },
        gpu: LaunchPercentRange {
            min: resolved.gpu.min.value,
            max: resolved.gpu.max.value,
        },
        governor: resolved.governor.unwrap_or(facts.defaults.governor),
    })
}
