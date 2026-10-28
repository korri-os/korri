//! Integration seam for the approved 2026-10-28 admin contract.
//!
//! The host worker owns the producer and is implementing it in parallel. These
//! exact wire fixtures let this branch build against main until integration.
//! INTEGRATOR: replace this module with reexports of korrid's shared wire types
//! after the host branch lands. Do not retain two maintained wire definitions.
pub use korrid::plugin_admin::{
    PluginChangeApplied, PluginInspectRequest, PluginInstallRequest, PluginInstalledState,
    PluginPorts, PluginRepositories, PluginRepositoriesRequest, PluginRepositoryAddResult,
    PluginRepositoryRemoveResult, PluginRepositoryRequest, PluginSetEnabledRequest,
    CODE_NEEDS_REVIEW,
};
use korrid::RpcFailure;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum Outcome<T> {
    Ok(T),
    Err(RpcFailure),
}

// Names follow the existing shared outcomes so integration replaces fixtures,
// not command behavior.
pub type PluginRepositoriesOutcome = Outcome<PluginRepositories>;
pub type PluginRepositoryAddOutcome = Outcome<PluginRepositoryAddResult>;
pub type PluginRepositoryRemoveOutcome = Outcome<PluginRepositoryRemoveResult>;
pub type PluginCatalogOutcome = Outcome<PluginCatalog>;
pub type PluginInspectOutcome = Outcome<PluginInspection>;
pub type PluginChangeOutcome = Outcome<PluginChangeApplied>;
pub type PluginPublishersOutcome = Outcome<PluginPublishers>;
pub type PluginArchiveBeginOutcome = Outcome<PluginArchiveUpload>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum PluginPublisherState {
    Bound,
    Trusted,
    New,
    KeyChanged,
    Unsigned,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginPublisher {
    pub namespace: String,
    pub state: PluginPublisherState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disclosure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCatalogEntry {
    pub plugin_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub release_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<PluginInstalledState>,
    pub publisher: PluginPublisher,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCatalog {
    pub entries: Vec<PluginCatalogEntry>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginInspection {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub approval: String,
    pub warning: String,
    pub ports: PluginPorts,
    pub native_units: Vec<String>,
    pub brings: Vec<PluginDependency>,
    pub publisher: PluginPublisher,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDependency {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub warning: String,
    pub already_approved: bool,
    pub publisher: PluginPublisher,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPublishers {
    pub entries: Vec<PluginPublisherEntry>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPublisherEntry {
    pub namespace: String,
    pub fingerprint: String,
    pub state: PluginPublisherState,
    pub installed: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PluginPublishersRequest {}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTrustPublisherRequest {
    pub namespace: String,
    pub public_key: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PluginRevokePublisherRequest {
    pub namespace: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PluginArchiveBeginRequest {
    pub sha256: String,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginArchiveChunkRequest {
    pub upload_id: String,
    pub offset: u64,
    pub data: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginArchiveUploadRequest {
    pub upload_id: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginArchiveInstallRequest {
    pub upload_id: String,
    pub approval: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginArchiveUpload {
    pub upload_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum RpcRequest {
    #[serde(rename = "system.plugins.repositories")]
    PluginRepositories(PluginRepositoriesRequest),
    #[serde(rename = "system.plugins.repository.add")]
    PluginRepositoryAdd(PluginRepositoryRequest),
    #[serde(rename = "system.plugins.repository.remove")]
    PluginRepositoryRemove(PluginRepositoryRequest),
    #[serde(rename = "system.plugins.catalog")]
    PluginCatalog(PluginRepositoryRequest),
    #[serde(rename = "system.plugins.inspect")]
    PluginInspect(PluginInspectRequest),
    #[serde(rename = "system.plugins.install")]
    PluginInstall(PluginInstallRequest),
    #[serde(rename = "system.plugins.setEnabled")]
    PluginSetEnabled(PluginSetEnabledRequest),
    #[serde(rename = "system.plugins.publishers")]
    PluginPublishers(PluginPublishersRequest),
    #[serde(rename = "system.plugins.publisher.trust")]
    PluginTrustPublisher(PluginTrustPublisherRequest),
    #[serde(rename = "system.plugins.publisher.revoke")]
    PluginRevokePublisher(PluginRevokePublisherRequest),
    #[serde(rename = "system.plugins.archive.begin")]
    PluginArchiveBegin(PluginArchiveBeginRequest),
    #[serde(rename = "system.plugins.archive.chunk")]
    PluginArchiveChunk(PluginArchiveChunkRequest),
    #[serde(rename = "system.plugins.archive.finish")]
    PluginArchiveFinish(PluginArchiveUploadRequest),
    #[serde(rename = "system.plugins.archive.inspect")]
    PluginArchiveInspect(PluginArchiveUploadRequest),
    #[serde(rename = "system.plugins.archive.install")]
    PluginArchiveInstall(PluginArchiveInstallRequest),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "outcome")]
pub enum RpcResponse {
    #[serde(rename = "system.plugins.repositories")]
    PluginRepositories(Outcome<PluginRepositories>),
    #[serde(rename = "system.plugins.repository.add")]
    PluginRepositoryAdd(Outcome<PluginRepositoryAddResult>),
    #[serde(rename = "system.plugins.repository.remove")]
    PluginRepositoryRemove(Outcome<PluginRepositoryRemoveResult>),
    #[serde(rename = "system.plugins.catalog")]
    PluginCatalog(Outcome<PluginCatalog>),
    #[serde(rename = "system.plugins.inspect")]
    PluginInspect(Outcome<PluginInspection>),
    #[serde(rename = "system.plugins.install")]
    PluginInstall(Outcome<PluginChangeApplied>),
    #[serde(rename = "system.plugins.setEnabled")]
    PluginSetEnabled(Outcome<PluginChangeApplied>),
    #[serde(rename = "system.plugins.publishers")]
    PluginPublishers(Outcome<PluginPublishers>),
    #[serde(rename = "system.plugins.publisher.trust")]
    PluginTrustPublisher(Outcome<PluginChangeApplied>),
    #[serde(rename = "system.plugins.publisher.revoke")]
    PluginRevokePublisher(Outcome<PluginChangeApplied>),
    #[serde(rename = "system.plugins.archive.begin")]
    PluginArchiveBegin(Outcome<PluginArchiveUpload>),
    #[serde(rename = "system.plugins.archive.chunk")]
    PluginArchiveChunk(Outcome<PluginChangeApplied>),
    #[serde(rename = "system.plugins.archive.finish")]
    PluginArchiveFinish(Outcome<PluginChangeApplied>),
    #[serde(rename = "system.plugins.archive.inspect")]
    PluginArchiveInspect(Outcome<PluginInspection>),
    #[serde(rename = "system.plugins.archive.install")]
    PluginArchiveInstall(Outcome<PluginChangeApplied>),
}
impl RpcResponse {
    pub fn failure(&self) -> Option<RpcFailure> {
        let value = serde_json::to_value(self).ok()?;
        (value["outcome"]["_tag"] == "Err")
            .then(|| serde_json::from_value(value["outcome"]["payload"].clone()).ok())
            .flatten()
    }
}
