//! Wire between korrid and the root plugin host's admin socket.
//!
//! korrid runs unprivileged and still cannot reach the host's state, receipts
//! or store. The host's `korri-plugin serve-admin` accepts only the typed
//! requests below from korrid's uid only, and runs each through the same
//! `Host` method the `korri-plugin` CLI uses. Lock, catalog validation,
//! archive hash, publisher signature and approval-digest checks stay in the
//! host. Every shape is a subset of an existing host producer, cited per type.
//! The plugin host compiles this file through `#[path]`, so both sides share
//! one definition. korrid's `/rpc` returns these payloads unchanged.
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

/// Both directions carry one JSON document. A catalog is at most 4 MiB and a
/// response projects only a subset of it.
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

/// korrid could not reach the admin socket or read its answer.
pub const CODE_UPLOAD_NOT_FOUND: &str = "PluginUploadNotFound";
pub const CODE_UNAVAILABLE: &str = "PluginHostUnavailable";
/// The host refused the request; the message is the host's own error text.
pub const CODE_REFUSED: &str = "PluginHostRefused";
/// The repository address failed `https_url::parse`; the message is its text.
pub const CODE_URL_INVALID: &str = "PluginRepositoryUrlInvalid";
/// Enable was refused because the stored approval no longer matches the
/// installed package or host policy (`host.rs` approved_in).
pub const CODE_PUBLISHER_NOT_TRUSTED: &str = "PluginPublisherNotTrusted";
pub const CODE_PUBLISHER_KEY_CHANGED: &str = "PluginPublisherKeyChanged";
pub const CODE_NAMESPACE_TAKEN: &str = "PluginNamespaceTaken";
pub const CODE_REQUIREMENT_CONFLICT: &str = "PluginRequirementConflict";
pub const CODE_NEEDS_REVIEW: &str = "PluginNeedsReview";

// Producers: `main.rs` `repository list` (official vs user-added split),
// `source_store.rs` list_sources, `repository.rs` Configuration.official, and
// the host receipts (`selection.rs` Receipt, `provenance.rs` Repository).
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRepositories {
    /// Absent until the device's trusted configuration names an official
    /// catalog (`services.korri.pluginHost.officialCatalogUrl`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official: Option<String>,
    pub user_added: Vec<String>,
    pub repository_installs: Vec<PluginRepositoryInstall>,
    /// Every other installed plugin, on or off: image-seeded and raw-cache
    /// installs.
    pub other_installs: Vec<PluginOtherInstall>,
}

// Producer: `selection.rs:10` Receipt (`id`, `desired` Enabled/Disabled) with
// `provenance.rs:12` Provenance::Repository (`source_url`,
// `release_version`). Receipts in the Removed state are left out.
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRepositoryInstall {
    pub id: String,
    pub source_url: String,
    pub release_version: String,
    pub enabled: bool,
}

// Producers: `selection.rs:10` Receipt (`id`, `desired` Enabled/Disabled) for
// every receipt whose provenance is not Provenance::Repository, and
// `declaration.rs` Declaration.title of the installed package, left out when
// that declaration cannot be read. Receipts in the Removed state are left out.
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginOtherInstall {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub enabled: bool,
}

// Producer: `source_store.rs:40` AddOutcome, returned by `host.rs` add_source.
#[typeshare]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum PluginRepositoryAddResult {
    Added,
    AlreadyPresent,
}

// Producer: `source_store.rs:45` RemoveOutcome, returned by `host.rs`
// remove_source. The host refuses the official source before this point.
#[typeshare]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum PluginRepositoryRemoveResult {
    Removed,
    NotFound,
}

// Producer: `selection.rs` Desired without its Removed variant.
#[typeshare]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum PluginInstalledState {
    Disabled,
    Enabled,
}

#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCatalog {
    /// Records for this host's platform only, in catalog order.
    pub entries: Vec<PluginCatalogEntry>,
}

// Producers: `catalog.rs:31` CatalogRecord (`plugin_id`, `title`,
// `description`, `release_version`), `selection.rs:10` Receipt.desired, and
// `package.rs:214` PublisherBindings membership of the plugin_id namespace.
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCatalogEntry {
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub release_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<PluginInstalledState>,
    pub publisher: PluginPublisher,
}

// Producer: `package.rs:24-52` Report, returned by `host.rs:270`
// inspect_repository. `title` is Report.declaration.title, `nativeUnits` the
// keys of Report.native_units, `ports` Report.ports.
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginInspection {
    pub publisher: PluginPublisher,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub approval: String,
    pub warning: String,
    pub ports: PluginPorts,
    pub native_units: Vec<String>,
    pub brings: Vec<PluginDependency>,
}

// Producer: `firewall.rs` Ports, with its NixOS field names.
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPorts {
    #[serde(rename = "allowedTCPPorts")]
    pub allowed_tcp_ports: Vec<u16>,
    #[serde(rename = "allowedUDPPorts")]
    pub allowed_udp_ports: Vec<u16>,
}

// Producer: `package.rs` DependencyReport (its Report's id, declaration title
// and warning, plus `already_approved`).
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDependency {
    pub publisher: PluginPublisher,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub warning: String,
    pub already_approved: bool,
}

/// The host committed the change. Install leaves the plugin Disabled.
#[typeshare]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginChangeApplied {}

#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRepositoriesRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRepositoryRequest {
    pub url: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginInspectRequest {
    pub url: String,
    pub plugin_id: String,
    pub release_version: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginInstallRequest {
    pub url: String,
    pub plugin_id: String,
    pub release_version: String,
    /// The exact approval digest the person reviewed from PluginInspection.
    pub approval: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginSetEnabledRequest {
    pub id: String,
    pub enabled: bool,
}

// Approved plugin-direction schema sheet, rows 7, 9, 14-17.
#[typeshare]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum PluginPublisherState {
    Bound,
    Trusted,
    New,
    KeyChanged,
    Unsigned,
}

#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginPublisher {
    pub namespace: String,
    pub state: PluginPublisherState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disclosure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Publishers emits only Bound or Trusted; the shared state type also serves inspection.
pub struct PluginPublisherEntry {
    pub namespace: String,
    pub fingerprint: String,
    pub state: PluginPublisherState,
    pub installed: Vec<String>,
}

#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPublishers {
    pub entries: Vec<PluginPublisherEntry>,
}

#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPublishersRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginTrustPublisherRequest {
    pub namespace: String,
    pub public_key: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRevokePublisherRequest {
    pub namespace: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArchiveRequest {
    pub path: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArchiveInstallRequest {
    pub path: String,
    pub approval: String,
}

/// The complete admin socket request set. Each maps 1:1 to the `Host` method
/// that the named `korri-plugin` CLI command already calls. Anything else
/// fails to parse and is refused.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum PluginAdminRequest {
    Publishers(PluginPublishersRequest),
    TrustPublisher(PluginTrustPublisherRequest),
    RevokePublisher(PluginRevokePublisherRequest),
    InspectArchive(PluginArchiveRequest),
    InstallArchive(PluginArchiveInstallRequest),
    /// `repository list`
    Repositories(PluginRepositoriesRequest),
    /// `repository add URL`
    RepositoryAdd(PluginRepositoryRequest),
    /// `repository remove URL`
    RepositoryRemove(PluginRepositoryRequest),
    /// `repository catalog URL`
    Catalog(PluginRepositoryRequest),
    /// `repository inspect URL ID RELEASE`
    Inspect(PluginInspectRequest),
    /// `repository install URL ID RELEASE APPROVAL`
    Install(PluginInstallRequest),
    /// `enable ID` / `disable ID`
    SetEnabled(PluginSetEnabledRequest),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginAdminFailure {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
// One owned JSON response per connection; keep the shared constructors simple.
#[allow(clippy::large_enum_variant)]
pub enum PluginAdminResponse {
    Publishers(PluginPublishers),
    Repositories(PluginRepositories),
    RepositoryAdded(PluginRepositoryAddResult),
    RepositoryRemoved(PluginRepositoryRemoveResult),
    Catalog(PluginCatalog),
    Inspection(PluginInspection),
    Applied(PluginChangeApplied),
    Refused(PluginAdminFailure),
}

// Chunked archive transport. Only korrid maps these opaque IDs to paths.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArchiveBeginRequest {
    pub sha256: String,
    #[typeshare(serialized_as = "f64")]
    pub size: u64,
}
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginArchiveUpload {
    pub upload_id: String,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginArchiveChunkRequest {
    pub upload_id: String,
    #[typeshare(serialized_as = "f64")]
    pub offset: u64,
    pub data: String,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginArchiveUploadRequest {
    pub upload_id: String,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginArchiveUploadInstallRequest {
    pub upload_id: String,
    pub approval: String,
}
