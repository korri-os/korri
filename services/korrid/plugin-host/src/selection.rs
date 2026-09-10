//! The existing selection receipt is the atomic lifecycle commit point.
//! The approved brief adds exactly one prior package/provenance/approval slot.
//! Roots are repaired from that receipt, never used to infer an approval.
use crate::{provenance::Provenance, storage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub id: String,
    pub package: PathBuf,
    pub provenance: Provenance,
    pub approval: String,
    pub desired: Desired,
    // Explicit null means no previous selection. Missing is an old receipt,
    // not permission for a runtime migration.
    #[serde(deserialize_with = "Option::deserialize")]
    pub previous: Option<Previous>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Previous {
    pub package: PathBuf,
    pub provenance: Provenance,
    pub approval: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "state", deny_unknown_fields)]
pub enum Desired {
    Disabled,
    Enabled,
    Removed { purge: bool },
}

impl Receipt {
    pub fn select(
        &self,
        package: PathBuf,
        provenance: Provenance,
        approval: String,
    ) -> Result<Self, String> {
        if matches!(self.desired, Desired::Removed { .. }) {
            return Err("plugin removal is unfinished".into());
        }
        let changed =
            self.package != package || self.provenance != provenance || self.approval != approval;
        Ok(Self {
            id: self.id.clone(),
            package,
            provenance,
            approval,
            desired: self.desired.clone(),
            previous: if changed {
                Some(Previous {
                    package: self.package.clone(),
                    provenance: self.provenance.clone(),
                    approval: self.approval.clone(),
                })
            } else {
                self.previous.clone()
            },
        })
    }

    pub fn rollback(&self) -> Result<Self, String> {
        let previous = self
            .previous
            .as_ref()
            .ok_or("plugin has no previous selection")?;
        self.select(
            previous.package.clone(),
            previous.provenance.clone(),
            previous.approval.clone(),
        )
    }
}

/// Callers hold the host lock and complete approved service effects before
/// commit/settle/remove. Failed effects must leave these receipts and roots
/// intact. During a transition three builds can be pinned; at rest only two.
pub struct SelectionStore {
    receipt: PathBuf,
    roots: PathBuf,
}

impl SelectionStore {
    pub fn new(receipt: PathBuf, roots: PathBuf) -> Self {
        Self { receipt, roots }
    }

    pub fn read(&self) -> Result<Option<Receipt>, String> {
        storage::read_json(&self.receipt)
    }

    pub fn stage(&self, package: &Path) -> Result<(), String> {
        storage::root_link(&self.roots.join("pending"), package)
    }

    pub fn commit(&self, receipt: &Receipt) -> Result<(), String> {
        storage::write_json(&self.receipt, receipt)?;
        self.settle(receipt)
    }

    pub fn settle(&self, receipt: &Receipt) -> Result<(), String> {
        // Before commit, active pins the old selection and pending pins the
        // candidate. Root the new previous before replacing active, so a GC
        // concurrent with any crash boundary can collect neither selection.
        if let Some(previous) = &receipt.previous {
            storage::root_link(&self.roots.join("previous"), &previous.package)?;
        } else {
            storage::remove(&self.roots.join("previous"))?;
        }
        storage::root_link(&self.roots.join("active"), &receipt.package)?;
        storage::remove(&self.roots.join("pending"))?;
        self.remove_temporaries()
    }

    pub fn remove(&self) -> Result<(), String> {
        storage::remove(&self.receipt)?;
        for name in ["active", "previous", "pending"] {
            storage::remove(&self.roots.join(name))?;
        }
        self.remove_temporaries()
    }

    fn remove_temporaries(&self) -> Result<(), String> {
        for name in ["active.new", "previous.new", "pending.new"] {
            storage::remove(&self.roots.join(name))?;
        }
        Ok(())
    }
}
