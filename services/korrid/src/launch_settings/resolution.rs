use super::*;
use crate::{
    config::{
        place::PlaceBody,
        resolver::{self, ResolvedRoute},
        settings::{self, SettingsError},
        ConfigSnapshot,
    },
    plugin::PluginRegistry,
};
use std::{collections::BTreeSet, path::Path};

pub(crate) const SUNSHINE: &str = "@korri:sunshine/sunshine";
pub(crate) fn choosable(runner: &crate::plugin::RunnerRecord) -> bool {
    !(runner.program.is_some()
        && runner.systems.as_ref().is_none_or(Vec::is_empty)
        && runner.releases.as_ref().is_none_or(Vec::is_empty))
}

fn checked_settings(
    registry: &PluginRegistry,
    id: &str,
    settings: &mut std::collections::HashMap<String, crate::config::cascade::LaunchSettingValue>,
    prefix: &str,
    values: &mut LaunchResolvedSnapshot,
    notices: &mut Vec<LaunchNotice>,
    path: impl Fn(String) -> LaunchPath,
) -> Result<(), SettingsError> {
    let package = registry
        .installed_package(id)
        .map_err(|e| SettingsError::Invalid(e.to_string()))?;
    let source = crate::script::source::SourceSnapshot::package_plugin(
        &package.package,
        &package.entry,
        &package.sources,
    )
    .map_err(SettingsError::Invalid)?;
    let check = crate::launcher::typed_settings::validate(
        &source,
        settings,
        id,
        &package.package.display().to_string(),
    )
    .map_err(SettingsError::Invalid)?;
    for (key, message) in check.skipped {
        values.values.retain(|name, _| {
            if key.is_empty() {
                !name.starts_with(&format!("{prefix}."))
            } else {
                !(name == &format!("{prefix}.{key}")
                    || name.starts_with(&format!("{prefix}.{key}.")))
            }
        });
        let path = path(key);
        let runner_id = matches!(path, LaunchPath::Setting { .. }).then(|| id.to_owned());
        notices.push(LaunchNotice::SettingSkipped {
            path,
            message,
            runner_id,
        });
    }
    Ok(())
}

pub(crate) fn context(snapshot: &ConfigSnapshot, game_id: Option<&str>) -> ResolvedRoute {
    let game = game_id.and_then(|id| snapshot.library.get(id));
    let release = game.and_then(|g| g.releases.first());
    ResolvedRoute {
        playable_id: game_id.unwrap_or("").into(),
        title: game.and_then(|v| v.title.clone()),
        release_id: release
            .and_then(|v| v.identity.as_ref())
            .map(|v| v.value().into())
            .unwrap_or_default(),
        identity: None,
        provider_id: String::new(),
        system_id: release
            .and_then(|v| v.system.as_ref())
            .map(|v| v.0.clone())
            .unwrap_or_default(),
        system_title: None,
        runner_id: String::new(),
        family_id: None,
        integration_token: String::new(),
        flattened_target: String::new(),
        linux_runner: None,
        core_path: None,
        file_target: None,
        provider_target: None,
        artifact_target: None,
        package_target: None,
        cache_target: None,
    }
}
/// A settings-only runner has no selected local release. Both snapshots and
/// the viewer use the joining device's own game, system and selected profile.
pub(crate) fn runner_settings_context(
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    runner_id: &str,
    game_id: Option<&str>,
) -> ResolvedRoute {
    let game_id = game_id.filter(|id| snapshot.library.contains_key(*id));
    let mut route = context(snapshot, game_id);
    route.runner_id = runner_id.into();
    route.release_id.clear();
    route.family_id = registry
        .runners()
        .get(runner_id)
        .and_then(|r| r.family.clone());
    route
}

fn runner_defaults(
    values: &mut LaunchResolvedSnapshot,
    description: &crate::launcher::typed_settings::SettingsDescription,
    plugin_id: &str,
) {
    for (key, value) in &description.defaults {
        let name = format!("settings.{key}");
        if !values.values.contains_key(&name) {
            super::launch::sourced(
                &mut values.values,
                name,
                LaunchValuePath::Editable(LaunchPath::Setting { key: key.clone() }),
                serde_json::to_value(value).unwrap(),
                LaunchSource::PluginDefault {
                    plugin_id: plugin_id.into(),
                },
            );
        }
    }
}

pub fn snapshot(
    root: &Path,
    registry: Option<&PluginRegistry>,
    game_id: Option<&str>,
    client: &helper::Client,
) -> Result<LaunchSnapshot, SettingsError> {
    snapshot_for_person(root, registry, game_id, client, None)
}
pub fn snapshot_for_person(
    root: &Path,
    registry: Option<&PluginRegistry>,
    game_id: Option<&str>,
    client: &helper::Client,
    person: Option<&str>,
) -> Result<LaunchSnapshot, SettingsError> {
    let (config, revisions) = settings::runner_choice_snapshot(root)?;
    if let Some(id) = game_id {
        if !config.library.contains_key(id) {
            return Err(SettingsError::Invalid(format!("unknown game {id}")));
        }
    }
    let facts = client.describe().ok();
    let mut notices = Vec::new();
    let (candidates, route_layers) = match (game_id, registry) {
        (None, _) => (Vec::new(), LaunchSettingsRouteLayers::Present),
        (Some(id), Some(registry)) => {
            match resolver::linux_route_candidates(root, &config, registry, id) {
                Ok(v) => (v, LaunchSettingsRouteLayers::Present),
                Err(v) => (
                    Vec::new(),
                    LaunchSettingsRouteLayers::Unavailable(v.message),
                ),
            }
        }
        _ => (
            Vec::new(),
            LaunchSettingsRouteLayers::Unavailable(
                "installed runner registry is unavailable".into(),
            ),
        ),
    };
    let (selected, missing) = super::launch::choose_runner(&config, &candidates, None);
    notices.extend(missing);
    let route = candidates
        .iter()
        .find(|v| Some(v.runner_id.as_str()) == selected.as_deref())
        .cloned()
        .unwrap_or_else(|| context(&config, game_id));
    let route_layers = if game_id.is_some() && selected.is_none() && !candidates.is_empty() {
        LaunchSettingsRouteLayers::NeedsRunnerChoice
    } else {
        route_layers
    };
    let mut folded = resolve_launch(&config, &route, None, facts.as_ref())?;
    notices.append(&mut folded.notices);
    let mut resolved = folded.values;
    let mut runners = Vec::new();
    let mut runner_resolved = Vec::new();
    let mut modifiers = Vec::new();
    let mut services = Vec::new();
    if let Some(registry) = registry {
        for declaration in registry
            .modifiers()
            .values()
            .filter(|v| registry.enabled_modifier(&v.id).is_ok())
        {
            let package = registry
                .installed_package(&declaration.id)
                .map_err(|e| SettingsError::Invalid(e.to_string()))?;
            let source = crate::script::source::SourceSnapshot::package_plugin(
                &package.package,
                &package.entry,
                &package.sources,
            )
            .map_err(SettingsError::Invalid)?;
            let description = crate::launcher::typed_settings::describe(
                &source,
                &declaration.id,
                &package.package.display().to_string(),
            )
            .map_err(SettingsError::Invalid)?;
            if let Some(record) = folded.modifiers.get_mut(&declaration.id) {
                checked_settings(
                    registry,
                    &declaration.id,
                    &mut record.settings,
                    &format!("modifiers.{}.settings", declaration.id),
                    &mut resolved,
                    &mut notices,
                    |key| LaunchPath::ModifierSetting {
                        id: declaration.id.clone(),
                        key,
                    },
                )?;
            }
            for (key, value) in &description.defaults {
                let name = format!("modifiers.{}.settings.{key}", declaration.id);
                if !resolved.values.contains_key(&name) {
                    super::launch::sourced(
                        &mut resolved.values,
                        name,
                        LaunchValuePath::Editable(LaunchPath::ModifierSetting {
                            id: declaration.id.clone(),
                            key: key.clone(),
                        }),
                        serde_json::to_value(value).unwrap(),
                        LaunchSource::PluginDefault {
                            plugin_id: package.id.clone(),
                        },
                    );
                }
            }
            let owner = declaration.id.split_once('/').unwrap().0;
            modifiers.push(LaunchModifierSnapshot {
                id: declaration.id.clone(),
                title: registry
                    .plugin_title(owner)
                    .unwrap_or(&declaration.id)
                    .into(),
                schema: description.schema_available.then_some(description.schema),
                revision: description.schema_available.then_some(description.revision),
            });
        }
        if let Ok(package) = registry.installed_package(SUNSHINE) {
            let source = crate::script::source::SourceSnapshot::package_plugin(
                &package.package,
                &package.entry,
                &package.sources,
            )
            .map_err(SettingsError::Invalid)?;
            let description = crate::launcher::typed_settings::describe(
                &source,
                SUNSHINE,
                &package.package.display().to_string(),
            )
            .map_err(SettingsError::Invalid)?;
            if let Some(record) = folded.services.get_mut(SUNSHINE) {
                checked_settings(
                    registry,
                    SUNSHINE,
                    &mut record.settings,
                    &format!("services.{SUNSHINE}.settings"),
                    &mut resolved,
                    &mut notices,
                    |key| LaunchPath::ServiceSetting {
                        id: SUNSHINE.into(),
                        key,
                    },
                )?;
            }
            for (key, value) in &description.defaults {
                let name = format!("services.{SUNSHINE}.settings.{key}");
                if !resolved.values.contains_key(&name) {
                    super::launch::sourced(
                        &mut resolved.values,
                        name,
                        LaunchValuePath::Editable(LaunchPath::ServiceSetting {
                            id: SUNSHINE.into(),
                            key: key.clone(),
                        }),
                        serde_json::to_value(value).unwrap(),
                        LaunchSource::PluginDefault {
                            plugin_id: package.id.clone(),
                        },
                    );
                }
            }
            services.push(LaunchServiceSnapshot {
                id: SUNSHINE.into(),
                title: registry
                    .plugin_title("@korri:sunshine")
                    .unwrap_or("Sunshine")
                    .into(),
                schema: description.schema_available.then_some(description.schema),
                revision: description.schema_available.then_some(description.revision),
            });
        }
        let ids: BTreeSet<String> = registry
            .runners()
            .values()
            .filter(|v| {
                v.program.is_some()
                    && (game_id.is_none()
                        || !choosable(v)
                        || candidates.iter().any(|c| c.runner_id == v.id))
            })
            .map(|v| v.id.clone())
            .collect();
        for id in ids {
            let declaration = &registry.runners()[&id];
            let package = registry
                .installed_package(&id)
                .map_err(|e| SettingsError::Invalid(e.to_string()))?;
            let description = (|| {
                let source = crate::script::source::SourceSnapshot::package_plugin(
                    &package.package,
                    &package.entry,
                    &package.sources,
                )?;
                crate::launcher::typed_settings::describe(
                    &source,
                    &id,
                    &package.package.display().to_string(),
                )
            })();
            let prepare = (|| -> Result<LaunchPrepareFacts, String> {
                let candidate = candidates
                    .iter()
                    .find(|v| v.runner_id == id)
                    .ok_or("select a game to inspect launch preparation")?;
                let person =
                    person.ok_or("a device owner is required for person-specific launch facts")?;
                let spec = crate::launcher::linux_plugin::launch_route(
                    root, &config, registry, candidate, None, None, person,
                )
                .map_err(|v| v.to_string())?;
                let plan = crate::launcher::plugin_launch::runner_prepare_plan(&spec.command)?;
                Ok(LaunchPrepareFacts {
                    env: plan.env,
                    args: plan.args,
                    cwd: plan.cwd,
                })
            })();
            if !choosable(declaration) {
                let settings_route = runner_settings_context(&config, registry, &id, game_id);
                let mut settings_only =
                    resolve_launch(&config, &settings_route, None, facts.as_ref())?;
                notices.extend(settings_only.notices.into_iter().filter(|n| {
                    matches!(
                        n,
                        LaunchNotice::SettingSkipped {
                            path: LaunchPath::Setting { .. },
                            ..
                        }
                    )
                }));
                settings_only
                    .values
                    .values
                    .retain(|name, _| name.starts_with("settings."));
                checked_settings(
                    registry,
                    &id,
                    &mut settings_only.runner.settings,
                    "settings",
                    &mut settings_only.values,
                    &mut notices,
                    |key| LaunchPath::Setting { key },
                )?;
                if let Ok(description) = &description {
                    runner_defaults(&mut settings_only.values, description, &package.id);
                }
                runner_resolved.push(LaunchRunnerResolved {
                    runner_id: id.clone(),
                    values: settings_only.values.values,
                });
            }
            if selected.as_deref() == Some(&id) {
                checked_settings(
                    registry,
                    &id,
                    &mut folded.runner.settings,
                    "settings",
                    &mut resolved,
                    &mut notices,
                    |key| LaunchPath::Setting { key },
                )?;
                let source = LaunchSource::PluginDefault {
                    plugin_id: package.id.clone(),
                };
                if let Ok(description) = &description {
                    runner_defaults(&mut resolved, description, &package.id);
                }
                if let Ok(prepare) = &prepare {
                    for (key, value) in &prepare.env {
                        let name = format!("env.{key}");
                        if !resolved.values.contains_key(&name) {
                            super::launch::sourced(
                                &mut resolved.values,
                                name,
                                LaunchValuePath::Env { key: key.clone() },
                                value.clone().into(),
                                source.clone(),
                            );
                        }
                    }
                    if let Some(cwd) = &prepare.cwd {
                        if !resolved.values.contains_key("cwd") {
                            super::launch::sourced(
                                &mut resolved.values,
                                "cwd".into(),
                                LaunchValuePath::Cwd,
                                cwd.clone().into(),
                                source.clone(),
                            );
                        }
                    }
                }
            }
            let (schema, revision, prepare) = match description {
                Ok(v) => (v.schema, v.revision, prepare),
                Err(e) => (serde_json::json!({}), String::new(), Err(e)),
            };
            runners.push(LaunchRunnerSnapshot {
                id: id.clone(),
                family: declaration.family.clone(),
                title: registry
                    .plugin_title(id.split_once('/').unwrap().0)
                    .unwrap_or(&id)
                    .into(),
                selected: selected.as_deref() == Some(&id),
                choosable: choosable(declaration),
                schema,
                revision,
                prepare: match prepare {
                    Ok(v) => LaunchRunnerFacts::Ready(v),
                    Err(e) => LaunchRunnerFacts::Unavailable(e),
                },
            });
        }
    }
    let mut places = Vec::new();
    let empty = PlaceBody::default();
    let game = game_id.and_then(|id| config.library.get(id));
    let release = game.and_then(|g| {
        g.releases.iter().find(|v| {
            v.identity
                .as_ref()
                .is_some_and(|id| id.value() == route.release_id)
        })
    });
    let layers = [
        Some((
            LaunchLayer::Device,
            "Device".into(),
            config.host.as_ref().map(|v| &v.place).unwrap_or(&empty),
        )),
        (!route.system_id.is_empty()).then(|| {
            (
                LaunchLayer::System(route.system_id.clone()),
                route
                    .system_title
                    .clone()
                    .unwrap_or_else(|| route.system_id.clone()),
                config
                    .systems
                    .get(&route.system_id)
                    .map(|v| &v.place)
                    .unwrap_or(&empty),
            )
        }),
        game_id.zip(game).map(|(id, g)| {
            (
                LaunchLayer::Game(id.into()),
                g.title.clone().unwrap_or_else(|| id.into()),
                &g.place,
            )
        }),
        game_id.zip(release).map(|(id, r)| {
            (
                LaunchLayer::Release {
                    game_id: id.into(),
                    release_id: r.id.0.clone(),
                },
                r.id.0.clone(),
                &r.inheritable,
            )
        }),
    ];
    let families: BTreeSet<_> = runners.iter().filter_map(|v| v.family.clone()).collect();
    let scopes: Vec<_> = std::iter::once(LaunchRunnerScope::Any)
        .chain(families.into_iter().map(LaunchRunnerScope::Family))
        .chain(
            runners
                .iter()
                .map(|v| LaunchRunnerScope::Runner(v.id.clone())),
        )
        .collect();
    let mut append = |layer: LaunchLayer, title: String, body: &PlaceBody| {
        for scope in &scopes {
            let authored = match scope {
                LaunchRunnerScope::Any => {
                    let mut v = serde_json::to_value(body).unwrap();
                    v.as_object_mut().unwrap().remove("families");
                    v.as_object_mut().unwrap().remove("runners");
                    v
                }
                LaunchRunnerScope::Family(id) => body
                    .families
                    .get(id)
                    .map(|v| serde_json::to_value(v).unwrap())
                    .unwrap_or_else(|| serde_json::json!({})),
                LaunchRunnerScope::Runner(id) => body
                    .runners
                    .get(id)
                    .map(|v| serde_json::to_value(v).unwrap())
                    .unwrap_or_else(|| serde_json::json!({})),
            };
            let mut kinds = vec![
                "cpu".into(),
                "gpu".into(),
                "modifiers".into(),
                "services".into(),
            ];
            if matches!(scope, LaunchRunnerScope::Any) {
                kinds.push("runner".into());
            } else {
                kinds.push("settings".into());
            }
            places.push(LaunchPlaceSnapshot {
                place: LaunchPlace {
                    layer: layer.clone(),
                    scope: scope.clone(),
                },
                title: title.clone(),
                body: authored,
                editable_kinds: kinds,
            });
        }
    };
    for (layer, title, body) in layers.into_iter().flatten() {
        append(layer, title, body);
    }
    if game_id.is_none() {
        // Match the existing System write boundary in store::set: authored
        // systems and systems referenced by releases, without choosing a game.
        let systems: BTreeSet<_> = config
            .systems
            .keys()
            .chain(
                config
                    .library
                    .values()
                    .flat_map(|g| &g.releases)
                    .filter_map(|r| r.system.as_ref().map(|s| &s.0)),
            )
            .collect();
        for id in systems {
            append(
                LaunchLayer::System(id.clone()),
                id.clone(),
                config.systems.get(id).map(|v| &v.place).unwrap_or(&empty),
            );
        }
    }
    if let Some((id, g)) = game_id.zip(game) {
        for (pid, p) in &g.profiles {
            append(
                LaunchLayer::Profile {
                    game_id: id.into(),
                    profile_id: pid.clone(),
                },
                p.title.0.clone(),
                &p.place,
            );
        }
    }
    Ok(LaunchSnapshot {
        places,
        resolved,
        runners,
        runner_resolved,
        modifiers,
        services,
        profiles: game
            .map(|g| {
                g.profiles
                    .iter()
                    .map(|(id, p)| LaunchProfileSnapshot {
                        id: id.clone(),
                        title: p.title.0.clone(),
                        body: serde_json::to_value(&p.place).unwrap(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        selected_profile: game.and_then(|g| g.profile.as_ref()).map(|v| v.0.clone()),
        device_facts: facts,
        notices,
        route_layers,
        revisions,
    })
}

pub fn apply_device(root: &Path, client: &helper::Client) -> Result<(), String> {
    let (snapshot, _) = settings::runner_choice_snapshot(root).map_err(|e| e.to_string())?;
    let facts = client.describe()?;
    let mut only = snapshot.clone();
    only.library.clear();
    only.systems.clear();
    if let Some(host) = &mut only.host {
        host.place.families.clear();
        host.place.runners.clear();
    }
    let resolved = resolve_launch(&only, &context(&only, None), None, Some(&facts))
        .map_err(|e| e.to_string())?;
    let value = |key: &str| resolved.values.values[key].value.as_u64().unwrap() as u8;
    client.apply_device(&LaunchDeviceDefaults {
        cpu: LaunchPercentRange {
            min: value("cpu.min"),
            max: value("cpu.max"),
        },
        gpu: LaunchPercentRange {
            min: value("gpu.min"),
            max: value("gpu.max"),
        },
        governor: resolved.limits.governor.unwrap_or(facts.defaults.governor),
    })
}
