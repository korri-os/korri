//! A transient decision joins the existing exact receipts and pending roots.
//! This finishes an interrupted approved update; it is not graph rollback.
use super::Host;
use crate::{
    declaration::validate_id,
    package::{self, Report},
    selection::{Desired, Receipt},
    storage,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub(super) const JOURNAL: &str = "graph-update.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GraphUpdate {
    phase: Phase,
    selections: Vec<Selection>,
}
#[derive(Deserialize, Serialize)]
enum Phase {
    Prepared,
    Switching,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    before: Option<Receipt>,
    after: Receipt,
}

impl Host {
    pub(super) fn graph_update_pending(&self) -> Result<(), String> {
        if storage::read_json::<GraphUpdate>(&self.state.root().join(JOURNAL))?.is_some() {
            return Err("plugin graph update is unfinished; run restore-all".into());
        }
        Ok(())
    }

    /// The caller has re-inspected and approved the root, and checked both
    /// shared-root refusals and the committed graph's provenance retention.
    pub(super) fn update_root_graph(&self, report: Report) -> Result<(), String> {
        self.verify_report(&report, None, package::StoreContents::Rehash)?;
        let installed = self.receipts()?;
        let root = self.receipt(&report.id)?.ok_or("plugin is not installed")?;
        let mut final_graph = BTreeMap::new();
        for receipt in &installed {
            if matches!(receipt.desired, Desired::Removed { .. }) {
                return Err(format!(
                    "plugin {} removal is unfinished; run restore-all",
                    receipt.id
                ));
            }
            self.settled(receipt)?;
            final_graph.insert(
                receipt.id.clone(),
                (receipt.clone(), self.approved(receipt)?),
            );
        }
        let mut selections = Vec::new();
        for target in report
            .brings
            .iter()
            .map(|dep| &dep.report)
            .chain(std::iter::once(&report))
        {
            let before = self.receipt(&target.id)?;
            if before.is_none()
                && (!self.selection(&target.id).software_released()?
                    || self.units_owned(&target.id)?)
            {
                return Err(format!(
                    "required plugin {} has an unrecorded pending selection or native cleanup; run restore-all",
                    target.id
                ));
            }
            let mut after = match &before {
                Some(before) => before.select(
                    target.package.clone(),
                    target.provenance.clone(),
                    target.approval.clone(),
                )?,
                None => Receipt {
                    id: target.id.clone(),
                    package: target.package.clone(),
                    provenance: target.provenance.clone(),
                    approval: target.approval.clone(),
                    desired: Desired::Disabled,
                    previous: None,
                },
            };
            // Preserve the existing root-update activation contract: an enabled
            // root enables its exact closure; a disabled root leaves independent
            // enabled/disabled choices alone, with new additions disabled.
            if root.desired == Desired::Enabled {
                after.desired = Desired::Enabled;
            }
            self.prepare(&after.id)?;
            final_graph.insert(after.id.clone(), (after.clone(), target.clone()));
            selections.push(Selection { before, after });
        }
        validate_final_graph(&final_graph)?;
        let mut update = GraphUpdate {
            phase: Phase::Prepared,
            selections,
        };
        let switched = (|| {
            storage::write_json(&self.state.root().join(JOURNAL), &update)?;
            self.graph_checkpoint("prepared")?;
            self.invalidate_registry()?;
            for selection in &update.selections {
                self.selection(&selection.after.id)
                    .stage(&selection.after.package)?;
                self.graph_checkpoint("staged")?;
            }
            self.quiesce_graph_update(&update)?;
            update.phase = Phase::Switching;
            storage::write_json(&self.state.root().join(JOURNAL), &update)?;
            self.graph_checkpoint("switching")?;
            self.finish_graph_update(&update)?;
            Ok::<_, String>(())
        })();
        if let Err(error) = switched {
            return Err(format!(
                "graph update interrupted: {error}; selections remain pinned; run restore-all"
            ));
        }
        self.restore_all().map_err(|error| {
            let mut outcome = format!("graph selections committed; restore-all failed: {error}");
            for before in &installed {
                match self.receipt(&before.id) {
                    Ok(Some(after)) if before.desired == Desired::Enabled && after.desired == Desired::Disabled => {
                        outcome.push_str(&format!("; {} left Disabled: its required graph failed restoration (see restore-all errors above)", before.id));
                    }
                    Err(read) => outcome.push_str(&format!("; cannot read final state of {}: {read}", before.id)),
                    _ => {}
                }
            }
            outcome
        })
    }

    fn quiesce_graph_update(&self, update: &GraphUpdate) -> Result<(), String> {
        let mut reports = BTreeMap::new();
        for before in update
            .selections
            .iter()
            .filter_map(|selection| selection.before.as_ref())
        {
            let report = self.approved(before)?;
            for dep in &report.brings {
                reports.insert(dep.report.package.clone(), dep.report.requires.clone());
            }
            reports.insert(report.package, report.requires);
        }
        let order = crate::dependencies::dependency_order(
            update
                .selections
                .iter()
                .filter_map(|s| s.before.as_ref().map(|r| r.package.clone())),
            |path| {
                reports
                    .get(path)
                    .cloned()
                    .ok_or_else(|| "missing approved graph relationship".into())
            },
        )?;
        for path in order.iter().rev() {
            if let Some(selection) = update
                .selections
                .iter()
                .find(|s| s.before.as_ref().is_some_and(|r| r.package == *path))
            {
                let before = selection.before.as_ref().unwrap();
                if before.package == selection.after.package
                    && before.provenance == selection.after.provenance
                    && before.approval == selection.after.approval
                {
                    continue;
                }
                self.units_stop(&before.id, false)?;
                self.graph_checkpoint("stopped")?;
            }
        }
        Ok(())
    }

    fn finish_graph_update(&self, update: &GraphUpdate) -> Result<(), String> {
        for selection in &update.selections {
            self.selection(&selection.after.id)
                .commit(&selection.after)?;
            self.graph_checkpoint("committed")?;
        }
        storage::remove(&self.state.root().join(JOURNAL))?;
        self.graph_checkpoint("finished")
    }

    pub(super) fn recover_graph_update(&self) -> Result<(), String> {
        let path = self.state.root().join(JOURNAL);
        if let Some(update) = storage::read_json::<GraphUpdate>(&path)? {
            if update.selections.is_empty()
                || update.selections.len() > crate::dependencies::MAX_PLUGINS
            {
                return Err("invalid pending graph update size".into());
            }
            let mut ids = BTreeSet::new();
            for selection in &update.selections {
                let after = &selection.after;
                validate_id(&after.id)?;
                if !ids.insert(&after.id) || matches!(after.desired, Desired::Removed { .. }) {
                    return Err("invalid pending graph update selections".into());
                }
                if let Some(before) = &selection.before {
                    let mut expected = before.select(
                        after.package.clone(),
                        after.provenance.clone(),
                        after.approval.clone(),
                    )?;
                    expected.desired = after.desired.clone();
                    if before.id != after.id || expected != *after {
                        return Err("invalid pending graph update history".into());
                    }
                } else if after.previous.is_some() {
                    return Err("invalid new graph selection history".into());
                }
                let current = self.receipt(&after.id)?;
                if current != selection.before
                    && (!matches!(update.phase, Phase::Switching)
                        || current.as_ref() != Some(after))
                {
                    return Err("pending graph update conflicts with installed selection".into());
                }
            }
            match update.phase {
                Phase::Prepared => {
                    self.quiesce_graph_update(&update)?;
                    for selection in &update.selections {
                        match &selection.before {
                            Some(before) => self.selection(&before.id).settle(before)?,
                            None => self.selection(&selection.after.id).remove()?,
                        }
                    }
                    storage::remove(&path)?;
                }
                Phase::Switching => {
                    // This decision performs no starts. Complete the approved
                    // receipts even if a publisher was since revoked; ordinary
                    // restore checks current authority before any activation.
                    for selection in &update.selections {
                        self.approved_graph_candidate(&selection.after, &update)?;
                    }
                    self.finish_graph_update(&update)?;
                }
            }
        }
        storage::remove(&path.with_extension("new"))
    }

    fn approved_graph_candidate(
        &self,
        receipt: &Receipt,
        update: &GraphUpdate,
    ) -> Result<Report, String> {
        #[cfg(test)]
        if self
            .test_runtime
            .as_ref()
            .is_some_and(|runtime| !runtime.real_packages)
        {
            return self.approved(receipt);
        }
        // The durable decision already records the independently approved
        // provenance of every target. A mixed set of live receipts cannot
        // supply it reliably, and current publisher bindings may have changed
        // since approval. Re-derive exact approvals from these recorded inputs;
        // ordinary restore still checks current authority before any start.
        let report = package::load_graph(
            &self.nix,
            None,
            &receipt.package,
            receipt.provenance.clone(),
            |path| {
                update
                    .selections
                    .iter()
                    .find(|selection| selection.after.package == path)
                    .map(|selection| selection.after.provenance.clone())
                    .ok_or_else(|| "pending graph update omits an exact dependency".into())
            },
        )?;
        if report.id != receipt.id || report.approval != receipt.approval {
            return Err(super::APPROVAL_STALE.into());
        }
        Ok(report)
    }

    fn graph_checkpoint(&self, _stage: &str) -> Result<(), String> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            let count = runtime.graph_checkpoints.get() + 1;
            runtime.graph_checkpoints.set(count);
            if runtime.interrupt_graph_at.get() == Some(count) {
                return Err(format!(
                    "injected graph interruption at {_stage}:{count}; run restore-all"
                ));
            }
        }
        Ok(())
    }
}

fn validate_final_graph(graph: &BTreeMap<String, (Receipt, Report)>) -> Result<(), String> {
    for (receipt, report) in graph.values() {
        for dep in &report.brings {
            let selected = graph.get(&dep.report.id).ok_or_else(|| {
                format!(
                    "incomplete root update: {} requires installed plugin {}",
                    report.id, dep.report.id
                )
            })?;
            if selected.0.package != dep.report.package
                || selected.0.provenance != dep.report.provenance
                || selected.0.approval != dep.report.approval
            {
                return Err(format!(
                    "incomplete root update: {} requires exact output {} of {}",
                    report.id,
                    dep.report.package.display(),
                    dep.report.id
                ));
            }
            if receipt.desired == Desired::Enabled && selected.0.desired != Desired::Enabled {
                return Err(format!(
                    "{} requires enabled plugin {}",
                    report.id, dep.report.id
                ));
            }
        }
    }
    crate::dependencies::dependency_order(
        graph.values().map(|(_, r)| r.package.clone()),
        |path: &PathBuf| {
            graph
                .values()
                .find(|(_, r)| r.package == *path)
                .map(|(_, r)| r.requires.clone())
                .ok_or_else(|| "incomplete root update exact requirements".into())
        },
    )?;
    Ok(())
}
