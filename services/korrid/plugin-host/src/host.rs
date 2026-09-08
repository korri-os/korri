use crate::{
    declaration::validate_id,
    package::{self, Report},
    storage,
    unit::Units,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

const STATE_ROOT: &str = "/var/lib/korri-plugin-host";
const ROOTS: &str = "/nix/var/nix/gcroots/korri-plugin-host";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub id: String,
    pub package: PathBuf,
    pub approval: String,
    pub desired: Desired,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "state", deny_unknown_fields)]
pub enum Desired {
    Disabled,
    Enabled,
    Removed { purge: bool },
}

pub struct Host {
    nix: PathBuf,
    units: Units,
    _lock: File,
}

impl Host {
    pub fn open(nix: &Path, systemctl: &Path) -> Result<Self, String> {
        let nix = package::tools(nix)?;
        let systemctl = package::tools(systemctl)?;
        storage::directory(Path::new(STATE_ROOT))?;
        storage::directory(Path::new(ROOTS))?;
        let lock = storage::lock(&Path::new(STATE_ROOT).join("lock"))?;
        Ok(Self {
            nix,
            units: Units { systemctl },
            _lock: lock,
        })
    }

    pub fn inspect(&self, source: &str, package: &Path) -> Result<Report, String> {
        package::validate_store_path(package)?;
        let download = Path::new(ROOTS).join("download");
        storage::root_link(&download, package)?;
        package::import(&self.nix, source, package)?;
        package::load(package)
    }

    pub fn release_download(&self) -> Result<(), String> {
        storage::remove(&Path::new(ROOTS).join("download"))
    }

    pub fn install(
        &self,
        report: Report,
        approval: &str,
        update: Option<&str>,
    ) -> Result<(), String> {
        if report.approval != approval {
            return Err(
                "approval does not match this package and its execution policy; inspect it first"
                    .into(),
            );
        }
        if let Some(id) = update {
            if id != report.id {
                return Err("an update cannot change the plugin identity".into());
            }
        }
        self.prepare(&report.id)?;
        self.recover_one(&report.id)?;
        let old = self.receipt(&report.id)?;
        match (&old, update) {
            (Some(_), None) => {
                return Err("plugin is installed; use update with explicit package approval".into())
            }
            (None, Some(_)) => return Err("plugin is not installed".into()),
            _ => {}
        }
        let desired = old
            .as_ref()
            .map(|r| r.desired.clone())
            .unwrap_or(Desired::Disabled);
        self.apply(Receipt {
            id: report.id,
            package: report.package,
            approval: report.approval,
            desired,
        })?;
        self.release_download()
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        validate_id(id)?;
        self.prepare(id)?;
        self.recover_one(id)?;
        let mut receipt = self.receipt(id)?.ok_or("plugin is not installed")?;
        receipt.desired = if enabled {
            Desired::Enabled
        } else {
            Desired::Disabled
        };
        self.apply(receipt)
    }

    pub fn remove(&self, id: &str, purge: bool) -> Result<(), String> {
        validate_id(id)?;
        self.prepare(id)?;
        self.recover_one(id)?;
        let mut receipt = self.receipt(id)?.ok_or("plugin is not installed")?;
        self.approved(&receipt)?;
        // A removal is a durable intent, including the explicit data-deletion
        // choice. Recovery must finish it, never resurrect an enabled plugin.
        receipt.desired = Desired::Removed { purge };
        storage::write_json(&self.receipt_path(id), &receipt)?;
        self.recover_one(id)
    }

    pub fn status(&self, id: &str) -> Result<Option<Receipt>, String> {
        validate_id(id)?;
        self.receipt(id)
    }

    pub fn restore(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        for entry in fs::read_dir(STATE_ROOT).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == "lock" {
                continue;
            }
            if let Err(error) = self.restore_directory(&entry.path()) {
                errors.push(error);
            }
        }
        self.release_download()?;
        if errors.is_empty() {
            Ok(())
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
            if !pending
                && matches!(receipt.desired, Desired::Enabled)
                && self.units.matches_running(&self.approved(&receipt)?)?
            {
                return Ok(());
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
        let pending = self.root(id, "pending");
        storage::root_link(&pending, &candidate.package)?;
        let result = self.units.stop(id, false).and_then(|_| {
            if matches!(candidate.desired, Desired::Enabled) {
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
        storage::remove(&pending)
    }

    fn approved(&self, receipt: &Receipt) -> Result<Report, String> {
        validate_id(&receipt.id)?;
        let report = package::load(&receipt.package)?;
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
