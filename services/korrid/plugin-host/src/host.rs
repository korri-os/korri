use crate::plugin_admin::{PluginRefusal as Refusal, PluginRefusalCategory as Category};
#[cfg(test)]
mod archive_tests;
mod graph_update;

use crate::{
    archive,
    declaration::validate_id,
    lifecycle::{LifecyclePolicy, ReleaseUpdateReview},
    package::{self, Report},
    provenance::{current_platform, Provenance, SelectionIntent},
    repository::{self, Configuration, SourceUrl},
    selection::{Desired, Receipt, SelectionStore},
    software_cleanup::SoftwareCleanup,
    source_store,
    storage::{self, State, ROOTS, STATE_ROOT},
    unit::Units,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// The host's refusal when a stored approval no longer matches the installed
/// package or the current host policy. The admin socket reports exactly this
/// refusal as needing a new review.
pub const APPROVAL_STALE: &str = "installed package or host policy no longer matches its approval";

/// The installed receipt names exactly this package from exactly this source.
fn same_selection(receipt: &Receipt, report: &Report) -> bool {
    receipt.package == report.package && receipt.provenance == report.provenance
}

#[derive(Clone)]
struct HostPaths {
    roots: PathBuf,
    registry_directory: PathBuf,
    registry_path: PathBuf,
}

impl HostPaths {
    fn production() -> Self {
        Self {
            roots: PathBuf::from(ROOTS),
            registry_directory: PathBuf::from(crate::plugin_installation::REGISTRY_DIRECTORY),
            registry_path: PathBuf::from(crate::plugin_installation::REGISTRY_PATH),
        }
    }
}

pub struct Host {
    nix: PathBuf,
    publishers: package::PublisherBindings,
    units: Units,
    state: State,
    paths: HostPaths,
    lifecycle: LifecyclePolicy,
    owner_uid: u32,
    /// Packages whose contents this host instance has already rehashed. A
    /// host lives for one command under the exclusive lock, so this removes
    /// repeat rehashes inside one install or enable without trusting
    /// anything an earlier command checked.
    rehashed: RefCell<BTreeSet<PathBuf>>,
    /// Only this locked inspection's exact, independently verified candidates.
    candidates: RefCell<BTreeMap<PathBuf, (Provenance, crate::plugin_admin::PluginPublisher)>>,
    resolved: RefCell<BTreeMap<PathBuf, Vec<PathBuf>>>,
    acquiring: RefCell<BTreeSet<PathBuf>>,
    candidate_roots: RefCell<Vec<tempfile::TempDir>>,
    #[cfg(test)]
    test_runtime: Option<TestRuntime>,
    #[cfg(test)]
    archive_upstream: Option<PathBuf>,
}

/// Boot-time checks prepared before restore walks the plugins.
struct BootChecks {
    snapshot: package::StoreSnapshot,
    /// Plugin ID -> (package it was loaded from, its report or load error).
    reports: BTreeMap<String, (PathBuf, Result<Report, Refusal>)>,
}

/// Read-only live/cold restore assessment. This is not another journal.
struct RestorePlan {
    report: Option<Result<Report, Refusal>>,
    authorized: Result<(), Refusal>,
    healthy: bool,
    owned: bool,
}

/// What boot restore did with one state directory.
enum Restored {
    Nothing,
    /// Enabled, checked and already running as approved.
    Running(Receipt, Report),
    /// Enabled and checked, but stopped: the caller starts it.
    Stopped(Receipt, Report),
}

#[cfg(test)]
#[derive(Clone, Default)]
struct TestRuntime {
    /// Exercise immutable package loading while keeping native effects controlled.
    real_packages: bool,
    reports: std::rc::Rc<std::cell::RefCell<BTreeMap<PathBuf, Report>>>,
    events: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    fail_stop: std::rc::Rc<std::cell::RefCell<Option<String>>>,
    fail_start: std::rc::Rc<std::cell::RefCell<Option<String>>>,
    fail_start_package: std::rc::Rc<std::cell::RefCell<Option<PathBuf>>>,
    running: std::rc::Rc<std::cell::RefCell<std::collections::BTreeSet<String>>>,
    owned: std::rc::Rc<std::cell::RefCell<std::collections::BTreeSet<String>>>,
    revoked: std::rc::Rc<std::cell::RefCell<std::collections::BTreeSet<PathBuf>>>,
    fail_registry: std::rc::Rc<std::cell::Cell<bool>>,
    fail_registry_after_rename: std::rc::Rc<std::cell::Cell<bool>>,
    owned_packages: std::rc::Rc<std::cell::RefCell<BTreeMap<String, PathBuf>>>,
    unsafe_stops: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    graph_checkpoints: std::rc::Rc<std::cell::Cell<usize>>,
    interrupt_graph_at: std::rc::Rc<std::cell::Cell<Option<usize>>>,
}

impl Drop for Host {
    fn drop(&mut self) {
        // Selection roots own current/previous/pending independently of acquisition.
        let _ = self.release_download();
    }
}

impl Host {
    pub fn open(
        nix: &Path,
        systemctl: &Path,
        iptables: &Path,
        ip6tables: &Path,
        lifecycle: LifecyclePolicy,
    ) -> Result<Self, Refusal> {
        let paths = HostPaths::production();
        let nix = package::tools(nix)?;
        let systemctl = package::tools(systemctl)?;
        let state = State::open(Path::new(STATE_ROOT))?;
        storage::directory(&paths.roots)?;
        let publishers = package::publisher_bindings(
            &fs::canonicalize("/etc/korri-plugin-host/publishers.json")
                .map_err(|error| format!("publisher bindings are unavailable: {error}"))?,
        )?;
        Ok(Self {
            nix,
            publishers,
            units: Units {
                systemctl,
                unit_directory: "/run/systemd/system".into(),
                ownership_directory: "/run/korri-plugin-host/units".into(),
                firewall: crate::firewall::Firewall {
                    ipv4: package::tools(iptables)?,
                    ipv6: package::tools(ip6tables)?,
                },
            },
            state,
            paths,
            lifecycle,
            owner_uid: 0,
            rehashed: RefCell::default(),
            candidates: RefCell::default(),
            resolved: RefCell::default(),
            acquiring: RefCell::default(),
            candidate_roots: RefCell::default(),
            #[cfg(test)]
            test_runtime: None,
            #[cfg(test)]
            archive_upstream: None,
        })
    }

    #[cfg(test)]
    fn for_test(
        root: &Path,
        nix: PathBuf,
        lifecycle: LifecyclePolicy,
        runtime: TestRuntime,
    ) -> Result<Self, Refusal> {
        let state_root = root.join("state");
        let paths = HostPaths {
            roots: root.join("roots"),
            registry_directory: root.join("registry"),
            registry_path: root.join("registry/enabled-packages.json"),
        };
        let state = State::open(&state_root)?;
        storage::directory(&paths.roots)?;
        Ok(Self {
            nix,
            publishers: BTreeMap::new(),
            units: Units {
                systemctl: PathBuf::from("/unavailable-systemctl"),
                unit_directory: root.join("units"),
                ownership_directory: root.join("unit-ownership"),
                firewall: crate::firewall::Firewall {
                    ipv4: PathBuf::from("/unavailable-iptables"),
                    ipv6: PathBuf::from("/unavailable-ip6tables"),
                },
            },
            state,
            paths,
            lifecycle,
            owner_uid: unsafe { libc::geteuid() },
            rehashed: RefCell::default(),
            candidates: RefCell::default(),
            resolved: RefCell::default(),
            acquiring: RefCell::default(),
            candidate_roots: RefCell::default(),
            test_runtime: Some(runtime),
            archive_upstream: None,
        })
    }

    pub fn inspect(&self, source: &str, package: &Path) -> Result<Report, Refusal> {
        self.graph_update_pending()?;
        package::validate_store_path(package)?;
        let download = self.paths.roots.join("download");
        storage::root_link(&download, package)?;
        package::import(&self.nix, source, package)?;
        let mut releases = false;
        crate::dependencies::dependency_order_with::<Refusal>([package.to_path_buf()], |path| {
            releases |= !package::manifest_releases(path)?.is_empty();
            package::manifest_requires(path).map_err(Refusal::from)
        })?;
        if releases {
            self.state.cleanup_staging()?;
            self.pin_candidate(package)?;
            let curl = std::env::var_os("KORRI_PLUGIN_CURL")
                .ok_or("release requirements need the packaged KORRI_PLUGIN_CURL")?;
            let config = Configuration::from_host(Path::new(&curl))?;
            self.resolve_candidates(&config, package)?;
        }
        self.load(
            package,
            Provenance::RawCache {
                cache_url: source.into(),
            },
        )
    }

    /// Resolve unsigned batch evidence through the existing signed-package
    /// inspection boundary. This grants neither installation nor activation.
    pub fn inspect_release(
        &self,
        curl: &Path,
        source: &str,
        id: &str,
        revision: &str,
    ) -> Result<Report, Refusal> {
        self.graph_update_pending()?;
        validate_id(id)?;
        let batch = crate::release::batch_url(source, revision)?;
        let namespace = id.split_once(':').ok_or("invalid plugin ID")?.0;
        let binding = self
            .publishers
            .get(namespace)
            .ok_or_else(|| format!("publisher {namespace} is not bound on this device"))?;
        if binding.cache_url != source {
            return Err(format!(
                "publisher {namespace} is bound to cache {}",
                binding.cache_url
            )
            .into());
        }
        let paths = crate::release::fetch_paths(
            &package::tools(curl)?,
            &batch,
            revision,
            current_platform(),
            None,
        )?;
        let selected = crate::release::select_output(&paths, id, |path| {
            self.inspect(source, path)
                .map(|report| report.id)
                .map_err(|error| error.to_string())
        })?;
        // Each candidate replaces the temporary download root. Reinspect the
        // selected output to retain its root, even if GC ran during the scan.
        let report = self.inspect(source, &selected)?;
        if report.id != id {
            return Err("selected release package changed identity".into());
        }
        Ok(report)
    }

    /// Every committed selection receipt, read under the host lock.
    pub fn installed(&self) -> Result<Vec<Receipt>, Refusal> {
        self.graph_update_pending()?;
        self.receipts()
    }

    /// Whether this device binds a key for the publisher namespace.
    pub fn publisher_bound(&self, namespace: &str) -> bool {
        self.publishers.contains_key(namespace)
    }

    pub fn sources(&self) -> Result<Vec<SourceUrl>, Refusal> {
        source_store::list_sources(&self.state).map_err(Refusal::from)
    }

    pub fn add_source(
        &self,
        config: &Configuration,
        source: &SourceUrl,
    ) -> Result<source_store::AddOutcome, Refusal> {
        if config.official.as_ref() == Some(source) {
            return Ok(source_store::AddOutcome::AlreadyPresent);
        }
        repository::fetch_catalog(&config.curl, source, config.ca_bundle.as_deref())
            .map_err(Refusal::from)?;
        source_store::add(&self.state, source).map_err(Refusal::from)
    }

    pub fn remove_source(
        &self,
        config: &Configuration,
        source: &SourceUrl,
    ) -> Result<source_store::RemoveOutcome, Refusal> {
        if config.official.as_ref() == Some(source) {
            return Err("official source is managed by trusted host configuration".into());
        }
        source_store::remove(&self.state, source).map_err(Refusal::from)
    }

    pub fn catalog(
        &self,
        config: &Configuration,
        source: &SourceUrl,
    ) -> Result<crate::catalog::Catalog, Refusal> {
        if config.official.as_ref() != Some(source) && !self.sources()?.contains(source) {
            return Err(format!("repository not configured: {source}").into());
        }
        repository::fetch_catalog(&config.curl, source, config.ca_bundle.as_deref())
            .map_err(Refusal::from)
    }

    pub fn installed_source(&self, id: &str) -> Result<SourceUrl, Refusal> {
        let receipt = self.status(id)?.ok_or("plugin is not installed")?;
        match receipt.provenance {
            Provenance::Repository { source_url, .. } => SourceUrl::parse(&source_url).map_err(Refusal::from),
            Provenance::RawCache { .. } | Provenance::Sideload { .. } => Err("plugin was installed from a raw cache; inspect and switch to a repository explicitly".into()),
        }
    }

    pub fn inspect_repository(
        &self,
        config: &Configuration,
        source: &SourceUrl,
        id: &str,
        release: &str,
    ) -> Result<Report, Refusal> {
        self.graph_update_pending()?;
        validate_id(id)?;
        crate::catalog::validate_release_version(release)?;
        // The configured test runtime supplies reports instead of downloading
        // packages. Admin-socket process tests still use the real Host approval,
        // receipt, enablement and lifecycle boundaries below.
        #[cfg(test)]
        if let Some(runtime) = self
            .test_runtime
            .as_ref()
            .filter(|runtime| !runtime.real_packages)
        {
            let reports = runtime.reports.borrow();
            return reports.values().find(|report| matches!(&report.provenance,
                Provenance::Repository { source_url, plugin_id, release_version, .. }
                    if source_url == source.as_str() && plugin_id == id && release_version == release
            )).cloned().ok_or_else(|| "repository fixture release not found".into());
        }
        self.candidate_roots.borrow_mut().clear();
        self.candidates.borrow_mut().clear();
        self.resolved.borrow_mut().clear();
        let catalog = self.catalog(config, source)?;
        let record = catalog.record(id, release, current_platform())?;
        self.state.cleanup_staging()?;
        let package = self.fetch_record(config, source, record)?;
        self.resolve_candidates(config, &package)?;
        self.load(&package, self.candidates.borrow()[&package].0.clone())
    }

    fn fetch_record(
        &self,
        config: &Configuration,
        source: &SourceUrl,
        record: &crate::catalog::CatalogRecord,
    ) -> Result<PathBuf, Refusal> {
        let package = PathBuf::from(&record.store_path);
        if !self.acquiring.borrow_mut().insert(package.clone()) {
            return Err("required plugin acquisition cycle".into());
        }
        let result = self.fetch_record_inner(config, source, record);
        self.acquiring.borrow_mut().remove(&package);
        result
    }

    fn fetch_record_inner(
        &self,
        config: &Configuration,
        source: &SourceUrl,
        record: &crate::catalog::CatalogRecord,
    ) -> Result<PathBuf, Refusal> {
        if self.acquiring.borrow().len() > crate::dependencies::MAX_PLUGINS {
            return Err("too many required plugins".into());
        }
        let stage = self.state.staging()?;
        let archive_path = stage.path().join("archive.tar");
        repository::download(
            &config.curl,
            &SourceUrl::parse(&record.archive_url)?,
            config.ca_bundle.as_deref(),
            &archive_path,
            repository::TransferBounds::Unlimited {
                stall: repository::ARCHIVE_STALL,
            },
        )?;
        repository::verify_archive_hash(&archive_path, &record.archive_sha256)?;
        self.import_archive_file(config, &archive_path, Some((source, record)))
    }

    pub fn inspect_archive(&self, config: &Configuration, path: &Path) -> Result<Report, Refusal> {
        self.graph_update_pending()?;
        self.candidate_roots.borrow_mut().clear();
        self.candidates.borrow_mut().clear();
        self.resolved.borrow_mut().clear();
        self.state.cleanup_staging()?;
        let package = self.import_archive_file(config, path, None)?;
        self.resolve_candidates(config, &package)?;
        self.load(&package, self.candidates.borrow()[&package].0.clone())
    }

    fn import_archive_file(
        &self,
        config: &Configuration,
        path: &Path,
        record: Option<(&SourceUrl, &crate::catalog::CatalogRecord)>,
    ) -> Result<PathBuf, Refusal> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let stage = self.state.staging()?;
        // Freeze a korrid-owned input into this root-owned request directory.
        // Hash and extract these same bytes even if the caller replaces its file.
        let snapshot = stage.path().join("archive.tar");
        let sideload_digest = if record.is_none() {
            use std::os::unix::fs::OpenOptionsExt;
            let mut input = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(path)
                .map_err(|e| e.to_string())?;
            if !input.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("archive must be a regular file".into());
            }
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&snapshot)
                .map_err(|e| e.to_string())?;
            use std::io::Write;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                output
                    .write_all(&buffer[..count])
                    .map_err(|e| e.to_string())?;
                hash.update(&buffer[..count]);
            }
            Some(hex::encode(hash.finalize()))
        } else {
            None
        };
        let cache = stage.path().join("cache");
        storage::directory(&cache)?;
        archive::extract(if record.is_none() { &snapshot } else { path }, &cache)?;
        let package = match record {
            Some((_, r)) => PathBuf::from(&r.store_path),
            None => archive::root_path(&cache)?,
        };
        let cache_uri = archive::path_to_file_uri(&cache)?;
        let info = package::path_info(
            &self.nix,
            package.to_str().ok_or("invalid path")?,
            Some(&cache_uri),
        )?;
        storage::root_link(&self.paths.roots.join("download"), &package)?;
        if info.ca.as_deref().is_some_and(|ca| !ca.is_empty()) {
            archive::preflight_cache(&self.nix, package.to_str().ok_or("invalid path")?, &cache)?;
            package::import_archive(&self.nix, &cache_uri, &package)?;
        } else {
            archive::preflight_slim(&self.nix, &archive::cache_paths(&cache)?, &cache)?;
            let (namespace, requires) = package::archive_manifest(&self.nix, &cache_uri, &package)?;
            let pins = self.pins()?;
            let key = record
                .and_then(|(_, r)| r.public_key.as_deref())
                .or_else(|| {
                    self.publishers
                        .get(&namespace)
                        .map(|b| b.public_key.as_str())
                })
                .or_else(|| pins.get(&namespace).map(|p| p.public_key.as_str()))
                .ok_or_else(|| {
                    Refusal::new(
                        Category::PublisherNotTrusted,
                        format!("publisher {namespace} is not trusted on this device"),
                    )
                })?;
            crate::trust::inspect(&namespace, Some(key), &self.publishers, &pins)?;
            if !package::archive_signature(&self.nix, &package, &cache_uri, Some(key))? {
                return Err("input-addressed archive requires a publisher signature".into());
            }
            let mut excluded = BTreeSet::new();
            for required in requires {
                self.acquire_exact_for_slim(
                    config,
                    record.map(|(source, _)| source),
                    Some(&cache_uri),
                    &required,
                    &package,
                )?;
                excluded.extend(
                    package::StoreSnapshot::query(&self.nix, &[&required])?.closure(&required)?,
                );
            }
            if excluded.contains(&package) {
                return Err("required plugin closure contains its parent".into());
            }
            storage::root_link(&self.paths.roots.join("download"), &package)?;
            let mut upstream = self.shared_cache(config)?;
            crate::slim::import(&self.nix, &cache, &package, key, &mut upstream, &excluded)?;
            if package::manifest_namespace(&package)? != namespace {
                return Err("import changed the publisher namespace".into());
            }
        }
        self.pin_candidate(&package)?;
        let namespace = package::manifest_namespace(&package)?;
        let pins = self.pins()?;
        let key = record
            .and_then(|(_, r)| r.public_key.as_deref())
            .or_else(|| {
                self.publishers
                    .get(&namespace)
                    .map(|b| b.public_key.as_str())
            })
            .or_else(|| pins.get(&namespace).map(|p| p.public_key.as_str()));
        let signed =
            package::archive_signature(&self.nix, &package, &cache_uri, key).map_err(|e| {
                if record.is_none() || e.category == Category::PublisherNotTrusted {
                    Refusal::new(
                        Category::PublisherNotTrusted,
                        format!("publisher {namespace} is not trusted on this device"),
                    )
                } else {
                    e
                }
            })?;
        if record.is_some_and(|(_, r)| r.public_key.is_some()) && !signed {
            return Err("catalog publicKey requires an in-archive signature".into());
        }
        let publisher = crate::trust::inspect(
            &namespace,
            signed.then_some(key).flatten(),
            &self.publishers,
            &pins,
        )?;
        let platform = if package::architecture_independent(&package)?
            && package::no_references(&self.nix, &package)?
        {
            "any"
        } else {
            current_platform()
        };
        let provenance = match record {
            Some((source, record)) => {
                if record.platform == "any" && platform != "any" {
                    return Err("any-architecture package has references or manifest paths".into());
                }
                Provenance::Repository {
                    source_url: source.to_string(),
                    plugin_id: record.plugin_id.clone(),
                    release_version: record.release_version.clone(),
                    platform: record.platform.clone(),
                    archive_sha256: record.archive_sha256.clone(),
                    signed,
                }
            }
            None => Provenance::Sideload {
                archive_sha256: sideload_digest.ok_or("sideload snapshot digest missing")?,
                platform: platform.into(),
                signed,
            },
        };
        self.candidates
            .borrow_mut()
            .insert(package.clone(), (provenance, publisher));
        Ok(package)
    }

    fn shared_cache(&self, config: &Configuration) -> Result<crate::slim::Upstream, Refusal> {
        let mut upstream = crate::slim::Upstream::new(&self.nix, config)?;
        #[cfg(test)]
        if let Some(cache) = &self.archive_upstream {
            upstream = crate::slim::Upstream::file(&self.nix, cache);
        }
        // NixOS also registers bound plugin keys globally. Those are publisher
        // authority, not upstream authority, including for another requirement.
        let pins = self.pins()?;
        upstream.exclude_publishers(
            self.publishers
                .values()
                .map(|binding| binding.public_key.as_str())
                .chain(pins.values().map(|pin| pin.public_key.as_str())),
        )?;
        Ok(upstream)
    }

    /// A required plugin establishes exactly the same per-path authority as
    /// its parent, including when its established provenance is RawCache.
    fn acquire_exact_for_slim(
        &self,
        config: &Configuration,
        source: Option<&SourceUrl>,
        cache: Option<&str>,
        required: &Path,
        parent: &Path,
    ) -> Result<(), Refusal> {
        if required == parent || self.acquiring.borrow().contains(required) {
            return Err("required plugin acquisition cycle".into());
        }
        if self.candidates.borrow().contains_key(required) {
            return Ok(());
        }
        let selected = self.receipts()?.into_iter().find_map(|receipt| {
            if receipt.package == required {
                Some(receipt.provenance)
            } else {
                receipt
                    .previous
                    .filter(|previous| previous.package == required)
                    .map(|previous| previous.provenance)
            }
        });
        let namespace = if required.exists() {
            package::manifest_namespace(required)?
        } else {
            package::archive_manifest(
                &self.nix,
                cache.ok_or_else(|| {
                    format!(
                        "required plugin {} is unavailable locally",
                        required.display()
                    )
                })?,
                required,
            )?
            .0
        };
        let pins = self.pins()?;
        let key = self
            .publishers
            .get(&namespace)
            .map(|binding| binding.public_key.as_str())
            .or_else(|| pins.get(&namespace).map(|pin| pin.public_key.as_str()));
        let provenance = if let Some(provenance) = selected {
            self.verify_publisher_in(None, required, &provenance, package::StoreContents::Rehash)?;
            let snapshot = package::StoreSnapshot::query(&self.nix, &[required])?;
            let closure = snapshot.closure(required)?;
            let mut content_addressed = true;
            for path in &closure {
                content_addressed &= snapshot
                    .info(path)?
                    .ca
                    .as_deref()
                    .is_some_and(|ca| !ca.is_empty());
            }
            if content_addressed {
                package::verify_unsigned(&self.nix, required, package::StoreContents::Rehash)?;
            } else {
                self.verify_required_closure(
                    config,
                    source,
                    required,
                    key.ok_or("required plugin publisher is not trusted")?,
                    None,
                    parent,
                )?;
            }
            provenance
        } else if let Some(binding) = self.publishers.get(&namespace) {
            self.verify_required_closure(
                config,
                source,
                required,
                &binding.public_key,
                Some(&binding.cache_url),
                parent,
            )?;
            Provenance::RawCache {
                cache_url: binding.cache_url.clone(),
            }
        } else {
            let source = source.ok_or_else(|| format!("required plugin {} is not in a repository; publish it there or use a release requirement.", required.display()))?;
            let catalog =
                repository::fetch_catalog(&config.curl, source, config.ca_bundle.as_deref())?;
            let record = catalog.records.iter().find(|r| r.store_path == required.to_string_lossy() && (r.platform == current_platform() || r.platform == "any"))
                .ok_or_else(|| format!("required plugin {} is not in {source}; publish it there or use a release requirement.", required.display()))?;
            self.fetch_record(config, source, record)?;
            return Ok(());
        };
        self.pin_candidate(required)?;
        let signed = !matches!(
            provenance,
            Provenance::Repository { signed: false, .. }
                | Provenance::Sideload { signed: false, .. }
        );
        let publisher = crate::trust::inspect(
            &namespace,
            signed.then_some(key).flatten(),
            &self.publishers,
            &pins,
        )?;
        self.candidates
            .borrow_mut()
            .insert(required.to_path_buf(), (provenance, publisher));
        Ok(())
    }

    fn verify_required_closure(
        &self,
        config: &Configuration,
        source: Option<&SourceUrl>,
        required: &Path,
        key: &str,
        cache: Option<&str>,
        parent: &Path,
    ) -> Result<(), Refusal> {
        if !self.acquiring.borrow_mut().insert(required.to_path_buf()) {
            return Err("required plugin acquisition cycle".into());
        }
        let result = (|| {
            if self.acquiring.borrow().len() > crate::dependencies::MAX_PLUGINS {
                return Err(Refusal::from("too many required plugins"));
            }
            let children = if let Some(cache) = cache {
                if !package::archive_signature(&self.nix, required, cache, Some(key))? {
                    return Err("required plugin is not signed by its bound publisher".into());
                }
                package::archive_manifest(&self.nix, cache, required)?.1
            } else {
                package::manifest_requires(required)?
            };
            let mut excluded = BTreeSet::new();
            for child in children {
                self.acquire_exact_for_slim(config, source, cache, &child, required)?;
                excluded
                    .extend(package::StoreSnapshot::query(&self.nix, &[&child])?.closure(&child)?);
            }
            if excluded.contains(required) {
                return Err("required plugin closure contains its parent".into());
            }
            let mut forbidden = self.acquiring.borrow().clone();
            forbidden.remove(required);
            forbidden.insert(parent.to_path_buf());
            storage::root_link(&self.paths.roots.join("download"), required)?;
            let mut upstream = self.shared_cache(config)?;
            crate::slim::import_required(
                &self.nix,
                cache,
                required,
                key,
                &mut upstream,
                &excluded,
                &forbidden,
            )?;
            Ok(())
        })();
        self.acquiring.borrow_mut().remove(required);
        result
    }

    /// Nix owns the indirect root registry. Korri retains only the existing
    /// private staging TempDir for this request; no candidate lock is persisted.
    fn pin_candidate(&self, package: &Path) -> Result<(), Refusal> {
        let stage = self.state.staging()?;
        let root = stage.path().join("download");
        let nix_store = package::tools(
            &self
                .nix
                .parent()
                .ok_or("Nix helper has no directory")?
                .join("nix-store"),
        )?;
        crate::process::checked_uncapped(
            &nix_store,
            [
                "--option",
                "max-jobs",
                "0",
                "--option",
                "builders",
                "",
                "--option",
                "fallback",
                "false",
                "--option",
                "substituters",
                "",
                "--add-root",
                root.to_str().ok_or("invalid candidate root path")?,
                "--indirect",
                "--realise",
                package.to_str().ok_or("invalid package path")?,
            ],
            None,
        )?;
        self.candidate_roots.borrow_mut().push(stage);
        Ok(())
    }

    fn pins(&self) -> Result<crate::trust::Pins, Refusal> {
        let pins: crate::trust::Pins =
            storage::read_json(&self.state.root().join("publishers.json"))?.unwrap_or_default();
        for (namespace, pin) in &pins {
            validate_id(&format!("{namespace}:pin"))?;
            package::public_key_bytes(&pin.public_key)?;
            if self.publishers.contains_key(namespace) {
                return Err(format!("config-bound publisher {namespace} cannot have a pin").into());
            }
        }
        Ok(pins)
    }

    pub fn trust_publisher(&self, namespace: &str, public_key: &str) -> Result<(), Refusal> {
        self.graph_update_pending()?;
        validate_id(&format!("{namespace}:pin"))?;
        package::public_key_bytes(public_key)?;
        if self.publishers.contains_key(namespace) {
            return Err(format!("publisher {namespace} is config-bound").into());
        }
        let unsigned: Vec<_> = self
            .receipts()?
            .into_iter()
            .filter(|r| {
                r.id.split_once(':').is_some_and(|(ns, _)| ns == namespace)
                    && matches!(
                        r.provenance,
                        Provenance::Repository { signed: false, .. }
                            | Provenance::Sideload { signed: false, .. }
                    )
            })
            .map(|r| r.id)
            .collect();
        if !unsigned.is_empty() {
            return Err(Refusal::new(
                Category::NamespaceTaken,
                format!(
                    "remove the unsigned {namespace} plugins first: {}",
                    unsigned.join(", ")
                ),
            ));
        }
        let mut pins = self.pins()?;
        pins.insert(
            namespace.into(),
            crate::trust::Pin {
                public_key: public_key.into(),
            },
        );
        storage::write_json(&self.state.root().join("publishers.json"), &pins)?;
        self.candidates.borrow_mut().clear();
        self.resolved.borrow_mut().clear();
        Ok(())
    }

    pub fn revoke_publisher(&self, namespace: &str) -> Result<(), Refusal> {
        self.graph_update_pending()?;
        validate_id(&format!("{namespace}:pin"))?;
        if self.publishers.contains_key(namespace) {
            return Err(format!("publisher {namespace} is config-bound").into());
        }
        let mut pins = self.pins()?;
        pins.remove(namespace);
        self.candidates.borrow_mut().clear();
        self.resolved.borrow_mut().clear();
        storage::write_json(&self.state.root().join("publishers.json"), &pins)?;
        self.invalidate_registry()
    }

    pub fn publishers(&self) -> Result<crate::plugin_admin::PluginPublishers, Refusal> {
        use crate::plugin_admin::*;
        let receipts = self.installed()?;
        let mut entries = Vec::new();
        for (namespace, key, state) in self
            .publishers
            .iter()
            .map(|(ns, b)| {
                (
                    ns.clone(),
                    b.public_key.clone(),
                    PluginPublisherState::Bound,
                )
            })
            .chain(
                self.pins()?
                    .into_iter()
                    .map(|(ns, p)| (ns, p.public_key, PluginPublisherState::Trusted)),
            )
        {
            entries.push(PluginPublisherEntry {
                installed: receipts
                    .iter()
                    .filter(|r| {
                        !matches!(r.desired, Desired::Removed { .. })
                            && r.id.split_once(':').is_some_and(|(ns, _)| ns == namespace)
                    })
                    .map(|r| r.id.clone())
                    .collect(),
                fingerprint: crate::trust::fingerprint(&key)?,
                namespace,
                state,
            });
        }
        entries.sort_by(|a, b| a.namespace.cmp(&b.namespace));
        Ok(PluginPublishers { entries })
    }

    pub fn catalog_publisher(
        &self,
        namespace: &str,
    ) -> Result<crate::plugin_admin::PluginPublisher, Refusal> {
        use crate::plugin_admin::PluginPublisherState::*;
        crate::trust::publisher(
            namespace,
            if self.publishers.contains_key(namespace) {
                Bound
            } else if self.pins()?.contains_key(namespace) {
                Trusted
            } else {
                New
            },
            None,
        )
    }

    fn candidate_for_record(
        &self,
        config: &Configuration,
        source: &SourceUrl,
        record: &crate::catalog::CatalogRecord,
    ) -> Result<PathBuf, Refusal> {
        let package = PathBuf::from(&record.store_path);
        let existing = self
            .candidates
            .borrow()
            .get(&package)
            .map(|(origin, _)| origin.clone());
        if let Some(origin) = &existing {
            if !matches!(origin, Provenance::Repository { source_url, plugin_id, release_version, archive_sha256, .. }
                if source_url == source.as_str() && plugin_id == &record.plugin_id
                    && release_version == &record.release_version && archive_sha256 == &record.archive_sha256)
            {
                return Err(Refusal::new(Category::RequirementConflict, format!("conflicting repository requirements for {}: exact build has different source or release provenance", record.plugin_id)));
            }
        } else {
            self.fetch_record(config, source, record)?;
        }
        Ok(package)
    }

    fn resolve_candidates(&self, config: &Configuration, root: &Path) -> Result<(), Refusal> {
        let receipts = self.receipts()?;
        let repository_of = |path: &Path| -> Result<Option<SourceUrl>, Refusal> {
            let origin = self
                .candidates
                .borrow()
                .get(path)
                .map(|(origin, _)| origin.clone())
                .or_else(|| {
                    receipts
                        .iter()
                        .find(|receipt| receipt.package == path)
                        .map(|receipt| receipt.provenance.clone())
                });
            match origin {
                Some(Provenance::Repository { source_url, .. }) => SourceUrl::parse(&source_url)
                    .map_err(Refusal::from)
                    .map(Some),
                _ => Ok(None),
            }
        };
        let mut repositories = BTreeMap::from([(root.to_path_buf(), repository_of(root)?)]);
        crate::dependencies::dependency_order_with::<Refusal>([root.to_path_buf()], |path| {
            let source = repository_of(path)?.or_else(|| repositories.get(path).cloned().flatten());
            let mut requires = package::manifest_requires(path)?;
            for exact in &requires {
                let namespace = package::manifest_namespace(exact)?;
                let archive_acquisition = self.candidates.borrow().contains_key(path);
                if archive_acquisition && self.publishers.contains_key(&namespace) {
                    // Read the imported manifest too. A mutable requirement
                    // cache cannot hide a new exact dependency during preflight.
                    self.acquire_exact_for_slim(config, source.as_ref(), None, exact, path)?;
                }
                if !self.publishers.contains_key(&namespace) {
                    let id = package::load_declaration(exact)?.id();
                    let Some(source) = &source else {
                        return Err(format!("{id} is not in a repository; publish it there or use a release requirement.").into());
                    };
                    // Existing selections keep their independent provenance.
                    // A fresh unbound exact build needs its own release record;
                    // the parent's archive/signature grants no namespace trust.
                    let locked = receipts.iter().any(|receipt| {
                        receipt.package == *exact
                            || receipt
                                .previous
                                .as_ref()
                                .is_some_and(|previous| previous.package == *exact)
                    });
                    if !locked {
                        let catalog = repository::fetch_catalog(
                            &config.curl,
                            source,
                            config.ca_bundle.as_deref(),
                        )?;
                        let record = catalog.records.iter().find(|record| record.store_path == exact.to_string_lossy()
                            && (record.platform == current_platform() || record.platform == "any"))
                            .ok_or_else(|| format!("{id} is not in {source}; publish it there or use a release requirement."))?;
                        self.candidate_for_record(config, source, record)?;
                    }
                }
                repositories
                    .entry(exact.clone())
                    .or_insert(repository_of(exact)?.or_else(|| source.clone()));
            }
            for requirement in package::manifest_releases(path)? {
                let source = SourceUrl::parse(&requirement.repository)?;
                // A declared requirement explicitly selects its own repository;
                // it need not be registered as a browsing source.
                let catalog =
                    repository::fetch_catalog(&config.curl, &source, config.ca_bundle.as_deref())?;
                let release = requirement
                    .release
                    .as_ref()
                    .or_else(|| catalog.latest.get(&requirement.id))
                    .ok_or_else(|| {
                        format!("repository has no latest release for {}", requirement.id)
                    })?;
                let record = catalog.record(&requirement.id, release, current_platform())?;
                let package = self.candidate_for_record(config, &source, record)?;
                repositories.insert(package.clone(), Some(source));
                requires.push(package);
            }
            self.resolved
                .borrow_mut()
                .insert(path.clone(), requires.clone());
            Ok(requires)
        })?;
        Ok(())
    }

    fn locked_releases(path: &Path, receipts: &[Receipt]) -> Result<Vec<PathBuf>, Refusal> {
        package::manifest_releases(path)?.iter().map(|spec| {
            let source = SourceUrl::parse(&spec.repository)?;
            let receipt = receipts.iter().find(|r| r.id == spec.id && !matches!(r.desired, Desired::Removed { .. })).ok_or_else(|| format!("required plugin {} is not installed", spec.id))?;
            if !matches!(&receipt.provenance, Provenance::Repository { source_url, release_version, .. } if source_url == source.as_str() && spec.release.as_ref().is_none_or(|label| label == release_version)) {
                return Err(format!("required plugin {} has a mismatched repository or release", spec.id).into());
            }
            Ok(receipt.package.clone())
        }).collect()
    }

    fn load(&self, selected: &Path, provenance: Provenance) -> Result<Report, Refusal> {
        let mut report = self.report_for_in(
            selected,
            provenance,
            None,
            Some(package::StoreContents::Rehash),
        )?;
        for dependency in &mut report.brings {
            let dep = &dependency.report;
            dependency.already_approved = self.receipt(&dep.id)?.is_some_and(|receipt| {
                receipt.package == dep.package
                    && receipt.approval == dep.approval
                    && receipt.provenance == dep.provenance
                    && !matches!(receipt.desired, Desired::Removed { .. })
            });
        }
        Ok(report)
    }

    fn report_for(
        &self,
        selected: &Path,
        provenance: Provenance,
        snapshot: Option<&package::StoreSnapshot>,
    ) -> Result<Report, Refusal> {
        self.report_for_in(selected, provenance, snapshot, None)
    }

    fn report_for_in(
        &self,
        selected: &Path,
        provenance: Provenance,
        snapshot: Option<&package::StoreSnapshot>,
        contents: Option<package::StoreContents>,
    ) -> Result<Report, Refusal> {
        #[cfg(test)]
        if let Some(runtime) = self.test_runtime.as_ref().filter(|r| !r.real_packages) {
            return runtime
                .reports
                .borrow()
                .get(selected)
                .cloned()
                .ok_or_else(|| {
                    Refusal::from(format!("missing test report for {}", selected.display()))
                });
        }
        if let Some(contents) = contents {
            self.verify_publisher_in(snapshot, selected, &provenance, contents)?;
        }
        let receipts = if package::manifest_requires(selected)?.is_empty()
            && package::manifest_releases(selected)?.is_empty()
        {
            Vec::new()
        } else {
            self.receipts()?
        };
        let mut report = package::load_resolved_graph(
            &self.nix,
            snapshot,
            selected,
            provenance,
            |path| {
                if let Some(paths) = self.resolved.borrow().get(path) {
                    let exact = package::manifest_requires(path)?;
                    return Ok(paths
                        .iter()
                        .filter(|p| !exact.contains(p))
                        .cloned()
                        .collect());
                }
                if contents.is_some() {
                    return Self::locked_releases(path, &receipts);
                }
                Ok(package::manifest_releases(path)?
                    .iter()
                    .filter_map(|spec| {
                        receipts
                            .iter()
                            .find(|r| {
                                r.id == spec.id && !matches!(r.desired, Desired::Removed { .. })
                            })
                            .map(|r| r.package.clone())
                    })
                    .collect())
            },
            |path| {
                let provenance = match self.candidates.borrow().get(path) {
                    Some((provenance, _)) => provenance.clone(),
                    None => dependency_provenance(path, &receipts, &self.publishers)?,
                };
                if let Some(contents) = contents {
                    self.verify_publisher_in(snapshot, path, &provenance, contents)?;
                }
                Ok(provenance)
            },
        )?;
        for dep in &mut report.brings {
            dep.report.publisher = Some(self.report_publisher(&dep.report)?);
        }
        report.publisher = Some(self.report_publisher(&report)?);
        Ok(report)
    }

    fn report_publisher(
        &self,
        report: &Report,
    ) -> Result<crate::plugin_admin::PluginPublisher, Refusal> {
        if let Some((_, publisher)) = self.candidates.borrow().get(&report.package) {
            return Ok(publisher.clone());
        }
        let namespace = report.id.split_once(':').ok_or("invalid plugin ID")?.0;
        let pins = self.pins()?;
        let key = self
            .publishers
            .get(namespace)
            .map(|b| b.public_key.as_str())
            .or_else(|| pins.get(namespace).map(|p| p.public_key.as_str()));
        let unsigned = matches!(
            report.provenance,
            Provenance::Repository { signed: false, .. }
                | Provenance::Sideload { signed: false, .. }
        );
        if unsigned {
            return crate::trust::inspect(namespace, None, &self.publishers, &pins);
        }
        match key {
            Some(key) => crate::trust::inspect(namespace, Some(key), &self.publishers, &pins),
            None => self.catalog_publisher(namespace),
        }
    }

    fn verify_report(
        &self,
        report: &Report,
        checks: Option<&BootChecks>,
        contents: package::StoreContents,
    ) -> Result<(), Refusal> {
        for report in report
            .brings
            .iter()
            .map(|dep| &dep.report)
            .chain(std::iter::once(report))
        {
            if let Some(publisher) = &report.publisher {
                crate::trust::require_trusted(publisher)?;
            }
            self.verify_publisher_in(
                checks.map(|checks| &checks.snapshot),
                &report.package,
                &report.provenance,
                contents,
            )?;
        }
        Ok(())
    }

    fn verify_publisher_in(
        &self,
        snapshot: Option<&package::StoreSnapshot>,
        selected: &Path,
        provenance: &Provenance,
        contents: package::StoreContents,
    ) -> Result<(), Refusal> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            let _ = (provenance, contents);
            if runtime.revoked.borrow().contains(selected) {
                return Err(
                    format!("injected publisher revocation for {}", selected.display()).into(),
                );
            }
            return Ok(());
        }
        let cache = match provenance {
            Provenance::RawCache { cache_url } => Some(cache_url.as_str()),
            Provenance::Repository { .. } | Provenance::Sideload { .. } => None,
        };
        // Rehash a package's contents at most once per command. The signature
        // over its registered hash is still checked on every call.
        let contents = if contents == package::StoreContents::Rehash
            && self.rehashed.borrow().contains(selected)
        {
            package::StoreContents::TrustRegistered
        } else {
            contents
        };
        if !matches!(provenance, Provenance::RawCache { .. }) {
            let namespace = package::manifest_namespace(selected)?;
            let pins = self.pins()?;
            let signed = matches!(
                provenance,
                Provenance::Repository { signed: true, .. }
                    | Provenance::Sideload { signed: true, .. }
            );
            if !signed {
                crate::trust::inspect(&namespace, None, &self.publishers, &pins)?;
                return package::verify_unsigned(&self.nix, selected, contents)
                    .map_err(Refusal::from);
            }
            let candidate = self.candidates.borrow();
            let key = candidate
                .get(selected)
                .and_then(|(_, p)| p.public_key.as_deref())
                .or_else(|| {
                    self.publishers
                        .get(&namespace)
                        .map(|b| b.public_key.as_str())
                })
                .or_else(|| pins.get(&namespace).map(|p| p.public_key.as_str()))
                .ok_or_else(|| {
                    Refusal::new(
                        Category::PublisherNotTrusted,
                        format!("publisher {namespace} is not trusted on this device"),
                    )
                })?;
            return package::verify_local_signature(&self.nix, snapshot, selected, key, contents)
                .map_err(Refusal::from);
        }
        package::verify_publisher_in(
            &self.nix,
            snapshot,
            selected,
            cache,
            &self.publishers,
            contents,
        )?;
        if contents == package::StoreContents::Rehash {
            self.rehashed.borrow_mut().insert(selected.to_path_buf());
        }
        Ok(())
    }

    pub fn release_download(&self) -> Result<(), Refusal> {
        self.candidate_roots.borrow_mut().clear();
        for name in ["download", "download.new"] {
            storage::remove(&self.paths.roots.join(name))?;
        }
        Ok(())
    }

    /// Build the complete approval surface before changing any plugin
    /// selection. Optional recommendations are recorded only for the caller's
    /// UI; this transaction never installs them on an existing device.
    pub fn review_release_update(
        &self,
        required: Vec<Report>,
        optional: Vec<Report>,
    ) -> Result<ReleaseUpdateReview, Refusal> {
        crate::lifecycle::review_release_update(required, optional).map_err(Refusal::from)
    }

    /// Install every newly required plugin before the caller selects its new
    /// release. Approval refusal, an installation failure, or a release-select
    /// failure leaves the caller's current release selected. New selections
    /// made by this transaction are removed on failure; pre-existing enabled,
    /// disabled, and selected plugins are not changed.
    pub fn apply_release_update<F>(
        &self,
        review: ReleaseUpdateReview,
        approvals: &BTreeMap<String, String>,
        select_release: F,
    ) -> Result<(), Refusal>
    where
        F: FnOnce() -> Result<(), Refusal>,
    {
        self.graph_update_pending()?;
        let (roots, _optional_ids) = review.into_parts();
        let mut required = Vec::<Report>::new();
        // Root approval covers its disclosed closure. Keep the existing
        // release rule: pre-existing selections are never replaced here.
        for report in roots {
            let approval = approvals
                .get(&report.id)
                .ok_or_else(|| format!("required plugin {} was not approved", report.id))?;
            if approval != &report.approval {
                return Err(format!(
                    "approval for required plugin {} does not match its disclosed permissions",
                    report.id
                )
                .into());
            }
            let checked = self.load(&report.package, report.provenance.clone())?;
            if checked.approval != report.approval {
                return Err("release dependency closure no longer matches its approval".into());
            }
            if self.receipt(&report.id)?.is_none() {
                for dep in &checked.brings {
                    if let Some(receipt) = self.receipt(&dep.report.id)? {
                        if receipt.package != dep.report.package
                            || receipt.approval != dep.report.approval
                            || receipt.provenance != dep.report.provenance
                            || matches!(receipt.desired, Desired::Removed { .. })
                        {
                            return Err(format!(
                                "conflicting exact selection for required plugin {}",
                                dep.report.id
                            )
                            .into());
                        }
                        self.approved(&receipt)?;
                    }
                    if let Some(existing) = required.iter().find(|r| r.id == dep.report.id) {
                        if existing.package != dep.report.package
                            || existing.approval != dep.report.approval
                            || existing.provenance != dep.report.provenance
                        {
                            return Err(format!(
                                "conflicting exact selection for required plugin {}",
                                dep.report.id
                            )
                            .into());
                        }
                    } else {
                        required.push(dep.report.clone());
                    }
                }
            }
            if let Some(existing) = required.iter().find(|r| r.id == report.id) {
                if existing.package != report.package
                    || existing.approval != report.approval
                    || existing.provenance != report.provenance
                {
                    return Err(format!(
                        "conflicting exact selection for required plugin {}",
                        report.id
                    )
                    .into());
                }
            } else {
                required.push(report);
            }
        }
        let mut additions = Vec::new();
        for report in &required {
            report.provenance.validate(&report.id)?;
            match self.receipt(&report.id)? {
                Some(receipt) if matches!(receipt.desired, Desired::Removed { .. }) => {
                    return Err(format!("plugin {} removal is unfinished", report.id).into());
                }
                Some(receipt) => {
                    self.approved(&receipt)?;
                }
                None => additions.push(report.id.clone()),
            }
        }

        // Pin every approved addition before the first receipt changes. This
        // uses the existing per-selection pending roots, not a release manifest
        // or second transaction format.
        for report in &required {
            if additions.contains(&report.id) {
                if let Err(error) = self.prepare(&report.id).and_then(|_| {
                    self.selection(&report.id)
                        .stage(&report.package)
                        .map_err(Refusal::from)
                }) {
                    return self.rollback_release_additions(
                        &additions,
                        format!("required plugin staging failed: {error}"),
                    );
                }
            }
        }
        for report in required {
            if !additions.contains(&report.id) {
                continue;
            }
            let candidate = Receipt {
                id: report.id.clone(),
                package: report.package.clone(),
                provenance: report.provenance.clone(),
                approval: report.approval.clone(),
                desired: Desired::Disabled,
                previous: None,
            };
            if let Err(error) = self.apply(candidate) {
                return self.rollback_release_additions(
                    &additions,
                    format!("required plugin installation failed: {error}"),
                );
            }
        }
        if let Err(error) = self.release_download() {
            return self.rollback_release_additions(
                &additions,
                format!("required plugin acquisition cleanup failed: {error}"),
            );
        }

        if let Err(error) = select_release() {
            return self.rollback_release_additions(
                &additions,
                format!("release selection failed: {error}"),
            );
        }
        Ok(())
    }

    fn rollback_release_additions(
        &self,
        additions: &[String],
        operation_error: String,
    ) -> Result<(), Refusal> {
        let mut rollback_errors: Vec<String> = Vec::new();
        if let Err(error) = self.release_download() {
            rollback_errors.push(error.to_string());
        }
        if let Err(error) = self.invalidate_registry() {
            rollback_errors.push(error.to_string());
        }
        for id in additions.iter().rev() {
            let cleanup = match self.receipt(id) {
                Ok(Some(_)) => self.deactivate(id, Desired::Removed { purge: false }),
                Ok(None) => self
                    .units_stop(id, false)
                    .and_then(|_| self.selection(id).remove().map_err(Refusal::from)),
                Err(error) => Err(error),
            };
            if let Err(error) = cleanup {
                rollback_errors.push(format!("{id}: {error}"));
            }
        }
        if let Err(error) = self.publish_registry() {
            rollback_errors.push(error.to_string());
        }
        if rollback_errors.is_empty() {
            Err(operation_error.into())
        } else {
            Err(format!(
                "{operation_error}; required-plugin rollback failed: {}",
                rollback_errors.join("; ")
            )
            .into())
        }
    }

    /// Install a reviewed selection from the admin socket. For an installed
    /// plugin, accept only a new review of the exact installed package and
    /// provenance, for example after the host policy changed its approval.
    /// That is the existing update intent and keeps the desired state. Any
    /// other selection for an installed plugin needs `repository update` or
    /// `repository switch`, which the socket does not offer.
    pub fn install_reviewed(&self, report: Report, approval: &str) -> Result<(), Refusal> {
        self.graph_update_pending()?;
        validate_id(&report.id)?;
        let id = report.id.clone();
        match self.receipt(&id)? {
            None => self.install(report, approval, SelectionIntent::Install),
            Some(old) if same_selection(&old, &report) => {
                self.install(report, approval, SelectionIntent::Update { id: &id })
            }
            Some(_) => Err("plugin is installed; use update or explicit source switch".into()),
        }
    }

    pub fn install(
        &self,
        report: Report,
        approval: &str,
        intent: SelectionIntent<'_>,
    ) -> Result<(), Refusal> {
        self.graph_update_pending()?;
        if self.review_approval(&report)? != approval {
            return Err(
                "approval does not match this package and its execution policy; inspect it first"
                    .into(),
            );
        }
        let checked = self.load(&report.package, report.provenance.clone())?;
        if self.review_approval(&checked)? != approval || checked.id != report.id {
            return Err("package dependency closure no longer matches its approval".into());
        }
        for r in checked
            .brings
            .iter()
            .map(|d| &d.report)
            .chain(std::iter::once(&checked))
        {
            if let Some(publisher) = &r.publisher {
                crate::trust::require_trusted(publisher)?;
            }
        }
        let old = self.receipt(&report.id)?;
        intent.validate(
            &report.id,
            old.as_ref().map(|r| &r.provenance),
            &report.provenance,
        )?;
        self.check_required_by(&report.id, false)?;
        let enabled_graph_transition = match old.as_ref() {
            // A new review of the same package has the same requirements,
            // and its old approval may be the stale one being replaced.
            Some(old)
                if matches!(old.desired, Desired::Enabled) && same_selection(old, &checked) =>
            {
                !checked.requires.is_empty()
            }
            Some(old) if matches!(old.desired, Desired::Enabled) => {
                !checked.requires.is_empty() || !self.approved(old)?.requires.is_empty()
            }
            _ => false,
        };
        let mut additions = Vec::new();
        let mut replacements = BTreeMap::new();
        // Refuse conflicting selections before the first receipt or daemon changes.
        for dep in &checked.brings {
            let dep = &dep.report;
            match self.receipt(&dep.id)? {
                Some(receipt) => {
                    if receipt.package != dep.package
                        || receipt.provenance != dep.provenance
                        || receipt.approval != dep.approval
                        || matches!(receipt.desired, Desired::Removed { .. })
                    {
                        if !matches!(intent, SelectionIntent::Update { .. })
                            || matches!(receipt.desired, Desired::Removed { .. })
                        {
                            return Err(Refusal::new(
                                Category::RequirementConflict,
                                self.requirement_conflict(&checked, dep, &receipt)?,
                            ));
                        }
                        replacements
                            .insert(dep.id.clone(), matches!(receipt.desired, Desired::Enabled));
                    }
                    self.approved(&receipt)?;
                }
                None => additions.push(dep.id.clone()),
            }
        }
        // Only this reviewed root and the nodes it replaces may release their
        // old exact requirements. Every other installed dependent still vetoes.
        let updating: BTreeSet<_> = replacements
            .keys()
            .cloned()
            .chain((!replacements.is_empty()).then(|| report.id.clone()))
            .collect();
        for id in replacements.keys() {
            if let Err(error) = self.check_required_by_except(id, false, &updating) {
                let target = &checked
                    .brings
                    .iter()
                    .find(|d| &d.report.id == id)
                    .unwrap()
                    .report;
                if matches!(target.provenance, Provenance::Repository { .. }) {
                    return Err(Refusal::new(
                        Category::RequirementConflict,
                        self.requirement_conflict(
                            &checked,
                            target,
                            &self.receipt(id)?.ok_or("dependency missing")?,
                        )?,
                    ));
                }
                return Err(error);
            }
        }
        if !replacements.is_empty() {
            let committed = self.approved(old.as_ref().ok_or("plugin is not installed")?)?;
            for dep in &checked.brings {
                let target = &dep.report;
                if !replacements.contains_key(&target.id) {
                    continue;
                }
                let Some(required) = committed
                    .brings
                    .iter()
                    .find(|dep| dep.report.id == target.id)
                else {
                    continue;
                };
                let required = &required.report;
                let current = self
                    .receipt(&target.id)?
                    .ok_or("required plugin is not installed")?;
                let current_matches =
                    same_selection(&current, required) && current.approval == required.approval;
                let restores_required = target.package == required.package
                    && target.provenance == required.provenance
                    && target.approval == required.approval;
                // A failed graph update can leave the committed root's exact
                // dependency only in previous. A third build would overwrite
                // its provenance before that root can be approved for cleanup.
                if !current_matches && !restores_required {
                    return Err(format!(
                        "cannot replace required plugin {} again; restore the committed root's exact dependency graph for {} first",
                        target.id, committed.id
                    ).into());
                }
            }
        }
        if !replacements.is_empty() {
            return self.update_root_graph(checked);
        }
        for dep in &checked.brings {
            if !additions.contains(&dep.report.id) {
                continue;
            }
            self.prepare(&dep.report.id)?;
            self.selection(&dep.report.id).stage(&dep.report.package)?;
        }
        let result = (|| {
            for dep in &checked.brings {
                if additions.contains(&dep.report.id) {
                    self.install_one(
                        dep.report.clone(),
                        &dep.report.approval,
                        SelectionIntent::Install,
                    )?;
                }
            }
            if enabled_graph_transition {
                // Stop the pack before a new dependency may be started. Keep
                // the committed pack disabled on failure, never half-active.
                self.deactivate(&report.id, Desired::Disabled)?;
                self.activate_dependencies(&checked)?;
                self.install_one(checked.clone(), &checked.approval, intent)?;
                self.set_enabled(&report.id, true)
            } else {
                self.install_one(checked.clone(), &checked.approval, intent)
            }
        })();
        if let Err(error) = result {
            // An enabled committed pack may now require these additions. Never
            // release them merely because registry publication failed.
            let mut cleanup_errors = Vec::new();
            for id in additions.iter().rev() {
                if self.check_required_by_enabled(id).is_err() {
                    continue;
                }
                let cleanup = if self.receipt(id)?.is_some() {
                    self.deactivate(id, Desired::Removed { purge: false })
                } else {
                    self.units_stop(id, false)
                        .and_then(|_| self.selection(id).remove().map_err(Refusal::from))
                };
                if let Err(cleanup) = cleanup {
                    cleanup_errors.push(format!("{id}: {cleanup}"));
                }
            }
            return Err(format!(
                "{error}{}",
                if cleanup_errors.is_empty() {
                    String::new()
                } else {
                    format!("; dependency cleanup failed: {}", cleanup_errors.join("; "))
                }
            )
            .into());
        }
        self.release_download()
    }

    fn requirement_conflict(
        &self,
        root: &Report,
        dep: &Report,
        locked: &Receipt,
    ) -> Result<String, Refusal> {
        if !matches!(dep.provenance, Provenance::Repository { .. }) {
            return Ok(format!(
                "conflicting exact selection for required plugin {}",
                dep.id
            ));
        }
        let release = |provenance: &Provenance, path: &Path| match provenance {
            Provenance::Repository {
                release_version, ..
            } => release_version.clone(),
            _ => path.display().to_string(),
        };
        let mut owner = locked.id.clone();
        for receipt in self.receipts()? {
            if receipt.id != root.id
                && receipt.id != dep.id
                && self
                    .selection_requires(&receipt.package)?
                    .contains(&locked.package)
            {
                owner = receipt.id;
                break;
            }
        }
        Ok(format!(
            "{} needs {} {}, but {} locked {} {}",
            root.id,
            dep.id,
            release(&dep.provenance, &dep.package),
            owner,
            dep.id,
            release(&locked.provenance, &locked.package)
        ))
    }

    pub fn review_approval(&self, report: &Report) -> Result<String, Refusal> {
        #[cfg(test)]
        if self.test_runtime.as_ref().is_some_and(|r| !r.real_packages) {
            return Ok(report.approval.clone());
        }
        package::review_approval(report).map_err(Refusal::from)
    }

    fn install_one(
        &self,
        report: Report,
        approval: &str,
        intent: SelectionIntent<'_>,
    ) -> Result<(), Refusal> {
        if report.approval != approval {
            return Err(
                "approval does not match this package and its execution policy; inspect it first"
                    .into(),
            );
        }
        report.provenance.validate(&report.id)?;
        match intent {
            SelectionIntent::Update { id } | SelectionIntent::SwitchSource { id }
                if id != report.id =>
            {
                return Err("selection cannot change the plugin identity".into());
            }
            _ => {}
        }
        let old = self.receipt(&report.id)?;
        intent.validate(
            &report.id,
            old.as_ref().map(|r| &r.provenance),
            &report.provenance,
        )?;
        self.prepare(&report.id)?;
        self.recover_one(&report.id)?;
        let old = self.receipt(&report.id)?;
        intent.validate(
            &report.id,
            old.as_ref().map(|r| &r.provenance),
            &report.provenance,
        )?;
        let candidate = match old {
            Some(old) if same_selection(&old, &report) => {
                // A new review of the exact installed selection replaces only
                // its approval, which install() has just re-derived from the
                // verified package. The old approval may be stale, so it is
                // not kept as a previous selection to roll back to.
                if matches!(old.desired, Desired::Removed { .. }) {
                    return Err("plugin removal is unfinished".into());
                }
                Receipt {
                    approval: report.approval.clone(),
                    ..old
                }
            }
            Some(old) => {
                // Retaining a prior approval does not require its publisher
                // to remain authorized, but it must still match exact bytes.
                self.approved(&old)?;
                old.select(
                    report.package.clone(),
                    report.provenance.clone(),
                    report.approval.clone(),
                )?
            }
            None => Receipt {
                id: report.id.clone(),
                package: report.package.clone(),
                provenance: report.provenance.clone(),
                approval: report.approval.clone(),
                desired: Desired::Disabled,
                previous: None,
            },
        };
        // The outer graph installation owns acquisition roots until every
        // dependency and the root selection have committed.
        self.apply(candidate)
    }

    /// Swap the two exact approved selections. No inspection, repository
    /// lookup, import, or dependency substitution is part of rollback.
    pub fn rollback(&self, id: &str) -> Result<(), Refusal> {
        self.graph_update_pending()?;
        validate_id(id)?;
        self.prepare(id)?;
        // Validate before recovery can start anything. A refused swap must
        // not disturb the installed graph or discard an interrupted operation.
        let current = self.receipt(id)?.ok_or("plugin is not installed")?;
        let current_report = self.approved(&current)?;
        let candidate = current.rollback()?;
        let report = self.approved(&candidate)?;
        self.check_required_by(id, false)?;
        self.check_dependencies(&report, false, None)?;
        self.verify_report(&report, None, package::StoreContents::Rehash)?;
        if matches!(candidate.desired, Desired::Enabled)
            && (!report.requires.is_empty() || !current_report.requires.is_empty())
        {
            self.deactivate(id, Desired::Disabled)?;
            self.activate_dependencies(&report)?;
        }
        self.recover_one(id)?;
        self.apply(candidate)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), Refusal> {
        self.graph_update_pending()?;
        validate_id(id)?;
        self.prepare(id)?;
        if !enabled {
            return self.deactivate(id, Desired::Disabled);
        }
        let mut receipt = self.receipt(id)?.ok_or("plugin is not installed")?;
        if matches!(receipt.desired, Desired::Removed { .. }) {
            return Err("plugin removal is unfinished".into());
        }
        let preflight = (|| {
            let report = self.approved(&receipt)?;
            self.verify_report(&report, None, package::StoreContents::Rehash)?;
            self.check_dependencies(&report, false, None)?;
            Ok::<_, Refusal>(report)
        })();
        let report = match preflight {
            Ok(report) => report,
            Err(error) => {
                if matches!(receipt.desired, Desired::Enabled) {
                    self.stop_enabled_dependents(&receipt.package)?;
                    self.deny_start(&receipt)?;
                }
                return Err(error);
            }
        };
        self.check_required_by_enabled(id)?;
        if matches!(receipt.desired, Desired::Enabled) && !report.requires.is_empty() {
            self.deactivate(id, Desired::Disabled)?;
            receipt.desired = Desired::Disabled;
        }
        self.activate_dependencies(&report)?;
        self.recover_one(id)?;
        receipt.desired = Desired::Enabled;
        self.apply(receipt)
    }

    fn activate_dependencies(&self, report: &Report) -> Result<(), Refusal> {
        // brings is a checked dependency-first flat closure. All approvals and
        // exact selections are checked before the first activation.
        self.check_dependencies(report, false, None)?;
        self.verify_report(report, None, package::StoreContents::Rehash)?;
        for dependency in &report.brings {
            let mut receipt = self
                .receipt(&dependency.report.id)?
                .ok_or("required plugin is not installed")?;
            if matches!(receipt.desired, Desired::Enabled) {
                if self.settled(&receipt).is_err()
                    || !self.units_matches_running(&dependency.report)?
                {
                    self.stop_enabled_dependents(&dependency.report.package)?;
                    self.restore_one(&receipt.id)?;
                }
            } else {
                self.stop_enabled_dependents(&receipt.package)?;
                self.recover_one(&receipt.id)?;
                receipt.desired = Desired::Enabled;
                self.apply(receipt)?;
            }
        }
        Ok(())
    }

    fn check_dependencies(
        &self,
        report: &Report,
        enabled: bool,
        checks: Option<&BootChecks>,
    ) -> Result<(), Refusal> {
        #[cfg(test)]
        let real = self.test_runtime.as_ref().is_none_or(|r| r.real_packages);
        #[cfg(not(test))]
        let real = true;
        if real {
            let receipts = self.receipts()?;
            for r in report
                .brings
                .iter()
                .map(|d| &d.report)
                .chain(std::iter::once(report))
            {
                Self::locked_releases(&r.package, &receipts)?;
            }
        }
        for dep in &report.brings {
            let dep = &dep.report;
            let receipt = self.receipt(&dep.id)?.ok_or_else(|| {
                format!(
                    "{} requires installed plugin {} ({})",
                    report.id,
                    dep.id,
                    dep.package.display()
                )
            })?;
            if receipt.package != dep.package
                || receipt.approval != dep.approval
                || receipt.provenance != dep.provenance
                || matches!(receipt.desired, Desired::Removed { .. })
            {
                return Err(format!(
                    "required plugin {} has a stale or unapproved exact selection",
                    dep.id
                )
                .into());
            }
            self.approved_in(&receipt, checks)?;
            if enabled {
                if !matches!(receipt.desired, Desired::Enabled) {
                    return Err(format!("{} requires enabled plugin {}", report.id, dep.id).into());
                }
                self.settled(&receipt)?;
            }
        }
        Ok(())
    }

    fn check_required_by_enabled(&self, id: &str) -> Result<(), Refusal> {
        self.check_required_by(id, true)
    }

    fn check_required_by(&self, id: &str, active_only: bool) -> Result<(), Refusal> {
        self.check_required_by_except(id, active_only, &BTreeSet::new())
    }

    fn check_required_by_except(
        &self,
        id: &str,
        active_only: bool,
        updating: &BTreeSet<String>,
    ) -> Result<(), Refusal> {
        let Some(selected) = self.receipt(id)? else {
            return Ok(());
        };
        for receipt in self.receipts()? {
            if receipt.id == id
                || updating.contains(&receipt.id)
                || (active_only && !self.cleanup_unfinished(&receipt)?)
            {
                continue;
            }
            if matches!(receipt.desired, Desired::Removed { .. })
                && self.selection(&receipt.id).software_released()?
                && !self.cleanup_unfinished(&receipt)?
            {
                continue;
            }
            let runtime = crate::dependencies::dependency_order_with::<Refusal>(
                [receipt.package.clone()],
                |path| {
                    if *path == receipt.package {
                        self.runtime_requires(&receipt)
                    } else {
                        self.selection_requires(path)
                    }
                },
            )?;
            let report = self.report_for(&receipt.package, receipt.provenance.clone(), None)?;
            if runtime.contains(&selected.package)
                || report.brings.iter().any(|dep| dep.report.id == id)
            {
                return Err(if matches!(receipt.desired, Desired::Enabled) {
                    format!("plugin {id} is actively required by {}", receipt.id).into()
                } else if active_only {
                    format!(
                        "plugin {id} is actively required by {}; dependent cleanup is unfinished",
                        receipt.id
                    )
                    .into()
                } else {
                    format!("plugin {id} is required by installed plugin {}; remove or update that dependent first", receipt.id).into()
                });
            }
        }
        Ok(())
    }

    fn units_owned(&self, id: &str) -> Result<bool, Refusal> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            return Ok(runtime.owned.borrow().contains(id));
        }
        self.units.has_owned_units(id).map_err(Refusal::from)
    }

    fn cleanup_unfinished(&self, receipt: &Receipt) -> Result<bool, Refusal> {
        if matches!(receipt.desired, Desired::Enabled) || self.units_owned(&receipt.id)? {
            return Ok(true);
        }
        match fs::symlink_metadata(self.root(&receipt.id, "pending")) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.to_string().into()),
        }
    }

    fn journal_cleanup(&self, receipt: &Receipt) -> Result<(), Refusal> {
        match fs::symlink_metadata(self.root(&receipt.id, "pending")) {
            Ok(_) => Ok(()), // Keep an interrupted candidate pinned through cleanup.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => self
                .selection(&receipt.id)
                .stage(&receipt.package)
                .map_err(Refusal::from),
            Err(error) => Err(error.to_string().into()),
        }
    }

    fn runtime_requires(&self, receipt: &Receipt) -> Result<Vec<PathBuf>, Refusal> {
        let mut requires = self.selection_requires(&receipt.package)?;
        match fs::read_link(self.root(&receipt.id, "pending")) {
            Ok(candidate) => requires.extend(self.selection_requires(&candidate)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string().into()),
        }
        requires.sort();
        requires.dedup();
        Ok(requires)
    }

    fn selection_requires(&self, path: &Path) -> Result<Vec<PathBuf>, Refusal> {
        package::validate_store_path(path)?;
        #[cfg(test)]
        if let Some(runtime) = self.test_runtime.as_ref().filter(|r| !r.real_packages) {
            return runtime
                .reports
                .borrow()
                .get(path)
                .map(|r| r.requires.clone())
                .ok_or_else(|| {
                    Refusal::from(format!(
                        "cannot discover required plugins for {}",
                        path.display()
                    ))
                });
        }
        let mut paths = package::manifest_requires(path)?;
        paths.extend(Self::locked_releases(path, &self.receipts()?)?);
        Ok(paths)
    }

    /// Stop may never use durable disable/removal intent as evidence that an
    /// ancestor's effects finished. A restore pass can supply successful stop
    /// proofs under the same exclusive host lock, not infer them from intent.
    fn check_stop(
        &self,
        id: &str,
        quiesced: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<(), Refusal> {
        let Some(selected) = self.receipt(id)? else {
            return Ok(());
        };
        for receipt in self.receipts()? {
            if receipt.id == id
                || (quiesced.contains(&receipt.package) && !self.units_owned(&receipt.id)?)
                || !self.cleanup_unfinished(&receipt)?
            {
                continue;
            }
            let order = crate::dependencies::dependency_order_with::<Refusal>(
                [receipt.package.clone()],
                |path| {
                    if *path == receipt.package {
                        self.runtime_requires(&receipt)
                    } else {
                        self.selection_requires(path)
                    }
                },
            )?;
            if order.contains(&selected.package) {
                return Err(format!(
                    "plugin {id} is actively required by {}; dependent cleanup is unfinished",
                    receipt.id
                )
                .into());
            }
        }
        Ok(())
    }

    fn stop_enabled_dependents(&self, package: &Path) -> Result<(), Refusal> {
        // Discovery/size/cycle refusal must not leave a stale projection.
        self.invalidate_registry()?;
        let mut affected = BTreeMap::new();
        for receipt in self.receipts()? {
            if !self.cleanup_unfinished(&receipt)? || receipt.package == package {
                continue;
            }
            let order = crate::dependencies::dependency_order_with::<Refusal>(
                [receipt.package.clone()],
                |path| {
                    if *path == receipt.package {
                        self.runtime_requires(&receipt)
                    } else {
                        self.selection_requires(path)
                    }
                },
            )?;
            if order.contains(&package.to_path_buf()) {
                let requires = self.runtime_requires(&receipt)?;
                affected.insert(receipt.package.clone(), (receipt, requires));
            }
        }
        let order = crate::dependencies::dependency_order_with::<Refusal>(
            affected.keys().cloned(),
            |path| {
                Ok(affected
                    .get(path)
                    .map(|(_, requires)| requires.clone())
                    .unwrap_or_default())
            },
        )?;
        // Quiesce outer dependents before any required service can stop.
        for path in order.into_iter().rev() {
            let Some((mut receipt, _)) = affected.remove(&path) else {
                continue;
            };
            self.journal_cleanup(&receipt)?;
            if matches!(receipt.desired, Desired::Enabled) {
                receipt.desired = Desired::Disabled;
            }
            storage::write_json(&self.receipt_path(&receipt.id), &receipt)?;
            self.restore_one(&receipt.id)?;
        }
        Ok(())
    }

    pub fn remove(&self, id: &str, purge: bool) -> Result<(), Refusal> {
        self.graph_update_pending()?;
        self.lifecycle.check_removal(id)?;
        self.prepare(id)?;
        self.deactivate(id, Desired::Removed { purge })
    }

    fn deactivate(&self, id: &str, desired: Desired) -> Result<(), Refusal> {
        self.check_required_by(id, !matches!(desired, Desired::Removed { .. }))?;
        let mut receipt = self.receipt(id)?.ok_or("plugin is not installed")?;
        if matches!(receipt.desired, Desired::Removed { .. }) {
            // A prior removal keeps its original purge choice. Retrying the
            // command resumes that request instead of replacing it.
            self.restore_one(id)?;
            return self.publish_registry();
        }
        // Do not restore a pending enabled selection before stopping it. The
        // immutable approval still authorizes cleanup, not a new daemon start.
        let report = self.approved(&receipt)?;
        // Persist both disable and removal before cleanup: failure or a crash
        // must never roll back this intent into a (possibly revoked) start.
        receipt.desired = desired;
        self.invalidate_registry()?;
        // Existing pending root journals unfinished cleanup even when unit
        // shutdown succeeded but firewall cleanup or receipt settlement failed.
        self.journal_cleanup(&receipt)?;
        storage::write_json(&self.receipt_path(id), &receipt)?;
        self.restore_one(id)?;
        // Restore the f3aa66d91 stop cascade: dependents stop first; an
        // independent installed dependency stays selected, and stops only
        // when no enabled plugin still requires it. Never hide cleanup failure.
        for dep in report.brings.iter().rev() {
            let dep = &dep.report;
            if self
                .receipt(&dep.id)?
                .is_some_and(|r| matches!(r.desired, Desired::Enabled))
                && self.check_required_by_enabled(&dep.id).is_ok()
            {
                self.deactivate(&dep.id, Desired::Disabled)?;
            }
        }
        self.publish_registry()
    }

    pub fn status(&self, id: &str) -> Result<Option<Receipt>, Refusal> {
        self.graph_update_pending()?;
        validate_id(id)?;
        self.receipt(id)
    }

    /// The local registry's admission boundary. The host lock makes the list a
    /// single selection snapshot. No package realization, receipt repair or
    /// callback execution occurs. Signature verification may consult the bound
    /// cache; pending transitions never publish launch authority.
    pub fn enabled_packages(&self) -> Result<Vec<Report>, Refusal> {
        self.graph_update_pending()?;
        // Every install, enable, disable, update and rollback republishes this
        // list. Loading each enabled graph separately costs several Nix calls
        // and a declaration evaluation per plugin, so a batch of N changes did
        // O(N^2) work. Read store metadata once and load the graphs together,
        // exactly as boot restore does. The same approval, dependency and
        // signature checks still run on every report; a cached report is used
        // only for the exact package its receipt selects.
        let checks = self.boot_checks();
        let mut reports = Vec::new();
        for receipt in self.receipts()? {
            if !matches!(receipt.desired, Desired::Enabled) {
                continue;
            }
            self.settled(&receipt)?;
            let report = self.approved_in(&receipt, checks.as_ref())?;
            self.check_dependencies(&report, true, checks.as_ref())?;
            self.verify_report(
                &report,
                checks.as_ref(),
                package::StoreContents::TrustRegistered,
            )?;
            reports.push(report);
        }
        Ok(reports)
    }

    /// An enabled receipt is listed only when its selection is committed and
    /// its active root pins exactly the receipt's package.
    fn settled(&self, receipt: &Receipt) -> Result<(), Refusal> {
        if fs::symlink_metadata(self.root(&receipt.id, "pending")).is_ok() {
            return Err(format!("plugin {} has an unfinished selection", receipt.id).into());
        }
        if fs::read_link(self.root(&receipt.id, "active")).map_err(|e| e.to_string())?
            != receipt.package
        {
            return Err(format!("plugin {} has an inconsistent active root", receipt.id).into());
        }
        Ok(())
    }

    fn receipts(&self) -> Result<Vec<Receipt>, Refusal> {
        let mut receipts = Vec::new();
        for entry in fs::read_dir(self.state.root()).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == "publishers.json"
                || entry.file_name() == "publishers.new"
                || entry.file_name() == "lock"
                || entry.file_name() == storage::SOURCES_DIR
                || entry.file_name() == storage::STAGING_DIR
                || entry.file_name() == graph_update::JOURNAL
                || entry.file_name() == "graph-update.new"
            {
                continue;
            }
            let name = entry.file_name();
            if !name
                .to_str()
                .and_then(|name| name.strip_prefix("korri-plugin-"))
                .is_some_and(|suffix| {
                    suffix.len() == 64 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
            {
                return Err("unexpected entry in plugin state directory".into());
            }
            storage::directory(&entry.path())?;
            if let Some(receipt) =
                storage::read_json::<Receipt>(&entry.path().join("selection.json"))?
            {
                validate_id(&receipt.id)?;
                if self.directory(&receipt.id) != entry.path() {
                    return Err("receipt identity does not match its directory".into());
                }
                receipts.push(receipt);
            }
        }
        receipts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(receipts)
    }

    pub fn restore_all(&self) -> Result<(), Refusal> {
        self.invalidate_registry()?;
        self.recover_graph_update()?;
        let mut errors = Vec::new();
        if let Err(error) = self.state.cleanup_staging() {
            errors.push(error.to_string());
        }
        self.release_download()?;
        let mut enabled = Vec::new();
        let mut stopped = Vec::new();
        let checks = self.boot_checks();
        // Assess every selected graph before any native stop, denial or repair.
        // Unknown relationships refuse the pass with authority removed;
        // filesystem enumeration is never a native lifecycle order.
        let receipts = self.receipts()?;
        let mut released = std::collections::BTreeSet::new();
        for receipt in &receipts {
            if matches!(receipt.desired, Desired::Removed { .. })
                && self.selection(&receipt.id).software_released()?
                && !self.units_owned(&receipt.id)?
            {
                released.insert(receipt.package.clone());
            }
        }
        let mut edges = BTreeMap::new();
        let order = crate::dependencies::dependency_order_with::<Refusal>(
            receipts.iter().map(|r| r.package.clone()),
            |path| {
                let requires = if released.contains(path) {
                    Vec::new()
                } else if let Some(receipt) = receipts.iter().find(|r| r.package == *path) {
                    self.runtime_requires(receipt)?
                } else {
                    self.selection_requires(path)?
                };
                edges.insert(path.clone(), requires.clone());
                Ok(requires)
            },
        )?;
        let mut plans = BTreeMap::new();
        let mut stop = std::collections::BTreeSet::new();
        for receipt in &receipts {
            self.prepare(&receipt.id)?;
            let report = if released.contains(&receipt.package) {
                None
            } else {
                Some(self.approved_in(receipt, checks.as_ref()))
            };
            let authorized = if matches!(receipt.desired, Desired::Enabled) {
                self.verify_publisher_in(
                    checks.as_ref().map(|checks| &checks.snapshot),
                    &receipt.package,
                    &receipt.provenance,
                    package::StoreContents::TrustRegistered,
                )
            } else {
                Ok(())
            };
            let healthy = matches!(receipt.desired, Desired::Enabled)
                && authorized.is_ok()
                && self.settled(receipt).is_ok()
                && match &report {
                    Some(Ok(report)) => self.units_matches_running(report).unwrap_or(false),
                    _ => false,
                };
            if !healthy {
                stop.insert(receipt.package.clone());
            }
            plans.insert(
                receipt.package.clone(),
                RestorePlan {
                    report,
                    authorized,
                    healthy,
                    owned: self.units_owned(&receipt.id)?,
                },
            );
        }
        // Expand to live or unfinished ancestors. Reversed dependency order
        // stops outer dependents first, including failed disable/removal intents.
        for path in &order {
            if edges[path].iter().any(|required| stop.contains(required)) {
                if let Some(receipt) = receipts.iter().find(|r| r.package == *path) {
                    if self.cleanup_unfinished(receipt)? {
                        stop.insert(path.clone());
                    }
                }
            }
        }
        let mut quiesced = std::collections::BTreeSet::new();
        let mut blocked = std::collections::BTreeSet::new();
        for path in order.iter().rev().filter(|path| stop.contains(*path)) {
            let Some(receipt) = receipts.iter().find(|r| r.package == *path) else {
                continue;
            };
            let plan = &plans[path];
            let result = (|| {
                self.check_stop(&receipt.id, &quiesced)?;
                if !matches!(receipt.desired, Desired::Enabled) {
                    if let Some(report) = &plan.report {
                        report.as_ref().map_err(Clone::clone)?;
                    }
                }
                if !released.contains(path) {
                    self.journal_cleanup(receipt)?;
                }
                let purge = matches!(receipt.desired, Desired::Removed { purge: true });
                self.units_stop(&receipt.id, purge)
            })();
            match result {
                Ok(()) => {
                    quiesced.insert(path.clone());
                }
                Err(error) => {
                    blocked.insert(path.clone());
                    errors.push(format!(
                        "plugin {}: stop refused or cleanup failed: {error}",
                        receipt.id
                    ));
                }
            }
        }
        for entry in fs::read_dir(self.state.root()).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == "publishers.json"
                || entry.file_name() == "publishers.new"
                || entry.file_name() == "lock"
            {
                continue;
            }
            let result = if entry.file_name() == storage::SOURCES_DIR
                || entry.file_name() == storage::STAGING_DIR
            {
                storage::directory(&entry.path())
                    .map(|()| Restored::Nothing)
                    .map_err(Refusal::from)
            } else {
                self.restore_directory(&entry.path(), &plans, &quiesced, &blocked)
            };
            match result {
                Ok(Restored::Running(receipt, report)) => enabled.push((receipt, report)),
                Ok(Restored::Stopped(receipt, report)) => stopped.push((receipt, report)),
                Ok(Restored::Nothing) => {}
                Err(error) => errors.push(error.to_string()),
            }
        }
        // Start one dependency level per systemd batch. A failed node blocks
        // only its dependents; unrelated leaf plugins retain the single batch.
        let mut waiting: BTreeMap<_, _> = enabled
            .drain(..)
            .map(|(receipt, report)| (receipt.package.clone(), (receipt, report, true)))
            .chain(
                stopped
                    .into_iter()
                    .map(|(receipt, report)| (receipt.package.clone(), (receipt, report, false))),
            )
            .collect();
        let mut outcomes = BTreeMap::new();
        while !waiting.is_empty() {
            let mut ready: Vec<_> = waiting
                .iter()
                .filter(|(_, (_, report, _))| {
                    report
                        .requires
                        .iter()
                        .all(|path| !waiting.contains_key(path))
                })
                .map(|(path, _)| path.clone())
                .collect();
            let cycle = ready.is_empty();
            if cycle {
                ready = waiting.keys().cloned().collect();
            }
            let mut start = Vec::new();
            for path in ready {
                let (receipt, report, running) = waiting.remove(&path).unwrap();
                if cycle
                    || report
                        .requires
                        .iter()
                        .any(|path| outcomes.get(path) != Some(&true))
                {
                    let error = format!(
                        "plugin {}: required plugin failed to restore or dependency cycle",
                        receipt.id
                    );
                    errors.push(error.to_string());
                    if let Err(error) = self.block_dependency_start(&receipt, &quiesced) {
                        errors.push(error.to_string());
                    }
                    outcomes.insert(path, false);
                } else if running {
                    // Even healthy dependents must wait for dependency
                    // authority before reopening their ports or settling roots.
                    match self.firewall_apply(&report).and_then(|()| {
                        self.selection(&receipt.id)
                            .settle(&receipt)
                            .map_err(Refusal::from)
                    }) {
                        Ok(()) => {
                            outcomes.insert(path, true);
                            enabled.push((receipt, report));
                        }
                        Err(error) => {
                            outcomes.insert(path, false);
                            errors.push(format!("plugin {}: {error}", receipt.id));
                            if !report.requires.is_empty() {
                                if let Err(error) = self.block_dependency_start(&receipt, &quiesced)
                                {
                                    errors.push(error.to_string());
                                }
                            }
                        }
                    }
                } else {
                    start.push((receipt, report));
                }
            }
            // Successful stop proofs expire before new effects are attempted.
            for (receipt, _) in &start {
                quiesced.remove(&receipt.package);
            }
            let reports: Vec<_> = start.iter().map(|(_, report)| report).collect();
            let started = if reports.is_empty() {
                Vec::new()
            } else {
                self.units_start_all(&reports)
            };
            for ((receipt, report), result) in start.into_iter().zip(started) {
                match result.and_then(|()| {
                    self.selection(&receipt.id)
                        .settle(&receipt)
                        .map_err(Refusal::from)
                }) {
                    Ok(()) => {
                        outcomes.insert(receipt.package.clone(), true);
                        enabled.push((receipt, report));
                    }
                    Err(error) => {
                        outcomes.insert(receipt.package.clone(), false);
                        errors.push(format!("plugin {}: {error}", receipt.id));
                        if !report.requires.is_empty() {
                            if let Err(error) = self.block_dependency_start(&receipt, &quiesced) {
                                errors.push(error.to_string());
                            }
                        }
                    }
                }
            }
        }
        // A plugin that failed to restore is left out of the list, and its
        // error still fails the command. It must not take the others with it:
        // one plugin service losing a boot-time race (Sunshine on the RG353M,
        // 2026-10-01) left every plugin unavailable.
        //
        // Restore already checked each enabled plugin's approval, declaration
        // and publisher signature. Publish those reports; checking them again
        // would repeat the slowest part of boot. Single-plugin operations
        // publish through enabled_packages(), and a test holds the two lists
        // equal.
        enabled.sort_by(|(a, _), (b, _)| a.id.cmp(&b.id));
        let mut reports = Vec::new();
        for (receipt, report) in enabled {
            match self.settled(&receipt) {
                Ok(()) => reports.push(report),
                Err(error) => errors.push(error.to_string()),
            }
        }
        self.write_registry(reports)?;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; ").into())
        }
    }

    /// Boot-time checks prepared up front: store metadata for every enabled
    /// plugin from one Nix call, and each plugin's report loaded on all
    /// cores. Only a speed-up: a plugin missing here is loaded on its own, and
    /// any failure to prepare means no cache, never a refused plugin.
    fn boot_checks(&self) -> Option<BootChecks> {
        #[cfg(test)]
        if self.test_runtime.is_some() {
            return None;
        }
        let mut receipts = Vec::new();
        for entry in fs::read_dir(self.state.root()).ok()?.flatten() {
            if let Ok(Some(receipt)) =
                storage::read_json::<Receipt>(&entry.path().join("selection.json"))
            {
                if matches!(receipt.desired, Desired::Enabled)
                    && package::validate_store_path(&receipt.package).is_ok()
                {
                    receipts.push(receipt);
                }
            }
        }
        if receipts.is_empty() {
            return None;
        }
        let packages: Vec<&Path> = receipts.iter().map(|r| r.package.as_path()).collect();
        let snapshot = package::StoreSnapshot::query(&self.nix, &packages).ok()?;
        // A corrupt unrelated receipt must not prevent boot checks for an
        // independent graph. Invalid state is still reported by restore_directory.
        let selected: Vec<_> = fs::read_dir(self.state.root())
            .ok()?
            .flatten()
            .filter_map(|entry| {
                let receipt =
                    storage::read_json::<Receipt>(&entry.path().join("selection.json")).ok()??;
                (validate_id(&receipt.id).is_ok() && self.directory(&receipt.id) == entry.path())
                    .then_some(receipt)
            })
            .collect();
        let nix = &self.nix;
        let publishers = &self.publishers;
        let loaded = crate::parallel::map(&receipts, |receipt| {
            // Each graph uses the same metadata snapshot and independently
            // resolves dependency provenance from receipts or bound caches.
            package::load_resolved_graph(
                nix,
                Some(&snapshot),
                &receipt.package,
                receipt.provenance.clone(),
                |path| Self::locked_releases(path, &selected),
                |path| dependency_provenance(path, &selected, publishers),
            )
        });
        let loaded = loaded.into_iter().map(|result| {
            result.and_then(|mut report| {
                for dep in &mut report.brings {
                    dep.report.publisher = Some(self.report_publisher(&dep.report)?);
                }
                report.publisher = Some(self.report_publisher(&report)?);
                Ok(report)
            })
        });
        let reports = receipts
            .into_iter()
            .zip(loaded)
            .map(|(receipt, report)| (receipt.id, (receipt.package, report)))
            .collect();
        Some(BootChecks { snapshot, reports })
    }

    /// Restore one state directory. An enabled plugin comes back with its
    /// checked report, running or stopped for the caller to start. An error
    /// names the plugin it belongs to.
    fn restore_directory(
        &self,
        path: &Path,
        plans: &BTreeMap<PathBuf, RestorePlan>,
        quiesced: &std::collections::BTreeSet<PathBuf>,
        blocked: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<Restored, Refusal> {
        storage::directory(path)?;
        if let Some(receipt) = storage::read_json::<Receipt>(&path.join("selection.json"))? {
            if self.directory(&receipt.id) != path {
                return Err("receipt identity does not match its directory".into());
            }
            let id = receipt.id.clone();
            if blocked.contains(&receipt.package) {
                return Ok(Restored::Nothing);
            }
            let plan = plans
                .get(&receipt.package)
                .ok_or("selection changed during restore planning")?;
            return self
                .restore_receipt(receipt, plan, quiesced)
                .map_err(|error| Refusal::new(error.category, format!("plugin {id}: {error}")));
        }
        // Install is always disabled. Without a committed receipt it
        // cannot have started a daemon. A completed removal also reaches
        // this state only after stop/cleanup succeeded.
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid state directory")?;
        if !name
            .strip_prefix("korri-plugin-")
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("unexpected entry in plugin state directory".into());
        }
        let roots = self.paths.roots.join(name);
        storage::directory(&roots)?;
        SelectionStore::new(path.join("selection.json"), roots).remove()?;
        Ok(Restored::Nothing)
    }

    fn restore_receipt(
        &self,
        receipt: Receipt,
        plan: &RestorePlan,
        quiesced: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<Restored, Refusal> {
        self.prepare(&receipt.id)?;
        if !matches!(receipt.desired, Desired::Enabled) {
            self.restore_one_in(&receipt.id, quiesced, Some(plan))?;
            return Ok(Restored::Nothing);
        }
        let report = match plan
            .report
            .as_ref()
            .ok_or("enabled selection has no approval assessment")?
            .clone()
        {
            Ok(report) => report,
            Err(error) => {
                // Corrupt approval is never a reason to leave old units running.
                self.deny_start_in(&receipt, quiesced)?;
                return Err(error);
            }
        };
        if plan.healthy && !quiesced.contains(&receipt.package) {
            return Ok(Restored::Running(receipt, report));
        }
        // Stop first even when authority was revoked. Keep the receipt and
        // roots on denial; never restart revoked code.
        self.check_stop(&receipt.id, quiesced)?;
        if !quiesced.contains(&receipt.package) {
            self.units_stop(&receipt.id, false)?;
        }
        plan.authorized.clone()?;
        Ok(Restored::Stopped(receipt, report))
    }

    fn deny_start(&self, receipt: &Receipt) -> Result<(), Refusal> {
        self.deny_start_in(receipt, &std::collections::BTreeSet::new())
    }

    fn deny_start_in(
        &self,
        receipt: &Receipt,
        quiesced: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<(), Refusal> {
        self.invalidate_registry()?;
        self.check_stop(&receipt.id, quiesced)?;
        self.journal_cleanup(receipt)?;
        let mut blocked = receipt.clone();
        blocked.desired = Desired::Disabled;
        storage::write_json(&self.receipt_path(&receipt.id), &blocked)?;
        // Approval failure cannot authorize root repair. Keep all existing
        // selection roots, including a pending candidate, until checked cleanup.
        if quiesced.contains(&receipt.package) {
            Ok(())
        } else {
            self.units_stop(&receipt.id, false)
        }
    }

    fn block_dependency_start(
        &self,
        receipt: &Receipt,
        quiesced: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<(), Refusal> {
        self.deny_start_in(receipt, quiesced)?;
        let mut blocked = receipt.clone();
        blocked.desired = Desired::Disabled;
        self.selection(&receipt.id)
            .settle(&blocked)
            .map_err(Refusal::from)
    }

    fn apply(&self, candidate: Receipt) -> Result<(), Refusal> {
        let id = &candidate.id;
        let report = self.approved(&candidate)?;
        let changes_selection = self.receipt(id)?.is_some_and(|old| {
            old.package != candidate.package
                || old.provenance != candidate.provenance
                || old.approval != candidate.approval
        });
        self.check_required_by(id, !changes_selection)?;
        self.check_dependencies(&report, matches!(candidate.desired, Desired::Enabled), None)?;
        self.verify_report(&report, None, package::StoreContents::Rehash)?;
        self.invalidate_registry()?;
        self.selection(id).stage(&candidate.package)?;
        let result = self.units_stop(id, false).and_then(|_| {
            if matches!(candidate.desired, Desired::Enabled) {
                self.units_start(&report)
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            return match self.restore_one(id) {
                Ok(()) => Err(format!("operation failed; committed selection restored: {error}").into()),
                Err(recovery) => Err(format!("operation failed: {error}; recovery failed: {recovery}; package remains pinned; run restore-all").into()),
            };
        }
        self.selection(id).commit(&candidate)?;
        self.publish_registry()
    }

    fn invalidate_registry(&self) -> Result<(), Refusal> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let directory = &self.paths.registry_directory;
        if !directory.exists() {
            fs::create_dir(directory).map_err(|e| e.to_string())?;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        let metadata = fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
        if !metadata.is_dir()
            || metadata.uid() != self.owner_uid
            || metadata.permissions().mode() & 0o022 != 0
        {
            return Err("plugin registry directory must be protected by root".into());
        }
        storage::remove(&self.paths.registry_path).map_err(Refusal::from)
    }

    fn publish_registry(&self) -> Result<(), Refusal> {
        self.write_registry(self.enabled_packages()?)
    }

    fn write_registry(&self, reports: Vec<Report>) -> Result<(), Refusal> {
        let selections: Vec<_> = reports
            .into_iter()
            .map(|report| crate::plugin_installation::EnabledPackage {
                id: report.id,
                package: report.package,
                files: report.files,
                entry: report.entry,
                sources: report.sources,
                requires: report.requires,
                hosts: report.hosts,
            })
            .collect();
        #[cfg(test)]
        let fail_before = self
            .test_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.fail_registry.get());
        #[cfg(not(test))]
        let fail_before = false;
        let result = if fail_before {
            Err("injected enabled projection write failure".into())
        } else {
            storage::write_atomic_mode(
                &self.paths.registry_path,
                &crate::plugin_installation::encode(&selections)?,
                0o644,
            )
        };
        #[cfg(test)]
        let result = result.and_then(|()| {
            if self
                .test_runtime
                .as_ref()
                .is_some_and(|runtime| runtime.fail_registry_after_rename.get())
            {
                Err("injected projection durability failure after rename".into())
            } else {
                Ok(())
            }
        });
        match result {
            Ok(()) => Ok(()),
            Err(error) => match self.invalidate_registry() {
                Ok(()) => Err(error.into()),
                Err(cleanup) => {
                    Err(format!("{error}; failed to withdraw enabled projection: {cleanup}").into())
                }
            },
        }
    }

    fn reclaim_software(&self) -> Result<(), Refusal> {
        SoftwareCleanup::new(self.nix.clone())
            .reclaim()
            .map_err(Refusal::from)
    }

    fn units_stop(&self, id: &str, purge: bool) -> Result<(), Refusal> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            runtime
                .events
                .borrow_mut()
                .push(format!("stop:{id}:{purge}"));
            // Model retained runtime ownership, not just whether a daemon is
            // running. Failed ExecStopPost may need its required daemon too.
            let target = runtime
                .owned_packages
                .borrow()
                .get(id)
                .cloned()
                .or_else(|| self.receipt(id).ok().flatten().map(|r| r.package));
            if let Some(target) = target {
                for (parent, path) in runtime.owned_packages.borrow().iter() {
                    if parent == id {
                        continue;
                    }
                    let order = crate::dependencies::dependency_order_with::<Refusal>(
                        [path.clone()],
                        |path| self.selection_requires(path),
                    )
                    .unwrap();
                    if order.contains(&target) {
                        runtime
                            .unsafe_stops
                            .borrow_mut()
                            .push(format!("stop:{id} while {parent} owns effects"));
                    }
                }
            }
            if runtime.fail_stop.borrow().as_deref() == Some(id) {
                return Err(format!("injected stop failure for {id}").into());
            }
            runtime.running.borrow_mut().remove(id);
            runtime.owned.borrow_mut().remove(id);
            runtime.owned_packages.borrow_mut().remove(id);
            return Ok(());
        }
        self.units.stop(id, purge).map_err(Refusal::from)
    }

    fn units_start(&self, report: &Report) -> Result<(), Refusal> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            runtime
                .events
                .borrow_mut()
                .push(format!("start:{}", report.id));
            if runtime.fail_start.borrow().as_deref() == Some(report.id.as_str())
                || runtime.fail_start_package.borrow().as_deref() == Some(report.package.as_path())
            {
                return Err(format!("injected start failure for {}", report.id).into());
            }
            runtime.running.borrow_mut().insert(report.id.clone());
            runtime.owned.borrow_mut().insert(report.id.clone());
            runtime
                .owned_packages
                .borrow_mut()
                .insert(report.id.clone(), report.package.clone());
            return Ok(());
        }
        self.units.start(report).map_err(Refusal::from)
    }

    fn units_start_all(&self, reports: &[&Report]) -> Vec<Result<(), Refusal>> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            let ids: Vec<&str> = reports.iter().map(|report| report.id.as_str()).collect();
            runtime
                .events
                .borrow_mut()
                .push(format!("start-batch:{}", ids.join(",")));
            return reports
                .iter()
                .map(|report| self.units_start(report))
                .collect();
        }
        self.units
            .start_all(reports)
            .into_iter()
            .map(|result| result.map_err(Refusal::from))
            .collect()
    }

    fn units_matches_running(&self, report: &Report) -> Result<bool, Refusal> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            return Ok(runtime.running.borrow().contains(&report.id));
        }
        self.units.matches_running(report).map_err(Refusal::from)
    }

    fn firewall_apply(&self, report: &Report) -> Result<(), Refusal> {
        #[cfg(test)]
        if self.test_runtime.is_some() {
            return Ok(());
        }
        self.units
            .firewall
            .apply(&report.id, &report.ports)
            .map_err(Refusal::from)
    }

    fn approved(&self, receipt: &Receipt) -> Result<Report, Refusal> {
        self.approved_in(receipt, None)
    }

    fn approved_in(
        &self,
        receipt: &Receipt,
        checks: Option<&BootChecks>,
    ) -> Result<Report, Refusal> {
        validate_id(&receipt.id)?;
        let load = || {
            if let Some((_, report)) = checks
                .and_then(|checks| checks.reports.get(&receipt.id))
                .filter(|(package, _)| *package == receipt.package)
            {
                return report.clone();
            }
            self.report_for(
                &receipt.package,
                receipt.provenance.clone(),
                checks.map(|checks| &checks.snapshot),
            )
        };
        #[cfg(test)]
        let report = if let Some(runtime) = self.test_runtime.as_ref().filter(|r| !r.real_packages)
        {
            runtime
                .events
                .borrow_mut()
                .push(format!("load:{}", receipt.id));
            runtime
                .reports
                .borrow()
                .get(&receipt.package)
                .cloned()
                .ok_or_else(|| format!("missing test report for {}", receipt.package.display()))?
        } else {
            load()?
        };
        #[cfg(not(test))]
        let report = load()?;
        if report.id != receipt.id || report.approval != receipt.approval {
            return Err(Refusal::new(Category::NeedsReview, APPROVAL_STALE));
        }
        Ok(report)
    }

    fn recover_one(&self, id: &str) -> Result<(), Refusal> {
        let pending = fs::symlink_metadata(self.root(id, "pending")).is_ok();
        let removed = self
            .receipt(id)?
            .is_some_and(|r| matches!(r.desired, Desired::Removed { .. }));
        if pending || removed {
            self.restore_one(id)?;
        }
        Ok(())
    }

    /// Recovery for single-plugin operations. Boot restore handles enabled
    /// plugins itself and reaches this only for disabled or removed ones.
    fn restore_one(&self, id: &str) -> Result<(), Refusal> {
        self.restore_one_in(id, &std::collections::BTreeSet::new(), None)
    }

    fn restore_one_in(
        &self,
        id: &str,
        quiesced: &std::collections::BTreeSet<PathBuf>,
        plan: Option<&RestorePlan>,
    ) -> Result<(), Refusal> {
        self.check_stop(id, quiesced)?;
        let receipt = self.receipt(id)?;
        match receipt {
            Some(receipt) => {
                let selection = self.selection(id);
                if let Desired::Removed { purge } = receipt.desired {
                    if !selection.software_released()? {
                        let report = match plan.and_then(|plan| plan.report.as_ref()) {
                            Some(report) => report.clone()?,
                            None => self.approved(&receipt)?,
                        };
                        let was_owned = match plan {
                            Some(plan) => plan.owned,
                            None => self.units_owned(id)?,
                        };
                        if purge && !was_owned {
                            self.units.purge_inactive(&report)?;
                        } else if !quiesced.contains(&receipt.package) {
                            self.units_stop(id, purge)?;
                        }
                        // Keep the receipt as the durable removal request while
                        // both retained selections are released. Root removal
                        // is not completion until Nix confirms cleanup.
                        selection.release_software()?;
                    }
                    // Inspection roots are acquisition authority only. Neither
                    // their committed nor interrupted name may retain removed
                    // software while cleanup reports success.
                    self.release_download()?;
                    self.reclaim_software()?;
                    selection.finish_removal()?;
                } else {
                    let report = match plan.and_then(|plan| plan.report.as_ref()) {
                        Some(report) => report.clone()?,
                        None => self.approved(&receipt)?,
                    };
                    if !quiesced.contains(&receipt.package) {
                        self.units_stop(id, false)?;
                    }
                    if matches!(receipt.desired, Desired::Enabled) {
                        // Stop first even when authority was revoked. Keep the
                        // receipt and roots on denial; never restart revoked code.
                        self.check_dependencies(&report, true, None)?;
                        self.verify_report(&report, None, package::StoreContents::Rehash)?;
                        self.units_start(&report)?;
                    }
                    selection.settle(&receipt)?;
                }
            }
            None => {
                self.units_stop(id, false)?;
                self.selection(id).remove().map_err(Refusal::from)?;
            }
        }
        Ok(())
    }

    fn prepare(&self, id: &str) -> Result<(), Refusal> {
        validate_id(id)?;
        storage::directory(&self.directory(id))?;
        storage::directory(&self.paths.roots.join(package::unit_name(id))).map_err(Refusal::from)
    }
    fn directory(&self, id: &str) -> PathBuf {
        self.state.root().join(package::unit_name(id))
    }
    fn root(&self, id: &str, name: &str) -> PathBuf {
        self.paths.roots.join(package::unit_name(id)).join(name)
    }
    fn receipt_path(&self, id: &str) -> PathBuf {
        self.directory(id).join("selection.json")
    }
    fn selection(&self, id: &str) -> SelectionStore {
        SelectionStore::new(
            self.receipt_path(id),
            self.paths.roots.join(package::unit_name(id)),
        )
    }
    fn receipt(&self, id: &str) -> Result<Option<Receipt>, Refusal> {
        self.selection(id).read().map_err(Refusal::from)
    }
}

fn dependency_provenance(
    path: &Path,
    receipts: &[Receipt],
    publishers: &package::PublisherBindings,
) -> Result<Provenance, Refusal> {
    if let Some(receipt) = receipts.iter().find(|receipt| receipt.package == path) {
        return Ok(receipt.provenance.clone());
    }
    // Cleanup and re-review of the displaced root still need the exact source
    // of its old dependencies. The existing previous selection owns that fact.
    if let Some(previous) = receipts
        .iter()
        .filter_map(|r| r.previous.as_ref())
        .find(|p| p.package == path)
    {
        return Ok(previous.provenance.clone());
    }
    let namespace = package::manifest_namespace(path)?;
    let binding = publishers
        .get(&namespace)
        .ok_or_else(|| format!("publisher {namespace} is not bound on this device"))?;
    Ok(Provenance::RawCache {
        cache_url: binding.cache_url.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        declaration::Declaration,
        firewall::Ports,
        lifecycle::RequiredPlugin,
        package::{Report, BASE_POLICY},
    };
    use std::{
        collections::BTreeMap,
        os::unix::{
            fs::{symlink, PermissionsExt},
            process::ExitStatusExt,
        },
        process::{Child, Command},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };

    fn report(id: &str, suffix: &str) -> Report {
        let (namespace, name) = id.split_once(':').unwrap();
        let package = PathBuf::from(format!(
            "/nix/store/00000000000000000000000000000000-{suffix}"
        ));
        let provenance = Provenance::RawCache {
            cache_url: "https://cache.example".into(),
        };
        Report {
            publisher: None,
            id: id.into(),
            package,
            provenance,
            approval: format!("{suffix}-approval"),
            policy: BASE_POLICY,
            warning: String::new(),
            unit: None,
            state_directory: format!("/var/lib/private/{}", package::unit_name(id)),
            runtime_directory: format!("/run/{}", package::unit_name(id)),
            unit_configuration: String::new(),
            declaration: Declaration::evaluate(
                namespace,
                &format!("export const name = {name:?}; export const services = [];"),
            )
            .unwrap(),
            native_units: BTreeMap::new(),
            ports: Ports::default(),
            hosts: Vec::new(),
            packages: BTreeMap::new(),
            files: BTreeMap::new(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: Vec::new(),
            brings: Vec::new(),
        }
    }

    fn permitted_policy() -> LifecyclePolicy {
        LifecyclePolicy::new(Vec::new()).unwrap()
    }

    fn runtime(reports: &[Report]) -> TestRuntime {
        let runtime = TestRuntime::default();
        for report in reports {
            runtime
                .reports
                .borrow_mut()
                .insert(report.package.clone(), report.clone());
        }
        runtime
    }

    fn cold_boot(runtime: &TestRuntime) {
        // Explicit power-off simulation: no live daemons or /run ownership.
        runtime.running.borrow_mut().clear();
        runtime.owned.borrow_mut().clear();
        runtime.owned_packages.borrow_mut().clear();
    }

    fn cleanup_program(root: &Path, body: &str) -> PathBuf {
        let path = root.join("nix");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    fn install(host: &Host, report: Report) {
        let approval = report.approval.clone();
        host.install(report, &approval, SelectionIntent::Install)
            .unwrap();
    }

    #[test]
    fn publishers_emit_only_bound_or_trusted_with_sheet_fingerprints() {
        let root = tempfile::tempdir().unwrap();
        let mut host = Host::for_test(
            root.path(),
            "/unavailable-nix".into(),
            permitted_policy(),
            runtime(&[]),
        )
        .unwrap();
        let key = "label:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        host.trust_publisher("@pinned", key).unwrap();
        host.publishers.insert(
            "@bound".into(),
            package::PublisherBinding {
                public_key: key.into(),
                cache_url: "https://cache.invalid/".into(),
            },
        );
        let list = host.publishers().unwrap();
        assert_eq!(list.entries.len(), 2);
        assert!(list.entries.iter().all(|entry| matches!(
            entry.state,
            crate::plugin_admin::PluginPublisherState::Bound
                | crate::plugin_admin::PluginPublisherState::Trusted
        )));
        assert!(list
            .entries
            .iter()
            .all(|entry| entry.fingerprint == "label AAAA AAAA AAAA AAAA"));
    }

    #[test]
    fn host_retains_two_selections_then_reclaims_only_after_releasing_all_roots() {
        let root = tempfile::tempdir().unwrap();
        let calls = root.path().join("gc-calls");
        let nix = cleanup_program(
            root.path(),
            &format!("printf '%s\\n' \"$*\" >> {}", calls.display()),
        );
        let first = report("@test:clock", "clock-v1");
        let second = report("@test:clock", "clock-v2");
        let runtime = runtime(&[first.clone(), second.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime).unwrap();
        install(&host, first.clone());
        host.install(
            second.clone(),
            &second.approval,
            SelectionIntent::Update { id: &second.id },
        )
        .unwrap();
        let selected = host.status(&second.id).unwrap().unwrap();
        assert_eq!(selected.package, second.package);
        assert_eq!(selected.previous.unwrap().package, first.package);
        let user_data = root.path().join("user-data");
        fs::write(&user_data, "keep").unwrap();
        symlink(&second.package, host.paths.roots.join("download")).unwrap();
        symlink(&first.package, host.paths.roots.join("download.new")).unwrap();

        host.remove(&second.id, false).unwrap();

        assert!(host.status(&second.id).unwrap().is_none());
        for name in ["download", "download.new"] {
            assert!(!host.paths.roots.join(name).exists());
        }
        assert_eq!(fs::read_to_string(user_data).unwrap(), "keep");
        assert!(fs::read_to_string(calls).unwrap().contains("store gc"));
    }

    #[test]
    fn host_cleanup_failure_is_visible_and_keeps_removal_incomplete() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "echo cleanup-failed >&2; exit 42");
        let selected = report("@test:clock", "clock-failure");
        let host = Host::for_test(
            root.path(),
            nix,
            permitted_policy(),
            runtime(std::slice::from_ref(&selected)),
        )
        .unwrap();
        install(&host, selected.clone());
        symlink(&selected.package, host.paths.roots.join("download")).unwrap();
        symlink(&selected.package, host.paths.roots.join("download.new")).unwrap();
        let user_data = root.path().join("user-data");
        fs::write(&user_data, "keep").unwrap();

        let error = host.remove(&selected.id, false).unwrap_err();

        assert!(error.contains("storage cleanup failed"), "{error}");
        assert!(matches!(
            host.status(&selected.id).unwrap().unwrap().desired,
            Desired::Removed { purge: false }
        ));
        assert!(host.selection(&selected.id).software_released().unwrap());
        assert!(!host.paths.roots.join("download").exists());
        assert!(!host.paths.roots.join("download.new").exists());
        assert_eq!(fs::read_to_string(user_data).unwrap(), "keep");
    }

    #[test]
    fn ordinary_host_removal_uses_its_required_behavior_policy() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let selected = report("@test:clock", "clock-required");
        let lifecycle = LifecyclePolicy::new(vec![RequiredPlugin::new(
            &selected.id,
            "portal accepts local input",
        )
        .unwrap()])
        .unwrap();
        let host = Host::for_test(
            root.path(),
            nix,
            lifecycle,
            runtime(std::slice::from_ref(&selected)),
        )
        .unwrap();
        install(&host, selected.clone());
        let before = host.status(&selected.id).unwrap();

        let error = host.remove(&selected.id, false).unwrap_err();

        assert!(error.contains("portal accepts local input"), "{error}");
        assert_eq!(host.status(&selected.id).unwrap(), before);
    }

    #[test]
    fn release_update_reviews_every_required_report_and_is_atomic_at_the_host_boundary() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let required_a = report("@test:required-a", "required-a");
        let required_b = report("@test:required-b", "required-b");
        let optional = report("@test:optional", "optional");
        let runtime = runtime(&[required_a.clone(), required_b.clone(), optional.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        let review = host
            .review_release_update(
                vec![required_a.clone(), required_b.clone()],
                vec![optional.clone()],
            )
            .unwrap();
        assert_eq!(
            review
                .required_reports()
                .iter()
                .map(|report| report.id.as_str())
                .collect::<Vec<_>>(),
            ["@test:required-a", "@test:required-b"]
        );
        assert_eq!(review.optional_ids(), ["@test:optional"]);
        let selected = Arc::new(AtomicBool::new(false));
        let selected_for_call = selected.clone();
        let refusal = host
            .apply_release_update(review, &BTreeMap::new(), move || {
                selected_for_call.store(true, Ordering::SeqCst);
                Ok(())
            })
            .unwrap_err();
        assert!(refusal.contains("was not approved"), "{refusal}");
        assert!(!selected.load(Ordering::SeqCst));
        assert!(host.status(&required_a.id).unwrap().is_none());
        assert!(host.status(&required_b.id).unwrap().is_none());
        assert!(host.status(&optional.id).unwrap().is_none());

        runtime.fail_stop.replace(Some(required_b.id.clone()));
        let review = host
            .review_release_update(
                vec![required_a.clone(), required_b.clone()],
                vec![optional.clone()],
            )
            .unwrap();
        let approvals = BTreeMap::from([
            (required_a.id.clone(), required_a.approval.clone()),
            (required_b.id.clone(), required_b.approval.clone()),
        ]);
        let failure = host
            .apply_release_update(review, &approvals, || Ok(()))
            .unwrap_err();
        assert!(failure.contains("installation failed"), "{failure}");
        assert!(host.status(&required_a.id).unwrap().is_none());
        assert!(host.status(&required_b.id).unwrap().is_none());
        assert!(host.status(&optional.id).unwrap().is_none());
        runtime.fail_stop.replace(None);

        let review = host
            .review_release_update(
                vec![required_a.clone(), required_b.clone()],
                vec![optional.clone()],
            )
            .unwrap();
        host.apply_release_update(review, &approvals, || {
            selected.store(true, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        assert!(selected.load(Ordering::SeqCst));
        assert!(host.status(&required_a.id).unwrap().is_some());
        assert!(host.status(&required_b.id).unwrap().is_some());
        assert!(host.status(&optional.id).unwrap().is_none());
    }

    #[test]
    fn optional_recommendations_preserve_disable_and_removal_choices() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let optional = report("@test:optional", "optional-choice");
        let host = Host::for_test(
            root.path(),
            nix,
            permitted_policy(),
            runtime(std::slice::from_ref(&optional)),
        )
        .unwrap();
        install(&host, optional.clone());
        let disabled = host.status(&optional.id).unwrap();
        let review = host
            .review_release_update(Vec::new(), vec![optional.clone()])
            .unwrap();
        host.apply_release_update(review, &BTreeMap::new(), || Ok(()))
            .unwrap();
        assert_eq!(host.status(&optional.id).unwrap(), disabled);

        host.remove(&optional.id, false).unwrap();
        let review = host
            .review_release_update(Vec::new(), vec![optional.clone()])
            .unwrap();
        host.apply_release_update(review, &BTreeMap::new(), || Ok(()))
            .unwrap();
        assert!(host.status(&optional.id).unwrap().is_none());
    }

    fn published(host: &Host) -> (Vec<u8>, Vec<String>) {
        let bytes = fs::read(&host.paths.registry_path).unwrap();
        let ids = serde_json::from_slice::<Vec<crate::plugin_installation::EnabledPackage>>(&bytes)
            .unwrap()
            .into_iter()
            .map(|selection| selection.id)
            .collect();
        (bytes, ids)
    }

    fn loads(runtime: &TestRuntime) -> Vec<String> {
        let mut loads: Vec<_> = runtime
            .events
            .borrow()
            .iter()
            .filter(|event| event.starts_with("load:"))
            .cloned()
            .collect();
        loads.sort();
        loads
    }

    #[test]
    fn boot_restore_checks_each_plugin_once_and_publishes_the_list_an_enable_publishes() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let clock = report("@test:clock", "clock-boot");
        let idle = report("@test:idle", "idle-boot");
        let weather = report("@test:weather", "weather-boot");
        let runtime = runtime(&[clock.clone(), idle.clone(), weather.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        for selected in [&clock, &idle, &weather] {
            install(&host, selected.clone());
        }
        host.set_enabled(&clock.id, true).unwrap();
        host.set_enabled(&weather.id, true).unwrap();
        let (enable_list, ids) = published(&host);
        assert_eq!(ids, ["@test:clock", "@test:weather"]);
        runtime.events.borrow_mut().clear();

        host.restore_all().unwrap();

        assert_eq!(published(&host).0, enable_list);
        assert_eq!(
            loads(&runtime),
            ["load:@test:clock", "load:@test:idle", "load:@test:weather"]
        );
    }

    #[test]
    fn enabled_packages_project_the_declared_provider_hosts() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let mut shop = report("@test:shop", "shop-hosts");
        shop.hosts = vec!["www.lexaloffle.com".into()];
        let runtime = runtime(std::slice::from_ref(&shop));
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime).unwrap();
        install(&host, shop.clone());
        host.set_enabled(&shop.id, true).unwrap();
        let selections = serde_json::from_slice::<Vec<crate::plugin_installation::EnabledPackage>>(
            &published(&host).0,
        )
        .unwrap();
        assert_eq!(selections[0].hosts, ["www.lexaloffle.com"]);
    }

    #[test]
    fn boot_restore_starts_every_stopped_plugin_in_one_batch() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let clock = report("@test:clock", "clock-batch");
        let idle = report("@test:idle", "idle-batch");
        let stream = report("@test:stream", "stream-batch");
        let runtime = runtime(&[clock.clone(), idle.clone(), stream.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        for selected in [&clock, &idle, &stream] {
            install(&host, selected.clone());
        }
        host.set_enabled(&clock.id, true).unwrap();
        host.set_enabled(&stream.id, true).unwrap();
        cold_boot(&runtime);
        runtime.events.borrow_mut().clear();

        host.restore_all().unwrap();

        let batches: Vec<String> = runtime
            .events
            .borrow()
            .iter()
            .filter(|event| event.starts_with("start-batch:"))
            .cloned()
            .collect();
        assert_eq!(batches.len(), 1, "{batches:?}");
        let mut started: Vec<&str> = batches[0]["start-batch:".len()..].split(',').collect();
        started.sort();
        assert_eq!(started, ["@test:clock", "@test:stream"]);
        assert_eq!(published(&host).1, ["@test:clock", "@test:stream"]);
    }

    #[test]
    fn one_plugin_that_fails_to_start_at_boot_leaves_the_others_listed() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let clock = report("@test:clock", "clock-isolated");
        let stream = report("@test:stream", "stream-isolated");
        let runtime = runtime(&[clock.clone(), stream.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        for selected in [&clock, &stream] {
            install(&host, selected.clone());
            host.set_enabled(&selected.id, true).unwrap();
        }
        runtime.fail_start.replace(Some(stream.id.clone()));
        cold_boot(&runtime);

        let error = host.restore_all().unwrap_err();

        assert!(error.contains("plugin @test:stream"), "{error}");
        assert!(!error.contains("plugin @test:clock"), "{error}");
        assert_eq!(published(&host).1, ["@test:clock"]);
        assert!(matches!(
            host.status(&stream.id).unwrap().unwrap().desired,
            Desired::Enabled
        ));
    }

    #[test]
    fn boot_restore_finishes_an_interrupted_enabled_selection_and_lists_it() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let clock = report("@test:clock", "clock-pending");
        let runtime = runtime(std::slice::from_ref(&clock));
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        install(&host, clock.clone());
        host.set_enabled(&clock.id, true).unwrap();
        let (enable_list, _) = published(&host);
        host.selection(&clock.id).stage(&clock.package).unwrap();
        runtime.events.borrow_mut().clear();

        host.restore_all().unwrap();

        assert!(fs::symlink_metadata(host.root(&clock.id, "pending")).is_err());
        assert_eq!(published(&host).0, enable_list);
        assert_eq!(loads(&runtime), ["load:@test:clock"]);
        assert_eq!(
            runtime
                .events
                .borrow()
                .iter()
                .filter(|event| event.starts_with("start:"))
                .collect::<Vec<_>>(),
            ["start:@test:clock"]
        );
    }

    fn pack(id: &str, suffix: &str, dependencies: &[Report]) -> Report {
        let mut pack = report(id, suffix);
        pack.requires = dependencies.iter().map(|dep| dep.package.clone()).collect();
        for dep in dependencies {
            for nested in &dep.brings {
                if !pack
                    .brings
                    .iter()
                    .any(|entry| entry.report.package == nested.report.package)
                {
                    pack.brings.push(nested.clone());
                }
            }
            let mut flat = dep.clone();
            flat.brings.clear();
            pack.brings.push(package::DependencyReport {
                report: flat,
                already_approved: false,
            });
        }
        pack
    }

    #[test]
    fn approved_pack_installs_independent_dependencies_disabled_then_enables_in_order() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let mut fake08 = report("@runtime:fake08", "fake08");
        fake08.provenance = Provenance::RawCache {
            cache_url: "https://runtime.example/cache".into(),
        };
        let pack = pack(
            "@games:starter-pack",
            "starter-pack",
            std::slice::from_ref(&fake08),
        );
        let runtime = runtime(&[fake08.clone(), pack.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        assert!(host
            .install(pack.clone(), "not-approved", SelectionIntent::Install)
            .is_err());
        assert!(host.status(&fake08.id).unwrap().is_none());
        install(&host, pack.clone());
        for selected in [&fake08, &pack] {
            let receipt = host.status(&selected.id).unwrap().unwrap();
            assert_eq!(receipt.provenance, selected.provenance);
            assert_eq!(receipt.approval, selected.approval);
            assert_eq!(receipt.desired, Desired::Disabled);
            assert_eq!(
                fs::read_link(host.root(&selected.id, "active")).unwrap(),
                selected.package
            );
        }
        runtime.events.borrow_mut().clear();
        host.set_enabled(&pack.id, true).unwrap();
        let starts: Vec<_> = runtime
            .events
            .borrow()
            .iter()
            .filter(|event| event.starts_with("start:"))
            .cloned()
            .collect();
        assert_eq!(
            starts,
            ["start:@runtime:fake08", "start:@games:starter-pack"]
        );
        assert_eq!(
            published(&host).1,
            ["@games:starter-pack", "@runtime:fake08"]
        );
        assert!(host
            .set_enabled(&fake08.id, false)
            .unwrap_err()
            .contains("actively required"));
        assert!(host
            .remove(&fake08.id, false)
            .unwrap_err()
            .contains("actively required"));
        assert!(host.rollback(&fake08.id).is_err());
        host.set_enabled(&pack.id, false).unwrap();
        assert_eq!(
            host.status(&fake08.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
    }

    #[test]
    fn shared_dependency_stays_enabled_until_the_last_pack_stops() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let fake08 = report("@runtime:fake08", "shared-fake08");
        let a = pack("@games:a", "a", std::slice::from_ref(&fake08));
        let b = pack("@games:b", "b", std::slice::from_ref(&fake08));
        let runtime = runtime(&[fake08.clone(), a.clone(), b.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        install(&host, a.clone());
        install(&host, b.clone());
        host.set_enabled(&a.id, true).unwrap();
        host.set_enabled(&b.id, true).unwrap();
        assert_eq!(
            host.status(&a.id).unwrap().unwrap().desired,
            Desired::Enabled
        );
        host.set_enabled(&a.id, false).unwrap();
        assert_eq!(
            host.status(&fake08.id).unwrap().unwrap().desired,
            Desired::Enabled
        );
        host.set_enabled(&b.id, false).unwrap();
        assert_eq!(
            host.status(&fake08.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
    }

    #[test]
    fn conflicting_exact_dependency_is_refused_without_replacing_any_selection() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let first = report("@runtime:fake08", "fake08-first");
        let second = report("@runtime:fake08", "fake08-second");
        let pack = pack(
            "@games:starter-pack",
            "conflicting-pack",
            std::slice::from_ref(&second),
        );
        let runtime = runtime(&[first.clone(), second, pack.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime).unwrap();
        install(&host, first.clone());
        assert!(host
            .install(pack.clone(), &pack.approval, SelectionIntent::Install)
            .unwrap_err()
            .contains("conflicting exact"));
        assert_eq!(
            host.status(&first.id).unwrap().unwrap().package,
            first.package
        );
        assert!(host.status(&pack.id).unwrap().is_none());
    }

    #[test]
    fn failed_dependency_activation_never_enables_the_pack_and_boot_failure_blocks_only_dependents()
    {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let fake08 = report("@runtime:fake08", "failing-fake08");
        let pack = pack(
            "@games:starter-pack",
            "failing-pack",
            std::slice::from_ref(&fake08),
        );
        let clock = report("@test:clock", "independent-clock");
        let runtime = runtime(&[fake08.clone(), pack.clone(), clock.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        install(&host, pack.clone());
        install(&host, clock.clone());
        runtime.fail_start.replace(Some(fake08.id.clone()));
        assert!(host.set_enabled(&pack.id, true).is_err());
        assert_eq!(
            host.status(&pack.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
        assert_eq!(
            host.status(&fake08.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
        runtime.fail_start.replace(None);
        host.set_enabled(&pack.id, true).unwrap();
        host.set_enabled(&clock.id, true).unwrap();
        cold_boot(&runtime);
        runtime.events.borrow_mut().clear();
        host.restore_all().unwrap();
        let starts: Vec<_> = runtime
            .events
            .borrow()
            .iter()
            .filter(|e| e.starts_with("start:"))
            .cloned()
            .collect();
        assert!(
            starts
                .iter()
                .position(|e| e == "start:@runtime:fake08")
                .unwrap()
                < starts
                    .iter()
                    .position(|e| e == "start:@games:starter-pack")
                    .unwrap()
        );
        runtime.fail_start.replace(Some(fake08.id.clone()));
        cold_boot(&runtime);
        assert!(host.restore_all().is_err());
        assert_eq!(published(&host).1, ["@test:clock"]);
        assert_eq!(
            host.status(&pack.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
        assert!(!runtime.running.borrow().contains(&pack.id));
    }

    #[test]
    fn corrupt_dependency_approval_is_never_reused_for_enable_or_boot_restore() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let fake08 = report("@runtime:fake08", "corrupt-fake08");
        let pack = pack(
            "@games:starter-pack",
            "corrupt-pack",
            std::slice::from_ref(&fake08),
        );
        let runtime = runtime(&[fake08.clone(), pack.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        install(&host, pack.clone());
        host.set_enabled(&pack.id, true).unwrap();
        let mut corrupt = host.status(&fake08.id).unwrap().unwrap();
        corrupt.approval = "not-the-approved-bytes".into();
        storage::write_json(&host.receipt_path(&fake08.id), &corrupt).unwrap();
        assert!(host.enabled_packages().is_err());
        assert!(host.restore_all().is_err());
        assert_eq!(published(&host).1, Vec::<String>::new());
        assert_eq!(
            host.status(&pack.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
        assert!(!runtime.running.borrow().contains(&pack.id));
        assert!(host.set_enabled(&pack.id, true).is_err());
    }

    #[test]
    fn failed_enabled_leaf_update_still_restores_the_enabled_committed_selection() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let first = report("@test:clock", "leaf-before");
        let second = report("@test:clock", "leaf-fails");
        let runtime = runtime(&[first.clone(), second.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        install(&host, first.clone());
        host.set_enabled(&first.id, true).unwrap();
        runtime
            .fail_start_package
            .replace(Some(second.package.clone()));
        let error = host
            .install(
                second.clone(),
                &second.approval,
                SelectionIntent::Update { id: &first.id },
            )
            .unwrap_err();
        assert!(error.contains("committed selection restored"), "{error}");
        let selected = host.status(&first.id).unwrap().unwrap();
        assert_eq!(selected.package, first.package);
        assert_eq!(selected.desired, Desired::Enabled);
        assert!(selected.previous.is_none());
        assert!(runtime.running.borrow().contains(&first.id));
        assert!(fs::symlink_metadata(host.root(&first.id, "pending")).is_err());
    }

    #[test]
    fn disabled_installed_pack_preserves_its_exact_dependency_selection_for_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let first = report("@runtime:fake08", "disabled-first");
        let second = report("@runtime:fake08", "disabled-second");
        let pack = pack(
            "@games:starter-pack",
            "disabled-pack",
            std::slice::from_ref(&first),
        );
        let runtime = runtime(&[first.clone(), second.clone(), pack.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime).unwrap();
        install(&host, pack.clone());
        assert!(host
            .remove(&first.id, false)
            .unwrap_err()
            .contains("required by installed"));
        assert!(host
            .install(
                second.clone(),
                &second.approval,
                SelectionIntent::Update { id: &first.id }
            )
            .unwrap_err()
            .contains("required by installed"));
        assert_eq!(
            host.status(&first.id).unwrap().unwrap().package,
            first.package
        );
        host.remove(&pack.id, false).unwrap();
        host.install(
            second.clone(),
            &second.approval,
            SelectionIntent::Update { id: &first.id },
        )
        .unwrap();
        assert_eq!(
            host.status(&first.id).unwrap().unwrap().package,
            second.package
        );
    }

    #[test]
    fn enabled_pack_update_and_rollback_activate_only_the_exact_approved_graph() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let a = report("@runtime:a", "update-a");
        let b = report("@runtime:b", "update-b");
        let first = pack(
            "@games:starter-pack",
            "pack-first",
            std::slice::from_ref(&a),
        );
        let second = pack(
            "@games:starter-pack",
            "pack-second",
            std::slice::from_ref(&b),
        );
        let runtime = runtime(&[a.clone(), b.clone(), first.clone(), second.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime).unwrap();
        install(&host, first.clone());
        host.set_enabled(&first.id, true).unwrap();
        host.install(
            second.clone(),
            &second.approval,
            SelectionIntent::Update { id: &first.id },
        )
        .unwrap();
        let selected = host.status(&first.id).unwrap().unwrap();
        assert_eq!(selected.package, second.package);
        assert_eq!(selected.previous.unwrap().package, first.package);
        assert_eq!(selected.desired, Desired::Enabled);
        assert_eq!(
            host.status(&a.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
        assert_eq!(
            host.status(&b.id).unwrap().unwrap().desired,
            Desired::Enabled
        );
        host.rollback(&first.id).unwrap();
        let selected = host.status(&first.id).unwrap().unwrap();
        assert_eq!(selected.package, first.package);
        assert_eq!(selected.previous.unwrap().package, second.package);
        assert_eq!(selected.desired, Desired::Enabled);
        assert_eq!(
            host.status(&a.id).unwrap().unwrap().desired,
            Desired::Enabled
        );
        assert_eq!(
            host.status(&b.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
    }

    #[test]
    fn root_graph_power_loss_never_leaves_mixed_exact_selections() {
        let old: Vec<_> = ["fake08", "solarus", "zquest"]
            .iter()
            .map(|name| report(&format!("@test:{name}"), &format!("{name}-old")))
            .collect();
        let new: Vec<_> = ["fake08", "solarus", "zquest"]
            .iter()
            .map(|name| report(&format!("@test:{name}"), &format!("{name}-new")))
            .collect();
        let old_pack = pack("@test:starter-pack", "starter-old", &old);
        let new_pack = pack("@test:starter-pack", "starter-new", &new);
        let reports: Vec<_> = old
            .iter()
            .chain(&new)
            .chain([&old_pack, &new_pack])
            .cloned()
            .collect();
        // Prepared, four stages, four stops, Switching, four commits, Finished.
        for interruption in 1..=15 {
            for cold in [false, true] {
                for all_enabled in [false, true] {
                    let root = tempfile::tempdir().unwrap();
                    let runtime = runtime(&reports);
                    let nix = cleanup_program(root.path(), "exit 0");
                    let host = Host::for_test(
                        root.path(),
                        nix.clone(),
                        permitted_policy(),
                        runtime.clone(),
                    )
                    .unwrap();
                    install(&host, old_pack.clone());
                    if all_enabled {
                        host.set_enabled(&old_pack.id, true).unwrap();
                    } else {
                        host.set_enabled(&old[0].id, true).unwrap();
                    }
                    let before = host.receipts().unwrap();
                    fs::write(root.path().join("save-data"), "retain progress").unwrap();
                    runtime.interrupt_graph_at.set(Some(interruption));
                    let error = host
                        .install(
                            new_pack.clone(),
                            &new_pack.approval,
                            SelectionIntent::Update { id: &old_pack.id },
                        )
                        .unwrap_err();
                    assert!(
                        error.contains("injected graph interruption"),
                        "{interruption}: {error}"
                    );
                    if interruption == 1 {
                        assert_eq!(
                            fs::read_link(host.root(&old_pack.id, "pending")).unwrap(),
                            new_pack.package
                        );
                    }
                    if interruption < 15 {
                        assert!(host
                            .enabled_packages()
                            .err()
                            .unwrap()
                            .contains("run restore-all"));
                        assert!(host
                            .set_enabled(&old[0].id, true)
                            .unwrap_err()
                            .contains("run restore-all"));
                        assert!(host
                            .rollback(&old[0].id)
                            .unwrap_err()
                            .contains("run restore-all"));
                    }
                    runtime.interrupt_graph_at.set(None);
                    if cold {
                        cold_boot(&runtime);
                    }
                    drop(host);
                    let host =
                        Host::for_test(root.path(), nix, permitted_policy(), runtime.clone())
                            .unwrap();
                    host.restore_all().unwrap();
                    host.restore_all().unwrap();
                    for original in before {
                        let selected = host.status(&original.id).unwrap().unwrap();
                        assert_eq!(
                            selected.desired, original.desired,
                            "interruption {interruption}"
                        );
                        if interruption < 10 {
                            assert_eq!(selected, original);
                        } else {
                            let target = reports
                                .iter()
                                .find(|r| r.id == original.id && r.package != original.package)
                                .unwrap();
                            assert_eq!(selected.package, target.package);
                            assert_eq!(selected.previous.unwrap().package, original.package);
                        }
                        assert_eq!(
                            fs::read_link(host.root(&original.id, "active")).unwrap(),
                            selected.package
                        );
                        assert!(fs::symlink_metadata(host.root(&original.id, "pending")).is_err());
                        assert_eq!(
                            runtime.running.borrow().contains(&original.id),
                            matches!(original.desired, Desired::Enabled)
                        );
                    }
                    assert_eq!(
                        fs::read_to_string(root.path().join("save-data")).unwrap(),
                        "retain progress"
                    );
                    assert!(
                        runtime.unsafe_stops.borrow().is_empty(),
                        "{:?}",
                        runtime.unsafe_stops.borrow()
                    );
                    assert!(!host.state.root().join("graph-update.json").exists());
                }
            }
        }
    }

    #[test]
    fn root_graph_recovery_keeps_required_targets_rooted_until_journal_retirement() {
        let old = report("@test:runner", "gc-old");
        let new = report("@test:runner", "gc-new");
        let first = pack("@test:pack", "gc-pack-old", std::slice::from_ref(&old));
        let next = pack("@test:pack", "gc-pack-new", std::slice::from_ref(&new));
        for (initial, boundary) in [(3, 4), (3, 3), (6, 2)] {
            let directory = tempfile::tempdir().unwrap();
            let runtime = runtime(&[old.clone(), new.clone(), first.clone(), next.clone()]);
            let nix = cleanup_program(directory.path(), "exit 0");
            let host = Host::for_test(
                directory.path(),
                nix.clone(),
                permitted_policy(),
                runtime.clone(),
            )
            .unwrap();
            install(&host, first.clone());
            host.set_enabled(&first.id, true).unwrap();
            let before = host.receipts().unwrap();
            runtime.interrupt_graph_at.set(Some(initial));
            assert!(host
                .install(
                    next.clone(),
                    &next.approval,
                    SelectionIntent::Update { id: &first.id }
                )
                .unwrap_err()
                .contains("injected graph interruption"));
            runtime.graph_checkpoints.set(0);
            runtime.interrupt_graph_at.set(Some(boundary));
            assert!(host
                .restore_all()
                .unwrap_err()
                .contains("injected graph interruption"));
            runtime.interrupt_graph_at.set(None);
            assert_eq!(
                host.state.root().join("graph-update.json").exists(),
                initial == 6
            );
            drop(host);
            cold_boot(&runtime);
            let host =
                Host::for_test(directory.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            // Simulate GC, including references reachable through a rooted
            // package. Remove collected target metadata so loading it fails.
            let mut live = BTreeSet::new();
            let mut pending = Vec::new();
            for node in fs::read_dir(&host.paths.roots).unwrap() {
                for link in fs::read_dir(node.unwrap().path()).unwrap() {
                    pending.push(fs::read_link(link.unwrap().path()).unwrap());
                }
            }
            while let Some(path) = pending.pop() {
                if live.insert(path.clone()) {
                    if let Some(report) = runtime.reports.borrow().get(&path) {
                        pending.extend(report.requires.clone());
                    }
                }
            }
            for target in [&new.package, &next.package] {
                if !live.contains(target) {
                    runtime.reports.borrow_mut().remove(target);
                }
            }
            let collected = !runtime.reports.borrow().contains_key(&next.package);
            // Resume first: the old ordering fails here after GC because it
            // retains a journal whose discarded target must be loaded again.
            host.restore_all().unwrap();
            host.restore_all().unwrap();
            if initial == 3 {
                assert_eq!(collected, boundary == 4);
                assert_eq!(host.receipts().unwrap(), before);
            } else {
                assert!(!collected, "Switching target must stay active-rooted");
                assert_eq!(
                    host.receipt(&first.id).unwrap().unwrap().package,
                    next.package
                );
                assert_eq!(host.receipt(&old.id).unwrap().unwrap().package, new.package);
            }
            assert!(!host.state.root().join("graph-update.json").exists());
            assert!(runtime.unsafe_stops.borrow().is_empty());
        }
    }

    #[test]
    fn root_graph_recovers_additions_and_a_receipt_written_before_its_roots() {
        let old = report("@test:runner", "roots-old");
        let new = report("@test:runner", "roots-new");
        let added = report("@test:added", "roots-added");
        let first = pack("@test:pack", "roots-pack-old", std::slice::from_ref(&old));
        let second = pack(
            "@test:pack",
            "roots-pack-new",
            &[new.clone(), added.clone()],
        );
        for interruption in [4, 7, 8] {
            let root = tempfile::tempdir().unwrap();
            let runtime = runtime(&[
                old.clone(),
                new.clone(),
                added.clone(),
                first.clone(),
                second.clone(),
            ]);
            let nix = cleanup_program(root.path(), "exit 0");
            let host = Host::for_test(
                root.path(),
                nix.clone(),
                permitted_policy(),
                runtime.clone(),
            )
            .unwrap();
            install(&host, first.clone());
            host.set_enabled(&first.id, true).unwrap();
            runtime.interrupt_graph_at.set(Some(interruption));
            let error = host
                .install(
                    second.clone(),
                    &second.approval,
                    SelectionIntent::Update { id: &first.id },
                )
                .unwrap_err();
            assert!(error.contains("injected graph interruption"), "{error}");
            if interruption == 7 {
                // Switching is durable, but no receipt/root pair has committed.
                // Simulate loss of power between the receipt rename and settle.
                let before = host.receipt(&old.id).unwrap().unwrap();
                let after = before
                    .select(
                        new.package.clone(),
                        new.provenance.clone(),
                        new.approval.clone(),
                    )
                    .unwrap();
                storage::write_json(&host.receipt_path(&old.id), &after).unwrap();
                assert_eq!(
                    fs::read_link(host.root(&old.id, "active")).unwrap(),
                    old.package
                );
                assert_eq!(
                    fs::read_link(host.root(&old.id, "pending")).unwrap(),
                    new.package
                );
            }
            runtime.interrupt_graph_at.set(None);
            cold_boot(&runtime);
            drop(host);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            host.restore_all().unwrap();
            let committed = interruption >= 7;
            assert_eq!(
                host.status(&first.id).unwrap().unwrap().package,
                if committed {
                    second.package.clone()
                } else {
                    first.package.clone()
                }
            );
            assert_eq!(
                host.status(&old.id).unwrap().unwrap().package,
                if committed {
                    new.package.clone()
                } else {
                    old.package.clone()
                }
            );
            let addition = host.status(&added.id).unwrap();
            assert_eq!(addition.is_some(), committed);
            if let Some(receipt) = addition {
                assert!(receipt.previous.is_none());
                assert_eq!(receipt.desired, Desired::Enabled);
            }
            assert!(runtime.unsafe_stops.borrow().is_empty());
            assert!(!host.state.root().join("graph-update.json").exists());
        }
    }

    #[test]
    fn root_graph_refuses_to_overwrite_an_additions_unfinished_selection() {
        let root = tempfile::tempdir().unwrap();
        let old = report("@test:runner", "pending-old");
        let new = report("@test:runner", "pending-new");
        let added = report("@test:added", "pending-addition");
        let first = pack("@test:pack", "pending-pack-old", std::slice::from_ref(&old));
        let second = pack(
            "@test:pack",
            "pending-pack-new",
            &[new.clone(), added.clone()],
        );
        let runtime = runtime(&[
            old.clone(),
            new.clone(),
            added.clone(),
            first.clone(),
            second.clone(),
        ]);
        let host = Host::for_test(
            root.path(),
            cleanup_program(root.path(), "exit 0"),
            permitted_policy(),
            runtime.clone(),
        )
        .unwrap();
        install(&host, first.clone());
        host.prepare(&added.id).unwrap();
        host.selection(&added.id).stage(&added.package).unwrap();
        let before = host.receipts().unwrap();
        runtime.events.borrow_mut().clear();
        let error = host
            .install(
                second.clone(),
                &second.approval,
                SelectionIntent::Update { id: &first.id },
            )
            .unwrap_err();
        assert!(error.contains("unrecorded pending selection"), "{error}");
        assert_eq!(host.receipts().unwrap(), before);
        assert_eq!(
            fs::read_link(host.root(&added.id, "pending")).unwrap(),
            added.package
        );
        assert!(stops(&runtime).is_empty());
        assert!(!host.state.root().join("graph-update.json").exists());
    }

    #[test]
    fn root_graph_finishes_committed_selections_before_refusing_revoked_activation() {
        let root = tempfile::tempdir().unwrap();
        let old = report("@test:runner", "revoked-old");
        let new = report("@test:runner", "revoked-new");
        let first = pack("@test:pack", "revoked-pack-old", std::slice::from_ref(&old));
        let second = pack("@test:pack", "revoked-pack-new", std::slice::from_ref(&new));
        let runtime = runtime(&[old.clone(), new.clone(), first.clone(), second.clone()]);
        let host = Host::for_test(
            root.path(),
            cleanup_program(root.path(), "exit 0"),
            permitted_policy(),
            runtime.clone(),
        )
        .unwrap();
        install(&host, first.clone());
        host.set_enabled(&first.id, true).unwrap();
        runtime.interrupt_graph_at.set(Some(6));
        assert!(host
            .install(
                second.clone(),
                &second.approval,
                SelectionIntent::Update { id: &first.id }
            )
            .is_err());
        runtime.interrupt_graph_at.set(None);
        cold_boot(&runtime);
        runtime.events.borrow_mut().clear();
        runtime.revoked.borrow_mut().insert(new.package.clone());
        let error = host.restore_all().unwrap_err();
        assert!(error.contains("revocation"), "{error}");
        assert_eq!(host.status(&new.id).unwrap().unwrap().package, new.package);
        let parent = host.status(&second.id).unwrap().unwrap();
        assert_eq!(parent.package, second.package);
        assert_eq!(parent.desired, Desired::Disabled);
        assert!(!host.state.root().join("graph-update.json").exists());
        assert!(!runtime
            .events
            .borrow()
            .iter()
            .any(|e| e == &format!("start:{}", new.id)));
    }

    #[test]
    fn root_update_replaces_exact_dependencies_with_independent_previous_selections() {
        for enabled in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nix = cleanup_program(root.path(), "exit 0");
            let a = report("@runtime:a", "graph-a-before");
            let b = report("@runtime:b", "graph-b-before");
            let next_a = report("@runtime:a", "graph-a-after");
            let next_b = report("@runtime:b", "graph-b-after");
            let first = pack("@games:pack", "graph-before", &[a.clone(), b.clone()]);
            let second = pack(
                "@games:pack",
                "graph-after",
                &[next_a.clone(), next_b.clone()],
            );
            let unrelated = report("@runtime:unrelated", "graph-unrelated");
            let runtime = runtime(&[
                a.clone(),
                b.clone(),
                next_a.clone(),
                next_b.clone(),
                first.clone(),
                second.clone(),
                unrelated.clone(),
            ]);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            install(&host, first.clone());
            install(&host, unrelated.clone());
            host.set_enabled(&unrelated.id, true).unwrap();
            if enabled {
                host.set_enabled(&first.id, true).unwrap();
            }
            let untouched = host.status(&unrelated.id).unwrap();
            runtime.events.borrow_mut().clear();
            host.install(
                second.clone(),
                &second.approval,
                SelectionIntent::Update { id: &first.id },
            )
            .unwrap();
            for (old, new) in [(&first, &second), (&a, &next_a), (&b, &next_b)] {
                let selected = host.status(&new.id).unwrap().unwrap();
                assert_eq!(selected.package, new.package);
                assert_eq!(selected.approval, new.approval);
                assert_eq!(
                    selected.desired,
                    if enabled {
                        Desired::Enabled
                    } else {
                        Desired::Disabled
                    }
                );
                let previous = selected.previous.unwrap();
                assert_eq!(previous.package, old.package);
                assert_eq!(previous.approval, old.approval);
                assert_eq!(previous.provenance, old.provenance);
                assert_eq!(
                    fs::read_link(host.root(&new.id, "previous")).unwrap(),
                    old.package
                );
            }
            assert_eq!(host.status(&unrelated.id).unwrap(), untouched);
            assert!(!runtime
                .events
                .borrow()
                .iter()
                .any(|e| e == "stop:@runtime:unrelated:false" || e == "start:@runtime:unrelated"));
            assert!(runtime.unsafe_stops.borrow().is_empty());
            if enabled {
                let events = runtime.events.borrow();
                let at = |event: &str| events.iter().position(|e| e == event).unwrap();
                assert!(at("stop:@games:pack:false") < at("stop:@runtime:a:false"));
                assert!(at("stop:@games:pack:false") < at("stop:@runtime:b:false"));
                assert!(at("start:@runtime:a") < at("start:@games:pack"));
                assert!(at("start:@runtime:b") < at("start:@games:pack"));
            }
        }
    }

    #[test]
    fn failed_graph_update_keeps_old_builds_pinned_and_restorable_by_reviewed_update() {
        for fail_root in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nix = cleanup_program(root.path(), "exit 0");
            let a = report("@runtime:a", "failed-graph-a-before");
            let b = report("@runtime:b", "failed-graph-b-before");
            let next_a = report("@runtime:a", "failed-graph-a-after");
            let next_b = report("@runtime:b", "failed-graph-b-after");
            let first = pack(
                "@games:pack",
                "failed-graph-before",
                &[a.clone(), b.clone()],
            );
            let second = pack(
                "@games:pack",
                "failed-graph-after",
                &[next_a.clone(), next_b.clone()],
            );
            let runtime = runtime(&[
                a.clone(),
                b.clone(),
                next_a.clone(),
                next_b.clone(),
                first.clone(),
                second.clone(),
            ]);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            install(&host, first.clone());
            host.set_enabled(&first.id, true).unwrap();
            runtime.fail_start_package.replace(Some(if fail_root {
                second.package.clone()
            } else {
                next_b.package.clone()
            }));
            let error = host
                .install(
                    second.clone(),
                    &second.approval,
                    SelectionIntent::Update { id: &first.id },
                )
                .unwrap_err();
            assert!(error.contains("start failure"), "{error}");
            assert!(
                error.contains(&format!("{} left Disabled", first.id)),
                "{error}"
            );
            assert_eq!(
                host.status(&first.id).unwrap().unwrap().desired,
                Desired::Disabled
            );
            assert!(!runtime.running.borrow().contains(&first.id));
            for old in [&first, &a, &b] {
                let receipt = host.status(&old.id).unwrap().unwrap();
                let slot = if receipt.package == old.package {
                    "active"
                } else {
                    assert_eq!(receipt.previous.unwrap().package, old.package);
                    "previous"
                };
                assert_eq!(
                    fs::read_link(host.root(&old.id, slot)).unwrap(),
                    old.package
                );
            }
            assert!(runtime.unsafe_stops.borrow().is_empty());
            runtime.fail_start_package.replace(None);
            host.restore_all().unwrap();
            assert_eq!(
                host.status(&first.id).unwrap().unwrap().desired,
                Desired::Disabled
            );
            assert!(!runtime.running.borrow().contains(&first.id));
            // No automatic graph rollback: explicitly review/reselect the old graph.
            host.install(
                first.clone(),
                &first.approval,
                SelectionIntent::Update { id: &first.id },
            )
            .unwrap();
            host.set_enabled(&first.id, true).unwrap();
            for old in [&first, &a, &b] {
                let receipt = host.status(&old.id).unwrap().unwrap();
                assert_eq!(receipt.package, old.package);
                assert_eq!(receipt.desired, Desired::Enabled);
                assert!(runtime.running.borrow().contains(&old.id));
            }
        }
    }

    #[test]
    fn graph_update_keeps_shared_dependency_direct_update_and_install_refusals() {
        for enabled in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nix = cleanup_program(root.path(), "exit 0");
            let a = report("@runtime:a", "shared-graph-before");
            let next_a = report("@runtime:a", "shared-graph-after");
            let first = pack(
                "@games:pack",
                "shared-pack-before",
                std::slice::from_ref(&a),
            );
            let second = pack(
                "@games:pack",
                "shared-pack-after",
                std::slice::from_ref(&next_a),
            );
            let other = pack("@games:other", "shared-other", std::slice::from_ref(&a));
            let fresh = pack(
                "@games:fresh",
                "shared-fresh",
                std::slice::from_ref(&next_a),
            );
            let runtime = runtime(&[
                a.clone(),
                next_a.clone(),
                first.clone(),
                second.clone(),
                other.clone(),
                fresh.clone(),
            ]);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            install(&host, first.clone());
            install(&host, other.clone());
            if enabled {
                host.set_enabled(&first.id, true).unwrap();
                host.set_enabled(&other.id, true).unwrap();
            }
            let before = host.receipts().unwrap();
            runtime.events.borrow_mut().clear();
            let error = host
                .install(
                    second.clone(),
                    &second.approval,
                    SelectionIntent::Update { id: &first.id },
                )
                .unwrap_err();
            assert!(
                error.contains("required by") && error.contains(&other.id),
                "{error}"
            );
            assert!(host
                .install(
                    next_a.clone(),
                    &next_a.approval,
                    SelectionIntent::Update { id: &a.id }
                )
                .unwrap_err()
                .contains("required by"));
            assert!(host
                .install(fresh.clone(), &fresh.approval, SelectionIntent::Install)
                .unwrap_err()
                .contains("conflicting exact selection"));
            assert_eq!(host.receipts().unwrap(), before);
            assert!(stops(&runtime).is_empty());
        }
    }

    #[test]
    fn transitive_graph_update_finishes_cleanup_before_replacing_required_nodes() {
        for fail_cleanup in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nix = cleanup_program(root.path(), "exit 0");
            let a = report("@runtime:a", "transitive-update-a-before");
            let next_a = report("@runtime:a", "transitive-update-a-after");
            let b = pack(
                "@runtime:b",
                "transitive-update-b-before",
                std::slice::from_ref(&a),
            );
            let next_b = pack(
                "@runtime:b",
                "transitive-update-b-after",
                std::slice::from_ref(&next_a),
            );
            let first = pack(
                "@games:pack",
                "transitive-update-before",
                std::slice::from_ref(&b),
            );
            let second = pack(
                "@games:pack",
                "transitive-update-after",
                std::slice::from_ref(&next_b),
            );
            let runtime = runtime(&[
                a.clone(),
                b.clone(),
                next_a.clone(),
                next_b.clone(),
                first.clone(),
                second.clone(),
            ]);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            install(&host, first.clone());
            host.set_enabled(&first.id, true).unwrap();
            if fail_cleanup {
                runtime.fail_stop.replace(Some(b.id.clone()));
            }
            runtime.events.borrow_mut().clear();
            let result = host.install(
                second.clone(),
                &second.approval,
                SelectionIntent::Update { id: &first.id },
            );
            if fail_cleanup {
                let error = result.unwrap_err();
                assert!(error.contains("stop failure"), "{error}");
                for old in [&first, &a, &b] {
                    let receipt = host.receipt(&old.id).unwrap().unwrap();
                    assert_eq!(receipt.package, old.package);
                    assert!(receipt.previous.is_none());
                }
                assert!(runtime.running.borrow().contains(&a.id));
                assert!(!stops(&runtime).iter().any(|e| e.contains(&a.id)));
                runtime.fail_stop.replace(None);
                host.restore_all().unwrap();
                host.install(
                    second.clone(),
                    &second.approval,
                    SelectionIntent::Update { id: &first.id },
                )
                .unwrap();
                host.set_enabled(&second.id, true).unwrap();
            } else {
                result.unwrap();
            }
            for (old, new) in [(&first, &second), (&a, &next_a), (&b, &next_b)] {
                let receipt = host.status(&new.id).unwrap().unwrap();
                assert_eq!(receipt.package, new.package);
                assert_eq!(receipt.previous.unwrap().package, old.package);
                assert_eq!(receipt.desired, Desired::Enabled);
            }
            assert!(runtime.unsafe_stops.borrow().is_empty());
        }
    }

    #[test]
    fn disabled_root_update_preserves_an_independently_enabled_dependency() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let a = report("@runtime:a", "independent-a-before");
        let next_a = report("@runtime:a", "independent-a-after");
        let b = report("@runtime:b", "independent-b-unchanged");
        let first = pack("@games:pack", "independent-before", &[a.clone(), b.clone()]);
        let second = pack(
            "@games:pack",
            "independent-after",
            &[next_a.clone(), b.clone()],
        );
        let runtime = runtime(&[
            a.clone(),
            b.clone(),
            next_a.clone(),
            first.clone(),
            second.clone(),
        ]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        install(&host, first.clone());
        host.set_enabled(&a.id, true).unwrap();
        host.set_enabled(&b.id, true).unwrap();
        let unchanged = host.status(&b.id).unwrap();
        host.install(
            second.clone(),
            &second.approval,
            SelectionIntent::Update { id: &first.id },
        )
        .unwrap();
        assert_eq!(
            host.status(&second.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
        assert_eq!(
            host.status(&a.id).unwrap().unwrap().desired,
            Desired::Enabled
        );
        assert!(runtime.running.borrow().contains(&a.id));
        assert_eq!(host.status(&b.id).unwrap(), unchanged);
        assert!(runtime.running.borrow().contains(&b.id));
        assert!(!runtime.running.borrow().contains(&first.id));
    }

    #[test]
    #[ignore = "requires builder dependency fixtures and real build-machine Nix; native effects are controlled"]
    fn real_store_graph_repeated_failed_update_keeps_committed_dependency_provenance() {
        let fixtures = PathBuf::from(
            std::env::var_os("KORRI_TEST_DEPENDENCY_FIXTURES")
                .expect("built builder-check fixtures"),
        );
        let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
        let fixture = |name| fs::canonicalize(fixtures.join(name)).unwrap();
        let root = tempfile::tempdir().unwrap();
        let runtime = TestRuntime {
            real_packages: true,
            ..TestRuntime::default()
        };
        let mut host =
            Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        host.publishers = ["@games", "@runtime"]
            .into_iter()
            .map(|namespace| {
                (
                    namespace.into(),
                    package::PublisherBinding {
                        public_key: "seed-fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
                            .into(),
                        cache_url: format!("https://cache.example/{namespace}"),
                    },
                )
            })
            .collect();
        let root_origin = Provenance::RawCache {
            cache_url: host.publishers["@games"].cache_url.clone(),
        };
        let dependency_origin = Provenance::Repository {
            signed: true,
            source_url: "https://repository.example/catalog.json".into(),
            plugin_id: "@runtime:fake08".into(),
            release_version: "0".into(),
            platform: current_platform().into(),
            archive_sha256: "a".repeat(64),
        };
        let dependency0 = host
            .load(&fixture("runner"), dependency_origin.clone())
            .unwrap();
        install(&host, dependency0.clone());
        let root0 = host.load(&fixture("pack"), root_origin.clone()).unwrap();
        assert_eq!(root0.brings[0].report.provenance, dependency_origin);
        install(&host, root0.clone());
        host.set_enabled(&root0.id, true).unwrap();
        let mut legacy_committed = host.status(&root0.id).unwrap().unwrap();
        legacy_committed.desired = Desired::Disabled;
        let root1 = host
            .load(&fixture("secondPack"), root_origin.clone())
            .unwrap();
        let dependency1 = &root1.brings[0].report;
        runtime
            .fail_start_package
            .replace(Some(dependency1.package.clone()));
        let error = host
            .install(
                root1.clone(),
                &root1.approval,
                SelectionIntent::Update { id: &root0.id },
            )
            .unwrap_err();
        assert!(error.contains("injected start failure"), "{error}");
        assert_eq!(
            host.status(&root0.id).unwrap().unwrap().package,
            root1.package
        );
        // Reproduce #202's pre-transaction failed-update state explicitly. The
        // new decision prevents creating this state, but must still refuse
        // overwriting provenance a committed root needs when it is observed.
        host.selection(&root0.id).commit(&legacy_committed).unwrap();
        let committed = host.status(&root0.id).unwrap().unwrap();
        assert_eq!(committed.package, root0.package);
        assert_eq!(committed.desired, Desired::Disabled);
        assert_eq!(host.approved(&committed).unwrap().approval, root0.approval);
        let selected = host.status(&dependency0.id).unwrap().unwrap();
        assert_eq!(selected.package, dependency1.package);
        let previous = selected.previous.unwrap();
        assert_eq!(previous.package, dependency0.package);
        assert_eq!(previous.provenance, dependency_origin);

        runtime.fail_start_package.replace(None);
        let root2 = host
            .load(&fixture("thirdPack"), root_origin.clone())
            .unwrap();
        let before = host.receipts().unwrap();
        let roots: Vec<_> = before
            .iter()
            .map(|receipt| {
                ["active", "previous", "pending"]
                    .map(|name| fs::read_link(host.root(&receipt.id, name)).ok())
            })
            .collect();
        runtime.events.borrow_mut().clear();
        let error = host
            .install(
                root2.clone(),
                &root2.approval,
                SelectionIntent::Update { id: &root0.id },
            )
            .unwrap_err();
        assert!(
            error.contains("restore the committed root's exact dependency graph"),
            "{error}"
        );
        assert_eq!(host.receipts().unwrap(), before);
        for (receipt, roots) in before.iter().zip(roots) {
            for (name, path) in ["active", "previous", "pending"].into_iter().zip(roots) {
                assert_eq!(fs::read_link(host.root(&receipt.id, name)).ok(), path);
            }
        }
        assert!(stops(&runtime).is_empty());
        assert!(!runtime
            .events
            .borrow()
            .iter()
            .any(|event| event.starts_with("start:")));
        assert_eq!(host.approved(&committed).unwrap().approval, root0.approval);
        host.restore_all().unwrap();
        assert_eq!(host.receipts().unwrap(), before);

        // An explicitly reviewed restoration must remain possible. Once the
        // committed graph is exact again, another new build may replace it.
        let restored = host.load(&root0.package, root_origin.clone()).unwrap();
        assert_eq!(restored.approval, root0.approval);
        host.install(
            restored.clone(),
            &restored.approval,
            SelectionIntent::Update { id: &root0.id },
        )
        .unwrap();
        host.set_enabled(&root0.id, true).unwrap();
        let next = host.load(&root2.package, root_origin).unwrap();
        host.install(
            next.clone(),
            &next.approval,
            SelectionIntent::Update { id: &root0.id },
        )
        .unwrap();
        let selected = host.status(&dependency0.id).unwrap().unwrap();
        assert_eq!(selected.package, next.brings[0].report.package);
        assert_eq!(selected.previous.unwrap().provenance, dependency_origin);
        assert_eq!(
            host.status(&root0.id).unwrap().unwrap().package,
            root2.package
        );
        assert!(runtime.unsafe_stops.borrow().is_empty());
    }

    #[test]
    fn displaced_dependency_keeps_its_exact_repository_provenance_for_root_cleanup() {
        let old = repository_report(
            "@runtime:a",
            "old-repository-dependency",
            "https://repository.example/catalog.json",
            "1",
        );
        let new = report("@runtime:a", "new-raw-dependency");
        let receipt = Receipt {
            id: old.id.clone(),
            package: old.package.clone(),
            provenance: old.provenance.clone(),
            approval: old.approval,
            desired: Desired::Disabled,
            previous: None,
        }
        .select(new.package.clone(), new.provenance.clone(), new.approval)
        .unwrap();
        let receipts = [receipt];
        assert_eq!(
            dependency_provenance(&old.package, &receipts, &BTreeMap::new()).unwrap(),
            old.provenance
        );
        assert_eq!(
            dependency_provenance(&new.package, &receipts, &BTreeMap::new()).unwrap(),
            new.provenance
        );
    }

    #[test]
    fn release_approval_covers_new_independent_dependencies_without_a_second_schema() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let fake08 = report("@runtime:fake08", "release-fake08");
        let pack = pack(
            "@games:starter-pack",
            "release-pack",
            std::slice::from_ref(&fake08),
        );
        let runtime = runtime(&[fake08.clone(), pack.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime).unwrap();
        let approvals = BTreeMap::from([(pack.id.clone(), pack.approval.clone())]);
        let review = host
            .review_release_update(vec![pack.clone()], Vec::new())
            .unwrap();
        assert!(host
            .apply_release_update(review, &approvals, || Err("selection refused".into()))
            .is_err());
        assert!(host.status(&pack.id).unwrap().is_none());
        assert!(host.status(&fake08.id).unwrap().is_none());
        let review = host
            .review_release_update(vec![pack.clone()], Vec::new())
            .unwrap();
        host.apply_release_update(review, &approvals, || Ok(()))
            .unwrap();
        assert_eq!(
            host.status(&pack.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
        assert_eq!(
            host.status(&fake08.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
    }

    #[test]
    fn dependency_install_failure_retains_an_unfinished_cleanup_receipt() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 42");
        let fake08 = report("@runtime:fake08", "retained-fake08");
        let pack = pack(
            "@games:starter-pack",
            "retained-pack",
            std::slice::from_ref(&fake08),
        );
        let runtime = runtime(&[fake08.clone(), pack.clone()]);
        runtime.fail_stop.replace(Some(pack.id.clone()));
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        let error = host
            .install(pack.clone(), &pack.approval, SelectionIntent::Install)
            .unwrap_err();
        assert!(error.contains("dependency cleanup failed"), "{error}");
        let retained = host.status(&fake08.id).unwrap().unwrap();
        assert_eq!(retained.desired, Desired::Removed { purge: false });
        assert!(host.status(&pack.id).unwrap().is_none());
        runtime.fail_stop.replace(None);
        cleanup_program(root.path(), "exit 0");
        host.restore_all().unwrap();
        assert!(host.status(&fake08.id).unwrap().is_none());
    }

    #[test]
    #[ignore = "requires builder dependency fixtures and real build-machine Nix; native effects are controlled"]
    fn real_store_graph_recovery_refuses_journal_scope_changes_before_effects() {
        let fixtures = PathBuf::from(
            std::env::var_os("KORRI_TEST_DEPENDENCY_FIXTURES").expect("built dependency fixtures"),
        );
        let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
        let fixture = |name| fs::canonicalize(fixtures.join(name)).unwrap();
        for checkpoint in [3, 6] {
            // Fully staged Prepared, then Switching before any commit.
            for change in [
                "root-omitted",
                "root-anchor-omitted",
                "root-swapped",
                "root-approval-corrupted",
                "dependency-omitted",
                "unrelated-injected",
                "addition-injected",
                "dependency-altered",
                "desired-altered",
                "shared-root-added",
                "unrelated-pending",
                "unrelated-graph-altered",
            ] {
                let directory = tempfile::tempdir().unwrap();
                let runtime = TestRuntime {
                    real_packages: true,
                    ..TestRuntime::default()
                };
                let mut host = Host::for_test(
                    directory.path(),
                    nix.clone(),
                    permitted_policy(),
                    runtime.clone(),
                )
                .unwrap();
                host.publishers = ["@games", "@runtime"]
                    .into_iter()
                    .map(|namespace| {
                        (
                            namespace.into(),
                            package::PublisherBinding {
                                public_key:
                                    "seed-fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
                                        .into(),
                                cache_url: format!("https://cache.example/{namespace}"),
                            },
                        )
                    })
                    .collect();
                let origin = |namespace: &str| Provenance::RawCache {
                    cache_url: format!("https://cache.example/{namespace}"),
                };
                let first = host.load(&fixture("pack"), origin("@games")).unwrap();
                let unrelated = host
                    .load(&fixture("unrelated"), origin("@runtime"))
                    .unwrap();
                install(&host, first.clone());
                install(&host, unrelated.clone());
                host.set_enabled(&first.id, true).unwrap();
                host.set_enabled(&unrelated.id, true).unwrap();
                let next = host.load(&fixture("secondPack"), origin("@games")).unwrap();
                runtime.interrupt_graph_at.set(Some(checkpoint));
                let error = host
                    .install(
                        next.clone(),
                        &next.approval,
                        SelectionIntent::Update { id: &first.id },
                    )
                    .unwrap_err();
                assert!(error.contains("injected graph interruption"), "{error}");
                runtime.interrupt_graph_at.set(None);
                let journal = host.state.root().join("graph-update.json");
                let mut value: serde_json::Value = storage::read_json(&journal).unwrap().unwrap();
                let selections = value["selections"].as_array_mut().unwrap();
                let expected = match change {
                    "root-omitted" => {
                        selections.retain(|entry| entry["after"]["id"] != first.id);
                        "root"
                    }
                    "root-anchor-omitted" => "root",
                    "root-swapped" => "closure",
                    "root-approval-corrupted" => {
                        let entry = selections
                            .iter_mut()
                            .find(|entry| entry["after"]["id"] == first.id)
                            .unwrap();
                        entry["after"]["approval"] = serde_json::json!("corrupted approval");
                        super::APPROVAL_STALE
                    }
                    "dependency-omitted" => {
                        selections.retain(|entry| entry["after"]["id"] == first.id);
                        // Membership is checked before native effects. The
                        // checks below still require identical journal bytes,
                        // receipts, roots and live ownership on this refusal.
                        "pending graph update selections differ from the approved root closure"
                    }
                    "unrelated-injected" => {
                        let target = host
                            .load(&fixture("nextUnrelated"), origin("@runtime"))
                            .unwrap();
                        let before = host.receipt(&unrelated.id).unwrap().unwrap();
                        let after = before
                            .select(target.package, target.provenance, target.approval)
                            .unwrap();
                        selections.push(serde_json::json!({"before": before, "after": after}));
                        "closure"
                    }
                    "addition-injected" => {
                        let target = host.load(&fixture("native"), origin("@runtime")).unwrap();
                        let after = Receipt {
                            id: target.id,
                            package: target.package,
                            provenance: target.provenance,
                            approval: target.approval,
                            desired: Desired::Enabled,
                            previous: None,
                        };
                        host.prepare(&after.id).unwrap();
                        host.selection(&after.id).stage(&after.package).unwrap();
                        selections.push(serde_json::json!({"before": null, "after": after}));
                        "closure"
                    }
                    "dependency-altered" => {
                        let target = host
                            .load(&fixture("thirdRunner"), origin("@runtime"))
                            .unwrap();
                        let before = host.receipt(&target.id).unwrap().unwrap();
                        let after = before
                            .select(target.package, target.provenance, target.approval)
                            .unwrap();
                        let entry = selections
                            .iter_mut()
                            .find(|entry| entry["after"]["id"] == after.id)
                            .unwrap();
                        *entry = serde_json::json!({"before": before, "after": after});
                        "dependency"
                    }
                    "desired-altered" => {
                        let entry = selections
                            .iter_mut()
                            .find(|entry| entry["after"]["id"] != first.id)
                            .unwrap();
                        entry["after"]["desired"] =
                            serde_json::to_value(Desired::Disabled).unwrap();
                        "desired"
                    }
                    "shared-root-added" => {
                        let target = host.load(&fixture("transitive"), origin("@games")).unwrap();
                        let receipt = Receipt {
                            id: target.id,
                            package: target.package,
                            provenance: target.provenance,
                            approval: target.approval,
                            desired: Desired::Disabled,
                            previous: None,
                        };
                        host.prepare(&receipt.id).unwrap();
                        host.selection(&receipt.id).commit(&receipt).unwrap();
                        "required"
                    }
                    "unrelated-pending" => {
                        let target = host
                            .load(&fixture("nextUnrelated"), origin("@runtime"))
                            .unwrap();
                        host.selection(&target.id).stage(&target.package).unwrap();
                        "unfinished"
                    }
                    "unrelated-graph-altered" => {
                        let other = host.load(&fixture("other"), origin("@games")).unwrap();
                        let receipt = Receipt {
                            id: other.id,
                            package: other.package,
                            provenance: other.provenance,
                            approval: other.approval,
                            desired: Desired::Disabled,
                            previous: None,
                        };
                        host.prepare(&receipt.id).unwrap();
                        host.selection(&receipt.id).commit(&receipt).unwrap();
                        let target = host
                            .load(&fixture("nextUnrelated"), origin("@runtime"))
                            .unwrap();
                        let before = host.receipt(&unrelated.id).unwrap().unwrap();
                        let after = before
                            .select(target.package, target.provenance, target.approval)
                            .unwrap();
                        host.selection(&after.id).commit(&after).unwrap();
                        "incomplete root update"
                    }
                    _ => unreachable!(),
                };
                if change == "root-swapped" {
                    value["root"] = serde_json::json!(next.brings[0].report.id);
                }
                if change == "root-anchor-omitted" {
                    value.as_object_mut().unwrap().remove("root");
                }
                storage::write_json(&journal, &value).unwrap();
                let bytes = fs::read(&journal).unwrap();
                let receipts = host.receipts().unwrap();
                // Include a newly injected addition's roots even without a receipt.
                let roots: BTreeMap<_, _> = fs::read_dir(&host.paths.roots)
                    .unwrap()
                    .map(|entry| {
                        let entry = entry.unwrap();
                        let links = fs::read_dir(entry.path())
                            .unwrap()
                            .map(|link| {
                                let link = link.unwrap();
                                (link.file_name(), fs::read_link(link.path()).unwrap())
                            })
                            .collect::<BTreeMap<_, _>>();
                        (entry.file_name(), links)
                    })
                    .collect();
                let running = runtime.running.borrow().clone();
                let owned = runtime.owned_packages.borrow().clone();
                runtime.events.borrow_mut().clear();
                let error = host.restore_all().expect_err(change);
                assert!(
                    error.contains(expected),
                    "phase checkpoint {checkpoint}, {change}: {error}"
                );
                assert_eq!(host.receipts().unwrap(), receipts, "{change}");
                assert_eq!(fs::read(&journal).unwrap(), bytes, "{change}");
                for (name, links) in roots {
                    let actual = fs::read_dir(host.paths.roots.join(name))
                        .unwrap()
                        .map(|link| {
                            let link = link.unwrap();
                            (link.file_name(), fs::read_link(link.path()).unwrap())
                        })
                        .collect::<BTreeMap<_, _>>();
                    assert_eq!(actual, links, "{change}");
                }
                assert!(stops(&runtime).is_empty(), "{change}");
                assert!(
                    !runtime
                        .events
                        .borrow()
                        .iter()
                        .any(|event| event.starts_with("start")),
                    "{change}"
                );
                assert_eq!(*runtime.running.borrow(), running, "{change}");
                assert_eq!(*runtime.owned_packages.borrow(), owned, "{change}");
                assert!(runtime.unsafe_stops.borrow().is_empty(), "{change}");
            }
        }
    }

    #[test]
    #[ignore = "requires observed retired signed ARM outputs in the build-machine store; no device access"]
    fn real_store_graph_retired_rg35xxpro_sources_fail_closed_without_effects_or_state_loss() {
        // Observed old graph and then-pinned targets from the RG35XX Pro
        // inventory. ADR 0045 deliberately retires their authored contribution
        // IDs. The real-store one-root-approval update proof moves to the image
        // cut-over stage, against republished signed outputs. Do not rewrite
        // these bytes, change pins, or treat their saved approvals as admission.
        let old = [
            "/nix/store/c0j4s87fx0j81n7s1szhszpimjlp9f48-korri-plugin",
            "/nix/store/66c4hds7g34kra7jkl53wmrm42p60v2w-korri-plugin",
            "/nix/store/2yp6hpj8bk6h78h3p46cq51af6p36kq3-korri-plugin",
            "/nix/store/1wkq4gsf2fkak9w3r3dbzg3nbsrpf5gs-korri-plugin",
        ]
        .map(PathBuf::from);
        let target = [
            "/nix/store/9rz2xjv1ppqzkgxkivvkqdy40573z4kj-korri-plugin",
            "/nix/store/yy6pl7m6k7c2icgvmjig1xp5dcki83wg-korri-plugin",
            "/nix/store/vpirf6d752ax7mbilbx8wv2g61d3vl5r-korri-plugin",
            "/nix/store/fgpg2glx2n202j975w62d73jlj5l86xx-korri-plugin",
        ]
        .map(PathBuf::from);
        let selections = [
            (
                "@korri:starter-pack",
                "1217c3d5c7fcea11da169f96e33a14df331eda4da11f465146b8bc7ddb7fd7b8",
            ),
            (
                "@korri:fake08",
                "8097740bbe89b89b257a4228eecaa0c825cbee5f2199ce2e93c0246440037e99",
            ),
            (
                "@korri:solarus",
                "b955d4d6cb1ee9de8cc5a344e58fa88679001750c3c29dee379b15e0026a6e57",
            ),
            (
                "@korri:zquest-classic",
                "76b2475368d63e273499eb307606fb4298af8dbd13c0f3ca49b6b60e043645f5",
            ),
        ];
        let nix = PathBuf::from(
            std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable build-machine Nix"),
        );
        let root = tempfile::tempdir().unwrap();
        // Real package admission, with controlled native effects so the test can
        // prove there are none. Full-key verification has separate real-store
        // coverage; this checkpoint specifically tests the source clean cut.
        let runtime = TestRuntime {
            real_packages: true,
            ..TestRuntime::default()
        };
        let mut host =
            Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        let cache = "https://github.com/korri-os/plugins/releases/download/cache/";
        host.publishers.insert(
            "@korri".into(),
            package::PublisherBinding {
                public_key: "korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=".into(),
                cache_url: cache.into(),
            },
        );
        let origin = Provenance::RawCache {
            cache_url: cache.into(),
        };
        for (index, (id, approval)) in selections.iter().enumerate() {
            host.prepare(id).unwrap();
            host.selection(id)
                .commit(&Receipt {
                    id: (*id).into(),
                    package: old[index].clone(),
                    provenance: origin.clone(),
                    approval: (*approval).into(),
                    desired: Desired::Enabled,
                    // A stored approval cannot stand in for re-admission, even
                    // when the requested previous selection is also a retired build.
                    previous: Some(crate::selection::Previous {
                        package: target[index].clone(),
                        provenance: origin.clone(),
                        approval: "unusable-retired-target-approval".into(),
                    }),
                })
                .unwrap();
        }
        let user_data = root.path().join("existing-user-data");
        fs::write(&user_data, b"preserve game progress").unwrap();
        let before = host.receipts().unwrap();
        let roots: Vec<_> = before
            .iter()
            .map(|r| {
                ["active", "previous", "pending"]
                    .map(|name| fs::read_link(host.root(&r.id, name)).ok())
            })
            .collect();
        for path in old.iter().chain(&target) {
            assert!(
                path.is_dir(),
                "observed retired output is missing: {}",
                path.display()
            );
            let error = host
                .load(path, origin.clone())
                .err()
                .expect("retired source must not load");
            assert!(
                error.contains("id is derived; remove it"),
                "{}: {error}",
                path.display()
            );
        }
        for (index, (id, _)) in selections.iter().enumerate() {
            // A caller-supplied report/digest must never bypass re-admission.
            let mut requested = report(id, "caller-supplied-review");
            requested.package = target[index].clone();
            requested.provenance = origin.clone();
            let error = host
                .install(
                    requested.clone(),
                    &requested.approval,
                    SelectionIntent::Update { id },
                )
                .unwrap_err();
            assert!(error.contains("id is derived; remove it"), "{id}: {error}");
            let error = host.rollback(id).unwrap_err();
            assert!(error.contains("id is derived; remove it"), "{id}: {error}");
        }
        assert_eq!(host.receipts().unwrap(), before);
        for (receipt, links) in before.iter().zip(roots) {
            for (name, expected) in ["active", "previous", "pending"].into_iter().zip(links) {
                assert_eq!(fs::read_link(host.root(&receipt.id, name)).ok(), expected);
            }
        }
        assert_eq!(fs::read(user_data).unwrap(), b"preserve game progress");
        assert!(
            runtime
                .events
                .borrow()
                .iter()
                .all(|event| !event.starts_with("start:") && !event.starts_with("stop:")),
            "{:?}",
            runtime.events.borrow()
        );
        assert!(!host.state.root().join("graph-update.json").exists());
    }

    #[test]
    #[ignore = "requires builder dependency fixtures and real build-machine Nix; no device or trust configuration"]
    fn real_store_graph_keeps_each_publishers_full_key_cache_source_and_runner() {
        use base64::Engine;
        let fixtures = PathBuf::from(
            std::env::var_os("KORRI_TEST_DEPENDENCY_FIXTURES")
                .expect("built builder-check fixtures"),
        );
        let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
        let helper = package::tools(&PathBuf::from(
            std::env::var_os("KORRI_TEST_TRUE").expect("immutable no-effect helper"),
        ))
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let output = Command::new(&nix)
                .args(["--extra-experimental-features", "nix-command"])
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        };
        let games_secret = run(&["key", "generate-secret", "--key-name", "same-label"]);
        let runtime_secret = run(&["key", "generate-secret", "--key-name", "same-label"]);
        let public = |secret: &str| {
            let (label, value) = secret.split_once(':').unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(value)
                .unwrap();
            format!(
                "{label}:{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes[32..])
            )
        };
        let games_key = root.path().join("games.key");
        let runtime_key = root.path().join("runtime.key");
        fs::write(&games_key, &games_secret).unwrap();
        fs::write(&runtime_key, &runtime_secret).unwrap();
        let runner = fs::canonicalize(fixtures.join("runner")).unwrap();
        let pack = fs::canonicalize(fixtures.join("pack")).unwrap();
        // The pack's key also signs the runner under the SAME label. That
        // cannot grant the runner's namespace, even in an already local store.
        for output in [&pack, &runner] {
            run(&[
                "store",
                "sign",
                "--key-file",
                games_key.to_str().unwrap(),
                output.to_str().unwrap(),
            ]);
        }
        let games_cache = format!("file://{}/games-cache", root.path().display());
        let runtime_cache = format!("file://{}/runtime-cache", root.path().display());
        let mut host = Host::for_test(
            root.path(),
            nix.clone(),
            permitted_policy(),
            TestRuntime::default(),
        )
        .unwrap();
        host.test_runtime = None;
        host.units.systemctl = helper.clone();
        host.units.firewall.ipv4 = helper.clone();
        host.units.firewall.ipv6 = helper;
        host.publishers = BTreeMap::from([
            (
                "@games".into(),
                package::PublisherBinding {
                    public_key: public(&games_secret),
                    cache_url: games_cache.clone(),
                },
            ),
            (
                "@runtime".into(),
                package::PublisherBinding {
                    public_key: public(&runtime_secret),
                    cache_url: runtime_cache.clone(),
                },
            ),
        ]);
        let origin = Provenance::RawCache {
            cache_url: games_cache.clone(),
        };
        assert!(
            host.load(&pack, origin.clone()).is_err(),
            "parent key must not authorize a dependency publisher"
        );
        assert!(host.status("@runtime:fake08").unwrap().is_none());
        let unsigned_seed =
            package::load_graph_for_seed(&[pack.clone(), runner.clone()], &host.publishers)
                .unwrap();
        for report in unsigned_seed {
            let receipt = Receipt {
                id: report.id,
                package: report.package,
                provenance: report.provenance,
                approval: report.approval,
                desired: Desired::Enabled,
                previous: None,
            };
            assert!(
                host.verify_report(
                    &host.approved(&receipt).unwrap(),
                    None,
                    package::StoreContents::Rehash,
                )
                .is_err(),
                "a seeded approval cannot bypass the dependency's full bound key"
            );
        }
        run(&[
            "store",
            "sign",
            "--key-file",
            runtime_key.to_str().unwrap(),
            runner.to_str().unwrap(),
        ]);
        let inspected = host.load(&pack, origin.clone()).unwrap();
        assert_eq!(inspected.requires.as_slice(), std::slice::from_ref(&runner));
        assert!(inspected.files.is_empty());
        assert_eq!(inspected.brings.len(), 1);
        let dependency = &inspected.brings[0].report;
        assert_eq!(dependency.id, "@runtime:fake08");
        assert_eq!(
            dependency.provenance,
            Provenance::RawCache {
                cache_url: runtime_cache.clone()
            }
        );
        assert_eq!(dependency.sources, ["launch.ts", "plugin.ts"]);
        assert!(dependency.files["fake08"].is_file());
        assert!(
            serde_json::to_value(&dependency.declaration).unwrap()["runners"]["fake08"].is_object()
        );
        let wrong_cache = Provenance::RawCache {
            cache_url: games_cache.clone(),
        };
        assert!(host
            .verify_publisher_in(None, &runner, &wrong_cache, package::StoreContents::Rehash)
            .unwrap_err()
            .contains("bound to cache"));
        install(&host, inspected.clone());
        let installed = host.status(&dependency.id).unwrap().unwrap();
        assert_eq!(installed.provenance, dependency.provenance);
        assert_eq!(installed.approval, dependency.approval);
        assert_eq!(installed.desired, Desired::Disabled);
        let reviewed_again = host.load(&pack, origin).unwrap();
        assert!(reviewed_again.brings[0].already_approved);
        assert_eq!(reviewed_again.approval, inspected.approval);
        host.set_enabled(&inspected.id, true).unwrap();
        assert_eq!(
            published(&host).1,
            ["@games:starter-pack", "@runtime:fake08"]
        );
        // The batched projection must equal loading each receipt on its own.
        let batched = serde_json::to_value(host.enabled_packages().unwrap()).unwrap();
        let mut separate = Vec::new();
        for receipt in host.receipts().unwrap() {
            if matches!(receipt.desired, Desired::Enabled) {
                let report = host.approved(&receipt).unwrap();
                host.verify_report(&report, None, package::StoreContents::TrustRegistered)
                    .unwrap();
                separate.push(report);
            }
        }
        assert_eq!(batched, serde_json::to_value(separate).unwrap());
        // Rebinding the dependency's namespace to a key that signed nothing
        // still fails the batched publish. (The games key also signed the
        // runner above, so it cannot stand in for a revoked key.)
        let unused_secret = run(&["key", "generate-secret", "--key-name", "same-label"]);
        let bound = host.publishers.clone();
        host.publishers.get_mut("@runtime").unwrap().public_key = public(&unused_secret);
        assert!(host.enabled_packages().is_err());
        host.publishers = bound;
        assert!(host.set_enabled(&dependency.id, false).is_err());
        assert!(host.remove(&dependency.id, false).is_err());
        host.selection(&dependency.id).stage(&runner).unwrap();
        host.restore_all().unwrap();
        assert!(fs::symlink_metadata(host.root(&dependency.id, "pending")).is_err());
        assert_eq!(
            published(&host).1,
            ["@games:starter-pack", "@runtime:fake08"]
        );
        let collection = fs::canonicalize(fixtures.join("transitive")).unwrap();
        run(&[
            "store",
            "sign",
            "--key-file",
            games_key.to_str().unwrap(),
            collection.to_str().unwrap(),
        ]);
        let collection = host
            .load(&collection, inspected.provenance.clone())
            .unwrap();
        assert_eq!(collection.brings.len(), 2);
        assert_eq!(collection.brings[1].report.approval, inspected.approval);
        install(&host, collection.clone());
        host.set_enabled(&collection.id, true).unwrap();
        assert!(host.set_enabled(&inspected.id, false).is_err());
        host.restore_all().unwrap();
        assert_eq!(
            published(&host).1,
            [
                "@games:collection",
                "@games:starter-pack",
                "@runtime:fake08"
            ]
        );
        // Replace selections with the image producer's existing receipts and
        // roots. Boot must re-derive the same approvals through the real store,
        // publisher checks and closure checks, not activate on seed authority.
        let seeded = package::load_graph_for_seed(
            &[collection.package.clone(), pack.clone(), runner.clone()],
            &host.publishers,
        )
        .unwrap();
        let mut seed_receipts = Vec::new();
        for report in seeded {
            let receipt = Receipt {
                id: report.id,
                package: report.package,
                provenance: report.provenance,
                approval: report.approval,
                desired: Desired::Enabled,
                previous: None,
            };
            let installed = host.status(&receipt.id).unwrap().unwrap();
            assert_eq!(receipt, installed);
            host.selection(&receipt.id).commit(&receipt).unwrap();
            let mut changed = receipt.clone();
            changed.approval = "0".repeat(64);
            assert!(host.approved(&changed).is_err());
            changed = receipt.clone();
            changed.provenance = Provenance::RawCache {
                cache_url: "file:///unbound-cache".into(),
            };
            assert!(host.approved(&changed).is_err());
            seed_receipts.push(receipt);
        }
        host.restore_all().unwrap();
        assert_eq!(
            published(&host).1,
            [
                "@games:collection",
                "@games:starter-pack",
                "@runtime:fake08"
            ]
        );
        let mut corrupt = seed_receipts[0].clone();
        corrupt.approval = "0".repeat(64);
        host.selection(&corrupt.id).commit(&corrupt).unwrap();
        assert!(host.restore_all().is_err());
        assert!(published(&host).1.is_empty());
        for receipt in seed_receipts {
            assert_eq!(
                fs::read_link(host.root(&receipt.id, "active")).unwrap(),
                receipt.package
            );
            host.selection(&receipt.id).commit(&receipt).unwrap();
        }
        host.restore_all().unwrap();
        host.publishers.remove("@runtime");
        assert!(host.restore_all().is_err());
        assert_eq!(published(&host).1, Vec::<String>::new());
        assert_eq!(
            host.status(&inspected.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
    }

    fn stops(runtime: &TestRuntime) -> Vec<String> {
        runtime
            .events
            .borrow()
            .iter()
            .filter(|event| event.starts_with("stop:"))
            .cloned()
            .collect()
    }

    #[test]
    fn unfinished_disabled_or_removed_parent_protects_dependency_and_shared_cascades() {
        for removed in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nix = cleanup_program(root.path(), "exit 0");
            let dependency = report("@runtime:live", "unfinished-dependency");
            let parent = pack(
                "@pack:failed",
                "unfinished-parent",
                std::slice::from_ref(&dependency),
            );
            let other = pack(
                "@pack:other",
                "unfinished-other",
                std::slice::from_ref(&dependency),
            );
            let runtime = runtime(&[dependency.clone(), parent.clone(), other.clone()]);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            install(&host, parent.clone());
            install(&host, other.clone());
            host.set_enabled(&parent.id, true).unwrap();
            host.set_enabled(&other.id, true).unwrap();
            runtime.fail_stop.replace(Some(parent.id.clone()));
            assert!(if removed {
                host.remove(&parent.id, false)
            } else {
                host.set_enabled(&parent.id, false)
            }
            .is_err());
            assert!(runtime.owned.borrow().contains(&parent.id));
            assert!(fs::symlink_metadata(host.root(&parent.id, "pending")).is_ok());
            // Even daemon exit does not establish that ExecStopPost finished.
            runtime.running.borrow_mut().remove(&parent.id);
            runtime.events.borrow_mut().clear();
            assert!(host
                .set_enabled(&dependency.id, false)
                .unwrap_err()
                .contains("cleanup is unfinished"));
            // Service shutdown can release ownership while firewall/settlement
            // still fails. The pending cleanup journal remains a stop guard.
            runtime.owned.borrow_mut().remove(&parent.id);
            runtime.owned_packages.borrow_mut().remove(&parent.id);
            assert!(host.set_enabled(&dependency.id, false).is_err());
            host.set_enabled(&other.id, false).unwrap();
            assert!(!stops(&runtime)
                .iter()
                .any(|e| e == "stop:@runtime:live:false"));
            assert_eq!(
                host.status(&dependency.id).unwrap().unwrap().desired,
                Desired::Enabled
            );
            assert!(runtime.unsafe_stops.borrow().is_empty());
            runtime.fail_stop.replace(None);
            if removed {
                host.remove(&parent.id, false).unwrap();
            } else {
                host.set_enabled(&parent.id, false).unwrap();
            }
            host.set_enabled(&dependency.id, false).unwrap();
            assert!(runtime.unsafe_stops.borrow().is_empty());
        }
    }

    #[test]
    fn live_restore_quiesces_transitive_and_shared_parents_before_required_repair_or_revoke() {
        for revoke in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nix = cleanup_program(root.path(), "exit 0");
            let dependency = report("@runtime:d", "live-d");
            let middle = pack(
                "@pack:middle",
                "live-middle",
                std::slice::from_ref(&dependency),
            );
            let outer = pack("@pack:outer", "live-outer", std::slice::from_ref(&middle));
            let shared = pack(
                "@pack:shared",
                "live-shared",
                std::slice::from_ref(&dependency),
            );
            let leaf = report("@leaf:unrelated", "live-unrelated");
            let runtime = runtime(&[
                dependency.clone(),
                middle.clone(),
                outer.clone(),
                shared.clone(),
                leaf.clone(),
            ]);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            install(&host, outer.clone());
            install(&host, shared.clone());
            install(&host, leaf.clone());
            host.set_enabled(&outer.id, true).unwrap();
            host.set_enabled(&shared.id, true).unwrap();
            host.set_enabled(&leaf.id, true).unwrap();
            if revoke {
                runtime
                    .revoked
                    .borrow_mut()
                    .insert(dependency.package.clone());
            } else {
                host.selection(&dependency.id)
                    .stage(&dependency.package)
                    .unwrap();
            }
            runtime.events.borrow_mut().clear();
            let result = host.restore_all();
            assert_eq!(result.is_err(), revoke);
            let stopped = stops(&runtime);
            let at = |id: &str| {
                stopped
                    .iter()
                    .position(|event| event == &format!("stop:{id}:false"))
                    .unwrap()
            };
            assert!(at(&outer.id) < at(&middle.id));
            assert!(at(&middle.id) < at(&dependency.id));
            assert!(at(&shared.id) < at(&dependency.id));
            assert!(!stopped.iter().any(|e| e.contains(&leaf.id)));
            assert!(
                runtime.unsafe_stops.borrow().is_empty(),
                "{:?}",
                runtime.unsafe_stops.borrow()
            );
            if revoke {
                assert_eq!(
                    host.status(&outer.id).unwrap().unwrap().desired,
                    Desired::Disabled
                );
                assert_eq!(
                    host.status(&middle.id).unwrap().unwrap().desired,
                    Desired::Disabled
                );
                assert_eq!(
                    published(&host).1.as_slice(),
                    std::slice::from_ref(&leaf.id)
                );
            } else {
                for report in [&outer, &middle, &shared, &dependency] {
                    assert_eq!(
                        host.status(&report.id).unwrap().unwrap().desired,
                        Desired::Enabled
                    );
                    assert!(runtime.running.borrow().contains(&report.id));
                    assert!(fs::symlink_metadata(host.root(&report.id, "pending")).is_err());
                }
            }
        }
    }

    #[test]
    fn failed_live_parent_cleanup_refuses_required_stop_on_revoke_and_interruption() {
        for revoke in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nix = cleanup_program(root.path(), "exit 0");
            let dependency = report("@runtime:d", "refused-d");
            let middle = pack(
                "@pack:middle",
                "refused-middle",
                std::slice::from_ref(&dependency),
            );
            let outer = pack(
                "@pack:outer",
                "refused-outer",
                std::slice::from_ref(&middle),
            );
            let runtime = runtime(&[dependency.clone(), middle.clone(), outer.clone()]);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            install(&host, outer.clone());
            host.set_enabled(&outer.id, true).unwrap();
            if revoke {
                runtime
                    .revoked
                    .borrow_mut()
                    .insert(dependency.package.clone());
            } else {
                host.selection(&dependency.id)
                    .stage(&dependency.package)
                    .unwrap();
            }
            runtime.fail_stop.replace(Some(outer.id.clone()));
            runtime.events.borrow_mut().clear();
            assert!(host.restore_all().unwrap_err().contains("cleanup failed"));
            assert_eq!(stops(&runtime), ["stop:@pack:outer:false"]);
            for report in [&dependency, &middle, &outer] {
                assert!(runtime.running.borrow().contains(&report.id));
                assert_eq!(
                    fs::read_link(host.root(&report.id, "active")).unwrap(),
                    report.package
                );
            }
            assert!(runtime.unsafe_stops.borrow().is_empty());
            assert!(published(&host).1.is_empty());
            runtime.fail_stop.replace(None);
            let result = host.restore_all();
            assert_eq!(result.is_err(), revoke);
            assert!(runtime.unsafe_stops.borrow().is_empty());
        }
    }

    #[test]
    fn interrupted_candidate_dependencies_and_root_remain_protected_after_failed_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let old_dependency = report("@runtime:a", "candidate-a");
        let new_dependency = report("@runtime:b", "candidate-b");
        let old = pack(
            "@pack:p",
            "candidate-before",
            std::slice::from_ref(&old_dependency),
        );
        let candidate = pack(
            "@pack:p",
            "candidate-after",
            std::slice::from_ref(&new_dependency),
        );
        let runtime = runtime(&[
            old_dependency.clone(),
            new_dependency.clone(),
            old.clone(),
            candidate.clone(),
        ]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        install(&host, old.clone());
        install(&host, new_dependency.clone());
        host.set_enabled(&new_dependency.id, true).unwrap();
        // Crash after an approved candidate's start, before receipt commit.
        host.selection(&old.id).stage(&candidate.package).unwrap();
        host.units_start(&candidate).unwrap();
        runtime.fail_stop.replace(Some(old.id.clone()));
        assert!(host.set_enabled(&old.id, false).is_err());
        assert_eq!(
            fs::read_link(host.root(&old.id, "pending")).unwrap(),
            candidate.package
        );
        assert!(host.set_enabled(&new_dependency.id, false).is_err());
        runtime.events.borrow_mut().clear();
        assert!(host.restore_all().is_err());
        assert!(!stops(&runtime)
            .iter()
            .any(|e| e.contains(&new_dependency.id)));
        assert_eq!(
            fs::read_link(host.root(&old.id, "pending")).unwrap(),
            candidate.package
        );
        assert!(runtime.unsafe_stops.borrow().is_empty());
        runtime.fail_stop.replace(None);
        host.set_enabled(&old.id, false).unwrap();
        host.set_enabled(&new_dependency.id, false).unwrap();
        assert!(runtime.unsafe_stops.borrow().is_empty());
    }

    #[test]
    fn projection_failures_withdraw_authority_but_retain_committed_graph_and_history() {
        for after_rename in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nix = cleanup_program(root.path(), "exit 0");
            let first = report("@runtime:d", "projection-before");
            let second = report("@runtime:d", "projection-after");
            let parent = pack(
                "@pack:p",
                "projection-parent",
                std::slice::from_ref(&second),
            );
            let runtime = runtime(&[first.clone(), second.clone(), parent.clone()]);
            let host =
                Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
            install(&host, first.clone());
            host.install(
                second.clone(),
                &second.approval,
                SelectionIntent::Update { id: &first.id },
            )
            .unwrap();
            install(&host, parent.clone());
            if after_rename {
                runtime.fail_registry_after_rename.set(true);
            } else {
                runtime.fail_registry.set(true);
            }
            assert!(host.set_enabled(&parent.id, true).is_err());
            assert!(!host.paths.registry_path.exists());
            let receipt = host.status(&second.id).unwrap().unwrap();
            assert_eq!(receipt.package, second.package);
            assert_eq!(receipt.previous.unwrap().package, first.package);
            assert_eq!(
                fs::read_link(host.root(&second.id, "active")).unwrap(),
                second.package
            );
            assert_eq!(
                fs::read_link(host.root(&second.id, "previous")).unwrap(),
                first.package
            );
            assert_eq!(
                fs::read_link(host.root(&parent.id, "active")).unwrap(),
                parent.package
            );
            assert!(host.restore_all().is_err());
            assert!(!host.paths.registry_path.exists());
            assert!(runtime.unsafe_stops.borrow().is_empty());
            runtime.fail_registry.set(false);
            runtime.fail_registry_after_rename.set(false);
            host.set_enabled(&parent.id, true).unwrap();
            assert_eq!(published(&host).1, [parent.id.clone(), second.id.clone()]);
            assert!(runtime.unsafe_stops.borrow().is_empty());
        }
    }

    #[test]
    fn live_restore_preserves_twenty_one_healthy_leaf_receipts_approvals_and_owned_effects() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let reports: Vec<_> = (0..21)
            .map(|i| {
                report(
                    &format!("@korri:leaf-{i:02}"),
                    &format!("healthy-leaf-{i:02}"),
                )
            })
            .collect();
        let runtime = runtime(&reports);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        for report in &reports {
            install(&host, report.clone());
            host.set_enabled(&report.id, true).unwrap();
        }
        let before = host.receipts().unwrap();
        let registry = published(&host).0;
        runtime.events.borrow_mut().clear();
        host.restore_all().unwrap();
        assert_eq!(host.receipts().unwrap(), before);
        assert_eq!(published(&host).0, registry);
        assert_eq!(runtime.running.borrow().len(), 21);
        assert_eq!(runtime.owned.borrow().len(), 21);
        assert!(stops(&runtime).is_empty());
        assert!(!runtime
            .events
            .borrow()
            .iter()
            .any(|e| e.starts_with("start:")));
        assert!(runtime.unsafe_stops.borrow().is_empty());
    }

    #[test]
    fn unknown_restore_relationships_withdraw_projection_without_native_effects_or_root_loss() {
        let root = tempfile::tempdir().unwrap();
        let nix = cleanup_program(root.path(), "exit 0");
        let dependency = report("@runtime:d", "unknown-d");
        let parent = pack(
            "@pack:p",
            "unknown-parent",
            std::slice::from_ref(&dependency),
        );
        let runtime = runtime(&[dependency.clone(), parent.clone()]);
        let host = Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        install(&host, parent.clone());
        host.set_enabled(&parent.id, true).unwrap();
        runtime.reports.borrow_mut().remove(&parent.package);
        runtime.events.borrow_mut().clear();
        assert!(host.restore_all().unwrap_err().contains("cannot discover"));
        assert!(!host.paths.registry_path.exists());
        assert!(stops(&runtime).is_empty());
        assert!(runtime.running.borrow().contains(&parent.id));
        assert!(runtime.running.borrow().contains(&dependency.id));
        for report in [&parent, &dependency] {
            assert_eq!(
                fs::read_link(host.root(&report.id, "active")).unwrap(),
                report.package
            );
        }
        assert!(runtime.unsafe_stops.borrow().is_empty());
    }

    // A failed assertion must not leave the removal child stopped or running.
    struct RemovalChild(Child);

    impl Drop for RemovalChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn interrupted_removal_child() {
        let Some(root) = std::env::var_os("KORRI_HOST_INTERRUPTION_CHILD") else {
            return;
        };
        let root = PathBuf::from(root);
        let selected = report("@test:clock", "clock-interrupted");
        let host = Host::for_test(
            &root,
            root.join("nix"),
            permitted_policy(),
            runtime(std::slice::from_ref(&selected)),
        )
        .unwrap();
        host.remove(&selected.id, false).unwrap();
    }

    #[test]
    fn host_recovers_a_sigkilled_removal_from_the_durable_original_request() {
        let root = tempfile::tempdir().unwrap();
        // Stop the caller at the actual GC boundary, regardless of runner
        // speed. Keep the helper alive until the caller dies: SIGSTOP delivery
        // is asynchronous, so an immediate exit could let removal commit first.
        // tail exits after we reap the caller, including on a failed assertion.
        let nix = cleanup_program(
            root.path(),
            "caller=$PPID; kill -STOP \"$caller\" && exec tail --pid=\"$caller\" -s 0.01 -f /dev/null",
        );
        let selected = report("@test:clock", "clock-interrupted");
        let runtime = runtime(std::slice::from_ref(&selected));
        let host = Host::for_test(
            root.path(),
            nix.clone(),
            permitted_policy(),
            runtime.clone(),
        )
        .unwrap();
        install(&host, selected.clone());
        drop(host);
        // Seed acquisition roots after dropping the setup host; its Drop also
        // releases them. The removal child must remove both names before GC.
        for name in ["download", "download.new"] {
            symlink(&selected.package, root.path().join("roots").join(name)).unwrap();
        }

        let mut child = RemovalChild(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "host::tests::interrupted_removal_child",
                    "--nocapture",
                ])
                .env("KORRI_HOST_INTERRUPTION_CHILD", root.path())
                .spawn()
                .unwrap(),
        );
        let mut event = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
        loop {
            // Observe exit too, so an early child failure is reported instead
            // of waiting forever. Leave reaping to Child, including on panic.
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    child.0.id(),
                    &mut event,
                    libc::WSTOPPED | libc::WEXITED | libc::WNOWAIT,
                )
            };
            if result == 0 {
                break;
            }
            let error = std::io::Error::last_os_error();
            assert_eq!(error.kind(), std::io::ErrorKind::Interrupted, "{error}");
        }
        assert_eq!(
            event.si_code,
            libc::CLD_STOPPED,
            "child exited before store cleanup (status {})",
            unsafe { event.si_status() }
        );
        assert_eq!(unsafe { event.si_status() }, libc::SIGSTOP);
        let receipt_path = root
            .path()
            .join("state")
            .join(package::unit_name(&selected.id))
            .join("selection.json");
        let receipt: Receipt = storage::read_json(&receipt_path).unwrap().unwrap();
        assert!(matches!(receipt.desired, Desired::Removed { purge: false }));
        for name in ["download", "download.new"] {
            assert!(fs::symlink_metadata(root.path().join("roots").join(name)).is_err());
        }
        child.0.kill().unwrap();
        assert_eq!(child.0.wait().unwrap().signal(), Some(libc::SIGKILL));
        cleanup_program(root.path(), "exit 0");

        let recovered =
            Host::for_test(root.path(), nix, permitted_policy(), runtime.clone()).unwrap();
        recovered.restore_all().unwrap();
        assert!(recovered.status(&selected.id).unwrap().is_none());
        assert!(!runtime
            .events
            .borrow()
            .iter()
            .any(|event| event.starts_with("start:")));
    }

    /// Launched explicitly by korrid's two-process remote administration test.
    /// Artifacts and native effects use Host's configured test runtime; the
    /// socket credentials, admin handler, approvals and receipts are real.
    #[test]
    #[ignore = "subprocess helper for korrid remote administration proof"]
    fn remote_admin_process() {
        use crate::admin::*;
        let root = PathBuf::from(std::env::var_os("KORRI_REMOTE_PLUGIN_ROOT").expect("test root"));
        let selected =
            repository_report("@test:clock", "clock-v1", "https://catalog.example/", "v1");
        let runtime = runtime(std::slice::from_ref(&selected));
        let host = Host::for_test(
            &root,
            PathBuf::from("/unavailable-nix"),
            permitted_policy(),
            runtime.clone(),
        )
        .unwrap();
        let config = admin_configuration(Some("https://catalog.example/"));
        let listener = std::os::unix::net::UnixListener::bind(root.join("admin.sock")).unwrap();
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            serve_connection(stream, unsafe { libc::geteuid() }, |request| {
                use std::io::Write;
                let mut audit = fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(root.join("requests.jsonl"))
                    .unwrap();
                writeln!(audit, "{}", serde_json::to_string(&request).unwrap()).unwrap();
                if root.join("stale-policy").exists() {
                    change_policy(&runtime, &selected, "changed-policy-approval");
                }
                handle(&host, &config, request)
            })
            .unwrap();
        }
    }

    fn admin_configuration(official: Option<&str>) -> Configuration {
        Configuration {
            curl: PathBuf::from("/unavailable-curl"),
            official: official.map(|url| SourceUrl::parse(url).unwrap()),
            ca_bundle: None,
        }
    }

    fn admin_failure(response: crate::admin::PluginAdminResponse) -> (String, String) {
        match response {
            crate::admin::PluginAdminResponse::Refused(failure) => (failure.code, failure.message),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn admin_socket_cannot_remove_the_official_repository() {
        use crate::admin::*;
        let root = tempfile::tempdir().unwrap();
        let host = Host::for_test(
            root.path(),
            PathBuf::from("/unavailable-nix"),
            permitted_policy(),
            runtime(&[]),
        )
        .unwrap();
        let official = "https://official.example/catalog.json";
        let config = admin_configuration(Some(official));
        let (code, message) = admin_failure(handle(
            &host,
            &config,
            PluginAdminRequest::RepositoryRemove(PluginRepositoryRequest {
                url: official.into(),
            }),
        ));
        assert_eq!(code, CODE_REFUSED);
        assert!(message.contains("official source"), "{message}");
        // Adding the official source is a no-op answer, not a stored copy.
        assert_eq!(
            handle(
                &host,
                &config,
                PluginAdminRequest::RepositoryAdd(PluginRepositoryRequest {
                    url: official.into(),
                }),
            ),
            PluginAdminResponse::RepositoryAdded(PluginRepositoryAddResult::AlreadyPresent)
        );
        assert_eq!(
            handle(
                &host,
                &config,
                PluginAdminRequest::Repositories(PluginRepositoriesRequest {}),
            ),
            PluginAdminResponse::Repositories(PluginRepositories {
                official: Some(official.into()),
                user_added: Vec::new(),
                repository_installs: Vec::new(),
                other_installs: Vec::new(),
            })
        );
        let (code, message) = admin_failure(handle(
            &host,
            &config,
            PluginAdminRequest::RepositoryAdd(PluginRepositoryRequest {
                url: "http://plain.example/".into(),
            }),
        ));
        assert_eq!(code, CODE_URL_INVALID);
        assert!(message.contains("HTTPS"), "{message}");
    }

    #[test]
    fn install_refuses_a_missing_or_stale_digest_and_starts_disabled() {
        let root = tempfile::tempdir().unwrap();
        let selected = report("@test:clock", "clock-v1");
        let host = Host::for_test(
            root.path(),
            PathBuf::from("/unavailable-nix"),
            permitted_policy(),
            runtime(std::slice::from_ref(&selected)),
        )
        .unwrap();
        for approval in ["", "clock-v0-approval"] {
            let error = host
                .install(selected.clone(), approval, SelectionIntent::Install)
                .unwrap_err();
            assert!(error.contains("approval does not match"), "{error}");
            assert!(host.status(&selected.id).unwrap().is_none());
        }
        install(&host, selected.clone());
        let receipt = host.status(&selected.id).unwrap().unwrap();
        assert_eq!(receipt.desired, Desired::Disabled);
        assert_eq!(receipt.approval, selected.approval);
        assert!(!host
            .test_runtime
            .as_ref()
            .unwrap()
            .events
            .borrow()
            .iter()
            .any(|event| event.starts_with("start:")));
    }

    #[test]
    fn admin_enable_reports_a_stale_stored_approval_as_needing_review() {
        use crate::admin::*;
        let root = tempfile::tempdir().unwrap();
        let mut selected = report("@test:clock", "clock-v1");
        selected.provenance = Provenance::Repository {
            signed: true,
            source_url: "https://removed.example/catalog.json".into(),
            plugin_id: selected.id.clone(),
            release_version: "v1".into(),
            platform: current_platform().into(),
            archive_sha256: "a".repeat(64),
        };
        // An image-seeded (raw-cache) plugin, installed and off.
        let seeded = report("@test:seeded", "seeded-v1");
        let runtime = runtime(&[selected.clone(), seeded.clone()]);
        let host = Host::for_test(
            root.path(),
            PathBuf::from("/unavailable-nix"),
            permitted_policy(),
            runtime.clone(),
        )
        .unwrap();
        install(&host, selected.clone());
        install(&host, seeded.clone());
        let config = admin_configuration(None);
        let set_enabled = |enabled| {
            handle(
                &host,
                &config,
                PluginAdminRequest::SetEnabled(PluginSetEnabledRequest {
                    id: selected.id.clone(),
                    enabled,
                }),
            )
        };
        assert_eq!(
            set_enabled(true),
            PluginAdminResponse::Applied(PluginChangeApplied {})
        );
        // An installed plugin from a repository that is no longer configured
        // stays listed with its real on/off state.
        assert_eq!(
            handle(
                &host,
                &config,
                PluginAdminRequest::Repositories(PluginRepositoriesRequest {}),
            ),
            PluginAdminResponse::Repositories(PluginRepositories {
                official: None,
                user_added: Vec::new(),
                repository_installs: vec![PluginRepositoryInstall {
                    id: selected.id.clone(),
                    source_url: "https://removed.example/catalog.json".into(),
                    release_version: "v1".into(),
                    enabled: true,
                }],
                // Off plugins stay listed; the test package has no readable
                // declaration, so its title is left out.
                other_installs: vec![PluginOtherInstall {
                    id: seeded.id.clone(),
                    title: None,
                    enabled: false,
                }],
            })
        );
        assert_eq!(
            set_enabled(false),
            PluginAdminResponse::Applied(PluginChangeApplied {})
        );
        // The host policy changes, so the stored digest no longer matches.
        runtime
            .reports
            .borrow_mut()
            .get_mut(&selected.package)
            .unwrap()
            .approval = "clock-v1-new-policy-approval".into();
        let (code, message) = admin_failure(set_enabled(true));
        assert_eq!(code, CODE_NEEDS_REVIEW);
        assert_eq!(message, APPROVAL_STALE);
        assert_eq!(
            host.status(&selected.id).unwrap().unwrap().desired,
            Desired::Disabled
        );
        let (code, _) = admin_failure(handle(
            &host,
            &config,
            PluginAdminRequest::SetEnabled(PluginSetEnabledRequest {
                id: "@test:missing".into(),
                enabled: true,
            }),
        ));
        assert_eq!(code, CODE_REFUSED);
    }

    fn repository_report(id: &str, suffix: &str, source: &str, release: &str) -> Report {
        let mut selected = report(id, suffix);
        selected.provenance = Provenance::Repository {
            signed: true,
            source_url: source.into(),
            plugin_id: id.into(),
            release_version: release.into(),
            platform: current_platform().into(),
            archive_sha256: "a".repeat(64),
        };
        selected
    }

    /// The host policy changes, so the plugin's stored approval is stale.
    fn change_policy(runtime: &TestRuntime, selected: &Report, approval: &str) -> Report {
        let mut reviewed = selected.clone();
        reviewed.approval = approval.into();
        runtime
            .reports
            .borrow_mut()
            .insert(reviewed.package.clone(), reviewed.clone());
        reviewed
    }

    fn reviewed_host(root: &Path, selected: &Report) -> (Host, TestRuntime) {
        let runtime = runtime(std::slice::from_ref(selected));
        let host = Host::for_test(
            root,
            PathBuf::from("/unavailable-nix"),
            permitted_policy(),
            runtime.clone(),
        )
        .unwrap();
        host.install_reviewed(selected.clone(), &selected.approval)
            .unwrap();
        (host, runtime)
    }

    #[test]
    fn a_new_review_of_the_installed_selection_replaces_a_stale_approval_and_keeps_its_state() {
        let root = tempfile::tempdir().unwrap();
        let source = "https://plugins.example/catalog.json";
        let selected = repository_report("@test:clock", "clock-v1", source, "v1");
        let (host, runtime) = reviewed_host(root.path(), &selected);
        // An installed, disabled plugin: enabling it finds the stale approval.
        let reviewed = change_policy(&runtime, &selected, "clock-v1-new-policy-approval");
        assert_eq!(
            host.set_enabled(&selected.id, true).unwrap_err(),
            APPROVAL_STALE
        );
        assert!(!fs::symlink_metadata(host.root(&selected.id, "pending")).is_ok());
        // Only the freshly derived digest is accepted.
        assert!(host
            .install_reviewed(reviewed.clone(), &selected.approval)
            .unwrap_err()
            .contains("approval does not match"));
        host.install_reviewed(reviewed.clone(), &reviewed.approval)
            .unwrap();
        let receipt = host.status(&selected.id).unwrap().unwrap();
        assert_eq!(receipt.approval, reviewed.approval);
        assert_eq!(receipt.package, selected.package);
        assert_eq!(receipt.provenance, selected.provenance);
        assert_eq!(receipt.desired, Desired::Disabled);
        assert_eq!(receipt.previous, None);
        host.set_enabled(&selected.id, true).unwrap();
        assert!(runtime.running.borrow().contains(&selected.id));

        // A review of an enabled plugin keeps it on and running.
        let again = change_policy(&runtime, &selected, "clock-v1-third-policy-approval");
        host.install_reviewed(again.clone(), &again.approval)
            .unwrap();
        let receipt = host.status(&selected.id).unwrap().unwrap();
        assert_eq!(receipt.approval, again.approval);
        assert_eq!(receipt.desired, Desired::Enabled);
        assert!(runtime.running.borrow().contains(&selected.id));
    }

    /// A pending root left by unfinished cleanup is never adopted by a review:
    /// on disk, a failed native cleanup and a refused start look the same.
    #[test]
    fn a_review_never_adopts_a_pending_cleanup_journal() {
        let source = "https://plugins.example/catalog.json";
        let selected = repository_report("@test:clock", "clock-v1", source, "v1");

        // A failed native cleanup: disable cannot stop the unit.
        let root = tempfile::tempdir().unwrap();
        let (host, runtime) = reviewed_host(root.path(), &selected);
        host.set_enabled(&selected.id, true).unwrap();
        runtime.fail_stop.replace(Some(selected.id.clone()));
        assert!(host.set_enabled(&selected.id, false).is_err());
        runtime.fail_stop.replace(None);
        assert!(fs::symlink_metadata(host.root(&selected.id, "pending")).is_ok());
        let reviewed = change_policy(&runtime, &selected, "clock-v1-new-policy-approval");
        assert_eq!(
            host.install_reviewed(reviewed.clone(), &reviewed.approval)
                .unwrap_err(),
            APPROVAL_STALE
        );
        let receipt = host.status(&selected.id).unwrap().unwrap();
        assert_eq!(receipt.approval, selected.approval);
        assert!(fs::symlink_metadata(host.root(&selected.id, "pending")).is_ok());

        // A start refused for a stale approval leaves the same journal.
        let root = tempfile::tempdir().unwrap();
        let (host, runtime) = reviewed_host(root.path(), &selected);
        host.set_enabled(&selected.id, true).unwrap();
        let reviewed = change_policy(&runtime, &selected, "clock-v1-new-policy-approval");
        assert_eq!(
            host.set_enabled(&selected.id, true).unwrap_err(),
            APPROVAL_STALE
        );
        assert!(fs::symlink_metadata(host.root(&selected.id, "pending")).is_ok());
        assert_eq!(
            host.install_reviewed(reviewed.clone(), &reviewed.approval)
                .unwrap_err(),
            APPROVAL_STALE
        );
        assert_eq!(
            host.status(&selected.id).unwrap().unwrap().approval,
            selected.approval
        );
    }

    #[test]
    fn an_installed_plugin_refuses_another_release_source_or_package_from_install() {
        let root = tempfile::tempdir().unwrap();
        let source = "https://plugins.example/catalog.json";
        let selected = repository_report("@test:clock", "clock-v1", source, "v1");
        let other_release = repository_report("@test:clock", "clock-v1", source, "v2");
        let other_source = repository_report(
            "@test:clock",
            "clock-v1",
            "https://other.example/catalog.json",
            "v1",
        );
        let other_package = repository_report("@test:clock", "clock-v2", source, "v1");
        let raw_cache = report("@test:clock", "clock-v1");
        let host = Host::for_test(
            root.path(),
            PathBuf::from("/unavailable-nix"),
            permitted_policy(),
            runtime(&[selected.clone(), other_package.clone()]),
        )
        .unwrap();
        host.install_reviewed(selected.clone(), &selected.approval)
            .unwrap();
        for candidate in [other_release, other_source, other_package, raw_cache] {
            let approval = candidate.approval.clone();
            assert_eq!(
                host.install_reviewed(candidate, &approval).unwrap_err(),
                "plugin is installed; use update or explicit source switch"
            );
        }
        assert_eq!(
            host.status(&selected.id).unwrap().unwrap().approval,
            selected.approval
        );
    }
    #[test]
    #[ignore = "requires writable build-machine Nix; no device services or shared-data cleanup"]
    fn direction_signed_archives_trust_unsigned_sideload_and_locked_latest_requirements() {
        use crate::catalog::Catalog;
        use crate::plugin_admin::PluginPublisherState;
        let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
        let helper = package::tools(&PathBuf::from(
            std::env::var_os("KORRI_TEST_TRUE").expect("immutable true"),
        ))
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let result = Command::new(&nix)
                .args(["--extra-experimental-features", "nix-command"])
                .args(args)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            String::from_utf8(result.stdout).unwrap().trim().to_owned()
        };
        let secret = run(&["key", "generate-secret", "--key-name", "direction-test"]);
        let key = root.path().join("key");
        fs::write(&key, &secret).unwrap();
        let public = crate::publisher::public_half(&secret).unwrap();
        let source_url = "https://direction.invalid/catalog.json";
        let make = |namespace: &str, name: &str, title: &str, release: Option<Option<&str>>| {
            let source = root.path().join(format!("{name}-{title}"));
            fs::create_dir(&source).unwrap();
            fs::write(
                source.join("plugin.ts"),
                format!("export const name = {name:?}; export const title = {title:?};"),
            )
            .unwrap();
            let mut manifest = serde_json::json!({"publisher":{"namespace":namespace},"entry":"plugin.ts","sources":["plugin.ts"]});
            if let Some(label) = release {
                let mut spec = serde_json::json!({"repository":source_url,"id":"@dep:runner"});
                if let Some(label) = label {
                    spec["release"] = label.into();
                }
                manifest["requiresReleases"] = serde_json::json!([spec]);
            }
            fs::write(source.join("manifest.json"), manifest.to_string()).unwrap();
            PathBuf::from(run(&["store", "add-path", source.to_str().unwrap()]))
        };
        let assets = root.path().join("assets");
        fs::create_dir(&assets).unwrap();
        let publish = |path: &Path, label: &str| {
            crate::publisher::publish(&crate::publisher::PublishArgs {
                nix: nix.clone(),
                secret_key: Some(key.clone()),
                self_contained: true,
                input_store_path: path.into(),
                release_version: label.into(),
                platform: current_platform().into(),
                archive_url: "https://direction.invalid/".into(),
                output_dir: assets.clone(),
            })
            .unwrap()
        };
        let dep1 = publish(&make("@dep", "runner", "one", None), "v1");
        let dep2 = publish(&make("@dep", "runner", "two", None), "v2");
        let pack = publish(&make("@pack", "pack", "Pack", Some(None)), "root");
        let other = publish(&make("@pack", "other", "Other", Some(Some("v1"))), "root");
        let conflict_source = root.path().join("conflicting-origins");
        fs::create_dir(&conflict_source).unwrap();
        fs::write(
            conflict_source.join("plugin.ts"),
            "export const name = 'conflict';",
        )
        .unwrap();
        fs::write(conflict_source.join("manifest.json"), serde_json::json!({"publisher":{"namespace":"@pack"},"entry":"plugin.ts","sources":["plugin.ts"],"requiresReleases":[{"repository":source_url,"id":"@dep:runner"},{"repository":"https://other.invalid/catalog.json","id":"@dep:runner"}]}).to_string()).unwrap();
        let conflict = publish(
            &PathBuf::from(run(&[
                "store",
                "add-path",
                conflict_source.to_str().unwrap(),
            ])),
            "root",
        );
        assert_eq!(pack.record.platform, "any");
        let unpacked = root.path().join("unpacked");
        fs::create_dir(&unpacked).unwrap();
        archive::extract(&pack.archive_path, &unpacked).unwrap();
        assert!(fs::read_dir(&unpacked)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "narinfo"))
            .all(|e| fs::read_to_string(e.path())
                .unwrap()
                .contains("Sig: direction-test:")));
        let catalog_file = root.path().join("catalog.json");
        let catalog = |latest: &str| Catalog {
            records: vec![
                pack.record.clone(),
                conflict.record.clone(),
                other.record.clone(),
                dep1.record.clone(),
                dep2.record.clone(),
            ],
            latest: BTreeMap::from([
                ("@dep:runner".into(), latest.into()),
                ("@pack:pack".into(), "root".into()),
                ("@pack:other".into(), "root".into()),
            ]),
        };
        fs::write(&catalog_file, catalog("v1").to_json().unwrap()).unwrap();
        // A deterministic resource server, not an intercepted network client.
        // It implements curl's --output/--url file transfer contract with real
        // processes, while signature, import, archive and host code stay real.
        let server = root.path().join("server");
        fs::create_dir_all(server.join("bin")).unwrap();
        let script = server.join("bin/curl");
        fs::write(&script, format!("#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in --output) shift; out=$1;; --url) shift; url=$1;; esac; shift; done\ncase \"$url\" in */catalog.json) exec {} {} \"$out\";; *) name=${{url##*/}}; exec {} {}/\"$name\" \"$out\";; esac\n", helper.parent().unwrap().join("cp").display(), catalog_file.display(), helper.parent().unwrap().join("cp").display(), assets.display())).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let curl =
            PathBuf::from(run(&["store", "add-path", server.to_str().unwrap()])).join("bin/curl");
        let config = Configuration {
            curl,
            official: Some(SourceUrl::parse(source_url).unwrap()),
            ca_bundle: None,
        };
        let host_dir = root.path().join("host");
        fs::create_dir(&host_dir).unwrap();
        let mut host = Host::for_test(
            &host_dir,
            nix.clone(),
            permitted_policy(),
            TestRuntime::default(),
        )
        .unwrap();
        host.test_runtime = None;
        host.units.systemctl = helper.clone();
        host.units.firewall.ipv4 = helper.clone();
        host.units.firewall.ipv6 = helper.clone();
        let inspect = |host: &Host, id: &str| {
            host.inspect_repository(&config, config.official.as_ref().unwrap(), id, "root")
                .unwrap()
        };
        let new = inspect(&host, "@pack:pack");
        assert_eq!(
            new.publisher.as_ref().unwrap().state,
            PluginPublisherState::New
        );
        assert_eq!(
            new.brings[0].report.publisher.as_ref().unwrap().state,
            PluginPublisherState::New
        );
        assert!(host
            .install_reviewed(new.clone(), &host.review_approval(&new).unwrap())
            .unwrap_err()
            .contains("not trusted"));
        host.trust_publisher("@pack", &public).unwrap();
        host.trust_publisher("@dep", &public).unwrap();
        let conflict_error = host
            .inspect_repository(
                &config,
                config.official.as_ref().unwrap(),
                "@pack:conflict",
                "root",
            )
            .err()
            .unwrap();
        assert!(
            conflict_error.contains("conflicting repository requirements for @dep:runner"),
            "{conflict_error}"
        );
        let first = inspect(&host, "@pack:pack");
        let first_digest = host.review_approval(&first).unwrap();
        let candidate_roots: Vec<_> = host
            .candidate_roots
            .borrow()
            .iter()
            .map(|dir| dir.path().to_path_buf())
            .collect();
        let dependency = &first.brings[0].report;
        host.install_one(
            dependency.clone(),
            &dependency.approval,
            SelectionIntent::Install,
        )
        .unwrap();
        assert!(candidate_roots.iter().all(|path| path.exists()), "committing a dependency must not release the root or remaining candidate acquisition roots");
        host.install_reviewed(first.clone(), &first_digest).unwrap();
        assert_eq!(
            host.receipt("@dep:runner").unwrap().unwrap().package,
            PathBuf::from(&dep1.ca_store_path)
        );
        fs::write(&catalog_file, catalog("v2").to_json().unwrap()).unwrap();
        // Ordinary activation reads the dependency receipt, never the catalog.
        host.resolved.borrow_mut().clear();
        host.candidates.borrow_mut().clear();
        host.set_enabled("@pack:pack", true).unwrap();
        assert_eq!(
            host.receipt("@dep:runner").unwrap().unwrap().package,
            PathBuf::from(&dep1.ca_store_path)
        );
        let blocker = inspect(&host, "@pack:other");
        host.install_reviewed(blocker.clone(), &host.review_approval(&blocker).unwrap())
            .unwrap();
        let next = inspect(&host, "@pack:pack");
        assert_ne!(host.review_approval(&next).unwrap(), first_digest);
        assert_eq!(
            next.approval, first.approval,
            "receipt approval is root authority, review approval binds the graph"
        );
        assert_eq!(
            next.brings[0].report.package,
            PathBuf::from(&dep2.ca_store_path)
        );
        let error = host
            .install(
                next.clone(),
                &host.review_approval(&next).unwrap(),
                SelectionIntent::Update { id: "@pack:pack" },
            )
            .unwrap_err();
        assert!(
            error
                .contains("@pack:pack needs @dep:runner v2, but @pack:other locked @dep:runner v1"),
            "{error}"
        );
        // The disabled no-service blocker has no native effects. Remove only
        // this test's receipt/roots, not a real Nix store GC.
        host.selection("@pack:other").remove().unwrap();
        host.install(
            next.clone(),
            &host.review_approval(&next).unwrap(),
            SelectionIntent::Update { id: "@pack:pack" },
        )
        .unwrap();
        assert_eq!(
            host.receipt("@dep:runner").unwrap().unwrap().package,
            PathBuf::from(&dep2.ca_store_path)
        );
        assert_eq!(
            host.receipt("@dep:runner")
                .unwrap()
                .unwrap()
                .previous
                .unwrap()
                .package,
            PathBuf::from(&dep1.ca_store_path)
        );
        let other_secret = run(&["key", "generate-secret", "--key-name", "direction-test"]);
        host.trust_publisher(
            "@pack",
            &crate::publisher::public_half(&other_secret).unwrap(),
        )
        .unwrap();
        let changed = inspect(&host, "@pack:pack");
        assert_eq!(
            changed.publisher.as_ref().unwrap().state,
            PluginPublisherState::KeyChanged
        );
        assert!(host
            .install_reviewed(changed.clone(), &host.review_approval(&changed).unwrap())
            .unwrap_err()
            .contains("key changed: was"));
        host.revoke_publisher("@pack").unwrap();
        host.publishers.insert(
            "@pack".into(),
            package::PublisherBinding {
                public_key: public.clone(),
                cache_url: "https://unreachable.invalid/never-contacted".into(),
            },
        );
        let bound = inspect(&host, "@pack:pack");
        assert_eq!(
            bound.publisher.as_ref().unwrap().state,
            PluginPublisherState::Bound
        );
        assert!(host
            .trust_publisher("@pack", &public)
            .unwrap_err()
            .contains("config-bound"));
        host.publishers.remove("@pack");
        host.candidates.borrow_mut().clear();
        host.resolved.borrow_mut().clear();
        assert!(host.set_enabled("@pack:pack", true).is_err());
        host.set_enabled("@pack:pack", false).unwrap();
        // Signed sideload needs an existing pin; no key is invented from a Sig label.
        assert!(host
            .inspect_archive(&config, &pack.archive_path)
            .err()
            .unwrap()
            .contains("@pack"));
        host.trust_publisher("@pack", &public).unwrap();
        assert!(matches!(
            host.inspect_archive(&config, &pack.archive_path)
                .unwrap()
                .provenance,
            Provenance::Sideload { signed: true, .. }
        ));
        // Strip only the in-archive signatures. Local Nix still has its saved
        // signature, which must not turn this archive into a signed release.
        let unsigned_cache = root.path().join("unsigned-cache");
        fs::create_dir(&unsigned_cache).unwrap();
        archive::extract(&dep1.archive_path, &unsigned_cache).unwrap();
        for e in fs::read_dir(&unsigned_cache)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "narinfo"))
        {
            let text = fs::read_to_string(e.path()).unwrap();
            fs::write(
                e.path(),
                text.lines()
                    .filter(|line| !line.starts_with("Sig:"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n",
            )
            .unwrap();
        }
        let unsigned_path = root.path().join("unsigned.tar");
        archive::pack(&unsigned_cache, &unsigned_path).unwrap();
        assert!(host
            .inspect_archive(&config, &unsigned_path)
            .err()
            .unwrap()
            .contains("unsigned plugins are refused"));
        let unsigned_dir = root.path().join("unsigned-host");
        fs::create_dir(&unsigned_dir).unwrap();
        let mut unsigned_host = Host::for_test(
            &unsigned_dir,
            nix,
            permitted_policy(),
            TestRuntime::default(),
        )
        .unwrap();
        unsigned_host.test_runtime = None;
        unsigned_host.units.systemctl = helper.clone();
        unsigned_host.units.firewall.ipv4 = helper.clone();
        unsigned_host.units.firewall.ipv6 = helper;
        let unsigned = unsigned_host
            .inspect_archive(&config, &unsigned_path)
            .unwrap();
        assert_eq!(
            unsigned.publisher.as_ref().unwrap().disclosure.as_deref(),
            Some(crate::trust::DISCLOSURE)
        );
        assert!(matches!(
            unsigned.provenance,
            Provenance::Sideload { signed: false, .. }
        ));
        unsigned_host
            .install_reviewed(unsigned.clone(), &unsigned.approval)
            .unwrap();
        unsigned_host.set_enabled("@dep:runner", true).unwrap();
        assert!(unsigned_host
            .trust_publisher("@dep", &public)
            .unwrap_err()
            .contains("remove the unsigned @dep plugins first: @dep:runner"));
        let roots: Vec<_> = host
            .candidate_roots
            .borrow()
            .iter()
            .map(|d| d.path().to_path_buf())
            .collect();
        assert!(!roots.is_empty());
        host.release_download().unwrap();
        assert!(roots.iter().all(|p| !p.exists()));
        host.pin_candidate(&PathBuf::from(&pack.ca_store_path))
            .unwrap();
        let abandoned = host.candidate_roots.borrow_mut().pop().unwrap().keep();
        drop(host);
        let mut next_request = Host::for_test(
            &host_dir,
            PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").unwrap()),
            permitted_policy(),
            TestRuntime::default(),
        )
        .unwrap();
        next_request.test_runtime = None;
        let reviewed = inspect(&next_request, "@pack:pack");
        assert_eq!(reviewed.id, "@pack:pack");
        assert!(
            !abandoned.exists(),
            "the next request removes stale staging, including its dangling indirect root"
        );
        let roots: Vec<_> = next_request
            .candidate_roots
            .borrow()
            .iter()
            .map(|d| d.path().to_path_buf())
            .collect();
        next_request.release_download().unwrap();
        assert!(roots.iter().all(|p| !p.exists()));
    }
    #[test]
    #[ignore = "requires builder fixtures and writable build-machine Nix; no native service starts"]
    fn direction_unsigned_native_services_keep_privileged_review_and_any_is_refused() {
        let fixtures = PathBuf::from(
            std::env::var_os("KORRI_TEST_DEPENDENCY_FIXTURES").expect("builder fixtures"),
        );
        let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
        let package = fs::canonicalize(fixtures.join("native")).unwrap();
        assert_eq!(
            crate::publisher::publication_platform(&nix, &package, Some(current_platform()))
                .unwrap(),
            current_platform()
        );
        let root = tempfile::tempdir().unwrap();
        let secret = Command::new(&nix)
            .args([
                "--extra-experimental-features",
                "nix-command",
                "key",
                "generate-secret",
                "--key-name",
                "unsigned-native-test",
            ])
            .output()
            .unwrap();
        assert!(secret.status.success());
        let key = root.path().join("secret");
        fs::write(&key, secret.stdout).unwrap();
        let mut args = crate::publisher::PublishArgs {
            nix: nix.clone(),
            secret_key: Some(key),
            self_contained: true,
            input_store_path: package,
            release_version: "native".into(),
            platform: "any".into(),
            archive_url: String::new(),
            output_dir: root.path().into(),
        };
        assert!(crate::publisher::publish_archive(&args)
            .err()
            .unwrap()
            .contains("any-architecture"));
        args.platform = current_platform().into();
        let archive = crate::publisher::publish_archive(&args).unwrap();
        let cache = root.path().join("cache");
        fs::create_dir(&cache).unwrap();
        archive::extract(&archive.archive_path, &cache).unwrap();
        for entry in fs::read_dir(&cache)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|e| e == "narinfo"))
        {
            let text = fs::read_to_string(entry.path()).unwrap();
            fs::write(
                entry.path(),
                text.lines()
                    .filter(|l| !l.starts_with("Sig:"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n",
            )
            .unwrap();
        }
        let unsigned = root.path().join("unsigned.tar");
        archive::pack(&cache, &unsigned).unwrap();
        let state = root.path().join("host");
        fs::create_dir(&state).unwrap();
        let host = Host::for_test(
            &state,
            nix,
            permitted_policy(),
            TestRuntime {
                real_packages: true,
                ..TestRuntime::default()
            },
        )
        .unwrap();
        let config = admin_configuration(None);
        let report = host.inspect_archive(&config, &unsigned).unwrap();
        assert_eq!(report.native_units.len(), 2);
        assert!(report.warning.contains("DEVICE-WIDE ROOT AUTHORITY"));
        assert!(report.warning.contains("CAP_NET_RAW"));
        assert_eq!(
            report.publisher.as_ref().unwrap().state,
            crate::plugin_admin::PluginPublisherState::Unsigned
        );
        assert_eq!(
            report.publisher.as_ref().unwrap().disclosure.as_deref(),
            Some(crate::trust::DISCLOSURE)
        );
        host.install_reviewed(report.clone(), &report.approval)
            .unwrap();
        assert!(matches!(
            host.status("@runtime:native").unwrap().unwrap().desired,
            Desired::Disabled
        ));
    }
    #[test]
    #[ignore = "requires builder fixtures and writable build-machine Nix; no native service starts"]
    fn direction_exact_requirements_use_each_publishers_repository_record() {
        use crate::catalog::Catalog;
        use crate::plugin_admin::PluginPublisherState;
        let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
        let fixtures = PathBuf::from(
            std::env::var_os("KORRI_TEST_DEPENDENCY_FIXTURES").expect("builder fixtures"),
        );
        let root = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let result = Command::new(&nix)
                .args(["--extra-experimental-features", "nix-command"])
                .args(args)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            String::from_utf8(result.stdout).unwrap().trim().to_owned()
        };
        let assets = root.path().join("assets");
        fs::create_dir(&assets).unwrap();
        let publish = |name: &str, label: &str| {
            let secret = root.path().join(format!("{name}.secret"));
            fs::write(
                &secret,
                run(&["key", "generate-secret", "--key-name", name]),
            )
            .unwrap();
            crate::publisher::publish(&crate::publisher::PublishArgs {
                nix: nix.clone(),
                secret_key: Some(secret),
                self_contained: true,
                input_store_path: fs::canonicalize(fixtures.join(name)).unwrap(),
                release_version: label.into(),
                platform: current_platform().into(),
                archive_url: "https://exact.invalid/".into(),
                output_dir: assets.clone(),
            })
            .unwrap()
        };
        let dependency = publish("runner", "v1");
        let pack = publish("pack", "root");
        assert_eq!(
            package::manifest_requires(Path::new(&pack.ca_store_path)).unwrap(),
            [PathBuf::from(&dependency.ca_store_path)]
        );
        let unsigned_cache = root.path().join("unsigned-cache");
        fs::create_dir(&unsigned_cache).unwrap();
        archive::extract(&dependency.archive_path, &unsigned_cache).unwrap();
        for entry in fs::read_dir(&unsigned_cache)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|e| e == "narinfo"))
        {
            let text = fs::read_to_string(entry.path()).unwrap();
            fs::write(
                entry.path(),
                text.lines()
                    .filter(|line| !line.starts_with("Sig:"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n",
            )
            .unwrap();
        }
        let mut unsigned_record = dependency.record.clone();
        unsigned_record.public_key = None;
        unsigned_record.archive_url = "https://exact.invalid/unsigned.tar".into();
        unsigned_record.archive_sha256 =
            archive::pack(&unsigned_cache, &assets.join("unsigned.tar")).unwrap();
        let catalog_file = root.path().join("catalog.json");
        let server = root.path().join("server");
        fs::create_dir_all(server.join("bin")).unwrap();
        let cp = PathBuf::from(std::env::var_os("KORRI_TEST_TRUE").expect("immutable true"))
            .parent()
            .unwrap()
            .join("cp");
        let script = server.join("bin/curl");
        fs::write(&script, format!("#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in --output) shift; out=$1;; --url) shift; url=$1;; esac; shift; done\ncase \"$url\" in */catalog.json) exec {} {} \"$out\";; *) name=${{url##*/}}; exec {} {}/\"$name\" \"$out\";; esac\n", cp.display(), catalog_file.display(), cp.display(), assets.display())).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let curl =
            PathBuf::from(run(&["store", "add-path", server.to_str().unwrap()])).join("bin/curl");
        let source = SourceUrl::parse("https://exact.invalid/catalog.json").unwrap();
        let config = Configuration {
            curl,
            official: Some(source.clone()),
            ca_bundle: None,
        };
        for lane in ["new", "pinned", "unsigned", "missing"] {
            let record = if lane == "unsigned" {
                &unsigned_record
            } else {
                &dependency.record
            };
            let records = if lane == "missing" {
                vec![pack.record.clone()]
            } else {
                vec![pack.record.clone(), record.clone()]
            };
            let latest = records
                .iter()
                .map(|record| (record.plugin_id.clone(), record.release_version.clone()))
                .collect();
            fs::write(
                &catalog_file,
                Catalog { records, latest }.to_json().unwrap(),
            )
            .unwrap();
            let directory = root.path().join(lane);
            fs::create_dir(&directory).unwrap();
            let host = Host::for_test(
                &directory,
                nix.clone(),
                permitted_policy(),
                TestRuntime {
                    real_packages: true,
                    ..TestRuntime::default()
                },
            )
            .unwrap();
            host.trust_publisher("@games", pack.record.public_key.as_deref().unwrap())
                .unwrap();
            if lane == "pinned" {
                host.trust_publisher("@runtime", dependency.record.public_key.as_deref().unwrap())
                    .unwrap();
            }
            let inspected =
                host.inspect_repository(&config, &source, "@games:starter-pack", "root");
            if lane == "missing" {
                assert_eq!(inspected.err().unwrap(), "@runtime:fake08 is not in https://exact.invalid/catalog.json; publish it there or use a release requirement.");
                assert!(host.installed().unwrap().is_empty());
                continue;
            }
            let inspected = inspected.unwrap();
            let dep = &inspected.brings[0].report;
            assert_eq!(dep.id, "@runtime:fake08");
            assert_eq!(
                dep.publisher.as_ref().unwrap().state,
                match lane {
                    "new" => PluginPublisherState::New,
                    "pinned" => PluginPublisherState::Trusted,
                    _ => PluginPublisherState::Unsigned,
                }
            );
            assert!(
                matches!(&dep.provenance, Provenance::Repository { source_url, release_version, signed, .. } if source_url == source.as_str() && release_version == "v1" && *signed == (lane != "unsigned"))
            );
            if lane == "new" {
                assert!(host
                    .install_reviewed(inspected.clone(), &inspected.approval)
                    .unwrap_err()
                    .contains("publisher @runtime is not trusted"));
            } else {
                if lane == "unsigned" {
                    assert_eq!(
                        dep.publisher.as_ref().unwrap().disclosure.as_deref(),
                        Some(crate::trust::DISCLOSURE)
                    );
                }
                host.install_reviewed(inspected.clone(), &inspected.approval)
                    .unwrap();
                assert_eq!(
                    host.status("@runtime:fake08").unwrap().unwrap().provenance,
                    dep.provenance
                );
            }
            let error = host
                .inspect_archive(&config, &pack.archive_path)
                .err()
                .unwrap();
            assert!(
                error.contains("@runtime:fake08 is not in a repository;")
                    && error.contains("publish it there or use a release requirement"),
                "{error}"
            );
        }
    }
}
