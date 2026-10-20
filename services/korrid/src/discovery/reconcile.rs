pub mod change;
mod documents;
pub use change::{reconcile, CatalogChange};
use documents::Documents;

use crate::{
    config::{
        self,
        snapshot::{DEVICE_FILE_NAME, FILE_NAMES, LIBRARY_FILE_NAME},
        ConfigSnapshot,
    },
    discovery::{
        attention,
        identify::{AccountRoot, HashCache, PathRecord, ScanReport, SourceView},
        scanner::TraversalBudget,
        sources::{self, PairingError, PairingRequest},
    },
    plugin_policy,
};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const PRIVATE_STATE_DIR: &str = "game-discovery";
const HASH_CACHE_FILE: &str = "hash-cache.json";
const OWNERSHIP_FILE: &str = "ownership.json";
const REPAIR_FILE: &str = "repair.json";

#[derive(Clone)]
pub struct DiscoveryCoordinator {
    readable_root: PathBuf,
    private_root: PathBuf,
    write_lock: Arc<Mutex<()>>,
    registry_source: plugin_policy::RegistrySource,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryOptions {
    pub first_seen_at: String,
    pub max_diagnostics: usize,
    pub max_candidates: usize,
    pub max_entries: usize,
    pub max_directories: usize,
    pub max_depth: usize,
    pub max_sortable_entries: usize,
}
impl Default for DiscoveryOptions {
    fn default() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            first_seen_at: chrono::DateTime::from_timestamp(seconds as i64, 0)
                .expect("current time")
                .to_rfc3339(),
            max_diagnostics: 1000,
            max_candidates: 10_000,
            max_entries: 100_000,
            max_directories: 10_000,
            max_depth: 32,
            max_sortable_entries: 10_000,
        }
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveryMutationReport {
    pub scan: ScanReport,
    /// File bytes read to hash content during this scan; unchanged and
    /// seeded files cost none.
    pub hashed_bytes: u64,
    pub scan_duration_ms: u128,
    pub added_games: usize,
    pub removed_locations: usize,
    pub storage_id: Option<String>,
    pub repaired: bool,
}
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("discovery configuration changed outside Korri; reload and try again")]
    Conflict,
    #[error("invalid discovery input: {0}")]
    Invalid(String),
    #[error("discovery storage: {0}")]
    Storage(String),
    #[error("discovery candidate: {0}")]
    Candidate(String),
    #[error(transparent)]
    Pairing(#[from] PairingError),
}

enum Mutation<'a> {
    Rescan,
    Pair(PathBuf, &'a str),
    Change(&'a str, &'a str),
    Unpair(&'a str),
}
impl DiscoveryCoordinator {
    pub fn new(readable_root: impl AsRef<Path>, private_root: impl AsRef<Path>) -> Self {
        Self::with_write_lock(readable_root, private_root, Arc::new(Mutex::new(())))
    }
    pub fn with_write_lock(
        readable_root: impl AsRef<Path>,
        private_root: impl AsRef<Path>,
        write_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            readable_root: readable_root.as_ref().into(),
            private_root: private_root.as_ref().into(),
            write_lock,
            registry_source: plugin_policy::RegistrySource::Installed,
        }
    }
    pub fn with_registry_source(mut self, source: plugin_policy::RegistrySource) -> Self {
        self.registry_source = source;
        self
    }
    pub fn for_tests(readable_root: impl AsRef<Path>, private_root: impl AsRef<Path>) -> Self {
        Self::for_tests_with_write_lock(readable_root, private_root, Arc::new(Mutex::new(())))
    }
    pub fn for_tests_with_write_lock(
        readable_root: impl AsRef<Path>,
        private_root: impl AsRef<Path>,
        write_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self::with_write_lock(readable_root, private_root, write_lock).with_registry_source(
            plugin_policy::RegistrySource::Selected(Arc::new(
                crate::plugin_test_fixtures::claims_only_registry(),
            )),
        )
    }
    pub fn write_lock(&self) -> Arc<Mutex<()>> {
        self.write_lock.clone()
    }
    /// Read-only: callers can already hold the shared publication lock.
    pub fn source_views(&self) -> Result<Vec<SourceView>, DiscoveryError> {
        let path = self.readable_root.join(DEVICE_FILE_NAME);
        let device = match fs::read_to_string(path) {
            Ok(device) => device,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(DiscoveryError::Storage(error.to_string())),
        };
        let device = parse_mapping(&device)?;
        let storage = serde_yaml::from_value(
            device
                .get("storage")
                .cloned()
                .unwrap_or_else(|| Value::Mapping(Mapping::new())),
        )
        .map_err(|e| DiscoveryError::Candidate(e.to_string()))?;
        let sources = serde_yaml::from_value(
            device
                .get("sources")
                .cloned()
                .unwrap_or_else(|| Value::Mapping(Mapping::new())),
        )
        .map_err(|e| DiscoveryError::Candidate(e.to_string()))?;
        Ok(source_views(&ConfigSnapshot {
            storage,
            sources,
            ..ConfigSnapshot::default()
        }))
    }
    pub fn pair(
        &self,
        root: PathBuf,
        system: &str,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        self.mutate(Mutation::Pair(root, system), options)
    }
    pub fn change(
        &self,
        source: &str,
        system: &str,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        self.mutate(Mutation::Change(source, system), options)
    }
    pub fn unpair(
        &self,
        source: &str,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        self.mutate(Mutation::Unpair(source), options)
    }
    pub fn rescan(
        &self,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        self.mutate(Mutation::Rescan, options)
    }

    /// The committed documents, validated, under the publication lock.
    pub fn published(&self) -> Result<ConfigSnapshot, DiscoveryError> {
        let _guard = self
            .write_lock
            .lock()
            .expect("discovery write lock poisoned");
        Documents::read(&self.readable_root)?.validate()
    }

    /// Seed the private hash cache with digests computed while writing
    /// `path`, under the publication lock, so the next scan reads none of it.
    pub fn seed_hash(
        &self,
        path: &Path,
        rules: &[crate::plugin::HeaderRule],
        sha256_hex: &str,
        sha1_hex: &str,
    ) -> Result<bool, DiscoveryError> {
        let _guard = self
            .write_lock
            .lock()
            .expect("discovery write lock poisoned");
        let mut cache: HashCache = read_json(&state_file(&self.private_root, HASH_CACHE_FILE))?;
        let seeded = cache
            .seed(path, rules, sha256_hex, sha1_hex)
            .map_err(|e| DiscoveryError::Storage(e.to_string()))?;
        if seeded {
            attention::atomic_save_json(&self.private_root, HASH_CACHE_FILE, &cache)
                .map_err(|e| DiscoveryError::Storage(e.to_string()))?;
        }
        Ok(seeded)
    }

    fn mutate(
        &self,
        mutation: Mutation<'_>,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        let _guard = self
            .write_lock
            .lock()
            .expect("discovery write lock poisoned");
        ensure_fixed_files(&self.readable_root)?;
        let mut private = PrivateState::read(&self.private_root)?;
        let repaired = documents::recover(&self.readable_root, &self.private_root, &mut private)?;
        let current = Documents::read(&self.readable_root)?;
        let initial = current.validate()?;
        let mut staged = initial.clone();
        let registry = self
            .registry_source
            .registry()
            .map_err(|e| DiscoveryError::Candidate(e.to_string()))?;
        let existing = source_views(&initial);
        let mut unpaired = Vec::new();
        let mut clear_source = None;
        let mut storage_id = None;
        let pairing = match mutation {
            Mutation::Pair(root, system) => Some((root, system, None)),
            Mutation::Change(source, system) => {
                let root = existing
                    .iter()
                    .find(|s| s.source_id == source)
                    .ok_or_else(|| PairingError::UnknownSource(source.into()))?
                    .root
                    .clone();
                unpaired.push(source.to_owned());
                clear_source = Some(source.to_owned());
                Some((root, system, Some(source)))
            }
            Mutation::Unpair(source) => {
                if !staged.sources.contains_key(source) {
                    return Err(PairingError::UnknownSource(source.into()).into());
                }
                unpaired.push(source.to_owned());
                clear_source = Some(source.to_owned());
                None
            }
            Mutation::Rescan => None,
        };
        if let Some((root, system, replacing)) = &pairing {
            sources::validate_pairing(
                &PairingRequest {
                    root,
                    system,
                    readable_root: &self.readable_root,
                    private_root: &self.private_root,
                    replacing: *replacing,
                },
                &existing,
                &registry,
            )?;
        }
        // A change is unpair + pair in a single publication, including private provenance.
        if !unpaired.is_empty() {
            let cleanup = reconcile(
                &ScanReport {
                    unpaired: unpaired.clone(),
                    created_storages: private.ownership.created_storages.clone(),
                    origins: private.ownership.origins.clone(),
                    paths: private.ownership.paths.clone(),
                    ..ScanReport::default()
                },
                &staged,
                &options.first_seen_at,
            );
            staged = apply_change(&staged, &cleanup)?;
            private.ownership = OwnershipJournal {
                created_storages: cleanup.created_storages,
                origins: cleanup.origins,
                paths: cleanup.paths,
            };
        }
        if let Some((root, system, _)) = pairing {
            let root = root
                .canonicalize()
                .map_err(|e| DiscoveryError::Storage(e.to_string()))?;
            // Source and storage share one id. Never overwrite an authored storage record.
            let occupied: BTreeSet<_> = staged
                .sources
                .keys()
                .chain(staged.storage.keys())
                .cloned()
                .collect();
            let id = change::unique_id(system, |id| occupied.contains(id));
            staged.storage.insert(
                id.clone(),
                config::StoragePayload {
                    root: config::NonEmptyString(root.display().to_string()),
                    path: None,
                },
            );
            staged.sources.insert(
                id.clone(),
                config::SourcePayload {
                    title: None,
                    kind: vec![config::NonEmptyString("files".into())],
                    storage: config::NonEmptyString(id.clone()),
                    system: config::NonEmptyString(system.into()),
                },
            );
            private.ownership.created_storages.insert(id.clone());
            storage_id = Some(id);
        }
        let accounts: Vec<AccountRoot> = sources::account_roots(&self.readable_root)
            .map_err(|e| DiscoveryError::Storage(e.to_string()))?;
        let excluded = sources::excluded_roots(&self.readable_root, &self.private_root);
        let mut cache: HashCache = read_json(&state_file(&self.private_root, HASH_CACHE_FILE))?;
        let mut decisions = attention::load(&self.private_root)
            .map_err(|e| DiscoveryError::Storage(e.to_string()))?;
        // Old assignments must not influence the new pairing before they are removed on success.
        if let Some(source) = &clear_source {
            decisions.clear_source(source);
        }
        decisions.retain_pairings(&sources::pairings(&source_views(&staged), &accounts));
        let started = Instant::now();
        let mut scan = crate::discovery::scanner::scan(
            &source_views(&staged),
            &accounts,
            &excluded,
            &registry,
            &decisions,
            &mut cache,
            &TraversalBudget {
                max_entries: options.max_entries,
                max_directories: options.max_directories,
                max_depth: options.max_depth,
                max_sortable_entries: options.max_sortable_entries,
            },
        );
        scan.providers =
            crate::discovery::provider::scan(&accounts, &registry, &mut scan.diagnostics);
        scan.accounts = accounts;
        scan.declared_games = registry.declared_games().clone();
        scan.created_storages = private.ownership.created_storages.clone();
        scan.origins = private.ownership.origins.clone();
        scan.paths = private.ownership.paths.clone();
        // Cleanup has already run against the old source records, not the new pairing.
        let change = reconcile(&scan, &staged, &options.first_seen_at);
        let final_snapshot = apply_change(&staged, &change)?;
        let candidate = current.with_snapshot(&initial, &final_snapshot)?;
        private.next_ownership = Some(OwnershipJournal {
            created_storages: change.created_storages,
            origins: change.origins,
            paths: change.paths,
        });
        current.commit(
            candidate,
            &self.readable_root,
            &self.private_root,
            &mut private,
        )?;
        attention::atomic_save_json(&self.private_root, HASH_CACHE_FILE, &cache)
            .map_err(|e| DiscoveryError::Storage(e.to_string()))?;
        let mut decisions = attention::load(&self.private_root)
            .map_err(|e| DiscoveryError::Storage(e.to_string()))?;
        if let Some(source) = clear_source {
            decisions.clear_source(&source);
        }
        decisions.retain_pairings(&sources::pairings(
            &source_views(&final_snapshot),
            &scan.accounts,
        ));
        attention::save(&self.private_root, &decisions)
            .map_err(|e| DiscoveryError::Storage(e.to_string()))?;
        scan.unpaired = unpaired;
        Ok(DiscoveryMutationReport {
            hashed_bytes: cache.hashed_bytes(),
            scan,
            scan_duration_ms: started.elapsed().as_millis(),
            added_games: final_snapshot
                .library
                .keys()
                .filter(|id| !initial.library.contains_key(*id))
                .count(),
            removed_locations: initial
                .locations
                .values()
                .map(Vec::len)
                .sum::<usize>()
                .saturating_sub(final_snapshot.locations.values().map(Vec::len).sum()),
            storage_id,
            repaired,
        })
    }

    /// One-off migration title carry-over uses the same conflict gate and ownership journal.
    pub(crate) fn carry_titles(
        &self,
        titles: &BTreeMap<String, String>,
    ) -> Result<(), DiscoveryError> {
        let _guard = self
            .write_lock
            .lock()
            .expect("discovery write lock poisoned");
        let mut private = PrivateState::read(&self.private_root)?;
        documents::recover(&self.readable_root, &self.private_root, &mut private)?;
        let current = Documents::read(&self.readable_root)?;
        let before = current.validate()?;
        let mut after = before.clone();
        for (id, title) in titles {
            let game = after
                .library
                .get_mut(id)
                .ok_or_else(|| DiscoveryError::Candidate("migration game is unavailable".into()))?;
            game.title = Some(title.clone());
        }
        current.commit(
            current.with_snapshot(&before, &after)?,
            &self.readable_root,
            &self.private_root,
            &mut private,
        )
    }

    /// Publish a completed scan. The coordinator and offline tools share this path.
    pub fn run(
        &self,
        report: &ScanReport,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        let _guard = self
            .write_lock
            .lock()
            .expect("discovery write lock poisoned");
        ensure_fixed_files(&self.readable_root)?;
        let mut private = PrivateState::read(&self.private_root)?;
        let repaired = documents::recover(&self.readable_root, &self.private_root, &mut private)?;
        let current = Documents::read(&self.readable_root)?;
        let snapshot = current.validate()?;
        let mut report = report.clone();
        report
            .created_storages
            .extend(private.ownership.created_storages.iter().cloned());
        report.origins = private.ownership.origins.clone();
        report.paths = private.ownership.paths.clone();
        let change = reconcile(&report, &snapshot, &options.first_seen_at);
        let candidate = current.with_snapshot(&snapshot, &apply_change(&snapshot, &change)?)?;
        private.next_ownership = Some(OwnershipJournal {
            created_storages: change.created_storages,
            origins: change.origins,
            paths: change.paths,
        });
        current.commit(
            candidate,
            &self.readable_root,
            &self.private_root,
            &mut private,
        )?;
        Ok(DiscoveryMutationReport {
            scan: report,
            added_games: change.added_games,
            repaired,
            ..DiscoveryMutationReport::default()
        })
    }
}

pub(crate) fn source_views(snapshot: &ConfigSnapshot) -> Vec<SourceView> {
    snapshot
        .sources
        .iter()
        .filter_map(|(id, source)| {
            snapshot.storage.get(&source.storage.0).map(|storage| {
                let root = PathBuf::from(&storage.root.0);
                let canonical_root = root.canonicalize().ok();
                SourceView {
                    source_id: id.clone(),
                    storage_id: source.storage.0.clone(),
                    title: source.title.clone(),
                    system: source.system.0.clone(),
                    root,
                    canonical_root,
                    scans_files: source.kind.iter().any(|k| k.0 == "files"),
                }
            })
        })
        .collect()
}
pub(crate) fn apply_change(
    current: &ConfigSnapshot,
    change: &CatalogChange,
) -> Result<ConfigSnapshot, DiscoveryError> {
    let device = change.device_yaml.clone().unwrap_or_else(|| {
        serde_yaml::to_string(&change::device_value(current)).expect("device serializes")
    });
    let library = change.library_yaml.clone().unwrap_or_else(|| {
        serde_yaml::to_string(&BTreeMap::from([("library", &current.library)]))
            .expect("library serializes")
    });
    config::decode_config_documents(&device, &library)
        .map_err(|e| DiscoveryError::Candidate(e.to_string()))
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct OwnershipJournal {
    #[serde(default)]
    created_storages: BTreeSet<String>,
    #[serde(default)]
    origins: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    paths: BTreeMap<String, PathRecord>,
}
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct RepairJournal {
    #[serde(default)]
    pending_write: Option<documents::PendingWrite>,
}
#[derive(Default)]
struct PrivateState {
    ownership: OwnershipJournal,
    repair: RepairJournal,
    next_ownership: Option<OwnershipJournal>,
}
impl PrivateState {
    fn read(root: &Path) -> Result<Self, DiscoveryError> {
        Ok(Self {
            ownership: read_json(&state_file(root, OWNERSHIP_FILE))?,
            repair: read_json(&state_file(root, REPAIR_FILE))?,
            next_ownership: None,
        })
    }
    fn write(&self, root: &Path) -> Result<(), DiscoveryError> {
        write_json_atomically(&state_file(root, OWNERSHIP_FILE), &self.ownership)?;
        write_json_atomically(&state_file(root, REPAIR_FILE), &self.repair)
    }
}
pub(crate) fn reject_pending_publication(private_root: &Path) -> Result<(), DiscoveryError> {
    if PrivateState::read(private_root)?
        .repair
        .pending_write
        .is_some()
    {
        Err(DiscoveryError::Conflict)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiscoveryOwnedGame {
    pub playable_id: String,
    pub title: String,
    pub release_id: String,
    pub release_fingerprint: String,
    pub rom_identity: String,
}
pub(crate) fn owned_discovery_games(
    readable_root: &Path,
    private_root: &Path,
) -> Result<Vec<DiscoveryOwnedGame>, DiscoveryError> {
    let snapshot = Documents::read(readable_root)?.validate()?;
    let private = PrivateState::read(private_root)?;
    Ok(snapshot
        .library
        .iter()
        .filter_map(|(id, game)| {
            // Replacement preserves release history. Enrich the first available
            // discovery-owned release, not an unavailable historical release.
            let key = game.releases.iter().find_map(|release| {
                let key = release.identity.as_ref()?.value();
                (key.starts_with("sha256:")
                    && private.ownership.origins.contains_key(key)
                    && snapshot.locations.get(key).is_some_and(|locations| {
                        locations.iter().any(|location| {
                            matches!(location,
                            config::Location::File { discovery: Some(meta), .. }
                            | config::Location::FileSet { discovery: Some(meta), .. }
                            if meta.missing_since.is_none())
                        })
                    }))
                .then_some(key)
            })?;
            Some(DiscoveryOwnedGame {
                playable_id: id.clone(),
                title: game.title.clone().unwrap_or_default(),
                release_id: key.into(),
                release_fingerprint: fingerprint_mapping(
                    serde_yaml::to_value(game).ok()?.as_mapping()?,
                ),
                rom_identity: key.into(),
            })
        })
        .collect())
}
pub(crate) fn current_owned_discovery_game(
    readable_root: &Path,
    private_root: &Path,
    expected: &DiscoveryOwnedGame,
) -> Result<Option<DiscoveryOwnedGame>, DiscoveryError> {
    Ok(owned_discovery_games(readable_root, private_root)?
        .into_iter()
        .find(|game| game == expected))
}
pub(crate) fn update_owned_discovery_title(
    readable_root: &Path,
    private_root: &Path,
    write_lock: &Arc<Mutex<()>>,
    game: &DiscoveryOwnedGame,
    title: &str,
) -> Result<Option<DiscoveryOwnedGame>, DiscoveryError> {
    let _guard = write_lock.lock().expect("discovery write lock poisoned");
    let mut private = PrivateState::read(private_root)?;
    documents::recover(readable_root, private_root, &mut private)?;
    let Some(mut owned) = current_owned_discovery_game(readable_root, private_root, game)? else {
        return Ok(None);
    };
    let current = Documents::read(readable_root)?;
    let before = current.validate()?;
    let mut after = before.clone();
    let item = after
        .library
        .get_mut(&game.playable_id)
        .expect("owned game exists");
    item.title = Some(title.into());
    owned.title = title.into();
    owned.release_fingerprint = fingerprint_mapping(
        serde_yaml::to_value(item)
            .expect("game serializes")
            .as_mapping()
            .expect("game map"),
    );
    current.commit(
        current.with_snapshot(&before, &after)?,
        readable_root,
        private_root,
        &mut private,
    )?;
    Ok(Some(owned))
}

#[cfg(test)]
mod tests;
fn ensure_fixed_files(root: &Path) -> Result<(), DiscoveryError> {
    fs::create_dir_all(root.join("catalog"))
        .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
    for name in FILE_NAMES {
        let path = root.join(name);
        if !path.exists() {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut file) => file
                    .write_all(b"{}\n")
                    .map_err(|error| DiscoveryError::Storage(error.to_string()))?,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(DiscoveryError::Storage(error.to_string())),
            }
        }
    }
    Ok(())
}

fn parse_mapping(content: &str) -> Result<Mapping, DiscoveryError> {
    match serde_yaml::from_str::<Value>(content)
        .map_err(|error| DiscoveryError::Candidate(error.to_string()))?
    {
        Value::Null => Ok(Mapping::new()),
        Value::Mapping(mapping) => Ok(mapping),
        _ => Err(DiscoveryError::Invalid(
            "YAML document must contain a record".into(),
        )),
    }
}

fn serialize_mapping(document: Mapping) -> Result<String, DiscoveryError> {
    serde_yaml::to_string(&Value::Mapping(document))
        .map_err(|error| DiscoveryError::Candidate(error.to_string()))
}

fn read_fixed(root: &Path, name: &str) -> Result<String, DiscoveryError> {
    fs::read_to_string(root.join(name)).map_err(|error| DiscoveryError::Storage(error.to_string()))
}

fn write_atomically(
    path: &Path,
    content: &[u8],
    expected_revision: &str,
) -> Result<(), DiscoveryError> {
    let parent = path
        .parent()
        .ok_or_else(|| DiscoveryError::Storage("config file has no parent".into()))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config"),
        hex::encode(rand::random::<[u8; 8]>())
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        file.write_all(content)
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        file.sync_all()
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        let current =
            fs::read_to_string(path).map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        if revision(&current) != expected_revision {
            return Err(DiscoveryError::Conflict);
        }
        fs::rename(&temporary, path).map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn revision(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

fn state_dir(root: &Path) -> PathBuf {
    root.join(PRIVATE_STATE_DIR)
}

/// Records storage that a one-off migration created as discovery-owned, so a
/// later unpair removes it like storage created by `pair` (contract
/// section 4). Existing ownership entries are kept.
pub fn record_created_storages(
    private_root: &Path,
    storage_ids: impl IntoIterator<Item = String>,
) -> Result<(), DiscoveryError> {
    let path = state_file(private_root, OWNERSHIP_FILE);
    let mut ownership: OwnershipJournal = read_json(&path)?;
    ownership.created_storages.extend(storage_ids);
    write_json_atomically(&path, &ownership)
}

fn state_file(root: &Path, name: &str) -> PathBuf {
    state_dir(root).join(name)
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T, DiscoveryError> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content =
        fs::read_to_string(path).map_err(|error| DiscoveryError::Storage(error.to_string()))?;
    serde_json::from_str(&content).map_err(|error| DiscoveryError::Candidate(error.to_string()))
}

fn write_json_atomically<T: Serialize>(path: &Path, value: &T) -> Result<(), DiscoveryError> {
    let parent = path
        .parent()
        .ok_or_else(|| DiscoveryError::Storage("private state file has no parent".into()))?;
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
    let content =
        serde_json::to_vec(value).map_err(|error| DiscoveryError::Candidate(error.to_string()))?;
    if fs::read(path).ok().as_deref() == Some(content.as_slice()) {
        return Ok(());
    }
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        hex::encode(rand::random::<[u8; 8]>())
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        file.write_all(&content)
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        file.sync_all()
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        fs::rename(&temporary, path).map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn fingerprint_mapping(mapping: &Mapping) -> String {
    let bytes = serde_yaml::to_string(&Value::Mapping(mapping.clone())).unwrap_or_default();
    format!("sha256:{}", hex::encode(Sha256::digest(bytes.as_bytes())))
}
