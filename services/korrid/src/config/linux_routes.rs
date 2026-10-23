use super::resolver::*;
use crate::{
    config::{ConfigSnapshot, Location},
    plugin::{DeclaredReleaseFile, PluginRegistry},
};
use std::{collections::BTreeSet, path::Path};

pub fn linux_route_candidates(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    game_id: &str,
) -> Result<Vec<ResolvedRoute>, RouteUnavailable> {
    linux_route_candidates_for_runner(root, snapshot, registry, game_id, None)
}

fn linux_route_candidates_for_runner(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    game_id: &str,
    chosen_runner: Option<&str>,
) -> Result<Vec<ResolvedRoute>, RouteUnavailable> {
    let failure = |message: String| RouteDiagnostic {
        code: RouteDiagnosticCode::LocalRouteUnavailable,
        message,
        playable_id: Some(game_id.into()),
    };
    let game = snapshot
        .library
        .get(game_id)
        .ok_or_else(|| failure(format!("game {game_id} is unavailable")))?;
    let mut last_error = failure(format!("game {game_id} has no installed Linux runner"));
    let mut candidates = Vec::new();
    let mut emitted = BTreeSet::new();
    for release in &game.releases {
        let Some(key) = release.identity.as_ref().map(|i| i.value()) else {
            continue;
        };
        let system_id = release.system.as_ref().map(|s| s.0.as_str()).unwrap_or("");
        for runner in registry.runners().values().filter(|runner| {
            runner.program.is_some()
                && chosen_runner.is_none_or(|id| runner.id == id)
                && match (&runner.releases, &runner.systems) {
                    (Some(releases), _) => releases.iter().any(|candidate| candidate.0 == key),
                    (None, Some(systems)) => systems.iter().any(|s| s == system_id),
                    (None, None) => false,
                }
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
                .find(|system| system.id == system_id)
                .and_then(|system| system.title.clone())
                .or_else(|| {
                    snapshot
                        .systems
                        .get(system_id)
                        .and_then(|system| system.title.clone().or_else(|| system.name.clone()))
                });
            let route = |file_target,
                         provider_target,
                         artifact_target,
                         package_target,
                         cache_target,
                         flattened_target| ResolvedRoute {
                playable_id: game_id.into(),
                title: game.title.clone(),
                release_id: key.into(),
                identity: snapshot.game_identity(game_id),
                provider_id: runner
                    .family
                    .clone()
                    .unwrap_or_else(|| runner.id.split_once('/').unwrap().0.into()),
                system_id: system_id.into(),
                system_title: system_title.clone(),
                runner_id: runner.id.clone(),
                family_id: runner.family.clone(),
                integration_token: String::new(),
                flattened_target,
                linux_runner: Some(ResolvedLinuxRunner {
                    program: program.display().to_string(),
                }),
                core_path: core_path.clone(),
                file_target,
                provider_target,
                artifact_target,
                package_target,
                cache_target,
            };
            // A saved location is tried before the declaration's artifact.
            let mut routed = false;
            for location in snapshot.locations.get(key).into_iter().flatten() {
                let (file_target, provider_target, flattened_target) =
                    if let Location::ProviderRef {
                        provider,
                        provider_ref,
                    } = location
                    {
                        (
                            None,
                            Some(crate::GameProviderIdentity {
                                provider: provider.0.clone(),
                                provider_ref: provider_ref.0.clone(),
                            }),
                            format!("{}/{}", provider.0, provider_ref.0),
                        )
                    } else {
                        let (storage, path, paths, discovery) = match location {
                            Location::File {
                                storage,
                                path,
                                discovery,
                            } => (storage, path.0.clone(), vec![path.0.clone()], discovery),
                            Location::FileSet {
                                storage,
                                root,
                                files,
                                discovery,
                            } => {
                                let paths: Vec<_> = files
                                    .0
                                    .iter()
                                    .map(|p| {
                                        root.as_ref()
                                            .map(|r| format!("{}/{}", r.0, p.path.0))
                                            .unwrap_or_else(|| p.path.0.clone())
                                    })
                                    .collect();
                                let primary = files
                                    .0
                                    .iter()
                                    .position(|p| {
                                        p.role.as_ref().is_some_and(|r| r.0 == "descriptor")
                                    })
                                    .unwrap_or(0);
                                (storage, paths[primary].clone(), paths, discovery)
                            }
                            _ => continue,
                        };
                        if discovery
                            .as_ref()
                            .is_some_and(|d| d.missing_since.is_some())
                        {
                            last_error = RouteDiagnostic {
                                code: RouteDiagnosticCode::LocalRomMissing,
                                message: format!("release {key} location is missing"),
                                playable_id: Some(game_id.into()),
                            };
                            continue;
                        }
                        let target = ResolvedFileTarget {
                            storage_id: storage.0.clone(),
                            path: path.clone(),
                        };
                        if let Some(error) = paths.iter().find_map(|path| {
                            crate::config::storage::resolve_file_target(
                                root,
                                snapshot,
                                &ResolvedFileTarget {
                                    storage_id: storage.0.clone(),
                                    path: path.clone(),
                                },
                            )
                            .err()
                        }) {
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
                        (Some(target), None, format!("{}:{}", storage.0, path))
                    };
                emitted.insert(runner.id.clone());
                candidates.push(route(
                    file_target,
                    provider_target,
                    None,
                    None,
                    None,
                    flattened_target,
                ));
                routed = true;
                break;
            }
            // ADR 0013: no usable saved location, but an enabled plugin
            // declares this release's content file inside an artifact.
            if !routed {
                if let Some(target) = artifact_target(registry, key) {
                    let flattened = format!("{}:{}", target.artifact_id, target.path);
                    emitted.insert(runner.id.clone());
                    candidates.push(route(None, None, Some(target), None, None, flattened));
                    routed = true;
                } else if let Some(target) = package_target(registry, key) {
                    let path = package_content_path(registry, &target)
                        .map_err(|error| failure(error.to_string()))?;
                    emitted.insert(runner.id.clone());
                    candidates.push(route(
                        None,
                        None,
                        None,
                        Some(target),
                        None,
                        path.display().to_string(),
                    ));
                    routed = true;
                }
            }
            // ADR 0016: no location and no declaration, but a provider linked
            // to this game holds the release's download in its cache.
            if !routed {
                if let Some(path) = crate::acquisition::acquired_game_content(
                    root, registry, snapshot, game_id, key,
                ) {
                    let flattened = format!("cache:{}", path.display());
                    emitted.insert(runner.id.clone());
                    candidates.push(route(None, None, None, None, Some(path), flattened));
                }
            }
        }
    }
    if candidates.is_empty() {
        Err(last_error)
    } else {
        Ok(candidates)
    }
}

/// The first enabled declaration of this release key whose file is in an
/// artifact. Readiness is not checked here; the launch gate checks it.
fn artifact_target(registry: &PluginRegistry, key: &str) -> Option<ResolvedArtifactTarget> {
    registry
        .declared_hash(key)
        .into_iter()
        .find_map(|declared| {
            let file = match &declared.release.file {
                Some(DeclaredReleaseFile::Artifact(file)) => file,
                // A prepared file reaches the library as a saved location,
                // which the location loop has already tried.
                Some(DeclaredReleaseFile::AccountRoot(_) | DeclaredReleaseFile::Package(_))
                | None => return None,
            };
            Some(ResolvedArtifactTarget {
                artifact_id: file.artifact.clone(),
                path: file.path.clone(),
                release_artifacts: declared.release.artifacts.clone(),
            })
        })
}

fn package_target(registry: &PluginRegistry, key: &str) -> Option<ResolvedPackageTarget> {
    registry
        .declared_hash(key)
        .into_iter()
        .find_map(|declared| match &declared.release.file {
            Some(DeclaredReleaseFile::Package(file)) => Some(ResolvedPackageTarget {
                declared_game_id: format!("{}/{}", declared.game.provider, declared.game.key),
                file: file.clone(),
            }),
            Some(DeclaredReleaseFile::AccountRoot(_) | DeclaredReleaseFile::Artifact(_)) | None => {
                None
            }
        })
}

pub(crate) fn package_content_path(
    registry: &PluginRegistry,
    target: &ResolvedPackageTarget,
) -> Result<std::path::PathBuf, crate::plugin::PluginError> {
    Ok(registry
        .installed_file(&target.declared_game_id, &target.file.package_file)?
        .join(&target.file.path))
}

pub fn stored_runner<'a>(snapshot: &'a ConfigSnapshot, route: &ResolvedRoute) -> Option<&'a str> {
    crate::launch_settings::launch::cells(snapshot, route, None)
        .into_iter()
        .rev()
        .find_map(|c| c.runner)
}

pub(crate) enum LinuxRouteSelection {
    Selected(String),
    Choose,
}
pub(crate) fn linux_route_selection(
    snapshot: &ConfigSnapshot,
    candidates: &[ResolvedRoute],
    chosen_runner: Option<&str>,
) -> Result<LinuxRouteSelection, RouteUnavailable> {
    let (selected, _) =
        crate::launch_settings::launch::choose_runner(snapshot, candidates, chosen_runner);
    if chosen_runner.is_some() && selected.is_none() {
        return Err(RouteDiagnostic {
            code: RouteDiagnosticCode::LocalRouteUnavailable,
            message: "selected runner is unavailable".into(),
            playable_id: candidates.first().map(|v| v.playable_id.clone()),
        });
    }
    Ok(match selected {
        Some(id) => LinuxRouteSelection::Selected(id),
        None => LinuxRouteSelection::Choose,
    })
}

pub fn resolve_linux_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    game_id: &str,
    chosen_runner: Option<&str>,
) -> Result<ResolvedRoute, RouteUnavailable> {
    // Explicit selection retains its missing-file diagnostic even when another
    // runner can play. Catalog discovery does not require this single route.
    let candidates =
        linux_route_candidates_for_runner(root, snapshot, registry, game_id, chosen_runner)?;
    match linux_route_selection(snapshot, &candidates, chosen_runner)? {
        LinuxRouteSelection::Selected(id) => Ok(candidates
            .into_iter()
            .find(|route| route.runner_id == id)
            .expect("selection names an available route")),
        LinuxRouteSelection::Choose => Err(RouteDiagnostic {
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
        }),
    }
}
