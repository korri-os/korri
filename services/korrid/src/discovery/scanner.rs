use crate::discovery::identify::HashCache;
use crate::discovery::{
    attention::{AttentionDecisions, AttentionFinding, AttentionKind},
    identify::{self, AccountRoot, DeclaredScan, ScanReport, SourceInput, SourceScan, SourceView},
    sources::overlaps,
};
use crate::plugin::PluginRegistry;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryDiagnosticCode {
    StorageUnavailable,
    StorageEscaped,
    EntryUnavailable,
    EntryEscaped,
    EntryUnclaimed,
    ClaimConflict,
    HashUnavailable,
    PathUnsupported,
    DiagnosticLimitReached,
    TraversalLimitReached,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryDiagnostic {
    pub code: DiscoveryDiagnosticCode,
    pub storage_id: Option<String>,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraversalBudget {
    pub max_entries: usize,
    pub max_directories: usize,
    pub max_depth: usize,
    pub max_sortable_entries: usize,
}

impl Default for TraversalBudget {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_directories: 10_000,
            max_depth: 32,
            max_sortable_entries: 10_000,
        }
    }
}

trait Identification {
    fn known(&self, system: &str) -> bool;
    fn source(
        &self,
        input: &SourceInput<'_>,
        decisions: &AttentionDecisions,
        cache: &mut HashCache,
        budget: &TraversalBudget,
    ) -> SourceScan;
    fn declared(&self, accounts: &[AccountRoot], cache: &mut HashCache) -> DeclaredScan;
}

impl Identification for PluginRegistry {
    fn known(&self, system: &str) -> bool {
        self.pairing_target(system).is_some()
    }
    fn source(
        &self,
        input: &SourceInput<'_>,
        decisions: &AttentionDecisions,
        cache: &mut HashCache,
        budget: &TraversalBudget,
    ) -> SourceScan {
        identify::identify_source(input, self, decisions, cache, budget)
    }
    fn declared(&self, accounts: &[AccountRoot], cache: &mut HashCache) -> DeclaredScan {
        identify::resolve_declared(accounts, self, cache)
    }
}

pub fn scan(
    sources: &[SourceView],
    accounts: &[AccountRoot],
    excluded_roots: &[PathBuf],
    registry: &PluginRegistry,
    decisions: &AttentionDecisions,
    cache: &mut HashCache,
    budget: &TraversalBudget,
) -> ScanReport {
    let mut report = scan_with(
        sources,
        accounts,
        excluded_roots,
        registry,
        decisions,
        cache,
        budget,
    );
    report.declared_games = registry.declared_games().clone();
    report
}

fn scan_with(
    sources: &[SourceView],
    accounts: &[AccountRoot],
    excluded_roots: &[PathBuf],
    identify: &impl Identification,
    decisions: &AttentionDecisions,
    cache: &mut HashCache,
    budget: &TraversalBudget,
) -> ScanReport {
    let mut report = ScanReport {
        accounts: accounts.to_vec(),
        ..ScanReport::default()
    };
    // Resolve paths again on each pass: hand edits and symlink changes must not use stale roots.
    let roots: Vec<_> = sources
        .iter()
        .map(|source| source.root.canonicalize().ok())
        .collect();
    for (index, source) in sources.iter().enumerate() {
        if !source.scans_files {
            continue;
        }
        let root = roots[index].as_deref();
        let overlapping = root.is_some_and(|root| {
            roots.iter().enumerate().any(|(other, path)| {
                index != other && path.as_deref().is_some_and(|path| overlaps(root, path))
            })
        });
        let blocked = root.is_some_and(|root| {
            excluded_roots
                .iter()
                .any(|excluded| overlaps(root, excluded))
        });
        let kind = if overlapping {
            Some(AttentionKind::Overlap)
        } else if !identify.known(&source.system) {
            Some(AttentionKind::SystemUnavailable)
        } else if blocked || root.is_none() || !root.is_some_and(|path| path.is_dir()) {
            Some(AttentionKind::Unreadable)
        } else {
            None
        };
        if let Some(kind) = kind {
            report.skipped.push(source.source_id.clone());
            report.attention.push(AttentionFinding {
                kind,
                source_id: source.source_id.clone(),
                path: None,
                identity: None,
                candidates: Vec::new(),
                message: match kind {
                    AttentionKind::Overlap => "source overlaps another source",
                    AttentionKind::SystemUnavailable => "paired system or group is unavailable",
                    _ => "source is unavailable or holds plugin data",
                }
                .into(),
            });
            continue;
        }
        let input = SourceInput {
            source_id: &source.source_id,
            storage_id: &source.storage_id,
            system: &source.system,
            root: root.expect("ready source has a root"),
            excluded_roots,
        };
        let source_scan = identify.source(&input, decisions, cache, budget);
        if incomplete(&source_scan) {
            // Reconciliation may remove unseen paths only after a complete walk.
            report.skipped.push(source.source_id.clone());
            report.attention.extend(source_scan.attention);
            report.diagnostics.extend(source_scan.diagnostics);
        } else {
            report.scanned.push(source_scan);
        }
    }
    report.declared = identify.declared(accounts, cache);
    report
}

pub(crate) fn incomplete(scan: &SourceScan) -> bool {
    scan.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            DiscoveryDiagnosticCode::TraversalLimitReached
                | DiscoveryDiagnosticCode::StorageUnavailable
                | DiscoveryDiagnosticCode::EntryUnavailable
        )
    })
}

pub fn attention_findings(report: &ScanReport) -> Vec<AttentionFinding> {
    report
        .attention
        .iter()
        .chain(report.scanned.iter().flat_map(|scan| scan.attention.iter()))
        .chain(report.declared.attention.iter())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct RecordingIdentification {
        calls: RefCell<Vec<String>>,
        diagnostics: Vec<DiscoveryDiagnostic>,
    }
    impl Identification for RecordingIdentification {
        fn known(&self, system: &str) -> bool {
            system == "gb"
        }
        fn source(
            &self,
            source: &SourceInput<'_>,
            _: &AttentionDecisions,
            _: &mut HashCache,
            _: &TraversalBudget,
        ) -> SourceScan {
            assert_eq!(source.excluded_roots.len(), 2);
            self.calls.borrow_mut().push(source.source_id.into());
            SourceScan {
                source_id: source.source_id.into(),
                diagnostics: self.diagnostics.clone(),
                ..SourceScan::default()
            }
        }
        fn declared(&self, accounts: &[AccountRoot], _: &mut HashCache) -> DeclaredScan {
            self.calls
                .borrow_mut()
                .extend(accounts.iter().map(|account| account.storage_id.clone()));
            DeclaredScan::default()
        }
    }
    fn source(id: &str, root: PathBuf, system: &str) -> SourceView {
        SourceView {
            source_id: id.into(),
            storage_id: id.into(),
            title: None,
            system: system.into(),
            canonical_root: root.canonicalize().ok(),
            root,
            scans_files: true,
        }
    }
    #[test]
    fn skips_both_overlapping_sources_and_unknown_system_but_checks_declared_accounts() {
        let root = tempfile::tempdir().unwrap();
        for path in ["a/inner", "b", "c", "d"] {
            std::fs::create_dir_all(root.path().join(path)).unwrap();
        }
        let sources = vec![
            source("a", root.path().join("a"), "gb"),
            source("nested", root.path().join("a/inner"), "gb"),
            source("unknown", root.path().join("b"), "missing"),
            source("ready", root.path().join("c"), "gb"),
        ];
        let identify = RecordingIdentification::default();
        let accounts = vec![AccountRoot {
            user: "default".into(),
            storage_id: "account-default".into(),
            root: root.path().join("users/default"),
        }];
        let report = scan_with(
            &sources,
            &accounts,
            &[root.path().join("users"), root.path().join("private")],
            &identify,
            &AttentionDecisions::default(),
            &mut HashCache::default(),
            &TraversalBudget::default(),
        );
        assert_eq!(report.skipped, ["a", "nested", "unknown"]);
        assert_eq!(
            report
                .attention
                .iter()
                .map(|item| item.kind)
                .collect::<Vec<_>>(),
            [
                AttentionKind::Overlap,
                AttentionKind::Overlap,
                AttentionKind::SystemUnavailable
            ]
        );
        assert_eq!(*identify.calls.borrow(), ["ready", "account-default"]);
        assert_eq!(report.accounts, accounts);
        assert!(report.paths.is_empty());
    }
    #[test]
    fn truncated_or_partly_unreadable_sources_are_skipped_to_preserve_unseen_locations() {
        let root = tempfile::tempdir().unwrap();
        let games = root.path().join("games");
        std::fs::create_dir(&games).unwrap();
        for code in [
            DiscoveryDiagnosticCode::TraversalLimitReached,
            DiscoveryDiagnosticCode::EntryUnavailable,
        ] {
            let diagnostic = DiscoveryDiagnostic {
                code,
                storage_id: Some("gb".into()),
                path: None,
                message: "walk incomplete".into(),
            };
            let identify = RecordingIdentification {
                diagnostics: vec![diagnostic.clone()],
                ..RecordingIdentification::default()
            };
            let report = scan_with(
                &[source("gb", games.clone(), "gb")],
                &[],
                &[root.path().join("users"), root.path().join("private")],
                &identify,
                &AttentionDecisions::default(),
                &mut HashCache::default(),
                &TraversalBudget::default(),
            );
            assert_eq!(report.skipped, ["gb"]);
            assert!(report.scanned.is_empty());
            assert_eq!(report.diagnostics, [diagnostic]);
        }
    }

    #[test]
    fn hand_edited_plugin_data_source_is_never_traversed() {
        let root = tempfile::tempdir().unwrap();
        let identify = RecordingIdentification::default();
        let report = scan_with(
            &[source("bad", root.path().into(), "gb")],
            &[],
            &[root.path().join("users"), root.path().join("private")],
            &identify,
            &AttentionDecisions::default(),
            &mut HashCache::default(),
            &TraversalBudget::default(),
        );
        assert_eq!(report.skipped, ["bad"]);
        assert!(identify.calls.borrow().is_empty());
    }
}
