//! Data shared between identification, scanning, reconciliation and
//! migration. Contract: `docs/briefs/2026-10-16-discovery-v2-contract.md`
//! section 5. Change fields only through the consolidator.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use crate::discovery::attention::AttentionFinding;
use crate::discovery::scanner::DiscoveryDiagnostic;

/// One paired source ready to scan (ADR 0001). `system` is the source's one
/// `system` key: a system id or a group id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceInput<'a> {
    pub source_id: &'a str,
    pub storage_id: &'a str,
    pub root: &'a std::path::Path,
    pub system: &'a str,
    /// Canonical plugin-data roots. Traversal and descriptor references must
    /// never enter these (contract section 4).
    pub excluded_roots: &'a [PathBuf],
}

/// A source as read from `device.yaml` `sources` and `storage`, with its
/// canonical root when it resolves.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceView {
    pub source_id: String,
    pub storage_id: String,
    pub title: Option<String>,
    pub system: String,
    pub root: PathBuf,
    pub canonical_root: Option<PathBuf>,
    pub scans_files: bool,
}

/// An account root (`<root>/users/<user>`, `launcher/linux_plugin.rs`) where
/// declared prepared files live (ADR 0003).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountRoot {
    pub user: String,
    pub storage_id: String,
    pub root: PathBuf,
}

/// Prior discovery-owned files for one source and primary path.
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct PathRecord {
    pub identity: String,
    pub files: Vec<String>,
}

/// A release's content identity (ADR 0005).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContentIdentity {
    /// One file, header excluded. Both values carry their prefix
    /// (`sha256:<64 hex>`, `sha1:<40 hex>`).
    Hash { sha256: String, sha1: String },
    /// Ordered tracks or discs. `value` is `sha256:` of SHA-256 over the
    /// part values joined by `\n`.
    Parts { value: String, parts: Vec<String> },
    /// CHD v5 header `rawsha1`, as `sha1:<40 hex>`.
    Chd { sha1: String },
}

impl ContentIdentity {
    /// The release key: `locations` key and catalog identity value.
    pub fn value(&self) -> &str {
        match self {
            Self::Hash { sha256, .. } => sha256,
            Self::Parts { value, .. } => value,
            Self::Chd { sha1 } => sha1,
        }
    }
}

/// The evidence level that decided a unit's system (brief 2026-10-02).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EvidenceLevel {
    Extension,
    Name,
    Bytes,
    DeclaredHash,
    /// A person assigned the system from the attention list.
    Assigned,
}

/// One identified unit: a file, a cue with its tracks, or a multi-disc set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundUnit {
    pub source_id: String,
    pub storage_id: String,
    /// Primary path relative to the source root (the file, cue or m3u, or
    /// the first disc).
    pub path: String,
    /// Every required file relative to the source root, in order, including
    /// nested descriptors. One entry for a single file.
    pub parts: Vec<String>,
    /// Paths named by cue FILE records, including tracks inside playlist discs.
    pub tracks: Vec<String>,
    pub system: String,
    pub title: String,
    pub version: Option<String>,
    pub region: Option<String>,
    pub identity: ContentIdentity,
    pub evidence: EvidenceLevel,
}

/// The result of scanning one source.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceScan {
    pub source_id: String,
    pub units: Vec<FoundUnit>,
    pub attention: Vec<AttentionFinding>,
    pub diagnostics: Vec<DiscoveryDiagnostic>,
    pub hashed_bytes: u64,
    /// Relative paths whose file was expected but is gone, for `missing-since`.
    pub seen_paths: Vec<String>,
}

/// One declared release found at its prepared file (ADR 0003).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredFile {
    /// `<plugin id>/<game key>`.
    pub game: String,
    pub release_key: String,
    pub storage_id: String,
    /// Path relative to the account root.
    pub path: String,
    pub status: DeclaredFileStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeclaredFileStatus {
    Found(ContentIdentity),
    Absent,
    Mismatch,
    Unreadable,
}

/// The result of checking every declared prepared file.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeclaredScan {
    pub files: Vec<DeclaredFile>,
    pub attention: Vec<AttentionFinding>,
    pub hashed_bytes: u64,
}

impl DeclaredScan {
    pub fn found(&self) -> impl Iterator<Item = (&DeclaredFile, &ContentIdentity)> {
        self.files.iter().filter_map(|file| match &file.status {
            DeclaredFileStatus::Found(identity) => Some((file, identity)),
            _ => None,
        })
    }
}

/// Everything one discovery pass produced; the input to reconciliation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanReport {
    pub providers: Vec<crate::discovery::provider::ProviderScan>,
    /// Storage records created by discovery, supplied by the document writer.
    pub created_storages: BTreeSet<String>,
    pub declared_games: BTreeMap<String, crate::plugin::DeclaredGame>,
    pub accounts: Vec<AccountRoot>,
    pub unpaired: Vec<String>,
    /// Private ownership history, supplied by the document writer.
    pub origins: BTreeMap<String, BTreeSet<String>>,
    /// Private primary path index: `source\npath` -> release identity.
    pub paths: BTreeMap<String, PathRecord>,
    /// Sources that were scanned completely. Reconciliation changes catalog
    /// entries only for these and for unpaired sources.
    pub scanned: Vec<SourceScan>,
    /// Sources that were not scanned (overlap, unknown system, unreadable).
    /// Their entries stay as they are.
    pub skipped: Vec<String>,
    pub declared: DeclaredScan,
    /// Source-level attention (overlap, system-unavailable).
    pub attention: Vec<AttentionFinding>,
    pub diagnostics: Vec<DiscoveryDiagnostic>,
}

/// Why identification of one path failed.
#[derive(Debug, thiserror::Error)]
pub enum IdentifyError {
    #[error("file could not be read: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported content: {0}")]
    Unsupported(String),
}
