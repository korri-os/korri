use super::resolver::*;
use crate::{
    config::{ConfigSnapshot, Location},
    plugin::PluginRegistry,
};
use std::{collections::BTreeSet, path::Path};

pub fn linux_route_candidates(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    game_id: &str,
) -> Result<Vec<ResolvedRoute>, RouteUnavailable> {
    let failure = |message: String| RouteDiagnostic {
        code: RouteDiagnosticCode::LocalRouteUnavailable,
        message,
        playable_id: Some(game_id.into()),
    };
    let game = snapshot
        .games
        .get(game_id)
        .ok_or_else(|| failure(format!("game {game_id} is unavailable")))?;
    let mut last_error = failure(format!("game {game_id} has no installed Linux runner"));
    let mut candidates = Vec::new();
    let mut emitted = BTreeSet::new();
    for key in &game.releases {
        let Some(release) = snapshot.releases.get(&key.0) else {
            continue;
        };
        for runner in registry.runners().values().filter(|runner| {
            runner.program.is_some()
                && runner
                    .systems
                    .as_ref()
                    .is_some_and(|systems| systems.contains(&release.system.0))
        }) {
            if emitted.contains(&runner.id) {
                continue;
            }
            let program = registry
                .installed_file(
                    &runner.id,
                    runner.program.as_deref().expect("filtered native runner"),
                )
                .map_err(|error| failure(error.to_string()))?;
            let core_path = runner
                .core
                .as_ref()
                .map(|core| registry.installed_file(&runner.id, core))
                .transpose()
                .map_err(|error| failure(error.to_string()))?
                .map(|path| path.display().to_string());
            let system_title = registry
                .systems()
                .values()
                .find(|system| system.id == release.system.0)
                .and_then(|system| system.title.clone())
                .or_else(|| {
                    snapshot
                        .systems
                        .get(&release.system.0)
                        .and_then(|system| system.title.clone().or_else(|| system.name.clone()))
                });
            for location in snapshot.locations.get(&key.0).into_iter().flatten() {
                let Location::File { storage, path, .. } = location else {
                    continue;
                };
                let target = ResolvedFileTarget {
                    storage_id: storage.0.clone(),
                    path: path.0.clone(),
                };
                if let Err(error) =
                    crate::config::storage::resolve_file_target(root, snapshot, &target)
                {
                    last_error = RouteDiagnostic {
                        code: if error.is_missing_target() {
                            RouteDiagnosticCode::LocalRomMissing
                        } else {
                            RouteDiagnosticCode::LocalRouteUnavailable
                        },
                        message: error.to_string(),
                        playable_id: Some(game_id.into()),
                    };
                    continue;
                }
                emitted.insert(runner.id.clone());
                candidates.push(ResolvedRoute {
                    playable_id: game_id.into(),
                    title: Some(game.title.clone()),
                    release_id: key.0.clone(),
                    identity: (game.releases.len() == 1 && key.0.starts_with("sha256:"))
                        .then(|| crate::GameIdentity::Hash(key.0.clone())),
                    provider_id: runner
                        .family
                        .clone()
                        .unwrap_or_else(|| runner.id.split_once('/').unwrap().0.into()),
                    system_id: release.system.0.clone(),
                    system_title,
                    runner_id: runner.id.clone(),
                    family_id: runner.family.clone(),
                    integration_token: String::new(),
                    flattened_target: format!("{}:{}", storage.0, path.0),
                    linux_runner: Some(ResolvedLinuxRunner {
                        program: program.display().to_string(),
                    }),
                    core_path: core_path.clone(),
                    file_target: Some(target),
                });
                break;
            }
        }
    }
    if candidates.is_empty() {
        Err(last_error)
    } else {
        Ok(candidates)
    }
}

pub fn stored_runner<'a>(snapshot: &'a ConfigSnapshot, route: &ResolvedRoute) -> Option<&'a str> {
    snapshot
        .games
        .get(&route.playable_id)
        .and_then(|game| game.runner.as_ref())
        .or_else(|| {
            snapshot
                .systems
                .get(&route.system_id)
                .and_then(|system| system.runner.as_ref())
        })
        .map(|id| id.0.as_str())
}

pub fn resolve_linux_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    game_id: &str,
    chosen_runner: Option<&str>,
) -> Result<ResolvedRoute, RouteUnavailable> {
    let candidates = linux_route_candidates(root, snapshot, registry, game_id)?;
    let selected: Vec<_> = candidates
        .iter()
        .filter(|route| {
            let choice = chosen_runner.or_else(|| stored_runner(snapshot, route));
            match choice {
                Some(id) => route.runner_id == id,
                None => candidates.len() == 1,
            }
        })
        .collect();
    if let [route] = selected.as_slice() {
        return Ok((*route).clone());
    }
    Err(RouteDiagnostic {
        code: RouteDiagnosticCode::LocalRouteUnavailable,
        message: format!(
            "choose a runner for game {game_id}: {}",
            candidates
                .iter()
                .map(|route| route.runner_id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        playable_id: Some(game_id.into()),
    })
}
