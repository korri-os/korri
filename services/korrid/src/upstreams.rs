//! Ordered host registry: selects legacy or native transport per host.

use crate::{
    upstream::{UpstreamClient, UpstreamConfig, UpstreamSessionStatus, UpstreamSessionStop},
    upstream_native::{NativeClient, NATIVE_RPC_TIMEOUT},
    CatalogHostFailure, CatalogSnapshot, Game, MoonlightCertificateProvisioned,
    MoonlightCertificateRevoked, SessionPrepared,
};
use futures::future::join_all;
use serde::Deserialize;
use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::time::timeout;

const CATALOG_HOST_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT: Duration =
    Duration::from_secs(NATIVE_RPC_TIMEOUT.as_secs() * 2 + 1);
#[cfg(any(target_os = "android", test))]
pub(crate) const MOONLIGHT_CERTIFICATE_CALLER_TIMEOUT: Duration =
    Duration::from_secs(MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT.as_secs() + 1);
const MAX_MOONLIGHT_HOST_CANDIDATES: usize = 16;

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
        }
    }

    pub fn native(label: impl Into<String>, base_url: String) -> Self {
        Self {
            label: label.into(),
            kind: UpstreamKind::Native,
            base_url,
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
    native_base_url: Option<String>,
    client: RegisteredClient,
}

impl RegisteredHost {
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
    resolved: Arc<OnceLock<UpstreamRegistry>>,
}

#[derive(Clone)]
pub struct UpstreamRegistry {
    hosts: Vec<RegisteredHost>,
    configuration_error: Option<String>,
    deferred_file_config: Option<DeferredFileConfig>,
}

impl UpstreamRegistry {
    pub fn new(configs: Vec<UpstreamHostConfig>) -> Self {
        Self {
            hosts: configs
                .into_iter()
                .map(|config| {
                    let native_base_url = match config.kind {
                        UpstreamKind::Native => Some(config.base_url.clone()),
                        UpstreamKind::Legacy => None,
                    };
                    RegisteredHost {
                        label: config.label,
                        native_base_url,
                        client: match config.kind {
                            UpstreamKind::Legacy => {
                                RegisteredClient::Legacy(UpstreamClient::new(UpstreamConfig {
                                    base_url: config.base_url,
                                }))
                            }
                            UpstreamKind::Native => {
                                RegisteredClient::Native(NativeClient::new(config.base_url))
                            }
                        },
                    }
                })
                .collect(),
            configuration_error: None,
            deferred_file_config: None,
        }
    }

    pub fn from_env_or_file(path: &Path) -> Self {
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

    fn from_file_or_default(path: &Path, fallback: UpstreamHostConfig) -> Self {
        match Self::from_file(path) {
            Ok(registry) => registry,
            Err(error) => {
                let deferred_file_config = DeferredFileConfig {
                    path: path.to_owned(),
                    resolved: Arc::new(OnceLock::new()),
                };
                let mut registry = if error.kind() == std::io::ErrorKind::NotFound {
                    Self::new(vec![fallback])
                } else {
                    Self::invalid_file_read(path, error)
                };
                registry.deferred_file_config = Some(deferred_file_config);
                registry
            }
        }
    }

    fn from_file(path: &Path) -> std::io::Result<Self> {
        fs::read_to_string(path).map(|json| Self::from_json(&path.display().to_string(), &json))
    }

    fn invalid_file_read(path: &Path, error: std::io::Error) -> Self {
        Self::invalid_configuration(format!(
            "could not read upstream config {}: {error}",
            path.display()
        ))
    }

    fn from_json(source: &str, json: &str) -> Self {
        match serde_json::from_str(json) {
            Ok(configs) => Self::from_configs(source, configs),
            Err(error) => {
                Self::invalid_configuration(format!("invalid upstream config {source}: {error}"))
            }
        }
    }

    fn from_configs(source: &str, configs: Vec<UpstreamHostConfig>) -> Self {
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
        Self::new(configs)
    }

    fn invalid_configuration(message: String) -> Self {
        Self {
            hosts: vec![],
            configuration_error: Some(message),
            deferred_file_config: None,
        }
    }

    fn resolved(&self) -> Cow<'_, Self> {
        let Some(config) = &self.deferred_file_config else {
            return Cow::Borrowed(self);
        };
        if let Some(registry) = config.resolved.get() {
            return Cow::Borrowed(registry);
        }
        match Self::from_file(&config.path) {
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
            .filter_map(|host| {
                host.native_base_url
                    .as_deref()
                    .map(|base_url| (host.label.as_str(), base_url))
            })
            .filter_map(|(label, base_url)| {
                let url = reqwest::Url::parse(base_url).ok()?;
                if !matches!(url.scheme(), "http" | "https") {
                    return None;
                }
                let address = url.host_str()?.trim();
                if address.is_empty() {
                    return None;
                }
                Some(MoonlightHostCandidate {
                    label: label.into(),
                    address: address.into(),
                })
            })
            .collect::<Vec<_>>();
        if candidates.len() > MAX_MOONLIGHT_HOST_CANDIDATES {
            return Err(UpstreamError::Wire(format!(
                "more than {MAX_MOONLIGHT_HOST_CANDIDATES} native Moonlight hosts are configured"
            )));
        }
        let mut addresses = BTreeSet::new();
        if candidates
            .iter()
            .any(|candidate| !addresses.insert(candidate.address.to_lowercase()))
        {
            return Err(UpstreamError::Wire(
                "duplicate native Moonlight host address is configured".into(),
            ));
        }
        Ok(candidates)
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
        match &host.client {
            RegisteredClient::Legacy(client) => {
                client
                    .prepare_stream(game_id)
                    .await
                    .map(|prepared| SessionPrepared {
                        game_id: prepared.game_id,
                        launch_id: prepared.session_id,
                    })
            }
            RegisteredClient::Native(client) => client.prepare_stream(game_id).await,
        }
    }

    pub async fn session_status(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        let registry = self.resolved();
        registry.session_status_resolved().await
    }

    async fn session_status_resolved(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        let host = self.legacy_host()?;
        let RegisteredClient::Legacy(client) = &host.client else {
            unreachable!("legacy_host returned a native client")
        };
        let mut status = client.session_status().await?;
        if self.hosts.len() > 1 {
            if let UpstreamSessionStatus::SessionStatus {
                active: Some(active),
            } = &mut status
            {
                active.host = Some(host.label.clone());
            }
        }
        Ok(status)
    }

    pub async fn session_stop(&self, force: bool) -> Result<UpstreamSessionStop, UpstreamError> {
        let registry = self.resolved();
        registry.session_stop_resolved(force).await
    }

    async fn session_stop_resolved(
        &self,
        force: bool,
    ) -> Result<UpstreamSessionStop, UpstreamError> {
        let host = self.legacy_host()?;
        let RegisteredClient::Legacy(client) = &host.client else {
            unreachable!("legacy_host returned a native client")
        };
        client.session_stop(force).await
    }

    fn legacy_host(&self) -> Result<&RegisteredHost, UpstreamError> {
        self.hosts
            .iter()
            .find(|host| matches!(&host.client, RegisteredClient::Legacy(_)))
            .ok_or_else(|| UpstreamError::Failure("no legacy session upstream configured".into()))
    }
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
            "app.session.status" => json!({
                "_tag":"SessionStatus",
                "active":{"launchId":"legacy-session","gameId":"shared"}
            }),
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

    async fn legacy_server(state: LegacyServerState) -> String {
        serve(
            Router::new()
                .route("/api/rpc", post(legacy_rpc))
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
    fn unreadable_file_config_is_retried_after_storage_access_becomes_available() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("upstreams.json");
        std::fs::write(
            &path,
            r#"[{"label":"zao","kind":"native","baseUrl":"http://100.114.19.92:39217"}]"#,
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
                address: "100.114.19.92".into(),
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
    }

    #[test]
    fn moonlight_host_candidates_use_only_configured_native_peer_addresses() {
        let registry = UpstreamRegistry::from_json(
            "test",
            r#"[
                {"label":"aka","kind":"legacy","baseUrl":"http://aka.example:3000"},
                {"label":"zao","kind":"native","baseUrl":"http://100.114.19.92:39217"},
                {"label":"desk","kind":"native","baseUrl":"https://desk.example:443/korri"}
            ]"#,
        );

        assert_eq!(
            registry.moonlight_host_candidates().unwrap(),
            vec![
                MoonlightHostCandidate {
                    label: "zao".into(),
                    address: "100.114.19.92".into(),
                },
                MoonlightHostCandidate {
                    label: "desk".into(),
                    address: "desk.example".into(),
                },
            ]
        );
    }

    #[test]
    fn moonlight_host_candidates_skip_invalid_native_peer_urls() {
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("broken", "not a URL".into()),
            UpstreamHostConfig::native("wrong-scheme", "file:///tmp/zao".into()),
            UpstreamHostConfig::native("zao", "http://100.114.19.92:39217".into()),
        ]);

        assert_eq!(
            registry.moonlight_host_candidates().unwrap(),
            vec![MoonlightHostCandidate {
                label: "zao".into(),
                address: "100.114.19.92".into(),
            }]
        );
    }

    #[test]
    fn moonlight_host_candidates_reject_more_than_sixteen_valid_hosts() {
        let registry = UpstreamRegistry::new(
            (0..17)
                .map(|index| {
                    UpstreamHostConfig::native(
                        format!("host-{index}"),
                        format!("http://192.0.2.{}:39217", index + 1),
                    )
                })
                .collect(),
        );

        assert!(matches!(
            registry.moonlight_host_candidates(),
            Err(UpstreamError::Wire(message)) if message.contains("more than 16")
        ));
    }

    #[test]
    fn moonlight_host_candidates_reject_duplicate_native_addresses() {
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::native("zao", "http://ZAO.example:39217".into()),
            UpstreamHostConfig::native("duplicate", "https://zao.example:443".into()),
        ]);

        assert!(matches!(
            registry.moonlight_host_candidates(),
            Err(UpstreamError::Wire(message)) if message.contains("duplicate")
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
        let registry = UpstreamRegistry::new(vec![
            UpstreamHostConfig::legacy("aka", legacy_url),
            UpstreamHostConfig::native("zao", native_url),
        ]);

        let catalog = registry.catalog_snapshot().await.unwrap();
        assert_eq!(catalog.games.len(), 2);
        assert_eq!(catalog.games[0].host.as_deref(), Some("aka"));
        assert_eq!(catalog.games[1].host.as_deref(), Some("zao"));
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
        registry
            .prepare_stream("shared", Some("zao"))
            .await
            .unwrap();
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
        registry.prepare_stream("shared", None).await.unwrap();
    }

    #[tokio::test]
    async fn qualifies_legacy_session_status_only_in_a_multi_host_registry() {
        let single = UpstreamRegistry::new(vec![UpstreamHostConfig::legacy(
            "aka",
            legacy_server(LegacyServerState {
                prepared: Default::default(),
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
                    prepared: Default::default(),
                })
                .await,
            ),
            UpstreamHostConfig::native("zao", "http://127.0.0.1:9".into()),
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
            UpstreamHostConfig::legacy("aka", legacy_server(state.clone()).await),
            UpstreamHostConfig::native("zao", "http://127.0.0.1:9".into()),
        ]);

        let stopped = registry.session_stop(true).await.unwrap();

        assert!(matches!(stopped, UpstreamSessionStop::Stopped { .. }));
        assert_eq!(&*state.prepared.lock().unwrap(), &["stop:true"]);
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
