use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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
    registry_source: crate::plugin_policy::RegistrySource,
    attention_lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverySnapshot {
    pub generation: String,
    pub state: DiscoveryPhase,
    pub sources: Vec<DiscoverySource>,
    pub attention: u32,
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
pub struct DiscoverySource {
    pub id: String,
    pub label: String,
    pub system: String,
    pub status: DiscoverySourceStatus,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoverySourceStatus {
    Ready,
    Overlap,
    SystemUnavailable,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryRequestError {
    #[error(transparent)]
    Receipt(#[from] FolderSelectionGrantError),
    #[error(transparent)]
    Pairing(#[from] crate::discovery::sources::PairingError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error("attention state is unavailable")]
    AttentionStorage,
    #[error("unknown attention item")]
    UnknownItem,
    #[error("system is not a candidate for this item")]
    NotACandidate,
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
    findings: Vec<crate::discovery::attention::AttentionFinding>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LifecycleSnapshotCache {
    fixed_files_revision: Option<FixedFilesRevision>,
    sources: Vec<DiscoverySource>,
    diagnostic: Option<crate::config::snapshot::SnapshotDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FixedFilesRevision {
    device: Option<FileRevision>,
    library: Option<FileRevision>,
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
        .with_registry_source(registry_source.clone());
        let enricher = SteamGridDbEnricher::new(&readable_root, &private_root, write_lock.clone());
        let attention_lock = reconciler.write_lock();
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
            registry_source,
            attention_lock,
        };
        coordinator.refresh_snapshot_cache();
        coordinator.start_work(WorkRequest::Rescan);
        coordinator
    }

    /// The folder-selection receipts `pair` consumes.
    pub fn grants(&self) -> &FolderSelectionGrantStore {
        &self.grants
    }

    pub fn with_options(mut self, options: DiscoveryOptions) -> Self {
        self.options = options;
        self
    }

    pub fn snapshot(&self) -> DiscoverySnapshot {
        self.snapshot_with_state(|_| {})
    }

    pub fn pair(
        &self,
        receipt: &str,
        system: &str,
    ) -> Result<DiscoverySnapshot, DiscoveryRequestError> {
        let root = self.grants.consume(receipt)?;
        self.validate(&root, system, None)?;
        self.start_work(WorkRequest::Pair(root, system.into()));
        Ok(self.snapshot())
    }

    pub fn change(
        &self,
        source: &str,
        system: &str,
    ) -> Result<DiscoverySnapshot, DiscoveryRequestError> {
        let views = self.reconciler.source_views()?;
        let view = views
            .iter()
            .find(|view| view.source_id == source)
            .ok_or_else(|| crate::discovery::sources::PairingError::UnknownSource(source.into()))?;
        self.validate(&view.root, system, Some(source))?;
        self.start_work(WorkRequest::Change(source.into(), system.into()));
        Ok(self.snapshot())
    }

    fn validate(
        &self,
        root: &Path,
        system: &str,
        replacing: Option<&str>,
    ) -> Result<(), DiscoveryRequestError> {
        let registry = self
            .registry_source
            .registry()
            .map_err(|_| DiscoveryError::Candidate("plugin registry is unavailable".into()))?;
        crate::discovery::sources::validate_pairing(
            &crate::discovery::sources::PairingRequest {
                root,
                system,
                readable_root: &self.readable_root,
                private_root: &self.private_root,
                replacing,
            },
            &self.reconciler.source_views()?,
            &registry,
        )?;
        Ok(())
    }

    pub fn unpair(&self, source: &str) -> Result<DiscoverySnapshot, DiscoveryRequestError> {
        if !self
            .reconciler
            .source_views()?
            .iter()
            .any(|view| view.source_id == source)
        {
            return Err(
                crate::discovery::sources::PairingError::UnknownSource(source.into()).into(),
            );
        }
        self.start_work(WorkRequest::Unpair(source.into()));
        Ok(self.snapshot())
    }

    fn valid_pairings(&self) -> Result<BTreeMap<String, String>, DiscoveryRequestError> {
        let accounts = crate::discovery::sources::account_roots(&self.readable_root)
            .map_err(|_| DiscoveryRequestError::AttentionStorage)?;
        Ok(crate::discovery::sources::pairings(
            &self.reconciler.source_views()?,
            &accounts,
        ))
    }

    pub fn attention(
        &self,
    ) -> Result<Vec<crate::discovery::attention::AttentionFinding>, DiscoveryRequestError> {
        self.attention_for(&self.valid_pairings()?)
    }

    fn attention_for(
        &self,
        sources: &BTreeMap<String, String>,
    ) -> Result<Vec<crate::discovery::attention::AttentionFinding>, DiscoveryRequestError> {
        let decisions = crate::discovery::attention::load(&self.private_root)
            .map_err(|_| DiscoveryRequestError::AttentionStorage)?;
        Ok(self
            .state
            .lock()
            .expect("discovery lifecycle state poisoned")
            .findings
            .iter()
            .filter(|item| {
                sources
                    .get(&item.source_id)
                    .is_some_and(|pairing| !decisions.is_ignored(item, pairing))
            })
            .cloned()
            .collect())
    }

    pub fn assign(
        &self,
        item: &str,
        system: &str,
    ) -> Result<Vec<crate::discovery::attention::AttentionFinding>, DiscoveryRequestError> {
        self.decide(item, Some(system))
    }

    pub fn ignore(
        &self,
        item: &str,
    ) -> Result<Vec<crate::discovery::attention::AttentionFinding>, DiscoveryRequestError> {
        self.decide(item, None)
    }

    fn decide(
        &self,
        item: &str,
        system: Option<&str>,
    ) -> Result<Vec<crate::discovery::attention::AttentionFinding>, DiscoveryRequestError> {
        let _guard = self.attention_lock.lock().expect("attention lock poisoned");
        let sources = self.valid_pairings()?;
        let finding = self
            .attention_for(&sources)?
            .into_iter()
            .find(|finding| finding.item_id() == item)
            .ok_or(DiscoveryRequestError::UnknownItem)?;
        let mut decisions = crate::discovery::attention::load(&self.private_root)
            .map_err(|_| DiscoveryRequestError::AttentionStorage)?;
        decisions.retain_pairings(&sources);
        let pairing = sources
            .get(&finding.source_id)
            .ok_or(DiscoveryRequestError::UnknownItem)?;
        match system {
            Some(system) => decisions
                .assign(&finding, system, pairing)
                .map_err(|_| DiscoveryRequestError::NotACandidate)?,
            None => decisions.ignore(&finding, pairing),
        }
        crate::discovery::attention::save(&self.private_root, &decisions)
            .map_err(|_| DiscoveryRequestError::AttentionStorage)?;
        {
            let mut state = self
                .state
                .lock()
                .expect("discovery lifecycle state poisoned");
            if system.is_some() {
                state.findings.retain(|finding| finding.item_id() != item);
            }
            bump(&mut state);
        }
        if system.is_some() {
            self.start_work(WorkRequest::Rescan);
        }
        self.attention_for(&sources)
    }

    pub fn rescan(&self) -> DiscoverySnapshot {
        self.start_work(WorkRequest::Rescan);
        self.snapshot()
    }

    /// ADR 0016: write an acquired game's library game, release and provider link.
    pub fn record_acquired(
        &self,
        acquired: &crate::discovery::reconcile::AcquiredGame<'_>,
    ) -> Result<String, DiscoveryError> {
        self.reconciler.record_acquired(acquired)
    }

    pub(crate) fn record_copied_release(
        &self,
        title: &str,
        system: &str,
        identity: &crate::discovery::identify::ContentIdentity,
    ) -> Result<String, DiscoveryError> {
        self.reconciler
            .record_copied_release(title, system, identity)
    }
    /// The published library and device documents, read fresh.
    pub fn published(&self) -> Result<crate::config::ConfigSnapshot, DiscoveryError> {
        self.reconciler.published()
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
                WorkRequest::Pair(ref path, ref system) => {
                    self.reconciler.pair(path.clone(), system, &self.options)
                }
                WorkRequest::Change(ref id, ref system) => {
                    self.reconciler.change(id, system, &self.options)
                }
                WorkRequest::Unpair(ref id) => self.reconciler.unpair(id, &self.options),
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
                drop(state);
                self.refresh_snapshot_cache();
                let mut state = self
                    .state
                    .lock()
                    .expect("discovery lifecycle state poisoned");
                state.findings = crate::discovery::scanner::attention_findings(&report.scan);
                state.diagnostics = report
                    .scan
                    .diagnostics
                    .iter()
                    .chain(
                        report
                            .scan
                            .scanned
                            .iter()
                            .flat_map(|scan| scan.diagnostics.iter()),
                    )
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
        let pairings = self.valid_pairings().unwrap_or_default();
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
            sources: cache.sources,
            attention: crate::discovery::attention::load(&self.private_root)
                .map(|decisions| {
                    state
                        .findings
                        .iter()
                        .filter(|item| {
                            pairings
                                .get(&item.source_id)
                                .is_some_and(|pairing| !decisions.is_ignored(item, pairing))
                        })
                        .count()
                        .min(u32::MAX as usize) as u32
                })
                .unwrap_or(0),
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
        let sources = self
            .reconciler
            .source_views()
            .ok()
            .map(|views| {
                let registry = self.registry_source.registry().ok();
                views
                    .iter()
                    .map(|source| {
                        let overlap = source.canonical_root.as_ref().is_some_and(|root| {
                            views.iter().any(|other| {
                                other.source_id != source.source_id
                                    && other.canonical_root.as_ref().is_some_and(|other| {
                                        crate::discovery::sources::overlaps(root, other)
                                    })
                            })
                        });
                        let available = registry.as_ref().is_some_and(|registry| {
                            registry.pairing_target(&source.system).is_some()
                        });
                        let label = source
                            .title
                            .clone()
                            .or_else(|| {
                                registry.as_ref().and_then(|registry| {
                                    registry
                                        .group(&source.system)
                                        .and_then(|group| group.title.clone())
                                        .or_else(|| {
                                            registry
                                                .systems()
                                                .values()
                                                .find(|system| system.id == source.system)
                                                .and_then(|system| system.title.clone())
                                        })
                                })
                            })
                            .unwrap_or_else(|| source.system.clone());
                        DiscoverySource {
                            id: source.source_id.clone(),
                            label,
                            system: source.system.clone(),
                            status: if overlap {
                                DiscoverySourceStatus::Overlap
                            } else if !available {
                                DiscoverySourceStatus::SystemUnavailable
                            } else {
                                DiscoverySourceStatus::Ready
                            },
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut cache = self
            .snapshot_cache
            .lock()
            .expect("discovery snapshot cache poisoned");
        *cache = LifecycleSnapshotCache {
            fixed_files_revision: Some(fixed_files_revision(&self.readable_root)),
            sources,
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
        library: file_revision(&root.join(crate::config::library::LIBRARY_FILE_NAME)),
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
    Pair(PathBuf, String),
    Change(String, String),
    Unpair(String),
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
        DiscoveryError::Pairing(_) => "DiscoveryPairingInvalid",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

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
    fn attention_ignore_uses_private_atomic_storage_and_updates_the_generation() {
        use crate::discovery::attention::{AttentionFinding, AttentionKind};
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        std::fs::write(readable.path().join("device.yaml"), format!("storage:\n  gb:\n    root: {}\nsources:\n  gb:\n    kind: [files]\n    storage: gb\n    system: gb\n", readable.path().join("games").display())).unwrap();
        let item = AttentionFinding {
            kind: AttentionKind::SystemUnavailable,
            source_id: "gb".into(),
            path: None,
            identity: None,
            candidates: vec![],
            message: "system unavailable".into(),
        };
        let lock = Arc::new(Mutex::new(()));
        let lifecycle = DiscoveryLifecycleCoordinator {
            reconciler: ReconcileCoordinator::with_write_lock(
                readable.path(),
                private.path(),
                lock.clone(),
            ),
            enricher: SteamGridDbEnricher::new(readable.path(), private.path(), lock.clone()),
            readable_root: readable.path().into(),
            private_root: private.path().into(),
            snapshot_reader: ConfigSnapshotCoordinator::new(readable.path()),
            grants: FolderSelectionGrantStore::default(),
            options: DiscoveryOptions::default(),
            state: Arc::new(Mutex::new(LifecycleState {
                findings: vec![item.clone()],
                ..LifecycleState::default()
            })),
            snapshot_cache: Arc::new(Mutex::new(LifecycleSnapshotCache::default())),
            registry_source: crate::plugin_policy::RegistrySource::Installed,
            attention_lock: lock,
        };
        assert_eq!(lifecycle.attention().unwrap(), vec![item.clone()]);
        assert!(lifecycle.ignore(&item.item_id()).unwrap().is_empty());
        assert!(lifecycle.attention().unwrap().is_empty());
        assert!(crate::discovery::attention::load(private.path())
            .unwrap()
            .is_ignored(
                &item,
                &crate::discovery::sources::pairing_key("gb", &readable.path().join("games"))
            ));
        assert_eq!(lifecycle.state.lock().unwrap().generation, 1);
        assert!(matches!(
            lifecycle.ignore("stale"),
            Err(DiscoveryRequestError::UnknownItem)
        ));
    }

    #[test]
    fn work_queue_coalesces_rescans_without_dropping_pairing_changes() {
        let mut state = LifecycleState::default();
        enqueue_work(&mut state, WorkRequest::Rescan);
        enqueue_work(
            &mut state,
            WorkRequest::Pair(PathBuf::from("/games"), "gb".into()),
        );
        enqueue_work(&mut state, WorkRequest::Change("gb".into(), "gbc".into()));
        enqueue_work(&mut state, WorkRequest::Unpair("gb".into()));
        enqueue_work(&mut state, WorkRequest::Rescan);
        assert_eq!(state.work_queue.len(), 4);
        assert_eq!(state.work_queue.pop_front(), Some(WorkRequest::Rescan));
    }

    #[test]
    fn startup_pair_change_unpair_and_rescan_run_discovery() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
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
        assert_eq!(
            lifecycle.wait_until_idle(Duration::from_secs(5)).state,
            DiscoveryPhase::Idle
        );
        lifecycle
            .pair(
                &grants.issue_approved_path(folder.path()).unwrap().token,
                "gba",
            )
            .unwrap();
        let snapshot = lifecycle.wait_until_idle(Duration::from_secs(5));
        assert_eq!(snapshot.sources.len(), 1);
        let id = &snapshot.sources[0].id;
        lifecycle.change(id, "gba").unwrap();
        assert_eq!(
            lifecycle.wait_until_idle(Duration::from_secs(5)).state,
            DiscoveryPhase::Idle
        );
        lifecycle.rescan();
        assert_eq!(
            lifecycle.wait_until_idle(Duration::from_secs(5)).state,
            DiscoveryPhase::Idle
        );
        lifecycle.unpair(id).unwrap();
        assert!(lifecycle
            .wait_until_idle(Duration::from_secs(5))
            .sources
            .is_empty());
    }
}
