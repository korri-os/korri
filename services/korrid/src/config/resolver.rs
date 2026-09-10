use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    plugin::{
        AndroidLauncherRecord, AndroidTransportImplementation, LauncherRecord, PluginRegistry,
        ProviderRecord, RuntimeRecord, SessionControlExecutor, SessionControlOwnerKind,
        SessionControlPlatform, SessionControlRecord, SystemRecord,
    },
    GameIdentity,
};

pub use super::linux_routes::{linux_route_candidates, resolve_linux_route, stored_runtime};
use super::{storage, AppPayload, ConfigSnapshot, GamePayload, Location};

const PROCESS_LAUNCHER_KIND: &str = "@korri:process";
const ANDROID_APP_COMMAND: &str = "android-app";
const RETROARCH_COMMAND: &str = "retroarch";
const LIBRETRO_CORE_KIND: &str = "libretro-core";

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

/** Resolve the enabled Moonlight declaration for the platform that can
 * actually provide its native transport edge. Registration alone is not
 * availability, and Linux intentionally has no Artemis implementation. */
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
    /** Active launcher/transport/runtime contributions in chosen-route order. */
    pub contributors: Vec<RouteContribution>,
    /** Live executors for this exact session; registration is not availability. */
    pub executor_availability: SessionExecutorAvailability,
}

/** Resolve declaration-only controls for the current route. Overlay-owned
 * controls are composed by the caller before these route-ordered records. */
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
pub struct ResolvedLinuxLauncher {
    pub program: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRuntime {
    pub id: String,
    pub kind: String,
    pub app: String,
    pub path: String,
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
    pub launcher_id: String,
    pub launcher_kind: String,
    pub integration_token: String,
    pub flattened_target: String,
    pub android_component: Option<ResolvedAndroidComponent>,
    pub linux_launcher: Option<ResolvedLinuxLauncher>,
    pub runtime: Option<ResolvedRuntime>,
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

#[derive(Clone, Debug)]
struct RouteLauncher {
    id: String,
    plugin: Option<String>,
    command: Option<String>,
    systems: Option<Vec<String>>,
    android: Option<AndroidLauncherRecord>,
}

#[derive(Clone, Debug, Default)]
struct Contributions {
    providers: BTreeMap<String, ProviderRecord>,
    systems: BTreeMap<String, SystemRecord>,
    launchers: BTreeMap<String, RouteLauncher>,
    runtimes: BTreeMap<String, RuntimeRecord>,
    provider_collisions: BTreeSet<String>,
    system_collisions: BTreeSet<String>,
    launcher_collisions: BTreeSet<String>,
}

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
    if platform == RoutePlatform::Linux {
        let static_ids: BTreeSet<_> = static_playable_ids.into_iter().collect();
        let mut catalog = RouteCatalog {
            routes: Vec::new(),
            diagnostics: Vec::new(),
        };
        for id in snapshot.games.keys() {
            let route = if static_ids.contains(id.as_str()) {
                Err(static_playable_collision(id))
            } else {
                linux_route_candidates(root, snapshot, registry, id)
            };
            match route {
                Ok(routes) => catalog.routes.extend(routes),
                Err(error) => catalog.diagnostics.push(error),
            }
        }
        return catalog;
    }
    let contributions = compose_contributions(snapshot, registry);
    let static_playable_ids: BTreeSet<String> =
        static_playable_ids.into_iter().map(str::to_owned).collect();
    let mut routes = Vec::new();
    let mut diagnostics = Vec::new();

    for playable_id in snapshot.games.keys() {
        if static_playable_ids.contains(playable_id) {
            diagnostics.push(static_playable_collision(playable_id));
            continue;
        }

        match resolve_route_with_contributions(
            root,
            snapshot,
            &contributions,
            playable_id,
            platform,
        ) {
            Ok(route) => routes.push(route),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    RouteCatalog {
        routes,
        diagnostics,
    }
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
    if static_playable_ids
        .into_iter()
        .any(|static_playable_id| static_playable_id == playable_id)
    {
        return Err(static_playable_collision(playable_id));
    }

    if platform == RoutePlatform::Linux {
        return resolve_linux_route(root, snapshot, registry, playable_id, None);
    }
    let contributions = compose_contributions(snapshot, registry);
    resolve_route_with_contributions(root, snapshot, &contributions, playable_id, platform)
}

fn resolve_route_with_contributions(
    root: &Path,
    snapshot: &ConfigSnapshot,
    contributions: &Contributions,
    playable_id: &str,
    platform: RoutePlatform,
) -> Result<ResolvedRoute, RouteUnavailable> {
    let item = snapshot.games.get(playable_id).ok_or_else(|| {
        unavailable(
            Some(playable_id),
            format!("local playable {playable_id} is not present in catalog/games.yaml"),
        )
    })?;
    let mut selected = None;
    let mut last_error = unavailable(
        Some(playable_id),
        format!("local playable {playable_id} has no complete location"),
    );
    for key in &item.releases {
        let Some(release) = snapshot.releases.get(&key.0) else {
            continue;
        };
        for location in snapshot.locations.get(&key.0).into_iter().flatten() {
            match resolve_located_release(
                root,
                snapshot,
                contributions,
                playable_id,
                (&key.0, release),
                location,
                platform,
            ) {
                Ok(route) => {
                    // Catalog list order supplies a stable choice without a
                    // personal preference fold. Still inspect later releases
                    // so declaration collisions remain fail-closed.
                    selected.get_or_insert(route);
                    break;
                }
                Err(error) if error.code == RouteDiagnosticCode::LocalRouteCollision => {
                    return Err(error)
                }
                Err(error) => last_error = error,
            }
        }
    }
    selected.ok_or(last_error)
}

fn resolve_located_release(
    root: &Path,
    snapshot: &ConfigSnapshot,
    contributions: &Contributions,
    playable_id: &str,
    (release_id, release): (&str, &super::ReleasePayload),
    target: &Location,
    _platform: RoutePlatform,
) -> Result<ResolvedRoute, RouteUnavailable> {
    for id in &contributions.launcher_collisions {
        let supports = snapshot
            .launchers
            .get(id)
            .and_then(|launcher| launcher.systems.as_ref())
            .is_some_and(|systems| systems.contains(&release.system.0))
            || contributions.runtimes.values().any(|runtime| {
                runtime.app.as_deref() == Some(id.as_str())
                    && runtime
                        .supports
                        .as_ref()
                        .and_then(|supports| supports.systems.as_ref())
                        .is_some_and(|systems| systems.contains(&release.system.0))
            });
        if supports {
            return Err(collision(
                Some(playable_id),
                format!(
                    "launcher {id} is declared by both device configuration and an enabled plugin"
                ),
            ));
        }
    }
    let candidates: Vec<_> = contributions
        .launchers
        .values()
        .filter(|launcher| {
            launcher.command.as_deref() != Some(RETROARCH_COMMAND) || launcher.android.is_some()
        })
        .filter(|launcher| {
            // RetroArch is system-agnostic. Its enabled cores declare both app
            // and supports.systems; that pair contributes the supported system.
            launcher
                .systems
                .as_ref()
                .is_some_and(|systems| systems.contains(&release.system.0))
                || (launcher.systems.as_ref().is_none_or(Vec::is_empty)
                    && contributions.runtimes.values().any(|runtime| {
                        runtime.app.as_deref() == Some(launcher.id.as_str())
                            && runtime.launcher.is_none()
                            && runtime
                                .supports
                                .as_ref()
                                .and_then(|supports| supports.systems.as_ref())
                                .is_some_and(|systems| systems.contains(&release.system.0))
                    }))
        })
        .collect();
    let launcher = match candidates.as_slice() {
        [launcher] => *launcher,
        [] => {
            return Err(unavailable(
                Some(playable_id),
                format!("no launcher supports system {}", release.system.0),
            ))
        }
        _ => {
            return Err(unavailable(
                Some(playable_id),
                format!("ambiguous launchers for system {}", release.system.0),
            ))
        }
    };
    let system_id = release.system.0.as_str();
    if contributions.system_collisions.contains(system_id) {
        return Err(collision(
            Some(playable_id),
            format!(
                "system {system_id} is declared by both user configuration and an enabled plugin"
            ),
        ));
    }
    let system = contributions.systems.get(system_id).ok_or_else(|| {
        unavailable(
            Some(playable_id),
            format!("system {system_id} is unavailable"),
        )
    })?;

    let launcher_kind = launcher.plugin.as_deref().unwrap_or(PROCESS_LAUNCHER_KIND);
    if launcher_kind == PROCESS_LAUNCHER_KIND {
        return Err(unavailable(
            Some(playable_id),
            format!(
                "launcher {} has no plugin kind; process fallback is not supported",
                launcher.id
            ),
        ));
    }

    let command = launcher.command.as_deref().ok_or_else(|| {
        unavailable(
            Some(playable_id),
            format!("launcher {} has no integration command", launcher.id),
        )
    })?;
    if !matches!(command, ANDROID_APP_COMMAND | RETROARCH_COMMAND) {
        return Err(unavailable(
            Some(playable_id),
            format!(
                "launcher {} command {command} is not supported",
                launcher.id
            ),
        ));
    }
    let (provider_id, flattened_target, file_target) = match (command, target) {
        (
            ANDROID_APP_COMMAND,
            Location::ProviderRef {
                provider,
                provider_ref,
            },
        ) => (
            provider.0.clone(),
            format!("{}:{}", provider.0, provider_ref.0),
            None,
        ),
        (
            RETROARCH_COMMAND,
            Location::File {
                storage: target_storage,
                path,
                ..
            },
        ) => {
            let file_target = ResolvedFileTarget {
                storage_id: target_storage.0.clone(),
                path: path.0.clone(),
            };
            (
                launcher_kind.to_owned(),
                format!("{}:{}", target_storage.0, path.0),
                Some(file_target),
            )
        }
        (_, other) => {
            return Err(unavailable(
                Some(playable_id),
                format!(
                    "release {} target kind {} is not supported for plugin routes",
                    release_id,
                    target_kind(other)
                ),
            ));
        }
    };

    if contributions.provider_collisions.contains(&provider_id) {
        return Err(collision(
            Some(playable_id),
            format!("provider {provider_id} is declared by both user configuration and an enabled plugin"),
        ));
    }
    if !contributions.providers.contains_key(&provider_id) {
        return Err(unavailable(
            Some(playable_id),
            format!("provider {provider_id} is unavailable"),
        ));
    }
    if command == ANDROID_APP_COMMAND && launcher_kind != provider_id {
        return Err(unavailable(
            Some(playable_id),
            format!(
                "launcher {} belongs to {launcher_kind}, not provider {provider_id}",
                launcher.id
            ),
        ));
    }

    let runtime = match command {
        ANDROID_APP_COMMAND => None,
        RETROARCH_COMMAND => {
            let runtimes: Vec<_> = contributions
                .runtimes
                .values()
                .filter(|runtime| {
                    runtime.app.as_deref() == Some(launcher.id.as_str())
                        && runtime.launcher.is_none()
                        && runtime
                            .supports
                            .as_ref()
                            .and_then(|supports| supports.systems.as_ref())
                            .is_some_and(|systems| systems.contains(&release.system.0))
                })
                .collect();
            let runtime = match runtimes.as_slice() {
                [runtime] => *runtime,
                [] => {
                    return Err(unavailable(
                        Some(playable_id),
                        format!(
                            "no runtime supports system {system_id} for launcher {}",
                            launcher.id
                        ),
                    ))
                }
                _ => {
                    return Err(unavailable(
                        Some(playable_id),
                        format!(
                            "ambiguous runtimes for system {system_id} and launcher {}",
                            launcher.id
                        ),
                    ))
                }
            };
            if runtime.kind != LIBRETRO_CORE_KIND {
                return Err(unavailable(
                    Some(playable_id),
                    format!(
                        "runtime {} has kind {}, expected {LIBRETRO_CORE_KIND}",
                        runtime.id, runtime.kind
                    ),
                ));
            }
            Some(ResolvedRuntime {
                id: runtime.id.clone(),
                kind: runtime.kind.clone(),
                app: runtime.app.clone().expect("validated Android runtime"),
                path: runtime.path.clone(),
            })
        }
        _ => unreachable!("integration token was validated above"),
    };

    let android_component = launcher
        .android
        .as_ref()
        .map(|component| ResolvedAndroidComponent {
            package_name: component.package_name.clone(),
            class_name: component.class_name.clone(),
        });
    let linux_launcher = None;
    // Observe bytes only after contribution validation. Missing media must not
    // conceal a declaration collision, but it must permit another healthy copy.
    if let Some(file_target) = &file_target {
        storage::resolve_file_target(root, snapshot, file_target).map_err(|error| {
            RouteDiagnostic {
                code: if error.is_missing_target() {
                    RouteDiagnosticCode::LocalRomMissing
                } else {
                    RouteDiagnosticCode::LocalRouteUnavailable
                },
                message: format!("release {release_id} {error}"),
                playable_id: Some(playable_id.to_owned()),
            }
        })?;
    }
    let item = &snapshot.games[playable_id];
    Ok(ResolvedRoute {
        playable_id: playable_id.to_owned(),
        title: Some(item.title.clone()),
        release_id: release_id.to_owned(),
        identity: single_release_identity(item),
        provider_id,
        system_id: system_id.to_owned(),
        system_title: system.title.clone(),
        launcher_id: launcher.id.clone(),
        launcher_kind: launcher_kind.to_owned(),
        integration_token: command.to_owned(),
        flattened_target,
        android_component,
        linux_launcher,
        runtime,
        file_target,
    })
}

fn single_release_identity(item: &GamePayload) -> Option<GameIdentity> {
    let [key] = item.releases.as_slice() else {
        return None;
    };
    key.0
        .starts_with("sha256:")
        .then(|| GameIdentity::Hash(key.0.clone()))
}

fn compose_contributions(snapshot: &ConfigSnapshot, registry: &PluginRegistry) -> Contributions {
    let mut contributions = Contributions::default();

    for (id, record) in registry.providers() {
        contributions.providers.insert(id.clone(), record.clone());
    }
    for record in registry.systems().values() {
        contributions
            .systems
            .insert(record.id.clone(), record.clone());
    }
    for record in registry.launchers().values() {
        contributions
            .launchers
            .insert(record.id.clone(), launcher_from_plugin(record));
    }
    for record in registry.runtimes().values() {
        contributions
            .runtimes
            .insert(record.id.clone(), record.clone());
    }

    for (id, provider) in &snapshot.providers {
        if contributions.providers.contains_key(id) {
            contributions.provider_collisions.insert(id.clone());
            contributions.providers.remove(id);
        } else if registry.owns_registered_provider_id(id) {
            continue;
        } else {
            contributions.providers.insert(
                id.clone(),
                ProviderRecord {
                    id: id.clone(),
                    title: provider.title.clone(),
                },
            );
        }
    }

    for (id, system) in &snapshot.systems {
        if contributions.systems.contains_key(id) {
            contributions.system_collisions.insert(id.clone());
            contributions.systems.remove(id);
        } else if registry.owns_registered_system_id(id) {
            continue;
        } else {
            contributions.systems.insert(
                id.clone(),
                SystemRecord {
                    id: id.clone(),
                    title: system.title.clone().or_else(|| system.name.clone()),
                    aliases: system.aliases.clone(),
                },
            );
        }
    }

    for (id, launcher) in &snapshot.launchers {
        if contributions.launchers.contains_key(id) {
            contributions.launcher_collisions.insert(id.clone());
            contributions.launchers.remove(id);
        } else if registry.owns_registered_launcher_id(id) {
            continue;
        } else {
            contributions
                .launchers
                .insert(id.clone(), launcher_from_snapshot(id, launcher));
        }
    }

    contributions
}

fn launcher_from_plugin(record: &LauncherRecord) -> RouteLauncher {
    RouteLauncher {
        id: record.id.clone(),
        plugin: record.plugin.clone(),
        command: record.command.clone(),
        systems: record.systems.clone(),
        android: record.android.clone(),
    }
}

fn launcher_from_snapshot(id: &str, payload: &AppPayload) -> RouteLauncher {
    RouteLauncher {
        id: id.to_owned(),
        plugin: payload.plugin.as_ref().map(|value| value.0.clone()),
        command: payload.command.as_ref().map(|value| value.0.clone()),
        systems: payload.systems.clone(),
        android: None,
    }
}

fn target_kind(target: &Location) -> &'static str {
    match target {
        Location::File { .. } => "file",
        Location::FileSet { .. } => "file-set",
        Location::Executable { .. } => "executable",
        Location::Url { .. } => "url",
        Location::ProviderRef { .. } => "provider-ref",
    }
}

fn static_playable_collision(playable_id: &str) -> RouteUnavailable {
    collision(
        Some(playable_id),
        format!(
            "dynamic local route {playable_id} collides with an existing static local game; the static route remains active"
        ),
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
