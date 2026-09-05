//! Ordered host registry: selects legacy or native transport per host.

use crate::{
    peer_rpc::PeerCredentials,
    upstream::{UpstreamClient, UpstreamConfig, UpstreamSessionStatus, UpstreamSessionStop},
    upstream_native::{NativeClient, NATIVE_RPC_TIMEOUT},
    CatalogHostFailure, CatalogSnapshot, Game, GameSource, MoonlightCertificateProvisioned,
    MoonlightCertificateRevoked, SessionFreezeResult, SessionPrepared, SourceStatus,
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
#[cfg(any(target_os = "android", test))]
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
    #[error("more than one native peer reports an active session")]
    AmbiguousActiveSessions,
    #[error("the selected remote session was replaced by a different launch")]
    SelectedRemoteSessionReplaced,
    #[error("native peer session recovery could not prove one active route")]
    NativeSessionRecoveryIncomplete,
    #[error("a remote peer already has an active session")]
    ActiveRemoteSessionConflict,
    #[error("expectedLaunchId is required for exact remote peer stop")]
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
            Self::AmbiguousActiveSessions => "AmbiguousActiveSessions",
            Self::SelectedRemoteSessionReplaced => "SelectedRemoteSessionReplaced",
            Self::NativeSessionRecoveryIncomplete => "NativeSessionRecoveryIncomplete",
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
        let mut device_public_key = hex::encode(label.as_bytes());
        device_public_key.push_str(&"0".repeat(64));
        device_public_key.truncate(64);
        Self {
            label,
            kind: UpstreamKind::Native,
            base_url,
            moonlight_address: None,
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
    route: SelectedRemoteRoute,
    launch_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FreezerVerb {
    Freeze,
    Thaw,
}

impl FreezerVerb {
    fn name(self) -> &'static str {
        match self {
            Self::Freeze => "freeze",
            Self::Thaw => "thaw",
        }
    }
}

#[derive(Clone)]
pub struct UpstreamRegistry {
    hosts: Vec<RegisteredHost>,
    configuration_error: Option<String>,
    deferred_file_config: Option<DeferredFileConfig>,
    selected_remote_session: Arc<Mutex<Option<SelectedRemoteSession>>>,
    remote_prepare_mutation: Arc<tokio::sync::Mutex<()>>,
}

impl UpstreamRegistry {
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
                        #[cfg(test)]
                        if credentials.is_none() {
                            return Some(RegisteredHost {
                                label,
                                moonlight_address,
                                device_public_key,
                                client: RegisteredClient::Native(NativeClient::new(
                                    config.base_url,
                                )),
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
                            config.base_url,
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
            hosts,
            configuration_error,
            deferred_file_config: None,
            selected_remote_session: Arc::new(Mutex::new(None)),
            remote_prepare_mutation: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn new_secure(configs: Vec<UpstreamHostConfig>, credentials: PeerCredentials) -> Self {
        Self::build(configs, Some(credentials))
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
        Self::from_file_or_default(
            path,
            UpstreamHostConfig::legacy(
                std::env::var("KORRID_UPSTREAM_LABEL").unwrap_or_else(|_| "aka".into()),
                UpstreamConfig::from_env().base_url,
            ),
        )
    }

    pub fn from_env_or_file(path: &Path, credentials: PeerCredentials) -> Self {
        if let Ok(json) = std::env::var("KORRID_UPSTREAMS") {
            return Self::from_json_secure("KORRID_UPSTREAMS", &json, credentials);
        }
        Self::from_file_or_empty_secure(path, credentials)
    }

    fn from_file_or_empty_secure(path: &Path, credentials: PeerCredentials) -> Self {
        match Self::from_file_inner(path, Some(credentials.clone())) {
            Ok(registry) => registry,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self {
                hosts: vec![],
                configuration_error: None,
                deferred_file_config: Some(DeferredFileConfig {
                    path: path.to_owned(),
                    credentials: Some(credentials),
                    resolved: Arc::new(OnceLock::new()),
                }),
                selected_remote_session: Arc::new(Mutex::new(None)),
                remote_prepare_mutation: Arc::new(tokio::sync::Mutex::new(())),
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
            hosts: vec![],
            configuration_error: Some(message),
            deferred_file_config: None,
            selected_remote_session: Arc::new(Mutex::new(None)),
            remote_prepare_mutation: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn resolved(&self) -> Cow<'_, Self> {
        let Some(config) = &self.deferred_file_config else {
            return Cow::Borrowed(self);
        };
        if let Some(registry) = config.resolved.get() {
            return Cow::Borrowed(registry);
        }
        match Self::from_file_inner(&config.path, config.credentials.clone()) {
            Ok(registry) if registry.configuration_error.is_none() => {
                Cow::Borrowed(config.resolved.get_or_init(|| registry))
            }
            Ok(registry) => Cow::Owned(registry),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Cow::Borrowed(self),
            Err(error) => Cow::Owned(Self::invalid_file_read(&config.path, error)),
        }
    }

    pub fn moonlight_host_candidates(&self) -> Result<Vec<MoonlightHostCandidate>, UpstreamError> {
        let registry = self.resolved();
        if let Some(error) = &registry.configuration_error {
            return Err(UpstreamError::Wire(error.clone()));
        }
        let candidates = registry
            .hosts
            .iter()
            .filter_map(|host| match &host.client {
                RegisteredClient::Native(_) => host
                    .moonlight_address
                    .as_deref()
                    .map(str::trim)
                    .filter(|address| !address.is_empty())
                    .map(|address| MoonlightHostCandidate {
                        label: host.label.clone(),
                        address: address.into(),
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
        let registry = self.resolved();
        registry.catalog_snapshot_resolved().await
    }

    async fn catalog_snapshot_resolved(&self) -> Result<CatalogSnapshot, UpstreamError> {
        if let Some(error) = &self.configuration_error {
            return Err(UpstreamError::Wire(error.clone()));
        }
        let qualify_legacy_host = self.hosts.len() > 1;
        let results = join_all(self.hosts.iter().map(|host| async move {
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
        let registry = self.resolved();
        let native: Vec<NativeClient> = registry
            .hosts
            .iter()
            .filter_map(|host| match &host.client {
                RegisteredClient::Native(client) => Some(client.clone()),
                RegisteredClient::Legacy(_) => None,
            })
            .collect();
        let results = join_all(
            native
                .iter()
                .map(|client| client.moonlight_certificate_attest(host_uuid)),
        )
        .await;
        let mut matched = Vec::new();
        for (client, result) in native.into_iter().zip(results) {
            if result?.matched {
                matched.push(client);
            }
        }
        match matched.len() {
            0 => Err(UpstreamError::MoonlightHostNotFound),
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
    ) -> Result<SessionPrepared, UpstreamError> {
        let registry = self.resolved();
        registry.prepare_stream_resolved(game_id, host_label).await
    }

    async fn prepare_stream_resolved(
        &self,
        game_id: &str,
        host_label: Option<&str>,
    ) -> Result<SessionPrepared, UpstreamError> {
        let host = match host_label {
            Some(label) => self
                .hosts
                .iter()
                .find(|host| host.label == label)
                .ok_or_else(|| {
                    UpstreamError::Failure(format!("upstream host {label:?} is not configured"))
                })?,
            None if self.hosts.len() == 1 => &self.hosts[0],
            None if self.hosts.is_empty() => {
                return Err(UpstreamError::Failure(
                    "no upstream hosts configured".into(),
                ))
            }
            None => {
                return Err(UpstreamError::Failure(
                    "host is required when multiple upstreams are configured".into(),
                ))
            }
        };
        let _mutation = self.remote_prepare_mutation.lock().await;
        let direct_legacy = self.is_legacy_only();
        if !direct_legacy {
            self.ensure_remote_prepare_available().await?;
        }
        let prepared = match &host.client {
            RegisteredClient::Legacy(client) => {
                let prepared = client.prepare_stream(game_id).await?;
                let prepared = SessionPrepared {
                    game_id: prepared.game_id,
                    launch_id: prepared.session_id,
                };
                self.set_selected(SelectedRemoteSession {
                    route: SelectedRemoteRoute::Legacy {
                        label: host.label.clone(),
                    },
                    launch_id: prepared.launch_id.clone(),
                });
                prepared
            }
            RegisteredClient::Native(client) => {
                let prepared = client.prepare_stream(game_id).await?;
                let route_key = host.native_route_key().ok_or_else(|| {
                    UpstreamError::Wire("native peer has no stable device identity".into())
                })?;
                self.set_selected(SelectedRemoteSession {
                    route: SelectedRemoteRoute::Native {
                        device_public_key: route_key,
                    },
                    launch_id: prepared.launch_id.clone(),
                });
                prepared
            }
        };
        Ok(prepared)
    }

    async fn ensure_remote_prepare_available(&self) -> Result<(), UpstreamError> {
        if let Some(selected) = self.selected() {
            match self.selected_status(&selected).await? {
                UpstreamSessionStatus::SessionStatus {
                    active: Some(active),
                } => {
                    return if active.launch_id == selected.launch_id {
                        Err(UpstreamError::ActiveRemoteSessionConflict)
                    } else {
                        Err(UpstreamError::SelectedRemoteSessionReplaced)
                    }
                }
                UpstreamSessionStatus::SessionStatus { active: None } => {
                    self.clear_selected_if(&selected);
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
        if self.recover_native_session().await?.is_some() {
            return Err(UpstreamError::ActiveRemoteSessionConflict);
        }
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

    pub async fn session_status(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        let registry = self.resolved();
        registry.session_status_resolved().await
    }

    async fn session_status_resolved(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        if let Some(selected) = self.selected() {
            let status = self.selected_status(&selected).await?;
            match &status {
                UpstreamSessionStatus::SessionStatus {
                    active: Some(active),
                } => {
                    if active.launch_id != selected.launch_id {
                        return Err(UpstreamError::SelectedRemoteSessionReplaced);
                    }
                    return Ok(status);
                }
                UpstreamSessionStatus::SessionStatus { active: None } => {
                    self.clear_selected_if(&selected);
                }
                UpstreamSessionStatus::SessiondNotConfigured {}
                | UpstreamSessionStatus::HostUnavailable {} => return Ok(status),
            }
        }

        if let Some((_selected, status)) = self.recover_native_session().await? {
            return Ok(status);
        }
        self.legacy_status_or_no_active().await
    }

    pub async fn session_stop(
        &self,
        expected_launch_id: Option<&str>,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        let registry = self.resolved();
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
                let expected = expected_launch_id.ok_or(UpstreamError::ExpectedLaunchIdRequired)?;
                if expected != selected.launch_id {
                    return Err(UpstreamError::StaleLaunchIdentity);
                }
            } else if expected_launch_id.is_some_and(|expected| expected != selected.launch_id) {
                return Err(UpstreamError::StaleLaunchIdentity);
            }
            return self.stop_selected(&selected, force).await;
        }

        if !self.has_native_hosts() {
            return self.legacy_stop_or_nothing(force).await;
        }
        if let Some((recovered, _status)) = self.recover_native_session().await? {
            let expected = expected_launch_id.ok_or(UpstreamError::ExpectedLaunchIdRequired)?;
            if expected != recovered.launch_id {
                return Err(UpstreamError::StaleLaunchIdentity);
            }
            return self.stop_selected(&recovered, force).await;
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
        let registry = self.resolved();
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
        timeout(
            CATALOG_HOST_TIMEOUT,
            client.source_status(device_public_key),
        )
        .await
        .map_err(|_| {
            UpstreamError::Unreachable(format!("host {:?} source status timed out", host.label))
        })?
    }

    pub async fn session_freeze(
        &self,
        expected_launch_id: Option<&str>,
    ) -> Result<SessionFreezeResult, UpstreamError> {
        let registry = self.resolved();
        registry
            .session_freezer_resolved(expected_launch_id, FreezerVerb::Freeze)
            .await
    }

    pub async fn session_thaw(
        &self,
        expected_launch_id: Option<&str>,
    ) -> Result<SessionFreezeResult, UpstreamError> {
        let registry = self.resolved();
        registry
            .session_freezer_resolved(expected_launch_id, FreezerVerb::Thaw)
            .await
    }

    /// Routes an exact freeze or thaw through the selected remote session.
    /// The route is selected or recovered the same way as stop. Only native
    /// peers support freezer control; a legacy route fails with a typed
    /// error and no legacy call is made.
    async fn session_freezer_resolved(
        &self,
        expected_launch_id: Option<&str>,
        verb: FreezerVerb,
    ) -> Result<SessionFreezeResult, UpstreamError> {
        let _mutation = self.remote_prepare_mutation.lock().await;
        let selected = match self.selected() {
            Some(selected) => selected,
            None => {
                if self.hosts.is_empty() {
                    return Err(UpstreamError::NoActiveSession);
                }
                if !self.has_native_hosts() {
                    return Err(UpstreamError::FreezerUnsupportedOnLegacyRoute);
                }
                match self.recover_native_session().await? {
                    Some((recovered, _status)) => recovered,
                    None => return Err(UpstreamError::NoActiveSession),
                }
            }
        };
        if let Some(expected) = expected_launch_id {
            if expected != selected.launch_id {
                return Err(UpstreamError::StaleLaunchIdentity);
            }
        }
        let SelectedRemoteRoute::Native { device_public_key } = &selected.route else {
            return Err(UpstreamError::FreezerUnsupportedOnLegacyRoute);
        };
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
        let result = timeout(CATALOG_HOST_TIMEOUT, call).await.map_err(|_| {
            UpstreamError::Unreachable(format!(
                "host {:?} session {} timed out",
                host.label,
                verb.name()
            ))
        })?;
        match result {
            Ok(result) => Ok(result),
            Err(UpstreamError::Tagged { code, .. }) if code == "NoActiveSession" => {
                self.clear_selected_if(&selected);
                Err(UpstreamError::NoActiveSession)
            }
            Err(UpstreamError::Tagged { code, .. }) if code == "StaleLaunchIdentity" => {
                Err(UpstreamError::SelectedRemoteSessionReplaced)
            }
            Err(error) => Err(error),
        }
    }

    async fn selected_status(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Result<UpstreamSessionStatus, UpstreamError> {
        match &selected.route {
            SelectedRemoteRoute::Native { device_public_key } => {
                let host = self
                    .native_host_by_route_key(device_public_key)
                    .ok_or_else(|| {
                        UpstreamError::Wire("selected native peer disappeared".into())
                    })?;
                timeout(CATALOG_HOST_TIMEOUT, host.native_session_status())
                    .await
                    .map_err(|_| {
                        UpstreamError::Unreachable(format!(
                            "host {:?} session status timed out",
                            host.label
                        ))
                    })?
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
        }
    }

    async fn recover_native_session(
        &self,
    ) -> Result<Option<(SelectedRemoteSession, UpstreamSessionStatus)>, UpstreamError> {
        let native_hosts = self
            .hosts
            .iter()
            .filter(|host| matches!(&host.client, RegisteredClient::Native(_)))
            .collect::<Vec<_>>();
        if native_hosts.is_empty() {
            return Ok(None);
        }
        let results = join_all(native_hosts.iter().map(|host| async move {
            timeout(CATALOG_HOST_TIMEOUT, host.native_session_status())
                .await
                .map_err(|_| {
                    UpstreamError::Unreachable(format!(
                        "host {:?} session status timed out",
                        host.label
                    ))
                })?
                .map(|status| (*host, status))
        }))
        .await;
        let mut active = Vec::new();
        let mut failed = false;
        for result in results {
            match result {
                Ok((host, status)) => {
                    if let Some(session) = active_session(&status) {
                        active.push((host, session.clone(), status));
                    }
                }
                Err(_) => failed = true,
            }
        }
        if active.len() > 1 {
            return Err(UpstreamError::AmbiguousActiveSessions);
        }
        if failed {
            return Err(UpstreamError::NativeSessionRecoveryIncomplete);
        }
        let Some((host, session, status)) = active.pop() else {
            return Ok(None);
        };
        let selected = SelectedRemoteSession {
            route: SelectedRemoteRoute::Native {
                device_public_key: host.native_route_key().ok_or_else(|| {
                    UpstreamError::Wire("native peer has no stable device identity".into())
                })?,
            },
            launch_id: session.launch_id,
        };
        self.set_selected(selected.clone());
        Ok(Some((selected, status)))
    }

    async fn stop_selected(
        &self,
        selected: &SelectedRemoteSession,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
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
                let status = self.selected_status(selected).await?;
                if let Some(active) = active_session(&status) {
                    if active.launch_id != selected.launch_id {
                        return Err(UpstreamError::SelectedRemoteSessionReplaced);
                    }
                } else {
                    self.clear_selected_if(selected);
                    return Ok(UpstreamSessionStop::NothingToStop {});
                }
                timeout(
                    CATALOG_HOST_TIMEOUT,
                    client.session_stop(&selected.launch_id, force),
                )
                .await
                .map_err(|_| {
                    UpstreamError::Unreachable(format!(
                        "host {:?} session stop timed out",
                        host.label
                    ))
                })??
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
                            return Err(UpstreamError::SelectedRemoteSessionReplaced)
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

    fn selected(&self) -> Option<SelectedRemoteSession> {
        self.selected_remote_session
            .lock()
            .expect("selected remote session mutex poisoned")
            .clone()
    }

    fn set_selected(&self, selected: SelectedRemoteSession) {
        *self
            .selected_remote_session
            .lock()
            .expect("selected remote session mutex poisoned") = Some(selected);
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
            *selected = None;
        }
    }

    fn legacy_host(&self) -> Option<&RegisteredHost> {
        self.hosts
            .iter()
            .find(|host| matches!(&host.client, RegisteredClient::Legacy(_)))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::sync::Arc;

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
        let device_key = crate::identity::DeviceIdentity::load_or_create(&private)
            .unwrap()
            .device_public_key()
            .unwrap()
            .to_owned();
        let (lan, _) = crate::plain_host_routers_for_tests(runtime);
        (serve(lan).await, device_key)
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

        let missing_file = UpstreamRegistry::from_file_or_empty_secure(
            &private.path().join("missing-upstreams.json"),
            credentials,
        );
        assert!(missing_file.hosts.is_empty());
        assert!(missing_file.configuration_error.is_none());
        assert!(missing_file.deferred_file_config.is_some());
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
                {"label":"desk","kind":"native","baseUrl":"https://desk.example:443/korri","moonlightAddress":"[::1]:47989"}
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
        assert_eq!(active.launch_id, prepared.launch_id);
        assert_eq!(active.host.as_deref(), Some("aka"));
        assert!(matches!(
            registry.session_stop(Some("stale-launch"), true).await,
            Err(UpstreamError::StaleLaunchIdentity)
        ));
        assert!(matches!(
            registry
                .session_stop(Some(&prepared.launch_id), true)
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

        assert_eq!(prepared.launch_id, "legacy-session");
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
    async fn native_recovery_failure_blocks_legacy_status_and_stop_fallback() {
        let state = LegacyServerState {
            prepared: Default::default(),
        };
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_server(state.clone()).await),
            UpstreamHostConfig::native("zao", "http://127.0.0.1:9".into()),
        ]);

        assert!(matches!(
            registry.session_status().await,
            Err(UpstreamError::NativeSessionRecoveryIncomplete)
        ));
        assert!(matches!(
            registry.session_stop(None, true).await,
            Err(UpstreamError::NativeSessionRecoveryIncomplete)
        ));
        assert!(state.prepared.lock().unwrap().is_empty());
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
        assert_eq!(active.launch_id, prepared.launch_id);
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
        assert_eq!(registry.selected().unwrap().launch_id, prepared.launch_id);
        assert!(matches!(
            registry
                .session_stop(Some(&prepared.launch_id), false)
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
    async fn freeze_and_thaw_follow_the_selected_native_route() {
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
        let frozen = registry.session_freeze(None).await.unwrap();
        assert_eq!(frozen.launch_id, prepared.launch_id);
        assert_eq!(frozen.state, crate::SessionFreezerState::Frozen);
        assert!(frozen.changed);
        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = registry.session_status().await.unwrap()
        else {
            panic!("frozen session stays active")
        };
        assert_eq!(active.phase.as_deref(), Some("frozen"));
        assert_eq!(registry.selected().unwrap().launch_id, prepared.launch_id);

        // A fresh registry with no cached route recovers the frozen session.
        let recovered = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)]);
        let thawed = recovered
            .session_thaw(Some(&prepared.launch_id))
            .await
            .unwrap();
        assert_eq!(thawed.state, crate::SessionFreezerState::Running);
        assert!(thawed.changed);
        assert_eq!(recovered.selected().unwrap().launch_id, prepared.launch_id);
        let again = recovered.session_thaw(None).await.unwrap();
        assert!(!again.changed);

        assert!(matches!(
            registry
                .session_stop(Some(&prepared.launch_id), false)
                .await
                .unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
        // The other registry still holds the route; the host reports no
        // active launch, which clears it.
        assert!(matches!(
            recovered.session_freeze(None).await,
            Err(UpstreamError::NoActiveSession)
        ));
        assert!(recovered.selected().is_none());
    }

    #[tokio::test]
    async fn peer_reported_stale_identity_surfaces_as_a_replaced_route() {
        let native_url = native_server("zao", "neverball").await;
        let first =
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url.clone())]);
        let prepared = first.prepare_stream("neverball", None).await.unwrap();

        // A second brain stops the launch and starts a replacement. The
        // first brain still holds the old route.
        let second = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)]);
        assert!(matches!(
            second
                .session_stop(Some(&prepared.launch_id), false)
                .await
                .unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
        let replacement = second.prepare_stream("neverball", None).await.unwrap();
        assert_ne!(replacement.launch_id, prepared.launch_id);

        // The peer reports StaleLaunchIdentity for the old launch. The
        // first brain maps that to SelectedRemoteSessionReplaced and keeps
        // the stale route so the caller can re-read status and re-select.
        assert!(matches!(
            first.session_freeze(None).await,
            Err(UpstreamError::SelectedRemoteSessionReplaced)
        ));
        assert!(matches!(
            first.session_thaw(Some(&prepared.launch_id)).await,
            Err(UpstreamError::SelectedRemoteSessionReplaced)
        ));
        assert_eq!(first.selected().unwrap().launch_id, prepared.launch_id);
        // The replacement launch was not frozen by the stale request.
        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = second.session_status().await.unwrap()
        else {
            panic!("replacement launch must be active")
        };
        assert_eq!(active.launch_id, replacement.launch_id);
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
    async fn empty_native_route_cache_recovers_one_active_peer() {
        let native_url = native_server("zao", "neverball").await;
        let starter =
            UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url.clone())]);
        let prepared = starter.prepare_stream("neverball", None).await.unwrap();
        let recovered = UpstreamRegistry::new(vec![UpstreamHostConfig::native("zao", native_url)]);

        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = recovered.session_status().await.unwrap()
        else {
            panic!("recovery fan-out must find the active native session")
        };
        assert_eq!(active.launch_id, prepared.launch_id);
        assert_eq!(
            recovered
                .selected_remote_session
                .lock()
                .unwrap()
                .as_ref()
                .map(|selected| selected.launch_id.as_str()),
            Some(prepared.launch_id.as_str())
        );
        assert!(matches!(
            recovered
                .session_stop(Some(&prepared.launch_id), false)
                .await
                .unwrap(),
            UpstreamSessionStop::Stopped { .. }
        ));
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
        assert_eq!(registry.selected().unwrap().launch_id, first.launch_id);
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
            route: SelectedRemoteRoute::Native {
                device_public_key: registry.hosts[0].device_public_key.clone().unwrap(),
            },
            launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        };
        registry.set_selected(selected.clone());

        assert!(matches!(
            registry.prepare_stream("two", Some("sobo")).await,
            Err(UpstreamError::Unreachable(_))
        ));
        assert_eq!(registry.selected().as_ref(), Some(&selected));
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
                .selected_remote_session
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .launch_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
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
            .session_stop(&first.launch_id, false)
            .await
            .unwrap();
        let second = NativeClient::new(sobo).prepare_stream("two").await.unwrap();

        let UpstreamSessionStatus::SessionStatus {
            active: Some(active),
        } = registry.session_status().await.unwrap()
        else {
            panic!("full native recovery must find the replacement peer")
        };
        assert_eq!(active.launch_id, second.launch_id);
        assert_eq!(active.host.as_deref(), Some("sobo"));
        assert_eq!(registry.selected().unwrap().launch_id, second.launch_id);
    }

    #[tokio::test]
    async fn selected_route_survives_a_transient_status_failure() {
        let registry = UpstreamRegistry::new(vec![UpstreamHostConfig::native(
            "zao",
            "http://127.0.0.1:9".into(),
        )]);
        let selected = SelectedRemoteSession {
            route: SelectedRemoteRoute::Native {
                device_public_key: registry.hosts[0].device_public_key.clone().unwrap(),
            },
            launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
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
    async fn recovery_rejects_more_than_one_active_native_peer() {
        let zao = native_server("zao", "one").await;
        let sobo = native_server("sobo", "two").await;
        NativeClient::new(zao.clone())
            .prepare_stream("one")
            .await
            .unwrap();
        NativeClient::new(sobo.clone())
            .prepare_stream("two")
            .await
            .unwrap();
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("zao", zao),
            UpstreamHostConfig::native("sobo", sobo),
        ]);

        assert!(matches!(
            registry.session_status().await,
            Err(UpstreamError::AmbiguousActiveSessions)
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
