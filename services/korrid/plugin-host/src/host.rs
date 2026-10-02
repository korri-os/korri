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

/// Boot-time checks prepared before restore walks the plugins.
struct BootChecks {
    snapshot: package::StoreSnapshot,
    /// Plugin ID -> (package it was loaded from, its report or load error).
    reports: BTreeMap<String, (PathBuf, Result<Report, String>)>,
}

/// Read-only live/cold restore assessment. This is not another journal.
struct RestorePlan {
    report: Option<Result<Report, String>>,
    authorized: Result<(), String>,
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
    ) -> Result<Report, String> {
        self.report_for_in(selected, provenance, snapshot, None)
    }

    fn report_for_in(
        &self,
        selected: &Path,
        provenance: Provenance,
        snapshot: Option<&package::StoreSnapshot>,
        contents: Option<package::StoreContents>,
    ) -> Result<Report, String> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            return runtime
                .reports
                .borrow()
                .get(selected)
                .cloned()
                .ok_or_else(|| format!("missing test report for {}", selected.display()));
        }
        if let Some(contents) = contents {
            self.verify_publisher_in(snapshot, selected, &provenance, contents)?;
        }
        let receipts = if package::manifest_requires(selected)?.is_empty() {
            Vec::new()
        } else {
            self.receipts()?
        };
        package::load_graph(&self.nix, snapshot, selected, provenance, |path| {
            let provenance = dependency_provenance(path, &receipts, &self.publishers)?;
            if let Some(contents) = contents {
                self.verify_publisher_in(snapshot, path, &provenance, contents)?;
            }
            Ok(provenance)
        })
    }

    fn verify_report(
        &self,
        report: &Report,
        checks: Option<&BootChecks>,
        contents: package::StoreContents,
    ) -> Result<(), String> {
        for report in report
            .brings
            .iter()
            .map(|dep| &dep.report)
            .chain(std::iter::once(report))
        {
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
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            let _ = (provenance, contents);
            if runtime.revoked.borrow().contains(selected) {
                return Err(format!(
                    "injected publisher revocation for {}",
                    selected.display()
                ));
            }
            return Ok(());
        }
        let cache = match provenance {
            Provenance::RawCache { cache_url } => Some(cache_url.as_str()),
            Provenance::Repository { .. } => None,
        };
        package::verify_publisher_in(
            &self.nix,
            snapshot,
            selected,
            cache,
            &self.publishers,
            contents,
        )?;
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
                ));
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
                            ));
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
                            ));
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
                    ));
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
            let cleanup = match self.receipt(id) {
                Ok(Some(_)) => self.deactivate(id, Desired::Removed { purge: false }),
                Ok(None) => self
                    .units_stop(id, false)
                    .and_then(|_| self.selection(id).remove()),
                Err(error) => Err(error),
            };
            if let Err(error) = cleanup {
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
        let checked = self.load(&report.package, report.provenance.clone())?;
        if checked.approval != approval || checked.id != report.id {
            return Err("package dependency closure no longer matches its approval".into());
        }
        let old = self.receipt(&report.id)?;
        intent.validate(
            &report.id,
            old.as_ref().map(|r| &r.provenance),
            &report.provenance,
        )?;
        self.check_required_by(&report.id, false)?;
        let enabled_graph_transition = match old.as_ref() {
            Some(old) if matches!(old.desired, Desired::Enabled) => {
                !checked.requires.is_empty() || !self.approved(old)?.requires.is_empty()
            }
            _ => false,
        };
        let mut additions = Vec::new();
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
                        return Err(format!(
                            "conflicting exact selection for required plugin {}",
                            dep.id
                        ));
                    }
                    self.approved(&receipt)?;
                }
                None => additions.push(dep.id.clone()),
            }
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
                self.install_one(report.clone(), approval, intent)?;
                self.set_enabled(&report.id, true)
            } else {
                self.install_one(report.clone(), approval, intent)
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
                        .and_then(|_| self.selection(id).remove())
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
            ));
        }
        self.release_download()
    }

    fn install_one(
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

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
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
            Ok::<_, String>(report)
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

    fn activate_dependencies(&self, report: &Report) -> Result<(), String> {
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
    ) -> Result<(), String> {
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
                ));
            }
            self.approved_in(&receipt, checks)?;
            if enabled {
                if !matches!(receipt.desired, Desired::Enabled) {
                    return Err(format!("{} requires enabled plugin {}", report.id, dep.id));
                }
                self.settled(&receipt)?;
            }
        }
        Ok(())
    }

    fn check_required_by_enabled(&self, id: &str) -> Result<(), String> {
        self.check_required_by(id, true)
    }

    fn check_required_by(&self, id: &str, active_only: bool) -> Result<(), String> {
        let Some(selected) = self.receipt(id)? else {
            return Ok(());
        };
        for receipt in self.receipts()? {
            if receipt.id == id || (active_only && !self.cleanup_unfinished(&receipt)?) {
                continue;
            }
            if matches!(receipt.desired, Desired::Removed { .. })
                && self.selection(&receipt.id).software_released()?
                && !self.cleanup_unfinished(&receipt)?
            {
                continue;
            }
            let runtime =
                crate::dependencies::dependency_order([receipt.package.clone()], |path| {
                    if *path == receipt.package {
                        self.runtime_requires(&receipt)
                    } else {
                        self.selection_requires(path)
                    }
                })?;
            let report = self.report_for(&receipt.package, receipt.provenance.clone(), None)?;
            if runtime.contains(&selected.package)
                || report.brings.iter().any(|dep| dep.report.id == id)
            {
                return Err(if matches!(receipt.desired, Desired::Enabled) {
                    format!("plugin {id} is actively required by {}", receipt.id)
                } else if active_only {
                    format!(
                        "plugin {id} is actively required by {}; dependent cleanup is unfinished",
                        receipt.id
                    )
                } else {
                    format!("plugin {id} is required by installed plugin {}; remove or update that dependent first", receipt.id)
                });
            }
        }
        Ok(())
    }

    fn units_owned(&self, id: &str) -> Result<bool, String> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            return Ok(runtime.owned.borrow().contains(id));
        }
        self.units.has_owned_units(id)
    }

    fn cleanup_unfinished(&self, receipt: &Receipt) -> Result<bool, String> {
        if matches!(receipt.desired, Desired::Enabled) || self.units_owned(&receipt.id)? {
            return Ok(true);
        }
        match fs::symlink_metadata(self.root(&receipt.id, "pending")) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }

    fn journal_cleanup(&self, receipt: &Receipt) -> Result<(), String> {
        match fs::symlink_metadata(self.root(&receipt.id, "pending")) {
            Ok(_) => Ok(()), // Keep an interrupted candidate pinned through cleanup.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.selection(&receipt.id).stage(&receipt.package)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn runtime_requires(&self, receipt: &Receipt) -> Result<Vec<PathBuf>, String> {
        let mut requires = self.selection_requires(&receipt.package)?;
        match fs::read_link(self.root(&receipt.id, "pending")) {
            Ok(candidate) => requires.extend(self.selection_requires(&candidate)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        requires.sort();
        requires.dedup();
        Ok(requires)
    }

    fn selection_requires(&self, path: &Path) -> Result<Vec<PathBuf>, String> {
        package::validate_store_path(path)?;
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            return runtime
                .reports
                .borrow()
                .get(path)
                .map(|r| r.requires.clone())
                .ok_or_else(|| format!("cannot discover required plugins for {}", path.display()));
        }
        package::manifest_requires(path)
    }

    /// Stop may never use durable disable/removal intent as evidence that an
    /// ancestor's effects finished. A restore pass can supply successful stop
    /// proofs under the same exclusive host lock, not infer them from intent.
    fn check_stop(
        &self,
        id: &str,
        quiesced: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<(), String> {
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
            let order = crate::dependencies::dependency_order([receipt.package.clone()], |path| {
                if *path == receipt.package {
                    self.runtime_requires(&receipt)
                } else {
                    self.selection_requires(path)
                }
            })?;
            if order.contains(&selected.package) {
                return Err(format!(
                    "plugin {id} is actively required by {}; dependent cleanup is unfinished",
                    receipt.id
                ));
            }
        }
        Ok(())
    }

    fn stop_enabled_dependents(&self, package: &Path) -> Result<(), String> {
        // Discovery/size/cycle refusal must not leave a stale projection.
        self.invalidate_registry()?;
        let mut affected = BTreeMap::new();
        for receipt in self.receipts()? {
            if !self.cleanup_unfinished(&receipt)? || receipt.package == package {
                continue;
            }
            let order = crate::dependencies::dependency_order([receipt.package.clone()], |path| {
                if *path == receipt.package {
                    self.runtime_requires(&receipt)
                } else {
                    self.selection_requires(path)
                }
            })?;
            if order.contains(&package.to_path_buf()) {
                let requires = self.runtime_requires(&receipt)?;
                affected.insert(receipt.package.clone(), (receipt, requires));
            }
        }
        let order = crate::dependencies::dependency_order(affected.keys().cloned(), |path| {
            Ok(affected
                .get(path)
                .map(|(_, requires)| requires.clone())
                .unwrap_or_default())
        })?;
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

    pub fn remove(&self, id: &str, purge: bool) -> Result<(), String> {
        self.lifecycle.check_removal(id)?;
        self.prepare(id)?;
        self.deactivate(id, Desired::Removed { purge })
    }

    fn deactivate(&self, id: &str, desired: Desired) -> Result<(), String> {
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
            self.settled(&receipt)?;
            let report = self.approved(&receipt)?;
            self.check_dependencies(&report, true, None)?;
            self.verify_report(&report, None, package::StoreContents::TrustRegistered)?;
            reports.push(report);
        }
        Ok(reports)
    }

    /// An enabled receipt is listed only when its selection is committed and
    /// its active root pins exactly the receipt's package.
    fn settled(&self, receipt: &Receipt) -> Result<(), String> {
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
        Ok(())
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
        let order = crate::dependencies::dependency_order(
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
            if entry.file_name() == "lock" {
                continue;
            }
            let result = if entry.file_name() == storage::SOURCES_DIR
                || entry.file_name() == storage::STAGING_DIR
            {
                storage::directory(&entry.path()).map(|()| Restored::Nothing)
            } else {
                self.restore_directory(&entry.path(), &plans, &quiesced, &blocked)
            };
            match result {
                Ok(Restored::Running(receipt, report)) => enabled.push((receipt, report)),
                Ok(Restored::Stopped(receipt, report)) => stopped.push((receipt, report)),
                Ok(Restored::Nothing) => {}
                Err(error) => errors.push(error),
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
                    errors.push(error);
                    if let Err(error) = self.block_dependency_start(&receipt, &quiesced) {
                        errors.push(error);
                    }
                    outcomes.insert(path, false);
                } else if running {
                    // Even healthy dependents must wait for dependency
                    // authority before reopening their ports or settling roots.
                    match self
                        .firewall_apply(&report)
                        .and_then(|()| self.selection(&receipt.id).settle(&receipt))
                    {
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
                                    errors.push(error);
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
                match result.and_then(|()| self.selection(&receipt.id).settle(&receipt)) {
                    Ok(()) => {
                        outcomes.insert(receipt.package.clone(), true);
                        enabled.push((receipt, report));
                    }
                    Err(error) => {
                        outcomes.insert(receipt.package.clone(), false);
                        errors.push(format!("plugin {}: {error}", receipt.id));
                        if !report.requires.is_empty() {
                            if let Err(error) = self.block_dependency_start(&receipt, &quiesced) {
                                errors.push(error);
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
                Err(error) => errors.push(error),
            }
        }
        self.write_registry(reports)?;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
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
            package::load_graph(
                nix,
                Some(&snapshot),
                &receipt.package,
                receipt.provenance.clone(),
                |path| dependency_provenance(path, &selected, publishers),
            )
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
    ) -> Result<Restored, String> {
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
                .map_err(|error| format!("plugin {id}: {error}"));
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
    ) -> Result<Restored, String> {
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

    fn deny_start(&self, receipt: &Receipt) -> Result<(), String> {
        self.deny_start_in(receipt, &std::collections::BTreeSet::new())
    }

    fn deny_start_in(
        &self,
        receipt: &Receipt,
        quiesced: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<(), String> {
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
    ) -> Result<(), String> {
        self.deny_start_in(receipt, quiesced)?;
        let mut blocked = receipt.clone();
        blocked.desired = Desired::Disabled;
        self.selection(&receipt.id).settle(&blocked)
    }

    fn apply(&self, candidate: Receipt) -> Result<(), String> {
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
        self.write_registry(self.enabled_packages()?)
    }

    fn write_registry(&self, reports: Vec<Report>) -> Result<(), String> {
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
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!(
                    "{error}; failed to withdraw enabled projection: {cleanup}"
                )),
            },
        }
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
                    let order = crate::dependencies::dependency_order([path.clone()], |path| {
                        self.selection_requires(path)
                    })
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
                return Err(format!("injected stop failure for {id}"));
            }
            runtime.running.borrow_mut().remove(id);
            runtime.owned.borrow_mut().remove(id);
            runtime.owned_packages.borrow_mut().remove(id);
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
            if runtime.fail_start.borrow().as_deref() == Some(report.id.as_str())
                || runtime.fail_start_package.borrow().as_deref() == Some(report.package.as_path())
            {
                return Err(format!("injected start failure for {}", report.id));
            }
            runtime.running.borrow_mut().insert(report.id.clone());
            runtime.owned.borrow_mut().insert(report.id.clone());
            runtime
                .owned_packages
                .borrow_mut()
                .insert(report.id.clone(), report.package.clone());
            return Ok(());
        }
        self.units.start(report)
    }

    fn units_start_all(&self, reports: &[&Report]) -> Vec<Result<(), String>> {
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
        self.units.start_all(reports)
    }

    fn units_matches_running(&self, report: &Report) -> Result<bool, String> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            return Ok(runtime.running.borrow().contains(&report.id));
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
        self.approved_in(receipt, None)
    }

    fn approved_in(
        &self,
        receipt: &Receipt,
        checks: Option<&BootChecks>,
    ) -> Result<Report, String> {
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
        let report = if let Some(runtime) = &self.test_runtime {
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

    /// Recovery for single-plugin operations. Boot restore handles enabled
    /// plugins itself and reaches this only for disabled or removed ones.
    fn restore_one(&self, id: &str) -> Result<(), String> {
        self.restore_one_in(id, &std::collections::BTreeSet::new(), None)
    }

    fn restore_one_in(
        &self,
        id: &str,
        quiesced: &std::collections::BTreeSet<PathBuf>,
        plan: Option<&RestorePlan>,
    ) -> Result<(), String> {
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

fn dependency_provenance(
    path: &Path,
    receipts: &[Receipt],
    publishers: &package::PublisherBindings,
) -> Result<Provenance, String> {
    if let Some(receipt) = receipts.iter().find(|receipt| receipt.package == path) {
        return Ok(receipt.provenance.clone());
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
