use super::resolver::*;
use crate::{
    config::{ConfigSnapshot, Location},
    plugin::PluginRegistry,
};
use std::path::Path;

/// An explicit choice names a runtime, never a program family. Without one,
/// only a single installed compatible route is selectable. UI choice storage
/// and configuration cascade are separate consumers of this API.
pub fn resolve_linux_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    game_id: &str,
    chosen_runtime: Option<&str>,
) -> Result<ResolvedRoute, RouteUnavailable> {
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
    for key in &game.releases {
        let Some(release) = snapshot.releases.get(&key.0) else {
            continue;
        };
        let runtimes: Vec<_> = registry
            .runtimes()
            .values()
            .filter(|runtime| {
                runtime.launcher.is_some()
                    && chosen_runtime.is_none_or(|id| id == runtime.id)
                    && runtime
                        .supports
                        .as_ref()
                        .and_then(|supports| supports.systems.as_ref())
                        .is_some_and(|systems| systems.contains(&release.system.0))
            })
            .collect();
        let runtime = match runtimes.as_slice() {
            [runtime] => *runtime,
            [] => continue,
            _ => {
                return Err(failure(format!(
                    "choose a runtime for system {}: {}",
                    release.system.0,
                    runtimes
                        .iter()
                        .map(|runtime| runtime.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )))
            }
        };
        let launcher_id = runtime
            .launcher
            .as_deref()
            .expect("filtered native runtime");
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
        if snapshot.launchers.contains_key(launcher_id)
            || snapshot.systems.contains_key(&system.id)
            || snapshot.providers.contains_key(provider_id)
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
            if let Err(error) = crate::config::storage::resolve_file_target(root, snapshot, &target)
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
            return Ok(ResolvedRoute {
                playable_id: game_id.into(),
                title: Some(game.title.clone()),
                release_id: key.0.clone(),
                identity: (game.releases.len() == 1 && key.0.starts_with("sha256:"))
                    .then(|| crate::GameIdentity::Hash(key.0.clone())),
                provider_id: launcher_id
                    .split_once('/')
                    .expect("validated identity")
                    .0
                    .into(),
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
        }
    }
    Err(last_error)
}
