use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use super::{SourceInput, SourceScan};
use crate::discovery::scanner::{DiscoveryDiagnostic, DiscoveryDiagnosticCode, TraversalBudget};

pub(super) fn diagnostic(
    scan: &mut SourceScan,
    source: &SourceInput<'_>,
    code: DiscoveryDiagnosticCode,
    path: Option<String>,
    message: &str,
) {
    scan.diagnostics.push(DiscoveryDiagnostic {
        code,
        storage_id: Some(source.storage_id.to_owned()),
        path,
        message: message.to_owned(),
    });
}

pub(super) fn relative(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root).ok()?.to_str().map(str::to_owned)
}

#[derive(Default)]
pub(super) struct FileLayout {
    pub files: BTreeSet<PathBuf>,
    pub directories: BTreeSet<PathBuf>,
    pub entries: BTreeSet<PathBuf>,
}

pub(super) fn layout(
    source: &SourceInput<'_>,
    root: &Path,
    budget: &TraversalBudget,
    scan: &mut SourceScan,
) -> FileLayout {
    let mut walker = Walker {
        source,
        root,
        budget,
        scan,
        entries: 0,
        directories: 0,
        exhausted: false,
        visited: BTreeSet::new(),
        files: BTreeSet::new(),
        paths: BTreeSet::new(),
    };
    walker.walk(root, 0);
    FileLayout {
        files: walker.files,
        directories: walker.visited,
        entries: walker.paths,
    }
}

struct Walker<'a, 's> {
    source: &'a SourceInput<'s>,
    root: &'a Path,
    budget: &'a TraversalBudget,
    scan: &'a mut SourceScan,
    entries: usize,
    directories: usize,
    exhausted: bool,
    visited: BTreeSet<PathBuf>,
    files: BTreeSet<PathBuf>,
    paths: BTreeSet<PathBuf>,
}

impl Walker<'_, '_> {
    fn limit(&mut self) {
        if !self.exhausted {
            diagnostic(
                self.scan,
                self.source,
                DiscoveryDiagnosticCode::TraversalLimitReached,
                None,
                "discovery traversal budget was exhausted; additional entries were omitted",
            );
        }
        self.exhausted = true;
    }

    fn walk(&mut self, directory: &Path, depth: usize) {
        if self.exhausted
            || self
                .source
                .excluded_roots
                .iter()
                .any(|root| directory.starts_with(root))
        {
            return;
        }
        if depth > self.budget.max_depth || self.directories >= self.budget.max_directories {
            self.limit();
            return;
        }
        let Ok(canonical) = directory.canonicalize() else {
            return;
        };
        if !self.visited.insert(canonical) {
            return;
        }
        self.directories += 1;
        let read_dir = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => {
                diagnostic(
                    self.scan,
                    self.source,
                    DiscoveryDiagnosticCode::EntryUnavailable,
                    relative(self.root, directory),
                    "folder entry is unreadable",
                );
                return;
            }
        };
        let mut entries = Vec::new();
        for entry in read_dir {
            if entries.len() >= self.budget.max_sortable_entries {
                self.limit();
                return;
            }
            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => diagnostic(
                    self.scan,
                    self.source,
                    DiscoveryDiagnosticCode::EntryUnavailable,
                    relative(self.root, directory),
                    "folder entry is unreadable",
                ),
            }
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if self.exhausted {
                return;
            }
            if self.entries >= self.budget.max_entries {
                self.limit();
                return;
            }
            self.entries += 1;
            let path = entry.path();
            self.paths.insert(path.clone());
            let canonical = match path.canonicalize() {
                Ok(path) => path,
                Err(_) => {
                    diagnostic(
                        self.scan,
                        self.source,
                        DiscoveryDiagnosticCode::EntryUnavailable,
                        relative(self.root, &path),
                        "folder entry is unavailable",
                    );
                    continue;
                }
            };
            if !canonical.starts_with(self.root) {
                diagnostic(
                    self.scan,
                    self.source,
                    DiscoveryDiagnosticCode::EntryEscaped,
                    relative(self.root, &path),
                    "folder entry escapes the source",
                );
                continue;
            }
            if self
                .source
                .excluded_roots
                .iter()
                .any(|root| canonical.starts_with(root))
            {
                continue;
            }
            if canonical.is_dir() {
                // Do not traverse directory symlinks: this also prevents cycles
                // and avoids choosing between aliases of the same subtree.
                if !entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
                    self.walk(&path, depth + 1);
                }
            } else if canonical.is_file() {
                if relative(self.root, &path).is_some() {
                    self.files.insert(path);
                } else {
                    diagnostic(
                        self.scan,
                        self.source,
                        DiscoveryDiagnosticCode::PathUnsupported,
                        None,
                        "file path is not UTF-8",
                    );
                }
            }
        }
    }
}

/// The future walk counts existing empty or skipped entries and incoming parents.
pub(super) fn planned_within_budget(
    files: &BTreeSet<PathBuf>,
    existing: &BTreeSet<PathBuf>,
    entries: &BTreeSet<PathBuf>,
    root: &Path,
    budget: &TraversalBudget,
) -> bool {
    let mut directories = existing.clone();
    directories.insert(root.to_owned());
    let mut entries = entries.clone();
    for file in files {
        let mut child = file.as_path();
        while let Some(parent) = child.parent() {
            if !parent.starts_with(root) {
                break;
            }
            directories.insert(parent.to_owned());
            child = parent;
        }
    }
    entries.extend(
        directories
            .iter()
            .filter(|directory| directory.as_path() != root)
            .cloned(),
    );
    let mut children = std::collections::BTreeMap::<PathBuf, usize>::new();
    for entry in &entries {
        if let Some(parent) = entry.parent() {
            *children.entry(parent.to_owned()).or_default() += 1;
        }
    }
    entries.len() <= budget.max_entries
        && directories.len() <= budget.max_directories
        && directories.iter().all(|directory| {
            directory
                .strip_prefix(root)
                .is_ok_and(|relative| relative.components().count() <= budget.max_depth)
        })
        && children
            .values()
            .all(|count| *count <= budget.max_sortable_entries)
}
