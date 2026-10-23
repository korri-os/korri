//! Pure launch resolution shared by preparation, limits and hooks.
use super::*;
use crate::config::{
    cascade::RunnerConfig,
    modifiers::ModifierConfigs,
    place::{PlaceBody, PlaceValues},
    resolver::ResolvedRoute,
    ConfigSnapshot, HookAfterStep, HookBeforeStep,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResolvedHooks {
    /// Merge order. The host executes after steps in reverse order.
    pub before: Vec<HookBeforeStep>,
    pub after: Vec<HookAfterStep>,
}
#[derive(Clone, Debug)]
pub struct ResolvedLaunch {
    /// Only authored settings are passed to the runner, not schema defaults.
    pub runner: RunnerConfig,
    pub modifiers: ModifierConfigs,
    pub env: BTreeMap<String, Option<String>>,
    pub args: LaunchArgs,
    pub cwd: Option<String>,
    pub hooks: ResolvedHooks,
    pub limits: LaunchSettingsResolved,
    pub values: LaunchResolvedSnapshot,
}
pub(crate) struct Cell<'a> {
    pub place: LaunchPlace,
    pub values: &'a PlaceValues,
    pub settings: Option<&'a RunnerConfig>,
    pub runner: Option<&'a str>,
}
fn cell<'a>(layer: LaunchLayer, body: &'a PlaceBody) -> Cell<'a> {
    Cell {
        place: LaunchPlace {
            layer,
            scope: LaunchRunnerScope::Any,
        },
        values: &body.values,
        settings: None,
        runner: body.runner.as_ref().map(|v| v.0.as_str()),
    }
}
fn scoped<'a>(layer: LaunchLayer, body: &'a PlaceBody, route: &ResolvedRoute) -> Vec<Cell<'a>> {
    let mut cells = Vec::new();
    if let Some((id, v)) = route
        .family_id
        .as_ref()
        .and_then(|id| body.families.get(id).map(|v| (id, v)))
    {
        cells.push(Cell {
            place: LaunchPlace {
                layer: layer.clone(),
                scope: LaunchRunnerScope::Family(id.clone()),
            },
            values: &v.values,
            settings: Some(v),
            runner: None,
        });
    }
    if let Some(v) = body.runners.get(&route.runner_id) {
        cells.push(Cell {
            place: LaunchPlace {
                layer,
                scope: LaunchRunnerScope::Runner(route.runner_id.clone()),
            },
            values: &v.values,
            settings: Some(v),
            runner: None,
        });
    }
    cells
}
/// This table owns the order for every setting kind, including runner choice.
pub(crate) fn cells<'a>(
    snapshot: &'a ConfigSnapshot,
    route: &ResolvedRoute,
    launch: Option<&'a PlaceBody>,
) -> Vec<Cell<'a>> {
    let device = snapshot.host.as_ref().map(|v| &v.place);
    let system = snapshot.systems.get(&route.system_id).map(|v| &v.place);
    let game = snapshot.library.get(&route.playable_id);
    let release = game.and_then(|g| {
        g.releases.iter().find(|v| {
            v.identity
                .as_ref()
                .is_some_and(|id| id.value() == route.release_id)
        })
    });
    let profile = game.and_then(|g| {
        g.profile
            .as_ref()
            .and_then(|id| g.profiles.get(&id.0).map(|p| (&id.0, p)))
    });
    let mut result = Vec::new();
    if let Some(v) = device {
        result.push(cell(LaunchLayer::Device, v));
    }
    if let Some(v) = system {
        result.push(cell(LaunchLayer::System(route.system_id.clone()), v));
    }
    if let Some(v) = device {
        result.extend(scoped(LaunchLayer::Device, v, route));
    }
    if let Some(v) = system {
        result.extend(scoped(
            LaunchLayer::System(route.system_id.clone()),
            v,
            route,
        ));
    }
    for (layer, body) in [
        game.map(|v| (LaunchLayer::Game(route.playable_id.clone()), &v.place)),
        release.map(|v| {
            (
                LaunchLayer::Release {
                    game_id: route.playable_id.clone(),
                    release_id: v.id.0.clone(),
                },
                &v.inheritable,
            )
        }),
        profile.map(|(id, p)| {
            (
                LaunchLayer::Profile {
                    game_id: route.playable_id.clone(),
                    profile_id: id.clone(),
                },
                &p.place,
            )
        }),
        launch.map(|v| (LaunchLayer::Launch, v)),
    ]
    .into_iter()
    .flatten()
    {
        result.push(cell(layer.clone(), body));
        result.extend(scoped(layer, body, route));
    }
    result
}
pub(crate) fn override_body(
    launch: &LaunchOverride,
) -> Result<PlaceBody, crate::config::settings::SettingsError> {
    let invalid = |s: &str| crate::config::settings::SettingsError::Invalid(s.into());
    for v in [&launch.cpu, &launch.gpu].into_iter().flatten() {
        if v.min.is_some_and(|v| v > 100)
            || v.max.is_some_and(|v| v == 0 || v > 100)
            || matches!((v.min,v.max),(Some(a),Some(b)) if a>b)
        {
            return Err(invalid("invalid launch percent range"));
        }
    }
    let mut body = PlaceBody::default();
    body.values.cpu = launch.cpu.as_ref().map(|v| config::DeviceCpu {
        min: v.min,
        max: v.max,
        governor: None,
    });
    body.values.gpu = launch.gpu.as_ref().map(|v| config::Limits {
        min: v.min,
        max: v.max,
    });
    body.values.modifiers = serde_json::from_value(
        serde_json::to_value(&launch.modifiers).expect("serializable launch modifiers"),
    )
    .map_err(|e| invalid(&e.to_string()))?;
    if let Some(settings) = &launch.settings {
        let record = RunnerConfig {
            settings: settings.values.clone(),
            ..Default::default()
        };
        match &settings.scope {
            LaunchRunnerScope::Runner(id) => {
                crate::config::place::validate_scope_id("runners", id).map_err(|e| invalid(&e))?;
                body.runners.insert(id.clone(), record);
            }
            LaunchRunnerScope::Family(id) => {
                crate::config::place::validate_scope_id("families", id).map_err(|e| invalid(&e))?;
                body.families.insert(id.clone(), record);
            }
            LaunchRunnerScope::Any => {
                return Err(invalid("settings need a runner or family scope"))
            }
        }
    }
    Ok(body)
}
fn bound(value: u8) -> LaunchResolvedValue {
    LaunchResolvedValue {
        value,
        source: LaunchSource::DeviceDefault,
        adjusted: false,
    }
}
fn range(cells: &[Cell<'_>], cpu: bool, session: bool, defaults: (u8, u8)) -> LaunchResolvedRange {
    let mut min = (0, bound(defaults.0));
    let mut max = (0, bound(defaults.1));
    for (index, c) in cells.iter().enumerate() {
        if cpu
            && session
            && c.place
                == (LaunchPlace {
                    layer: LaunchLayer::Device,
                    scope: LaunchRunnerScope::Any,
                })
        {
            continue;
        }
        let (lo, hi) = if cpu {
            (
                c.values.cpu.as_ref().and_then(|v| v.min),
                c.values.cpu.as_ref().and_then(|v| v.max),
            )
        } else {
            (
                c.values.gpu.as_ref().and_then(|v| v.min),
                c.values.gpu.as_ref().and_then(|v| v.max),
            )
        };
        if let Some(value) = lo {
            min = (
                index + 1,
                LaunchResolvedValue {
                    value,
                    source: LaunchSource::Place(c.place.clone()),
                    adjusted: false,
                },
            );
        }
        if let Some(value) = hi {
            max = (
                index + 1,
                LaunchResolvedValue {
                    value,
                    source: LaunchSource::Place(c.place.clone()),
                    adjusted: false,
                },
            );
        }
    }
    if min.1.value > max.1.value {
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
pub(crate) fn sourced(
    values: &mut HashMap<String, LaunchSourcedValue>,
    key: String,
    path: LaunchValuePath,
    value: serde_json::Value,
    source: LaunchSource,
) {
    values.insert(
        key,
        LaunchSourcedValue {
            path,
            value,
            source,
            adjusted: false,
        },
    );
}
fn put(
    values: &mut HashMap<String, LaunchSourcedValue>,
    key: &str,
    path: LaunchValuePath,
    value: impl Serialize,
    place: &LaunchPlace,
) {
    sourced(
        values,
        key.into(),
        path,
        serde_json::to_value(value).expect("serializable authored value"),
        LaunchSource::Place(place.clone()),
    );
}

pub fn resolve_launch(
    snapshot: &ConfigSnapshot,
    route: &ResolvedRoute,
    launch: Option<&LaunchOverride>,
    facts: Option<&LaunchDeviceFacts>,
) -> Result<ResolvedLaunch, crate::config::settings::SettingsError> {
    let launch = launch.map(override_body).transpose()?;
    let cells = cells(snapshot, route, launch.as_ref());
    let mut runner = RunnerConfig::default();
    let mut modifiers = ModifierConfigs::default();
    let mut env = BTreeMap::new();
    let mut args = LaunchArgs::default();
    let mut cwd = None;
    let mut hooks = ResolvedHooks::default();
    let mut values = HashMap::new();
    for cell in &cells {
        let v = cell.values;
        let p = &cell.place;
        if let Some(id) = cell.runner.filter(|id| *id == route.runner_id) {
            put(
                &mut values,
                "runner",
                LaunchValuePath::Editable(LaunchPath::Runner),
                id,
                p,
            );
        }
        if let Some(v) = cell.settings {
            for (key, value) in &v.settings {
                runner.settings.insert(key.clone(), value.clone());
                put(
                    &mut values,
                    &format!("settings.{key}"),
                    LaunchValuePath::Editable(LaunchPath::Setting { key: key.clone() }),
                    value,
                    p,
                );
            }
            if let Some(v) = &v.config {
                let target = runner.config.get_or_insert_with(Default::default);
                if v.prepend.is_some() {
                    target.prepend = v.prepend.clone();
                }
                if v.append.is_some() {
                    target.append = v.append.clone();
                }
                if v.replace.is_some() {
                    target.replace = v.replace.clone();
                }
                put(&mut values, "config", LaunchValuePath::Config, &target, p);
            }
        }
        for (id, record) in v.modifiers.iter() {
            let target = modifiers.entry(id.clone()).or_default();
            if let Some(enabled) = record.enabled {
                target.enabled = Some(enabled);
                put(
                    &mut values,
                    &format!("modifiers.{id}.enabled"),
                    LaunchValuePath::Editable(LaunchPath::ModifierEnabled { id: id.clone() }),
                    enabled,
                    p,
                );
            }
            for (key, value) in &record.settings {
                target.settings.insert(key.clone(), value.clone());
                put(
                    &mut values,
                    &format!("modifiers.{id}.settings.{key}"),
                    LaunchValuePath::Editable(LaunchPath::ModifierSetting {
                        id: id.clone(),
                        key: key.clone(),
                    }),
                    value,
                    p,
                );
            }
            if let Some(v) = record.config.as_ref().and_then(|v| v.append.as_ref()) {
                target.config.get_or_insert_with(Default::default).append = Some(v.clone());
                put(
                    &mut values,
                    &format!("modifiers.{id}.config"),
                    LaunchValuePath::ModifierConfig { id: id.clone() },
                    &target.config,
                    p,
                );
            }
        }
        for (key, value) in &v.env {
            env.insert(key.clone(), value.clone());
            put(
                &mut values,
                &format!("env.{key}"),
                LaunchValuePath::Env { key: key.clone() },
                value,
                p,
            );
        }
        if !v.args.prepend.is_empty() {
            args.prepend.extend(v.args.prepend.clone());
            put(
                &mut values,
                "args.prepend",
                LaunchValuePath::ArgsPrepend,
                &args.prepend,
                p,
            );
        }
        if !v.args.append.is_empty() {
            args.append.extend(v.args.append.clone());
            put(
                &mut values,
                "args.append",
                LaunchValuePath::ArgsAppend,
                &args.append,
                p,
            );
        }
        if let Some(v) = &v.cwd {
            cwd = Some(v.clone());
            put(&mut values, "cwd", LaunchValuePath::Cwd, v, p);
        }
        if let Some(v) = &v.hooks {
            for name in v.use_profiles.iter().flatten() {
                let routine = snapshot.routines.get(&name.0).ok_or_else(|| {
                    crate::config::settings::SettingsError::Invalid(format!(
                        "unknown routine {}",
                        name.0
                    ))
                })?;
                hooks
                    .before
                    .extend(routine.before.iter().flatten().cloned());
                hooks.after.extend(routine.after.iter().flatten().cloned());
            }
            hooks.before.extend(v.before.iter().flatten().cloned());
            hooks.after.extend(v.after.iter().flatten().cloned());
            if !hooks.before.is_empty() {
                put(
                    &mut values,
                    "hooks.before",
                    LaunchValuePath::HooksBefore,
                    &hooks.before,
                    p,
                );
            }
            if !hooks.after.is_empty() {
                put(
                    &mut values,
                    "hooks.after",
                    LaunchValuePath::HooksAfter,
                    &hooks.after,
                    p,
                );
            }
        }
    }
    let defaults = facts.map(|v| &v.defaults);
    let cpu = range(
        &cells,
        true,
        false,
        defaults.map(|v| (v.cpu.min, v.cpu.max)).unwrap_or((0, 100)),
    );
    let gpu = range(
        &cells,
        false,
        false,
        defaults.map(|v| (v.gpu.min, v.gpu.max)).unwrap_or((0, 100)),
    );
    for (key, path, v) in [
        ("cpu.min", LaunchPath::CpuMin, &cpu.min),
        ("cpu.max", LaunchPath::CpuMax, &cpu.max),
        ("gpu.min", LaunchPath::GpuMin, &gpu.min),
        ("gpu.max", LaunchPath::GpuMax, &gpu.max),
    ] {
        sourced(
            &mut values,
            key.into(),
            LaunchValuePath::Editable(path),
            v.value.into(),
            v.source.clone(),
        );
        values.get_mut(key).unwrap().adjusted = v.adjusted;
    }
    let governor = snapshot
        .host
        .as_ref()
        .and_then(|v| v.cpu.as_ref())
        .and_then(|v| v.governor.as_ref())
        .map(|v| v.0.clone())
        .or_else(|| defaults.map(|v| v.governor.clone()));
    if let Some(v) = &governor {
        sourced(
            &mut values,
            "cpu.governor".into(),
            LaunchValuePath::Editable(LaunchPath::CpuGovernor),
            v.clone().into(),
            if snapshot
                .host
                .as_ref()
                .and_then(|v| v.cpu.as_ref())
                .is_some_and(|v| v.governor.is_some())
            {
                LaunchSource::Place(LaunchPlace {
                    layer: LaunchLayer::Device,
                    scope: LaunchRunnerScope::Any,
                })
            } else {
                LaunchSource::DeviceDefault
            },
        );
    }
    let limits = LaunchSettingsResolved {
        cpu: range(&cells, true, true, (0, 100)),
        gpu,
        governor,
    };
    Ok(ResolvedLaunch {
        runner,
        modifiers,
        env,
        args,
        cwd,
        hooks,
        limits,
        values: LaunchResolvedSnapshot { values },
    })
}

/// Runner choice precedes every runner-scoped setting. Missing opinions are
/// notices, not a reason to discard a less-specific available route.
pub(crate) fn choose_runner(
    snapshot: &ConfigSnapshot,
    candidates: &[ResolvedRoute],
    chosen: Option<&str>,
) -> (Option<String>, Vec<LaunchNotice>) {
    if let Some(id) = chosen {
        return (
            candidates
                .iter()
                .find(|v| v.runner_id == id)
                .map(|v| v.runner_id.clone()),
            Vec::new(),
        );
    }
    let mut notices = Vec::new();
    for rank in (0..5).rev() {
        let mut choices = std::collections::BTreeSet::new();
        for route in candidates {
            for c in cells(snapshot, route, None) {
                let r = match c.place.layer {
                    LaunchLayer::Device => 0,
                    LaunchLayer::System(_) => 1,
                    LaunchLayer::Game(_) => 2,
                    LaunchLayer::Release { .. } => 3,
                    LaunchLayer::Profile { .. } => 4,
                    LaunchLayer::Launch => 5,
                };
                if r != rank {
                    continue;
                }
                if let Some(id) = c.runner {
                    if candidates.iter().any(|v|v.runner_id==id){choices.insert(id.to_string());}
                    else if !notices.iter().any(|v|matches!(v,LaunchNotice::RunnerUnavailable {runner_id,place} if runner_id==id && place==&c.place)){notices.push(LaunchNotice::RunnerUnavailable {runner_id:id.into(),place:c.place});}
                }
            }
        }
        if choices.len() == 1 {
            return (choices.into_iter().next(), notices);
        }
        if choices.len() > 1 {
            return (None, notices);
        }
    }
    (
        (candidates.len() == 1).then(|| candidates[0].runner_id.clone()),
        notices,
    )
}
