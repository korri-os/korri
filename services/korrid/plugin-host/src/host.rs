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
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

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
    #[cfg(test)]
    test_runtime: Option<TestRuntime>,
}

#[cfg(test)]
#[derive(Clone, Default)]
struct TestRuntime {
    reports: std::rc::Rc<std::cell::RefCell<BTreeMap<PathBuf, Report>>>,
    events: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    fail_stop: std::rc::Rc<std::cell::RefCell<Option<String>>>,
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
    ) -> Result<Self, String> {
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
                firewall: crate::firewall::Firewall {
                    ipv4: package::tools(iptables)?,
                    ipv6: package::tools(ip6tables)?,
                },
            },
            state,
            paths,
            lifecycle,
            owner_uid: 0,
            #[cfg(test)]
            test_runtime: None,
        })
    }

    #[cfg(test)]
    fn for_test(
        root: &Path,
        nix: PathBuf,
        lifecycle: LifecyclePolicy,
        runtime: TestRuntime,
    ) -> Result<Self, String> {
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
                firewall: crate::firewall::Firewall {
                    ipv4: PathBuf::from("/unavailable-iptables"),
                    ipv6: PathBuf::from("/unavailable-ip6tables"),
                },
            },
            state,
            paths,
            lifecycle,
            owner_uid: unsafe { libc::geteuid() },
            test_runtime: Some(runtime),
        })
    }

    pub fn inspect(&self, source: &str, package: &Path) -> Result<Report, String> {
        package::validate_store_path(package)?;
        let download = self.paths.roots.join("download");
        storage::root_link(&download, package)?;
        package::import(&self.nix, source, package)?;
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
    ) -> Result<Report, String> {
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
            ));
        }
        let paths = crate::release::fetch_paths(
            &package::tools(curl)?,
            &batch,
            revision,
            current_platform(),
            None,
        )?;
        let selected = crate::release::select_output(&paths, id, |path| {
            self.inspect(source, path).map(|report| report.id)
        })?;
        // Each candidate replaces the temporary download root. Reinspect the
        // selected output to retain its root, even if GC ran during the scan.
        let report = self.inspect(source, &selected)?;
        if report.id != id {
            return Err("selected release package changed identity".into());
        }
        Ok(report)
    }

    pub fn sources(&self) -> Result<Vec<SourceUrl>, String> {
        source_store::list_sources(&self.state)
    }

    pub fn add_source(
        &self,
        config: &Configuration,
        source: &SourceUrl,
    ) -> Result<source_store::AddOutcome, String> {
        if config.official.as_ref() == Some(source) {
            return Ok(source_store::AddOutcome::AlreadyPresent);
        }
        repository::fetch_catalog(&config.curl, source, config.ca_bundle.as_deref())?;
        source_store::add(&self.state, source)
    }

    pub fn remove_source(
        &self,
        config: &Configuration,
        source: &SourceUrl,
    ) -> Result<source_store::RemoveOutcome, String> {
        if config.official.as_ref() == Some(source) {
            return Err("official source is managed by trusted host configuration".into());
        }
        source_store::remove(&self.state, source)
    }

    pub fn catalog(
        &self,
        config: &Configuration,
        source: &SourceUrl,
    ) -> Result<crate::catalog::Catalog, String> {
        if config.official.as_ref() != Some(source) && !self.sources()?.contains(source) {
            return Err(format!("repository not configured: {source}"));
        }
        repository::fetch_catalog(&config.curl, source, config.ca_bundle.as_deref())
    }

    pub fn installed_source(&self, id: &str) -> Result<SourceUrl, String> {
        let receipt = self.status(id)?.ok_or("plugin is not installed")?;
        match receipt.provenance {
            Provenance::Repository { source_url, .. } => SourceUrl::parse(&source_url),
            Provenance::RawCache { .. } => Err("plugin was installed from a raw cache; inspect and switch to a repository explicitly".into()),
        }
    }

    pub fn inspect_repository(
        &self,
        config: &Configuration,
        source: &SourceUrl,
        id: &str,
        release: &str,
    ) -> Result<Report, String> {
        validate_id(id)?;
        crate::catalog::validate_release_version(release)?;
        let catalog = self.catalog(config, source)?;
        let record = catalog.record(id, release, current_platform())?;
        self.state.cleanup_staging()?;
        let stage = self.state.staging()?;
        let archive_path = stage.path().join("archive.tar");
        repository::download(
            &config.curl,
            &SourceUrl::parse(&record.archive_url)?,
            config.ca_bundle.as_deref(),
            &archive_path,
            archive::MAX_ARCHIVE_BYTES - 1,
            Duration::from_secs(180),
        )?;
        repository::verify_archive_hash(&archive_path, &record.archive_sha256)?;
        let cache = stage.path().join("cache");
        storage::directory(&cache)?;
        archive::extract(&archive_path, &cache)?;
        archive::preflight_cache(
            &self.nix,
            &record.store_path,
            &cache,
            archive::MAX_NAR_BYTES,
        )?;
        let package = Path::new(&record.store_path);
        storage::root_link(&self.paths.roots.join("download"), package)?;
        package::import(&self.nix, &archive::path_to_file_uri(&cache)?, package)?;
        let provenance = Provenance::Repository {
            source_url: source.to_string(),
            plugin_id: record.plugin_id.clone(),
            release_version: record.release_version.clone(),
            platform: record.platform.clone(),
            archive_sha256: record.archive_sha256.clone(),
        };
        self.load(package, provenance)
    }

    fn load(&self, selected: &Path, provenance: Provenance) -> Result<Report, String> {
        self.verify_publisher(selected, &provenance)?;
        package::load(&self.nix, selected, provenance)
    }

    fn verify_publisher(&self, selected: &Path, provenance: &Provenance) -> Result<(), String> {
        #[cfg(test)]
        if self.test_runtime.is_some() {
            let _ = (selected, provenance);
            return Ok(());
        }
        let cache = match provenance {
            Provenance::RawCache { cache_url } => Some(cache_url.as_str()),
            Provenance::Repository { .. } => None,
        };
        package::verify_publisher(&self.nix, selected, cache, &self.publishers)?;
        Ok(())
    }

    pub fn release_download(&self) -> Result<(), String> {
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
    ) -> Result<ReleaseUpdateReview, String> {
        crate::lifecycle::review_release_update(required, optional)
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
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let (required, _optional_ids) = review.into_parts();
        let mut additions = Vec::new();
        for report in &required {
            let approval = approvals
                .get(&report.id)
                .ok_or_else(|| format!("required plugin {} was not approved", report.id))?;
            if approval != &report.approval {
                return Err(format!(
                    "approval for required plugin {} does not match its disclosed permissions",
                    report.id
                ));
            }
            report.provenance.validate(&report.id)?;
            match self.receipt(&report.id)? {
                Some(receipt) if matches!(receipt.desired, Desired::Removed { .. }) => {
                    return Err(format!("plugin {} removal is unfinished", report.id));
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
                if let Err(error) = self
                    .prepare(&report.id)
                    .and_then(|_| self.selection(&report.id).stage(&report.package))
                {
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
    ) -> Result<(), String> {
        let mut rollback_errors = Vec::new();
        if let Err(error) = self.release_download() {
            rollback_errors.push(error);
        }
        if let Err(error) = self.invalidate_registry() {
            rollback_errors.push(error);
        }
        for id in additions.iter().rev() {
            if let Err(error) = self.selection(id).remove() {
                rollback_errors.push(format!("{id}: {error}"));
            }
        }
        if let Err(error) = self.publish_registry() {
            rollback_errors.push(error);
        }
        if rollback_errors.is_empty() {
            Err(operation_error)
        } else {
            Err(format!(
                "{operation_error}; required-plugin rollback failed: {}",
                rollback_errors.join("; ")
            ))
        }
    }

    pub fn install(
        &self,
        report: Report,
        approval: &str,
        intent: SelectionIntent<'_>,
    ) -> Result<(), String> {
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
        self.apply(candidate)?;
        self.release_download()
    }

    /// Swap the two exact approved selections. No inspection, repository
    /// lookup, import, or dependency substitution is part of rollback.
    pub fn rollback(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        self.prepare(id)?;
        // Validate before recovery can start anything. A refused swap must
        // not disturb the installed graph or discard an interrupted operation.
        let current = self.receipt(id)?.ok_or("plugin is not installed")?;
        self.approved(&current)?;
        let candidate = current.rollback()?;
        self.approved(&candidate)?;
        self.verify_publisher(&candidate.package, &candidate.provenance)?;
        self.recover_one(id)?;
        self.apply(candidate)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        validate_id(id)?;
        self.prepare(id)?;
        if !enabled {
            return self.deactivate(id, Desired::Disabled);
        }
        self.recover_one(id)?;
        let mut receipt = self.receipt(id)?.ok_or("plugin is not installed")?;
        self.approved(&receipt)?;
        receipt.desired = Desired::Enabled;
        self.apply(receipt)
    }

    pub fn remove(&self, id: &str, purge: bool) -> Result<(), String> {
        self.lifecycle.check_removal(id)?;
        self.prepare(id)?;
        self.deactivate(id, Desired::Removed { purge })
    }

    fn deactivate(&self, id: &str, desired: Desired) -> Result<(), String> {
        let mut receipt = self.receipt(id)?.ok_or("plugin is not installed")?;
        if matches!(receipt.desired, Desired::Removed { .. }) {
            // A prior removal keeps its original purge choice. Retrying the
            // command resumes that request instead of replacing it.
            self.restore_one(id)?;
            return self.publish_registry();
        }
        // Do not restore a pending enabled selection before stopping it. The
        // immutable approval still authorizes cleanup, not a new daemon start.
        self.approved(&receipt)?;
        // Persist both disable and removal before cleanup: failure or a crash
        // must never roll back this intent into a (possibly revoked) start.
        receipt.desired = desired;
        self.invalidate_registry()?;
        storage::write_json(&self.receipt_path(id), &receipt)?;
        self.restore_one(id)?;
        self.publish_registry()
    }

    pub fn status(&self, id: &str) -> Result<Option<Receipt>, String> {
        validate_id(id)?;
        self.receipt(id)
    }

    /// The local registry's admission boundary. The host lock makes the list a
    /// single selection snapshot. No package realization, receipt repair or
    /// callback execution occurs. Signature verification may consult the bound
    /// cache; pending transitions never publish launch authority.
    pub fn enabled_packages(&self) -> Result<Vec<Report>, String> {
        let mut reports = Vec::new();
        for receipt in self.receipts()? {
            if !matches!(receipt.desired, Desired::Enabled) {
                continue;
            }
            if fs::symlink_metadata(self.root(&receipt.id, "pending")).is_ok() {
                return Err(format!("plugin {} has an unfinished selection", receipt.id));
            }
            if fs::read_link(self.root(&receipt.id, "active")).map_err(|e| e.to_string())?
                != receipt.package
            {
                return Err(format!(
                    "plugin {} has an inconsistent active root",
                    receipt.id
                ));
            }
            let report = self.approved(&receipt)?;
            self.verify_publisher(&receipt.package, &receipt.provenance)?;
            reports.push(report);
        }
        Ok(reports)
    }

    fn receipts(&self) -> Result<Vec<Receipt>, String> {
        let mut receipts = Vec::new();
        for entry in fs::read_dir(self.state.root()).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == "lock"
                || entry.file_name() == storage::SOURCES_DIR
                || entry.file_name() == storage::STAGING_DIR
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

    pub fn restore_all(&self) -> Result<(), String> {
        self.invalidate_registry()?;
        let mut errors = Vec::new();
        if let Err(error) = self.state.cleanup_staging() {
            errors.push(error);
        }
        self.release_download()?;
        for entry in fs::read_dir(self.state.root()).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == "lock" {
                continue;
            }
            let result = if entry.file_name() == storage::SOURCES_DIR
                || entry.file_name() == storage::STAGING_DIR
            {
                storage::directory(&entry.path())
            } else {
                self.restore_directory(&entry.path())
            };
            if let Err(error) = result {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            self.publish_registry()
        } else {
            Err(errors.join("; "))
        }
    }

    fn restore_directory(&self, path: &Path) -> Result<(), String> {
        storage::directory(path)?;
        if let Some(receipt) = storage::read_json::<Receipt>(&path.join("selection.json"))? {
            if self.directory(&receipt.id) != path {
                return Err("receipt identity does not match its directory".into());
            }
            self.prepare(&receipt.id)?;
            let pending = fs::symlink_metadata(self.root(&receipt.id, "pending")).is_ok();
            if !pending && matches!(receipt.desired, Desired::Enabled) {
                let report = self.approved(&receipt)?;
                if self
                    .verify_publisher(&receipt.package, &receipt.provenance)
                    .is_ok()
                    && fs::read_link(self.root(&receipt.id, "active"))
                        .is_ok_and(|path| path == receipt.package)
                    && self.units_matches_running(&report)?
                {
                    self.firewall_apply(&report)?;
                    self.selection(&receipt.id).settle(&receipt)?;
                    return Ok(());
                }
            }
            self.restore_one(&receipt.id)?;
        } else {
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
        }
        Ok(())
    }

    fn apply(&self, candidate: Receipt) -> Result<(), String> {
        let id = &candidate.id;
        let report = self.approved(&candidate)?;
        self.verify_publisher(&candidate.package, &candidate.provenance)?;
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
                Ok(()) => Err(format!("operation failed; committed selection restored: {error}")),
                Err(recovery) => Err(format!("operation failed: {error}; recovery failed: {recovery}; package remains pinned; run restore-all")),
            };
        }
        self.selection(id).commit(&candidate)?;
        self.publish_registry()
    }

    fn invalidate_registry(&self) -> Result<(), String> {
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
        storage::remove(&self.paths.registry_path)
    }

    fn publish_registry(&self) -> Result<(), String> {
        let reports = self.enabled_packages()?;
        let selections: Vec<_> = reports
            .into_iter()
            .map(|report| crate::plugin_installation::EnabledPackage {
                id: report.id,
                package: report.package,
                files: report.files,
                entry: report.entry,
                sources: report.sources,
            })
            .collect();
        storage::write_atomic_mode(
            &self.paths.registry_path,
            &crate::plugin_installation::encode(&selections)?,
            0o644,
        )
    }

    fn reclaim_software(&self) -> Result<(), String> {
        SoftwareCleanup::new(self.nix.clone()).reclaim()
    }

    fn units_stop(&self, id: &str, purge: bool) -> Result<(), String> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            runtime
                .events
                .borrow_mut()
                .push(format!("stop:{id}:{purge}"));
            if runtime.fail_stop.borrow().as_deref() == Some(id) {
                return Err(format!("injected stop failure for {id}"));
            }
            return Ok(());
        }
        self.units.stop(id, purge)
    }

    fn units_start(&self, report: &Report) -> Result<(), String> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            runtime
                .events
                .borrow_mut()
                .push(format!("start:{}", report.id));
            return Ok(());
        }
        self.units.start(report)
    }

    fn units_matches_running(&self, report: &Report) -> Result<bool, String> {
        #[cfg(test)]
        if self.test_runtime.is_some() {
            let _ = report;
            return Ok(false);
        }
        self.units.matches_running(report)
    }

    fn firewall_apply(&self, report: &Report) -> Result<(), String> {
        #[cfg(test)]
        if self.test_runtime.is_some() {
            return Ok(());
        }
        self.units.firewall.apply(&report.id, &report.ports)
    }

    fn approved(&self, receipt: &Receipt) -> Result<Report, String> {
        validate_id(&receipt.id)?;
        #[cfg(test)]
        let report = if let Some(runtime) = &self.test_runtime {
            runtime
                .reports
                .borrow()
                .get(&receipt.package)
                .cloned()
                .ok_or_else(|| format!("missing test report for {}", receipt.package.display()))?
        } else {
            package::load(&self.nix, &receipt.package, receipt.provenance.clone())?
        };
        #[cfg(not(test))]
        let report = package::load(&self.nix, &receipt.package, receipt.provenance.clone())?;
        if report.id != receipt.id || report.approval != receipt.approval {
            return Err("installed package or host policy no longer matches its approval".into());
        }
        Ok(report)
    }

    fn recover_one(&self, id: &str) -> Result<(), String> {
        let pending = fs::symlink_metadata(self.root(id, "pending")).is_ok();
        let removed = self
            .receipt(id)?
            .is_some_and(|r| matches!(r.desired, Desired::Removed { .. }));
        if pending || removed {
            self.restore_one(id)?;
        }
        Ok(())
    }

    fn restore_one(&self, id: &str) -> Result<(), String> {
        let receipt = self.receipt(id)?;
        match receipt {
            Some(receipt) => {
                let selection = self.selection(id);
                if let Desired::Removed { purge } = receipt.desired {
                    if !selection.software_released()? {
                        let report = self.approved(&receipt)?;
                        if purge && !self.units.path(id).exists() {
                            self.units.purge_inactive(&report)?;
                        } else {
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
                    let report = self.approved(&receipt)?;
                    self.units_stop(id, false)?;
                    if matches!(receipt.desired, Desired::Enabled) {
                        // Stop first even when authority was revoked. Keep the
                        // receipt and roots on denial; never restart revoked code.
                        self.verify_publisher(&receipt.package, &receipt.provenance)?;
                        self.units_start(&report)?;
                    }
                    selection.settle(&receipt)?;
                }
            }
            None => {
                self.units_stop(id, false)?;
                self.selection(id).remove()?;
            }
        }
        Ok(())
    }

    fn prepare(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        storage::directory(&self.directory(id))?;
        storage::directory(&self.paths.roots.join(package::unit_name(id)))
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
    fn receipt(&self, id: &str) -> Result<Option<Receipt>, String> {
        self.selection(id).read()
    }
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
        os::unix::fs::{symlink, PermissionsExt},
        process::{Command, Stdio},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::{Duration, Instant},
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
            packages: BTreeMap::new(),
            files: BTreeMap::new(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
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
        let _ = host.remove(&selected.id, false);
    }

    #[test]
    fn host_recovers_a_sigkilled_removal_from_the_durable_original_request() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("gc-blocked");
        let nix = cleanup_program(
            root.path(),
            &format!("touch {}; while :; do sleep 1; done", marker.display()),
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
        symlink(&selected.package, host.paths.roots.join("download")).unwrap();
        symlink(&selected.package, host.paths.roots.join("download.new")).unwrap();
        drop(host);

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "host::tests::interrupted_removal_child",
                "--nocapture",
            ])
            .env("KORRI_HOST_INTERRUPTION_CHILD", root.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(marker.exists(), "child did not reach store cleanup");
        let receipt_path = root
            .path()
            .join("state")
            .join(package::unit_name(&selected.id))
            .join("selection.json");
        let receipt: Receipt = storage::read_json(&receipt_path).unwrap().unwrap();
        assert!(matches!(receipt.desired, Desired::Removed { purge: false }));
        for name in ["download", "download.new"] {
            assert!(!root.path().join("roots").join(name).exists());
        }
        unsafe {
            libc::kill(child.id() as i32, libc::SIGKILL);
        }
        child.wait().unwrap();
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
}
