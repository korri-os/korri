mod config;
pub(crate) mod control;
mod identity;
mod input_seat;
pub(crate) mod moonlight_certificate;
pub(crate) mod play_log;
mod prepare;
mod session_state;
mod systemd_unit;

use crate::{
    config::{
        resolver::{resolve_launchable_routes_for_platform, RoutePlatform},
        snapshot::{ConfigSnapshotCoordinator, SnapshotAuthorization},
    },
    identity::DeviceIdentity,
    launcher::linux_retroarch,
    plugin_policy, CatalogSnapshot, Game, GameIdentity, GameSource, RpcFailure, SessionPrepared,
    SourceCatalogState, SourceStatus, SourceStreamControlState,
};
use config::{HostConfig, HostConfigError};
use moonlight_certificate::MoonlightCertificateAdapter;
use prepare::HostLauncher;
use session_state::{
    HostSessionControl, HostSessionFreezeChange, HostSessionStatus, HostSessionStop,
};
use std::{
    ffi::OsString,
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
    command: Vec<String>,
}

#[derive(Clone)]
struct DynamicHostRuntime {
    games: Vec<DynamicHostGame>,
}

impl DynamicHostRuntime {
    fn from_root(root: &Path) -> Result<Self, RpcFailure> {
        Self::from_root_with_env(root, |key| std::env::var_os(key))
    }

    fn from_root_with_env(
        root: &Path,
        lookup: impl Fn(&str) -> Option<OsString> + Copy,
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
        let registry = plugin_policy::registry_for_snapshot(&state.snapshot)
            .map_err(|error| dynamic_failure(error.to_string()))?;
        let catalog = resolve_launchable_routes_for_platform(
            &state.snapshot,
            &registry,
            std::iter::empty(),
            RoutePlatform::Linux,
        );
        if !catalog.diagnostics.is_empty() {
            return Err(dynamic_failure(
                catalog
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        let mut games = Vec::new();
        for route in catalog.routes {
            let launch = linux_retroarch::launch_route_with_env(root, &route, lookup)
                .map_err(|error| dynamic_failure(error.to_string()))?;
            games.push(DynamicHostGame {
                id: route.playable_id,
                title: route.title.unwrap_or(route.release_id),
                identity: route.identity,
                command: launch.command,
            });
        }
        Ok(Self { games })
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
    config: Result<HostConfig, HostConfigError>,
    launcher: Option<HostLauncher>,
    dynamic: Option<Result<DynamicHostRuntime, RpcFailure>>,
    device_public_key: Option<String>,
    owner_public_key: Option<String>,
    moonlight_certificate: Arc<dyn MoonlightCertificateAdapter>,
    moonlight_certificate_permits: Arc<tokio::sync::Semaphore>,
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
        let dynamic = storage_root.map(|root| DynamicHostRuntime::from_root(&root));
        let (device_public_key, owner_public_key) = identity_keys(&private_state_root);
        Self {
            config,
            launcher,
            dynamic,
            device_public_key,
            owner_public_key,
            moonlight_certificate: moonlight_certificate::production_adapter(),
            moonlight_certificate_permits: Arc::new(tokio::sync::Semaphore::new(
                MAX_CONCURRENT_CERTIFICATE_CONTROLS,
            )),
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
        let dynamic = storage_root.map(|root| DynamicHostRuntime::from_root(&root));
        let (device_public_key, owner_public_key) = identity_keys(&private_state_root);
        Self {
            config,
            launcher,
            dynamic,
            device_public_key,
            owner_public_key,
            moonlight_certificate: moonlight_certificate::production_adapter(),
            moonlight_certificate_permits: Arc::new(tokio::sync::Semaphore::new(
                MAX_CONCURRENT_CERTIFICATE_CONTROLS,
            )),
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
        let play_stats = self
            .launcher
            .as_ref()
            .map(|launcher| launcher.control().load_all_play_stats())
            .unwrap_or_default();
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
                play_stats: play_stats
                    .get(&game.id)
                    .cloned()
                    .filter(|s| s.play_count > 0 || s.last_played.is_some()),
                source: source.clone(),
                play_stats: stats_for(&game.id)?,
            });
        }
        if let Some(dynamic) = &self.dynamic {
            let dynamic = dynamic.as_ref().map_err(Clone::clone)?;
            for game in &dynamic.games {
                if games.iter().any(|existing| existing.id == game.id) {
                    return Err(dynamic_failure(format!(
                        "game id {:?} is declared by both host.toml and library.yaml",
                        game.id
                    )));
                }
                games.push(Game {
                    id: game.id.clone(),
                    title: game.title.clone(),
                    host: Some(config.label.clone()),
                    identity: game.identity.clone(),
                    play_stats: play_stats
                        .get(&game.id)
                        .cloned()
                        .filter(|s| s.play_count > 0 || s.last_played.is_some()),
                    source: source.clone(),
                    play_stats: stats_for(&game.id)?,
                });
            }
        }
        Ok(CatalogSnapshot {
            games,
            failures: None,
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
            let dynamic = dynamic.as_ref().map_err(Clone::clone)?;
            if let Some(game) = dynamic.games.iter().find(|game| game.id == game_id) {
                return launcher.prepare_command(game_id, person_public_key, &game.command);
            }
        }
        launcher.prepare(game_id, person_public_key)
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
        collections::{BTreeMap, HashMap},
        fs,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Condvar, Mutex,
        },
        thread,
        time::Duration,
    };

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
    fn linux_host_materializes_wario_from_the_shared_plugins() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("config.yaml"), "{}\n").unwrap();
        fs::write(
            root.path().join("library.yaml"),
            r#"
library:
  wl4:
    title: Wario Land 4
    releases:
      - id: gba
        system: gba
        target: { kind: file, storage: roms, path: wl4.gba }
        identity: { kind: hash, value: "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414" }
        launch:
          use: "@korri:retroarch/retroarch"
          runtime: "@korri:mgba/mgba"
"#,
        )
        .unwrap();
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
        let environment = HashMap::from([
            ("KORRI_RETROARCH_EXECUTABLE", executable.as_os_str()),
            ("KORRI_MGBA_CORE", core.as_os_str()),
            ("KORRI_RETROARCH_AUTOCONFIG", autoconfig.as_os_str()),
        ]);

        let runtime = DynamicHostRuntime::from_root_with_env(root.path(), |key| {
            environment.get(key).map(|value| OsString::from(value))
        })
        .unwrap();

        assert_eq!(runtime.games.len(), 1);
        assert_eq!(runtime.games[0].id, "wl4");
        assert_eq!(
            runtime.games[0].identity,
            Some(GameIdentity::Hash(
                "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414".into()
            ))
        );
        assert_eq!(
            runtime.games[0].command[0],
            executable.display().to_string()
        );
        assert_eq!(runtime.games[0].command[4], core.display().to_string());
    }
}
