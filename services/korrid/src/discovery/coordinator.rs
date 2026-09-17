use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[cfg(target_os = "android")]
use std::ffi::CString;

use crate::{
    config::snapshot::ConfigSnapshotCoordinator,
    discovery::{
        reconcile::{DiscoveryCoordinator as ReconcileCoordinator, DiscoveryMutationReport},
        scanner::{DiscoveryDiagnostic, DiscoveryDiagnosticCode},
        DiscoveryError, DiscoveryOptions,
    },
    enrichment::SteamGridDbEnricher,
};
use rand::RngCore;

const DEFAULT_GRANT_TTL: Duration = Duration::from_secs(5 * 60);
const DEFAULT_MAX_SNAPSHOT_DIAGNOSTICS: usize = 50;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderSelectionGrant {
    pub token: String,
    pub canonical_path: PathBuf,
}

#[derive(Clone)]
pub struct FolderSelectionGrantStore {
    inner: Arc<Mutex<GrantStoreInner>>,
    ttl: Duration,
}

#[derive(Debug, Default)]
struct GrantStoreInner {
    grants: BTreeMap<String, StoredGrant>,
}

#[derive(Clone, Debug)]
struct StoredGrant {
    canonical_path: PathBuf,
    expires_at: Instant,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum FolderSelectionGrantError {
    #[error("selected folder is not available: {0}")]
    InvalidPath(String),
    #[error("folder selection receipt is unknown or has already been used")]
    Unknown,
    #[error("folder selection receipt has expired")]
    Expired,
}

impl Default for FolderSelectionGrantStore {
    fn default() -> Self {
        Self::new(DEFAULT_GRANT_TTL)
    }
}

impl FolderSelectionGrantStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(GrantStoreInner::default())),
            ttl,
        }
    }

    pub fn issue_approved_path(
        &self,
        selected_root: impl AsRef<Path>,
    ) -> Result<FolderSelectionGrant, FolderSelectionGrantError> {
        let canonical_path = canonical_directory(selected_root.as_ref())?;
        let token = random_token();
        let expires_at = Instant::now() + self.ttl;
        self.inner
            .lock()
            .expect("folder selection grant store poisoned")
            .grants
            .insert(
                token.clone(),
                StoredGrant {
                    canonical_path: canonical_path.clone(),
                    expires_at,
                },
            );
        Ok(FolderSelectionGrant {
            token,
            canonical_path,
        })
    }

    pub fn consume(&self, token: &str) -> Result<PathBuf, FolderSelectionGrantError> {
        self.consume_when(token, |_| true)
    }

    fn consume_when(
        &self,
        token: &str,
        accept: impl FnOnce(&Path) -> bool,
    ) -> Result<PathBuf, FolderSelectionGrantError> {
        let mut inner = self
            .inner
            .lock()
            .expect("folder selection grant store poisoned");
        let Some(grant) = inner.grants.get(token).cloned() else {
            prune_expired(&mut inner);
            return Err(FolderSelectionGrantError::Unknown);
        };
        if Instant::now() > grant.expires_at {
            inner.grants.remove(token);
            return Err(FolderSelectionGrantError::Expired);
        }
        if accept(&grant.canonical_path) {
            inner.grants.remove(token);
        }
        Ok(grant.canonical_path)
    }
}

fn prune_expired(inner: &mut GrantStoreInner) {
    let now = Instant::now();
    inner.grants.retain(|_, grant| grant.expires_at >= now);
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn canonical_directory(path: &Path) -> Result<PathBuf, FolderSelectionGrantError> {
    let canonical = path
        .canonicalize()
        .map_err(|error| FolderSelectionGrantError::InvalidPath(error.to_string()))?;
    if !canonical.is_dir() {
        return Err(FolderSelectionGrantError::InvalidPath(
            "selected path is not a directory".into(),
        ));
    }
    Ok(canonical)
}

#[derive(Clone)]
pub struct DiscoveryLifecycleCoordinator {
    reconciler: ReconcileCoordinator,
    enricher: SteamGridDbEnricher,
    readable_root: PathBuf,
    private_root: PathBuf,
    snapshot_reader: ConfigSnapshotCoordinator,
    grants: FolderSelectionGrantStore,
    options: DiscoveryOptions,
    state: Arc<Mutex<LifecycleState>>,
    snapshot_cache: Arc<Mutex<LifecycleSnapshotCache>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverySnapshot {
    pub generation: String,
    pub state: DiscoveryPhase,
    pub locations: Vec<DiscoveryLocationSummary>,
    pub diagnostics: Vec<DiscoveryLifecycleDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryPhase {
    Idle,
    Scanning,
    Enriching,
    Problem,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryLocationSummary {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryLifecycleDiagnostic {
    pub code: String,
    pub message: String,
    pub location_id: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LifecycleState {
    generation: u64,
    active: bool,
    enriching: bool,
    work_queue: VecDeque<WorkRequest>,
    last_problem: Option<DiscoveryLifecycleDiagnostic>,
    diagnostics: Vec<DiscoveryLifecycleDiagnostic>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LifecycleSnapshotCache {
    fixed_files_revision: Option<FixedFilesRevision>,
    locations: Vec<DiscoveryLocationSummary>,
    diagnostic: Option<crate::config::snapshot::SnapshotDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FixedFilesRevision {
    device: Option<FileRevision>,
    games: Option<FileRevision>,
    releases: Option<FileRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileRevision {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

impl DiscoveryLifecycleCoordinator {
    pub fn new(
        readable_root: impl AsRef<Path>,
        private_root: impl AsRef<Path>,
        write_lock: Arc<Mutex<()>>,
        grants: FolderSelectionGrantStore,
    ) -> Self {
        Self::new_with_registry_source(
            readable_root,
            private_root,
            write_lock,
            grants,
            crate::plugin_policy::RegistrySource::Installed,
        )
    }

    pub fn new_with_registry_source(
        readable_root: impl AsRef<Path>,
        private_root: impl AsRef<Path>,
        write_lock: Arc<Mutex<()>>,
        grants: FolderSelectionGrantStore,
        registry_source: crate::plugin_policy::RegistrySource,
    ) -> Self {
        let readable_root = readable_root.as_ref().to_owned();
        let private_root = private_root.as_ref().to_owned();
        let reconciler = ReconcileCoordinator::with_write_lock(
            &readable_root,
            &private_root,
            write_lock.clone(),
        )
        .with_registry_source(registry_source);
        let enricher = SteamGridDbEnricher::new(&readable_root, &private_root, write_lock);
        let snapshot_reader = ConfigSnapshotCoordinator::new(&readable_root);
        snapshot_reader.reload();
        let coordinator = Self {
            readable_root,
            private_root,
            snapshot_reader,
            reconciler,
            enricher,
            grants,
            options: DiscoveryOptions::default(),
            state: Arc::new(Mutex::new(LifecycleState::default())),
            snapshot_cache: Arc::new(Mutex::new(LifecycleSnapshotCache::default())),
        };
        coordinator.refresh_snapshot_cache();
        if coordinator.reconciler.has_recovery_work() {
            coordinator.start_work(WorkRequest::Rescan);
        }
        coordinator
    }

    pub fn with_options(mut self, options: DiscoveryOptions) -> Self {
        self.options = options;
        self
    }

    pub fn snapshot(&self) -> DiscoverySnapshot {
        self.snapshot_with_state(|_| {})
    }

    pub fn register_receipt(
        &self,
        receipt: &str,
    ) -> Result<DiscoverySnapshot, FolderSelectionGrantError> {
        self.grants.consume_when(receipt, |path| {
            self.start_work(WorkRequest::AddLocation(path.to_owned()));
            true
        })?;
        Ok(self.snapshot())
    }

    #[cfg(test)]
    pub fn register_path_for_test(&self, path: impl AsRef<Path>) -> DiscoverySnapshot {
        self.start_work(WorkRequest::AddLocation(path.as_ref().to_owned()));
        self.snapshot()
    }

    pub fn remove_location(&self, location_id: String) -> DiscoverySnapshot {
        self.start_work(WorkRequest::RemoveLocation(location_id));
        self.snapshot()
    }

    pub fn rescan(&self) -> DiscoverySnapshot {
        self.start_work(WorkRequest::Rescan);
        self.snapshot()
    }

    #[cfg(test)]
    pub fn wait_until_idle(&self, timeout: Duration) -> DiscoverySnapshot {
        let deadline = Instant::now() + timeout;
        loop {
            let snapshot = self.snapshot();
            if !matches!(
                snapshot.state,
                DiscoveryPhase::Scanning | DiscoveryPhase::Enriching
            ) {
                return snapshot;
            }
            if Instant::now() >= deadline {
                return snapshot;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn start_work(&self, request: WorkRequest) {
        let mut state = self
            .state
            .lock()
            .expect("discovery lifecycle state poisoned");
        if state.active {
            enqueue_work(&mut state, request);
            bump(&mut state);
            return;
        }
        state.active = true;
        state.last_problem = None;
        bump(&mut state);
        drop(state);

        let this = self.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("korrid-discovery".into())
            .spawn(move || this.run_work_loop(request))
        {
            self.record_spawn_failure(error);
        }
    }

    fn run_work_loop(&self, mut request: WorkRequest) {
        loop {
            let result = match request {
                WorkRequest::AddLocation(ref path) => {
                    self.reconciler.add_location(path, &self.options)
                }
                WorkRequest::RemoveLocation(ref id) => {
                    self.reconciler.remove_location(id, &self.options)
                }
                WorkRequest::Rescan => {
                    let _ = self.enricher.clear_retryable_attempts();
                    self.reconciler.rescan(&self.options)
                }
            };
            let scan_succeeded = self.record_result(result);
            if scan_succeeded {
                self.run_enrichment();
            }

            let mut state = self
                .state
                .lock()
                .expect("discovery lifecycle state poisoned");
            if let Some(next) = state.work_queue.pop_front() {
                state.active = true;
                bump(&mut state);
                request = next;
                continue;
            }
            state.active = false;
            bump(&mut state);
            break;
        }
    }

    fn record_spawn_failure(&self, error: std::io::Error) {
        let mut state = self
            .state
            .lock()
            .expect("discovery lifecycle state poisoned");
        state.active = false;
        state.enriching = false;
        let diagnostic = DiscoveryLifecycleDiagnostic {
            code: "DiscoveryWorkerSpawnFailed".into(),
            message: format!("discovery worker could not be started: {error}"),
            location_id: None,
        };
        state.diagnostics = vec![diagnostic.clone()];
        state.last_problem = Some(diagnostic);
        bump(&mut state);
    }

    fn record_result(&self, result: Result<DiscoveryMutationReport, DiscoveryError>) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("discovery lifecycle state poisoned");
        match result {
            Ok(report) => {
                emit_scan_metric(&report);
                drop(state);
                self.refresh_snapshot_cache();
                let mut state = self
                    .state
                    .lock()
                    .expect("discovery lifecycle state poisoned");
                state.diagnostics = report
                    .scan
                    .diagnostics
                    .iter()
                    .take(DEFAULT_MAX_SNAPSHOT_DIAGNOSTICS)
                    .map(discovery_diagnostic)
                    .collect();
                state.last_problem = None;
                bump(&mut state);
                return true;
            }
            Err(error) => {
                let diagnostic = discovery_error_diagnostic(error);
                state.diagnostics = vec![diagnostic.clone()];
                state.last_problem = Some(diagnostic);
            }
        }
        bump(&mut state);
        false
    }

    fn run_enrichment(&self) {
        {
            let mut state = self
                .state
                .lock()
                .expect("discovery lifecycle state poisoned");
            state.enriching = true;
            bump(&mut state);
        }
        let report = self.enricher.run();
        let diagnostics = report.diagnostics;
        let mut state = self
            .state
            .lock()
            .expect("discovery lifecycle state poisoned");
        state.enriching = false;
        for diagnostic in diagnostics
            .into_iter()
            .take(DEFAULT_MAX_SNAPSHOT_DIAGNOSTICS)
        {
            state.diagnostics.push(DiscoveryLifecycleDiagnostic {
                code: diagnostic.code.into(),
                message: diagnostic.message,
                location_id: diagnostic.playable_id,
            });
        }
        state.diagnostics.truncate(DEFAULT_MAX_SNAPSHOT_DIAGNOSTICS);
        bump(&mut state);
    }

    fn snapshot_with_state(&self, mutate: impl FnOnce(&mut LifecycleState)) -> DiscoverySnapshot {
        self.refresh_snapshot_cache_if_stale();
        let mut state = self
            .state
            .lock()
            .expect("discovery lifecycle state poisoned");
        mutate(&mut state);
        let generation = state.generation;
        let active = state.active;
        let enriching = state.enriching;
        let cache = self
            .snapshot_cache
            .lock()
            .expect("discovery snapshot cache poisoned")
            .clone();
        let diagnostics = bounded_diagnostics(&state, &cache.diagnostic);
        let phase = if active && !enriching {
            DiscoveryPhase::Scanning
        } else if enriching {
            DiscoveryPhase::Enriching
        } else if state.last_problem.is_some() {
            DiscoveryPhase::Problem
        } else {
            DiscoveryPhase::Idle
        };
        DiscoverySnapshot {
            generation: format!("discovery-{generation}"),
            state: phase,
            locations: cache.locations,
            diagnostics,
        }
    }

    fn refresh_snapshot_cache_if_stale(&self) {
        let revision = fixed_files_revision(&self.readable_root);
        let is_stale = self
            .snapshot_cache
            .lock()
            .expect("discovery snapshot cache poisoned")
            .fixed_files_revision
            .as_ref()
            != Some(&revision);
        if is_stale {
            self.refresh_snapshot_cache();
        }
    }

    fn refresh_snapshot_cache(&self) {
        let config_state = self.snapshot_reader.reload();
        let locations = match ReconcileCoordinator::owned_location_summaries(
            &self.readable_root,
            &self.private_root,
        ) {
            Ok(locations) => locations
                .into_iter()
                .map(|(id, label)| DiscoveryLocationSummary { id, label })
                .collect(),
            Err(_) => Vec::new(),
        };
        let mut cache = self
            .snapshot_cache
            .lock()
            .expect("discovery snapshot cache poisoned");
        *cache = LifecycleSnapshotCache {
            fixed_files_revision: Some(fixed_files_revision(&self.readable_root)),
            locations,
            diagnostic: config_state.diagnostic,
        };
    }
}

fn bump(state: &mut LifecycleState) {
    state.generation = state.generation.saturating_add(1);
}

fn enqueue_work(state: &mut LifecycleState, request: WorkRequest) {
    if matches!(request, WorkRequest::Rescan)
        && state
            .work_queue
            .iter()
            .any(|queued| matches!(queued, WorkRequest::Rescan))
    {
        return;
    }
    state.work_queue.push_back(request);
}

fn fixed_files_revision(root: &Path) -> FixedFilesRevision {
    FixedFilesRevision {
        device: file_revision(&root.join(crate::config::snapshot::DEVICE_FILE_NAME)),
        games: file_revision(&root.join(crate::config::snapshot::GAMES_FILE_NAME)),
        releases: file_revision(&root.join(crate::config::snapshot::RELEASES_FILE_NAME)),
    }
}

fn file_revision(path: &Path) -> Option<FileRevision> {
    let metadata = fs::metadata(path).ok()?;
    Some(FileRevision {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WorkRequest {
    AddLocation(PathBuf),
    RemoveLocation(String),
    Rescan,
}

fn bounded_diagnostics(
    state: &LifecycleState,
    snapshot_diagnostic: &Option<crate::config::snapshot::SnapshotDiagnostic>,
) -> Vec<DiscoveryLifecycleDiagnostic> {
    let mut diagnostics = state.diagnostics.clone();
    if let Some(diagnostic) = snapshot_diagnostic {
        diagnostics.push(DiscoveryLifecycleDiagnostic {
            code: "LocalConfigReloadFailed".into(),
            message: diagnostic.message.clone(),
            location_id: None,
        });
    }
    diagnostics.truncate(DEFAULT_MAX_SNAPSHOT_DIAGNOSTICS);
    diagnostics
}

fn discovery_diagnostic(diagnostic: &DiscoveryDiagnostic) -> DiscoveryLifecycleDiagnostic {
    DiscoveryLifecycleDiagnostic {
        code: discovery_diagnostic_code(diagnostic.code).into(),
        message: diagnostic.message.clone(),
        location_id: diagnostic.storage_id.clone(),
    }
}

fn discovery_diagnostic_code(code: DiscoveryDiagnosticCode) -> &'static str {
    match code {
        DiscoveryDiagnosticCode::StorageUnavailable => "StorageUnavailable",
        DiscoveryDiagnosticCode::StorageEscaped => "StorageEscaped",
        DiscoveryDiagnosticCode::EntryUnavailable => "EntryUnavailable",
        DiscoveryDiagnosticCode::EntryEscaped => "EntryEscaped",
        DiscoveryDiagnosticCode::EntryUnclaimed => "EntryUnclaimed",
        DiscoveryDiagnosticCode::ClaimConflict => "ClaimConflict",
        DiscoveryDiagnosticCode::HashUnavailable => "HashUnavailable",
        DiscoveryDiagnosticCode::PathUnsupported => "PathUnsupported",
        DiscoveryDiagnosticCode::DiagnosticLimitReached => "DiagnosticLimitReached",
        DiscoveryDiagnosticCode::TraversalLimitReached => "TraversalLimitReached",
    }
}

fn discovery_error_diagnostic(error: DiscoveryError) -> DiscoveryLifecycleDiagnostic {
    let code = match &error {
        DiscoveryError::Conflict => "DiscoveryConflict",
        DiscoveryError::Invalid(_) => "DiscoveryInvalid",
        DiscoveryError::Storage(_) => "DiscoveryStorageUnavailable",
        DiscoveryError::Candidate(_) => "DiscoveryCandidateInvalid",
    };
    DiscoveryLifecycleDiagnostic {
        code: code.into(),
        message: error.to_string(),
        location_id: None,
    }
}

impl From<FolderSelectionGrantError> for DiscoveryLifecycleDiagnostic {
    fn from(error: FolderSelectionGrantError) -> Self {
        let code = match error {
            FolderSelectionGrantError::InvalidPath(_) => "FolderSelectionInvalid",
            FolderSelectionGrantError::Unknown => "FolderSelectionReceiptUnknown",
            FolderSelectionGrantError::Expired => "FolderSelectionReceiptExpired",
        };
        Self {
            code: code.into(),
            message: error.to_string(),
            location_id: None,
        }
    }
}

#[cfg(any(target_os = "android", test))]
fn scan_metric_json(report: &DiscoveryMutationReport) -> String {
    serde_json::json!({
        "schema": "korri.discovery.scanMetrics.v1",
        "durationMs": report.scan_duration_ms,
        "hashedBytes": report.scan.hashed_bytes,
        "candidates": report.scan.candidates.len(),
        "diagnostics": report.scan.diagnostics.len(),
        "addedGames": report.added_games,
        "removedLocations": report.removed_locations,
        "storageId": report.storage_id.as_deref(),
        "repaired": report.repaired,
    })
    .to_string()
}

#[cfg(target_os = "android")]
fn emit_scan_metric(report: &DiscoveryMutationReport) {
    let Ok(tag) = CString::new("KorriDiscovery") else {
        return;
    };
    let Ok(message) = CString::new(scan_metric_json(report)) else {
        return;
    };
    unsafe {
        __android_log_write(4, tag.as_ptr(), message.as_ptr());
    }
}

#[cfg(not(target_os = "android"))]
fn emit_scan_metric(_report: &DiscoveryMutationReport) {}

#[cfg(target_os = "android")]
#[link(name = "log")]
extern "C" {
    fn __android_log_write(
        prio: std::os::raw::c_int,
        tag: *const std::os::raw::c_char,
        text: *const std::os::raw::c_char,
    ) -> std::os::raw::c_int;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, thread};

    #[test]
    fn grants_consume_once_and_expire_without_persisting_paths_in_token() {
        let root = tempfile::tempdir().unwrap();
        let store = FolderSelectionGrantStore::new(Duration::from_millis(20));
        let grant = store.issue_approved_path(root.path()).unwrap();
        assert!(!grant.token.contains(root.path().to_string_lossy().as_ref()));
        assert_eq!(
            store.consume(&grant.token).unwrap(),
            root.path().canonicalize().unwrap()
        );
        assert_eq!(
            store.consume(&grant.token).unwrap_err(),
            FolderSelectionGrantError::Unknown
        );

        let expired = store.issue_approved_path(root.path()).unwrap();
        thread::sleep(Duration::from_millis(30));
        assert_eq!(
            store.consume(&expired.token).unwrap_err(),
            FolderSelectionGrantError::Expired
        );
    }

    #[test]
    fn scan_metric_json_is_structured_and_contains_no_paths() {
        let mut report = DiscoveryMutationReport::default();
        report.scan_duration_ms = 12;
        report.scan.hashed_bytes = 34;
        report.storage_id = Some("selected-1".into());

        let metric = scan_metric_json(&report);
        let json: serde_json::Value = serde_json::from_str(&metric).unwrap();
        assert_eq!(json["schema"], "korri.discovery.scanMetrics.v1");
        assert_eq!(json["durationMs"], 12);
        assert_eq!(json["hashedBytes"], 34);
        assert_eq!(json["storageId"], "selected-1");
        assert!(!metric.contains("/storage/"));
        assert!(!metric.contains("SteamGridDB"));
        assert!(!metric.contains("Bearer"));
    }

    #[test]
    fn lifecycle_registers_receipt_as_scanning_then_idle_with_location() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join("game.gba"), b"rom").unwrap();
        let grants = FolderSelectionGrantStore::default();
        let lifecycle = DiscoveryLifecycleCoordinator::new_with_registry_source(
            readable.path(),
            private.path(),
            Arc::new(Mutex::new(())),
            grants.clone(),
            crate::plugin_policy::RegistrySource::Selected(Arc::new(
                crate::plugin_test_fixtures::installed(readable.path()),
            )),
        );
        let receipt = grants.issue_approved_path(folder.path()).unwrap().token;

        let scanning = lifecycle.register_receipt(&receipt).unwrap();
        assert_eq!(scanning.state, DiscoveryPhase::Scanning);
        assert!(matches!(
            lifecycle.register_receipt(&receipt),
            Err(FolderSelectionGrantError::Unknown)
        ));

        let idle = lifecycle.wait_until_idle(Duration::from_secs(5));
        assert_eq!(idle.state, DiscoveryPhase::Idle);
        assert_eq!(idle.locations.len(), 1);
        assert!(ConfigSnapshotCoordinator::new(readable.path())
            .reload()
            .snapshot
            .games
            .values()
            .any(|game| game.title == "game"));
    }
}
