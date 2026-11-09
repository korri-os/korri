//! ADR 0054: plugin administration on a device without the root plugin host.
//!
//! On Linux, korrid forwards plugin requests to `korri-plugin serve-admin`.
//! The Android app has no such host, so korrid answers the same request set
//! itself, for declaration-only releases only, and installs through
//! `nixfree_install`. Responses use the existing `plugin_admin` wire types so
//! every client (CLI, surfaces, peers) works unchanged.
//!
//! Differences from the Linux host, all narrower:
//! - Only catalog records for `aarch64-android` or `any` are offered.
//! - A release's approval digest is its catalog `archive_sha256`: an Android
//!   release has no ports, units or native files to approve, so the archive
//!   identity is the whole of what the person reviews.
//! - Publishers are bound per namespace in app storage (ADR 0044).

use crate::nixfree_install::{self, AndroidPluginStore, ReleaseRef};
use crate::plugin_admin::*;
use crate::RpcFailure;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const ANDROID_PLATFORMS: [&str; 2] = ["aarch64-android", "any"];

/// The catalog fields this backend reads (plugin-host `catalog.rs`).
#[derive(Clone, Debug, Deserialize)]
pub struct CatalogRecord {
    pub plugin_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub release_version: String,
    pub platform: String,
    pub store_path: String,
    pub archive_url: String,
    pub archive_sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Catalog {
    pub records: Vec<CatalogRecord>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    publishers: BTreeMap<String, String>,
    repositories: Vec<String>,
    installs: Vec<PluginRepositoryInstall>,
}

/// Fetches bytes. Real devices use HTTPS; tests use a closure.
pub type Fetch = dyn Fn(&str, u64) -> Result<Vec<u8>, String> + Send + Sync;

pub struct AndroidPluginAdmin {
    root: PathBuf,
    store: &'static AndroidPluginStore,
    official: Option<String>,
    fetch: Box<Fetch>,
}

fn refused(category: PluginRefusalCategory, message: impl Into<String>) -> RpcFailure {
    let failure = PluginRefusal::new(category, message).into_failure();
    RpcFailure {
        code: failure.code,
        message: failure.message,
    }
}
fn other(message: impl Into<String>) -> RpcFailure {
    refused(PluginRefusalCategory::Other, message)
}

pub fn https_fetch(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    if !url.starts_with("https://") {
        return Err("plugin downloads must use HTTPS".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(url).send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download failed: HTTP {}", response.status()));
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("download exceeds its size bound".into());
    }
    Ok(bytes)
}

impl AndroidPluginAdmin {
    pub fn new(
        private_root: &Path,
        store: &'static AndroidPluginStore,
        official: Option<String>,
        fetch: Box<Fetch>,
    ) -> Self {
        Self {
            root: private_root.join("plugins"),
            store,
            official,
            fetch,
        }
    }

    fn state_path(&self) -> PathBuf {
        self.root.join("admin.json")
    }
    fn state(&self) -> Result<State, RpcFailure> {
        match fs::read(self.state_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| other(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(other(e.to_string())),
        }
    }
    fn save(&self, state: &State) -> Result<(), RpcFailure> {
        fs::create_dir_all(&self.root).map_err(|e| other(e.to_string()))?;
        let temporary = self.root.join(".admin.json.tmp");
        fs::write(
            &temporary,
            serde_json::to_vec(state).map_err(|e| other(e.to_string()))?,
        )
        .map_err(|e| other(e.to_string()))?;
        fs::rename(&temporary, self.state_path()).map_err(|e| other(e.to_string()))
    }

    fn known_url(&self, state: &State, url: &str) -> Result<(), RpcFailure> {
        if self.official.as_deref() == Some(url) || state.repositories.iter().any(|r| r == url) {
            Ok(())
        } else {
            Err(other("add this repository before using it"))
        }
    }

    fn catalog(&self, url: &str) -> Result<Vec<CatalogRecord>, RpcFailure> {
        let bytes = (self.fetch)(url, MAX_CATALOG_BYTES).map_err(other)?;
        let catalog: Catalog =
            serde_json::from_slice(&bytes).map_err(|e| other(format!("invalid catalog: {e}")))?;
        Ok(catalog
            .records
            .into_iter()
            .filter(|r| ANDROID_PLATFORMS.contains(&r.platform.as_str()))
            .collect())
    }

    /// One record per plugin and release: `aarch64-android` wins over `any`.
    fn record(&self, url: &str, id: &str, release: &str) -> Result<CatalogRecord, RpcFailure> {
        let mut records: Vec<_> = self
            .catalog(url)?
            .into_iter()
            .filter(|r| r.plugin_id == id && r.release_version == release)
            .collect();
        records.sort_by_key(|r| r.platform != "aarch64-android");
        records
            .into_iter()
            .next()
            .ok_or_else(|| other("the repository has no Android release of this plugin"))
    }

    fn publisher(&self, state: &State, plugin_id: &str) -> PluginPublisher {
        let namespace = plugin_id
            .split_once(':')
            .map_or(plugin_id, |(ns, _)| ns)
            .to_owned();
        match state.publishers.get(&namespace) {
            Some(key) => PluginPublisher {
                namespace,
                state: PluginPublisherState::Bound,
                fingerprint: fingerprint(key),
                previous_fingerprint: None,
                disclosure: None,
                public_key: Some(key.clone()),
            },
            None => PluginPublisher {
                namespace,
                state: PluginPublisherState::New,
                fingerprint: None,
                previous_fingerprint: None,
                disclosure: None,
                public_key: None,
            },
        }
    }

    /// Blocking: call from `spawn_blocking`.
    pub fn handle(&self, request: PluginAdminRequest) -> Result<PluginAdminResponse, RpcFailure> {
        let mut state = self.state()?;
        Ok(match request {
            PluginAdminRequest::Publishers(_) => {
                let enabled = self.store.read().map_err(other)?;
                PluginAdminResponse::Publishers(PluginPublishers {
                    entries: state
                        .publishers
                        .iter()
                        .map(|(namespace, key)| PluginPublisherEntry {
                            namespace: namespace.clone(),
                            fingerprint: fingerprint(key).unwrap_or_default(),
                            state: PluginPublisherState::Bound,
                            installed: enabled
                                .iter()
                                .filter(|p| p.id.starts_with(&format!("{namespace}:")))
                                .map(|p| p.id.clone())
                                .collect(),
                        })
                        .collect(),
                })
            }
            PluginAdminRequest::TrustPublisher(request) => {
                nixfree_install::public_key_bytes(&request.public_key)
                    .map_err(|_| other("publisher needs a full Nix public key"))?;
                match state.publishers.get(&request.namespace) {
                    Some(existing) if existing != &request.public_key => {
                        return Err(refused(
                            PluginRefusalCategory::PublisherKeyChanged,
                            "this namespace is already bound to another key",
                        ))
                    }
                    _ => {}
                }
                state
                    .publishers
                    .insert(request.namespace, request.public_key);
                self.save(&state)?;
                PluginAdminResponse::Applied(PluginChangeApplied {})
            }
            PluginAdminRequest::RevokePublisher(request) => {
                state.publishers.remove(&request.namespace);
                self.save(&state)?;
                PluginAdminResponse::Applied(PluginChangeApplied {})
            }
            PluginAdminRequest::Repositories(_) => {
                PluginAdminResponse::Repositories(PluginRepositories {
                    official: self.official.clone(),
                    user_added: state.repositories.clone(),
                    repository_installs: state.installs.clone(),
                    other_installs: Vec::new(),
                })
            }
            PluginAdminRequest::RepositoryAdd(PluginRepositoryRequest { url }) => {
                if !url.starts_with("https://") {
                    return Err(refused(
                        PluginRefusalCategory::Other,
                        "repository URLs must use HTTPS",
                    ));
                }
                let result =
                    if state.repositories.contains(&url) || self.official.as_ref() == Some(&url) {
                        PluginRepositoryAddResult::AlreadyPresent
                    } else {
                        state.repositories.push(url);
                        self.save(&state)?;
                        PluginRepositoryAddResult::Added
                    };
                PluginAdminResponse::RepositoryAdded(result)
            }
            PluginAdminRequest::RepositoryRemove(PluginRepositoryRequest { url }) => {
                let before = state.repositories.len();
                state.repositories.retain(|r| r != &url);
                let result = if state.repositories.len() == before {
                    PluginRepositoryRemoveResult::NotFound
                } else {
                    self.save(&state)?;
                    PluginRepositoryRemoveResult::Removed
                };
                PluginAdminResponse::RepositoryRemoved(result)
            }
            PluginAdminRequest::Catalog(PluginRepositoryRequest { url }) => {
                self.known_url(&state, &url)?;
                let enabled = self.store.read().map_err(other)?;
                let mut seen = std::collections::BTreeSet::new();
                let mut records = self.catalog(&url)?;
                records.sort_by_key(|r| r.platform != "aarch64-android");
                let entries =
                    records
                        .into_iter()
                        .filter(|r| seen.insert((r.plugin_id.clone(), r.release_version.clone())))
                        .map(|r| PluginCatalogEntry {
                            installed: state.installs.iter().find(|i| i.id == r.plugin_id).map(
                                |i| {
                                    if i.enabled && enabled.iter().any(|p| p.id == i.id) {
                                        PluginInstalledState::Enabled
                                    } else {
                                        PluginInstalledState::Disabled
                                    }
                                },
                            ),
                            publisher: self.publisher(&state, &r.plugin_id),
                            plugin_id: r.plugin_id,
                            title: r.title,
                            description: r.description,
                            release_version: r.release_version,
                        })
                        .collect();
                PluginAdminResponse::Catalog(PluginCatalog { entries })
            }
            PluginAdminRequest::Inspect(request) => {
                self.known_url(&state, &request.url)?;
                let record =
                    self.record(&request.url, &request.plugin_id, &request.release_version)?;
                PluginAdminResponse::Inspection(PluginInspection {
                    publisher: self.publisher(&state, &record.plugin_id),
                    id: record.plugin_id,
                    title: record.title,
                    approval: record.archive_sha256,
                    warning: "Runs plugin declarations only. Android plugins bring no programs, services or open ports.".into(),
                    ports: PluginPorts { allowed_tcp_ports: vec![], allowed_udp_ports: vec![] },
                    native_units: vec![],
                    brings: vec![],
                })
            }
            PluginAdminRequest::Install(request) => {
                self.known_url(&state, &request.url)?;
                let record =
                    self.record(&request.url, &request.plugin_id, &request.release_version)?;
                if request.approval != record.archive_sha256 {
                    return Err(refused(
                        PluginRefusalCategory::NeedsReview,
                        "the release changed since it was reviewed",
                    ));
                }
                let namespace = record.plugin_id.split_once(':').map_or("", |(ns, _)| ns);
                let key = state.publishers.get(namespace).cloned().ok_or_else(|| {
                    refused(
                        PluginRefusalCategory::PublisherNotTrusted,
                        "trust this publisher before installing",
                    )
                })?;
                let archive = (self.fetch)(&record.archive_url, nixfree_install::MAX_ARCHIVE_BYTES)
                    .map_err(other)?;
                let release = ReleaseRef {
                    plugin_id: &record.plugin_id,
                    store_path: &record.store_path,
                    archive_sha256: &record.archive_sha256,
                };
                self.store
                    .install(&archive, &release, &key)
                    .map_err(|e| other(e.to_string()))?;
                state.installs.retain(|i| i.id != record.plugin_id);
                state.installs.push(PluginRepositoryInstall {
                    id: record.plugin_id,
                    source_url: request.url,
                    release_version: record.release_version,
                    enabled: true,
                });
                self.save(&state)?;
                PluginAdminResponse::Applied(PluginChangeApplied {})
            }
            PluginAdminRequest::SetEnabled(request) => {
                let install = state
                    .installs
                    .iter_mut()
                    .find(|i| i.id == request.id)
                    .ok_or_else(|| other("this plugin is not installed"))?;
                install.enabled = request.enabled;
                self.store
                    .set_enabled(&request.id, request.enabled)
                    .map_err(|e| other(e.to_string()))?;
                self.save(&state)?;
                PluginAdminResponse::Applied(PluginChangeApplied {})
            }
            PluginAdminRequest::InspectArchive(_) | PluginAdminRequest::InstallArchive(_) => {
                return Err(other(
                    "sideloaded archives are not supported on Android yet",
                ))
            }
        })
    }
}

/// The plugin host's display form (plugin-host `trust.rs` `fingerprint`).
fn fingerprint(key: &str) -> Option<String> {
    let (label, encoded) = key.split_once(':')?;
    let head = encoded.as_bytes().get(..16)?;
    Some(format!(
        "{label} {}",
        head.chunks(4)
            .map(|part| std::str::from_utf8(part).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

#[cfg(test)]
#[path = "android_plugin_admin_tests.rs"]
mod tests;
