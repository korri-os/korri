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
    let mut last_error = failure(format!("game {game_id} has no installed Linux route"));
    let mut candidates = Vec::new();
    let mut emitted = BTreeSet::new();
    for key in &game.releases {
        let Some(release) = snapshot.releases.get(&key.0) else {
            continue;
        };
        for runtime in registry.runtimes().values().filter(|runtime| {
            runtime.launcher.is_some()
                && runtime
                    .supports
                    .as_ref()
                    .and_then(|supports| supports.systems.as_ref())
                    .is_some_and(|systems| systems.contains(&release.system.0))
        }) {
            if emitted.contains(&runtime.id) {
                continue;
            }
            let launcher_id = runtime.launcher.as_deref().expect("native runtime");
            let (launcher, kind) = registry
                .native_launcher(launcher_id)
                .map_err(|e| failure(e.to_string()))?;
            let program = registry
                .installed_file(
                    launcher_id,
                    launcher.program.as_deref().expect("validated program"),
                )
                .map_err(|e| failure(e.to_string()))?;
            let runtime_path = registry
                .installed_file(&runtime.id, &runtime.path)
                .map_err(|e| failure(e.to_string()))?;
            let system = registry
                .systems()
                .values()
                .find(|system| system.id == release.system.0)
                .ok_or_else(|| failure(format!("system {} is unavailable", release.system.0)))?;
            let provider_id = launcher_id.split_once('/').expect("validated identity").0;
            // Configuration blocks are opinions, not a second contribution.
            if snapshot.launchers.get(launcher_id).is_some_and(|value| {
                value.plugin.is_some() || value.command.is_some() || value.systems.is_some()
            }) || snapshot.providers.contains_key(provider_id)
            {
                return Err(RouteDiagnostic {
                    code: RouteDiagnosticCode::LocalRouteCollision,
                    message: format!(
                        "installed route {launcher_id} collides with device configuration"
                    ),
                    playable_id: Some(game_id.into()),
                });
            }
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
                emitted.insert(runtime.id.clone());
                candidates.push(ResolvedRoute {
                    playable_id: game_id.into(),
                    title: Some(game.title.clone()),
                    release_id: key.0.clone(),
                    identity: (game.releases.len() == 1 && key.0.starts_with("sha256:"))
                        .then(|| crate::GameIdentity::Hash(key.0.clone())),
                    provider_id: provider_id.into(),
                    system_id: system.id.clone(),
                    system_title: system.title.clone(),
                    launcher_id: launcher_id.into(),
                    launcher_kind: kind.id.clone(),
                    integration_token: String::new(),
                    flattened_target: format!("{}:{}", storage.0, path.0),
                    android_component: None,
                    linux_launcher: Some(ResolvedLinuxLauncher {
                        program: program.display().to_string(),
                    }),
                    runtime: Some(ResolvedRuntime {
                        id: runtime.id.clone(),
                        kind: runtime.kind.clone(),
                        app: launcher_id.into(),
                        path: runtime_path.display().to_string(),
                    }),
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

pub fn stored_runtime<'a>(snapshot: &'a ConfigSnapshot, route: &ResolvedRoute) -> Option<&'a str> {
    snapshot
        .games
        .get(&route.playable_id)
        .and_then(|game| game.runtime.as_ref())
        .or_else(|| {
            snapshot
                .systems
                .get(&route.system_id)
                .and_then(|system| system.runtime.as_ref())
        })
        .map(|id| id.0.as_str())
}

pub fn resolve_linux_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    game_id: &str,
    chosen_runtime: Option<&str>,
) -> Result<ResolvedRoute, RouteUnavailable> {
    let candidates = linux_route_candidates(root, snapshot, registry, game_id)?;
    let selected: Vec<_> = candidates
        .iter()
        .filter(|route| {
            let choice = chosen_runtime.or_else(|| stored_runtime(snapshot, route));
            match choice {
                Some(id) => route
                    .runtime
                    .as_ref()
                    .is_some_and(|runtime| runtime.id == id),
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
            "choose a runtime for game {game_id}: {}",
            candidates
                .iter()
                .filter_map(|route| route.runtime.as_ref().map(|runtime| runtime.id.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        playable_id: Some(game_id.into()),
    })
}
