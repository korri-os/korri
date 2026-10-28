//! A transient decision joins the existing exact receipts and pending roots.
//! This finishes an interrupted approved update; it is not graph rollback.
use super::Host;
use crate::plugin_admin::{PluginRefusal as Refusal, PluginRefusalCategory as Category};
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
    // Anchor membership to the owner-reviewed root, never to a guessed entry
    // or to a collection of individually reproducible dependency approvals.
    root: String,
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
    pub(super) fn graph_update_pending(&self) -> Result<(), Refusal> {
        if storage::read_json::<GraphUpdate>(&self.state.root().join(JOURNAL))?.is_some() {
            return Err("plugin graph update is unfinished; run restore-all".into());
        }
        Ok(())
    }

    /// The caller has re-inspected and approved the root, and checked both
    /// shared-root refusals and the committed graph's provenance retention.
    pub(super) fn update_root_graph(&self, report: Report) -> Result<(), Refusal> {
        self.verify_report(&report, None, package::StoreContents::Rehash)?;
        let installed = self.receipts()?;
        let root = self.receipt(&report.id)?.ok_or("plugin is not installed")?;
        let mut final_graph = BTreeMap::new();
        for receipt in &installed {
            if matches!(receipt.desired, Desired::Removed { .. }) {
                return Err(format!(
                    "plugin {} removal is unfinished; run restore-all",
                    receipt.id
                )
                .into());
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
                ).into());
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
            root: root.id.clone(),
            phase: Phase::Prepared,
            selections,
        };
        let switched = (|| {
            // Recovery must be able to reload the approved root even if a
            // crash/error precedes the remaining staging operations. Its
            // validated Nix closure pins every exact required output too.
            self.selection(&root.id).stage(&report.package)?;
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
            Ok::<_, Refusal>(())
        })();
        if let Err(error) = switched {
            return Err(format!(
                "graph update interrupted: {error}; selections remain pinned; run restore-all"
            )
            .into());
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
            Refusal::new(error.category, outcome)
        })
    }

    fn quiesce_graph_update(&self, update: &GraphUpdate) -> Result<(), Refusal> {
        let mut reports = BTreeMap::new();
        for before in update
            .selections
            .iter()
            .filter_map(|selection| selection.before.as_ref())
        {
            let report = self.graph_selection_report(before, update, false)?;
            for dep in &report.brings {
                reports.insert(dep.report.package.clone(), dep.report.requires.clone());
            }
            reports.insert(report.package, report.requires);
        }
        let order = crate::dependencies::dependency_order_with::<Refusal>(
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
                    && !update.selections.iter().any(|s| {
                        s.before.as_ref().is_some_and(|b| {
                            b.package != s.after.package
                                || b.provenance != s.after.provenance
                                || b.approval != s.after.approval
                        })
                    })
                {
                    continue;
                }
                self.units_stop(&before.id, false)?;
                self.graph_checkpoint("stopped")?;
            }
        }
        Ok(())
    }

    fn finish_graph_update(&self, update: &GraphUpdate) -> Result<(), Refusal> {
        for selection in &update.selections {
            self.selection(&selection.after.id)
                .commit(&selection.after)?;
            self.graph_checkpoint("committed")?;
        }
        storage::remove(&self.state.root().join(JOURNAL))?;
        self.graph_checkpoint("finished")
    }

    pub(super) fn recover_graph_update(&self) -> Result<(), Refusal> {
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
            self.validate_recovery_graph(&update)?;
            match update.phase {
                Phase::Prepared => {
                    self.quiesce_graph_update(&update)?;
                    // Retire the decision durably before releasing its target
                    // roots. storage::remove fsyncs the parent directory. A
                    // later crash resumes ordinary receipt/pending cleanup,
                    // which does not need a discarded target's approval.
                    storage::remove(&path)?;
                    self.graph_checkpoint("prepared-before-settle")?;
                    for selection in &update.selections {
                        match &selection.before {
                            Some(before) => self.selection(&before.id).settle(before)?,
                            None => self.selection(&selection.after.id).remove()?,
                        }
                    }
                    self.graph_checkpoint("prepared-settled")?;
                }
                Phase::Switching => {
                    // This decision performs no starts. Complete the approved
                    // receipts even if a publisher was since revoked; ordinary
                    // restore checks current authority before any activation.
                    self.finish_graph_update(&update)?;
                }
            }
        }
        storage::remove(&path.with_extension("new")).map_err(Refusal::from)
    }

    fn validate_recovery_graph(&self, update: &GraphUpdate) -> Result<(), Refusal> {
        validate_id(&update.root)?;
        let root = update
            .selections
            .iter()
            .find(|selection| selection.after.id == update.root)
            .ok_or("pending graph update omits its approved root")?;
        let before = root
            .before
            .as_ref()
            .ok_or("pending graph update root is not installed")?;
        if root.after.desired != before.desired {
            return Err("pending graph update changes the root's desired state".into());
        }
        crate::provenance::SelectionIntent::Update { id: &update.root }.validate(
            &update.root,
            Some(&before.provenance),
            &root.after.provenance,
        )?;
        let report = self.approved_graph_candidate(&root.after, update)?;
        let mut candidates: BTreeMap<_, _> = report
            .brings
            .iter()
            .map(|dep| (dep.report.id.clone(), dep.report.clone()))
            .collect();
        candidates.insert(report.id.clone(), report);
        if candidates.len() != update.selections.len() {
            return Err(
                "pending graph update selections differ from the approved root closure".into(),
            );
        }
        let mut updating = BTreeSet::from([update.root.clone()]);
        for selection in &update.selections {
            let after = &selection.after;
            let approved = candidates.get(&after.id).ok_or(
                "pending graph update includes a plugin outside the approved root closure",
            )?;
            if after.package != approved.package
                || after.provenance != approved.provenance
                || after.approval != approved.approval
            {
                return Err(
                    "pending graph update dependency differs from the approved root closure".into(),
                );
            }
            let desired = if root.after.desired == Desired::Enabled {
                Desired::Enabled
            } else {
                selection
                    .before
                    .as_ref()
                    .map(|receipt| receipt.desired.clone())
                    .unwrap_or(Desired::Disabled)
            };
            if after.desired != desired {
                return Err("pending graph update changes a dependency's desired state".into());
            }
            if selection.before.as_ref().is_some_and(|before| {
                before.package != after.package
                    || before.provenance != after.provenance
                    || before.approval != after.approval
            }) {
                updating.insert(after.id.clone());
            }
        }
        // Recheck installed dependents, including disabled ones and any
        // unfinished native cleanup. The host may have been reopened after a
        // crash; current state must not silently expand the approved scope.
        self.check_required_by(&update.root, false)?;
        for id in updating.iter().filter(|id| **id != update.root) {
            self.check_required_by_except(id, false, &updating)?;
        }
        let mut final_graph = BTreeMap::new();
        for receipt in self.receipts()? {
            if candidates.contains_key(&receipt.id) {
                continue;
            }
            if matches!(receipt.desired, Desired::Removed { .. }) {
                return Err("installed plugin removal is unfinished during graph recovery".into());
            }
            self.settled(&receipt)?;
            let report = self.approved(&receipt)?;
            final_graph.insert(receipt.id.clone(), (receipt, report));
        }
        for selection in &update.selections {
            final_graph.insert(
                selection.after.id.clone(),
                (
                    selection.after.clone(),
                    candidates.remove(&selection.after.id).unwrap(),
                ),
            );
        }
        validate_final_graph(&final_graph)
    }

    fn approved_graph_candidate(
        &self,
        receipt: &Receipt,
        update: &GraphUpdate,
    ) -> Result<Report, Refusal> {
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
        self.graph_selection_report(receipt, update, true)
    }

    fn graph_selection_report(
        &self,
        receipt: &Receipt,
        update: &GraphUpdate,
        after: bool,
    ) -> Result<Report, Refusal> {
        #[cfg(test)]
        if self.test_runtime.as_ref().is_some_and(|r| !r.real_packages) {
            return self.approved(receipt);
        }
        let mut selected = self.receipts()?;
        for selection in &update.selections {
            selected.retain(|r| r.id != selection.after.id);
            if after {
                selected.push(selection.after.clone());
            } else if let Some(before) = &selection.before {
                selected.push(before.clone());
            }
        }
        let report = package::load_resolved_graph(
            &self.nix,
            None,
            &receipt.package,
            receipt.provenance.clone(),
            |path| Self::locked_releases(path, &selected),
            |path| super::dependency_provenance(path, &selected, &self.publishers),
        )?;
        if report.id != receipt.id || report.approval != receipt.approval {
            return Err(Refusal::new(Category::NeedsReview, super::APPROVAL_STALE));
        }
        Ok(report)
    }

    fn graph_checkpoint(&self, _stage: &str) -> Result<(), Refusal> {
        #[cfg(test)]
        if let Some(runtime) = &self.test_runtime {
            let count = runtime.graph_checkpoints.get() + 1;
            runtime.graph_checkpoints.set(count);
            if runtime.interrupt_graph_at.get() == Some(count) {
                return Err(format!(
                    "injected graph interruption at {_stage}:{count}; run restore-all"
                )
                .into());
            }
        }
        Ok(())
    }
}

fn validate_final_graph(graph: &BTreeMap<String, (Receipt, Report)>) -> Result<(), Refusal> {
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
                )
                .into());
            }
            if receipt.desired == Desired::Enabled && selected.0.desired != Desired::Enabled {
                return Err(
                    format!("{} requires enabled plugin {}", report.id, dep.report.id).into(),
                );
            }
        }
    }
    crate::dependencies::dependency_order_with::<Refusal>(
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
