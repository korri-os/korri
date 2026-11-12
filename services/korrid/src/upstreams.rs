//! Ordered host registry: selects legacy or native transport per host.

#[path = "stream_play/registry.rs"]
mod stream_play;
use stream_play::Participation;

use crate::{
    federation::reach::SelectedViewer,
    linux_viewer::{LinuxStreamViewer, LinuxViewerStatus},
    peer_rpc::PeerCredentials,
    upstream::{UpstreamClient, UpstreamConfig, UpstreamSessionStatus, UpstreamSessionStop},
    upstream_native::{NativeClient, NATIVE_RPC_TIMEOUT},
    CatalogHostFailure, CatalogSnapshot, Game, GameSource, MoonlightCertificateProvisioned,
    MoonlightCertificateRevoked, SessionControlFailure, SessionControlFailureReason,
    SessionControlInvokeOutcome, SessionControlInvokeRequest, SessionControlsOutcome,
    SessionPrepared, SessionTransitionResult, SourceStatus,
};
use futures::future::join_all;
use serde::Deserialize;
use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::time::timeout;

const CATALOG_HOST_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT: Duration =
    Duration::from_secs(NATIVE_RPC_TIMEOUT.as_secs() * 2 + 1);
#[cfg(any(test, target_os = "android"))]
pub(crate) const MOONLIGHT_CERTIFICATE_CALLER_TIMEOUT: Duration =
    Duration::from_secs(MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT.as_secs() + 1);

#[derive(Clone, Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("upstream unreachable: {0}")]
    Unreachable(String),
    #[error("upstream returned HTTP {0}")]
    Http(u16),
    #[error("upstream wire error: {0}")]
    Wire(String),
    #[error("upstream returned {code}: {message}")]
    Tagged { code: String, message: String },
    #[error("upstream call failed: {0}")]
    Failure(String),
    #[error("certificate peer is unavailable")]
    MoonlightCertificatePeerUnavailable,
    #[error("certificate peer returned an invalid response")]
    MoonlightCertificatePeerProtocol,
    #[error("certificate peer rejected the request")]
    MoonlightCertificateRejected,
    #[error("certificate control is busy")]
    MoonlightCertificateBusy,
    #[error("Sunshine host UUID changed during certificate control")]
    MoonlightHostChanged,
    #[error("no configured native peer owns the requested Sunshine host UUID")]
    MoonlightHostNotFound,
    #[error("more than one configured native peer owns the requested Sunshine host UUID")]
    MoonlightHostAmbiguous,
    #[error("the selected remote session was replaced by a different launch")]
    SelectedRemoteSessionReplaced,
    #[error("a remote peer already has an active session")]
    ActiveRemoteSessionConflict,
    #[error("expectedLaunchId is required for exact remote session control")]
    ExpectedLaunchIdRequired,
    #[error("expectedLaunchId does not identify the selected remote peer launch")]
    StaleLaunchIdentity,
    #[error("no remote peer session is active")]
    NoActiveSession,
    #[error("the selected legacy session route does not support freezer control")]
    FreezerUnsupportedOnLegacyRoute,
    #[error("no configured native peer has the requested device public key")]
    SourcePeerNotFound,
    #[error("no valid explicit Moonlight host address is configured")]
    MoonlightHostCandidatesUnavailable,
}

impl UpstreamError {
    pub fn code(&self) -> &str {
        match self {
            Self::Unreachable(_) => "UpstreamUnreachable",
            Self::Http(_) => "UpstreamHttp",
            Self::Wire(_) => "UpstreamWire",
            Self::Tagged { code, .. } => code,
            Self::Failure(_) => "UpstreamFailure",
            Self::MoonlightCertificatePeerUnavailable => "MoonlightCertificatePeerUnavailable",
            Self::MoonlightCertificatePeerProtocol => "MoonlightCertificatePeerProtocol",
            Self::MoonlightCertificateRejected => "MoonlightCertificateRejected",
            Self::MoonlightCertificateBusy => "MoonlightCertificateBusy",
            Self::MoonlightHostChanged => "MoonlightHostChanged",
            Self::MoonlightHostNotFound => "MoonlightHostNotFound",
            Self::MoonlightHostAmbiguous => "MoonlightHostAmbiguous",
            Self::SelectedRemoteSessionReplaced => "SelectedRemoteSessionReplaced",
            Self::ActiveRemoteSessionConflict => "ActiveRemoteSessionConflict",
            Self::ExpectedLaunchIdRequired => "ExpectedLaunchIdRequired",
            Self::StaleLaunchIdentity => "StaleLaunchIdentity",
            Self::NoActiveSession => "NoActiveSession",
            Self::FreezerUnsupportedOnLegacyRoute => "FreezerUnsupportedOnLegacyRoute",
            Self::SourcePeerNotFound => "SourcePeerNotFound",
            Self::MoonlightHostCandidatesUnavailable => "MoonlightHostCandidatesUnavailable",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoonlightHostCandidate {
    pub label: String,
    pub address: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamHostConfig {
    label: String,
    kind: UpstreamKind,
    base_url: String,
    #[serde(default)]
    moonlight_address: Option<String>,
    #[serde(default)]
    device_public_key: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum UpstreamKind {
    Legacy,
    Native,
}

impl UpstreamHostConfig {
    pub fn legacy(label: impl Into<String>, base_url: String) -> Self {
        Self {
            label: label.into(),
            kind: UpstreamKind::Legacy,
            base_url,
            moonlight_address: None,
            device_public_key: None,
        }
    }

    pub fn native_secure(
        label: impl Into<String>,
        base_url: String,
        device_public_key: String,
    ) -> Self {
        Self {
            label: label.into(),
            kind: UpstreamKind::Native,
            base_url,
            moonlight_address: None,
            device_public_key: Some(device_public_key),
        }
    }

    #[cfg(test)]
    pub fn with_moonlight_address(mut self, moonlight_address: impl Into<String>) -> Self {
        self.moonlight_address = Some(moonlight_address.into());
        self
    }

    #[cfg(test)]
    pub fn native(label: impl Into<String>, base_url: String) -> Self {
        let label = label.into();
        let moonlight_address = Some(format!("{label}:47989"));
        let mut device_public_key = hex::encode(label.as_bytes());
        device_public_key.push_str(&"0".repeat(64));
        device_public_key.truncate(64);
        Self {
            label,
            kind: UpstreamKind::Native,
            base_url,
            moonlight_address,
            device_public_key: Some(device_public_key),
        }
    }
}

#[derive(Clone)]
enum RegisteredClient {
    Legacy(UpstreamClient),
    Native(NativeClient),
}

#[derive(Clone)]
struct RegisteredHost {
    label: String,
    moonlight_address: Option<String>,
    device_public_key: Option<String>,
    client: RegisteredClient,
}

impl RegisteredHost {
    async fn viewer_address(&self) -> Result<String, UpstreamError> {
        if let Some(address) = self
            .moonlight_address
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(address.to_owned());
        }
        let RegisteredClient::Native(client) = &self.client else {
            return Err(UpstreamError::MoonlightHostCandidatesUnavailable);
        };
        // Federation supplies authenticated RPC origins even when the endpoint
        // has no explicit viewer address. Probe Sunshine on those same hosts.
        let origins = moonlight_control_origins(&client.candidate_origins());
        let selected = crate::federation::connect::select_origin(&origins)
            .await
            .map_err(|_| UpstreamError::MoonlightHostCandidatesUnavailable)?;
        let address = origins[selected]
            .strip_prefix("http://")
            .expect("derived HTTP origin")
            .to_owned();
        log::info!("Moonlight control probe accepted address={address}");
        Ok(address)
    }

    fn native_route_key(&self) -> Option<String> {
        match &self.client {
            RegisteredClient::Native(_) => self.device_public_key.clone(),
            RegisteredClient::Legacy(_) => None,
        }
    }

    fn qualify_status(&self, mut status: UpstreamSessionStatus) -> UpstreamSessionStatus {
        if let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = &mut status
        {
            active.host = Some(self.label.clone());
        }
        status
    }

    async fn native_session_status(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        let RegisteredClient::Native(client) = &self.client else {
            return Err(UpstreamError::Failure("session peer is not native".into()));
        };
        client
            .session_status()
            .await
            .map(|status| self.qualify_status(status))
    }

    async fn catalog(&self, qualify_legacy_host: bool) -> Result<Vec<Game>, UpstreamError> {
        match &self.client {
            RegisteredClient::Legacy(client) => client.catalog_snapshot().await.map(|catalog| {
                catalog
                    .entries
                    .into_iter()
                    .filter(|entry| entry.launchable)
                    .map(|entry| {
                        let identity = entry.single_identity();
                        Game {
                            title: entry.title.unwrap_or_else(|| entry.id.clone()),
                            id: entry.id,
                            host: qualify_legacy_host.then(|| self.label.clone()),
                            identity,
                            source: GameSource {
                                device_public_key: None,
                                label: self.label.clone(),
                                is_local: false,
                            },
                            supports_runner_selection: false,
                            play_stats: None,
                        }
                    })
                    .collect()
            }),
            RegisteredClient::Native(client) => client.catalog_snapshot().await.map(|catalog| {
                catalog
                    .games
                    .into_iter()
                    .map(|mut game| {
                        game.host = Some(self.label.clone());
                        game.source = GameSource {
                            device_public_key: self.device_public_key.clone(),
                            label: self.label.clone(),
                            is_local: false,
                        };
                        game
                    })
                    .collect()
            }),
        }
    }
}

#[derive(Clone)]
struct DeferredFileConfig {
    path: PathBuf,
    credentials: Option<PeerCredentials>,
    resolved: Arc<OnceLock<UpstreamRegistry>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SelectedRemoteRoute {
    Legacy { label: String },
    Native { device_public_key: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SelectedRemoteSession {
    display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    participation: Participation,
    route: SelectedRemoteRoute,
    launch_id: String,
    viewer: Option<SelectedViewer>,
    /// Local viewer intent, using its existing Running/Backgrounded/Stopped
    /// lifecycle. Independent of the streamed game's execution phase.
    viewer_target: LinuxViewerStatus,
}

/// What a streamed launch carries beyond its game (ADR 0066): the runner the
/// person chose for it, and the values of this one launch.
#[derive(Clone, Debug, Default)]
pub struct StreamedLaunch {
    pub runner_id: Option<String>,
    pub launch: Option<crate::launch_settings::LaunchOverride>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FreezerVerb {
    Freeze,
    Thaw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveredHolderPhase {
    Running,
    Frozen,
    FocusFailed,
}

impl RecoveredHolderPhase {
    fn parse(phase: Option<&str>) -> Result<Self, UpstreamError> {
        match phase {
            Some("running") => Ok(Self::Running),
            Some("frozen") => Ok(Self::Frozen),
            Some("focus-failed") => Ok(Self::FocusFailed),
            Some("stopping") => Err(viewer_error(
                "NativeViewerRecoveryIncomplete",
                "streamed launch is stopping; viewer recovery is blocked",
            )),
            Some(other) => Err(viewer_error(
                "NativeViewerRecoveryIncomplete",
                format!("streamed launch has unknown phase {other:?}"),
            )),
            None => Err(viewer_error(
                "NativeViewerRecoveryIncomplete",
                "streamed launch has no recoverable phase",
            )),
        }
    }

    fn viewer_backgrounded(self) -> bool {
        matches!(self, Self::Frozen | Self::FocusFailed)
    }
}

fn default_viewer() -> Arc<dyn LinuxStreamViewer> {
    #[cfg(test)]
    {
        Arc::new(crate::linux_viewer::test_backend::TestLinuxStreamViewer::default())
    }
    #[cfg(not(test))]
    {
        Arc::new(crate::linux_viewer::UnavailableLinuxStreamViewer)
    }
}

mod live_shape;

#[derive(Clone)]
pub struct UpstreamRegistry {
    live_shape: Arc<Mutex<Option<live_shape::ClientShape>>>,
    last_stream_play_end: Arc<Mutex<Option<crate::stream_play::StreamPlayEnded>>>,
    hosts: Vec<RegisteredHost>,
    credentials: Option<PeerCredentials>,
    directory: Option<crate::federation::FederationDirectory>,
    configuration_error: Option<String>,
    deferred_file_config: Option<DeferredFileConfig>,
    selected_remote_session: Arc<Mutex<Option<SelectedRemoteSession>>>,
    remote_prepare_mutation: Arc<tokio::sync::Mutex<()>>,
    viewer: Arc<dyn LinuxStreamViewer>,
}

impl UpstreamRegistry {
    pub(crate) fn has_selected_session(&self) -> bool {
        self.selected().is_some()
    }

    fn build(configs: Vec<UpstreamHostConfig>, credentials: Option<PeerCredentials>) -> Self {
        let mut configuration_error = None;
        let hosts = configs
            .into_iter()
            .filter_map(|config| {
                let label = config.label;
                let device_public_key = match config.kind {
                    UpstreamKind::Native => config.device_public_key.clone(),
                    UpstreamKind::Legacy => None,
                };
                let moonlight_address = match config.kind {
                    UpstreamKind::Native => config.moonlight_address,
                    UpstreamKind::Legacy => None,
                };
                let client = match config.kind {
                    UpstreamKind::Legacy => {
                        if credentials.is_some() {
                            configuration_error = Some(format!(
                                "legacy plaintext upstream {label:?} is not supported"
                            ));
                            return None;
                        }
                        RegisteredClient::Legacy(UpstreamClient::new(UpstreamConfig {
                            base_url: config.base_url,
                        }))
                    }
                    UpstreamKind::Native => {
                        let base_url = match crate::federation::peer_origin(&config.base_url) {
                            Ok(origin) => origin,
                            Err(_) => {
                                configuration_error = Some(format!(
                                    "native upstream {label:?} has an invalid baseUrl origin"
                                ));
                                return None;
                            }
                        };
                        #[cfg(test)]
                        if credentials.is_none() {
                            return Some(RegisteredHost {
                                label,
                                moonlight_address,
                                device_public_key,
                                client: RegisteredClient::Native(NativeClient::new(base_url)),
                            });
                        }
                        let Some(peer_key) = config.device_public_key else {
                            configuration_error =
                                Some(format!("native upstream {label:?} has no devicePublicKey"));
                            return None;
                        };
                        if !valid_public_key(&peer_key) {
                            configuration_error = Some(format!(
                                "native upstream {label:?} has an invalid devicePublicKey"
                            ));
                            return None;
                        }
                        if moonlight_address
                            .as_deref()
                            .map(str::trim)
                            .is_none_or(str::is_empty)
                        {
                            configuration_error =
                                Some(format!("native upstream {label:?} has no moonlightAddress"));
                            return None;
                        }
                        let Some(credentials) = credentials.as_ref() else {
                            configuration_error =
                                Some(format!("native upstream {label:?} has no peer credentials"));
                            return None;
                        };
                        RegisteredClient::Native(NativeClient::new_secure(
                            base_url,
                            peer_key,
                            credentials.clone(),
                        ))
                    }
                };
                Some(RegisteredHost {
                    label,
                    moonlight_address,
                    device_public_key,
                    client,
                })
            })
            .collect();
        Self {
            last_stream_play_end: Arc::new(Mutex::new(None)),
            hosts,
            credentials,
            directory: None,
            configuration_error,
            deferred_file_config: None,
            selected_remote_session: Arc::new(Mutex::new(None)),
            remote_prepare_mutation: Arc::new(tokio::sync::Mutex::new(())),
            viewer: default_viewer(),
            live_shape: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_secure(configs: Vec<UpstreamHostConfig>, credentials: PeerCredentials) -> Self {
        Self::build(configs, Some(credentials))
    }

    pub(crate) fn local_viewer(&self) -> Arc<dyn LinuxStreamViewer> {
        Arc::new(live_shape::ShapeControls(self.clone()))
    }

    pub fn with_linux_viewer(mut self, viewer: Arc<dyn LinuxStreamViewer>) -> Self {
        self.viewer = viewer;
        self
    }

    #[cfg(test)]
    pub fn new(configs: Vec<UpstreamHostConfig>) -> Self {
        Self::build(configs, None)
    }

    #[cfg(test)]
    pub fn from_env_or_file_for_tests(path: &Path) -> Self {
        if let Ok(json) = std::env::var("KORRID_UPSTREAMS") {
            return Self::from_json("KORRID_UPSTREAMS", &json);
        }
        Self::from_file_or_empty(path, None)
    }

    pub fn from_env_or_file(path: &Path, credentials: PeerCredentials) -> Self {
        if let Ok(json) = std::env::var("KORRID_UPSTREAMS") {
            return Self::from_json_secure("KORRID_UPSTREAMS", &json, credentials);
        }
        Self::from_file_or_empty(path, Some(credentials))
    }

    /// No configured file means no upstream peers. There is no default host.
    fn from_file_or_empty(path: &Path, credentials: Option<PeerCredentials>) -> Self {
        match Self::from_file_inner(path, credentials.clone()) {
            Ok(registry) => registry,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self {
                last_stream_play_end: Arc::new(Mutex::new(None)),
                hosts: vec![],
                credentials: credentials.clone(),
                directory: None,
                configuration_error: None,
                deferred_file_config: Some(DeferredFileConfig {
                    path: path.to_owned(),
                    credentials,
                    resolved: Arc::new(OnceLock::new()),
                }),
                selected_remote_session: Arc::new(Mutex::new(None)),
                remote_prepare_mutation: Arc::new(tokio::sync::Mutex::new(())),
                viewer: default_viewer(),
                live_shape: Arc::new(Mutex::new(None)),
            },
            Err(error) => Self::invalid_file_read(path, error),
        }
    }

    #[cfg(test)]
    fn from_file_or_default(path: &Path, fallback: UpstreamHostConfig) -> Self {
        Self::from_file_or_default_inner(path, fallback, None)
    }

    #[cfg(test)]
    fn from_file_or_default_inner(
        path: &Path,
        fallback: UpstreamHostConfig,
        credentials: Option<PeerCredentials>,
    ) -> Self {
        match Self::from_file_inner(path, credentials.clone()) {
            Ok(registry) => registry,
            Err(error) => {
                let deferred_file_config = DeferredFileConfig {
                    path: path.to_owned(),
                    credentials: credentials.clone(),
                    resolved: Arc::new(OnceLock::new()),
                };
                let mut registry = if error.kind() == std::io::ErrorKind::NotFound {
                    Self::build(vec![fallback], credentials)
                } else {
                    Self::invalid_file_read(path, error)
                };
                registry.deferred_file_config = Some(deferred_file_config);
                registry
            }
        }
    }

    fn from_file_inner(path: &Path, credentials: Option<PeerCredentials>) -> std::io::Result<Self> {
        fs::read_to_string(path)
            .map(|json| Self::from_json_inner(&path.display().to_string(), &json, credentials))
    }

    fn invalid_file_read(path: &Path, error: std::io::Error) -> Self {
        Self::invalid_configuration(format!(
            "could not read upstream config {}: {error}",
            path.display()
        ))
    }

    fn from_json_secure(source: &str, json: &str, credentials: PeerCredentials) -> Self {
        Self::from_json_inner(source, json, Some(credentials))
    }

    #[cfg(test)]
    fn from_json(source: &str, json: &str) -> Self {
        Self::from_json_inner(source, json, None)
    }

    fn from_json_inner(source: &str, json: &str, credentials: Option<PeerCredentials>) -> Self {
        match serde_json::from_str(json) {
            Ok(configs) => Self::from_configs(source, configs, credentials),
            Err(error) => {
                Self::invalid_configuration(format!("invalid upstream config {source}: {error}"))
            }
        }
    }

    fn from_configs(
        source: &str,
        configs: Vec<UpstreamHostConfig>,
        credentials: Option<PeerCredentials>,
    ) -> Self {
        let mut labels = BTreeSet::new();
        if let Some(duplicate) = configs
            .iter()
            .find(|config| !labels.insert(config.label.as_str()))
        {
            return Self::invalid_configuration(format!(
                "invalid upstream config {source}: duplicate upstream label {:?}",
                duplicate.label
            ));
        }
        let mut native_device_keys = BTreeSet::new();
        if let Some(duplicate) = configs.iter().find(|config| {
            matches!(config.kind, UpstreamKind::Native)
                && config
                    .device_public_key
                    .as_deref()
                    .is_some_and(|key| !native_device_keys.insert(key))
        }) {
            return Self::invalid_configuration(format!(
                "invalid upstream config {source}: duplicate native devicePublicKey for {:?}",
                duplicate.label
            ));
        }
        Self::build(configs, credentials)
    }

    fn invalid_configuration(message: String) -> Self {
        Self {
            last_stream_play_end: Arc::new(Mutex::new(None)),
            hosts: vec![],
            credentials: None,
            directory: None,
            configuration_error: Some(message),
            deferred_file_config: None,
            selected_remote_session: Arc::new(Mutex::new(None)),
            remote_prepare_mutation: Arc::new(tokio::sync::Mutex::new(())),
            viewer: default_viewer(),
            live_shape: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach the single directory opened with the same credentials and
    /// Authorization as the local runtime. No discovery task is started here.
    pub fn with_federation(mut self, directory: crate::federation::FederationDirectory) -> Self {
        self.directory = Some(directory);
        self
    }

    fn resolved(&self) -> Result<Cow<'_, Self>, UpstreamError> {
        let configured = self.resolved_static();
        if let Some(error) = &configured.configuration_error {
            return Err(UpstreamError::Wire(error.clone()));
        }
        let Some(directory) = &self.directory else {
            return Ok(configured);
        };
        let peers = directory
            .snapshot()
            .map_err(|_| UpstreamError::Wire("peer directory unavailable".into()))?;
        let mut registry = configured.into_owned();
        registry.directory = Some(directory.clone());
        registry.credentials = self.credentials.clone();
        registry.last_stream_play_end = self.last_stream_play_end.clone();
        registry.selected_remote_session = self.selected_remote_session.clone();
        registry.remote_prepare_mutation = self.remote_prepare_mutation.clone();
        registry.viewer = self.viewer.clone();
        registry.live_shape = self.live_shape.clone();
        let mut seen = BTreeSet::new();
        let mut hosts = Vec::new();
        for mut host in registry.hosts {
            if let Some(key) = host.native_route_key() {
                if !seen.insert(key.clone())
                    || directory
                        .peer_is_revoked(&key)
                        .map_err(|_| UpstreamError::SourcePeerNotFound)?
                {
                    continue;
                }
                let RegisteredClient::Native(client) = host.client else {
                    unreachable!()
                };
                let mut candidates = client.candidate_origins();
                if let Some(peer) = peers.iter().find(|p| p.device_public_key == key) {
                    candidates.extend(endpoint_candidates(peer));
                }
                host.client = RegisteredClient::Native(
                    client
                        .with_candidates(candidates)?
                        .with_directory(directory.clone()),
                );
            }
            hosts.push(host);
        }
        for peer in peers {
            if !seen.insert(peer.device_public_key.clone()) {
                continue;
            }
            let credentials = registry
                .credentials
                .clone()
                .ok_or_else(|| UpstreamError::Wire("peer credentials unavailable".into()))?;
            let metadata = peer
                .current_endpoint
                .as_ref()
                .or(peer.remembered_endpoint.as_ref());
            let label = metadata
                .and_then(|e| e.label.clone())
                .unwrap_or_else(|| peer.device_public_key.clone());
            let moonlight_address = metadata.and_then(|e| e.moonlight_address.clone());
            let client = NativeClient::new_secure_candidates(
                endpoint_candidates(&peer),
                peer.device_public_key.clone(),
                credentials,
            )
            .with_directory(directory.clone());
            hosts.push(RegisteredHost {
                label,
                moonlight_address,
                device_public_key: Some(peer.device_public_key),
                client: RegisteredClient::Native(client),
            });
        }
        // Reach failures name the already-resolved peer label. Do not replace
        // endpoint facts with the transient forward address in this snapshot.
        for host in &mut hosts {
            if let RegisteredClient::Native(client) = &mut host.client {
                *client = client.clone().with_peer_label(host.label.clone());
            }
        }
        registry.hosts = hosts;
        Ok(Cow::Owned(registry))
    }

    fn resolved_static(&self) -> Cow<'_, Self> {
        let Some(config) = &self.deferred_file_config else {
            return Cow::Borrowed(self);
        };
        if let Some(registry) = config.resolved.get() {
            return Cow::Borrowed(registry);
        }
        match Self::from_file_inner(&config.path, config.credentials.clone()) {
            Ok(mut registry) if registry.configuration_error.is_none() => {
                // The deferred file is static configuration only. Selection and
                // mutation ordering belong to the runtime, not a rebuilt registry.
                registry.last_stream_play_end = self.last_stream_play_end.clone();
                registry.selected_remote_session = self.selected_remote_session.clone();
                registry.remote_prepare_mutation = self.remote_prepare_mutation.clone();
                registry.viewer = self.viewer.clone();
                registry.live_shape = self.live_shape.clone();
                Cow::Borrowed(config.resolved.get_or_init(|| registry))
            }
            Ok(registry) => Cow::Owned(registry),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Cow::Borrowed(self),
            Err(error) => Cow::Owned(Self::invalid_file_read(&config.path, error)),
        }
    }

    /// Static metadata only: do not take another directory snapshot or probe peers.
    pub(crate) fn configured_peer_labels(&self) -> std::collections::BTreeMap<String, String> {
        let registry = self.resolved_static();
        if registry.configuration_error.is_some() {
            return Default::default();
        }
        registry
            .hosts
            .iter()
            .filter_map(|host| host.native_route_key().map(|key| (key, host.label.clone())))
            .collect()
    }

    pub fn moonlight_host_candidates(&self) -> Result<Vec<MoonlightHostCandidate>, UpstreamError> {
        let registry = self.resolved()?;
        if let Some(error) = &registry.configuration_error {
            return Err(UpstreamError::Wire(error.clone()));
        }
        let candidates = registry
            .hosts
            .iter()
            .filter_map(|host| match &host.client {
                // Reach address view: only a live selected lease overrides
                // the configured or endpoint address. Never retain it as fact.
                RegisteredClient::Native(_) => registry
                    .directory
                    .as_ref()
                    .and_then(|directory| {
                        host.device_public_key
                            .as_deref()
                            .and_then(|key| directory.stream_address(key))
                    })
                    .or_else(|| {
                        host.device_public_key
                            .as_deref()
                            .and_then(|key| {
                                crate::federation::reach::nearby_seam::moonlight_address(
                                    key,
                                    host.moonlight_address.as_deref(),
                                )
                            })
                            .or_else(|| host.moonlight_address.clone())
                    })
                    .filter(|address| !address.trim().is_empty())
                    .map(|address| MoonlightHostCandidate {
                        label: host.label.clone(),
                        address: address.trim().into(),
                    }),
                RegisteredClient::Legacy(_) => None,
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            Err(UpstreamError::MoonlightHostCandidatesUnavailable)
        } else {
            Ok(candidates)
        }
    }

    pub async fn catalog_snapshot(&self) -> Result<CatalogSnapshot, UpstreamError> {
        let registry = self.resolved()?;
        registry.catalog_snapshot_resolved().await
    }

    async fn catalog_snapshot_resolved(&self) -> Result<CatalogSnapshot, UpstreamError> {
        if let Some(error) = &self.configuration_error {
            return Err(UpstreamError::Wire(error.clone()));
        }
        let qualify_legacy_host = self.hosts.len() > 1;
        let results = join_all(self.hosts.iter().map(|host| async move {
            // NativeClient owns its aggregate deadline and records the completed
            // attempt. An outer equal deadline would cancel that state commit.
            if matches!(host.client, RegisteredClient::Native(_)) {
                return host.catalog(qualify_legacy_host).await;
            }
            timeout(CATALOG_HOST_TIMEOUT, host.catalog(qualify_legacy_host))
                .await
                .unwrap_or_else(|_| {
                    Err(UpstreamError::Unreachable(format!(
                        "host {:?} catalog timed out",
                        host.label
                    )))
                })
        }))
        .await;
        let mut games = vec![];
        let mut failures = vec![];
        let mut first_error = None;
        let mut successes = 0;
        for (host, result) in self.hosts.iter().zip(results) {
            // Another peer can hold fan-out open after this response completed.
            let result = match &host.client {
                RegisteredClient::Native(client) => client.ensure_not_revoked().and(result),
                RegisteredClient::Legacy(_) => result,
            };
            match result {
                Ok(mut host_games) => {
                    successes += 1;
                    games.append(&mut host_games);
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error.clone());
                    }
                    failures.push(CatalogHostFailure {
                        host: host.label.clone(),
                        code: error.code().into(),
                        message: error.to_string(),
                    });
                }
            }
        }
        if successes == 0 {
            if let Some(error) = first_error {
                return Err(error);
            }
        }
        Ok(CatalogSnapshot {
            games,
            failures: (!failures.is_empty()).then_some(failures),
        })
    }

    async fn moonlight_native_peer(&self, host_uuid: &str) -> Result<NativeClient, UpstreamError> {
        let registry = self.resolved()?;
        let native: Vec<NativeClient> = registry
            .hosts
            .iter()
            .filter_map(|host| match &host.client {
                RegisteredClient::Native(client) => Some(client.clone()),
                RegisteredClient::Legacy(_) => None,
            })
            .collect();
        // One slow or unreachable owner device must not spend the whole
        // broker budget: bound each attest by the native operation timeout.
        let results = join_all(native.iter().map(|client| async move {
            timeout(
                NATIVE_RPC_TIMEOUT,
                client.moonlight_certificate_attest(host_uuid),
            )
            .await
            .unwrap_or(Err(UpstreamError::MoonlightCertificatePeerUnavailable))
        }))
        .await;
        // Most owner devices run no Sunshine host, and any of them may be
        // offline. Their attest failures say they do not own this UUID; they
        // must not hide the one peer that does. Report a failure only when no
        // peer matched.
        let mut matched = Vec::new();
        let mut first_failure = None;
        for (client, result) in native.into_iter().zip(results) {
            client.ensure_not_revoked()?;
            match result {
                Ok(attested) if attested.matched => matched.push(client),
                Ok(_) => {}
                Err(error) => {
                    first_failure.get_or_insert(error);
                }
            }
        }
        match matched.len() {
            0 => Err(first_failure.unwrap_or(UpstreamError::MoonlightHostNotFound)),
            1 => Ok(matched.remove(0)),
            _ => Err(UpstreamError::MoonlightHostAmbiguous),
        }
    }

    pub async fn moonlight_certificate_attest(
        &self,
        host_uuid: &str,
    ) -> Result<crate::MoonlightCertificateAttested, UpstreamError> {
        self.moonlight_native_peer(host_uuid)
            .await
            .map(|_| crate::MoonlightCertificateAttested { matched: true })
    }

    pub async fn moonlight_certificate_provision(
        &self,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<MoonlightCertificateProvisioned, UpstreamError> {
        timeout(MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT, async {
            self.moonlight_native_peer(host_uuid)
                .await?
                .moonlight_certificate_provision(host_uuid, client_certificate)
                .await
        })
        .await
        .map_err(|_| UpstreamError::MoonlightCertificatePeerUnavailable)?
    }

    pub async fn moonlight_certificate_revoke(
        &self,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<MoonlightCertificateRevoked, UpstreamError> {
        self.moonlight_native_peer(host_uuid)
            .await?
            .moonlight_certificate_revoke(host_uuid, client_certificate)
            .await
    }

    pub async fn prepare_stream(
        &self,
        game_id: &str,
        host_label: Option<&str>,
    ) -> Result<crate::game_routes::SelectedGameLaunch, UpstreamError> {
        self.prepare_streamed_launch(game_id, host_label, StreamedLaunch::default())
            .await
    }

    /// Prepare a streamed game with a runner choice or one-launch values.
    pub async fn prepare_streamed_launch(
        &self,
        game_id: &str,
        host_label: Option<&str>,
        launch: StreamedLaunch,
    ) -> Result<crate::game_routes::SelectedGameLaunch, UpstreamError> {
        let registry = self.resolved()?;
        registry
            .prepare_stream_resolved(game_id, host_label, launch)
            .await
    }

    /// One call to the holder a label names, for the launch settings and
    /// runner choices of a streamed game (ADR 0066). The holder decides
    /// whether this device may make it.
    pub(crate) async fn holder_call(
        &self,
        host_label: &str,
        request: crate::RpcRequest,
    ) -> Result<crate::RpcResponse, UpstreamError> {
        let registry = self.resolved()?;
        match &registry.host_for(Some(host_label))?.client {
            RegisteredClient::Native(client) => client.call(request).await,
            RegisteredClient::Legacy(_) => Err(UpstreamError::Failure(format!(
                "{host_label} does not offer launch settings"
            ))),
        }
    }

    fn host_for(&self, host_label: Option<&str>) -> Result<&RegisteredHost, UpstreamError> {
        match host_label {
            Some(label) => {
                let mut matches = self.hosts.iter().filter(|host| host.label == label);
                let host = matches.next().ok_or_else(|| {
                    UpstreamError::Failure(format!("upstream host {label:?} is not configured"))
                })?;
                // Authenticated display labels are not unique device identities.
                // The existing label selector must not guess between peers.
                if matches.next().is_some() {
                    return Err(UpstreamError::Failure(
                        "upstream host label is ambiguous".into(),
                    ));
                }
                Ok(host)
            }
            None if self.hosts.len() == 1 => Ok(&self.hosts[0]),
            None if self.hosts.is_empty() => Err(UpstreamError::Failure(
                "no upstream hosts configured".into(),
            )),
            None => Err(UpstreamError::Failure(
                "host is required when multiple upstreams are configured".into(),
            )),
        }
    }

    async fn prepare_stream_resolved(
        &self,
        game_id: &str,
        host_label: Option<&str>,
        launch: StreamedLaunch,
    ) -> Result<crate::game_routes::SelectedGameLaunch, UpstreamError> {
        let host = self.host_for(host_label)?;
        let _mutation = self.remote_prepare_mutation.lock().await;
        let direct_legacy = self.is_legacy_only();
        if !direct_legacy {
            self.ensure_remote_prepare_available().await?;
        }
        let prepared = match &host.client {
            RegisteredClient::Legacy(client) => {
                if launch.runner_id.is_some() || launch.launch.is_some() {
                    return Err(UpstreamError::Failure(format!(
                        "{} takes no runner choice and no launch settings",
                        host.label
                    )));
                }
                let prepared = client.prepare_stream(game_id).await?;
                let prepared = SessionPrepared {
                    game_id: prepared.game_id,
                    launch_id: prepared.session_id,
                    display: None,
                };
                self.set_selected(SelectedRemoteSession {
                    display: None,
                    viewer_target: LinuxViewerStatus::Running,
                    participation: Participation::Starter,
                    route: SelectedRemoteRoute::Legacy {
                        label: host.label.clone(),
                    },
                    launch_id: prepared.launch_id.clone(),
                    viewer: None,
                });
                crate::game_routes::SelectedGameLaunch {
                    session: prepared,
                    warnings: Vec::new(),
                }
            }
            RegisteredClient::Native(client) => {
                let route_key = host.native_route_key().ok_or_else(|| {
                    UpstreamError::Wire("native peer has no stable device identity".into())
                })?;
                let native_address = host.viewer_address().await.ok();
                let viewer = crate::federation::reach::prepare_peer_viewer(
                    self.directory.as_ref(),
                    &route_key,
                    native_address.as_deref(),
                )
                .await?;
                let moonlight_address = viewer.moonlight_address.clone();
                let reach = viewer.reach.clone();
                let viewer_display = self.viewer.display();
                eprintln!(
                    "korrid: viewer display sent with prepare: {}",
                    viewer_display
                        .as_ref()
                        .map(|d| format!("{}x{}", d.width, d.height))
                        .unwrap_or_else(|| "none".into())
                );
                let prepared = match launch.runner_id {
                    // The owner-device launch with a chosen runner (issue #401).
                    Some(runner_id) => {
                        client
                            .selected_game_launch(game_id, &runner_id, launch.launch)
                            .await?
                    }
                    None => {
                        client
                            .prepare_stream_for_launch(game_id, viewer_display, launch.launch)
                            .await?
                    }
                };
                let selected = SelectedRemoteSession {
                    display: prepared.session.display.clone(),
                    viewer_target: LinuxViewerStatus::Running,
                    participation: Participation::Starter,
                    route: SelectedRemoteRoute::Native {
                        device_public_key: route_key,
                    },
                    launch_id: prepared.session.launch_id.clone(),
                    viewer: Some(viewer),
                };
                self.set_selected(selected.clone());
                if let Err(error) = self
                    .viewer
                    .start(
                        &prepared.session.launch_id,
                        &moonlight_address,
                        selected.display.clone(),
                    )
                    .await
                {
                    if let Some(reach) = &reach {
                        reach.release();
                    }
                    return self
                        .compensate_viewer_start_failure(client, &selected, error.to_string())
                        .await;
                }
                self.set_viewer_confirmed(&selected, true);
                prepared
            }
        };
        Ok(prepared)
    }

    async fn compensate_viewer_start_failure<T>(
        &self,
        client: &NativeClient,
        selected: &SelectedRemoteSession,
        start_error: String,
    ) -> Result<T, UpstreamError> {
        match client.session_stop(&selected.launch_id, false).await {
            Ok(UpstreamSessionStop::Stopped { .. } | UpstreamSessionStop::NothingToStop {}) => {
                self.clear_selected_if(selected);
                Err(viewer_error("ViewerStartFailed", start_error))
            }
            Ok(UpstreamSessionStop::StopPending { .. }) => Err(viewer_error(
                "ViewerStartCompensationUncertain",
                format!("{start_error}; holder stop is pending"),
            )),
            Ok(other) => Err(viewer_error(
                "ViewerStartCompensationUncertain",
                format!("{start_error}; holder stop returned {other:?}"),
            )),
            Err(error) => Err(viewer_error(
                "ViewerStartCompensationUncertain",
                format!("{start_error}; holder stop failed: {error}"),
            )),
        }
    }

    async fn recover_viewer(
        &self,
        selected: &SelectedRemoteSession,
        holder_phase: RecoveredHolderPhase,
    ) -> Result<(), UpstreamError> {
        let Some(viewer) = &selected.viewer else {
            return Ok(());
        };
        if selected.viewer_target == LinuxViewerStatus::Stopped {
            return Ok(());
        }
        let viewer_backgrounded = selected.viewer_target == LinuxViewerStatus::Backgrounded
            || holder_phase.viewer_backgrounded();
        let mut recovering = selected.clone();
        let result = match self.viewer.status(&selected.launch_id).await {
            Ok(LinuxViewerStatus::Running) if viewer_backgrounded => {
                self.viewer.background(&selected.launch_id).await
            }
            Ok(LinuxViewerStatus::Backgrounded) if !viewer_backgrounded => {
                self.viewer.resume_and_focus(&selected.launch_id).await
            }
            Ok(LinuxViewerStatus::Running | LinuxViewerStatus::Backgrounded) => Ok(()),
            Ok(LinuxViewerStatus::Stopped) => {
                // Prepare this peer again before the viewer starts.
                let mut address = viewer.moonlight_address.clone();
                if let SelectedRemoteRoute::Native { device_public_key } = &selected.route {
                    if let Some(host) = self.native_host_by_route_key(device_public_key) {
                        let viewer = crate::federation::reach::prepare_peer_viewer(
                            self.directory.as_ref(),
                            device_public_key,
                            host.moonlight_address.as_deref(),
                        )
                        .await?;
                        address = viewer.moonlight_address.clone();
                        recovering.viewer = Some(viewer);
                        self.set_selected(recovering.clone());
                    }
                }
                match self
                    .viewer
                    .start(&selected.launch_id, &address, selected.display.clone())
                    .await
                {
                    Ok(()) if viewer_backgrounded => {
                        self.viewer.background(&selected.launch_id).await
                    }
                    other => other,
                }
            }
            Err(error) => {
                return Err(viewer_error(
                    "NativeViewerRecoveryIncomplete",
                    error.to_string(),
                ))
            }
        };
        match result {
            Ok(()) => {
                self.set_viewer_confirmed(selected, true);
                Ok(())
            }
            Err(error) => {
                if let Some(reach) = recovering.viewer.as_ref().and_then(|v| v.reach.as_ref()) {
                    reach.release();
                }
                Err(viewer_error(
                    "NativeViewerRecoveryIncomplete",
                    error.to_string(),
                ))
            }
        }
    }

    async fn ensure_remote_prepare_available(&self) -> Result<(), UpstreamError> {
        if let Some(selected) = self.selected() {
            if !self.retire_selected_if_revoked(&selected)? {
                let status = match self.selected_status(&selected).await {
                    // A failed streamed launch is terminal too. Prepare retires it
                    // without turning its diagnostic into a new launch refusal.
                    Err(UpstreamError::Tagged { code, .. }) if code == "HostLaunchFailed" => {
                        UpstreamSessionStatus::SessionStatus { active: None }
                    }
                    other => other?,
                };
                match status {
                    UpstreamSessionStatus::SessionStatus {
                        active: Some(active),
                    } => {
                        return if active.launch_id == selected.launch_id {
                            Err(UpstreamError::ActiveRemoteSessionConflict)
                        } else {
                            if selected.viewer.is_none() {
                                self.clear_selected_if(&selected);
                            } else {
                                self.set_viewer_confirmed(&selected, false);
                            }
                            Err(UpstreamError::SelectedRemoteSessionReplaced)
                        }
                    }
                    UpstreamSessionStatus::SessionStatus { active: None } => {
                        self.retire_ended_selection(&selected).await?;
                    }
                    UpstreamSessionStatus::SessiondNotConfigured {} => {
                        return Err(UpstreamError::Failure(
                            "selected legacy session status is not configured".into(),
                        ))
                    }
                    UpstreamSessionStatus::HostUnavailable {} => {
                        return Err(UpstreamError::Unreachable(
                            "selected legacy session status is unavailable".into(),
                        ))
                    }
                }
            }
        }
        // ADR 0035: only an explicitly selected route belongs to this device.
        // The chosen holder admits its own launch; unrelated peers do not vote.
        match self.legacy_status_or_no_active().await? {
            UpstreamSessionStatus::SessionStatus { active: Some(_) } => {
                Err(UpstreamError::ActiveRemoteSessionConflict)
            }
            UpstreamSessionStatus::SessionStatus { active: None } => Ok(()),
            UpstreamSessionStatus::SessiondNotConfigured {} => Err(UpstreamError::Failure(
                "legacy session status is not configured".into(),
            )),
            UpstreamSessionStatus::HostUnavailable {} => Err(UpstreamError::Unreachable(
                "legacy session status is unavailable".into(),
            )),
        }
    }

    /// Terminal status from the holder is authoritative, but the exact local viewer must
    /// also be stopped before another launch can replace this selection.
    async fn retire_ended_selection(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Result<(), UpstreamError> {
        if selected.viewer.is_some() {
            let cleanup = async {
                if self.viewer.status(&selected.launch_id).await? != LinuxViewerStatus::Stopped {
                    self.viewer.stop(&selected.launch_id).await?;
                }
                Ok::<_, crate::linux_viewer::LinuxViewerError>(())
            }
            .await;
            if let Err(error) = cleanup {
                self.set_viewer_confirmed(selected, false);
                return Err(viewer_error(
                    "NativeViewerRecoveryIncomplete",
                    error.to_string(),
                ));
            }
        }
        self.clear_selected_if(selected);
        Ok(())
    }

    pub async fn session_status(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        let registry = self.resolved()?;
        registry.session_status_resolved().await
    }

    async fn session_status_resolved(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        let _mutation = self.remote_prepare_mutation.lock().await;
        if let Some(selected) = self.selected() {
            if matches!(selected.participation, Participation::StreamPlayer { .. }) {
                let status = self.observe_stream_player(&selected).await?;
                if matches!(
                    &status,
                    UpstreamSessionStatus::SessionStatus { active: Some(_) }
                ) {
                    let _ = self.synchronize_shape(&selected).await;
                }
                return Ok(status);
            }
            let status = match self.selected_status(&selected).await {
                Err(error) if error.code() == "HostLaunchFailed" => {
                    // Return this diagnostic only to the observation that
                    // retires the selection. Later polls stay silent.
                    self.retire_ended_selection(&selected).await?;
                    return Err(error);
                }
                other => other?,
            };
            match &status {
                UpstreamSessionStatus::SessionStatus {
                    active: Some(active),
                } => {
                    if active.launch_id != selected.launch_id {
                        self.set_viewer_confirmed(&selected, false);
                        return Err(UpstreamError::SelectedRemoteSessionReplaced);
                    }
                    if selected.viewer.is_some() {
                        let phase = RecoveredHolderPhase::parse(active.phase.as_deref())?;
                        self.recover_viewer(&selected, phase).await?;
                        let _ = self.synchronize_shape(&selected).await;
                    }
                    return Ok(status);
                }
                UpstreamSessionStatus::SessionStatus { active: None } => {
                    self.retire_ended_selection(&selected).await?;
                }
                UpstreamSessionStatus::SessiondNotConfigured {}
                | UpstreamSessionStatus::HostUnavailable {} => return Ok(status),
            }
        }

        self.legacy_status_or_no_active().await
    }

    pub async fn session_stop(
        &self,
        expected_launch_id: Option<&str>,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        let registry = self.resolved()?;
        registry
            .session_stop_resolved(expected_launch_id, force)
            .await
    }

    async fn session_stop_resolved(
        &self,
        expected_launch_id: Option<&str>,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        let _mutation = self.remote_prepare_mutation.lock().await;
        if let Some(selected) = self.selected() {
            let exact_required = matches!(selected.route, SelectedRemoteRoute::Native { .. })
                || !self.is_legacy_only();
            if exact_required {
                let expected = expected_launch_id
                    .filter(|expected| !expected.is_empty())
                    .ok_or(UpstreamError::ExpectedLaunchIdRequired)?;
                if expected != selected.launch_id {
                    return Err(UpstreamError::StaleLaunchIdentity);
                }
            } else if expected_launch_id.is_some_and(|expected| expected != selected.launch_id) {
                return Err(UpstreamError::StaleLaunchIdentity);
            }
            return self.stop_selected(&selected, force).await;
        }

        self.legacy_stop_or_nothing(force).await
    }

    /// Reports one native peer's readiness. The peer is selected by its
    /// configured expected device public key. A key that names no native
    /// peer, or that names a legacy peer, fails closed without any network
    /// call. A transport failure is returned as an error and is never
    /// converted into a false readiness answer.
    pub async fn source_status(
        &self,
        device_public_key: &str,
    ) -> Result<SourceStatus, UpstreamError> {
        let registry = self.resolved()?;
        registry.source_status_resolved(device_public_key).await
    }

    async fn source_status_resolved(
        &self,
        device_public_key: &str,
    ) -> Result<SourceStatus, UpstreamError> {
        if let Some(error) = &self.configuration_error {
            return Err(UpstreamError::Wire(error.clone()));
        }
        let host = self
            .native_host_by_route_key(device_public_key)
            .ok_or(UpstreamError::SourcePeerNotFound)?;
        let RegisteredClient::Native(client) = &host.client else {
            unreachable!("native_host_by_route_key returned a legacy client")
        };
        client.source_status(device_public_key).await
    }

    pub async fn session_controls(&self, launch_id: &str) -> SessionControlsOutcome {
        let result = async {
            let registry = self.resolved().map_err(gameplay_control_failure)?;
            let _mutation = registry.remote_prepare_mutation.lock().await;
            registry
                .selected_gameplay_client(launch_id)?
                .session_controls(launch_id)
                .await
                .map_err(gameplay_control_failure)
        }
        .await;
        result.unwrap_or_else(SessionControlsOutcome::Err)
    }

    pub async fn invoke_session_control(
        &self,
        request: SessionControlInvokeRequest,
    ) -> SessionControlInvokeOutcome {
        let result = async {
            let registry = self.resolved().map_err(gameplay_control_failure)?;
            let _mutation = registry.remote_prepare_mutation.lock().await;
            registry
                .selected_gameplay_client(&request.launch_id)?
                .invoke_session_control(request)
                .await
                .map_err(gameplay_control_failure)
        }
        .await;
        result.unwrap_or_else(SessionControlInvokeOutcome::Err)
    }

    /// Route only this device's exact selection. The holder validates live
    /// identity, starter authority, declarations and values at execution time.
    fn selected_gameplay_client(
        &self,
        launch_id: &str,
    ) -> Result<&NativeClient, SessionControlFailure> {
        let selected = self
            .selected()
            .ok_or_else(|| gameplay_control_failure(UpstreamError::NoActiveSession))?;
        if launch_id.is_empty() || selected.launch_id != launch_id {
            return Err(gameplay_control_failure(UpstreamError::StaleLaunchIdentity));
        }
        if matches!(selected.participation, Participation::StreamPlayer { .. }) {
            return Err(SessionControlFailure {
                reason: SessionControlFailureReason::NotSessionStarter,
                message: "A stream player cannot use the holding device's gameplay controls."
                    .into(),
            });
        }
        if self
            .retire_selected_if_revoked(&selected)
            .map_err(gameplay_control_failure)?
        {
            return Err(gameplay_control_failure(UpstreamError::SourcePeerNotFound));
        }
        let SelectedRemoteRoute::Native { device_public_key } = &selected.route else {
            return Err(SessionControlFailure {
                reason: SessionControlFailureReason::Unavailable,
                message: "The selected peer does not expose live gameplay controls.".into(),
            });
        };
        let host = self
            .native_host_by_route_key(device_public_key)
            .ok_or_else(|| gameplay_control_failure(UpstreamError::SourcePeerNotFound))?;
        let RegisteredClient::Native(client) = &host.client else {
            unreachable!("native_host_by_route_key returned a legacy client")
        };
        Ok(client)
    }

    /// Android Leave holds input in its Surface host, not the holder's cgroup freezer.
    /// Validate the exact selected launch using the real peer before publishing Home.
    pub async fn session_leave_live(
        &self,
        expected: Option<&str>,
    ) -> Result<SessionTransitionResult, UpstreamError> {
        let registry = self.resolved()?;
        let _mutation = registry.remote_prepare_mutation.lock().await;
        let selected = registry.exact_live_selection(expected).await?;
        Ok(SessionTransitionResult {
            launch_id: selected.launch_id,
            state: crate::SessionFreezerState::Running,
            changed: false,
        })
    }

    pub async fn session_resume_live(
        &self,
        expected: Option<&str>,
    ) -> Result<SessionTransitionResult, UpstreamError> {
        let registry = self.resolved()?;
        let _mutation = registry.remote_prepare_mutation.lock().await;
        let selected = registry.exact_live_selection(expected).await?;
        if selected.viewer_target == LinuxViewerStatus::Stopped {
            // Disconnect retains the exact holder identity, not a decoder. Renew
            // its route and restart only this viewer when the person resumes.
            let mut resumed = selected.clone();
            resumed.viewer_target = LinuxViewerStatus::Running;
            registry
                .recover_viewer(&resumed, RecoveredHolderPhase::Running)
                .await?;
        } else {
            registry
                .viewer
                .resume_and_focus(&selected.launch_id)
                .await
                .map_err(|error| viewer_error("ViewerControlFailed", error.to_string()))?;
        }
        registry.set_viewer_target(&selected, LinuxViewerStatus::Running);
        Ok(SessionTransitionResult {
            launch_id: selected.launch_id,
            state: crate::SessionFreezerState::Running,
            changed: false,
        })
    }

    pub async fn session_viewer(
        &self,
        expected: &str,
        action: crate::SessionViewerAction,
    ) -> Result<(), UpstreamError> {
        let registry = self.resolved()?;
        let _mutation = registry.remote_prepare_mutation.lock().await;
        let selected = registry.exact_live_selection(Some(expected)).await?;
        match action {
            crate::SessionViewerAction::OpenKorri => registry.viewer.background(expected).await,
            crate::SessionViewerAction::Disconnect => registry.viewer.stop(expected).await,
        }
        .map_err(|error| viewer_error("ViewerControlFailed", error.to_string()))?;
        match action {
            crate::SessionViewerAction::OpenKorri => {
                registry.set_viewer_target(&selected, LinuxViewerStatus::Backgrounded)
            }
            crate::SessionViewerAction::Disconnect
                if matches!(selected.participation, Participation::StreamPlayer { .. }) =>
            {
                registry.stop_stream_player(&selected).await?;
            }
            crate::SessionViewerAction::Disconnect => {
                if let Some(reach) = selected
                    .viewer
                    .as_ref()
                    .and_then(|viewer| viewer.reach.as_ref())
                {
                    reach.release();
                }
                registry.set_viewer_target(&selected, LinuxViewerStatus::Stopped);
                registry.set_viewer_confirmed(&selected, false);
            }
        }
        Ok(())
    }

    async fn exact_live_selection(
        &self,
        expected: Option<&str>,
    ) -> Result<SelectedRemoteSession, UpstreamError> {
        let expected = expected
            .filter(|id| !id.is_empty())
            .ok_or(UpstreamError::ExpectedLaunchIdRequired)?;
        let selected = self.selected().ok_or(UpstreamError::NoActiveSession)?;
        if selected.launch_id != expected {
            return Err(UpstreamError::StaleLaunchIdentity);
        }
        match self.selected_status(&selected).await? {
            UpstreamSessionStatus::SessionStatus {
                active: Some(active),
            } if active.launch_id == expected && active.phase.as_deref() == Some("running") => {
                Ok(selected)
            }
            UpstreamSessionStatus::SessionStatus { active: None } => {
                Err(UpstreamError::NoActiveSession)
            }
            UpstreamSessionStatus::SessionStatus {
                active: Some(active),
            } if active.launch_id != expected => Err(UpstreamError::SelectedRemoteSessionReplaced),
            _ => Err(viewer_error(
                "SessionNotRunning",
                "The selected game is not running",
            )),
        }
    }

    pub async fn session_freeze(
        &self,
        expected_launch_id: Option<&str>,
    ) -> Result<SessionTransitionResult, UpstreamError> {
        let registry = self.resolved()?;
        registry
            .session_freezer_resolved(expected_launch_id, FreezerVerb::Freeze)
            .await
    }

    pub async fn session_thaw(
        &self,
        expected_launch_id: Option<&str>,
    ) -> Result<SessionTransitionResult, UpstreamError> {
        let registry = self.resolved()?;
        registry
            .session_freezer_resolved(expected_launch_id, FreezerVerb::Thaw)
            .await
    }

    /// Routes an exact freeze or thaw through the selected remote session.
    /// Only the explicitly selected route can be controlled. Only native
    /// peers support freezer control; a legacy route fails with a typed
    /// error and no legacy call is made.
    async fn session_freezer_resolved(
        &self,
        expected_launch_id: Option<&str>,
        verb: FreezerVerb,
    ) -> Result<SessionTransitionResult, UpstreamError> {
        let _mutation = self.remote_prepare_mutation.lock().await;
        let selected = match self.selected() {
            Some(selected) => {
                if !matches!(selected.route, SelectedRemoteRoute::Native { .. }) {
                    return Err(UpstreamError::FreezerUnsupportedOnLegacyRoute);
                }
                selected
            }
            None => {
                if self.hosts.is_empty() {
                    return Err(UpstreamError::NoActiveSession);
                }
                if !self.has_native_hosts() {
                    return Err(UpstreamError::FreezerUnsupportedOnLegacyRoute);
                }
                return Err(UpstreamError::NoActiveSession);
            }
        };
        let expected = expected_launch_id
            .filter(|expected| !expected.is_empty())
            .ok_or(UpstreamError::ExpectedLaunchIdRequired)?;
        if expected != selected.launch_id {
            return Err(UpstreamError::StaleLaunchIdentity);
        }
        if matches!(selected.participation, Participation::StreamPlayer { .. }) {
            match verb {
                FreezerVerb::Freeze => {
                    self.stop_stream_player(&selected).await?;
                }
                FreezerVerb::Thaw => {
                    return Err(UpstreamError::Tagged {
                        code: "NotSessionStarter".into(),
                        message: "A stream player cannot thaw the holding device's session.".into(),
                    })
                }
            }
            return Ok(SessionTransitionResult {
                launch_id: selected.launch_id,
                state: crate::SessionFreezerState::Frozen,
                changed: true,
            });
        }
        let SelectedRemoteRoute::Native { device_public_key } = &selected.route else {
            unreachable!("legacy routes returned before exact freezer control")
        };
        if self.retire_selected_if_revoked(&selected)? {
            return Err(UpstreamError::SourcePeerNotFound);
        }
        let host = self
            .native_host_by_route_key(device_public_key)
            .ok_or_else(|| UpstreamError::Wire("selected native peer disappeared".into()))?;
        let RegisteredClient::Native(client) = &host.client else {
            unreachable!("native_host_by_route_key returned a legacy client")
        };
        let call = async {
            match verb {
                FreezerVerb::Freeze => client.session_freeze(&selected.launch_id).await,
                FreezerVerb::Thaw => client.session_thaw(&selected.launch_id).await,
            }
        };
        let result = call.await;
        match result {
            Ok(result) => {
                let viewer_phase = match verb {
                    FreezerVerb::Freeze => RecoveredHolderPhase::Frozen,
                    FreezerVerb::Thaw => RecoveredHolderPhase::Running,
                };
                match self.recover_viewer(&selected, viewer_phase).await {
                    Ok(()) => Ok(result),
                    Err(error) => {
                        self.set_viewer_confirmed(&selected, false);
                        let compensation = match verb {
                            FreezerVerb::Freeze => client.session_thaw(&selected.launch_id).await,
                            FreezerVerb::Thaw => client.session_freeze(&selected.launch_id).await,
                        };
                        match compensation {
                            Ok(_) => Err(viewer_error(
                                "ViewerControlFailed",
                                format!(
                                    "local viewer {} failed for the exact launch and the holder change was compensated: {error}",
                                    match verb {
                                        FreezerVerb::Freeze => "background",
                                        FreezerVerb::Thaw => "resume and focus",
                                    }
                                ),
                            )),
                            Err(compensation_error) => Err(viewer_error(
                                "ViewerControlCompensationUncertain",
                                format!(
                                    "local viewer {} failed for the exact launch: {error}; holder compensation failed: {compensation_error}",
                                    match verb {
                                        FreezerVerb::Freeze => "background",
                                        FreezerVerb::Thaw => "resume and focus",
                                    }
                                ),
                            )),
                        }
                    }
                }
            }
            Err(UpstreamError::Tagged { code, message })
                if matches!(
                    code.as_str(),
                    "SessionCompleted" | "NoActiveSession" | "HostLaunchFailed"
                ) =>
            {
                self.retire_ended_selection(&selected).await?;
                if code == "HostLaunchFailed" {
                    Err(UpstreamError::Tagged { code, message })
                } else {
                    Err(UpstreamError::NoActiveSession)
                }
            }
            Err(UpstreamError::Tagged { code, .. })
                if matches!(
                    code.as_str(),
                    "StaleLaunchIdentity" | "SelectedRemoteSessionReplaced"
                ) =>
            {
                self.set_viewer_confirmed(&selected, false);
                Err(UpstreamError::SelectedRemoteSessionReplaced)
            }
            Err(UpstreamError::SelectedRemoteSessionReplaced) => {
                self.set_viewer_confirmed(&selected, false);
                Err(UpstreamError::SelectedRemoteSessionReplaced)
            }
            Err(error) => Err(error),
        }
    }

    async fn selected_status(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Result<UpstreamSessionStatus, UpstreamError> {
        if self.retire_selected_if_revoked(selected)? {
            return Err(UpstreamError::SourcePeerNotFound);
        }
        let result = match &selected.route {
            SelectedRemoteRoute::Native { device_public_key } => {
                let host = self
                    .native_host_by_route_key(device_public_key)
                    .ok_or_else(|| {
                        UpstreamError::Wire("selected native peer disappeared".into())
                    })?;
                host.native_session_status().await
            }
            SelectedRemoteRoute::Legacy { label } => {
                let host = self.legacy_host_by_label(label).ok_or_else(|| {
                    UpstreamError::Wire("selected legacy peer disappeared".into())
                })?;
                let RegisteredClient::Legacy(client) = &host.client else {
                    unreachable!("legacy_host_by_label returned a native client")
                };
                timeout(CATALOG_HOST_TIMEOUT, client.session_status())
                    .await
                    .map_err(|_| {
                        UpstreamError::Unreachable(format!(
                            "host {:?} session status timed out",
                            host.label
                        ))
                    })?
                    .map(|status| {
                        if self.hosts.len() > 1 {
                            host.qualify_status(status)
                        } else {
                            status
                        }
                    })
            }
        };
        match result {
            Err(UpstreamError::Tagged { code, .. })
                if matches!(
                    code.as_str(),
                    "StaleLaunchIdentity" | "SelectedRemoteSessionReplaced"
                ) =>
            {
                if selected.viewer.is_none() {
                    self.clear_selected_if(selected);
                } else {
                    self.set_viewer_confirmed(selected, false);
                }
                Err(UpstreamError::SelectedRemoteSessionReplaced)
            }
            Err(UpstreamError::SelectedRemoteSessionReplaced) => {
                if selected.viewer.is_none() {
                    self.clear_selected_if(selected);
                } else {
                    self.set_viewer_confirmed(selected, false);
                }
                Err(UpstreamError::SelectedRemoteSessionReplaced)
            }
            other => other,
        }
    }

    async fn stop_selected(
        &self,
        selected: &SelectedRemoteSession,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        if matches!(selected.participation, Participation::StreamPlayer { .. }) {
            return self.stop_stream_player(selected).await;
        }
        if self.retire_selected_if_revoked(selected)? {
            return Err(UpstreamError::SourcePeerNotFound);
        }
        let result = match &selected.route {
            SelectedRemoteRoute::Native { device_public_key } => {
                let host = self
                    .native_host_by_route_key(device_public_key)
                    .ok_or_else(|| {
                        UpstreamError::Wire("selected native peer disappeared".into())
                    })?;
                let RegisteredClient::Native(client) = &host.client else {
                    unreachable!("native_host_by_route_key returned a legacy client")
                };
                let status = host.native_session_status().await?;
                let holder_outcome = match active_session(&status) {
                    Some(active) if active.launch_id == selected.launch_id => {
                        match client.session_stop(&selected.launch_id, force).await {
                            Ok(outcome) => outcome,
                            Err(UpstreamError::Tagged { code, .. })
                                if matches!(
                                    code.as_str(),
                                    "StaleLaunchIdentity" | "SelectedRemoteSessionReplaced"
                                ) =>
                            {
                                self.set_viewer_confirmed(selected, false);
                                return Err(UpstreamError::SelectedRemoteSessionReplaced);
                            }
                            Err(UpstreamError::SelectedRemoteSessionReplaced) => {
                                self.set_viewer_confirmed(selected, false);
                                return Err(UpstreamError::SelectedRemoteSessionReplaced);
                            }
                            Err(error) => return Err(error),
                        }
                    }
                    Some(_) => {
                        self.set_viewer_confirmed(selected, false);
                        return Err(UpstreamError::SelectedRemoteSessionReplaced);
                    }
                    None => UpstreamSessionStop::NothingToStop {},
                };
                if matches!(
                    holder_outcome,
                    UpstreamSessionStop::Stopped { .. } | UpstreamSessionStop::NothingToStop {}
                ) {
                    if let Err(error) = self.viewer.stop(&selected.launch_id).await {
                        self.set_viewer_confirmed(selected, false);
                        return Err(viewer_error(
                            "ViewerControlFailed",
                            format!("local viewer stop failed for the exact launch: {error}"),
                        ));
                    }
                    self.clear_selected_if(selected);
                }
                return Ok(holder_outcome);
            }
            SelectedRemoteRoute::Legacy { label } => {
                let host = self.legacy_host_by_label(label).ok_or_else(|| {
                    UpstreamError::Wire("selected legacy peer disappeared".into())
                })?;
                let RegisteredClient::Legacy(client) = &host.client else {
                    unreachable!("legacy_host_by_label returned a native client")
                };
                if !self.is_legacy_only() {
                    match self.selected_status(selected).await? {
                        UpstreamSessionStatus::SessionStatus {
                            active: Some(active),
                        } if active.launch_id == selected.launch_id => {}
                        UpstreamSessionStatus::SessionStatus { active: Some(_) } => {
                            self.clear_selected_if(selected);
                            return Err(UpstreamError::SelectedRemoteSessionReplaced);
                        }
                        UpstreamSessionStatus::SessionStatus { active: None } => {
                            self.clear_selected_if(selected);
                            return Ok(UpstreamSessionStop::NothingToStop {});
                        }
                        UpstreamSessionStatus::SessiondNotConfigured {} => {
                            return Err(UpstreamError::Failure(
                                "selected legacy session status is not configured".into(),
                            ))
                        }
                        UpstreamSessionStatus::HostUnavailable {} => {
                            return Err(UpstreamError::Unreachable(
                                "selected legacy session status is unavailable".into(),
                            ))
                        }
                    }
                }
                client.session_stop(force).await?
            }
        };
        if matches!(
            result,
            UpstreamSessionStop::Stopped { .. } | UpstreamSessionStop::NothingToStop {}
        ) {
            self.clear_selected_if(selected);
        }
        Ok(result)
    }

    async fn legacy_status_or_no_active(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        let Some(host) = self.legacy_host() else {
            return Ok(UpstreamSessionStatus::SessionStatus { active: None });
        };
        let RegisteredClient::Legacy(client) = &host.client else {
            unreachable!("legacy_host returned a native client")
        };
        client.session_status().await.map(|status| {
            if self.hosts.len() > 1 {
                host.qualify_status(status)
            } else {
                status
            }
        })
    }

    async fn legacy_stop_or_nothing(
        &self,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        let Some(host) = self.legacy_host() else {
            return Ok(UpstreamSessionStop::NothingToStop {});
        };
        let RegisteredClient::Legacy(client) = &host.client else {
            unreachable!("legacy_host returned a native client")
        };
        client.session_stop(force).await
    }

    fn retire_selected_if_revoked(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Result<bool, UpstreamError> {
        if let (Some(directory), SelectedRemoteRoute::Native { device_public_key }) =
            (&self.directory, &selected.route)
        {
            if directory
                .peer_is_revoked(device_public_key)
                .map_err(|_| UpstreamError::SourcePeerNotFound)?
            {
                // Revocation is not evidence that the old holder execution or
                // its exact local viewer stopped. Keep a viewer-backed
                // selection tracked until stop can confirm both sides ended.
                if selected.viewer.is_none() {
                    self.clear_selected_if(selected);
                } else {
                    self.set_viewer_confirmed(selected, false);
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn selected(&self) -> Option<SelectedRemoteSession> {
        self.selected_remote_session
            .lock()
            .expect("selected remote session mutex poisoned")
            .clone()
    }

    fn set_selected(&self, selected: SelectedRemoteSession) {
        self.last_stream_play_end
            .lock()
            .expect("stream play end mutex poisoned")
            .take();
        let mut current = self
            .selected_remote_session
            .lock()
            .expect("selected remote session mutex poisoned");
        if let Some(old) = current
            .as_ref()
            .and_then(|s| s.viewer.as_ref())
            .and_then(|v| v.reach.as_ref())
        {
            if selected.viewer.as_ref().and_then(|v| v.reach.as_ref()) != Some(old) {
                old.release();
            }
        }
        *current = Some(selected);
    }

    fn set_viewer_target(&self, expected: &SelectedRemoteSession, target: LinuxViewerStatus) {
        let mut selected = self
            .selected_remote_session
            .lock()
            .expect("selected remote session mutex poisoned");
        if let Some(current) = selected.as_mut().filter(|current| {
            current.route == expected.route && current.launch_id == expected.launch_id
        }) {
            current.viewer_target = target;
        }
    }

    fn set_viewer_confirmed(&self, expected: &SelectedRemoteSession, confirmed_live: bool) {
        let mut selected = self
            .selected_remote_session
            .lock()
            .expect("selected remote session mutex poisoned");
        if let Some(current) = selected.as_mut().filter(|current| {
            current.route == expected.route && current.launch_id == expected.launch_id
        }) {
            if let Some(viewer) = current.viewer.as_mut() {
                viewer.confirmed_live = confirmed_live;
            }
        }
    }

    fn has_native_hosts(&self) -> bool {
        self.hosts
            .iter()
            .any(|host| matches!(&host.client, RegisteredClient::Native(_)))
    }

    fn is_legacy_only(&self) -> bool {
        self.hosts.len() == 1 && matches!(self.hosts[0].client, RegisteredClient::Legacy(_))
    }

    fn native_host_by_route_key(&self, route_key: &str) -> Option<&RegisteredHost> {
        self.hosts.iter().find(|host| {
            host.native_route_key()
                .as_deref()
                .is_some_and(|candidate| candidate == route_key)
        })
    }

    fn legacy_host_by_label(&self, label: &str) -> Option<&RegisteredHost> {
        self.hosts
            .iter()
            .find(|host| host.label == label && matches!(host.client, RegisteredClient::Legacy(_)))
    }

    fn clear_selected_if(&self, expected: &SelectedRemoteSession) {
        let mut selected = self
            .selected_remote_session
            .lock()
            .expect("selected remote session mutex poisoned");
        if selected.as_ref() == Some(expected) {
            // Reach release is idempotent and also runs on the lease's Drop.
            if let Some(reach) = selected
                .as_ref()
                .and_then(|s| s.viewer.as_ref())
                .and_then(|v| v.reach.as_ref())
            {
                reach.release();
            }
            *selected = None;
        }
    }

    fn legacy_host(&self) -> Option<&RegisteredHost> {
        self.hosts
            .iter()
            .find(|host| matches!(&host.client, RegisteredClient::Legacy(_)))
    }
}

fn viewer_error(code: &'static str, message: impl Into<String>) -> UpstreamError {
    UpstreamError::Tagged {
        code: code.into(),
        message: message.into(),
    }
}

fn moonlight_control_origins(candidates: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    candidates
        .iter()
        .filter_map(|candidate| {
            let origin = crate::federation::peer_origin(candidate).ok()?;
            let mut url = url::Url::parse(&origin).ok()?;
            url.set_scheme("http").ok()?;
            url.set_port(Some(47989)).ok()?;
            let origin = url.origin().ascii_serialization();
            seen.insert(origin.clone()).then_some(origin)
        })
        .collect()
}

fn endpoint_candidates(peer: &crate::federation::PeerSnapshot) -> Vec<String> {
    peer.current_endpoint
        .iter()
        .chain(peer.remembered_endpoint.iter())
        .flat_map(|endpoint| endpoint.candidates.iter().cloned())
        .collect()
}

fn active_session(
    status: &UpstreamSessionStatus,
) -> Option<&crate::upstream::UpstreamActiveSession> {
    match status {
        UpstreamSessionStatus::SessionStatus { active } => active.as_ref(),
        UpstreamSessionStatus::SessiondNotConfigured {}
        | UpstreamSessionStatus::HostUnavailable {} => None,
    }
}

fn valid_public_key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn gameplay_control_failure(error: UpstreamError) -> SessionControlFailure {
    let reason = match error.code() {
        "StaleLaunchIdentity"
        | "SelectedRemoteSessionReplaced"
        | "NoActiveSession"
        | "SessionCompleted"
        | "HostLaunchFailed" => SessionControlFailureReason::StaleSession,
        "NotSessionStarter" => SessionControlFailureReason::NotSessionStarter,
        _ => SessionControlFailureReason::Unavailable,
    };
    SessionControlFailure {
        reason,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    include!("stream_play/registry_tests.rs");
    use super::*;
    use crate::linux_viewer::test_backend::{
        LinuxViewerAction, LinuxViewerCall, TestLinuxStreamViewer,
    };
    use axum::{extract::State, routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::sync::Arc;

    /// A known process-local selection whose viewer needs observation or recovery.
    /// This does not ask peers to infer which launch belongs to this device.
    async fn selected_viewer_registry(
        registry: UpstreamRegistry,
        launch_id: &str,
    ) -> UpstreamRegistry {
        let host = registry
            .hosts
            .iter()
            .find(|host| matches!(host.client, RegisteredClient::Native(_)))
            .unwrap();
        registry.set_selected(SelectedRemoteSession {
            display: None,
            viewer_target: LinuxViewerStatus::Running,
            participation: Participation::Starter,
            route: SelectedRemoteRoute::Native {
                device_public_key: host.native_route_key().unwrap(),
            },
            launch_id: launch_id.into(),
            viewer: Some(SelectedViewer {
                moonlight_address: host.viewer_address().await.unwrap(),
                confirmed_live: false,
                reach: None,
            }),
        });
        registry
    }

    #[derive(Clone)]
    struct LegacyServerState {
        prepared: Arc<std::sync::Mutex<Vec<String>>>,
    }

    async fn legacy_rpc(
        State(state): State<LegacyServerState>,
        Json(request): Json<Value>,
    ) -> Json<Value> {
        let value = match request["tag"].as_str().unwrap() {
            "app.catalog.snapshot" => json!({
                "entries": [{"id":"shared","title":"Legacy game","launchable":true}]
            }),
            "app.server.stream.prepare" => {
                let id = request["payload"]["id"].as_str().unwrap().to_owned();
                state.prepared.lock().unwrap().push(id.clone());
                json!({"gameId":id,"sessionId":"legacy-session"})
            }
            "app.session.status" => {
                let prepared = state.prepared.lock().unwrap();
                let active_game = prepared.iter().fold(None, |_active, event| {
                    if event.starts_with("stop:") {
                        None
                    } else {
                        Some(event.as_str())
                    }
                });
                json!({
                    "_tag":"SessionStatus",
                    "active":active_game.map(|game_id| json!({
                        "launchId":"legacy-session",
                        "gameId":game_id
                    }))
                })
            }
            "app.session.stop" => {
                let force = request["payload"]["force"].as_bool().unwrap_or(false);
                state.prepared.lock().unwrap().push(format!("stop:{force}"));
                json!({"_tag":"Stopped","launchId":"legacy-session"})
            }
            tag => panic!("unexpected legacy tag {tag}"),
        };
        Json(json!([{"exit":{"_tag":"Success","value":value}}]))
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn native_test_router(label: &str, game_id: &str) -> Router {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            format!(
                "label = {label:?}\n[[games]]\nid = {game_id:?}\ntitle = {game_id:?}\ncommand = [\"game\"]\n"
            ),
        )
        .unwrap();
        crate::host_router_with_in_memory_units(&config)
    }

    async fn native_server(label: &str, game_id: &str) -> String {
        serve(native_test_router(label, game_id)).await
    }

    /// Serves a plain native host whose private state root is known so
    /// that the test can learn its device public key and configure a
    /// registry entry that names that exact key.
    async fn identified_native_server(label: &str, game_id: &str) -> (String, String) {
        let root = tempfile::tempdir().unwrap().keep();
        let config = root.join("host.toml");
        std::fs::write(
            &config,
            format!(
                "label = {label:?}\n[[games]]\nid = {game_id:?}\ntitle = {game_id:?}\ncommand = [\"game\"]\n"
            ),
        )
        .unwrap();
        let private = root.join("private");
        let runtime = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            private.clone(),
            Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        );
        runtime.claim_save_owner();
        let device_key = crate::identity::DeviceIdentity::load_or_create(&private)
            .unwrap()
            .device_public_key()
            .unwrap()
            .to_owned();
        let (lan, _) = crate::plain_host_routers_for_tests(runtime);
        (serve(lan).await, device_key)
    }

    #[derive(Clone)]
    struct ExactNativeState {
        shape_requests: Arc<Mutex<Vec<crate::live_display::SessionViewerDisplayRequest>>>,
        shape_replies:
            Arc<Mutex<std::collections::VecDeque<crate::live_display::SessionViewerDisplay>>>,
        launch_id: String,
        active_phase: Arc<std::sync::Mutex<Option<String>>>,
        display: Arc<std::sync::Mutex<Option<crate::launcher::plugin_launch::PluginDisplay>>>,
        viewer_display:
            Arc<std::sync::Mutex<Option<crate::launcher::plugin_launch::PluginDisplay>>>,
        steps: Arc<std::sync::Mutex<Option<korri_surface_treaty::SurfaceSteps>>>,
        stop_pending: bool,
        terminal_error: Arc<std::sync::Mutex<Option<String>>>,
        freeze_failures: Arc<std::sync::Mutex<usize>>,
        thaw_failures: Arc<std::sync::Mutex<usize>>,
        controls: Arc<Mutex<Option<SessionControlsOutcome>>>,
        control_outcome: Arc<Mutex<Option<SessionControlInvokeOutcome>>>,
        control_invocations: Arc<Mutex<Vec<SessionControlInvokeRequest>>>,
    }

    impl ExactNativeState {
        fn fail_next_freeze(&self) {
            *self.freeze_failures.lock().unwrap() += 1;
        }

        fn fail_next_thaw(&self) {
            *self.thaw_failures.lock().unwrap() += 1;
        }

        fn end_with(&self, code: &str) {
            *self.terminal_error.lock().unwrap() = Some(code.into());
            *self.active_phase.lock().unwrap() = None;
        }

        fn set_phase(&self, phase: &str) {
            *self.active_phase.lock().unwrap() = Some(phase.into());
        }
    }

    fn take_failure(failures: &std::sync::Mutex<usize>) -> bool {
        let mut failures = failures.lock().unwrap();
        if *failures == 0 {
            false
        } else {
            *failures -= 1;
            true
        }
    }

    async fn exact_native_rpc(
        State(state): State<ExactNativeState>,
        Json(request): Json<crate::RpcRequest>,
    ) -> Json<crate::RpcResponse> {
        use crate::{
            game_routes::SelectedGameLaunchOutcome, ActiveSession, RpcFailure, RpcResponse,
            SessionEndOutcome, SessionEndPhase, SessionEndResult, SessionFreezerState,
            SessionPrepared, SessionStatus, SessionStatusOutcome, SessionTransitionOutcome,
            SessionTransitionResult,
        };
        if let Some(code) = state.terminal_error.lock().unwrap().clone() {
            let failure = RpcFailure {
                code,
                message: "streamed launch ended".into(),
            };
            match &request {
                crate::RpcRequest::SessionStatus(_) => {
                    return Json(RpcResponse::SessionStatus(SessionStatusOutcome::Err(
                        failure,
                    )))
                }
                crate::RpcRequest::SessionResume(_) => {
                    return Json(RpcResponse::SessionResume(SessionTransitionOutcome::Err(
                        failure,
                    )))
                }
                _ => {}
            }
        }
        let response = match request {
            crate::RpcRequest::SessionStreamPlay(request) => {
                let phase = state.active_phase.lock().unwrap().clone();
                let outcome = match phase {
                    None => crate::stream_play::SessionStreamPlayOutcome::Err(RpcFailure {
                        code: "NoActiveSession".into(),
                        message: "No active session".into(),
                    }),
                    Some(_) if request.launch_id != state.launch_id => {
                        crate::stream_play::SessionStreamPlayOutcome::Err(RpcFailure {
                            code: "LaunchMismatch".into(),
                            message: "Wrong launch".into(),
                        })
                    }
                    Some(phase) => crate::stream_play::SessionStreamPlayOutcome::Ok(
                        serde_json::from_value(serde_json::json!({
                            "launchId":state.launch_id,"gameId":"game","title":"Game","phase":phase,
                            "streamPlayers":[{"devicePublicKey":"player","label":request.device_label}]
                        }))
                        .unwrap(),
                    ),
                };
                RpcResponse::SessionStreamPlay(outcome)
            }
            crate::RpcRequest::SessionStatus(_) => {
                let phase = state.active_phase.lock().unwrap().clone();
                let steps = state.steps.lock().unwrap().clone();
                RpcResponse::SessionStatus(SessionStatusOutcome::Ok(SessionStatus {
                    netplay: None,
                    stream_play_launch_id: None,
                    stream_play_ended: None,
                    active: phase.map(|phase| ActiveSession {
                        netplay: None,
                        person_public_key: None,
                        stream_players: Vec::new(),
                        steps: steps.clone(),
                        initial_handoff: steps.as_ref().map(|_| crate::InitialHandoff::Waiting),
                        focus_ownership: None,
                        launch_id: state.launch_id.clone(),
                        host: None,
                        game_id: Some("game".into()),
                        title: None,
                        phase: Some(phase),
                    }),
                    pending_launches: None,
                    observation_failure: None,
                    overlay: None,
                }))
            }
            crate::RpcRequest::SessionViewerDisplay(request) => {
                state.shape_requests.lock().unwrap().push(request);
                crate::RpcResponse::SessionViewerDisplay(
                    match state.shape_replies.lock().unwrap().pop_front() {
                        Some(response) => {
                            crate::live_display::SessionViewerDisplayOutcome::Ok(response)
                        }
                        None => crate::live_display::SessionViewerDisplayOutcome::Err(
                            crate::RpcFailure {
                                code: "UnexpectedDisplayRequest".into(),
                                message: "no scripted reply".into(),
                            },
                        ),
                    },
                )
            }
            crate::RpcRequest::SessionPrepare(request) => {
                *state.terminal_error.lock().unwrap() = None;
                *state.viewer_display.lock().unwrap() = request.viewer_display;
                *state.active_phase.lock().unwrap() = Some("running".into());
                RpcResponse::SessionPrepare(SelectedGameLaunchOutcome::Ok(
                    crate::game_routes::SelectedGameLaunch {
                        session: SessionPrepared {
                            game_id: "game".into(),
                            launch_id: state.launch_id.clone(),
                            display: state.display.lock().unwrap().clone(),
                        },
                        warnings: Vec::new(),
                    },
                ))
            }
            crate::RpcRequest::SessionControls(request) => {
                assert_eq!(request.launch_id, state.launch_id);
                RpcResponse::SessionControls(state.controls.lock().unwrap().clone().unwrap_or_else(
                    || {
                        SessionControlsOutcome::Ok(crate::SessionControls {
                            launch_id: state.launch_id.clone(),
                            title: None,
                            groups: Vec::new(),
                            retroarch_telemetry: None,
                        })
                    },
                ))
            }
            crate::RpcRequest::SessionControlInvoke(request) => {
                assert_eq!(request.launch_id, state.launch_id);
                state.control_invocations.lock().unwrap().push(request);
                RpcResponse::SessionControlInvoke(
                    state
                        .control_outcome
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or_else(|| {
                            SessionControlInvokeOutcome::Ok(crate::SessionControlCompleted {
                                launch_id: state.launch_id.clone(),
                            })
                        }),
                )
            }
            crate::RpcRequest::SessionLeave(request) => {
                if request.expected_launch_id.as_deref() != Some(state.launch_id.as_str()) {
                    RpcResponse::SessionLeave(SessionTransitionOutcome::Err(RpcFailure {
                        code: "StaleLaunchIdentity".into(),
                        message: "stale".into(),
                    }))
                } else if take_failure(&state.freeze_failures) {
                    RpcResponse::SessionLeave(SessionTransitionOutcome::Err(RpcFailure {
                        code: "FreezeFailed".into(),
                        message: "freeze failed".into(),
                    }))
                } else {
                    *state.active_phase.lock().unwrap() = Some("frozen".into());
                    RpcResponse::SessionLeave(SessionTransitionOutcome::Ok(
                        SessionTransitionResult {
                            launch_id: state.launch_id.clone(),
                            state: SessionFreezerState::Frozen,
                            changed: true,
                        },
                    ))
                }
            }
            crate::RpcRequest::SessionResume(request) => {
                if request.expected_launch_id.as_deref() != Some(state.launch_id.as_str()) {
                    RpcResponse::SessionResume(SessionTransitionOutcome::Err(RpcFailure {
                        code: "StaleLaunchIdentity".into(),
                        message: "stale".into(),
                    }))
                } else if take_failure(&state.thaw_failures) {
                    RpcResponse::SessionResume(SessionTransitionOutcome::Err(RpcFailure {
                        code: "ThawFailed".into(),
                        message: "thaw failed".into(),
                    }))
                } else {
                    *state.active_phase.lock().unwrap() = Some("running".into());
                    RpcResponse::SessionResume(SessionTransitionOutcome::Ok(
                        SessionTransitionResult {
                            launch_id: state.launch_id.clone(),
                            state: SessionFreezerState::Running,
                            changed: true,
                        },
                    ))
                }
            }
            crate::RpcRequest::SessionEnd(request) => {
                if request.expected_launch_id.as_deref() != Some(state.launch_id.as_str()) {
                    RpcResponse::SessionEnd(SessionEndOutcome::Err(RpcFailure {
                        code: "StaleLaunchIdentity".into(),
                        message: "stale".into(),
                    }))
                } else if state.stop_pending {
                    RpcResponse::SessionEnd(SessionEndOutcome::Ok(SessionEndResult {
                        phase: SessionEndPhase::Pending,
                    }))
                } else {
                    *state.active_phase.lock().unwrap() = None;
                    RpcResponse::SessionEnd(SessionEndOutcome::Ok(SessionEndResult {
                        phase: SessionEndPhase::Ended,
                    }))
                }
            }
            other => panic!("unexpected exact native request {other:?}"),
        };
        Json(response)
    }

    #[tokio::test]
    async fn live_shape_reports_screen_applies_encode_and_acknowledges_exact_revision() {
        use crate::launcher::plugin_launch::PluginDisplay;
        use crate::linux_viewer::{ViewerShapeSnapshot, ViewerSnapshot};
        use crate::live_display::{SessionViewerDisplay, ViewerDisplayEvent};
        let (url, holder) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("holder", url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        registry.prepare_stream("game", None).await.unwrap();
        let selected = registry.selected().unwrap();
        viewer.set_snapshot(Some(ViewerSnapshot {
            connected: Some(true),
            shape: Some(ViewerShapeSnapshot {
                connection: "native-connection-one".into(),
                screen: PluginDisplay {
                    width: 2448,
                    height: 1848,
                },
                encoded: PluginDisplay {
                    width: 1920,
                    height: 1080,
                },
                ready: false,
            }),
            ..Default::default()
        }));
        holder.shape_replies.lock().unwrap().extend([
            SessionViewerDisplay {
                display: PluginDisplay {
                    width: 1920,
                    height: 1080,
                },
                owner: true,
                portrait: false,
                revision: None,
                notice: None,
            },
            SessionViewerDisplay {
                display: PluginDisplay {
                    width: 2448,
                    height: 1848,
                },
                owner: true,
                portrait: false,
                revision: Some("revision-one".into()),
                notice: None,
            },
            SessionViewerDisplay {
                display: PluginDisplay {
                    width: 2448,
                    height: 1848,
                },
                owner: true,
                portrait: false,
                revision: None,
                notice: None,
            },
        ]);
        registry.synchronize_shape(&selected).await.unwrap();
        let mut observed = viewer.snapshot(&selected.launch_id).await.unwrap().unwrap();
        observed.shape.as_mut().unwrap().ready = true;
        viewer.set_snapshot(Some(observed));
        registry.synchronize_shape(&selected).await.unwrap();
        let requests = holder.shape_requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(
            matches!(&requests[1].event, ViewerDisplayEvent::Report{display,..} if display.width==2448 && display.height==1848)
        );
        assert!(
            matches!(&requests[2].event, ViewerDisplayEvent::Applied{revision,..} if revision=="revision-one")
        );
        assert_eq!(
            requests[0].event.connection(),
            requests[1].event.connection()
        );
        drop(requests);
        let snapshot = registry
            .local_viewer()
            .snapshot(&selected.launch_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            snapshot.shape.unwrap().encoded,
            PluginDisplay {
                width: 2448,
                height: 1848
            }
        );
        assert!(snapshot
            .controls
            .iter()
            .any(|c| c.label == "Portrait" && c.enabled));
    }

    async fn exact_native_server(stop_pending: bool) -> (String, ExactNativeState) {
        let state = ExactNativeState {
            launch_id: "11111111111111111111111111111111".into(),
            active_phase: Default::default(),
            display: Default::default(),
            viewer_display: Default::default(),
            shape_requests: Default::default(),
            shape_replies: Default::default(),
            steps: Default::default(),
            stop_pending,
            terminal_error: Default::default(),
            freeze_failures: Default::default(),
            thaw_failures: Default::default(),
            controls: Default::default(),
            control_outcome: Default::default(),
            control_invocations: Default::default(),
        };
        let url = serve(
            Router::new()
                .route("/rpc", post(exact_native_rpc))
                .with_state(state.clone()),
        )
        .await;
        (url, state)
    }

    #[tokio::test]
    async fn selected_native_gameplay_controls_forward_values_and_typed_refusals() {
        let (url, state) = exact_native_server(false).await;
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("holder", url)]);
        let launch = registry
            .prepare_stream("game", None)
            .await
            .unwrap()
            .session
            .launch_id;
        assert!(
            matches!(registry.session_controls(&launch).await, SessionControlsOutcome::Ok(c) if c.groups.is_empty())
        );
        for value in [
            None,
            Some(crate::SessionControlValue::Toggle(true)),
            Some(crate::SessionControlValue::Choice("core-option".into())),
            Some(crate::SessionControlValue::Range(0.5)),
        ] {
            let request = SessionControlInvokeRequest {
                launch_id: launch.clone(),
                control_id: "plugin/control".into(),
                value,
            };
            assert!(
                matches!(registry.invoke_session_control(request.clone()).await, SessionControlInvokeOutcome::Ok(c) if c.launch_id == launch)
            );
            assert_eq!(
                serde_json::to_value(state.control_invocations.lock().unwrap().last().unwrap())
                    .unwrap(),
                serde_json::to_value(request).unwrap()
            );
        }
        for reason in [
            SessionControlFailureReason::NotSessionStarter,
            SessionControlFailureReason::StaleSession,
            SessionControlFailureReason::UnknownControl,
            SessionControlFailureReason::Disabled,
            SessionControlFailureReason::InvalidValue,
            SessionControlFailureReason::Unavailable,
        ] {
            let failure = SessionControlFailure {
                reason,
                message: "peer refusal".into(),
            };
            *state.controls.lock().unwrap() = Some(SessionControlsOutcome::Err(failure.clone()));
            *state.control_outcome.lock().unwrap() =
                Some(SessionControlInvokeOutcome::Err(failure.clone()));
            assert!(
                matches!(registry.session_controls(&launch).await, SessionControlsOutcome::Err(f) if f == failure)
            );
            assert!(
                matches!(registry.invoke_session_control(SessionControlInvokeRequest { launch_id: launch.clone(), control_id: "plugin/control".into(), value: None }).await, SessionControlInvokeOutcome::Err(f) if f == failure)
            );
        }
    }

    #[tokio::test]
    async fn gameplay_controls_never_target_an_unselected_launch_or_a_stream_players_holder() {
        let (url, state) = exact_native_server(false).await;
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("holder", url)]);
        assert!(
            matches!(registry.session_controls("absent").await, SessionControlsOutcome::Err(f) if f.reason == SessionControlFailureReason::StaleSession)
        );
        let launch = registry
            .prepare_stream("game", None)
            .await
            .unwrap()
            .session
            .launch_id;
        for stale in ["", "replacement"] {
            assert!(
                matches!(registry.session_controls(stale).await, SessionControlsOutcome::Err(f) if f.reason == SessionControlFailureReason::StaleSession)
            );
            assert!(
                matches!(registry.invoke_session_control(SessionControlInvokeRequest { launch_id: stale.into(), control_id: "plugin/control".into(), value: None }).await, SessionControlInvokeOutcome::Err(f) if f.reason == SessionControlFailureReason::StaleSession)
            );
        }
        // A mismatched successful peer reply must not publish controls for a replacement.
        *state.controls.lock().unwrap() =
            Some(SessionControlsOutcome::Ok(crate::SessionControls {
                launch_id: "replacement".into(),
                title: None,
                groups: Vec::new(),
                retroarch_telemetry: None,
            }));
        *state.control_outcome.lock().unwrap() = Some(SessionControlInvokeOutcome::Ok(
            crate::SessionControlCompleted {
                launch_id: "replacement".into(),
            },
        ));
        assert!(
            matches!(registry.session_controls(&launch).await, SessionControlsOutcome::Err(f) if f.reason == SessionControlFailureReason::StaleSession)
        );
        assert!(
            matches!(registry.invoke_session_control(SessionControlInvokeRequest { launch_id: launch.clone(), control_id: "plugin/control".into(), value: None }).await, SessionControlInvokeOutcome::Err(f) if f.reason == SessionControlFailureReason::StaleSession)
        );
        let calls = state.control_invocations.lock().unwrap().len();
        let mut selected = registry.selected().unwrap();
        selected.participation = Participation::StreamPlayer {
            game_id: Some("game".into()),
            game_title: None,
            device_label: "phone".into(),
        };
        registry.set_selected(selected);
        assert!(
            matches!(registry.session_controls(&launch).await, SessionControlsOutcome::Err(f) if f.reason == SessionControlFailureReason::NotSessionStarter)
        );
        assert!(
            matches!(registry.invoke_session_control(SessionControlInvokeRequest { launch_id: launch, control_id: "plugin/control".into(), value: None }).await, SessionControlInvokeOutcome::Err(f) if f.reason == SessionControlFailureReason::NotSessionStarter)
        );
        assert_eq!(state.control_invocations.lock().unwrap().len(), calls);
    }

    async fn abortable_native_server(
        label: &str,
        game_id: &str,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = native_test_router(label, game_id);
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}"), task)
    }

    async fn forged_native_rpc(Json(request): Json<crate::RpcRequest>) -> Json<crate::RpcResponse> {
        match request {
            crate::RpcRequest::CatalogSnapshot(_) => Json(crate::RpcResponse::CatalogSnapshot(
                crate::CatalogSnapshotOutcome::Ok(crate::CatalogSnapshot {
                    games: vec![Game {
                        id: "forged".into(),
                        title: "Forged source".into(),
                        host: Some("attacker".into()),
                        identity: None,
                        source: GameSource {
                            device_public_key: Some("ff".repeat(32)),
                            label: "attacker".into(),
                            is_local: true,
                        },
                        supports_runner_selection: true,
                        play_stats: Some(crate::PlayStats {
                            last_played: Some("2026-09-04T10:00:00.000Z".into()),
                            play_count: 2,
                            total_playtime_seconds: 120.0,
                        }),
                    }],
                    failures: None,
                }),
            )),
            other => panic!("unexpected forged peer request {other:?}"),
        }
    }

    async fn forged_native_server() -> String {
        serve(Router::new().route("/rpc", post(forged_native_rpc))).await
    }

    async fn legacy_server(state: LegacyServerState) -> String {
        serve(
            Router::new()
                .route("/api/rpc", post(legacy_rpc))
                .with_state(state),
        )
        .await
    }

    async fn legacy_stop_only_rpc(
        State(state): State<LegacyServerState>,
        Json(request): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(request["tag"], "app.session.stop");
        let force = request["payload"]["force"].as_bool().unwrap_or(false);
        state.prepared.lock().unwrap().push(format!("stop:{force}"));
        Json(json!([{"exit":{"_tag":"Success","value":{
            "_tag":"Stopped","launchId":"legacy-session"
        }}}]))
    }

    async fn legacy_stop_only_server(state: LegacyServerState) -> String {
        serve(
            Router::new()
                .route("/api/rpc", post(legacy_stop_only_rpc))
                .with_state(state),
        )
        .await
    }

    async fn legacy_status_unavailable_rpc(
        State(state): State<LegacyServerState>,
        Json(request): Json<Value>,
    ) -> Json<Value> {
        let value = match request["tag"].as_str().unwrap() {
            "app.server.stream.prepare" => {
                let id = request["payload"]["id"].as_str().unwrap().to_owned();
                state.prepared.lock().unwrap().push(id.clone());
                json!({"gameId":id,"sessionId":"legacy-session"})
            }
            "app.session.status" => json!({"_tag":"HostUnavailable"}),
            "app.session.stop" => {
                let force = request["payload"]["force"].as_bool().unwrap_or(false);
                state.prepared.lock().unwrap().push(format!("stop:{force}"));
                json!({"_tag":"Stopped","launchId":"legacy-session"})
            }
            tag => panic!("unexpected legacy status-unavailable tag {tag}"),
        };
        Json(json!([{"exit":{"_tag":"Success","value":value}}]))
    }

    async fn legacy_status_unavailable_server(state: LegacyServerState) -> String {
        serve(
            Router::new()
                .route("/api/rpc", post(legacy_status_unavailable_rpc))
                .with_state(state),
        )
        .await
    }

    #[test]
    fn certificate_caller_deadline_outlives_the_complete_broker_budget() {
        assert!(MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT > NATIVE_RPC_TIMEOUT * 2);
        assert!(MOONLIGHT_CERTIFICATE_CALLER_TIMEOUT > MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT);
    }

    #[tokio::test]
    async fn file_config_falls_back_when_missing_and_surfaces_read_errors() {
        let root = tempfile::tempdir().unwrap();
        let fallback = UpstreamHostConfig::legacy("aka", "http://aka".into());
        let missing = UpstreamRegistry::from_file_or_default(
            &root.path().join("missing.json"),
            fallback.clone(),
        );
        assert_eq!(missing.hosts.len(), 1);
        assert_eq!(missing.hosts[0].label, "aka");
        assert!(matches!(
            &missing.hosts[0].client,
            RegisteredClient::Legacy(_)
        ));

        let unreadable = UpstreamRegistry::from_file_or_default(root.path(), fallback);
        assert!(matches!(
            unreadable.catalog_snapshot().await,
            Err(UpstreamError::Wire(message)) if message.contains("could not read")
        ));
    }

    #[tokio::test]
    async fn missing_file_config_is_retried_when_it_becomes_available() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("upstreams.json");
        let registry = UpstreamRegistry::from_file_or_default(
            &path,
            UpstreamHostConfig::legacy("fallback", "http://fallback.invalid".into()),
        );
        let prepared = Arc::new(std::sync::Mutex::new(vec![]));
        let upstream = legacy_server(LegacyServerState {
            prepared: prepared.clone(),
        })
        .await;
        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(
            registry.catalog_snapshot().await,
            Err(UpstreamError::Wire(_))
        ));
        std::fs::write(
            &path,
            format!(r#"[{{"label":"aka","kind":"legacy","baseUrl":"{upstream}"}}]"#),
        )
        .unwrap();

        let catalog = registry.catalog_snapshot().await.unwrap();

        assert_eq!(catalog.games.len(), 1);
        assert_eq!(catalog.games[0].title, "Legacy game");

        std::fs::remove_file(path).unwrap();
        let cached_catalog = registry.catalog_snapshot().await.unwrap();
        assert_eq!(cached_catalog.games[0].title, "Legacy game");

        registry
            .prepare_stream("shared", Some("aka"))
            .await
            .unwrap();
        assert_eq!(*prepared.lock().unwrap(), vec!["shared"]);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_file_config_is_retried_after_storage_permission_becomes_available() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("upstreams.json");
        std::fs::write(
            &path,
            r#"[{"label":"zao","kind":"native","baseUrl":"http://zao:43117","moonlightAddress":"zao:47989"}]"#,
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let registry = UpstreamRegistry::from_file_or_default(
            &path,
            UpstreamHostConfig::legacy("fallback", "http://fallback.invalid".into()),
        );
        assert!(matches!(
            registry.moonlight_host_candidates(),
            Err(UpstreamError::Wire(message)) if message.contains("could not read upstream config")
        ));

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            registry.moonlight_host_candidates().unwrap(),
            vec![MoonlightHostCandidate {
                label: "zao".into(),
                address: "zao:47989".into(),
            }]
        );
    }

    #[tokio::test]
    async fn config_json_preserves_order_and_surfaces_parse_errors() {
        let registry = UpstreamRegistry::from_json(
            "test",
            r#"[
                {"label":"aka","kind":"legacy","baseUrl":"http://aka"},
                {"label":"zao","kind":"native","baseUrl":"http://zao"}
            ]"#,
        );
        assert_eq!(
            registry
                .hosts
                .iter()
                .map(|host| host.label.as_str())
                .collect::<Vec<_>>(),
            vec!["aka", "zao"],
        );

        let invalid = UpstreamRegistry::from_json("broken.json", "not json");
        assert!(matches!(
            invalid.catalog_snapshot().await,
            Err(UpstreamError::Wire(message)) if message.contains("broken.json")
        ));

        let duplicate_labels = UpstreamRegistry::from_json(
            "test",
            r#"[
                {"label":"zao","kind":"native","baseUrl":"http://first"},
                {"label":"zao","kind":"native","baseUrl":"http://second"}
            ]"#,
        );
        assert!(matches!(
            duplicate_labels.catalog_snapshot().await,
            Err(UpstreamError::Wire(message)) if message.contains("duplicate upstream label \"zao\"")
        ));

        let duplicate_key = "11".repeat(32);
        let duplicate_native_keys = UpstreamRegistry::from_json(
            "test",
            &format!(
                r#"[
                    {{"label":"zao","kind":"native","baseUrl":"http://first","devicePublicKey":"{duplicate_key}"}},
                    {{"label":"sobo","kind":"native","baseUrl":"http://second","devicePublicKey":"{duplicate_key}"}}
                ]"#
            ),
        );
        assert!(matches!(
            duplicate_native_keys.catalog_snapshot().await,
            Err(UpstreamError::Wire(message)) if message.contains("duplicate native devicePublicKey")
        ));
    }

    #[test]
    fn secure_configuration_has_no_legacy_or_incomplete_native_fallback() {
        let private = tempfile::tempdir().unwrap();
        let credentials = PeerCredentials::load(private.path()).unwrap();
        let legacy = UpstreamRegistry::from_json_secure(
            "test",
            r#"[{"label":"aka","kind":"legacy","baseUrl":"http://aka:3001"}]"#,
            credentials.clone(),
        );
        assert!(legacy
            .configuration_error
            .as_deref()
            .is_some_and(|message| message.contains("legacy plaintext")));

        let missing_key = UpstreamRegistry::from_json_secure(
            "test",
            r#"[{"label":"zao","kind":"native","baseUrl":"http://zao:43117","moonlightAddress":"zao:47989"}]"#,
            credentials.clone(),
        );
        assert!(missing_key
            .configuration_error
            .as_deref()
            .is_some_and(|message| message.contains("devicePublicKey")));

        let missing_moonlight = UpstreamRegistry::from_json_secure(
            "test",
            &format!(
                r#"[{{"label":"zao","kind":"native","baseUrl":"http://zao:43117","devicePublicKey":"{}"}}]"#,
                "11".repeat(32)
            ),
            credentials.clone(),
        );
        assert!(missing_moonlight
            .configuration_error
            .as_deref()
            .is_some_and(|message| message.contains("moonlightAddress")));

        let missing_file = UpstreamRegistry::from_file_or_empty(
            &private.path().join("missing-upstreams.json"),
            Some(credentials),
        );
        assert!(missing_file.hosts.is_empty());
        assert!(missing_file.configuration_error.is_none());
        assert!(missing_file.deferred_file_config.is_some());
    }

    #[test]
    fn moonlight_control_origins_derive_hosts_on_the_sunshine_port() {
        let candidates = [
            "https://192.168.1.130:39217",
            "http://holder.example:8099",
            "http://[fd00::1]:39217",
            "http://192.168.1.130:8099",
        ]
        .map(str::to_owned);
        assert_eq!(
            moonlight_control_origins(&candidates),
            [
                "http://192.168.1.130:47989",
                "http://holder.example:47989",
                "http://[fd00::1]:47989",
            ]
        );
    }

    // The Sunshine port is fixed by its protocol. Run these network tests in
    // an isolated namespace, so a build host's own Sunshine cannot answer them.
    #[tokio::test]
    async fn moonlight_control_probe_uses_a_reachable_candidate_without_rpc_or_selection() {
        use tokio::{io::AsyncReadExt, net::TcpListener};
        let sunshine = TcpListener::bind("127.0.0.2:47989").await.unwrap();
        let rpc = TcpListener::bind("127.0.0.2:0").await.unwrap();
        let rpc_origin = format!("http://{}", rpc.local_addr().unwrap());
        let mut config = UpstreamHostConfig::native("holder", rpc_origin.clone());
        config.moonlight_address = None;
        let mut registry = UpstreamRegistry::new(vec![config]);
        let RegisteredClient::Native(client) = registry.hosts[0].client.clone() else {
            unreachable!()
        };
        registry.hosts[0].client = RegisteredClient::Native(
            client
                .with_candidates(vec![
                    "http://127.0.0.3:39217".into(),
                    rpc_origin,
                    "http://127.0.0.2:39218".into(),
                ])
                .unwrap(),
        );
        assert_eq!(
            registry.hosts[0].viewer_address().await.unwrap(),
            "127.0.0.2:47989"
        );
        assert!(!registry.has_selected_session());
        let (mut connection, _) = timeout(Duration::from_secs(1), sunshine.accept())
            .await
            .unwrap()
            .unwrap();
        let mut byte = [0];
        assert_eq!(
            timeout(Duration::from_secs(1), connection.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        assert!(
            timeout(Duration::from_millis(20), sunshine.accept())
                .await
                .is_err(),
            "no duplicate probe"
        );
        assert!(
            timeout(Duration::from_millis(20), rpc.accept())
                .await
                .is_err(),
            "probe must not send RPC"
        );
    }

    #[tokio::test]
    async fn moonlight_control_probe_refuses_when_only_the_rpc_port_answers() {
        let rpc = tokio::net::TcpListener::bind("127.0.0.3:0").await.unwrap();
        let mut config =
            UpstreamHostConfig::native("holder", format!("http://{}", rpc.local_addr().unwrap()));
        config.moonlight_address = None;
        let registry = UpstreamRegistry::new(vec![config]);
        assert!(matches!(
            registry.hosts[0].viewer_address().await,
            Err(UpstreamError::MoonlightHostCandidatesUnavailable)
        ));
        assert!(!registry.has_selected_session());
        assert!(
            timeout(Duration::from_millis(20), rpc.accept())
                .await
                .is_err(),
            "probe must not send RPC"
        );
    }

    #[test]
    fn moonlight_control_origins_reject_non_origins() {
        let candidates = [
            "http://user:secret@holder:39217",
            "http://holder:39217/path",
            "ws://holder:39217",
            "http://holder:0",
        ]
        .map(str::to_owned);
        assert!(moonlight_control_origins(&candidates).is_empty());
    }

    #[test]
    fn moonlight_host_candidates_use_only_explicit_native_peer_addresses() {
        let registry = UpstreamRegistry::from_json(
            "test",
            r#"[
                {"label":"aka","kind":"legacy","baseUrl":"http://aka.example:3000","moonlightAddress":"aka:47989"},
                {"label":"missing","kind":"native","baseUrl":"http://missing.example:43117"},
                {"label":"malformed","kind":"native","baseUrl":"http://malformed.example:43117","moonlightAddress":"   "},
                {"label":"zao","kind":"native","baseUrl":"http://zao:43117","moonlightAddress":"zao:48000"},
                {"label":"desk","kind":"native","baseUrl":"https://desk.example:443/","moonlightAddress":"[::1]:47989"}
            ]"#,
        );

        assert_eq!(
            registry.moonlight_host_candidates().unwrap(),
            vec![
                MoonlightHostCandidate {
                    label: "zao".into(),
                    address: "zao:48000".into(),
                },
                MoonlightHostCandidate {
                    label: "desk".into(),
                    address: "[::1]:47989".into(),
                },
            ]
        );
    }

    #[test]
    fn moonlight_host_candidates_fail_stably_when_no_explicit_native_address_exists() {
        let registry = UpstreamRegistry::from_json(
            "test",
            r#"[
                {"label":"aka","kind":"legacy","baseUrl":"http://aka.example:3000"},
                {"label":"zao","kind":"native","baseUrl":"http://zao:43117"}
            ]"#,
        );

        assert!(matches!(
            registry.moonlight_host_candidates(),
            Err(UpstreamError::MoonlightHostCandidatesUnavailable)
        ));
    }

    #[tokio::test]
    async fn merges_legacy_and_native_hosts_and_routes_duplicate_ids() {
        let legacy_state = LegacyServerState {
            prepared: Default::default(),
        };
        let legacy_url = legacy_server(legacy_state.clone()).await;
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            r#"
label = "zao"
[[games]]
id = "shared"
title = "Native game"
command = ["native-game"]
"#,
        )
        .unwrap();
        let native_url = serve(crate::host_router_with_in_memory_units(&config)).await;
        let device_key = "7bb368b270acb72d81856b7b7010d919ec4882afe7c3aaa56b7b6839e46b47f6";
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_url),
            UpstreamHostConfig::native_secure("zao", native_url, device_key.into()),
        ]);

        let catalog = registry.catalog_snapshot().await.unwrap();
        assert_eq!(catalog.games.len(), 2);
        assert_eq!(catalog.games[0].host.as_deref(), Some("aka"));
        assert_eq!(catalog.games[0].source.device_public_key, None);
        assert_eq!(catalog.games[0].source.label, "aka");
        assert!(!catalog.games[0].source.is_local);
        assert_eq!(catalog.games[1].host.as_deref(), Some("zao"));
        assert_eq!(
            catalog.games[1].source.device_public_key.as_deref(),
            Some(device_key)
        );
        assert_eq!(catalog.games[1].source.label, "zao");
        assert!(!catalog.games[1].source.is_local);
        assert_eq!(catalog.games[0].id, catalog.games[1].id);
        assert!(catalog.failures.is_none());
        assert!(registry
            .prepare_stream("shared", None)
            .await
            .unwrap_err()
            .to_string()
            .contains("host is required"));
        assert!(registry
            .prepare_stream("shared", Some("missing"))
            .await
            .unwrap_err()
            .to_string()
            .contains("missing"));

        registry
            .prepare_stream("shared", Some("aka"))
            .await
            .unwrap();
        assert_eq!(&*legacy_state.prepared.lock().unwrap(), &["shared"]);
        assert!(matches!(
            registry.prepare_stream("shared", Some("zao")).await,
            Err(UpstreamError::ActiveRemoteSessionConflict)
        ));
        assert!(matches!(
            registry.selected().map(|selected| selected.route),
            Some(SelectedRemoteRoute::Legacy { label }) if label == "aka"
        ));
    }

    #[tokio::test]
    async fn legacy_prepare_blocks_a_following_native_prepare() {
        let legacy_state = LegacyServerState {
            prepared: Default::default(),
        };
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_server(legacy_state.clone()).await),
            UpstreamHostConfig::native("zao", native_server("zao", "native").await),
        ]);

        registry
            .prepare_stream("shared", Some("aka"))
            .await
            .unwrap();

        assert!(matches!(
            registry.prepare_stream("native", Some("zao")).await,
            Err(UpstreamError::ActiveRemoteSessionConflict)
        ));
        assert_eq!(&*legacy_state.prepared.lock().unwrap(), &["shared"]);
        assert!(matches!(
            registry.selected().map(|selected| selected.route),
            Some(SelectedRemoteRoute::Legacy { label }) if label == "aka"
        ));
    }

    #[tokio::test]
    async fn selected_legacy_route_bypasses_failed_native_recovery_for_status_and_stop() {
        let legacy_state = LegacyServerState {
            prepared: Default::default(),
        };
        let (native_url, native_task) = abortable_native_server("zao", "native").await;
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_server(legacy_state.clone()).await),
            UpstreamHostConfig::native("zao", native_url),
        ]);

        let prepared = registry
            .prepare_stream("shared", Some("aka"))
            .await
            .unwrap();
        native_task.abort();
        let _ = native_task.await;

        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = registry.session_status().await.unwrap()
        else {
            panic!("selected legacy session must stay visible")
        };
        assert_eq!(active.launch_id, prepared.session.launch_id);
        assert_eq!(active.host.as_deref(), Some("aka"));
        assert!(matches!(
            registry.session_stop(Some("stale-launch"), true).await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert!(matches!(
            registry
                .session_stop(Some(&prepared.session.launch_id), true)
                .await
                .unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
        assert_eq!(
            &*legacy_state.prepared.lock().unwrap(),
            &["shared", "stop:true"]
        );
    }

    #[tokio::test]
    async fn native_prepare_blocks_a_following_legacy_prepare() {
        let legacy_state = LegacyServerState {
            prepared: Default::default(),
        };
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_server(legacy_state.clone()).await),
            UpstreamHostConfig::native("zao", native_server("zao", "native").await),
        ]);

        registry
            .prepare_stream("native", Some("zao"))
            .await
            .unwrap();

        assert!(matches!(
            registry.prepare_stream("shared", Some("aka")).await,
            Err(UpstreamError::ActiveRemoteSessionConflict)
        ));
        assert!(legacy_state.prepared.lock().unwrap().is_empty());
        assert!(registry.selected().is_some());
    }

    #[tokio::test]
    async fn concurrent_mixed_prepares_allow_only_one_remote_session() {
        let legacy_state = LegacyServerState {
            prepared: Default::default(),
        };
        let registry = Arc::new(UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_server(legacy_state.clone()).await),
            UpstreamHostConfig::native("zao", native_server("zao", "native").await),
        ]));
        let legacy_registry = registry.clone();
        let native_registry = registry.clone();

        let (legacy, native) = tokio::join!(
            async move { legacy_registry.prepare_stream("shared", Some("aka")).await },
            async move { native_registry.prepare_stream("native", Some("zao")).await }
        );

        assert_eq!(usize::from(legacy.is_ok()) + usize::from(native.is_ok()), 1);
        let failure = if legacy.is_err() { legacy } else { native };
        assert!(matches!(
            failure,
            Err(UpstreamError::ActiveRemoteSessionConflict)
        ));
        let legacy_won = !legacy_state.prepared.lock().unwrap().is_empty();
        assert!(matches!(
            (
                legacy_won,
                registry.selected().map(|selected| selected.route)
            ),
            (true, Some(SelectedRemoteRoute::Legacy { .. }))
                | (false, Some(SelectedRemoteRoute::Native { .. }))
        ));
    }

    #[tokio::test]
    async fn mixed_prepare_fails_closed_when_legacy_status_is_uncertain() {
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", "http://127.0.0.1:9".into()),
            UpstreamHostConfig::native("zao", native_server("zao", "native").await),
        ]);

        assert!(matches!(
            registry.prepare_stream("native", Some("zao")).await,
            Err(UpstreamError::Unreachable(_))
        ));
        assert!(registry.selected().is_none());
    }

    #[tokio::test]
    async fn native_catalog_replaces_a_forged_source_device_key() {
        let expected_key = "22".repeat(32);
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native_secure(
            "zao",
            forged_native_server().await,
            expected_key.clone(),
        )]);

        let catalog = registry.catalog_snapshot().await.unwrap();

        assert_eq!(catalog.games[0].host.as_deref(), Some("zao"));
        assert_eq!(
            catalog.games[0].source.device_public_key.as_deref(),
            Some(expected_key.as_str())
        );
        assert_eq!(catalog.games[0].source.label, "zao");
        assert!(!catalog.games[0].source.is_local);
    }

    #[tokio::test]
    async fn degrades_one_unreachable_host_to_a_noted_failure() {
        let legacy_url = legacy_server(LegacyServerState {
            prepared: Default::default(),
        })
        .await;
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_url),
            UpstreamHostConfig::native("zao", "http://127.0.0.1:9".into()),
        ]);

        let catalog = registry.catalog_snapshot().await.unwrap();

        assert_eq!(catalog.games.len(), 1);
        let failures = catalog.failures.expect("partial failure");
        assert_eq!(failures[0].host, "zao");
        assert_eq!(failures[0].code, "UpstreamUnreachable");
    }

    #[tokio::test]
    async fn a_single_legacy_catalog_preserves_hostless_games() {
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::legacy(
            "aka",
            legacy_server(LegacyServerState {
                prepared: Default::default(),
            })
            .await,
        )]);

        let catalog = registry.catalog_snapshot().await.unwrap();

        assert_eq!(catalog.games[0].host, None);
        assert_eq!(catalog.games[0].source.device_public_key, None);
        assert_eq!(catalog.games[0].source.label, "aka");
        assert!(!catalog.games[0].source.is_local);
        registry.prepare_stream("shared", None).await.unwrap();
    }

    #[tokio::test]
    async fn legacy_only_prepare_does_not_require_session_status() {
        let state = LegacyServerState {
            prepared: Default::default(),
        };
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::legacy(
            "aka",
            legacy_status_unavailable_server(state.clone()).await,
        )]);

        let prepared = registry.prepare_stream("shared", None).await.unwrap();

        assert_eq!(prepared.session.launch_id, "legacy-session");
        assert!(matches!(
            registry.session_status().await.unwrap(),
            UpstreamSessionStatus::HostUnavailable {}
        ));
        assert!(matches!(
            registry.session_stop(None, true).await.unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
        assert_eq!(&*state.prepared.lock().unwrap(), &["shared", "stop:true"]);
    }

    #[tokio::test]
    async fn qualifies_legacy_session_status_only_in_a_multi_host_registry() {
        let single = UpstreamRegistry::new(vec![UpstreamHostConfig::legacy(
            "aka",
            legacy_server(LegacyServerState {
                prepared: Arc::new(std::sync::Mutex::new(vec!["shared".into()])),
            })
            .await,
        )]);
        let UpstreamSessionStatus::SessionStatus {
            active: Some(single_active),
        } = single.session_status().await.unwrap()
        else {
            panic!("active single-host status")
        };
        assert_eq!(single_active.host, None);

        let multi = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy(
                "aka",
                legacy_server(LegacyServerState {
                    prepared: Arc::new(std::sync::Mutex::new(vec!["shared".into()])),
                })
                .await,
            ),
            UpstreamHostConfig::native("zao", native_server("zao", "neverball").await),
        ]);
        let UpstreamSessionStatus::SessionStatus {
            active: Some(multi_active),
        } = multi.session_status().await.unwrap()
        else {
            panic!("active multi-host status")
        };
        assert_eq!(multi_active.host.as_deref(), Some("aka"));
    }

    #[tokio::test]
    async fn routes_session_stop_to_the_legacy_host() {
        let state = LegacyServerState {
            prepared: Default::default(),
        };
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_stop_only_server(state.clone()).await),
            UpstreamHostConfig::native("zao", native_server("zao", "neverball").await),
        ]);

        let stopped = registry.session_stop(None, true).await.unwrap();

        assert!(matches!(stopped, UpstreamSessionStop::Stopped { .. }));
        assert_eq!(&*state.prepared.lock().unwrap(), &["stop:true"]);
    }

    #[tokio::test]
    async fn selected_native_failure_blocks_legacy_status_and_stop_fallback() {
        let state = LegacyServerState {
            prepared: Default::default(),
        };
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_server(state.clone()).await),
            UpstreamHostConfig::native("zao", "http://127.0.0.1:9".into()),
        ]);

        let launch_id = "aa".repeat(16);
        let host = registry
            .hosts
            .iter()
            .find(|host| matches!(host.client, RegisteredClient::Native(_)))
            .unwrap();
        registry.set_selected(SelectedRemoteSession {
            display: None,
            viewer_target: LinuxViewerStatus::Running,
            participation: Participation::Starter,
            route: SelectedRemoteRoute::Native {
                device_public_key: host.native_route_key().unwrap(),
            },
            launch_id: launch_id.clone(),
            viewer: Some(SelectedViewer {
                moonlight_address: "zao:47989".into(),
                confirmed_live: true,
                reach: None,
            }),
        });
        assert!(matches!(
            registry.session_status().await,
            Err(UpstreamError::Unreachable(_))
        ));
        assert!(matches!(
            registry.session_stop(Some(&launch_id), true).await,
            Err(UpstreamError::Unreachable(_))
        ));
        assert_eq!(registry.selected().unwrap().launch_id, launch_id);
        assert!(state.prepared.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unselected_failed_peer_does_not_block_explicit_native_prepare() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let router = axum::Router::new().route(
            "/rpc",
            axum::routing::post(move || {
                let calls = observed.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    axum::Json(serde_json::json!({
                        "_tag": "app.session.status",
                        "outcome": {"_tag": "Err", "payload": {
                            "code": "HostLaunchFailed",
                            "message": "game unit exited with status 143"
                        }}
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let failed_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let (ready_url, ready) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("failed", failed_url),
            UpstreamHostConfig::native("ready", ready_url),
        ])
        .with_linux_viewer(Arc::new(viewer.clone()));

        let prepared = registry.prepare_stream("game", Some("ready")).await;
        server.abort();
        let prepared = prepared.expect("an unselected peer does not own this device's session");
        assert_eq!(prepared.session.launch_id, ready.launch_id);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "unselected peer was queried"
        );
        assert_eq!(viewer.calls().len(), 1);
        assert_eq!(viewer.calls()[0].action, LinuxViewerAction::Start);
    }

    #[tokio::test]
    async fn terminal_streamed_status_retires_starter_after_viewer_disconnect() {
        for code in ["SessionCompleted", "NoActiveSession", "HostLaunchFailed"] {
            let (url, holder) = exact_native_server(false).await;
            let viewer = TestLinuxStreamViewer::default();
            let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("holder", url)])
                .with_linux_viewer(Arc::new(viewer.clone()));
            registry.prepare_stream("game", None).await.unwrap();
            viewer.seed(&holder.launch_id, LinuxViewerStatus::Stopped);
            // The disconnect wins the first poll, while the holder still says running.
            viewer.fail_next(LinuxViewerAction::Start, "disconnected");
            assert!(registry.session_status().await.is_err());
            holder.end_with(code);
            let outcome = registry.session_status().await;
            if code == "HostLaunchFailed" {
                assert!(
                    matches!(outcome, Err(UpstreamError::Tagged { code, message })
                    if code == "HostLaunchFailed" && message == "streamed launch ended")
                );
            } else {
                assert!(
                    matches!(
                        outcome,
                        Ok(UpstreamSessionStatus::SessionStatus { active: None })
                    ),
                    "{code}"
                );
            }
            assert!(
                matches!(
                    registry.session_status().await,
                    Ok(UpstreamSessionStatus::SessionStatus { active: None })
                ),
                "terminal failure must only be returned once"
            );
            assert!(!registry.has_selected_session(), "{code}");
        }
    }

    #[tokio::test]
    async fn terminal_streamed_resume_retires_selection_instead_of_recovering_viewer() {
        for code in ["SessionCompleted", "NoActiveSession", "HostLaunchFailed"] {
            let (url, holder) = exact_native_server(false).await;
            let viewer = TestLinuxStreamViewer::default();
            let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("holder", url)])
                .with_linux_viewer(Arc::new(viewer.clone()));
            registry.prepare_stream("game", None).await.unwrap();
            viewer.seed(&holder.launch_id, LinuxViewerStatus::Stopped);
            holder.end_with(code);
            let outcome = registry.session_thaw(Some(&holder.launch_id)).await;
            if code == "HostLaunchFailed" {
                assert!(
                    matches!(outcome, Err(UpstreamError::Tagged { code, message })
                    if code == "HostLaunchFailed" && message == "streamed launch ended")
                );
            } else {
                assert!(
                    matches!(outcome, Err(UpstreamError::NoActiveSession)),
                    "{code}"
                );
            }
            assert!(matches!(
                registry.session_status().await,
                Ok(UpstreamSessionStatus::SessionStatus { active: None })
            ));
            assert!(matches!(
                registry.session_thaw(Some(&holder.launch_id)).await,
                Err(UpstreamError::NoActiveSession)
            ));
            assert!(!registry.has_selected_session(), "{code}");
            assert_eq!(
                viewer
                    .calls()
                    .iter()
                    .filter(|c| c.action == LinuxViewerAction::Start)
                    .count(),
                1
            );
        }
    }

    #[tokio::test]
    async fn terminal_streamed_watch_delivers_only_failure_once_and_allows_prepare() {
        for code in ["HostLaunchFailed", "SessionCompleted", "NoActiveSession"] {
            let (url, holder) = exact_native_server(false).await;
            let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("holder", url)])
                .with_linux_viewer(Arc::new(TestLinuxStreamViewer::default()));
            registry.prepare_stream("game", None).await.unwrap();
            holder.end_with(code);
            let notices = Arc::new(std::sync::Mutex::new(Vec::new()));
            let observed = notices.clone();
            let watch = crate::stream_play::device::spawn_watch(registry.clone(), move |message| {
                observed.lock().unwrap().push(message);
            });
            tokio::time::timeout(Duration::from_secs(3), async {
                while registry.has_selected_session() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            tokio::time::sleep(Duration::from_millis(1100)).await;
            watch.abort();
            let notices = notices.lock().unwrap().clone();
            if code == "HostLaunchFailed" {
                assert_eq!(
                    notices,
                    ["upstream returned HostLaunchFailed: streamed launch ended"]
                );
            } else {
                assert!(notices.is_empty(), "{code}");
            }
            registry.prepare_stream("game", None).await.unwrap();
        }
    }

    #[tokio::test]
    async fn terminal_streamed_launch_admits_new_prepare_after_viewer_cleanup() {
        for code in ["HostLaunchFailed", "SessionCompleted", "NoActiveSession"] {
            for status in [LinuxViewerStatus::Stopped, LinuxViewerStatus::Running] {
                let (url, holder) = exact_native_server(false).await;
                let viewer = TestLinuxStreamViewer::default();
                let registry =
                    UpstreamRegistry::new(vec![UpstreamHostConfig::native("holder", url)])
                        .with_linux_viewer(Arc::new(viewer.clone()));
                registry.prepare_stream("game", None).await.unwrap();
                viewer.seed(&holder.launch_id, status);
                holder.end_with(code);
                registry.prepare_stream("game", None).await.expect(code);
                assert_eq!(
                    holder.active_phase.lock().unwrap().as_deref(),
                    Some("running")
                );
                let actions: Vec<_> = viewer.calls().into_iter().map(|c| c.action).collect();
                assert_eq!(
                    actions
                        .iter()
                        .filter(|a| **a == LinuxViewerAction::Start)
                        .count(),
                    2
                );
                if status == LinuxViewerStatus::Running {
                    assert!(
                        actions
                            .iter()
                            .position(|a| *a == LinuxViewerAction::Stop)
                            .unwrap()
                            < actions
                                .iter()
                                .rposition(|a| *a == LinuxViewerAction::Start)
                                .unwrap()
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn terminal_prepare_keeps_selection_until_viewer_cleanup_is_confirmed() {
        for action in [LinuxViewerAction::Status, LinuxViewerAction::Stop] {
            let (url, holder) = exact_native_server(false).await;
            let viewer = TestLinuxStreamViewer::default();
            let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("holder", url)])
                .with_linux_viewer(Arc::new(viewer.clone()));
            registry.prepare_stream("game", None).await.unwrap();
            holder.end_with("HostLaunchFailed");
            viewer.fail_next(action, "viewer unavailable");
            assert!(matches!(registry.prepare_stream("game", None).await,
                Err(UpstreamError::Tagged { code, .. }) if code == "NativeViewerRecoveryIncomplete"));
            assert!(registry.has_selected_session());
            assert_eq!(
                holder.terminal_error.lock().unwrap().as_deref(),
                Some("HostLaunchFailed")
            );
            assert_eq!(
                viewer
                    .calls()
                    .iter()
                    .filter(|call| call.action == LinuxViewerAction::Start)
                    .count(),
                1
            );
            registry.prepare_stream("game", None).await.unwrap();
        }
    }

    #[tokio::test]
    async fn native_prepare_preserves_the_holder_composed_display() {
        let prepared: crate::game_routes::SelectedGameLaunch =
            serde_json::from_value(serde_json::json!({
                "session": {"gameId": "game", "launchId": "exact",
                    "display": {"width": 640, "height": 480}},
                "warnings": []
            }))
            .unwrap();
        assert_eq!(
            serde_json::to_value(prepared).unwrap()["session"]["display"],
            serde_json::json!({"width": 640, "height": 480})
        );
    }

    #[tokio::test]
    async fn native_prepare_starts_one_exact_viewer_with_the_holder_composed_display() {
        use crate::launcher::plugin_launch::PluginDisplay;
        for composed in [
            None,
            Some(PluginDisplay {
                width: 640,
                height: 480,
            }),
            Some(PluginDisplay {
                width: 2448,
                height: 1848,
            }),
        ] {
            let (native_url, state) = exact_native_server(false).await;
            *state.display.lock().unwrap() = composed.clone();
            let viewer = TestLinuxStreamViewer::default();
            let screen = PluginDisplay {
                width: 2448,
                height: 1848,
            };
            viewer.set_display(screen.clone());
            let registry =
                UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                    .with_linux_viewer(Arc::new(viewer.clone()));

            let prepared = registry.prepare_stream("game", None).await.unwrap();

            assert_eq!(*state.viewer_display.lock().unwrap(), Some(screen));
            assert_eq!(prepared.session.launch_id, state.launch_id);
            assert_eq!(prepared.session.display, composed);
            assert_eq!(registry.selected().unwrap().display, composed);
            assert_eq!(
                viewer.calls(),
                vec![LinuxViewerCall {
                    display: composed.clone(),
                    action: LinuxViewerAction::Start,
                    launch_id: state.launch_id.clone(),
                    moonlight_address: Some("zao:47989".into()),
                }]
            );
            assert!(registry.selected().unwrap().viewer.unwrap().confirmed_live);

            // Reopening a released decoder must keep the same composed size.
            viewer.seed(&state.launch_id, LinuxViewerStatus::Stopped);
            registry.session_status().await.unwrap();
            assert_eq!(viewer.calls().last().unwrap().display, composed);
            assert_eq!(
                viewer.calls().last().unwrap().action,
                LinuxViewerAction::Start
            );
        }
    }

    #[tokio::test]
    async fn failed_viewer_start_stops_the_exact_streamed_launch_before_prepare_fails() {
        let (native_url, state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        viewer.fail_next(LinuxViewerAction::Start, "viewer failed");
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));

        assert!(matches!(
            registry.prepare_stream("game", None).await,
            Err(UpstreamError::Tagged { code, message })
                if code == "ViewerStartFailed" && message == "viewer failed"
        ));
        assert!(state.active_phase.lock().unwrap().is_none());
        assert!(registry.selected().is_none());
        assert_eq!(viewer.calls()[0].launch_id, state.launch_id);
    }

    #[tokio::test]
    async fn uncertain_prepare_compensation_keeps_the_exact_session_tracked() {
        let (native_url, state) = exact_native_server(true).await;
        let viewer = TestLinuxStreamViewer::default();
        viewer.fail_next(LinuxViewerAction::Start, "viewer failed");
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer));

        assert!(matches!(
            registry.prepare_stream("game", None).await,
            Err(UpstreamError::Tagged { code, message })
                if code == "ViewerStartCompensationUncertain"
                    && message.contains("holder stop is pending")
        ));
        assert_eq!(
            registry
                .selected()
                .as_ref()
                .map(|selected| selected.launch_id.as_str()),
            Some(state.launch_id.as_str())
        );
        assert!(state.active_phase.lock().unwrap().is_some());
    }

    #[tokio::test]
    async fn android_leave_resume_and_disconnect_never_freeze_or_stop_the_streamed_game() {
        let (native_url, state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        let prepared = registry.prepare_stream("game", None).await.unwrap();
        let launch = prepared.session.launch_id;
        state.fail_next_freeze(); // A freeze would fail, not quietly pass this test.
        for _ in 0..4 {
            let result = registry.session_leave_live(Some(&launch)).await.unwrap();
            assert_eq!(result.state, crate::SessionFreezerState::Running);
            assert!(!result.changed);
            assert_eq!(
                state.active_phase.lock().unwrap().as_deref(),
                Some("running")
            );
            registry.session_resume_live(Some(&launch)).await.unwrap();
        }
        assert_eq!(*state.freeze_failures.lock().unwrap(), 1);
        assert!(!viewer.calls().iter().any(|call| matches!(
            call.action,
            LinuxViewerAction::Background | LinuxViewerAction::Stop
        )));
        registry
            .session_viewer(&launch, crate::SessionViewerAction::Disconnect)
            .await
            .unwrap();
        assert_eq!(
            viewer.calls().last().unwrap().action,
            LinuxViewerAction::Stop
        );
        let starts_before_status = viewer
            .calls()
            .iter()
            .filter(|c| c.action == LinuxViewerAction::Start)
            .count();
        for _ in 0..3 {
            let status = registry.session_status().await.unwrap();
            let active = active_session(&status).unwrap();
            assert_eq!(active.launch_id, launch);
            assert_eq!(active.phase.as_deref(), Some("running"));
        }
        assert_eq!(
            starts_before_status,
            viewer
                .calls()
                .iter()
                .filter(|c| c.action == LinuxViewerAction::Start)
                .count()
        );
        registry.session_resume_live(Some(&launch)).await.unwrap();
        assert_eq!(
            viewer.calls().last().unwrap().action,
            LinuxViewerAction::Start
        );
        assert_eq!(viewer.calls().last().unwrap().launch_id, launch);
        assert_eq!(
            state.active_phase.lock().unwrap().as_deref(),
            Some("running")
        );
    }

    #[tokio::test]
    async fn android_open_korri_stays_backgrounded_through_observation_until_resume() {
        let (native_url, state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        let launch = registry
            .prepare_stream("game", None)
            .await
            .unwrap()
            .session
            .launch_id;
        registry
            .session_viewer(&launch, crate::SessionViewerAction::OpenKorri)
            .await
            .unwrap();
        for _ in 0..3 {
            registry.session_status().await.unwrap();
        }
        assert!(!viewer
            .calls()
            .iter()
            .any(|call| call.action == LinuxViewerAction::ResumeAndFocus));
        registry.session_resume_live(Some(&launch)).await.unwrap();
        assert_eq!(
            viewer.calls().last().unwrap().action,
            LinuxViewerAction::ResumeAndFocus
        );
        assert_eq!(
            state.active_phase.lock().unwrap().as_deref(),
            Some("running")
        );
        registry.session_stop(Some(&launch), false).await.unwrap();
        assert!(state.active_phase.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn live_viewer_controls_refuse_missing_stale_and_replaced_launches() {
        let (native_url, _state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        registry.prepare_stream("game", None).await.unwrap();
        assert!(matches!(
            registry.session_leave_live(None).await,
            Err(UpstreamError::ExpectedLaunchIdRequired)
        ));
        assert!(matches!(
            registry.session_leave_live(Some("other")).await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert!(matches!(
            registry.session_resume_live(Some("other")).await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert!(matches!(
            registry
                .session_viewer("other", crate::SessionViewerAction::Disconnect)
                .await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert_eq!(viewer.calls().len(), 1);
        let mut replacement = registry.selected().unwrap();
        replacement.launch_id = "replacement".into();
        registry.set_selected(replacement);
        assert!(matches!(
            registry.session_leave_live(Some("replacement")).await,
            Err(UpstreamError::SelectedRemoteSessionReplaced)
        ));
        assert_eq!(viewer.calls().len(), 1);
    }

    #[tokio::test]
    async fn freeze_thaw_and_stop_control_the_same_viewer_only_after_holder_confirmation() {
        let (native_url, state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        let prepared = registry.prepare_stream("game", None).await.unwrap();

        registry
            .session_freeze(Some(&prepared.session.launch_id))
            .await
            .unwrap();
        assert_eq!(
            state.active_phase.lock().unwrap().as_deref(),
            Some("frozen")
        );
        registry
            .session_thaw(Some(&prepared.session.launch_id))
            .await
            .unwrap();
        assert_eq!(
            state.active_phase.lock().unwrap().as_deref(),
            Some("running")
        );
        registry
            .session_stop(Some(&prepared.session.launch_id), false)
            .await
            .unwrap();
        assert!(state.active_phase.lock().unwrap().is_none());
        assert_eq!(
            viewer
                .calls()
                .into_iter()
                .map(|call| (call.action, call.launch_id))
                .collect::<Vec<_>>(),
            vec![
                (LinuxViewerAction::Start, prepared.session.launch_id.clone()),
                (
                    LinuxViewerAction::Status,
                    prepared.session.launch_id.clone()
                ),
                (
                    LinuxViewerAction::Background,
                    prepared.session.launch_id.clone()
                ),
                (
                    LinuxViewerAction::Status,
                    prepared.session.launch_id.clone()
                ),
                (
                    LinuxViewerAction::ResumeAndFocus,
                    prepared.session.launch_id.clone()
                ),
                (LinuxViewerAction::Stop, prepared.session.launch_id),
            ]
        );
    }

    #[tokio::test]
    async fn stale_selected_route_requests_have_zero_viewer_effects() {
        let (native_url, state) = exact_native_server(false).await;
        NativeClient::new(native_url.clone())
            .prepare_stream("game", None)
            .await
            .unwrap();
        let viewer = TestLinuxStreamViewer::default();
        let registry = selected_viewer_registry(
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                .with_linux_viewer(Arc::new(viewer.clone())),
            &state.launch_id,
        )
        .await;
        let stale = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        assert!(matches!(
            registry.session_freeze(Some(stale)).await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert!(matches!(
            registry.session_thaw(Some(stale)).await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert!(matches!(
            registry.session_stop(Some(stale), false).await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert!(viewer.calls().is_empty());
        assert_eq!(registry.selected().unwrap().launch_id, state.launch_id);
    }

    #[tokio::test]
    async fn native_freeze_and_thaw_require_nonempty_exact_ids_on_the_selected_route() {
        let (native_url, state) = exact_native_server(false).await;
        NativeClient::new(native_url.clone())
            .prepare_stream("game", None)
            .await
            .unwrap();
        let viewer = TestLinuxStreamViewer::default();
        let registry = selected_viewer_registry(
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                .with_linux_viewer(Arc::new(viewer.clone())),
            &state.launch_id,
        )
        .await;

        assert!(matches!(
            registry.session_freeze(None).await,
            Err(UpstreamError::ExpectedLaunchIdRequired)
        ));
        assert!(matches!(
            registry.session_thaw(Some("")).await,
            Err(UpstreamError::ExpectedLaunchIdRequired)
        ));
        assert!(viewer.calls().is_empty());
        assert_eq!(registry.selected().unwrap().launch_id, state.launch_id);
    }

    #[tokio::test]
    async fn freeze_viewer_failure_thaws_the_exact_streamed_launch() {
        let (native_url, state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        let prepared = registry.prepare_stream("game", None).await.unwrap();
        viewer.fail_next(LinuxViewerAction::Background, "background failed");

        assert!(matches!(
            registry.session_freeze(Some(&prepared.session.launch_id)).await,
            Err(UpstreamError::Tagged { code, message })
                if code == "ViewerControlFailed"
                    && message.contains("holder change was compensated")
        ));
        assert_eq!(
            state.active_phase.lock().unwrap().as_deref(),
            Some("running")
        );
        assert_eq!(
            registry.selected().unwrap().launch_id,
            prepared.session.launch_id
        );
    }

    #[tokio::test]
    async fn thaw_viewer_failure_refreezes_the_exact_streamed_launch() {
        let (native_url, state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        let prepared = registry.prepare_stream("game", None).await.unwrap();
        registry
            .session_freeze(Some(&prepared.session.launch_id))
            .await
            .unwrap();
        viewer.fail_next(LinuxViewerAction::ResumeAndFocus, "focus failed");

        assert!(matches!(
            registry.session_thaw(Some(&prepared.session.launch_id)).await,
            Err(UpstreamError::Tagged { code, message })
                if code == "ViewerControlFailed"
                    && message.contains("holder change was compensated")
        ));
        assert_eq!(
            state.active_phase.lock().unwrap().as_deref(),
            Some("frozen")
        );
        assert_eq!(
            registry.selected().unwrap().launch_id,
            prepared.session.launch_id
        );
    }

    #[tokio::test]
    async fn viewer_failure_and_holder_compensation_failure_report_uncertainty() {
        let (native_url, state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        let prepared = registry.prepare_stream("game", None).await.unwrap();
        viewer.fail_next(LinuxViewerAction::Background, "background failed");
        state.fail_next_thaw();

        assert!(matches!(
            registry.session_freeze(Some(&prepared.session.launch_id)).await,
            Err(UpstreamError::Tagged { code, message })
                if code == "ViewerControlCompensationUncertain"
                    && message.contains("holder compensation failed")
        ));
        assert_eq!(
            state.active_phase.lock().unwrap().as_deref(),
            Some("frozen")
        );
        assert_eq!(
            registry.selected().unwrap().launch_id,
            prepared.session.launch_id
        );
    }

    #[tokio::test]
    async fn thaw_viewer_and_refreeze_failure_report_uncertainty() {
        let (native_url, state) = exact_native_server(false).await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        let prepared = registry.prepare_stream("game", None).await.unwrap();
        registry
            .session_freeze(Some(&prepared.session.launch_id))
            .await
            .unwrap();
        viewer.fail_next(LinuxViewerAction::ResumeAndFocus, "focus failed");
        state.fail_next_freeze();

        assert!(matches!(
            registry.session_thaw(Some(&prepared.session.launch_id)).await,
            Err(UpstreamError::Tagged { code, message })
                if code == "ViewerControlCompensationUncertain"
                    && message.contains("holder compensation failed")
        ));
        assert_eq!(
            state.active_phase.lock().unwrap().as_deref(),
            Some("running")
        );
        assert_eq!(
            registry.selected().unwrap().launch_id,
            prepared.session.launch_id
        );
    }

    #[tokio::test]
    async fn lost_holder_stop_confirmation_keeps_the_exact_viewer_live_and_tracked() {
        let (native_url, task) = abortable_native_server("zao", "neverball").await;
        let viewer = TestLinuxStreamViewer::default();
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        let prepared = registry.prepare_stream("neverball", None).await.unwrap();
        task.abort();
        tokio::task::yield_now().await;

        assert!(matches!(
            registry
                .session_stop(Some(&prepared.session.launch_id), false)
                .await,
            Err(UpstreamError::Unreachable(_))
        ));
        assert_eq!(
            viewer
                .calls()
                .iter()
                .filter(|call| call.action == LinuxViewerAction::Stop)
                .count(),
            0
        );
        assert_eq!(
            registry.selected().unwrap().launch_id,
            prepared.session.launch_id
        );
    }

    #[tokio::test]
    async fn runner_steps_cross_native_rpc_registry_and_session_status_outcome() {
        let (url, state) = exact_native_server(false).await;
        let steps = korri_surface_treaty::SurfaceSteps {
            labels: vec!["Starting Steam".into(), "Starting the game".into()],
            current: 1,
        };
        *state.steps.lock().unwrap() = Some(steps.clone());
        NativeClient::new(url.clone())
            .prepare_stream("game", None)
            .await
            .unwrap();
        let registry = selected_viewer_registry(
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("peer", url)])
                .with_linux_viewer(Arc::new(TestLinuxStreamViewer::default())),
            &state.launch_id,
        )
        .await;
        let outcome = crate::session_status_outcome(registry.session_status().await);
        let crate::SessionStatusOutcome::Ok(status) = outcome else {
            panic!("status failed")
        };
        let active = status.active.unwrap();
        assert_eq!(active.host.as_deref(), Some("peer"));
        assert_eq!(active.steps, Some(steps));
        assert_eq!(active.initial_handoff, Some(crate::InitialHandoff::Waiting));
        state.steps.lock().unwrap().take();
        let outcome = crate::session_status_outcome(registry.session_status().await);
        let crate::SessionStatusOutcome::Ok(status) = outcome else {
            panic!("status failed")
        };
        assert!(status.active.unwrap().steps.is_none());
    }

    #[tokio::test]
    async fn recovery_restores_only_the_explicitly_selected_streamed_launch_viewer() {
        let (native_url, state) = exact_native_server(false).await;
        NativeClient::new(native_url.clone())
            .prepare_stream("game", None)
            .await
            .unwrap();
        let viewer = TestLinuxStreamViewer::default();
        let registry = selected_viewer_registry(
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                .with_linux_viewer(Arc::new(viewer.clone())),
            &state.launch_id,
        )
        .await;

        let status = registry.session_status().await.unwrap();

        assert_eq!(active_session(&status).unwrap().launch_id, state.launch_id);
        assert_eq!(
            viewer.calls(),
            vec![
                LinuxViewerCall {
                    display: None,
                    action: LinuxViewerAction::Status,
                    launch_id: state.launch_id.clone(),
                    moonlight_address: None,
                },
                LinuxViewerCall {
                    display: None,
                    action: LinuxViewerAction::Start,
                    launch_id: state.launch_id,
                    moonlight_address: Some("zao:47989".into()),
                },
            ]
        );
    }

    #[tokio::test]
    async fn recovery_correlates_holder_freeze_with_the_exact_running_viewer() {
        let (native_url, state) = exact_native_server(false).await;
        let client = NativeClient::new(native_url.clone());
        client.prepare_stream("game", None).await.unwrap();
        client.session_freeze(&state.launch_id).await.unwrap();
        let viewer = TestLinuxStreamViewer::default();
        viewer.seed(state.launch_id.clone(), LinuxViewerStatus::Running);
        let registry = selected_viewer_registry(
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                .with_linux_viewer(Arc::new(viewer.clone())),
            &state.launch_id,
        )
        .await;

        registry.session_status().await.unwrap();

        assert_eq!(
            viewer
                .calls()
                .into_iter()
                .map(|call| call.action)
                .collect::<Vec<_>>(),
            vec![LinuxViewerAction::Status, LinuxViewerAction::Background]
        );
    }

    #[tokio::test]
    async fn stopping_recovery_has_zero_viewer_effects() {
        let (native_url, state) = exact_native_server(false).await;
        NativeClient::new(native_url.clone())
            .prepare_stream("game", None)
            .await
            .unwrap();
        state.set_phase("stopping");
        let viewer = TestLinuxStreamViewer::default();
        let registry = selected_viewer_registry(
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                .with_linux_viewer(Arc::new(viewer.clone())),
            &state.launch_id,
        )
        .await;

        assert!(matches!(
            registry.session_status().await,
            Err(UpstreamError::Tagged { code, message })
                if code == "NativeViewerRecoveryIncomplete"
                    && message.contains("stopping")
        ));
        assert!(viewer.calls().is_empty());
        assert_eq!(registry.selected().unwrap().launch_id, state.launch_id);
    }

    #[tokio::test]
    async fn status_recovery_and_stop_share_one_lifecycle_mutation_gate() {
        let (native_url, state) = exact_native_server(false).await;
        NativeClient::new(native_url.clone())
            .prepare_stream("game", None)
            .await
            .unwrap();
        let viewer = TestLinuxStreamViewer::default();
        viewer.seed(state.launch_id.clone(), LinuxViewerStatus::Stopped);
        let status_started = Arc::new(tokio::sync::Semaphore::new(0));
        let status_release = Arc::new(tokio::sync::Semaphore::new(0));
        viewer.block_status(status_started.clone(), status_release.clone());
        let registry = Arc::new(
            selected_viewer_registry(
                UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                    .with_linux_viewer(Arc::new(viewer.clone())),
                &state.launch_id,
            )
            .await,
        );

        let status_registry = registry.clone();
        let status_task = tokio::spawn(async move { status_registry.session_status().await });
        status_started.acquire().await.unwrap().forget();
        let stop_registry = registry.clone();
        let launch_id = state.launch_id.clone();
        let stop_task =
            tokio::spawn(async move { stop_registry.session_stop(Some(&launch_id), false).await });
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!stop_task.is_finished());

        status_release.add_permits(1);
        status_task.await.unwrap().unwrap();
        assert!(matches!(
            stop_task.await.unwrap().unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
        assert_eq!(
            viewer
                .calls()
                .into_iter()
                .map(|call| call.action)
                .collect::<Vec<_>>(),
            vec![
                LinuxViewerAction::Status,
                LinuxViewerAction::Start,
                LinuxViewerAction::Stop,
            ]
        );
        assert!(registry.selected().is_none());
    }

    #[tokio::test]
    async fn recovery_reports_explicit_incomplete_state_without_guessing_a_viewer() {
        let (native_url, state) = exact_native_server(false).await;
        NativeClient::new(native_url.clone())
            .prepare_stream("game", None)
            .await
            .unwrap();
        let viewer = TestLinuxStreamViewer::default();
        viewer.fail_next(LinuxViewerAction::Status, "viewer status unavailable");
        let registry = selected_viewer_registry(
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                .with_linux_viewer(Arc::new(viewer.clone())),
            &state.launch_id,
        )
        .await;

        assert!(matches!(
            registry.session_status().await,
            Err(UpstreamError::Tagged { code, message })
                if code == "NativeViewerRecoveryIncomplete"
                    && message == "viewer status unavailable"
        ));
        assert_eq!(registry.selected().unwrap().launch_id, state.launch_id);
        assert_eq!(viewer.calls().len(), 1);
        assert_eq!(viewer.calls()[0].action, LinuxViewerAction::Status);
    }

    #[tokio::test]
    async fn native_prepare_status_and_exact_stop_use_the_selected_peer() {
        let native_url = native_server("zao", "neverball").await;
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)]);

        let prepared = registry.prepare_stream("neverball", None).await.unwrap();
        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = registry.session_status().await.unwrap()
        else {
            panic!("selected native session must be active")
        };
        assert_eq!(active.launch_id, prepared.session.launch_id);
        assert_eq!(active.host.as_deref(), Some("zao"));
        assert_eq!(active.game_id.as_deref(), Some("neverball"));
        assert!(matches!(
            registry.session_stop(None, false).await,
            Err(UpstreamError::ExpectedLaunchIdRequired)
        ));
        assert!(matches!(
            registry
                .session_stop(Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), false)
                .await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert_eq!(
            registry.selected().unwrap().launch_id,
            prepared.session.launch_id
        );
        assert!(matches!(
            registry
                .session_stop(Some(&prepared.session.launch_id), false)
                .await
                .unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
        assert!(matches!(
            registry.session_status().await.unwrap(),
            UpstreamSessionStatus::SessionStatus { active: None }
        ));
    }

    #[tokio::test]
    async fn freeze_follows_the_selected_native_route_and_thaw_needs_focus_authority() {
        let native_url = native_server("zao", "neverball").await;
        let registry =
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url.clone())]);

        assert!(matches!(
            registry.session_freeze(None).await,
            Err(UpstreamError::NoActiveSession)
        ));
        let prepared = registry.prepare_stream("neverball", None).await.unwrap();
        assert!(matches!(
            registry
                .session_freeze(Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"))
                .await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        let frozen = registry
            .session_freeze(Some(&prepared.session.launch_id))
            .await
            .unwrap();
        assert_eq!(frozen.launch_id, prepared.session.launch_id);
        assert_eq!(frozen.state, crate::SessionFreezerState::Frozen);
        assert!(frozen.changed);
        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = registry.session_status().await.unwrap()
        else {
            panic!("frozen session stays active")
        };
        assert_eq!(active.phase.as_deref(), Some("frozen"));
        assert_eq!(
            registry.selected().unwrap().launch_id,
            prepared.session.launch_id
        );

        // An idle registry cannot adopt the frozen session. A known selected
        // route still requires the holder's exact focus authority on thaw.
        let recovered = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)]);
        assert!(matches!(
            recovered
                .session_thaw(Some(&prepared.session.launch_id))
                .await,
            Err(UpstreamError::NoActiveSession)
        ));
        let recovered = selected_viewer_registry(recovered, &prepared.session.launch_id).await;
        assert!(matches!(
            recovered.session_thaw(Some(&prepared.session.launch_id)).await,
            Err(UpstreamError::Tagged { code, message })
                if code == "HostFocusFailed"
                    && message == "compositor focus authority is not configured"
        ));
        assert_eq!(
            recovered.selected().unwrap().launch_id,
            prepared.session.launch_id
        );
        assert!(matches!(
            recovered.session_thaw(None).await,
            Err(UpstreamError::ExpectedLaunchIdRequired)
        ));

        assert!(matches!(
            registry
                .session_stop(Some(&prepared.session.launch_id), false)
                .await
                .unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
        // The other registry still holds the route; the host reports no
        // active launch, which clears it.
        assert!(matches!(
            recovered
                .session_freeze(Some(&prepared.session.launch_id))
                .await,
            Err(UpstreamError::NoActiveSession)
        ));
        assert!(recovered.selected().is_none());
        assert!(matches!(
            recovered
                .session_stop(Some(&prepared.session.launch_id), false)
                .await
                .unwrap(),
            UpstreamSessionStop::NothingToStop {}
        ));
        assert!(recovered.selected().is_none());
    }

    #[tokio::test]
    async fn peer_reported_replacement_refuses_old_viewer_control_and_preserves_evidence() {
        let native_url = native_server("zao", "neverball").await;
        let viewer = TestLinuxStreamViewer::default();
        let first =
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url.clone())])
                .with_linux_viewer(Arc::new(viewer.clone()));
        let prepared = first.prepare_stream("neverball", None).await.unwrap();

        // The holder ends its launch; another device then selects a new one.
        // The first device still holds the old route.
        NativeClient::new(native_url.clone())
            .session_stop(&prepared.session.launch_id, false)
            .await
            .unwrap();
        let second = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)]);
        let replacement = second.prepare_stream("neverball", None).await.unwrap();
        assert_ne!(replacement.session.launch_id, prepared.session.launch_id);

        // The peer reports the replacement for the old launch. The first
        // brain refuses control and preserves the exact old evidence without
        // touching its viewer or retargeting the replacement.
        assert!(matches!(
            first
                .session_freeze(Some(&prepared.session.launch_id))
                .await,
            Err(UpstreamError::SelectedRemoteSessionReplaced)
        ));
        assert!(matches!(
            first
                .session_stop(Some(&prepared.session.launch_id), false)
                .await,
            Err(UpstreamError::SelectedRemoteSessionReplaced)
        ));
        assert_eq!(
            first
                .selected()
                .as_ref()
                .map(|selected| selected.launch_id.as_str()),
            Some(prepared.session.launch_id.as_str())
        );
        assert_eq!(
            viewer
                .calls()
                .iter()
                .filter(|call| call.action == LinuxViewerAction::Stop)
                .count(),
            0
        );
        // The replacement launch was not frozen or stopped by the stale request.
        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = second.session_status().await.unwrap()
        else {
            panic!("replacement launch must be active")
        };
        assert_eq!(active.launch_id, replacement.session.launch_id);
        assert_eq!(active.phase.as_deref(), Some("running"));
    }

    #[tokio::test]
    async fn freeze_and_thaw_on_an_empty_registry_report_no_active_session() {
        let empty = UpstreamRegistry::new(vec![]);
        assert!(matches!(
            empty.session_freeze(None).await,
            Err(UpstreamError::NoActiveSession)
        ));
        assert!(matches!(
            empty
                .session_thaw(Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"))
                .await,
            Err(UpstreamError::NoActiveSession)
        ));
    }

    #[tokio::test]
    async fn source_status_selects_exactly_one_native_peer_by_device_key() {
        let (zao_url, zao_key) = identified_native_server("zao", "neverball").await;
        let (aka_url, aka_key) = identified_native_server("aka", "other").await;
        assert_ne!(zao_key, aka_key);
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native_secure("zao", zao_url, zao_key.clone()),
            UpstreamHostConfig::native_secure("aka", aka_url, aka_key.clone()),
        ]);

        let zao = registry.source_status(&zao_key).await.unwrap();
        assert_eq!(zao.catalog, crate::SourceCatalogState::Available);
        // In-memory test hosts run with the production adapter, which has no
        // socket configured, so stream control is honestly disabled.
        assert_eq!(
            zao.stream_control,
            crate::SourceStreamControlState::Disabled
        );
        let aka = registry.source_status(&aka_key).await.unwrap();
        assert_eq!(aka.catalog, crate::SourceCatalogState::Available);
    }

    #[tokio::test]
    async fn source_status_fails_closed_for_unknown_and_legacy_keys() {
        let state = LegacyServerState {
            prepared: Default::default(),
        };
        let (zao_url, zao_key) = identified_native_server("zao", "neverball").await;
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_server(state.clone()).await),
            UpstreamHostConfig::native_secure("zao", zao_url, zao_key.clone()),
        ]);

        let unknown = "ee".repeat(32);
        assert!(matches!(
            registry.source_status(&unknown).await,
            Err(UpstreamError::SourcePeerNotFound)
        ));
        // A legacy host has no device key, so no key can select it.
        assert!(matches!(
            registry.source_status("").await,
            Err(UpstreamError::SourcePeerNotFound)
        ));
        assert!(matches!(
            registry.source_status("aka").await,
            Err(UpstreamError::SourcePeerNotFound)
        ));
        // No legacy call was made by any failed selection.
        assert!(state.prepared.lock().unwrap().is_empty());
        // The native peer still answers for its own key.
        assert!(registry.source_status(&zao_key).await.is_ok());
    }

    #[tokio::test]
    async fn source_status_transport_failure_is_an_error_not_a_false_status() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let key = "dd".repeat(32);
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native_secure(
            "gone",
            dead_url,
            key.clone(),
        )]);
        assert!(matches!(
            registry.source_status(&key).await,
            Err(UpstreamError::Unreachable(_))
        ));
    }

    #[tokio::test]
    async fn source_status_rejects_a_registry_key_that_the_peer_does_not_own() {
        // The registry names a key the peer host does not hold. The host
        // refuses to answer for a foreign key, and the brain surfaces the
        // peer's tagged failure rather than a fabricated status.
        let native_url = native_server("zao", "neverball").await;
        let mismatched = "cc".repeat(32);
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native_secure(
            "zao",
            native_url,
            mismatched.clone(),
        )]);
        assert!(matches!(
            registry.source_status(&mismatched).await,
            Err(UpstreamError::Tagged { code, .. }) if code == "SourceDeviceMismatch"
        ));
    }

    #[tokio::test]
    async fn freeze_and_thaw_refuse_legacy_routes_without_calling_them() {
        let state = LegacyServerState {
            prepared: Default::default(),
        };
        let legacy_only = UpstreamRegistry::new(vec![UpstreamHostConfig::legacy(
            "aka",
            legacy_server(state.clone()).await,
        )]);
        legacy_only.prepare_stream("any", None).await.unwrap();
        assert!(matches!(
            legacy_only.session_freeze(None).await,
            Err(UpstreamError::FreezerUnsupportedOnLegacyRoute)
        ));
        assert!(matches!(
            legacy_only.session_thaw(None).await,
            Err(UpstreamError::FreezerUnsupportedOnLegacyRoute)
        ));
        assert_eq!(&*state.prepared.lock().unwrap(), &["any"]);
        assert!(legacy_only.selected().is_some());
    }

    #[tokio::test]
    async fn idle_native_registry_never_adopts_or_controls_an_unselected_live_peer() {
        let native_url = native_server("zao", "neverball").await;
        let starter =
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url.clone())]);
        let prepared = starter.prepare_stream("neverball", None).await.unwrap();
        let viewer = TestLinuxStreamViewer::default();
        let idle = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
            .with_linux_viewer(Arc::new(viewer.clone()));
        assert!(matches!(
            idle.session_status().await.unwrap(),
            UpstreamSessionStatus::SessionStatus { active: None }
        ));
        assert!(matches!(
            idle.session_stop(Some(&prepared.session.launch_id), false)
                .await
                .unwrap(),
            UpstreamSessionStop::NothingToStop {}
        ));
        assert!(matches!(
            idle.session_freeze(Some(&prepared.session.launch_id)).await,
            Err(UpstreamError::NoActiveSession)
        ));
        assert!(matches!(
            idle.session_thaw(Some(&prepared.session.launch_id)).await,
            Err(UpstreamError::NoActiveSession)
        ));
        assert!(idle.selected().is_none());
        assert!(viewer.calls().is_empty());
        assert!(
            matches!(starter.session_status().await.unwrap(), UpstreamSessionStatus::SessionStatus { active: Some(active) } if active.launch_id == prepared.session.launch_id)
        );
    }

    #[tokio::test]
    async fn sequential_native_prepare_does_not_replace_an_active_selection() {
        let zao = native_server("zao", "one").await;
        let sobo = native_server("sobo", "two").await;
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("zao", zao),
            UpstreamHostConfig::native("sobo", sobo),
        ]);

        let first = registry.prepare_stream("one", Some("zao")).await.unwrap();
        assert!(matches!(
            registry.prepare_stream("two", Some("sobo")).await,
            Err(UpstreamError::ActiveRemoteSessionConflict)
        ));
        assert_eq!(
            registry.selected().unwrap().launch_id,
            first.session.launch_id
        );
    }

    #[tokio::test]
    async fn concurrent_native_prepares_allow_only_one_selected_session() {
        let zao = native_server("zao", "one").await;
        let sobo = native_server("sobo", "two").await;
        let registry = Arc::new(UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("zao", zao),
            UpstreamHostConfig::native("sobo", sobo),
        ]));
        let first_registry = registry.clone();
        let second_registry = registry.clone();

        let (first, second) = tokio::join!(
            async move { first_registry.prepare_stream("one", Some("zao")).await },
            async move { second_registry.prepare_stream("two", Some("sobo")).await }
        );

        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
        let failure = if first.is_err() { first } else { second };
        assert!(matches!(
            failure,
            Err(UpstreamError::ActiveRemoteSessionConflict)
        ));
        assert!(registry.selected().is_some());
    }

    #[tokio::test]
    async fn uncertain_selected_session_blocks_a_second_native_prepare() {
        let reachable = native_server("sobo", "two").await;
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("zao", "http://127.0.0.1:9".into()),
            UpstreamHostConfig::native("sobo", reachable),
        ]);
        let selected = SelectedRemoteSession {
            display: None,
            viewer_target: LinuxViewerStatus::Running,
            participation: Participation::Starter,
            route: SelectedRemoteRoute::Native {
                device_public_key: registry.hosts[0].device_public_key.clone().unwrap(),
            },
            launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            viewer: Some(SelectedViewer {
                moonlight_address: "zao:47989".into(),
                confirmed_live: true,
                reach: None,
            }),
        };
        registry.set_selected(selected.clone());

        assert!(matches!(
            registry.prepare_stream("two", Some("sobo")).await,
            Err(UpstreamError::Unreachable(_))
        ));
        assert_eq!(registry.selected().as_ref(), Some(&selected));
    }

    #[tokio::test]
    async fn selected_peer_disappearance_does_not_retire_control() {
        let a = native_server("A", "one").await;
        let b = native_server("B", "two").await;
        let mut registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("A", a),
            UpstreamHostConfig::native("B", b),
        ]);
        let prepared = registry.prepare_stream("one", Some("A")).await.unwrap();
        // A configuration change is not verified terminal revocation.
        registry.hosts.retain(|host| host.label == "B");
        assert!(matches!(registry.session_status().await,
            Err(UpstreamError::Wire(message)) if message == "selected native peer disappeared"));
        assert!(matches!(registry.prepare_stream("two", Some("B")).await,
            Err(UpstreamError::Wire(message)) if message == "selected native peer disappeared"));
        assert!(matches!(
            registry.session_stop(Some("wrong-launch"), false).await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert!(
            matches!(registry.session_stop(Some(&prepared.session.launch_id), false).await,
            Err(UpstreamError::Wire(message)) if message == "selected native peer disappeared")
        );
    }

    #[tokio::test]
    async fn selected_route_rejects_a_replacement_launch_identity() {
        let native_url = native_server("zao", "neverball").await;
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)]);
        registry.prepare_stream("neverball", None).await.unwrap();
        registry
            .selected_remote_session
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .launch_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();

        assert!(matches!(
            registry.session_status().await,
            Err(UpstreamError::SelectedRemoteSessionReplaced)
        ));
        assert_eq!(
            registry
                .selected()
                .as_ref()
                .map(|selected| selected.launch_id.as_str()),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[tokio::test]
    async fn streamed_launch_end_stops_the_exact_live_viewer_and_clears_the_route() {
        let (native_url, state) = exact_native_server(false).await;
        NativeClient::new(native_url.clone())
            .prepare_stream("game", None)
            .await
            .unwrap();
        let viewer = TestLinuxStreamViewer::default();
        viewer.seed(state.launch_id.clone(), LinuxViewerStatus::Running);
        let registry = selected_viewer_registry(
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)])
                .with_linux_viewer(Arc::new(viewer.clone())),
            &state.launch_id,
        )
        .await;
        // The player quits the game on the holder; its status reports no session.
        *state.active_phase.lock().unwrap() = None;

        assert!(matches!(
            registry.session_status().await.unwrap(),
            UpstreamSessionStatus::SessionStatus { active: None }
        ));
        assert_eq!(
            viewer
                .calls()
                .into_iter()
                .map(|call| (call.action, call.launch_id))
                .collect::<Vec<_>>(),
            vec![
                (LinuxViewerAction::Status, state.launch_id.clone()),
                (LinuxViewerAction::Stop, state.launch_id.clone()),
            ]
        );
        assert!(registry.selected().is_none());
    }

    #[tokio::test]
    async fn terminal_selected_route_runs_full_native_recovery() {
        let zao = native_server("zao", "one").await;
        let sobo = native_server("sobo", "two").await;
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("zao", zao.clone()),
            UpstreamHostConfig::native("sobo", sobo.clone()),
        ]);
        let first = registry.prepare_stream("one", Some("zao")).await.unwrap();
        NativeClient::new(zao)
            .session_stop(&first.session.launch_id, false)
            .await
            .unwrap();
        let second = NativeClient::new(sobo)
            .prepare_stream("two", None)
            .await
            .unwrap();

        // The selected holder reports that the exact launch ended: recovery
        // stops this device's viewer itself instead of waiting for an exact
        // stop request (#292), and never adopts the other peer's new launch.
        assert!(matches!(
            registry.session_status().await.unwrap(),
            UpstreamSessionStatus::SessionStatus { active: None }
        ));
        assert!(registry.selected().is_none());

        assert!(matches!(
            registry.session_status().await.unwrap(),
            UpstreamSessionStatus::SessionStatus { active: None }
        ));
        assert!(registry.selected().is_none());
        assert_ne!(first.session.launch_id, second.session.launch_id);
    }

    #[tokio::test]
    async fn selected_route_survives_a_transient_status_failure() {
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native(
            "zao",
            "http://127.0.0.1:9".into(),
        )]);
        let selected = SelectedRemoteSession {
            display: None,
            viewer_target: LinuxViewerStatus::Running,
            participation: Participation::Starter,
            route: SelectedRemoteRoute::Native {
                device_public_key: registry.hosts[0].device_public_key.clone().unwrap(),
            },
            launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            viewer: Some(SelectedViewer {
                moonlight_address: "zao:47989".into(),
                confirmed_live: true,
                reach: None,
            }),
        };
        *registry.selected_remote_session.lock().unwrap() = Some(selected.clone());

        assert!(matches!(
            registry.session_status().await,
            Err(UpstreamError::Unreachable(_))
        ));
        assert_eq!(
            registry.selected_remote_session.lock().unwrap().as_ref(),
            Some(&selected)
        );
    }

    #[tokio::test]
    async fn idle_registry_does_not_adopt_any_of_several_live_peers() {
        let zao = native_server("zao", "one").await;
        let sobo = native_server("sobo", "two").await;
        NativeClient::new(zao.clone())
            .prepare_stream("one", None)
            .await
            .unwrap();
        NativeClient::new(sobo.clone())
            .prepare_stream("two", None)
            .await
            .unwrap();
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("zao", zao),
            UpstreamHostConfig::native("sobo", sobo),
        ]);

        assert!(matches!(
            registry.session_status().await.unwrap(),
            UpstreamSessionStatus::SessionStatus { active: None }
        ));
        assert!(registry.selected_remote_session.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn returns_an_error_when_every_configured_host_is_unreachable() {
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("zao", "http://127.0.0.1:9".into()),
            UpstreamHostConfig::native("sobo", "http://127.0.0.1:9".into()),
        ]);

        assert!(matches!(
            registry.catalog_snapshot().await,
            Err(UpstreamError::Unreachable(_))
        ));
    }

    #[tokio::test]
    async fn preserves_the_single_upstream_error_outcome() {
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native(
            "zao",
            "http://127.0.0.1:9".into(),
        )]);

        assert!(matches!(
            registry.catalog_snapshot().await,
            Err(UpstreamError::Unreachable(_))
        ));
    }
}
