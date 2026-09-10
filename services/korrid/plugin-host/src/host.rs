use crate::{
    archive,
    declaration::validate_id,
    dependencies::{dependency_order, validate_selection, SelectedPackage},
    package::{self, Report},
    provenance::{current_platform, Provenance, SelectionIntent},
    repository::{self, Configuration, SourceUrl},
    source_store,
    storage::{self, State, ROOTS, STATE_ROOT},
    unit::Units,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub id: String,
    pub package: PathBuf,
    pub provenance: Provenance,
    pub approval: String,
    pub desired: Desired,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "state", deny_unknown_fields)]
pub enum Desired {
    Disabled,
    Enabled,
    Removed { purge: bool },
}

pub struct Host {
    nix: PathBuf,
    publishers: package::PublisherBindings,
    units: Units,
    state: State,
}

impl Drop for Host {
    fn drop(&mut self) {
        // Pending/active roots own selected packages independently of acquisition.
        let _ = self.release_download();
    }
}

impl Host {
    pub fn open(
        nix: &Path,
        systemctl: &Path,
        iptables: &Path,
        ip6tables: &Path,
    ) -> Result<Self, String> {
        let nix = package::tools(nix)?;
        let systemctl = package::tools(systemctl)?;
        let state = State::open(Path::new(STATE_ROOT))?;
        storage::directory(Path::new(ROOTS))?;
        let publishers = package::publisher_bindings(
            &fs::canonicalize("/etc/korri-plugin-host/publishers.json")
                .map_err(|error| format!("publisher bindings are unavailable: {error}"))?,
        )?;
        Ok(Self {
            nix,
            publishers,
            units: Units {
                systemctl,
                firewall: crate::firewall::Firewall {
                    ipv4: package::tools(iptables)?,
                    ipv6: package::tools(ip6tables)?,
                },
            },
            state,
        })
    }

    pub fn inspect(&self, source: &str, package: &Path) -> Result<Report, String> {
        package::validate_store_path(package)?;
        let download = Path::new(ROOTS).join("download");
        storage::root_link(&download, package)?;
        package::import(&self.nix, source, package)?;
        self.load(
            package,
            Provenance::RawCache {
                cache_url: source.into(),
            },
        )
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
        storage::root_link(&Path::new(ROOTS).join("download"), package)?;
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
        let cache = match provenance {
            Provenance::RawCache { cache_url } => Some(cache_url.as_str()),
            Provenance::Repository { .. } => None,
        };
        package::verify_publisher(&self.nix, selected, cache, &self.publishers)?;
        Ok(())
    }

    pub fn release_download(&self) -> Result<(), String> {
        storage::remove(&Path::new(ROOTS).join("download"))
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
        let desired = old
            .as_ref()
            .map(|r| r.desired.clone())
            .unwrap_or(Desired::Disabled);
        self.apply(Receipt {
            id: report.id,
            package: report.package,
            provenance: report.provenance,
            approval: report.approval,
            desired,
        })?;
        self.release_download()
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        validate_id(id)?;
        self.prepare(id)?;
        if !enabled {
            return self.deactivate(id, Desired::Disabled);
        }
        self.recover_one(id)?;
        let mut receipt = self.receipt(id)?.ok_or("plugin is not installed")?;
        receipt.desired = Desired::Enabled;
        self.apply(receipt)
    }

    pub fn remove(&self, id: &str, purge: bool) -> Result<(), String> {
        validate_id(id)?;
        self.prepare(id)?;
        self.deactivate(id, Desired::Removed { purge })
    }

    fn deactivate(&self, id: &str, desired: Desired) -> Result<(), String> {
        let mut receipt = self.receipt(id)?.ok_or("plugin is not installed")?;
        if matches!(receipt.desired, Desired::Removed { .. }) {
            // A prior removal keeps its original purge choice.
            self.restore_one(id)?;
            return Err("plugin is not installed".into());
        }
        // Do not restore a pending enabled selection before stopping it. The
        // immutable approval still authorizes cleanup, not a new daemon start.
        self.approved(&receipt)?;
        // Persist both disable and removal before cleanup: failure or a crash
        // must never roll back this intent into a (possibly revoked) start.
        receipt.desired = desired;
        self.check_selection(&receipt)?;
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
        validate_selection(
            &reports
                .iter()
                .map(|report| SelectedPackage {
                    id: report.id.clone(),
                    package: report.package.clone(),
                    enabled: true,
                    requires: report.requires.clone(),
                })
                .collect::<Vec<_>>(),
        )?;
        crate::plugin_references::validate(
            reports
                .iter()
                .map(|report| {
                    Ok((
                        report.id.clone(),
                        serde_json::to_value(&report.declaration).map_err(|e| e.to_string())?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        )?;
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

    fn check_selection(&self, candidate: &Receipt) -> Result<(), String> {
        let mut receipts = self.receipts()?;
        receipts.retain(|r| r.id != candidate.id);
        receipts.push(candidate.clone());
        let mut packages = Vec::new();
        let mut all_declarations = Vec::new();
        let mut enabled_declarations = Vec::new();
        for receipt in receipts {
            if matches!(receipt.desired, Desired::Removed { .. }) {
                continue;
            }
            let report = self.approved(&receipt)?;
            let declaration = (
                report.id.clone(),
                serde_json::to_value(&report.declaration).map_err(|e| e.to_string())?,
            );
            if matches!(receipt.desired, Desired::Enabled) {
                enabled_declarations.push(declaration.clone());
            }
            all_declarations.push(declaration);
            packages.push(SelectedPackage {
                id: receipt.id,
                package: receipt.package,
                enabled: matches!(receipt.desired, Desired::Enabled),
                requires: report.requires,
            });
        }
        validate_selection(&packages)?;
        crate::plugin_references::validate(all_declarations)?;
        crate::plugin_references::validate(enabled_declarations)
    }

    fn selected_receipt(&self, package: &Path) -> Result<Receipt, String> {
        // Resolve identity from the required immutable artifact, never by
        // enumerating unrelated receipts. Approval and trust are checked by
        // the caller before any start; this lookup grants no authority.
        package::validate_store_path(package)?;
        let id = package::load_declaration(package)?.id();
        let receipt = self.receipt(&id)?.ok_or_else(|| {
            format!(
                "required plugin {id} ({}) is not installed",
                package.display()
            )
        })?;
        if receipt.id != id || receipt.package != package {
            return Err(format!(
                "required plugin {id} is not selected at {}",
                package.display()
            ));
        }
        Ok(receipt)
    }

    fn verify_dependencies(&self, report: &Report) -> Result<(), String> {
        for (_, result) in dependency_order([report.package.clone()], |required| {
            if required == &report.package {
                return Ok(report.requires.clone());
            }
            let receipt = self.selected_receipt(required)?;
            if !matches!(receipt.desired, Desired::Enabled) {
                return Err(format!(
                    "{} requires enabled exact plugin {}",
                    report.id,
                    required.display()
                ));
            }
            if fs::symlink_metadata(self.root(&receipt.id, "pending")).is_ok() {
                return Err(format!(
                    "required plugin {} has an unfinished selection",
                    receipt.id
                ));
            }
            if fs::read_link(self.root(&receipt.id, "active")).map_err(|e| e.to_string())?
                != *required
            {
                return Err(format!(
                    "required plugin {} has an inconsistent active root",
                    receipt.id
                ));
            }
            let dependency = self.approved(&receipt)?;
            self.verify_publisher(&receipt.package, &receipt.provenance)?;
            Ok(dependency.requires)
        }) {
            result?;
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<(), String> {
        self.invalidate_registry()?;
        let mut errors = Vec::new();
        if let Err(error) = self.state.cleanup_staging() {
            errors.push(error);
        }
        let mut receipts = BTreeMap::new();
        for entry in fs::read_dir(STATE_ROOT).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == "lock" {
                continue;
            }
            let result = if entry.file_name() == storage::SOURCES_DIR
                || entry.file_name() == storage::STAGING_DIR
            {
                storage::directory(&entry.path())
            } else {
                (|| {
                    storage::directory(&entry.path())?;
                    match storage::read_json::<Receipt>(&entry.path().join("selection.json"))? {
                        Some(receipt) => {
                            validate_id(&receipt.id)?;
                            if self.directory(&receipt.id) != entry.path() {
                                return Err("receipt identity does not match its directory".into());
                            }
                            if self.selected_receipt(&receipt.package)?.id != receipt.id {
                                return Err("receipt identity does not match its package".into());
                            }
                            receipts.insert(receipt.package.clone(), receipt);
                            Ok(())
                        }
                        None => self.restore_directory(&entry.path(), None),
                    }
                })()
            };
            if let Err(error) = result {
                errors.push(error);
            }
        }
        // Plan the rooted graph once, then recover each exact selection once.
        // Directory order cannot make a dependent restart before a dependency
        // has committed its interrupted selection. Failed recovery also blocks
        // its dependents, even when no pending root was present.
        let order = dependency_order(receipts.keys().cloned().collect::<Vec<_>>(), |path| {
            let receipt = self.selected_receipt(path)?;
            let report = self.approved(&receipt)?;
            let requires = if matches!(receipt.desired, Desired::Enabled) {
                for required in &report.requires {
                    let dependency = self.selected_receipt(required)?;
                    if !matches!(dependency.desired, Desired::Enabled) {
                        return Err(format!(
                            "{} requires enabled exact plugin {}",
                            report.id,
                            required.display()
                        ));
                    }
                }
                report.requires
            } else {
                Vec::new()
            };
            receipts.insert(path.clone(), receipt);
            Ok(requires)
        });
        let mut outcomes: BTreeMap<PathBuf, Result<(), String>> = BTreeMap::new();
        for (path, dependencies) in order {
            let authority = dependencies.and_then(|required| {
                for dependency in required {
                    match outcomes.get(&dependency) {
                        Some(Ok(())) => {}
                        Some(Err(error)) => {
                            return Err(format!(
                                "required plugin {} failed recovery: {error}",
                                dependency.display()
                            ))
                        }
                        None => {
                            return Err(format!(
                                "required plugin {} has no recovery outcome",
                                dependency.display()
                            ))
                        }
                    }
                }
                Ok(())
            });
            let result = if let Some(receipt) = receipts.get(&path) {
                self.restore_directory(&self.directory(&receipt.id), authority.err().as_deref())
            } else {
                authority
            };
            if let Err(error) = &result {
                errors.push(error.clone());
            }
            outcomes.insert(path, result);
        }
        self.release_download()?;
        if errors.is_empty() {
            self.publish_registry()
        } else {
            Err(errors.join("; "))
        }
    }

    fn restore_directory(&self, path: &Path, dependency_error: Option<&str>) -> Result<(), String> {
        storage::directory(path)?;
        if let Some(receipt) = storage::read_json::<Receipt>(&path.join("selection.json"))? {
            if self.directory(&receipt.id) != path {
                return Err("receipt identity does not match its directory".into());
            }
            self.prepare(&receipt.id)?;
            if let Some(error) = dependency_error {
                // Retain pins and approval on denial, but never leave an
                // enabled dependent running after required recovery failed.
                self.approved(&receipt)?;
                if matches!(receipt.desired, Desired::Enabled) {
                    self.units.stop(&receipt.id, false)?;
                }
                return Err(error.into());
            }
            let pending = fs::symlink_metadata(self.root(&receipt.id, "pending")).is_ok();
            if !pending && matches!(receipt.desired, Desired::Enabled) {
                let report = self.approved(&receipt)?;
                if self
                    .verify_publisher(&receipt.package, &receipt.provenance)
                    .is_ok()
                    && fs::read_link(self.root(&receipt.id, "active"))
                        .is_ok_and(|path| path == receipt.package)
                    && self.units.matches_running(&report)?
                {
                    self.units.firewall.apply(&report.id, &report.ports)?;
                    return Ok(());
                }
            }
            // The ordered pass has already recovered and verified the whole
            // required graph. Do not traverse it again for each dependent.
            self.restore_one_with_dependencies(&receipt.id, |_| Ok(()))?;
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
            let roots = Path::new(ROOTS).join(name);
            storage::directory(&roots)?;
            storage::remove(&roots.join("pending"))?;
            storage::remove(&roots.join("active"))?;
        }
        Ok(())
    }

    fn apply(&self, candidate: Receipt) -> Result<(), String> {
        let id = &candidate.id;
        let report = self.approved(&candidate)?;
        self.verify_publisher(&candidate.package, &candidate.provenance)?;
        self.check_selection(&candidate)?;
        self.invalidate_registry()?;
        let pending = self.root(id, "pending");
        storage::root_link(&pending, &candidate.package)?;
        let result = self.units.stop(id, false).and_then(|_| {
            if matches!(candidate.desired, Desired::Enabled) {
                self.verify_dependencies(&report)?;
                self.units.start(&report)
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            return match self.restore_one(id) {
                Ok(()) => Err(format!("operation failed; previous selection restored: {error}")),
                Err(recovery) => Err(format!("operation failed: {error}; recovery failed: {recovery}; package remains pinned; run restore")),
            };
        }
        storage::write_json(&self.receipt_path(id), &candidate)?;
        storage::root_link(&self.root(id, "active"), &candidate.package)?;
        storage::remove(&pending)?;
        self.publish_registry()
    }

    fn invalidate_registry(&self) -> Result<(), String> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let directory = Path::new(crate::plugin_installation::REGISTRY_DIRECTORY);
        if !directory.exists() {
            fs::create_dir(directory).map_err(|e| e.to_string())?;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        let metadata = fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0 {
            return Err("plugin registry directory must be protected by root".into());
        }
        storage::remove(Path::new(crate::plugin_installation::REGISTRY_PATH))
    }

    fn publish_registry(&self) -> Result<(), String> {
        let reports = self.enabled_packages()?;
        let selections: Vec<_> = reports
            .into_iter()
            .map(|report| crate::plugin_installation::EnabledPackage {
                id: report.id,
                package: report.package,
                files: report.files,
                requires: report.requires,
            })
            .collect();
        storage::write_atomic_mode(
            Path::new(crate::plugin_installation::REGISTRY_PATH),
            &crate::plugin_installation::encode(&selections)?,
            0o644,
        )
    }

    fn approved(&self, receipt: &Receipt) -> Result<Report, String> {
        validate_id(&receipt.id)?;
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
        self.restore_one_with_dependencies(id, |report| self.verify_dependencies(report))
    }

    fn restore_one_with_dependencies(
        &self,
        id: &str,
        verify_dependencies: impl FnOnce(&Report) -> Result<(), String>,
    ) -> Result<(), String> {
        let receipt = self.receipt(id)?;
        match receipt {
            Some(receipt) => {
                let report = self.approved(&receipt)?;
                if let Desired::Removed { purge } = receipt.desired {
                    if purge && !self.units.path(id).exists() {
                        self.units.purge_inactive(&report)?;
                    } else {
                        self.units.stop(id, purge)?;
                    }
                    storage::remove(&self.receipt_path(id))?;
                    storage::remove(&self.root(id, "active"))?;
                } else {
                    self.units.stop(id, false)?;
                    if matches!(receipt.desired, Desired::Enabled) {
                        // Stop first even when authority was revoked. Keep the
                        // receipt and roots on denial; never restart revoked code.
                        self.verify_publisher(&receipt.package, &receipt.provenance)?;
                        verify_dependencies(&report)?;
                        self.units.start(&report)?;
                    }
                    storage::root_link(&self.root(id, "active"), &receipt.package)?;
                }
            }
            None => {
                self.units.stop(id, false)?;
            }
        }
        storage::remove(&self.root(id, "pending"))
    }

    fn prepare(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        storage::directory(&self.directory(id))?;
        storage::directory(&Path::new(ROOTS).join(package::unit_name(id)))
    }
    fn directory(&self, id: &str) -> PathBuf {
        Path::new(STATE_ROOT).join(package::unit_name(id))
    }
    fn root(&self, id: &str, name: &str) -> PathBuf {
        Path::new(ROOTS).join(package::unit_name(id)).join(name)
    }
    fn receipt_path(&self, id: &str) -> PathBuf {
        self.directory(id).join("selection.json")
    }
    fn receipt(&self, id: &str) -> Result<Option<Receipt>, String> {
        storage::read_json(&self.receipt_path(id))
    }
}
