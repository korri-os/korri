mod documents;
use documents::Documents;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use sha2::{Digest, Sha256};

use crate::{
    config::{
        self,
        snapshot::{DEVICE_FILE_NAME, FILE_NAMES, GAMES_FILE_NAME, RELEASES_FILE_NAME},
    },
    discovery::scanner::{
        DiscoveryDiagnostic, DiscoveryDiagnosticCode, FolderScanner, HashCache, ScanCandidate,
        ScanReport, TraversalBudget,
    },
    plugin_policy,
};

const PRIVATE_STATE_DIR: &str = "game-discovery";
const HASH_CACHE_FILE: &str = "hash-cache.json";
const OWNERSHIP_FILE: &str = "ownership.json";
const REPAIR_FILE: &str = "repair.json";
const DEFAULT_MAX_DIAGNOSTICS: usize = 1000;
const DEFAULT_MAX_CANDIDATES: usize = 10_000;
const DEFAULT_MAX_ENTRIES: usize = 100_000;
const DEFAULT_MAX_DIRECTORIES: usize = 10_000;
const DEFAULT_MAX_DEPTH: usize = 32;
const DEFAULT_MAX_SORTABLE_ENTRIES: usize = 10_000;

#[derive(Clone)]
pub struct DiscoveryCoordinator {
    readable_root: PathBuf,
    private_root: PathBuf,
    write_lock: Arc<Mutex<()>>,
    scan_lock: Arc<Mutex<()>>,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveryMutationReport {
    pub scan: ScanReport,
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
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        Self {
            first_seen_at: format!("unix:{seconds}"),
            max_diagnostics: DEFAULT_MAX_DIAGNOSTICS,
            max_candidates: DEFAULT_MAX_CANDIDATES,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_directories: DEFAULT_MAX_DIRECTORIES,
            max_depth: DEFAULT_MAX_DEPTH,
            max_sortable_entries: DEFAULT_MAX_SORTABLE_ENTRIES,
        }
    }
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
            readable_root: readable_root.as_ref().to_owned(),
            private_root: private_root.as_ref().to_owned(),
            write_lock,
            scan_lock: Arc::new(Mutex::new(())),
            registry_source: plugin_policy::RegistrySource::Installed,
        }
    }

    pub fn with_registry_source(mut self, source: plugin_policy::RegistrySource) -> Self {
        self.registry_source = source;
        self
    }

    pub fn add_location(
        &self,
        selected_root: impl AsRef<Path>,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        let _guard = self
            .write_lock
            .lock()
            .expect("discovery write lock poisoned");
        ensure_fixed_files(&self.readable_root)?;
        let canonical_root = canonical_directory(selected_root.as_ref())?;
        let mut private = PrivateState::read(&self.private_root)?;
        documents::recover(&self.readable_root, &self.private_root, &mut private)?;
        let current = Documents::read(&self.readable_root)?;
        current.validate()?;
        let mut document = parse_mapping(&current.device)?;
        let storage_id = storage_id_for_root(&canonical_root, &document, &private);
        let record = set_storage_record(&mut document, &storage_id, &canonical_root)?;
        private.remember_owned_storage(&storage_id, &canonical_root, &record);
        private.repair.pending_scans.insert(storage_id.clone());
        private.write(&self.private_root)?;
        let candidate = Documents {
            device: serialize_mapping(document)?,
            ..current.clone()
        };
        current.commit(
            candidate,
            &self.readable_root,
            &self.private_root,
            &mut private,
        )?;
        drop(_guard);

        let mut report = self.rescan(options)?;
        report.storage_id = Some(storage_id);
        Ok(report)
    }

    pub fn remove_location(
        &self,
        storage_id: &str,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        let _guard = self
            .write_lock
            .lock()
            .expect("discovery write lock poisoned");
        ensure_fixed_files(&self.readable_root)?;
        let mut private = PrivateState::read(&self.private_root)?;
        private
            .repair
            .pending_removals
            .insert(storage_id.to_owned());
        private.write(&self.private_root)?;

        private.storage_order.retain(|id| id != storage_id);
        private.write(&self.private_root)?;
        drop(_guard);

        let mut report = self.rescan(options)?;
        report.storage_id = Some(storage_id.to_owned());
        Ok(report)
    }

    pub(crate) fn has_recovery_work(&self) -> bool {
        PrivateState::read(&self.private_root).is_ok_and(|private| {
            !private.repair.pending_scans.is_empty()
                || !private.repair.pending_removals.is_empty()
                || private.repair.pending_write.is_some()
        })
    }

    pub(crate) fn owned_location_summaries(
        readable_root: &Path,
        private_root: &Path,
    ) -> Result<Vec<(String, String)>, DiscoveryError> {
        ensure_fixed_files(readable_root)?;
        let private = PrivateState::read(private_root)?;
        let config_yaml = read_fixed(readable_root, DEVICE_FILE_NAME)?;
        let config_doc = parse_mapping(&config_yaml)?;
        Ok(ordered_owned_storage_summaries(&config_doc, &private))
    }

    pub fn rescan(
        &self,
        options: &DiscoveryOptions,
    ) -> Result<DiscoveryMutationReport, DiscoveryError> {
        let mut report = DiscoveryMutationReport::default();
        let (expected, registry, storages, mut hash_cache) = {
            let _guard = self
                .write_lock
                .lock()
                .expect("discovery write lock poisoned");
            ensure_fixed_files(&self.readable_root)?;
            let mut private = PrivateState::read(&self.private_root)?;
            report.repaired =
                documents::recover(&self.readable_root, &self.private_root, &mut private)?;
            let current = Documents::read(&self.readable_root)?;
            apply_pending_ownership(&current, &mut private)?;
            if !private.repair.pending_removals.is_empty() {
                let cleanup = cleanup_removed_storages(
                    &self.readable_root,
                    &self.private_root,
                    &current,
                    &mut private,
                )?;
                report.removed_locations += cleanup.removed_locations;
                report.repaired |= cleanup.changed;
                private.repair.pending_removals.clear();
            }
            private.write(&self.private_root)?;
            let current = Documents::read(&self.readable_root)?;
            let snapshot = current.validate()?;
            let registry = self
                .registry_source
                .registry()
                .map_err(|error| DiscoveryError::Candidate(error.to_string()))?;
            let config_doc = parse_mapping(&current.device)?;
            (
                current,
                registry,
                ordered_storages(&snapshot, &config_doc, &private),
                private.hash_cache.clone(),
            )
        };

        let _scan_guard = self.scan_lock.lock().expect("discovery scan lock poisoned");
        let scanner = FolderScanner::with_budget(
            &registry,
            options.max_diagnostics,
            options.max_candidates,
            TraversalBudget {
                max_entries: options.max_entries,
                max_directories: options.max_directories,
                max_depth: options.max_depth,
                max_sortable_entries: options.max_sortable_entries,
            },
        );
        let scan_started = Instant::now();
        let mut scan = scanner.scan(&storages, &mut hash_cache);
        let scan_duration_ms = scan_started.elapsed().as_millis();
        append_dedupe_diagnostics(&mut scan, options.max_diagnostics);
        drop(_scan_guard);

        let _guard = self
            .write_lock
            .lock()
            .expect("discovery write lock poisoned");
        let mut private = PrivateState::read(&self.private_root)?;
        private.hash_cache = hash_cache;
        let current = Documents::read(&self.readable_root)?;
        apply_pending_ownership(&current, &mut private)?;
        if current != expected {
            private.write(&self.private_root)?;
            return Err(DiscoveryError::Conflict);
        }
        let reconciliation = reconcile_candidates(
            &self.readable_root,
            &self.private_root,
            &current,
            &mut private,
            &scan.candidates,
            options,
        )?;
        report.added_games += reconciliation.added_games;
        report.removed_locations += reconciliation.removed_locations;
        report.scan = scan;
        report.scan_duration_ms = scan_duration_ms;
        private.repair.pending_scans.clear();
        private.write(&self.private_root)?;
        Ok(report)
    }
}

// Caller holds the shared readable-document write lock. Settings must not
// change the expected device bytes of an interrupted discovery publication.
pub(crate) fn reject_pending_publication(private_root: &Path) -> Result<(), DiscoveryError> {
    if PrivateState::read(private_root)?
        .repair
        .pending_write
        .is_some()
    {
        return Err(DiscoveryError::Conflict);
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct PrivateState {
    #[serde(default)]
    hash_cache: HashCache,
    #[serde(default)]
    ownership: OwnershipJournal,
    #[serde(default)]
    storage_ownership: StorageOwnershipJournal,
    #[serde(default)]
    repair: RepairJournal,
    #[serde(default)]
    storage_order: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct OwnershipJournal {
    #[serde(default)]
    releases: BTreeMap<String, OwnedRelease>,
    #[serde(default)]
    locations: BTreeMap<String, OwnedLocation>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct StorageOwnershipJournal {
    #[serde(default)]
    storages: BTreeMap<String, OwnedStorage>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct OwnedStorage {
    root: String,
    fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct OwnedRelease {
    playable_id: String,
    release_id: String,
    fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct OwnedLocation {
    release_id: String,
    fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiscoveryOwnedGame {
    pub playable_id: String,
    pub title: String,
    pub release_id: String,
    pub release_fingerprint: String,
    pub rom_identity: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct RepairJournal {
    #[serde(default)]
    pending_removals: BTreeSet<String>,
    #[serde(default)]
    pending_scans: BTreeSet<String>,
    #[serde(default)]
    pending_ownership: BTreeMap<String, OwnedRelease>,
    #[serde(default)]
    pending_locations: BTreeMap<String, OwnedLocation>,
    #[serde(default)]
    pending_write: Option<documents::PendingWrite>,
}

#[derive(Debug, Default)]
struct ReconcileStats {
    changed: bool,
    added_games: usize,
    removed_locations: usize,
}

impl PrivateState {
    fn read(root: &Path) -> Result<Self, DiscoveryError> {
        Ok(Self {
            hash_cache: read_json(&state_file(root, HASH_CACHE_FILE))?,
            ownership: read_json(&state_file(root, OWNERSHIP_FILE))?,
            storage_ownership: read_json(&state_file(root, "storage-ownership.json"))?,
            repair: read_json(&state_file(root, REPAIR_FILE))?,
            storage_order: read_json(&state_file(root, "storage-order.json"))?,
        })
    }

    fn write(&self, root: &Path) -> Result<(), DiscoveryError> {
        fs::create_dir_all(state_dir(root))
            .map_err(|error| DiscoveryError::Storage(error.to_string()))?;
        write_json_atomically(&state_file(root, HASH_CACHE_FILE), &self.hash_cache)?;
        write_json_atomically(&state_file(root, OWNERSHIP_FILE), &self.ownership)?;
        write_json_atomically(
            &state_file(root, "storage-ownership.json"),
            &self.storage_ownership,
        )?;
        write_json_atomically(&state_file(root, REPAIR_FILE), &self.repair)?;
        write_json_atomically(&state_file(root, "storage-order.json"), &self.storage_order)?;
        Ok(())
    }

    fn remember_storage(&mut self, storage_id: &str) {
        if !self.storage_order.iter().any(|id| id == storage_id) {
            self.storage_order.push(storage_id.to_owned());
        }
    }

    fn remember_owned_storage(&mut self, storage_id: &str, root: &Path, record: &Mapping) {
        self.remember_storage(storage_id);
        self.storage_ownership.storages.insert(
            storage_id.to_owned(),
            OwnedStorage {
                root: root.to_string_lossy().into_owned(),
                fingerprint: fingerprint_mapping(record),
            },
        );
    }

    fn storage_is_owned_current(&self, storage_id: &str, record: Option<&Mapping>) -> bool {
        let Some(owned) = self.storage_ownership.storages.get(storage_id) else {
            return false;
        };
        record.is_some_and(|record| owned.fingerprint == fingerprint_mapping(record))
    }
}

pub(crate) fn owned_discovery_games(
    readable_root: &Path,
    private_root: &Path,
) -> Result<Vec<DiscoveryOwnedGame>, DiscoveryError> {
    let documents = Documents::read(readable_root)?;
    let private = PrivateState::read(private_root)?;
    owned_games(&documents, &private)
}

fn owned_games(
    documents: &Documents,
    private: &PrivateState,
) -> Result<Vec<DiscoveryOwnedGame>, DiscoveryError> {
    let snapshot = documents.validate()?;
    let games = parse_mapping(&documents.games)?;
    let releases = parse_mapping(&documents.releases)?;
    let mut result = Vec::new();
    for (id, game) in &snapshot.games {
        // Enrichment has one content identity only when the game has one release.
        let [release] = game.releases.as_slice() else {
            continue;
        };
        if !release.0.starts_with("sha256:") {
            continue;
        }
        let Some(owned) = private
            .ownership
            .releases
            .get(&ownership_key(id, &release.0))
        else {
            continue;
        };
        let Some(fingerprint) = catalog_fingerprint(&games, &releases, id, &release.0) else {
            continue;
        };
        if owned.fingerprint != fingerprint {
            continue;
        }
        if !snapshot
            .locations
            .get(&release.0)
            .is_some_and(|locations| !locations.is_empty())
        {
            continue;
        }
        result.push(DiscoveryOwnedGame {
            playable_id: id.clone(),
            title: game.title.clone(),
            release_id: release.0.clone(),
            release_fingerprint: fingerprint,
            rom_identity: release.0.clone(),
        });
    }
    Ok(result)
}

fn catalog_fingerprint(games: &Mapping, releases: &Mapping, id: &str, sha: &str) -> Option<String> {
    let game = games.get("games")?.as_mapping()?.get(id)?.as_mapping()?;
    let release = releases
        .get("releases")?
        .as_mapping()?
        .get(sha)?
        .as_mapping()?;
    Some(revision(&format!(
        "{}\n{}",
        fingerprint_mapping(game),
        fingerprint_mapping(release)
    )))
}

fn same_owned_identity(left: &DiscoveryOwnedGame, right: &DiscoveryOwnedGame) -> bool {
    left.playable_id == right.playable_id
        && left.release_id == right.release_id
        && left.release_fingerprint == right.release_fingerprint
        && left.rom_identity == right.rom_identity
}

pub(crate) fn current_owned_discovery_game(
    readable_root: &Path,
    private_root: &Path,
    expected: &DiscoveryOwnedGame,
) -> Result<Option<DiscoveryOwnedGame>, DiscoveryError> {
    Ok(owned_discovery_games(readable_root, private_root)?
        .into_iter()
        .find(|current| same_owned_identity(current, expected)))
}

pub(crate) fn update_owned_discovery_title(
    readable_root: &Path,
    private_root: &Path,
    write_lock: &Arc<Mutex<()>>,
    game: &DiscoveryOwnedGame,
    title: &str,
) -> Result<Option<DiscoveryOwnedGame>, DiscoveryError> {
    let _guard = write_lock.lock().expect("discovery write lock poisoned");
    ensure_fixed_files(readable_root)?;
    let mut private = PrivateState::read(private_root)?;
    documents::recover(readable_root, private_root, &mut private)?;
    let current = Documents::read(readable_root)?;
    apply_pending_ownership(&current, &mut private)?;
    let Some(mut owned_game) = owned_games(&current, &private)?
        .into_iter()
        .find(|current| same_owned_identity(current, game))
    else {
        return Ok(None);
    };
    if owned_game.title == title {
        return Ok(Some(owned_game));
    }
    let mut games = parse_mapping(&current.games)?;
    let item = mapping_at(&mut games, "games")?
        .get_mut(game.playable_id.as_str())
        .and_then(Value::as_mapping_mut)
        .expect("validated game");
    item.insert("title".into(), title.into());
    let releases = parse_mapping(&current.releases)?;
    let fingerprint = catalog_fingerprint(&games, &releases, &game.playable_id, &game.release_id)
        .expect("validated release");
    let key = ownership_key(&game.playable_id, &game.release_id);
    let mut pending = private.ownership.releases[&key].clone();
    pending.fingerprint = fingerprint.clone();
    private.repair.pending_ownership.insert(key, pending);
    current.commit(
        Documents {
            games: serialize_mapping(games)?,
            ..current.clone()
        },
        readable_root,
        private_root,
        &mut private,
    )?;
    owned_game.title = title.to_owned();
    owned_game.release_fingerprint = fingerprint;
    Ok(Some(owned_game))
}

fn reconcile_candidates(
    root: &Path,
    private_root: &Path,
    current: &Documents,
    private: &mut PrivateState,
    candidates: &[ScanCandidate],
    options: &DiscoveryOptions,
) -> Result<ReconcileStats, DiscoveryError> {
    let snapshot = current.validate()?;
    let mut device_doc = parse_mapping(&current.device)?;
    let mut games_doc = parse_mapping(&current.games)?;
    let mut releases_doc = parse_mapping(&current.releases)?;
    let mut stats = ReconcileStats::default();
    let locations = mapping_at(&mut device_doc, "locations")?;
    // Index authored physical files as well as declared storage/path pairs.
    // Distinct storage IDs may name the same root or overlapping folders.
    let mut paths: BTreeMap<(String, String), BTreeMap<String, bool>> = BTreeMap::new();
    let mut authored_paths: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for (sha, values) in locations.iter() {
        let sha = sha.as_str().expect("validated release key");
        for location in values
            .as_sequence()
            .expect("validated locations")
            .iter()
            .filter_map(Value::as_mapping)
        {
            let (Some(storage_id), Some(path)) = (
                mapping_string(location, "storage"),
                mapping_string(location, "path"),
            ) else {
                continue;
            };
            let owned = location_owned(private, sha, location);
            paths
                .entry((storage_id.clone(), path.clone()))
                .or_default()
                .entry(sha.to_owned())
                .and_modify(|all_owned| *all_owned &= owned)
                .or_insert(owned);
            if !owned {
                let target = config::resolver::ResolvedFileTarget { storage_id, path };
                if let Ok(file) = config::storage::resolve_file_target(root, &snapshot, &target) {
                    authored_paths
                        .entry(file.path)
                        .or_default()
                        .insert(sha.to_owned());
                }
            }
        }
    }
    let mut replaced = BTreeSet::new();
    let releases = mapping_at(&mut releases_doc, "releases")?;
    let games = mapping_at(&mut games_doc, "games")?;
    for candidate in candidates {
        let sha = &candidate.hash;
        let key = (
            candidate.storage_id.clone(),
            candidate.relative_path.clone(),
        );
        let entries = paths.entry(key.clone()).or_default();
        if entries
            .iter()
            .any(|(existing, owned)| existing != sha && !owned)
            || authored_paths
                .get(&candidate.canonical_path)
                .is_some_and(|hashes| hashes.iter().any(|existing| existing != sha))
        {
            continue;
        }
        // Defer removals to one pass. Catalog facts survive byte replacement.
        entries.retain(|existing, owned| {
            if existing != sha && *owned {
                replaced.insert((existing.clone(), key.clone()));
                false
            } else {
                true
            }
        });
        if !releases.contains_key(sha.as_str()) {
            let id = mint_game_id(games);
            let game = Mapping::from_iter([
                ("title".into(), candidate.title.clone().into()),
                ("releases".into(), Value::Sequence(vec![sha.clone().into()])),
            ]);
            let release = Mapping::from_iter([
                ("game".into(), id.clone().into()),
                ("system".into(), candidate.system.clone().into()),
                ("identity".into(), "file".into()),
            ]);
            let fingerprint = revision(&format!(
                "{}\n{}",
                fingerprint_mapping(&game),
                fingerprint_mapping(&release)
            ));
            games.insert(id.clone().into(), Value::Mapping(game));
            releases.insert(sha.clone().into(), Value::Mapping(release));
            private.repair.pending_ownership.insert(
                ownership_key(&id, sha),
                OwnedRelease {
                    playable_id: id.clone(),
                    release_id: sha.clone(),
                    fingerprint,
                },
            );
            stats.added_games += 1;
        }
        let values = locations
            .entry(Value::String(sha.clone()))
            .or_insert_with(|| Value::Sequence(Vec::new()))
            .as_sequence_mut()
            .expect("validated locations");
        if entries.contains_key(sha) {
            continue;
        }
        entries.insert(sha.clone(), true);
        let location = generated_location(candidate, &options.first_seen_at);
        let fingerprint = fingerprint_mapping(&location);
        private.repair.pending_locations.insert(
            location_key(sha, &location),
            OwnedLocation {
                release_id: sha.clone(),
                fingerprint,
            },
        );
        values.push(Value::Mapping(location));
    }
    for (sha, values) in locations.iter_mut() {
        let sha = sha.as_str().expect("validated release key");
        values
            .as_sequence_mut()
            .expect("validated locations")
            .retain(|value| {
                let Some(location) = value.as_mapping() else {
                    return true;
                };
                let (Some(storage), Some(path)) = (
                    mapping_string(location, "storage"),
                    mapping_string(location, "path"),
                ) else {
                    return true;
                };
                let remove = replaced.contains(&(sha.to_owned(), (storage, path)));
                stats.removed_locations += usize::from(remove);
                !remove
            });
    }
    locations.retain(|_, values| !values.as_sequence().is_some_and(Vec::is_empty));
    // Do not rewrite an unchanged document (including comments and ordering).
    let candidate = Documents {
        device: preserve_unchanged(&current.device, device_doc)?,
        games: preserve_unchanged(&current.games, games_doc)?,
        releases: preserve_unchanged(&current.releases, releases_doc)?,
    };
    stats.changed = &candidate != current;
    current.commit(candidate, root, private_root, private)?;
    Ok(stats)
}

fn preserve_unchanged(original: &str, document: Mapping) -> Result<String, DiscoveryError> {
    if parse_mapping(original)? == document {
        Ok(original.into())
    } else {
        serialize_mapping(document)
    }
}

fn location_key(sha: &str, location: &Mapping) -> String {
    format!(
        "{sha}\n{}\n{}",
        mapping_string(location, "storage").unwrap_or_default(),
        mapping_string(location, "path").unwrap_or_default()
    )
}

fn location_owned(private: &PrivateState, sha: &str, location: &Mapping) -> bool {
    private
        .ownership
        .locations
        .get(&location_key(sha, location))
        .is_some_and(|owned| owned.fingerprint == fingerprint_mapping(location))
}

fn generated_location(candidate: &ScanCandidate, first_seen_at: &str) -> Mapping {
    Mapping::from_iter([
        ("storage".into(), candidate.storage_id.clone().into()),
        ("path".into(), candidate.relative_path.clone().into()),
        (
            "discovery".into(),
            Value::Mapping(Mapping::from_iter([(
                "first-seen-at".into(),
                first_seen_at.into(),
            )])),
        ),
    ])
}

fn mint_game_id(games: &Mapping) -> String {
    // ULID: 48-bit Unix milliseconds followed by 80 random bits, encoded in
    // Crockford base32. rand already supplies the OS-seeded discovery RNG.
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    loop {
        let milliseconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before Unix epoch")
            .as_millis();
        let mut value = ((milliseconds & ((1u128 << 48) - 1)) << 80)
            | (rand::random::<u128>() & ((1u128 << 80) - 1));
        let mut encoded = [b'0'; 26];
        for byte in encoded.iter_mut().rev() {
            *byte = ALPHABET[(value & 31) as usize];
            value >>= 5;
        }
        let id = String::from_utf8(encoded.to_vec()).expect("ULID ASCII");
        if !games.contains_key(id.as_str()) {
            return id;
        }
    }
}

fn cleanup_removed_storages(
    root: &Path,
    private_root: &Path,
    current: &Documents,
    private: &mut PrivateState,
) -> Result<ReconcileStats, DiscoveryError> {
    current.validate()?;
    let mut document = parse_mapping(&current.device)?;
    let mut stats = ReconcileStats::default();
    let removals = private.repair.pending_removals.clone();
    if let Some(locations) = document
        .get_mut("locations")
        .and_then(Value::as_mapping_mut)
    {
        for (sha, values) in locations.iter_mut() {
            if let Some(values) = values.as_sequence_mut() {
                let before = values.len();
                values.retain(|value| {
                    !value.as_mapping().is_some_and(|location| {
                        mapping_string(location, "storage")
                            .is_some_and(|storage| removals.contains(&storage))
                            && location_owned(private, sha.as_str().unwrap_or_default(), location)
                    })
                });
                stats.removed_locations += before - values.len();
            }
        }
        locations.retain(|_, values| !values.as_sequence().is_some_and(Vec::is_empty));
    }
    for storage_id in &removals {
        private.storage_order.retain(|id| id != storage_id);
        if private
            .storage_is_owned_current(storage_id, storage_record(&document, storage_id).as_ref())
            && !locations_reference_storage(&document, storage_id)
        {
            remove_storage_record(&mut document, storage_id)?;
        }
    }
    let candidate = Documents {
        device: preserve_unchanged(&current.device, document)?,
        ..current.clone()
    };
    stats.changed = &candidate != current;
    current.commit(candidate, root, private_root, private)?;
    Ok(stats)
}

fn locations_reference_storage(document: &Mapping, storage_id: &str) -> bool {
    document
        .get("locations")
        .and_then(Value::as_mapping)
        .into_iter()
        .flat_map(|locations| locations.values())
        .filter_map(Value::as_sequence)
        .flatten()
        .filter_map(Value::as_mapping)
        .any(|location| mapping_string(location, "storage").as_deref() == Some(storage_id))
}

fn append_dedupe_diagnostics(scan: &mut ScanReport, max_diagnostics: usize) {
    let mut canonical_seen = BTreeSet::new();
    let mut hash_seen = BTreeSet::new();
    let mut pending = Vec::new();
    for candidate in &scan.candidates {
        if !canonical_seen.insert(candidate.canonical_path.clone()) {
            pending.push(DiscoveryDiagnostic {
                code: DiscoveryDiagnosticCode::ClaimConflict,
                storage_id: Some(candidate.storage_id.clone()),
                path: Some(candidate.relative_path.clone()),
                message:
                    "file overlaps an earlier selected folder and both locations were recorded"
                        .into(),
            });
        } else if !hash_seen.insert(candidate.hash.clone()) {
            pending.push(DiscoveryDiagnostic {
                code: DiscoveryDiagnosticCode::ClaimConflict,
                storage_id: Some(candidate.storage_id.clone()),
                path: Some(candidate.relative_path.clone()),
                message:
                    "file content duplicates an earlier discovered game and both locations were recorded"
                        .into(),
            });
        }
    }
    for diagnostic in pending {
        if scan.diagnostics.len() >= max_diagnostics {
            if !scan
                .diagnostics
                .iter()
                .any(|existing| existing.code == DiscoveryDiagnosticCode::DiagnosticLimitReached)
            {
                scan.diagnostics.push(DiscoveryDiagnostic {
                    code: DiscoveryDiagnosticCode::DiagnosticLimitReached,
                    storage_id: None,
                    path: None,
                    message: "additional discovery diagnostics were omitted".into(),
                });
            }
            break;
        }
        scan.diagnostics.push(diagnostic);
    }
}

fn ordered_owned_storage_summaries(
    config_doc: &Mapping,
    private: &PrivateState,
) -> Vec<(String, String)> {
    let mut storages = Vec::new();
    for storage_id in &private.storage_order {
        let record = storage_record(config_doc, storage_id);
        if !private.storage_is_owned_current(storage_id, record.as_ref()) {
            continue;
        }
        let Some(root) = record
            .as_ref()
            .and_then(|record| record.get(Value::String("root".into())))
            .and_then(Value::as_str)
        else {
            continue;
        };
        storages.push((storage_id.clone(), root.to_owned()));
    }
    storages
}

fn ordered_storages(
    snapshot: &config::ConfigSnapshot,
    config_doc: &Mapping,
    private: &PrivateState,
) -> Vec<(String, PathBuf)> {
    let mut storages = Vec::new();
    for storage_id in &private.storage_order {
        let record = storage_record(config_doc, storage_id);
        if !private.storage_is_owned_current(storage_id, record.as_ref()) {
            continue;
        }
        if let Some(storage) = snapshot.storage.get(storage_id) {
            storages.push((storage_id.clone(), PathBuf::from(&storage.root.0)));
        }
    }
    storages
}

fn storage_id_for_root(root: &Path, config: &Mapping, private: &PrivateState) -> String {
    let storage = config
        .get(Value::String("storage".into()))
        .and_then(Value::as_mapping);
    if let Some(storage) = storage {
        for (key, value) in storage {
            let Some(key) = key.as_str() else {
                continue;
            };
            let Some(record) = value.as_mapping() else {
                continue;
            };
            if record
                .get(Value::String("root".into()))
                .and_then(Value::as_str)
                .is_some_and(|value| Path::new(value).canonicalize().ok().as_deref() == Some(root))
                && private.storage_is_owned_current(key, Some(record))
            {
                return key.to_owned();
            }
        }
    }
    let digest = hex::encode(Sha256::digest(root.to_string_lossy().as_bytes()));
    let base = format!("game-folder-{}", &digest[..12]);
    for suffix in 0..1000u32 {
        let candidate = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        let exists_in_config =
            storage.is_some_and(|storage| storage.contains_key(Value::String(candidate.clone())));
        let exists_private = private.storage_order.iter().any(|id| id == &candidate);
        if !exists_in_config && !exists_private {
            return candidate;
        }
    }
    format!("game-folder-{}", &digest[..24])
}

fn set_storage_record(
    document: &mut Mapping,
    storage_id: &str,
    root: &Path,
) -> Result<Mapping, DiscoveryError> {
    let storage = mapping_at(document, "storage")?;
    let mut record = Mapping::new();
    record.insert(
        Value::String("root".into()),
        Value::String(root.to_string_lossy().into_owned()),
    );
    storage.insert(
        Value::String(storage_id.into()),
        Value::Mapping(record.clone()),
    );
    Ok(record)
}

fn remove_storage_record(document: &mut Mapping, storage_id: &str) -> Result<(), DiscoveryError> {
    if let Some(storage) = document
        .get_mut(Value::String("storage".into()))
        .and_then(Value::as_mapping_mut)
    {
        storage.remove(Value::String(storage_id.into()));
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, DiscoveryError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| DiscoveryError::Invalid("selected folder is unavailable".into()))?;
    if !canonical.is_dir() {
        return Err(DiscoveryError::Invalid(
            "selected folder is not a directory".into(),
        ));
    }
    Ok(canonical)
}

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

fn mapping_at<'a>(parent: &'a mut Mapping, key: &str) -> Result<&'a mut Mapping, DiscoveryError> {
    let key = Value::String(key.into());
    if !parent.contains_key(&key) {
        parent.insert(key.clone(), Value::Mapping(Mapping::new()));
    }
    parent
        .get_mut(&key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| DiscoveryError::Invalid(format!("{key:?} must be a record")))
}

fn read_fixed(root: &Path, name: &str) -> Result<String, DiscoveryError> {
    fs::read_to_string(root.join(name)).map_err(|error| DiscoveryError::Storage(error.to_string()))
}

fn apply_pending_ownership(
    documents: &Documents,
    private: &mut PrivateState,
) -> Result<bool, DiscoveryError> {
    let games = parse_mapping(&documents.games)?;
    let releases = parse_mapping(&documents.releases)?;
    let device = parse_mapping(&documents.device)?;
    let before = private.ownership.clone();
    let before_storage = private.storage_ownership.clone();
    let mut locations_by_key: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if let Some(locations) = device.get("locations").and_then(Value::as_mapping) {
        for (release_id, values) in locations {
            let (Some(release_id), Some(values)) = (release_id.as_str(), values.as_sequence())
            else {
                continue;
            };
            for location in values.iter().filter_map(Value::as_mapping) {
                locations_by_key
                    .entry(location_key(release_id, location))
                    .or_default()
                    .insert(fingerprint_mapping(location));
            }
        }
    }
    private
        .ownership
        .locations
        .retain(|key, _| locations_by_key.contains_key(key));
    private
        .storage_ownership
        .storages
        .retain(|id, _| storage_record(&device, id).is_some());
    let changed = before != private.ownership
        || before_storage != private.storage_ownership
        || !private.repair.pending_ownership.is_empty()
        || !private.repair.pending_locations.is_empty();
    for (key, owned) in std::mem::take(&mut private.repair.pending_ownership) {
        if catalog_fingerprint(&games, &releases, &owned.playable_id, &owned.release_id).as_ref()
            == Some(&owned.fingerprint)
        {
            private.ownership.releases.insert(key, owned);
        }
    }
    for (key, owned) in std::mem::take(&mut private.repair.pending_locations) {
        let matches = locations_by_key
            .get(&key)
            .is_some_and(|fingerprints| fingerprints.contains(&owned.fingerprint));
        if matches {
            private.ownership.locations.insert(key, owned);
        }
    }
    Ok(changed)
}

fn storage_record(document: &Mapping, storage_id: &str) -> Option<Mapping> {
    document
        .get(Value::String("storage".into()))
        .and_then(Value::as_mapping)
        .and_then(|storage| storage.get(Value::String(storage_id.into())))
        .and_then(Value::as_mapping)
        .cloned()
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
    fs::create_dir_all(parent).map_err(|error| DiscoveryError::Storage(error.to_string()))?;
    let content =
        serde_json::to_vec(value).map_err(|error| DiscoveryError::Candidate(error.to_string()))?;
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

fn mapping_string(mapping: &Mapping, key: &str) -> Option<String> {
    mapping
        .get(Value::String(key.into()))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn fingerprint_mapping(mapping: &Mapping) -> String {
    let bytes = serde_yaml::to_string(&Value::Mapping(mapping.clone())).unwrap_or_default();
    format!("sha256:{}", hex::encode(Sha256::digest(bytes.as_bytes())))
}

fn ownership_key(playable_id: &str, release_id: &str) -> String {
    format!("{playable_id}\n{release_id}")
}

#[cfg(test)]
mod tests;
