mod compositor_focus;
#[cfg(test)]
pub(crate) use compositor_focus::CompositorControl;
mod config;
pub(crate) mod control;
mod identity;
mod input_coordination;
mod input_seat;
mod launch_reservation;
pub(crate) mod moonlight_certificate;
pub(crate) mod play_log;
mod prepare;
mod private_input;
pub(crate) mod retroarch_control;
#[cfg(test)]
mod route_state_tests;
mod session_state;
mod systemd_unit;

use crate::plugin::{
    SessionControlDeclarationInteraction, SessionControlEffect, SessionControlOwnerKind,
};
use crate::{
    config::{
        resolver::resolve_launchable_routes,
        snapshot::{ConfigSnapshotCoordinator, SnapshotAuthorization},
    },
    identity::DeviceIdentity,
    launcher::linux_plugin,
    plugin_policy, CatalogSnapshot, Game, GameIdentity, GameSource, RpcFailure, SessionControl,
    SessionControlCompleted, SessionControlFailure, SessionControlFailureReason,
    SessionControlGroup, SessionControlInteraction, SessionControlInvokeOutcome,
    SessionControlInvokeRequest, SessionControls, SessionPrepared, SourceCatalogState,
    SourceStatus, SourceStreamControlState,
};
use config::{HostConfig, HostConfigError};
use moonlight_certificate::MoonlightCertificateAdapter;
use prepare::HostLauncher;
use retroarch_control::{NetworkRetroarchControl, RetroarchControlExecutor};
use session_state::{
    HostSessionControl, HostSessionEffectFailure, HostSessionFreezeChange, HostSessionStatus,
    HostSessionStop,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[cfg(test)]
use systemd_unit::LaunchUnitBackend;

#[derive(Clone)]
struct DynamicHostGame {
    id: String,
    title: String,
    identity: Option<GameIdentity>,
}

#[derive(Clone)]
struct DynamicHostRuntime {
    games: Vec<DynamicHostGame>,
    declared_ids: std::collections::BTreeSet<String>,
    failures: Vec<RpcFailure>,
}

impl DynamicHostRuntime {
    fn validate_static_games(&self, config: &HostConfig) -> Result<(), RpcFailure> {
        for game in &config.games {
            if self.declared_ids.contains(&game.id) {
                return Err(dynamic_failure(format!(
                    "game id {:?} is declared by both host.toml and catalog/games.yaml",
                    game.id
                )));
            }
        }
        Ok(())
    }

    fn from_root_with_registry(
        root: &Path,
        registry: &crate::plugin::PluginRegistry,
    ) -> Result<Self, RpcFailure> {
        let coordinator = ConfigSnapshotCoordinator::new(root);
        let state = coordinator.reload();
        if state.authorization != SnapshotAuthorization::Authorized {
            return Err(dynamic_failure(
                state
                    .diagnostic
                    .map(|diagnostic| diagnostic.message)
                    .unwrap_or_else(|| "host library is unavailable".into()),
            ));
        }
        if let Some(diagnostic) = state.diagnostic {
            return Err(dynamic_failure(diagnostic.message));
        }
        let catalog =
            resolve_launchable_routes(root, &state.snapshot, registry, std::iter::empty());
        if catalog.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == crate::config::resolver::RouteDiagnosticCode::LocalRouteCollision
        }) {
            return Err(dynamic_failure(
                catalog
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        let mut failures: Vec<_> = catalog
            .diagnostics
            .iter()
            .map(crate::route_diagnostic_failure)
            .collect();
        let mut games = Vec::new();
        let mut by_game = std::collections::BTreeMap::<_, Vec<_>>::new();
        for route in catalog.routes {
            by_game
                .entry(route.playable_id.clone())
                .or_default()
                .push(route);
        }
        for candidates in by_game.into_values() {
            let candidate = &candidates[0];
            // Discovery validates availability without requiring one chosen
            // command. A stale saved choice is still a real configuration error.
            match crate::config::resolver::linux_route_selection(&state.snapshot, &candidates, None)
            {
                Ok(crate::config::resolver::LinuxRouteSelection::UnavailableSavedChoice(error))
                | Err(error) => failures.push(crate::route_diagnostic_failure(&error)),
                Ok(_) => {}
            }
            for route in &candidates {
                match linux_plugin::launch_route(root, &state.snapshot, registry, route, None) {
                    Ok(launch) => {
                        failures.extend(launch.warnings.into_iter().map(|warning| RpcFailure {
                            code: "LaunchSettingUnsupported".into(),
                            message: warning.message,
                        }))
                    }
                    Err(error) => failures.push(crate::game_routes::unavailable(error.to_string())),
                }
            }
            games.push(DynamicHostGame {
                id: candidate.playable_id.clone(),
                title: candidate
                    .title
                    .clone()
                    .unwrap_or_else(|| candidate.release_id.clone()),
                identity: candidate.identity.clone(),
            });
        }
        Ok(Self {
            games,
            declared_ids: state.snapshot.games.keys().cloned().collect(),
            failures,
        })
    }
}

#[derive(Clone)]
enum DynamicHostSource {
    Installed(PathBuf),
    #[cfg(test)]
    Selected(PathBuf, Arc<crate::plugin::PluginRegistry>),
    #[cfg(test)]
    Fixed(DynamicHostRuntime),
}

impl DynamicHostSource {
    fn load(&self) -> Result<DynamicHostRuntime, RpcFailure> {
        match self {
            Self::Installed(root) => {
                let registry = plugin_policy::installed_registry()
                    .map_err(|error| dynamic_failure(error.to_string()))?;
                DynamicHostRuntime::from_root_with_registry(root, &registry)
            }
            #[cfg(test)]
            Self::Selected(root, registry) => {
                DynamicHostRuntime::from_root_with_registry(root, registry)
            }
            #[cfg(test)]
            Self::Fixed(runtime) => Ok(runtime.clone()),
        }
    }
}

const MAX_CONCURRENT_CERTIFICATE_CONTROLS: usize = 4;

/// Observe focus and completion even when no browser can poll.
const PORTAL_WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Absorb brief watcher queries without hiding owned startup identity behind
/// a committing helper that can hold transition authority for seconds.
const PENDING_STATUS_LOCK_WAIT: std::time::Duration = std::time::Duration::from_millis(250);

fn identity_keys(private_state_root: &Path) -> (Option<String>, Option<String>) {
    let Some(identity) = DeviceIdentity::load_or_create(private_state_root).ok() else {
        return (None, None);
    };
    let device_public_key = identity.device_public_key().map(str::to_owned);
    let owner_public_key = match identity.state() {
        crate::identity::IdentityState::Owned {
            owner_public_key, ..
        } => Some(owner_public_key.clone()),
        _ => None,
    };
    (device_public_key, owner_public_key)
}

#[derive(Clone)]
pub struct HostRuntime {
    private_state_root: PathBuf,
    route_write_lock: Arc<std::sync::Mutex<()>>,
    launch_reservations: launch_reservation::LaunchReservations,
    // Tests can delay one response after the real setter releases its lock.
    #[cfg(test)]
    after_runner_choice_write: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    before_reserved_route: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    after_reserved_start: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    after_session_rpc: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    config: Result<HostConfig, HostConfigError>,
    launcher: Option<HostLauncher>,
    dynamic: Option<DynamicHostSource>,
    device_public_key: Option<String>,
    owner_public_key: Option<String>,
    moonlight_certificate: Arc<dyn MoonlightCertificateAdapter>,
    moonlight_certificate_permits: Arc<tokio::sync::Semaphore>,
    retroarch_control: Arc<dyn RetroarchControlExecutor>,
    input_coordinator: Option<Arc<input_coordination::SeatCoordinator>>,
}

impl HostRuntime {
    pub fn from_path(path: &Path) -> Self {
        Self::from_paths(path, None)
    }

    pub fn from_paths(path: &Path, storage_root: Option<PathBuf>) -> Self {
        Self::from_paths_with_private_state(path, storage_root, PathBuf::from("korri-state"))
    }

    pub fn from_paths_with_private_state(
        path: &Path,
        storage_root: Option<PathBuf>,
        private_state_root: PathBuf,
    ) -> Self {
        Self::from_paths_with_native_input(path, storage_root, private_state_root, None)
    }

    pub(crate) fn from_paths_with_native_input(
        path: &Path,
        storage_root: Option<PathBuf>,
        private_state_root: PathBuf,
        native: Option<Arc<crate::portal_input::PortalInputSource>>,
    ) -> Self {
        let config = HostConfig::read(path);
        let socket = std::env::var_os("KORRID_INPUT_SEAT_CONTROL_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/run/korri-input-seat/control.sock"));
        let coordinator = input_coordination::SeatCoordinator::connect(&socket, native.clone());
        let input_coordinator = coordinator.as_ref().ok().cloned();
        let manager = Arc::new(input_seat::UnixInputSeatManager::new(socket, coordinator));
        let launcher = config.as_ref().ok().map(|config| {
            let launcher = HostLauncher::new(config, &private_state_root, manager, native);
            let count = storage_root
                .as_ref()
                .ok_or_else(|| "device settings root is not configured".to_string())
                .and_then(|root| {
                    // device.yaml only: plugin state must never fence input.
                    crate::config::settings::read_player_count(root).map_err(|e| e.to_string())
                });
            if let Err(error) = count.and_then(|count| launcher.control().initialize_input(count)) {
                launcher.control().fence_input();
                eprintln!("korrid: input pool startup reconciliation failed: {error}");
            }
            launcher
        });
        let dynamic = storage_root.map(DynamicHostSource::Installed);
        let (device_public_key, owner_public_key) = identity_keys(&private_state_root);
        Self {
            private_state_root,
            route_write_lock: Arc::new(std::sync::Mutex::new(())),
            launch_reservations: launch_reservation::LaunchReservations::default(),
            #[cfg(test)]
            after_runner_choice_write: None,
            #[cfg(test)]
            before_reserved_route: None,
            #[cfg(test)]
            after_reserved_start: None,
            #[cfg(test)]
            after_session_rpc: None,
            config,
            launcher,
            dynamic,
            device_public_key,
            owner_public_key,
            moonlight_certificate: moonlight_certificate::production_adapter(),
            moonlight_certificate_permits: Arc::new(tokio::sync::Semaphore::new(
                MAX_CONCURRENT_CERTIFICATE_CONTROLS,
            )),
            retroarch_control: Arc::new(NetworkRetroarchControl::default()),
            input_coordinator,
        }
    }

    pub(crate) fn private_input_router(&self) -> Option<axum::Router> {
        self.input_coordinator.clone().map(private_input::router)
    }

    #[cfg(test)]
    pub(crate) fn from_paths_with_backend(
        path: &Path,
        storage_root: Option<PathBuf>,
        private_state_root: PathBuf,
        backend: Arc<dyn LaunchUnitBackend>,
    ) -> Self {
        let config = HostConfig::read(path);
        let launcher = config
            .as_ref()
            .ok()
            .map(|config| HostLauncher::with_backend(config, &private_state_root, backend));
        let dynamic = storage_root.map(DynamicHostSource::Installed);
        let (device_public_key, owner_public_key) = identity_keys(&private_state_root);
        Self {
            private_state_root,
            route_write_lock: Arc::new(std::sync::Mutex::new(())),
            launch_reservations: launch_reservation::LaunchReservations::default(),
            after_runner_choice_write: None,
            before_reserved_route: None,
            after_reserved_start: None,
            after_session_rpc: None,
            config,
            launcher,
            dynamic,
            device_public_key,
            owner_public_key,
            moonlight_certificate: moonlight_certificate::production_adapter(),
            moonlight_certificate_permits: Arc::new(tokio::sync::Semaphore::new(
                MAX_CONCURRENT_CERTIFICATE_CONTROLS,
            )),
            retroarch_control: Arc::new(NetworkRetroarchControl::default()),
            input_coordinator: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_paths_with_backends(
        path: &Path,
        storage_root: Option<PathBuf>,
        private_state_root: PathBuf,
        backend: Arc<dyn LaunchUnitBackend>,
        moonlight_certificate: Arc<dyn MoonlightCertificateAdapter>,
    ) -> Self {
        let mut runtime =
            Self::from_paths_with_backend(path, storage_root, private_state_root, backend);
        runtime.moonlight_certificate = moonlight_certificate;
        runtime
    }

    pub fn catalog_snapshot(&self) -> Result<CatalogSnapshot, RpcFailure> {
        self.catalog_snapshot_blocking(None)
    }

    pub async fn catalog_snapshot_for(
        &self,
        person_public_key: Option<&str>,
    ) -> Result<CatalogSnapshot, RpcFailure> {
        let runtime = self.clone();
        let person_public_key = person_public_key.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            runtime.catalog_snapshot_blocking(person_public_key.as_deref())
        })
        .await
        .map_err(host_worker_failure)?
    }

    fn catalog_snapshot_blocking(
        &self,
        person_public_key: Option<&str>,
    ) -> Result<CatalogSnapshot, RpcFailure> {
        let config = self.config.as_ref().map_err(config_failure)?;
        let source = GameSource {
            device_public_key: self.device_public_key.clone(),
            label: config.label.clone(),
            is_local: true,
        };
        let stats_for = |game_id: &str| -> Result<_, RpcFailure> {
            let Some(person_public_key) = person_public_key else {
                return Ok(None);
            };
            let launcher = self.launcher.as_ref().ok_or_else(|| {
                config_failure(self.config.as_ref().expect_err("invalid host config"))
            })?;
            launcher
                .control()
                .play_log()
                .stats(&play_log::PlayHistoryKey {
                    user_id: person_public_key.to_owned(),
                    game_id: game_id.to_owned(),
                })
                .map(Some)
                .map_err(|error| RpcFailure {
                    code: "PlayLogUnavailable".into(),
                    message: error.to_string(),
                })
        };
        let mut games = Vec::with_capacity(config.games.len());
        for game in &config.games {
            games.push(Game {
                id: game.id.clone(),
                title: game.title.clone(),
                host: Some(config.label.clone()),
                identity: game.identity.clone(),
                source: source.clone(),
                supports_runner_selection: false,
                play_stats: stats_for(&game.id)?,
            });
        }
        let mut failures = Vec::new();
        if let Some(dynamic) = &self.dynamic {
            let dynamic = dynamic.load()?;
            dynamic.validate_static_games(config)?;
            failures.extend(
                dynamic
                    .failures
                    .iter()
                    .map(|failure| crate::CatalogHostFailure {
                        host: config.label.clone(),
                        code: failure.code.clone(),
                        message: failure.message.clone(),
                    }),
            );
            for game in &dynamic.games {
                games.push(Game {
                    id: game.id.clone(),
                    title: game.title.clone(),
                    host: Some(config.label.clone()),
                    identity: game.identity.clone(),
                    source: source.clone(),
                    supports_runner_selection: true,
                    play_stats: stats_for(&game.id)?,
                });
            }
        }
        Ok(CatalogSnapshot {
            games,
            failures: (!failures.is_empty()).then_some(failures),
        })
    }

    pub fn owner_public_key(&self) -> Option<&str> {
        self.owner_public_key.as_deref()
    }

    /// Reports this host's current readiness as a federation source. The
    /// host answers only for its own device key; a request naming another
    /// device fails so a brain can never attribute one host's readiness to
    /// another. The catalog answer is derived from the same configuration
    /// that `catalog_snapshot` uses. The stream-control answer is a bounded,
    /// non-mutating probe of the protected Stream certificate control.
    /// A busy certificate permit set reports disabled rather than waiting.
    pub async fn source_status(
        &self,
        requested_device_public_key: &str,
    ) -> Result<SourceStatus, RpcFailure> {
        match &self.device_public_key {
            Some(own) if own == requested_device_public_key => {}
            Some(_) => {
                return Err(RpcFailure {
                    code: "SourceDeviceMismatch".into(),
                    message: "this host answers source status only for its own device key".into(),
                })
            }
            None => {
                return Err(RpcFailure {
                    code: "HostIdentityUnavailable".into(),
                    message: "host device identity is unavailable".into(),
                })
            }
        }
        Ok(self.own_source_status().await)
    }

    async fn own_source_status(&self) -> SourceStatus {
        let catalog = match self.catalog_snapshot() {
            Ok(_) => SourceCatalogState::Available,
            Err(_) => SourceCatalogState::Unavailable,
        };
        let stream_control = match self.certificate_control_permit() {
            Ok(permit) => {
                let adapter = Arc::clone(&self.moonlight_certificate);
                let available = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    adapter.available()
                })
                .await
                .unwrap_or(false);
                if available {
                    SourceStreamControlState::Enabled
                } else {
                    SourceStreamControlState::Disabled
                }
            }
            Err(_) => SourceStreamControlState::Disabled,
        };
        SourceStatus {
            catalog,
            stream_control,
        }
    }

    pub async fn prepare(
        &self,
        game_id: &str,
        person_public_key: Option<&str>,
    ) -> Result<SessionPrepared, RpcFailure> {
        let runtime = self.clone();
        let game_id = game_id.to_owned();
        let person_public_key = person_public_key.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            let outcome = runtime.prepare_blocking(&game_id, person_public_key.as_deref());
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.prepare");
            outcome
        })
        .await
        .map_err(host_worker_failure)?
    }

    fn prepare_blocking(
        &self,
        game_id: &str,
        person_public_key: Option<&str>,
    ) -> Result<SessionPrepared, RpcFailure> {
        let launcher = self.launcher.as_ref().ok_or_else(|| {
            config_failure(self.config.as_ref().expect_err("invalid host config"))
        })?;
        if let Some(dynamic) = &self.dynamic {
            let dynamic = dynamic.load()?;
            dynamic.validate_static_games(self.config.as_ref().map_err(config_failure)?)?;
            if dynamic.declared_ids.contains(game_id) {
                let command = self.route_root().and_then(|root| {
                    crate::game_routes::default_launch(root, &self.route_registry()?, game_id)
                        .map(|launch| launch.command)
                });
                // The session executor returns to an existing same-game live
                // session before requiring a fresh, revalidated launch command.
                return launcher.prepare_command(
                    game_id,
                    person_public_key,
                    command.as_deref().map_err(Clone::clone),
                );
            }
        }
        launcher.prepare(game_id, person_public_key)
    }

    fn route_root(&self) -> Result<&Path, RpcFailure> {
        match &self.dynamic {
            Some(DynamicHostSource::Installed(root)) => Ok(root),
            #[cfg(test)]
            Some(DynamicHostSource::Selected(root, _)) => Ok(root),
            _ => Err(dynamic_failure("installed game library is unavailable")),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_retroarch_control_port(mut self, port: u16) -> Self {
        self.retroarch_control = Arc::new(NetworkRetroarchControl::new(port));
        self
    }

    #[cfg(test)]
    pub(crate) fn with_route_registry(
        mut self,
        root: PathBuf,
        registry: crate::plugin::PluginRegistry,
    ) -> Self {
        self.dynamic = Some(DynamicHostSource::Selected(root, Arc::new(registry)));
        self
    }

    fn route_registry(&self) -> Result<crate::plugin::PluginRegistry, RpcFailure> {
        #[cfg(test)]
        if let Some(DynamicHostSource::Selected(_, registry)) = &self.dynamic {
            return Ok((**registry).clone());
        }
        plugin_policy::installed_registry().map_err(|error| dynamic_failure(error.to_string()))
    }

    pub async fn game_routes(
        &self,
        game_id: String,
    ) -> Result<crate::game_routes::GameRoutes, RpcFailure> {
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            let registry = runtime.route_registry()?;
            crate::game_routes::list(runtime.route_root()?, &registry, &game_id)
        })
        .await
        .map_err(host_worker_failure)?
    }

    fn settings_registry_source(&self) -> plugin_policy::RegistrySource {
        #[cfg(test)]
        if let Some(DynamicHostSource::Selected(_, registry)) = &self.dynamic {
            return plugin_policy::RegistrySource::Selected(Arc::clone(registry));
        }
        plugin_policy::RegistrySource::Installed
    }

    pub async fn settings_snapshot(&self) -> Result<crate::SettingsSnapshot, RpcFailure> {
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            let readable = crate::config::settings::read_with_registry_source(
                runtime.route_root()?,
                &runtime.settings_registry_source(),
            )
            .map_err(crate::settings_failure)?;
            let sensitive = crate::config::settings::read_sensitive(&runtime.private_state_root)
                .map_err(crate::settings_failure)?;
            Ok(crate::settings_snapshot(readable, sensitive))
        })
        .await
        .map_err(host_worker_failure)?
    }

    /// Commit runtime and file under the same idle transition lock. An
    /// uncertain acknowledgement or failed compensation fences new launches.
    pub async fn update_player_count(
        &self,
        request: crate::SettingsUpdateRequest,
    ) -> Result<crate::SettingsSnapshot, RpcFailure> {
        if request.setting_id != crate::config::settings::PLAYER_COUNT_SETTING_ID {
            return Err(RpcFailure {
                code: "OperationUnsupported".into(),
                message: "host settings writes support only host.preferences.playerCount".into(),
            });
        }
        let change = crate::config::settings::parse_change(&request.setting_id, request.value)
            .map_err(crate::settings_failure)?;
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            let control = runtime.control()?;
            control.with_idle_session(|| {
                use crate::config::settings;
                let root = runtime.route_root()?;
                let registry = runtime.settings_registry_source();
                let before = settings::read_with_registry_source(root, &registry)
                    .map_err(crate::settings_failure)?;
                if before.revision != request.expected_revision {
                    return Err(crate::settings_failure(settings::SettingsError::Conflict));
                }
                let sensitive = settings::read_sensitive(&runtime.private_state_root)
                    .map_err(crate::settings_failure)?;
                let settings::SettingChange::PlayerCount(count) = &change else {
                    unreachable!("validated player count")
                };
                let count = count.get();
                if let Err(message) = control.apply_input_count(count) {
                    if control.apply_input_count(before.player_count).is_err() {
                        control.fence_input();
                    }
                    return Err(RpcFailure {
                        code: "InputSeatUnavailable".into(),
                        message,
                    });
                }
                let written = settings::update_with_registry_source(
                    root,
                    &runtime.private_state_root,
                    &runtime.route_write_lock,
                    &request.expected_revision,
                    change,
                    &registry,
                );
                match written {
                    Ok(readable) => {
                        let current = settings::read_with_registry_source(root, &registry);
                        if readable.player_count != count
                            || !current.as_ref().is_ok_and(|current| {
                                current.revision == readable.revision
                                    && current.player_count == count
                            })
                        {
                            control.fence_input();
                            return Err(RpcFailure {
                                code: "InputSeatUnavailable".into(),
                                message:
                                    "settings changed during pool application; launches are fenced"
                                        .into(),
                            });
                        }
                        Ok(crate::settings_snapshot(readable, sensitive))
                    }
                    Err(error) => {
                        // Only compensate the runtime when the file is still
                        // exactly the pre-transaction revision. Do not overwrite
                        // an outside writer or guess after a partial file error.
                        let unchanged = settings::read_with_registry_source(root, &registry)
                            .is_ok_and(|current| current.revision == before.revision);
                        if !unchanged || control.apply_input_count(before.player_count).is_err() {
                            control.fence_input();
                        }
                        Err(crate::settings_failure(error))
                    }
                }
            })
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub async fn set_game_runner(
        &self,
        request: crate::game_routes::GameRunnerSetRequest,
    ) -> Result<crate::config::settings::RunnerChoiceRevisions, RpcFailure> {
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            let root = runtime.route_root()?;
            let revisions = crate::config::settings::set_runner_choice(
                root,
                &runtime.private_state_root,
                &runtime.route_write_lock,
                &request.expected_revision,
                &request.scope,
                request.runner_id.as_deref(),
            )
            .map_err(crate::game_routes::settings_failure)?;
            #[cfg(test)]
            if let Some(after_write) = &runtime.after_runner_choice_write {
                after_write();
            }
            Ok(revisions)
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub(crate) fn reserve_launch(
        &self,
        game_id: &str,
        caller: &str,
        person: Option<&str>,
    ) -> Result<SessionPrepared, RpcFailure> {
        self.control()?;
        Ok(self.launch_reservations.reserve(game_id, caller, person))
    }

    pub(crate) fn pending_launches(
        &self,
        caller: &str,
        person: Option<&str>,
    ) -> Option<Vec<crate::PendingLaunch>> {
        let pending = self.launch_reservations.snapshot(caller, person);
        (!pending.is_empty()).then_some(pending)
    }

    pub(crate) async fn start_launch(
        &self,
        request: crate::SessionStartRequest,
        caller: &str,
        person: Option<&str>,
    ) -> Result<crate::game_routes::SelectedGameLaunch, RpcFailure> {
        // Claim the exact caller-bound token before any route work. Cancel only
        // touches the small reservation mutex, never the plugin preparation.
        let reservation = self.launch_reservations.start(
            &request.game_id,
            &request.expected_launch_id,
            caller,
            person,
        )?;
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            let outcome = runtime.start_reserved_blocking(&request, &reservation);
            if let Some(committed) = runtime.launch_reservations.finish(&reservation) {
                if !committed {
                    return Err(launch_reservation::cancelled());
                }
                // Effects may have won the commit gate. End only this launch,
                // never an active-game match or a replacement. StopPending is
                // observed by the existing backend watcher.
                match runtime.control()?.stop(&request.expected_launch_id) {
                    HostSessionStop::RecoveryBlocked => {
                        return Err(crate::RpcFailure {
                            code: "HostRecoveryBlocked".into(),
                            message: "cancelled launch requires host recovery".into(),
                        })
                    }
                    _ => return Err(launch_reservation::cancelled()),
                }
            }
            outcome
        })
        .await
        .map_err(host_worker_failure)?
    }

    #[cfg(test)]
    pub(crate) fn with_reserved_route_probe(mut self, probe: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.before_reserved_route = Some(probe);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_reserved_start_probe(mut self, probe: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.after_reserved_start = Some(probe);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_session_rpc_probe(mut self, probe: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        self.after_session_rpc = Some(probe);
        self
    }

    #[cfg(test)]
    fn session_rpc_probe(&self, method: &str) {
        if let Some(probe) = &self.after_session_rpc {
            probe(method);
        }
    }

    fn start_reserved_blocking(
        &self,
        request: &crate::SessionStartRequest,
        reservation: &launch_reservation::LaunchReservation,
    ) -> Result<crate::game_routes::SelectedGameLaunch, RpcFailure> {
        #[cfg(test)]
        if let Some(probe) = &self.before_reserved_route {
            probe();
        }
        let config = self.config.as_ref().map_err(config_failure)?;
        let launcher = self.launcher.as_ref().expect("valid host config");
        let (command, runner_id, warnings) =
            if config.games.iter().any(|game| game.id == request.game_id) {
                // Preserve the command producer's existing collision check.
                if let Some(dynamic) = &self.dynamic {
                    dynamic.load()?.validate_static_games(config)?;
                }
                if request.runner_id.is_some() || request.overrides.is_some() {
                    return Err(dynamic_failure(
                        "host command games do not accept installed runner choices or overrides",
                    ));
                }
                (
                    launcher.command(&request.game_id)?.to_vec(),
                    None,
                    Vec::new(),
                )
            } else {
                let root = self.route_root()?;
                let registry = self.route_registry()?;
                let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
                    .map_err(crate::game_routes::settings_failure)?;
                let route = crate::config::resolver::resolve_linux_route(
                    root,
                    &snapshot,
                    &registry,
                    &request.game_id,
                    request.runner_id.as_deref(),
                )
                .map_err(|error| crate::route_diagnostic_failure(&error))?;
                let launch = linux_plugin::launch_route(
                    root,
                    &snapshot,
                    &registry,
                    &route,
                    request.overrides.clone(),
                )
                .map_err(|error| crate::game_routes::unavailable(error.to_string()))?;
                (launch.command, Some(route.runner_id), launch.warnings)
            };
        let session = launcher.prepare_reserved(reservation, runner_id.as_deref(), &command)?;
        // Delay only after the session transition releases its lock, allowing
        // native control to accept a newer Home before the start reply.
        #[cfg(test)]
        if let Some(probe) = &self.after_reserved_start {
            probe();
        }
        Ok(crate::game_routes::SelectedGameLaunch { session, warnings })
    }

    pub(crate) async fn cancel_launch(
        &self,
        launch_id: &str,
        caller: &str,
        person: Option<&str>,
    ) -> Result<HostSessionStop, RpcFailure> {
        match self.launch_reservations.cancel(launch_id, caller, person)? {
            Some(false) => Ok(HostSessionStop::Completed {
                launch_id: launch_id.into(),
            }),
            Some(true) => Ok(HostSessionStop::AlreadyStopping {
                launch_id: launch_id.into(),
            }),
            None => self.session_stop(launch_id).await,
        }
    }

    pub async fn prepare_selected(
        &self,
        request: crate::game_routes::SelectedGameLaunchRequest,
        person_public_key: Option<&str>,
    ) -> Result<crate::game_routes::SelectedGameLaunch, RpcFailure> {
        let runtime = self.clone();
        let person_public_key = person_public_key.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            let root = runtime.route_root()?;
            let registry = runtime.route_registry()?;
            let config = runtime.config.as_ref().map_err(config_failure)?;
            if config.games.iter().any(|game| game.id == request.game_id) {
                return Err(dynamic_failure("installed game collides with host.toml"));
            }
            let launch = crate::game_routes::selected_launch(root, &registry, &request)?;
            let session = runtime
                .launcher
                .as_ref()
                .expect("valid host config")
                .prepare_fresh_route(
                    &request.game_id,
                    &request.runner_id,
                    person_public_key.as_deref(),
                    &launch.command,
                )?;
            Ok(crate::game_routes::SelectedGameLaunch {
                session,
                warnings: launch.warnings,
            })
        })
        .await
        .map_err(host_worker_failure)?
    }

    fn control(&self) -> Result<&HostSessionControl, RpcFailure> {
        self.launcher
            .as_ref()
            .map(HostLauncher::control)
            .ok_or_else(|| config_failure(self.config.as_ref().expect_err("invalid host config")))
    }

    pub async fn session_status(&self) -> Result<HostSessionStatus, RpcFailure> {
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || Ok(runtime.control()?.status()))
            .await
            .map_err(host_worker_failure)?
    }

    pub async fn session_status_with_recovered_overlay_intent(
        &self,
    ) -> Result<
        (
            HostSessionStatus,
            Option<crate::FocusOwnership>,
            Option<String>,
        ),
        RpcFailure,
    > {
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            let outcome = runtime.control()?.status_with_recovered_overlay_intent();
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.status");
            Ok(outcome)
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub(crate) async fn observe_session_status<T: Send + 'static>(
        &self,
        has_owned_pending: bool,
        observe: impl FnOnce(
                HostSessionStatus,
                Option<crate::FocusOwnership>,
                Option<crate::InitialHandoff>,
                Option<String>,
            ) -> T
            + Send
            + 'static,
    ) -> Result<T, RpcFailure> {
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            let control = runtime.control()?;
            let observe = |status, ownership, handoff, recovered| {
                if let HostSessionStatus::Completed { launch_id }
                | HostSessionStatus::Failed { launch_id, .. } = &status
                {
                    runtime.launch_reservations.retire_completed(launch_id);
                }
                observe(status, ownership, handoff, recovered)
            };
            let outcome = if has_owned_pending {
                control.try_observe_status_with_recovered_overlay_intent(
                    PENDING_STATUS_LOCK_WAIT,
                    observe,
                )
            } else {
                // Ordinary overlay/native/peer observations retain their
                // blocking semantics; only owned startup needs a short bound.
                Some(control.observe_status_with_recovered_overlay_intent(observe))
            }
            .ok_or_else(|| RpcFailure {
                code: "HostSessionBusy".into(),
                message: "native session observation is busy; live state is unknown".into(),
            })?;
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.status");
            Ok(outcome)
        })
        .await
        .map_err(host_worker_failure)?
    }

    /// Observe session completion and input focus even without a portal freezer.
    pub fn spawn_portal_watch(&self) {
        if let Some(watch) = self.portal_watch(PORTAL_WATCH_INTERVAL) {
            tokio::spawn(watch);
        }
    }

    /// A frozen portal cannot poll, and a game can end without any caller.
    /// Observe focus as well as liveness, including a delayed first window
    /// while the portal is still running. The first pass
    /// runs at once, which releases a portal left frozen by an earlier run.
    fn portal_watch(
        &self,
        interval: std::time::Duration,
    ) -> Option<impl std::future::Future<Output = ()> + Send + 'static> {
        self.launcher.as_ref()?;
        let runtime = self.clone();
        Some(async move {
            let mut ticks = tokio::time::interval(interval);
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticks.tick().await;
                let observed = runtime.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(control) = observed.control() {
                        control.status();
                    }
                })
                .await;
            }
        })
    }

    #[cfg(test)]
    pub(crate) fn with_compositor(
        mut self,
        compositor: Arc<dyn CompositorControl>,
        never_focus: Vec<String>,
    ) -> Self {
        self.launcher = self
            .launcher
            .map(|launcher| launcher.with_compositor(compositor, never_focus));
        self
    }

    #[cfg(test)]
    pub(crate) fn with_portal_unit(mut self, portal: Arc<dyn systemd_unit::PortalUnit>) -> Self {
        self.launcher = self.launcher.map(|launcher| launcher.with_portal(portal));
        self
    }

    pub async fn session_stop(
        &self,
        expected_launch_id: &str,
    ) -> Result<HostSessionStop, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let outcome = runtime.control()?.stop(&expected_launch_id);
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.stop");
            Ok(outcome)
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub(crate) async fn observe_session_stop<T: Send + 'static>(
        &self,
        expected_launch_id: &str,
        observe: impl FnOnce(HostSessionStop) -> T + Send + 'static,
    ) -> Result<T, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let outcome = runtime
                .control()?
                .observe_stop(&expected_launch_id, observe);
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.stop");
            Ok(outcome)
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub async fn session_freeze(
        &self,
        expected_launch_id: &str,
    ) -> Result<HostSessionFreezeChange, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let outcome = runtime.control()?.freeze(&expected_launch_id);
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.freeze");
            Ok(outcome)
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub(crate) async fn observe_session_freeze<T: Send + 'static>(
        &self,
        expected_launch_id: &str,
        observe: impl FnOnce(HostSessionFreezeChange) -> T + Send + 'static,
    ) -> Result<T, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let outcome = runtime
                .control()?
                .observe_freeze(&expected_launch_id, observe);
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.freeze");
            Ok(outcome)
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub async fn session_thaw(
        &self,
        expected_launch_id: &str,
    ) -> Result<HostSessionFreezeChange, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let outcome = runtime.control()?.thaw(&expected_launch_id);
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.thaw");
            Ok(outcome)
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub(crate) async fn observe_session_thaw<T: Send + 'static>(
        &self,
        expected_launch_id: &str,
        observe: impl FnOnce(HostSessionFreezeChange) -> T + Send + 'static,
    ) -> Result<T, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let outcome = runtime
                .control()?
                .observe_thaw(&expected_launch_id, observe);
            #[cfg(test)]
            runtime.session_rpc_probe("app.session.thaw");
            Ok(outcome)
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub async fn session_controls(
        &self,
        expected_launch_id: &str,
    ) -> Result<SessionControls, SessionControlFailure> {
        let (launch_id, game_id) =
            exact_live_session(self.session_status().await, expected_launch_id)?;
        let runtime = self.clone();
        let materialized = tokio::task::spawn_blocking(move || {
            runtime.materialize_plugin_controls(&launch_id, game_id.as_deref())
        })
        .await
        .map_err(|_| unavailable_controls("Gameplay control resolution failed."))??;
        exact_live_session(self.session_status().await, expected_launch_id)?;
        Ok(materialized)
    }

    pub async fn invoke_session_control(
        &self,
        request: SessionControlInvokeRequest,
    ) -> SessionControlInvokeOutcome {
        let controls = match self.session_controls(&request.launch_id).await {
            Ok(controls) => controls,
            Err(failure) => return SessionControlInvokeOutcome::Err(failure),
        };
        let Some(control) = controls
            .groups
            .iter()
            .flat_map(|group| group.controls.iter())
            .find(|control| control.id == request.control_id)
        else {
            return SessionControlInvokeOutcome::Err(SessionControlFailure {
                reason: SessionControlFailureReason::UnknownControl,
                message: "That gameplay control is not declared for this exact launch.".into(),
            });
        };
        if let Err(failure) =
            crate::validate_session_control_invocation(&controls.launch_id, &request, control)
        {
            return SessionControlInvokeOutcome::Err(failure);
        }
        let record = match self.plugin_control_record(&request.launch_id, &request.control_id) {
            Ok(record) => record,
            Err(failure) => return SessionControlInvokeOutcome::Err(failure),
        };
        let command = record.effect.retroarch_command();
        let focus_after = record.effect == SessionControlEffect::RetroarchOpenMenu;
        let wait_for_completion = record.effect == SessionControlEffect::RetroarchQuit;
        let runtime = self.clone();
        let launch_id = request.launch_id.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let executor = Arc::clone(&runtime.retroarch_control);
            // Runner availability and command preflight happen while the exact
            // live session is still frozen. Only an executable declaration may
            // cross the one-way effect boundary and temporarily thaw it.
            let prepared = executor
                .prepare(command)
                .map_err(HostSessionEffectFailure::Unavailable)?;
            runtime
                .control()
                .map_err(|failure| HostSessionEffectFailure::Unavailable(failure.message))?
                .invoke_running_effect(&launch_id, focus_after, wait_for_completion, || {
                    executor.invoke(prepared)
                })
        })
        .await;
        match outcome {
            Ok(Ok(())) => SessionControlInvokeOutcome::Ok(SessionControlCompleted {
                launch_id: request.launch_id,
            }),
            Ok(Err(
                HostSessionEffectFailure::NoActive | HostSessionEffectFailure::StaleIdentity,
            )) => SessionControlInvokeOutcome::Err(stale_controls()),
            Ok(Err(HostSessionEffectFailure::Stopping)) => {
                SessionControlInvokeOutcome::Err(unavailable_controls("The game is stopping."))
            }
            Ok(Err(HostSessionEffectFailure::RecoveryBlocked)) => {
                SessionControlInvokeOutcome::Err(unavailable_controls(
                    "Gameplay control is blocked until session recovery is resolved.",
                ))
            }
            Ok(Err(HostSessionEffectFailure::FocusFailed(message)))
            | Ok(Err(HostSessionEffectFailure::Unavailable(message))) => {
                SessionControlInvokeOutcome::Err(unavailable_controls(&message))
            }
            Err(_) => SessionControlInvokeOutcome::Err(unavailable_controls(
                "Gameplay control execution failed.",
            )),
        }
    }

    fn plugin_control_record(
        &self,
        launch_id: &str,
        control_id: &str,
    ) -> Result<crate::plugin::SessionControlRecord, SessionControlFailure> {
        let runner_id = self
            .control()
            .map_err(|failure| unavailable_controls(&failure.message))?
            .active_runner_id(launch_id)
            .map_err(|message| unavailable_controls(&message))?
            .ok_or_else(stale_controls)?;
        self.route_registry()
            .map_err(|failure| unavailable_controls(&failure.message))?
            .session_controls()
            .values()
            .find(|record| {
                record.id == control_id
                    && record.owner.kind == SessionControlOwnerKind::Runner
                    && record.owner.id == runner_id
            })
            .cloned()
            .ok_or_else(|| SessionControlFailure {
                reason: SessionControlFailureReason::UnknownControl,
                message: "That gameplay control is not declared for this exact launch.".into(),
            })
    }

    fn materialize_plugin_controls(
        &self,
        launch_id: &str,
        game_id: Option<&str>,
    ) -> Result<SessionControls, SessionControlFailure> {
        let runner_id = self
            .control()
            .map_err(|failure| unavailable_controls(&failure.message))?
            .active_runner_id(launch_id)
            .map_err(|message| unavailable_controls(&message))?;
        let Some(runner_id) = runner_id else {
            return Ok(SessionControls {
                launch_id: launch_id.to_owned(),
                title: game_id.map(str::to_owned),
                groups: Vec::new(),
                retroarch_telemetry: None,
            });
        };
        let registry = self
            .route_registry()
            .map_err(|failure| unavailable_controls(&failure.message))?;
        let mut records = registry
            .session_controls()
            .values()
            .filter(|record| {
                record.owner.kind == SessionControlOwnerKind::Runner && record.owner.id == runner_id
            })
            .filter_map(|record| match record.interaction {
                SessionControlDeclarationInteraction::Command => Some(record.clone()),
                // Stateful forms need live executor values. Omit them instead
                // of inventing defaults or showing disabled controls.
                _ => None,
            })
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.local_id.cmp(&right.local_id))
        });
        let mut groups: Vec<SessionControlGroup> = Vec::new();
        for record in records {
            let control = SessionControl {
                id: record.id,
                label: record.label,
                description: record.description,
                enabled: true,
                disabled_reason: None,
                destructive: record.destructive,
                dismiss_on_success: record.dismiss_on_success,
                interaction: SessionControlInteraction::Command,
            };
            if let Some(group) = groups.iter_mut().find(|group| group.id == record.plugin_id) {
                group.controls.push(control);
            } else {
                groups.push(SessionControlGroup {
                    id: record.plugin_id.clone(),
                    label: registry
                        .plugin_title(&record.plugin_id)
                        .unwrap_or(&record.plugin_id)
                        .to_owned(),
                    controls: vec![control],
                });
            }
        }
        let title = game_id.and_then(|game_id| {
            self.dynamic
                .as_ref()
                .and_then(|dynamic| dynamic.load().ok())
                .and_then(|dynamic| {
                    dynamic
                        .games
                        .into_iter()
                        .find(|game| game.id == game_id)
                        .map(|game| game.title)
                })
        });
        Ok(SessionControls {
            launch_id: launch_id.to_owned(),
            title: title.or_else(|| game_id.map(str::to_owned)),
            groups,
            retroarch_telemetry: None,
        })
    }

    fn certificate_control_permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, RpcFailure> {
        Arc::clone(&self.moonlight_certificate_permits)
            .try_acquire_owned()
            .map_err(|_| RpcFailure {
                code: "StreamCertificateControlBusy".into(),
                message: "Stream certificate control is busy".into(),
            })
    }

    pub async fn moonlight_certificate_attest(&self, host_uuid: &str) -> Result<bool, RpcFailure> {
        let permit = self.certificate_control_permit()?;
        let adapter = Arc::clone(&self.moonlight_certificate);
        let host_uuid = host_uuid.to_owned();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            adapter.attest(&host_uuid)
        })
        .await
        .map_err(certificate_worker_failure)?
    }

    pub async fn moonlight_certificate_provision(
        &self,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<crate::MoonlightCertificateProvisioned, RpcFailure> {
        let permit = self.certificate_control_permit()?;
        let adapter = Arc::clone(&self.moonlight_certificate);
        let host_uuid = host_uuid.to_owned();
        let client_certificate = client_certificate.to_owned();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            adapter.provision(&host_uuid, &client_certificate)
        })
        .await
        .map_err(certificate_worker_failure)?
    }

    pub async fn moonlight_certificate_revoke(
        &self,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<bool, RpcFailure> {
        let permit = self.certificate_control_permit()?;
        let adapter = Arc::clone(&self.moonlight_certificate);
        let host_uuid = host_uuid.to_owned();
        let client_certificate = client_certificate.to_owned();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            adapter.revoke(&host_uuid, &client_certificate)
        })
        .await
        .map_err(certificate_worker_failure)?
    }
}

fn unavailable_controls(message: &str) -> SessionControlFailure {
    SessionControlFailure {
        reason: SessionControlFailureReason::Unavailable,
        message: message.to_owned(),
    }
}

fn stale_controls() -> SessionControlFailure {
    SessionControlFailure {
        reason: SessionControlFailureReason::StaleSession,
        message: "That game is no longer running. Reload the session before acting.".into(),
    }
}

fn exact_live_session(
    status: Result<HostSessionStatus, RpcFailure>,
    expected_launch_id: &str,
) -> Result<(String, Option<String>), SessionControlFailure> {
    match status {
        Ok(HostSessionStatus::Running { launch_id, game_id })
        | Ok(HostSessionStatus::Frozen { launch_id, game_id })
        | Ok(HostSessionStatus::FocusFailed { launch_id, game_id })
            if launch_id == expected_launch_id =>
        {
            Ok((launch_id, game_id))
        }
        Ok(HostSessionStatus::Stopping { launch_id, .. }) if launch_id == expected_launch_id => {
            Err(unavailable_controls("The game is stopping."))
        }
        Ok(HostSessionStatus::Running { .. })
        | Ok(HostSessionStatus::Frozen { .. })
        | Ok(HostSessionStatus::FocusFailed { .. })
        | Ok(HostSessionStatus::Stopping { .. })
        | Ok(HostSessionStatus::Completed { .. })
        | Ok(HostSessionStatus::Failed { .. })
        | Ok(HostSessionStatus::NoActive) => Err(stale_controls()),
        Ok(HostSessionStatus::RecoveryBlocked) => Err(unavailable_controls(
            "Host recovery identity requires administrator resolution.",
        )),
        Err(failure) => Err(unavailable_controls(&failure.message)),
    }
}

fn certificate_worker_failure(_error: tokio::task::JoinError) -> RpcFailure {
    RpcFailure {
        code: "StreamCertificateControlFailed".into(),
        message: "Stream certificate control worker failed".into(),
    }
}

fn host_worker_failure(error: tokio::task::JoinError) -> RpcFailure {
    RpcFailure {
        code: "HostControlFailed".into(),
        message: format!("host control worker failed: {error}"),
    }
}

fn config_failure(error: &HostConfigError) -> RpcFailure {
    RpcFailure {
        code: "HostConfigInvalid".into(),
        message: error.to_string(),
    }
}

fn dynamic_failure(message: impl Into<String>) -> RpcFailure {
    RpcFailure {
        code: "HostLibraryInvalid".into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::systemd_unit::{LaunchUnitError, LaunchUnitState};
    use std::{
        collections::BTreeMap,
        fs,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Condvar, Mutex,
        },
        thread,
        time::Duration,
    };

    fn count_runtime(root: &Path, pool: Arc<input_seat::RecordingInputPool>) -> HostRuntime {
        crate::config::test_fixtures::gba(root);
        let registry = crate::plugin_test_fixtures::installed(root);
        let path = root.join("host.toml");
        fs::write(&path, "label = \"count-test\"\n[[games]]\nid = \"one\"\ntitle = \"One\"\ncommand = [\"game\"]\n").unwrap();
        let backend = Arc::new(systemd_unit::InMemoryLaunchUnitBackend::default());
        let mut runtime = HostRuntime::from_paths_with_backend(
            &path,
            Some(root.into()),
            root.join("private"),
            backend.clone(),
        )
        .with_route_registry(root.into(), registry);
        runtime.launcher = Some(HostLauncher::with_backends(
            runtime.config.as_ref().unwrap(),
            &root.join("private"),
            backend,
            pool,
        ));
        runtime.control().unwrap().initialize_input(4).unwrap();
        runtime
    }

    struct GameUnitFixture {
        root: tempfile::TempDir,
        properties: PathBuf,
        backend: Arc<systemd_unit::SystemdLaunchUnitBackend>,
        runtime: HostRuntime,
        router: axum::Router,
    }

    impl GameUnitFixture {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;

            let root = tempfile::tempdir().unwrap();
            let config = root.path().join("host.toml");
            fs::write(&config, "label = \"startup-test\"\n[[games]]\nid = \"one\"\ntitle = \"One\"\ncommand = [\"game\"]\n").unwrap();
            let properties = root.path().join("unit-properties");
            let helper = root.path().join("systemctl");
            fs::write(&helper, format!(r#"#!/bin/sh
case "$3" in
    --quiet)
        if test -e '{launch_refused}'; then
            echo 'job failed' >&2
            exit 1
        fi
        ;;
    show)
        cat '{properties}'
        if test -e '{fail_after_query}'; then
            printf 'LoadState=loaded\nActiveState=failed\nFreezerState=running\nExecMainCode=1\nExecMainStatus=1\n' > '{properties}'
        fi
        ;;
    reset-failed)
        if test -e '{reset_refused}'; then exit 3; fi
        printf 'LoadState=not-found\n' > '{properties}'
        ;;
    list-units) exit 0;;
esac
"#,
                launch_refused = root.path().join("launch-refused").display(),
                properties = properties.display(),
                fail_after_query = root.path().join("fail-after-query").display(),
                reset_refused = root.path().join("reset-refused").display(),
            )).unwrap();
            fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
            let backend = Arc::new(
                systemd_unit::SystemdLaunchUnitBackend::new(helper.clone(), helper, 1000, 1000)
                    .unwrap(),
            );
            let runtime = HostRuntime::from_paths_with_backend(
                &config,
                None,
                root.path().join("private"),
                backend.clone(),
            );
            let (router, _) = crate::plain_host_routers_for_tests(runtime.clone());
            let fixture = Self {
                root,
                properties,
                backend,
                runtime,
                router,
            };
            fixture.state("active", 0, 0);
            fixture
        }

        fn portal() -> Self {
            let mut fixture = Self::new();
            let (lan, _) = crate::app_states(fixture.runtime.clone());
            fixture.router = crate::portal_router_for(
                &lan,
                Some(crate::PortalAccess::new(
                    "startup-test-capability",
                    "http://korrid.test",
                    crate::PortalPermission::Full,
                )),
            )
            .unwrap();
            fixture
        }

        fn state(&self, active: &str, code: i32, status: i32) {
            // These are native systemctl properties, read by the real bounded
            // subprocess adapter. Exact reset-failed collects the failed unit.
            fs::write(&self.properties, format!("LoadState=loaded\nActiveState={active}\nFreezerState=running\nExecMainCode={code}\nExecMainStatus={status}\n")).unwrap();
        }

        fn restart(&mut self) {
            let runtime = HostRuntime::from_paths_with_backend(
                &self.root.path().join("host.toml"),
                None,
                self.root.path().join("private"),
                self.backend.clone(),
            );
            self.runtime = runtime.clone();
            self.router = crate::plain_host_routers_for_tests(runtime).0;
        }

        async fn rpc(&self, body: &str) -> serde_json::Value {
            use axum::{body::Body, http::Request};
            use tower::ServiceExt;

            let response = self
                .router
                .clone()
                .oneshot(
                    Request::post("/rpc")
                        .header("content-type", "application/json")
                        .header("authorization", "Bearer startup-test-capability")
                        .header("origin", "http://korrid.test")
                        .body(Body::from(body.to_owned()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(response.status().is_success());
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }

        async fn prepare(&self) -> serde_json::Value {
            self.rpc(r#"{"_tag":"app.session.prepare","payload":{"gameId":"one"}}"#)
                .await
        }

        async fn status(&self) -> serde_json::Value {
            self.rpc(r#"{"_tag":"app.session.status","payload":{}}"#)
                .await
        }
    }

    #[tokio::test]
    async fn game_startup_failure_reports_exit_status_instead_of_completion() {
        let fixture = GameUnitFixture::new();
        let prepared = fixture.prepare().await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok", "{prepared}");
        let before = fixture.status().await;
        assert_eq!(
            before["outcome"]["payload"]["active"]["initialHandoff"],
            "waiting"
        );

        fixture.state("failed", 1, 1);
        let failed = fixture.status().await;
        assert_eq!(failed["outcome"]["_tag"], "Err", "{failed}");
        assert_eq!(failed["outcome"]["payload"]["code"], "HostLaunchFailed");
        assert!(failed["outcome"]["payload"]["message"]
            .as_str()
            .unwrap()
            .contains("exit status 1"));
        // The next poll must retain the failure after native unit collection.
        assert_eq!(fixture.status().await, failed);

        fixture.state("active", 0, 0);
        let next = fixture.prepare().await;
        assert_eq!(next["outcome"]["_tag"], "Ok", "{next}");
        assert_ne!(
            next["outcome"]["payload"]["launchId"],
            prepared["outcome"]["payload"]["launchId"]
        );
        fixture.state("inactive", 1, 0);
        let completed = fixture.status().await;
        assert_eq!(
            completed["outcome"]["payload"]["code"], "SessionCompleted",
            "{completed}"
        );
    }

    #[tokio::test]
    async fn game_startup_failure_already_observed_during_prepare_reports_exit_status() {
        let fixture = GameUnitFixture::new();
        fixture.state("failed", 1, 1);
        let prepared = fixture.prepare().await;
        assert_eq!(
            prepared["outcome"]["payload"]["code"], "HostLaunchFailed",
            "{prepared}"
        );
        assert!(prepared["outcome"]["payload"]["message"]
            .as_str()
            .unwrap()
            .contains("exit status 1"));
        fixture.state("active", 0, 0);
        let next = fixture.prepare().await;
        assert_eq!(next["outcome"]["_tag"], "Ok", "{next}");
    }

    #[tokio::test]
    async fn game_startup_failure_survives_executor_restart_until_observed() {
        let mut fixture = GameUnitFixture::new();
        let prepared = fixture.prepare().await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok", "{prepared}");
        fixture.state("failed", 1, 1);
        fixture.restart();
        let failed = fixture.status().await;
        assert_eq!(
            failed["outcome"]["payload"]["code"], "HostLaunchFailed",
            "{failed}"
        );
        assert!(failed["outcome"]["payload"]["message"]
            .as_str()
            .unwrap()
            .contains("exit status 1"));
        fixture.state("active", 0, 0);
        let next = fixture.prepare().await;
        assert_eq!(next["outcome"]["_tag"], "Ok", "{next}");
    }

    #[tokio::test]
    async fn game_startup_failure_when_native_start_job_fails_reports_exit_status() {
        let fixture = GameUnitFixture::new();
        fs::write(fixture.root.path().join("launch-refused"), "").unwrap();
        fixture.state("failed", 1, 203);
        let prepared = fixture.prepare().await;
        assert_eq!(
            prepared["outcome"]["payload"]["code"], "HostLaunchFailed",
            "{prepared}"
        );
        assert!(
            prepared["outcome"]["payload"]["message"]
                .as_str()
                .unwrap()
                .contains("exit status 203"),
            "{prepared}"
        );
        fs::remove_file(fixture.root.path().join("launch-refused")).unwrap();
        fixture.state("active", 0, 0);
        let next = fixture.prepare().await;
        assert_eq!(next["outcome"]["_tag"], "Ok", "{next}");
    }

    #[tokio::test]
    async fn game_startup_failure_observed_by_freeze_retains_failed_status() {
        let fixture = GameUnitFixture::new();
        let prepared = fixture.prepare().await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok", "{prepared}");
        fs::write(fixture.root.path().join("fail-after-query"), "").unwrap();
        let request = serde_json::json!({"_tag": "app.session.freeze", "payload": {"expectedLaunchId": prepared["outcome"]["payload"]["launchId"]}});
        fixture.rpc(&request.to_string()).await;
        let failed = fixture.status().await;
        assert_eq!(
            failed["outcome"]["payload"]["code"], "HostLaunchFailed",
            "{failed}"
        );
        assert!(failed["outcome"]["payload"]["message"]
            .as_str()
            .unwrap()
            .contains("exit status 1"));
    }

    #[tokio::test]
    async fn game_startup_failure_is_not_dropped_when_an_owned_launch_is_pending() {
        let fixture = GameUnitFixture::portal();
        let prepared = fixture.prepare().await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok", "{prepared}");
        let reserved = fixture
            .rpc(r#"{"_tag":"app.session.reserve","payload":{"gameId":"one"}}"#)
            .await;
        assert_eq!(reserved["outcome"]["_tag"], "Ok", "{reserved}");
        fixture.state("failed", 1, 1);
        let failed = fixture.status().await;
        assert_eq!(failed["outcome"]["_tag"], "Ok", "{failed}");
        let payload = &failed["outcome"]["payload"];
        assert_eq!(
            payload["pendingLaunches"][0]["session"]["launchId"],
            reserved["outcome"]["payload"]["launchId"]
        );
        assert_eq!(
            payload["observationFailure"]["code"], "HostLaunchFailed",
            "{failed}"
        );
        assert!(payload["observationFailure"]["message"]
            .as_str()
            .unwrap()
            .contains("exit status 1"));
    }

    #[tokio::test]
    async fn game_startup_failure_with_refused_native_cleanup_blocks_the_next_launch() {
        let fixture = GameUnitFixture::new();
        let prepared = fixture.prepare().await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok", "{prepared}");
        fs::write(fixture.root.path().join("reset-refused"), "").unwrap();
        fixture.state("failed", 1, 1);
        let failed = fixture.status().await;
        assert_eq!(
            failed["outcome"]["payload"]["code"], "HostRecoveryBlocked",
            "{failed}"
        );
        let next = fixture.prepare().await;
        assert_eq!(
            next["outcome"]["payload"]["code"], "HostRecoveryBlocked",
            "{next}"
        );
    }

    #[tokio::test]
    async fn game_startup_failure_recovery_precedes_input_initialization() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let fixture = GameUnitFixture::new();
        let prepared = fixture.prepare().await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok", "{prepared}");
        fixture.state("failed", 1, 1);

        // UnixInputSeatManager::route calls ready(), which rejects input before
        // initialize(). Configure that same startup order at the RPC boundary.
        let initialized = Arc::new(AtomicBool::new(false));
        let pool = Arc::new(input_seat::RecordingInputPool::default());
        let ready = initialized.clone();
        *pool.on_route.lock().unwrap() = Some(Box::new(move |_| {
            if ready.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err("input pool is not reconciled".into())
            }
        }));
        let ready = initialized.clone();
        *pool.on_apply.lock().unwrap() = Some(Box::new(move |_| {
            ready.store(true, Ordering::SeqCst);
        }));
        let config = HostConfig::read(&fixture.root.path().join("host.toml")).unwrap();
        let mut runtime = fixture.runtime.clone();
        runtime.launcher = Some(HostLauncher::with_backends(
            &config,
            &fixture.root.path().join("private"),
            fixture.backend.clone(),
            pool,
        ));
        let initialization = runtime.control().unwrap().initialize_input(4);
        let mut recovered = fixture;
        recovered.runtime = runtime.clone();
        recovered.router = crate::plain_host_routers_for_tests(runtime).0;
        let failed = recovered.status().await;
        assert_eq!(
            failed["outcome"]["payload"]["code"], "HostLaunchFailed",
            "{failed}"
        );
        assert!(failed["outcome"]["payload"]["message"]
            .as_str()
            .unwrap()
            .contains("exit status 1"));
        assert!(initialization.is_ok(), "{initialization:?}");
        assert!(initialized.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn count_success_means_acknowledged_pool_and_file_agree() {
        let root = tempfile::tempdir().unwrap();
        let pool = Arc::new(input_seat::RecordingInputPool::default());
        let runtime = count_runtime(root.path(), pool.clone());
        let before = runtime.settings_snapshot().await.unwrap();
        let after = runtime
            .update_player_count(crate::SettingsUpdateRequest {
                setting_id: crate::config::settings::PLAYER_COUNT_SETTING_ID.into(),
                expected_revision: before.revision,
                value: "6".into(),
            })
            .await
            .unwrap();
        assert_eq!(after.player_count, 6);
        assert_eq!(*pool.count.lock().unwrap(), 6);
        assert_eq!(
            runtime.settings_snapshot().await.unwrap().revision,
            after.revision
        );
        let prepared = runtime.prepare("one", None).await.unwrap();
        assert_eq!(
            pool.session.lock().unwrap().as_deref(),
            Some(prepared.launch_id.as_str())
        );
    }

    #[tokio::test]
    async fn refused_count_rolls_back_and_failed_compensation_fences_prepare() {
        for compensation_fails in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let pool = Arc::new(input_seat::RecordingInputPool::default());
            let runtime = count_runtime(root.path(), pool.clone());
            let before = runtime.settings_snapshot().await.unwrap();
            let original = fs::read(root.path().join("device.yaml")).unwrap();
            *pool.fail_counts.lock().unwrap() = if compensation_fails {
                vec![6, 4]
            } else {
                vec![6]
            };
            let error = runtime
                .update_player_count(crate::SettingsUpdateRequest {
                    setting_id: crate::config::settings::PLAYER_COUNT_SETTING_ID.into(),
                    expected_revision: before.revision,
                    value: "6".into(),
                })
                .await
                .unwrap_err();
            assert_eq!(error.code, "InputSeatUnavailable");
            assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), original);
            assert_eq!(pool.fenced.load(Ordering::SeqCst), compensation_fails);
            assert_eq!(
                runtime.prepare("one", None).await.is_err(),
                compensation_fails
            );
        }
    }

    #[tokio::test]
    async fn file_revision_change_during_runtime_ack_preserves_external_bytes_and_fences_launch() {
        let root = tempfile::tempdir().unwrap();
        let pool = Arc::new(input_seat::RecordingInputPool::default());
        let runtime = count_runtime(root.path(), pool.clone());
        let before = runtime.settings_snapshot().await.unwrap();
        let path = root.path().join("device.yaml");
        let mut external = fs::read(&path).unwrap();
        external.extend_from_slice(b"\n# external writer\n");
        let changed = external.clone();
        *pool.on_apply.lock().unwrap() =
            Some(Box::new(move |_| fs::write(&path, &changed).unwrap()));
        let error = runtime
            .update_player_count(crate::SettingsUpdateRequest {
                setting_id: crate::config::settings::PLAYER_COUNT_SETTING_ID.into(),
                expected_revision: before.revision,
                value: "6".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, "SettingsConflict");
        assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), external);
        assert!(pool.fenced.load(Ordering::SeqCst));
        assert!(runtime.prepare("one", None).await.is_err());
    }

    #[tokio::test]
    async fn concurrent_runner_choice_responses_keep_their_own_committed_revisions() {
        use crate::config::settings::{runner_choice_revisions, RunnerChoiceScope};
        use crate::game_routes::GameRunnerSetRequest;

        for scope in [
            RunnerChoiceScope::System("gba".into()),
            RunnerChoiceScope::Game(crate::config::test_fixtures::GBA_ID.into()),
        ] {
            let root = tempfile::tempdir().unwrap();
            crate::config::test_fixtures::gba(root.path());
            let config = root.path().join("host.toml");
            fs::write(&config, "label = \"route-device\"\ngames = []\n").unwrap();
            let runtime = HostRuntime::from_paths_with_backend(
                &config,
                Some(root.path().into()),
                root.path().join("private"),
                Arc::new(systemd_unit::InMemoryLaunchUnitBackend::default()),
            );
            let expected = |revisions: &crate::config::settings::RunnerChoiceRevisions| match &scope
            {
                RunnerChoiceScope::System(_) => revisions.device.clone(),
                RunnerChoiceScope::Game(_) => revisions.games.clone(),
            };
            let request = |revision, runner_id: &str| GameRunnerSetRequest {
                scope: scope.clone(),
                expected_revision: revision,
                runner_id: Some(runner_id.into()),
            };
            let before = runner_choice_revisions(root.path()).unwrap();
            let (entered, committed) = tokio::sync::oneshot::channel();
            let entered = Mutex::new(Some(entered));
            let (release, released) = std::sync::mpsc::channel();
            let released = Mutex::new(released);
            let mut delayed = runtime.clone();
            delayed.after_runner_choice_write = Some(Arc::new(move || {
                entered.lock().unwrap().take().unwrap().send(()).unwrap();
                released
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
            }));
            let first_request = request(expected(&before), "@test:first/core");
            let first = tokio::spawn(async move { delayed.set_game_runner(first_request).await });
            tokio::time::timeout(Duration::from_secs(10), committed)
                .await
                .unwrap()
                .unwrap();
            // Caller A has committed, but cannot deliver its response yet. Caller B
            // reads those bytes and commits a second real write through the host.
            let first_bytes = runner_choice_revisions(root.path()).unwrap();
            let second = runtime
                .set_game_runner(request(expected(&first_bytes), "@test:second/core"))
                .await
                .unwrap();
            // Also change the other document: the response must describe the pair
            // observed at A's commit, not a later mixed snapshot.
            let other_scope = match &scope {
                RunnerChoiceScope::System(_) => {
                    RunnerChoiceScope::Game(crate::config::test_fixtures::GBA_ID.into())
                }
                RunnerChoiceScope::Game(_) => RunnerChoiceScope::System("gba".into()),
            };
            let other_revision = match &other_scope {
                RunnerChoiceScope::System(_) => second.device.clone(),
                RunnerChoiceScope::Game(_) => second.games.clone(),
            };
            runtime
                .set_game_runner(GameRunnerSetRequest {
                    scope: other_scope,
                    expected_revision: other_revision,
                    runner_id: Some("@test:other/core".into()),
                })
                .await
                .unwrap();
            release.send(()).unwrap();
            let first = first.await.unwrap().unwrap();
            assert_eq!(
                first.device, first_bytes.device,
                "{scope:?}: device revision belongs to caller A"
            );
            assert_eq!(
                first.games, first_bytes.games,
                "{scope:?}: games revision belongs to caller A"
            );
            let conflict = runtime
                .set_game_runner(request(expected(&first), "@test:third/core"))
                .await
                .unwrap_err();
            assert_eq!(
                conflict.code, "SettingsConflict",
                "A must not overwrite B using A's response"
            );
        }
    }

    struct BlockingEnumerationBackend {
        entered: AtomicBool,
        released: (Mutex<bool>, Condvar),
    }

    impl LaunchUnitBackend for BlockingEnumerationBackend {
        fn launch(
            &self,
            _launch_id: &str,
            _command: &[String],
            _environment: &BTreeMap<String, String>,
        ) -> Result<(), LaunchUnitError> {
            unreachable!()
        }

        fn state(&self, _launch_id: &str) -> Result<LaunchUnitState, LaunchUnitError> {
            unreachable!()
        }

        fn stop(&self, _launch_id: &str) -> Result<(), LaunchUnitError> {
            unreachable!()
        }

        fn freeze(&self, _launch_id: &str) -> Result<(), LaunchUnitError> {
            unreachable!()
        }

        fn thaw(&self, _launch_id: &str) -> Result<(), LaunchUnitError> {
            unreachable!()
        }

        fn live_launch_ids(&self) -> Result<Vec<String>, LaunchUnitError> {
            self.entered.store(true, Ordering::SeqCst);
            let (lock, changed) = &self.released;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = changed.wait(released).unwrap();
            }
            Ok(Vec::new())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn systemd_and_identity_operations_run_off_the_async_worker() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        fs::write(&config, "label = \"zao\"\n").unwrap();
        let backend = Arc::new(BlockingEnumerationBackend {
            entered: AtomicBool::new(false),
            released: (Mutex::new(false), Condvar::new()),
        });
        let runtime = HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().join("private"),
            backend.clone(),
        );
        let worker_progressed = Arc::new(AtomicBool::new(false));
        let observed_progress = Arc::new(AtomicBool::new(false));
        let release_backend = backend.clone();
        let observe_progress = worker_progressed.clone();
        let observed = observed_progress.clone();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            observed.store(observe_progress.load(Ordering::SeqCst), Ordering::SeqCst);
            let (lock, changed) = &release_backend.released;
            *lock.lock().unwrap() = true;
            changed.notify_all();
        });

        let status = tokio::spawn(async move { runtime.session_status().await });
        while !backend.entered.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
        worker_progressed.store(true, Ordering::SeqCst);
        assert_eq!(status.await.unwrap().unwrap(), HostSessionStatus::NoActive);
        release.join().unwrap();
        assert!(observed_progress.load(Ordering::SeqCst));
    }

    struct BlockingCertificateAdapter {
        entered: AtomicUsize,
        released: (Mutex<bool>, Condvar),
    }

    impl MoonlightCertificateAdapter for BlockingCertificateAdapter {
        fn available(&self) -> bool {
            unreachable!()
        }

        fn attest(&self, _host_uuid: &str) -> Result<bool, RpcFailure> {
            self.entered.fetch_add(1, Ordering::SeqCst);
            let (lock, changed) = &self.released;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = changed.wait(released).unwrap();
            }
            Ok(true)
        }

        fn provision(
            &self,
            _host_uuid: &str,
            _client_certificate: &str,
        ) -> Result<crate::MoonlightCertificateProvisioned, RpcFailure> {
            unreachable!()
        }

        fn revoke(&self, _host_uuid: &str, _client_certificate: &str) -> Result<bool, RpcFailure> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn certificate_blocking_work_rejects_saturation_without_queueing() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        fs::write(&config, "label = \"zao\"\n").unwrap();
        let adapter = Arc::new(BlockingCertificateAdapter {
            entered: AtomicUsize::new(0),
            released: (Mutex::new(false), Condvar::new()),
        });
        let mut runtime = HostRuntime::from_paths(&config, None);
        runtime.moonlight_certificate = adapter.clone();

        let mut active = Vec::new();
        for _ in 0..MAX_CONCURRENT_CERTIFICATE_CONTROLS {
            let runtime = runtime.clone();
            active.push(tokio::spawn(async move {
                runtime.moonlight_certificate_attest("sunshine-host").await
            }));
        }
        while adapter.entered.load(Ordering::SeqCst) < MAX_CONCURRENT_CERTIFICATE_CONTROLS {
            tokio::task::yield_now().await;
        }

        let busy = runtime
            .moonlight_certificate_attest("sunshine-host")
            .await
            .unwrap_err();
        assert_eq!(busy.code, "StreamCertificateControlBusy");
        assert_eq!(
            adapter.entered.load(Ordering::SeqCst),
            MAX_CONCURRENT_CERTIFICATE_CONTROLS
        );

        let (lock, changed) = &adapter.released;
        *lock.lock().unwrap() = true;
        changed.notify_all();
        for task in active {
            assert!(task.await.unwrap().unwrap());
        }
    }

    #[tokio::test]
    async fn watcher_releases_completed_session_without_portal_unit_or_browser_polling() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        fs::write(&config, "label = \"watch-test\"\n[[games]]\nid = \"one\"\ntitle = \"One\"\ncommand = [\"game\"]\n").unwrap();
        let backend = Arc::new(systemd_unit::InMemoryLaunchUnitBackend::default());
        let pool = Arc::new(input_seat::RecordingInputPool::default());
        let mut runtime = HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().join("private"),
            backend.clone(),
        );
        runtime.launcher = Some(HostLauncher::with_backends(
            runtime.config.as_ref().unwrap(),
            &root.path().join("private"),
            backend.clone(),
            pool.clone(),
        ));
        runtime.prepare("one", None).await.unwrap();
        assert!(pool.session.lock().unwrap().is_some());
        let watcher = tokio::spawn(runtime.portal_watch(Duration::from_millis(10)).unwrap());
        backend.complete_live();
        tokio::time::timeout(Duration::from_secs(2), async {
            while pool.session.lock().unwrap().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        watcher.abort();
        let _ = watcher.await;
        assert!(pool
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.starts_with("end:")));
    }

    #[tokio::test]
    async fn portal_watch_retries_startup_thaw_without_any_client() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        fs::write(
            &config,
            "label = \"zao\"\n[[games]]\nid = \"wario\"\ntitle = \"Wario Land 4\"\ncommand = [\"game\"]\n",
        )
        .unwrap();
        let backend = Arc::new(systemd_unit::InMemoryLaunchUnitBackend::default());
        let portal = Arc::new(systemd_unit::RecordingPortalUnit::left_frozen());
        portal.refuse_next_thaws(2);
        let runtime = HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().join("private"),
            backend.clone(),
        )
        .with_portal_unit(portal.clone());

        let watch = tokio::spawn(
            runtime
                .portal_watch(Duration::from_millis(10))
                .expect("a configured portal is watched"),
        );
        // Startup releases a portal an earlier run left frozen and retries
        // native failures. No prepare/status/portal RPC drives this progress.
        tokio::time::timeout(Duration::from_secs(10), portal.thawed.notified())
            .await
            .expect("startup thaw retried within ten seconds");
        assert!(!portal.frozen());
        assert_eq!(portal.requests(), ["thaw", "thaw", "thaw"]);
        watch.abort();

        let unwatched =
            HostRuntime::from_paths_with_backend(&config, None, root.path().join("other"), backend);
        assert!(unwatched.portal_watch(Duration::from_millis(10)).is_some());
    }

    #[test]
    fn public_two_path_constructor_and_named_private_state_constructor_remain_available() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        fs::write(&config, "label = \"zao\"\n").unwrap();

        let public = HostRuntime::from_paths(&config, None);
        let private =
            HostRuntime::from_paths_with_private_state(&config, None, root.path().join("private"));

        assert!(public.catalog_snapshot().is_ok());
        assert!(private.catalog_snapshot().is_ok());
    }

    #[tokio::test]
    async fn catalog_derives_stats_for_only_the_authenticated_person() {
        const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        fs::write(
            &config,
            "label = \"zao\"\n[[games]]\nid = \"wario\"\ntitle = \"Wario Land 4\"\ncommand = [\"game\"]\n",
        )
        .unwrap();
        let private = root.path().join("private");
        let runtime = HostRuntime::from_paths_with_backend(
            &config,
            None,
            private.clone(),
            Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        );
        let store = play_log::PlayLogStore::new(&private);
        store
            .record(
                &play_log::PlayHistoryKey {
                    user_id: PERSON.into(),
                    game_id: "wario".into(),
                },
                play_log::PlayEntry {
                    occurred_at: "2026-09-04T10:00:00.000Z".into(),
                    duration_seconds: 75.0,
                    release_id: None,
                },
            )
            .unwrap();

        let own = runtime.catalog_snapshot_for(Some(PERSON)).await.unwrap();
        assert_eq!(
            own.games[0].play_stats,
            Some(crate::PlayStats {
                last_played: Some("2026-09-04T10:00:00.000Z".into()),
                play_count: 1,
                total_playtime_seconds: 75.0,
            })
        );
        assert_eq!(
            runtime
                .catalog_snapshot_for(Some(&"11".repeat(32)))
                .await
                .unwrap()
                .games[0]
                .play_stats,
            Some(crate::PlayStats {
                last_played: None,
                play_count: 0,
                total_playtime_seconds: 0.0,
            })
        );
        assert_eq!(
            runtime.catalog_snapshot().unwrap().games[0].play_stats,
            None
        );
    }

    #[test]
    fn linux_host_materializes_a_discovery_registered_gba() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let rom = folder.path().join("wl4.gba");
        fs::write(&rom, b"rom").unwrap();
        let report = crate::discovery::DiscoveryCoordinator::for_tests(root.path(), private.path())
            .add_location(
                folder.path(),
                &crate::discovery::DiscoveryOptions {
                    first_seen_at: "2026-08-05T00:00:00Z".into(),
                    ..crate::discovery::DiscoveryOptions::default()
                },
            )
            .unwrap();
        assert_eq!(report.added_games, 1);
        let snapshot = crate::config::snapshot::ConfigSnapshotCoordinator::new(root.path())
            .reload()
            .snapshot;
        let (game_id, game) = snapshot.games.iter().next().unwrap();
        let executable = root.path().join("retroarch");
        let core = root.path().join("mgba.so");
        let autoconfig = root.path().join("autoconfig");
        fs::write(&executable, b"binary").unwrap();
        fs::write(&core, b"core").unwrap();
        fs::create_dir(&autoconfig).unwrap();
        let registry = crate::plugin_test_fixtures::installed(root.path());
        let runtime = DynamicHostRuntime::from_root_with_registry(root.path(), &registry).unwrap();

        assert_eq!(runtime.games.len(), 1);
        // Discovery mints the catalog game id; the route must carry it through.
        assert_eq!(&runtime.games[0].id, game_id);
        assert_eq!(runtime.games[0].title, game.title);
        assert!(runtime.failures.is_empty(), "{:?}", runtime.failures);
        let launch = crate::game_routes::default_launch(root.path(), &registry, game_id).unwrap();
        let command = &launch.command;
        assert_eq!(command[1], "plugin-launch");
        let input: crate::launcher::plugin_launch::PluginLaunchInput =
            serde_json::from_str(&command[3]).unwrap();
        assert_eq!(input.program, executable.display().to_string());
        assert_eq!(
            input.core_path.as_deref(),
            Some(core.display().to_string().as_str())
        );
        assert_eq!(
            input.content_path,
            rom.canonicalize().unwrap().display().to_string()
        );
        assert!(
            !root.path().join("users").exists(),
            "catalog must not perform runtime writes"
        );
    }

    #[test]
    fn linux_host_materializes_wario_from_the_shared_plugins() {
        let root = tempfile::tempdir().unwrap();
        crate::config::test_fixtures::gba(root.path());
        fs::create_dir(root.path().join("roms")).unwrap();
        fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let executable = root.path().join("bin/retroarch");
        let core = root.path().join("cores/mgba.so");
        let autoconfig = root.path().join("share/libretro/autoconfig");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::create_dir_all(core.parent().unwrap()).unwrap();
        fs::create_dir_all(&autoconfig).unwrap();
        fs::write(&executable, b"binary").unwrap();
        fs::write(&core, b"core").unwrap();
        let registry = crate::plugin_test_fixtures::installed(root.path());

        // Retained catalog facts have no route after their last location is removed.
        let games_path = root.path().join("catalog/games.yaml");
        let releases_path = root.path().join("catalog/releases.yaml");
        let missing_sha = format!("sha256:{}", "a".repeat(64));
        fs::write(
            &games_path,
            format!(
                "{}  {}:\n    title: Missing game\n    releases: ['{missing_sha}']\n",
                fs::read_to_string(&games_path).unwrap(),
                crate::config::test_fixtures::OTHER_ID
            ),
        )
        .unwrap();
        fs::write(
            &releases_path,
            format!(
                "{}  '{missing_sha}':\n    game: {}\n    system: gba\n",
                fs::read_to_string(&releases_path).unwrap(),
                crate::config::test_fixtures::OTHER_ID
            ),
        )
        .unwrap();
        let runtime = DynamicHostRuntime::from_root_with_registry(root.path(), &registry).unwrap();

        assert_eq!(runtime.games.len(), 1);
        assert_eq!(runtime.games[0].id, "01K4J6K8Y00000000000000002");
        assert_eq!(
            runtime.games[0].identity,
            Some(GameIdentity::Hash(
                "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414".into()
            ))
        );
        let launch =
            crate::game_routes::default_launch(root.path(), &registry, &runtime.games[0].id)
                .unwrap();
        let command = &launch.command;
        assert_eq!(command[1], "plugin-launch");
        let input: crate::launcher::plugin_launch::PluginLaunchInput =
            serde_json::from_str(&command[3]).unwrap();
        assert_eq!(input.runner_id, "@korri:mgba/mgba");
        let explicit = tempfile::tempdir().unwrap();
        fs::write(explicit.path().join("wl4.gba"), b"rom").unwrap();
        fs::remove_file(root.path().join("roms/wl4.gba")).unwrap();
        let device_path = root.path().join("device.yaml");
        let mut device: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&device_path).unwrap()).unwrap();
        device["storage"]["selected"]["root"] = explicit.path().display().to_string().into();
        device["locations"][crate::config::test_fixtures::GBA_RELEASE]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::from_str("storage: selected\npath: wl4.gba\n").unwrap());
        fs::write(device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
        let runtime = DynamicHostRuntime::from_root_with_registry(root.path(), &registry).unwrap();
        assert_eq!(
            runtime.games.len(),
            1,
            "explicit copy must also materialize on Linux"
        );
        let launch =
            crate::game_routes::default_launch(root.path(), &registry, &runtime.games[0].id)
                .unwrap();
        let input: crate::launcher::plugin_launch::PluginLaunchInput =
            serde_json::from_str(&launch.command[3]).unwrap();
        assert_eq!(
            input.content_path,
            explicit.path().join("wl4.gba").display().to_string()
        );
        let config = root.path().join("host.toml");
        fs::write(&config, "label = \"zao\"\n[[games]]\nid = \"static\"\ntitle = \"Static game\"\ncommand = [\"game\"]\n").unwrap();
        let mut host =
            HostRuntime::from_paths_with_private_state(&config, None, root.path().join("private"));
        host.dynamic = Some(DynamicHostSource::Fixed(runtime));
        let catalog = host.catalog_snapshot().unwrap();
        assert_eq!(catalog.games.len(), 2);
        assert_eq!(catalog.games[0].id, "static");
        // Locality is shared; only the dynamic producer supports route selection.
        let wire = serde_json::to_value(&catalog).unwrap();
        assert_eq!(wire["games"][0]["supportsRunnerSelection"], false);
        assert_eq!(wire["games"][1]["supportsRunnerSelection"], true);
        assert!(catalog.games.iter().all(|game| game.source.is_local));
        let failures = catalog.failures.unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].code, "LocalRouteUnavailable");
        assert!(failures[0]
            .message
            .contains(crate::config::test_fixtures::OTHER_ID));
        host.config.as_mut().unwrap().games[0].id = crate::config::test_fixtures::OTHER_ID.into();
        assert_eq!(
            host.catalog_snapshot().unwrap_err().code,
            "HostLibraryInvalid"
        );
        assert_eq!(
            host.prepare_blocking(crate::config::test_fixtures::OTHER_ID, None)
                .unwrap_err()
                .code,
            "HostLibraryInvalid"
        );
        device["providers"]["@korri:retroarch"]["title"] = "Copied provider".into();
        fs::write(
            root.path().join("device.yaml"),
            serde_yaml::to_string(&device).unwrap(),
        )
        .unwrap();
        fs::remove_file(explicit.path().join("wl4.gba")).unwrap();
        let missing_media = DynamicHostRuntime::from_root_with_registry(root.path(), &registry);
        assert!(
            missing_media.is_ok(),
            "configuration opinions do not redeclare plugin runners"
        );
    }
}
