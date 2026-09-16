use std::{collections::BTreeSet, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    plugin::{
        AndroidTransportImplementation, PluginRegistry, SessionControlExecutor,
        SessionControlOwnerKind, SessionControlPlatform, SessionControlRecord,
    },
    GameIdentity,
};

pub use super::linux_routes::{linux_route_candidates, resolve_linux_route, stored_runner};
use super::{storage, ConfigSnapshot, GamePayload, Location};

const ANDROID_APP_COMMAND: &str = "android-app";
const RETROARCH_COMMAND: &str = "retroarch";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteCatalog {
    pub routes: Vec<ResolvedRoute>,
    pub diagnostics: Vec<RouteDiagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutePlatform {
    Android,
    Linux,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedMoonlightTransport {
    pub transport_id: String,
    pub implementation: AndroidTransportImplementation,
    pub sunshine_app: String,
}

pub fn resolve_moonlight_transport(
    registry: &PluginRegistry,
    platform: RoutePlatform,
) -> Option<ResolvedMoonlightTransport> {
    if platform != RoutePlatform::Android {
        return None;
    }
    let transport = registry.transports().get("@korri:moonlight/moonlight")?;
    let android = transport.android.as_ref()?;
    Some(ResolvedMoonlightTransport {
        transport_id: transport.id.clone(),
        implementation: android.implementation,
        sunshine_app: android.sunshine_app.clone(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteContribution {
    pub kind: SessionControlOwnerKind,
    pub id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionExecutorAvailability {
    available: BTreeSet<SessionControlExecutor>,
}

impl SessionExecutorAvailability {
    pub fn from_available(executors: impl IntoIterator<Item = SessionControlExecutor>) -> Self {
        Self {
            available: executors.into_iter().collect(),
        }
    }

    pub fn is_available(&self, executor: SessionControlExecutor) -> bool {
        self.available.contains(&executor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveRouteContext {
    pub platform: RoutePlatform,
    /// Active runner and transport contributions in explicit route order.
    pub contributors: Vec<RouteContribution>,
    pub executor_availability: SessionExecutorAvailability,
}

pub fn resolve_session_controls(
    registry: &PluginRegistry,
    context: &ActiveRouteContext,
) -> Vec<SessionControlRecord> {
    let mut resolved = Vec::new();
    let mut emitted = BTreeSet::new();
    for contributor in &context.contributors {
        let mut controls: Vec<_> = registry
            .session_controls()
            .values()
            .filter(|control| {
                control.owner.kind == contributor.kind
                    && control.owner.id == contributor.id
                    && matches!(
                        (control.effect.platform(), context.platform),
                        (SessionControlPlatform::Android, RoutePlatform::Android)
                    )
                    && context
                        .executor_availability
                        .is_available(control.effect.executor())
                    && emitted.insert(control.id.clone())
            })
            .cloned()
            .collect();
        controls.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.local_id.cmp(&right.local_id))
        });
        resolved.extend(controls);
    }
    resolved
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAndroidComponent {
    pub package_name: String,
    pub class_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLinuxRunner {
    pub program: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedFileTarget {
    pub storage_id: String,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRoute {
    pub playable_id: String,
    pub title: Option<String>,
    pub release_id: String,
    pub identity: Option<GameIdentity>,
    pub provider_id: String,
    pub system_id: String,
    pub system_title: Option<String>,
    pub runner_id: String,
    pub family_id: Option<String>,
    pub integration_token: String,
    pub flattened_target: String,
    pub android_component: Option<ResolvedAndroidComponent>,
    pub linux_runner: Option<ResolvedLinuxRunner>,
    pub core_path: Option<String>,
    pub file_target: Option<ResolvedFileTarget>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RouteDiagnosticCode {
    LocalRomMissing,
    LocalRouteUnavailable,
    LocalRouteCollision,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RouteDiagnostic {
    pub code: RouteDiagnosticCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playable_id: Option<String>,
}

pub type RouteUnavailable = RouteDiagnostic;

pub fn resolve_launchable_routes<'a>(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    static_playable_ids: impl IntoIterator<Item = &'a str>,
) -> RouteCatalog {
    resolve_launchable_routes_for_platform(
        root,
        snapshot,
        registry,
        static_playable_ids,
        RoutePlatform::Android,
    )
}

pub fn resolve_launchable_routes_for_platform<'a>(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    static_playable_ids: impl IntoIterator<Item = &'a str>,
    platform: RoutePlatform,
) -> RouteCatalog {
    let static_ids: BTreeSet<_> = static_playable_ids.into_iter().collect();
    let mut catalog = RouteCatalog {
        routes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for id in snapshot.games.keys() {
        if static_ids.contains(id.as_str()) {
            catalog.diagnostics.push(static_playable_collision(id));
            continue;
        }
        let result = if platform == RoutePlatform::Linux {
            linux_route_candidates(root, snapshot, registry, id)
        } else {
            resolve_android_route(root, snapshot, registry, id).map(|route| vec![route])
        };
        match result {
            Ok(routes) => catalog.routes.extend(routes),
            Err(error) => catalog.diagnostics.push(error),
        }
    }
    catalog
}

pub fn resolve_route<'a>(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    static_playable_ids: impl IntoIterator<Item = &'a str>,
    playable_id: &str,
) -> Result<ResolvedRoute, RouteUnavailable> {
    resolve_route_for_platform(
        root,
        snapshot,
        registry,
        static_playable_ids,
        playable_id,
        RoutePlatform::Android,
    )
}

pub fn resolve_route_for_platform<'a>(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    static_playable_ids: impl IntoIterator<Item = &'a str>,
    playable_id: &str,
    platform: RoutePlatform,
) -> Result<ResolvedRoute, RouteUnavailable> {
    if static_playable_ids.into_iter().any(|id| id == playable_id) {
        return Err(static_playable_collision(playable_id));
    }
    if platform == RoutePlatform::Linux {
        resolve_linux_route(root, snapshot, registry, playable_id, None)
    } else {
        resolve_android_route(root, snapshot, registry, playable_id)
    }
}

fn resolve_android_route(
    root: &Path,
    snapshot: &ConfigSnapshot,
    registry: &PluginRegistry,
    playable_id: &str,
) -> Result<ResolvedRoute, RouteUnavailable> {
    let game = snapshot.games.get(playable_id).ok_or_else(|| {
        unavailable(
            Some(playable_id),
            format!("game {playable_id} is unavailable"),
        )
    })?;
    let mut last = unavailable(
        Some(playable_id),
        format!("game {playable_id} has no Android runner"),
    );
    for key in &game.releases {
        let Some(release) = snapshot.releases.get(&key.0) else {
            continue;
        };
        let system_title = registry
            .systems()
            .values()
            .find(|system| system.id == release.system.0)
            .map(|system| system.title.clone())
            .or_else(|| {
                snapshot
                    .systems
                    .get(&release.system.0)
                    .map(|system| system.title.clone().or_else(|| system.name.clone()))
            });
        let Some(system_title) = system_title else {
            last = unavailable(
                Some(playable_id),
                format!("system {} is unavailable", release.system.0),
            );
            continue;
        };
        for location in snapshot.locations.get(&key.0).into_iter().flatten() {
            let command = match location {
                Location::ProviderRef { .. } => ANDROID_APP_COMMAND,
                Location::File { .. } => RETROARCH_COMMAND,
                _ => continue,
            };
            let candidates: Vec<_> = registry
                .runners()
                .values()
                .filter(|runner| runner.command.as_deref() == Some(command))
                .filter(|runner| {
                    runner
                        .systems
                        .as_ref()
                        .is_some_and(|systems| systems.contains(&release.system.0))
                })
                .collect();
            let runner = match candidates.as_slice() {
                [runner] => *runner,
                [] => {
                    last = unavailable(
                        Some(playable_id),
                        format!("no runner supports system {}", release.system.0),
                    );
                    continue;
                }
                _ => {
                    return Err(unavailable(
                        Some(playable_id),
                        format!("ambiguous runners for system {}", release.system.0),
                    ));
                }
            };
            let (provider_id, flattened_target, file_target) = match location {
                Location::ProviderRef {
                    provider,
                    provider_ref,
                } => (
                    provider.0.clone(),
                    format!("{}:{}", provider.0, provider_ref.0),
                    None,
                ),
                Location::File { storage, path, .. } => {
                    let target = ResolvedFileTarget {
                        storage_id: storage.0.clone(),
                        path: path.0.clone(),
                    };
                    if let Err(error) = storage::resolve_file_target(root, snapshot, &target) {
                        last = RouteDiagnostic {
                            code: if error.is_missing_target() {
                                RouteDiagnosticCode::LocalRomMissing
                            } else {
                                RouteDiagnosticCode::LocalRouteUnavailable
                            },
                            message: error.to_string(),
                            playable_id: Some(playable_id.into()),
                        };
                        continue;
                    }
                    (
                        runner
                            .family
                            .clone()
                            .unwrap_or_else(|| runner.id.split_once('/').unwrap().0.into()),
                        format!("{}:{}", storage.0, path.0),
                        Some(target),
                    )
                }
                _ => continue,
            };
            let android_component =
                runner
                    .android
                    .as_ref()
                    .map(|component| ResolvedAndroidComponent {
                        package_name: component.package_name.clone(),
                        class_name: component.class_name.clone(),
                    });
            return Ok(ResolvedRoute {
                playable_id: playable_id.into(),
                title: Some(game.title.clone()),
                release_id: key.0.clone(),
                identity: single_release_identity(game),
                provider_id,
                system_id: release.system.0.clone(),
                system_title,
                runner_id: runner.id.clone(),
                family_id: runner.family.clone(),
                integration_token: command.into(),
                flattened_target,
                android_component,
                linux_runner: None,
                core_path: runner.core.clone(),
                file_target,
            });
        }
    }
    Err(last)
}

fn single_release_identity(item: &GamePayload) -> Option<GameIdentity> {
    let [key] = item.releases.as_slice() else {
        return None;
    };
    key.0
        .starts_with("sha256:")
        .then(|| GameIdentity::Hash(key.0.clone()))
}

fn static_playable_collision(playable_id: &str) -> RouteUnavailable {
    collision(
        Some(playable_id),
        format!("dynamic local route {playable_id} collides with an existing static local game"),
    )
}

fn unavailable(playable_id: Option<&str>, message: String) -> RouteUnavailable {
    RouteUnavailable {
        code: RouteDiagnosticCode::LocalRouteUnavailable,
        message,
        playable_id: playable_id.map(str::to_owned),
    }
}

fn collision(playable_id: Option<&str>, message: String) -> RouteUnavailable {
    RouteUnavailable {
        code: RouteDiagnosticCode::LocalRouteCollision,
        message,
        playable_id: playable_id.map(str::to_owned),
    }
}
