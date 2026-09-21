mod compositor_focus;
mod config;
pub(crate) mod control;
mod identity;
mod input_seat;
pub(crate) mod moonlight_certificate;
pub(crate) mod play_log;
mod prepare;
pub(crate) mod retroarch_control;
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
    command: Result<Vec<String>, RpcFailure>,
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
        let mut emitted = std::collections::BTreeSet::new();
        for candidate in catalog.routes {
            if !emitted.insert(candidate.playable_id.clone()) {
                continue;
            }
            let command = crate::config::resolver::resolve_linux_route(
                root,
                &state.snapshot,
                registry,
                &candidate.playable_id,
                None,
            )
            .map_err(|error| crate::route_diagnostic_failure(&error))
            .and_then(|route| {
                linux_plugin::launch_route(root, &state.snapshot, registry, &route, None)
                    .map(|launch| {
                        failures.extend(launch.warnings.into_iter().map(|warning| RpcFailure {
                            code: "LaunchSettingUnsupported".into(),
                            message: warning.message,
                        }));
                        launch.command
                    })
                    .map_err(|error| dynamic_failure(error.to_string()))
            });
            // A game needing a chooser stays in the catalog. Its command is
            // unavailable until an explicit or stored runtime resolves it.
            if let Err(error) = &command {
                failures.push(error.clone());
            }
            games.push(DynamicHostGame {
                id: candidate.playable_id,
                title: candidate.title.unwrap_or(candidate.release_id),
                identity: candidate.identity,
                command,
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
    // Tests can delay one response after the real setter releases its lock.
    #[cfg(test)]
    after_runner_choice_write: Option<Arc<dyn Fn() + Send + Sync>>,
    config: Result<HostConfig, HostConfigError>,
    launcher: Option<HostLauncher>,
    dynamic: Option<DynamicHostSource>,
    device_public_key: Option<String>,
    owner_public_key: Option<String>,
    moonlight_certificate: Arc<dyn MoonlightCertificateAdapter>,
    moonlight_certificate_permits: Arc<tokio::sync::Semaphore>,
    retroarch_control: Arc<dyn RetroarchControlExecutor>,
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
        let config = HostConfig::read(path);
        let launcher = config
            .as_ref()
            .ok()
            .map(|config| HostLauncher::new(config, &private_state_root));
        let dynamic = storage_root.map(DynamicHostSource::Installed);
        let (device_public_key, owner_public_key) = identity_keys(&private_state_root);
        Self {
            private_state_root,
            route_write_lock: Arc::new(std::sync::Mutex::new(())),
            #[cfg(test)]
            after_runner_choice_write: None,
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
        }
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
            after_runner_choice_write: None,
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
    /// non-mutating probe of the protected Sunshine certificate control.
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
            runtime.prepare_blocking(&game_id, person_public_key.as_deref())
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
            if let Some(game) = dynamic.games.iter().find(|game| game.id == game_id) {
                return launcher.prepare_command(
                    game_id,
                    person_public_key,
                    game.command.as_deref().map_err(Clone::clone),
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
    ) -> Result<(HostSessionStatus, Option<String>), RpcFailure> {
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            Ok(runtime.control()?.status_with_recovered_overlay_intent())
        })
        .await
        .map_err(host_worker_failure)?
    }

    pub async fn session_stop(
        &self,
        expected_launch_id: &str,
    ) -> Result<HostSessionStop, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || Ok(runtime.control()?.stop(&expected_launch_id)))
            .await
            .map_err(host_worker_failure)?
    }

    pub async fn session_freeze(
        &self,
        expected_launch_id: &str,
    ) -> Result<HostSessionFreezeChange, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || Ok(runtime.control()?.freeze(&expected_launch_id)))
            .await
            .map_err(host_worker_failure)?
    }

    pub async fn session_thaw(
        &self,
        expected_launch_id: &str,
    ) -> Result<HostSessionFreezeChange, RpcFailure> {
        let runtime = self.clone();
        let expected_launch_id = expected_launch_id.to_owned();
        tokio::task::spawn_blocking(move || Ok(runtime.control()?.thaw(&expected_launch_id)))
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
                code: "SunshineCertificateControlBusy".into(),
                message: "Sunshine certificate control is busy".into(),
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
        | Ok(HostSessionStatus::NoActive) => Err(stale_controls()),
        Ok(HostSessionStatus::RecoveryBlocked) => Err(unavailable_controls(
            "Host recovery identity requires administrator resolution.",
        )),
        Err(failure) => Err(unavailable_controls(&failure.message)),
    }
}

fn certificate_worker_failure(_error: tokio::task::JoinError) -> RpcFailure {
    RpcFailure {
        code: "SunshineCertificateControlFailed".into(),
        message: "Sunshine certificate control worker failed".into(),
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
        assert_eq!(busy.code, "SunshineCertificateControlBusy");
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
        let command = runtime.games[0].command.as_ref().unwrap();
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
        let command = runtime.games[0].command.as_ref().unwrap();
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
        let input: crate::launcher::plugin_launch::PluginLaunchInput =
            serde_json::from_str(&runtime.games[0].command.as_ref().unwrap()[3]).unwrap();
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
