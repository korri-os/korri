//! Wire between korrid and the root plugin host's admin socket.
//!
//! korrid runs unprivileged and still cannot reach the host's state, receipts
//! or store. The host's `korri-plugin serve-admin` accepts exactly the seven
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
pub const CODE_UNAVAILABLE: &str = "PluginHostUnavailable";
/// The host refused the request; the message is the host's own error text.
pub const CODE_REFUSED: &str = "PluginHostRefused";
/// The repository address failed `https_url::parse`; the message is its text.
pub const CODE_URL_INVALID: &str = "PluginRepositoryUrlInvalid";
/// Enable was refused because the stored approval no longer matches the
/// installed package or host policy (`host.rs` approved_in).
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
    pub publisher_trusted: bool,
}

// Producer: `package.rs:24-52` Report, returned by `host.rs:270`
// inspect_repository. `title` is Report.declaration.title, `nativeUnits` the
// keys of Report.native_units, `ports` Report.ports.
#[typeshare]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginInspection {
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

/// The complete admin socket request set. Each maps 1:1 to the `Host` method
/// that the named `korri-plugin` CLI command already calls. Anything else
/// fails to parse and is refused.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum PluginAdminRequest {
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
pub enum PluginAdminResponse {
    Repositories(PluginRepositories),
    RepositoryAdded(PluginRepositoryAddResult),
    RepositoryRemoved(PluginRepositoryRemoveResult),
    Catalog(PluginCatalog),
    Inspection(PluginInspection),
    Applied(PluginChangeApplied),
    Refused(PluginAdminFailure),
}
