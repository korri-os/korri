use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{discovery::title, plugin::PluginRegistry};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanCandidate {
    pub storage_id: String,
    pub canonical_path: PathBuf,
    pub relative_path: String,
    pub title: String,
    pub hash: String,
    pub size: u64,
    pub claim_id: String,
    pub system: String,
    pub launcher: String,
    pub runtime: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanReport {
    pub candidates: Vec<ScanCandidate>,
    pub diagnostics: Vec<DiscoveryDiagnostic>,
    pub hashed_bytes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HashCache {
    #[serde(default)]
    entries: BTreeMap<String, HashCacheEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct HashCacheEntry {
    size: u64,
    modified_nanos: Option<u128>,
    hash: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HashFileSnapshot {
    size: u64,
    modified_nanos: Option<u128>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HashOutcome {
    value: String,
    read_bytes: u64,
    snapshot: HashFileSnapshot,
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

#[derive(Default)]
struct TraversalState {
    entries: usize,
    directories: usize,
    traversal_exhausted: bool,
}

pub struct FolderScanner<'a> {
    registry: &'a PluginRegistry,
    max_diagnostics: usize,
    max_candidates: usize,
    budget: TraversalBudget,
}

impl<'a> FolderScanner<'a> {
    pub fn new(
        registry: &'a PluginRegistry,
        max_diagnostics: usize,
        max_candidates: usize,
    ) -> Self {
        Self::with_budget(
            registry,
            max_diagnostics,
            max_candidates,
            TraversalBudget::default(),
        )
    }

    pub fn with_budget(
        registry: &'a PluginRegistry,
        max_diagnostics: usize,
        max_candidates: usize,
        budget: TraversalBudget,
    ) -> Self {
        Self {
            registry,
            max_diagnostics,
            max_candidates,
            budget,
        }
    }

    pub fn scan(&self, storages: &[(String, PathBuf)], cache: &mut HashCache) -> ScanReport {
        let mut report = ScanReport::default();
        let mut state = TraversalState::default();
        for (storage_id, root) in storages {
            let canonical_root = match root.canonicalize() {
                Ok(path) => path,
                Err(_) => {
                    self.push_diag(
                        &mut report,
                        DiscoveryDiagnosticCode::StorageUnavailable,
                        Some(storage_id),
                        None,
                        "selected folder is unavailable",
                    );
                    continue;
                }
            };
            match fs::metadata(&canonical_root) {
                Ok(metadata) if metadata.is_dir() => {}
                _ => {
                    self.push_diag(
                        &mut report,
                        DiscoveryDiagnosticCode::StorageUnavailable,
                        Some(storage_id),
                        None,
                        "selected folder is not readable",
                    );
                    continue;
                }
            }
            self.scan_directory(
                storage_id,
                &canonical_root,
                &canonical_root,
                0,
                cache,
                &mut report,
                &mut state,
            );
        }
        report
    }

    fn scan_directory(
        &self,
        storage_id: &str,
        root: &Path,
        directory: &Path,
        depth: usize,
        cache: &mut HashCache,
        report: &mut ScanReport,
        state: &mut TraversalState,
    ) {
        if state.traversal_exhausted {
            return;
        }
        if depth > self.budget.max_depth {
            self.push_traversal_exhausted(report, Some(storage_id), safe_relative(root, directory));
            state.traversal_exhausted = true;
            return;
        }
        if state.directories >= self.budget.max_directories {
            self.push_traversal_exhausted(report, Some(storage_id), safe_relative(root, directory));
            state.traversal_exhausted = true;
            return;
        }
        state.directories += 1;
        let read_dir = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => {
                self.push_diag(
                    report,
                    DiscoveryDiagnosticCode::EntryUnavailable,
                    Some(storage_id),
                    safe_relative(root, directory),
                    "folder entry is unreadable",
                );
                return;
            }
        };
        let mut entries = Vec::new();
        for entry in read_dir {
            if entries.len() >= self.budget.max_sortable_entries {
                self.push_traversal_exhausted(
                    report,
                    Some(storage_id),
                    safe_relative(root, directory),
                );
                state.traversal_exhausted = true;
                return;
            }
            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => self.push_diag(
                    report,
                    DiscoveryDiagnosticCode::EntryUnavailable,
                    Some(storage_id),
                    safe_relative(root, directory),
                    "folder entry is unreadable",
                ),
            }
        }
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            if state.traversal_exhausted {
                return;
            }
            if state.entries >= self.budget.max_entries {
                self.push_traversal_exhausted(
                    report,
                    Some(storage_id),
                    safe_relative(root, directory),
                );
                state.traversal_exhausted = true;
                return;
            }
            state.entries += 1;
            let original_path = entry.path();
            let canonical = match original_path.canonicalize() {
                Ok(path) => path,
                Err(_) => {
                    self.push_diag(
                        report,
                        DiscoveryDiagnosticCode::EntryUnavailable,
                        Some(storage_id),
                        safe_relative(root, &original_path),
                        "folder entry disappeared or is unavailable",
                    );
                    continue;
                }
            };
            if !is_strict_descendant(&canonical, root) {
                self.push_diag(
                    report,
                    DiscoveryDiagnosticCode::EntryEscaped,
                    Some(storage_id),
                    safe_relative(root, &original_path),
                    "folder entry escapes the selected folder",
                );
                continue;
            }
            let metadata = match fs::metadata(&canonical) {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.push_diag(
                        report,
                        DiscoveryDiagnosticCode::EntryUnavailable,
                        Some(storage_id),
                        safe_relative(root, &original_path),
                        "folder entry is unavailable",
                    );
                    continue;
                }
            };
            if metadata.is_dir() {
                self.scan_directory(
                    storage_id,
                    root,
                    &canonical,
                    depth + 1,
                    cache,
                    report,
                    state,
                );
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            self.scan_file(storage_id, root, &canonical, cache, report);
        }
    }

    fn scan_file(
        &self,
        storage_id: &str,
        root: &Path,
        path: &Path,
        cache: &mut HashCache,
        report: &mut ScanReport,
    ) {
        let relative_path = match relative_path(root, path) {
            Some(path) => path,
            None => {
                self.push_diag(
                    report,
                    DiscoveryDiagnosticCode::PathUnsupported,
                    Some(storage_id),
                    None,
                    "file path is not representable in the library",
                );
                return;
            }
        };
        let Some(file_name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            self.push_diag(
                report,
                DiscoveryDiagnosticCode::PathUnsupported,
                Some(storage_id),
                Some(relative_path),
                "file name is not representable in the library",
            );
            return;
        };
        let Some(extension) = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_owned)
        else {
            self.push_diag(
                report,
                DiscoveryDiagnosticCode::EntryUnclaimed,
                Some(storage_id),
                Some(relative_path),
                "file is not claimed by an enabled plugin",
            );
            return;
        };
        let claims = self
            .registry
            .file_release_discovery_claims_for_extension(&extension);
        if claims.is_empty() {
            self.push_diag(
                report,
                DiscoveryDiagnosticCode::EntryUnclaimed,
                Some(storage_id),
                Some(relative_path),
                "file is not claimed by an enabled plugin",
            );
            return;
        }
        if claims.len() > 1 {
            self.push_diag(
                report,
                DiscoveryDiagnosticCode::ClaimConflict,
                Some(storage_id),
                Some(relative_path),
                "multiple enabled plugins claim this file",
            );
            return;
        }
        if report.candidates.len() >= self.max_candidates {
            self.push_diag(
                report,
                DiscoveryDiagnosticCode::DiagnosticLimitReached,
                Some(storage_id),
                Some(relative_path),
                "additional discovery candidates were omitted",
            );
            return;
        }
        let claim = claims[0];
        let hash = match cache.hash_for(path) {
            Ok(value) => value,
            Err(_) => {
                self.push_diag(
                    report,
                    DiscoveryDiagnosticCode::HashUnavailable,
                    Some(storage_id),
                    Some(relative_path),
                    "file could not be read for identity hashing",
                );
                return;
            }
        };
        report.hashed_bytes += hash.read_bytes;
        report.candidates.push(ScanCandidate {
            storage_id: storage_id.to_owned(),
            canonical_path: path.to_owned(),
            relative_path,
            title: title::fallback_title(&file_name, &extension),
            hash: hash.value,
            size: hash.snapshot.size,
            claim_id: claim.id.clone(),
            system: claim.system.clone(),
            launcher: claim.launcher.clone(),
            runtime: claim.runtime.clone(),
        });
    }

    fn push_traversal_exhausted(
        &self,
        report: &mut ScanReport,
        storage_id: Option<&str>,
        path: Option<String>,
    ) {
        if report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiscoveryDiagnosticCode::TraversalLimitReached)
        {
            return;
        }
        self.push_diag(
            report,
            DiscoveryDiagnosticCode::TraversalLimitReached,
            storage_id,
            path,
            "discovery traversal budget was exhausted; additional entries were omitted",
        );
    }

    fn push_diag(
        &self,
        report: &mut ScanReport,
        code: DiscoveryDiagnosticCode,
        storage_id: Option<&str>,
        path: Option<String>,
        message: &str,
    ) {
        if report.diagnostics.len() >= self.max_diagnostics {
            if !report
                .diagnostics
                .iter()
                .any(|d| d.code == DiscoveryDiagnosticCode::DiagnosticLimitReached)
            {
                report.diagnostics.push(DiscoveryDiagnostic {
                    code: DiscoveryDiagnosticCode::DiagnosticLimitReached,
                    storage_id: None,
                    path: None,
                    message: "additional discovery diagnostics were omitted".into(),
                });
            }
            return;
        }
        report.diagnostics.push(DiscoveryDiagnostic {
            code,
            storage_id: storage_id.map(str::to_owned),
            path,
            message: message.into(),
        });
    }
}

impl HashCache {
    fn hash_for(&mut self, path: &Path) -> Result<HashOutcome, std::io::Error> {
        let snapshot = hash_file_snapshot(path)?;
        let key = path.to_string_lossy().into_owned();
        if let Some(entry) = self.entries.get(&key) {
            if entry.size == snapshot.size && entry.modified_nanos == snapshot.modified_nanos {
                return Ok(HashOutcome {
                    value: entry.hash.clone(),
                    read_bytes: 0,
                    snapshot,
                });
            }
        }
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            total += read as u64;
        }
        let hash = format!("sha256:{}", hex::encode(hasher.finalize()));
        self.entries.insert(
            key,
            HashCacheEntry {
                size: snapshot.size,
                modified_nanos: snapshot.modified_nanos,
                hash: hash.clone(),
            },
        );
        Ok(HashOutcome {
            value: hash,
            read_bytes: total,
            snapshot,
        })
    }
}

fn hash_file_snapshot(path: &Path) -> Result<HashFileSnapshot, std::io::Error> {
    let metadata = fs::metadata(path)?;
    Ok(HashFileSnapshot {
        size: metadata.len(),
        modified_nanos: metadata.modified().ok().and_then(|time| {
            time.duration_since(UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_nanos())
        }),
    })
}

fn safe_relative(root: &Path, path: &Path) -> Option<String> {
    relative_path(root, path).or_else(|| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    })
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let std::path::Component::Normal(part) = component else {
            return None;
        };
        parts.push(part.to_str()?.to_owned());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn is_strict_descendant(path: &Path, root: &Path) -> bool {
    path.parent()
        .is_some_and(|parent| parent == root || parent.starts_with(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        plugin::load_plugin_source,
        plugin_policy::{
            bundled_plugin_policy_layer, bundled_plugins, resolve_enabled_plugin_ids,
            ANDROID_APP_PLUGIN_SOURCE, MGBA_PLUGIN_SOURCE, RETROARCH_PLUGIN_SOURCE,
        },
    };

    fn registry() -> PluginRegistry {
        PluginRegistry::new(
            bundled_plugins().unwrap(),
            resolve_enabled_plugin_ids([bundled_plugin_policy_layer()]),
        )
        .unwrap()
    }

    #[test]
    fn claims_uppercase_gba_and_reuses_hash_cache_for_unchanged_files() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Wario_Land_4 (USA).GBA"), b"rom-bytes").unwrap();
        let mut cache = HashCache::default();
        let registry = registry();
        let scanner = FolderScanner::new(&registry, 20, 100);

        let first = scanner.scan(&[("selected".into(), root.path().to_owned())], &mut cache);
        assert_eq!(first.candidates.len(), 1);
        assert_eq!(first.candidates[0].title, "Wario Land 4");
        assert_eq!(first.hashed_bytes, 9);

        let second = scanner.scan(&[("selected".into(), root.path().to_owned())], &mut cache);
        assert_eq!(second.candidates.len(), 1);
        assert_eq!(second.hashed_bytes, 0);
    }

    #[test]
    fn conflicting_claims_are_diagnostics_not_candidates() {
        let conflict = r#"

export const name = "conflict";
export const title = "Conflicting GBA Claim";
export const discovery = {
      fileReleases: {
        gba: {
          id: "@test:conflict/gba",
          extensions: ["gba"],
          system: "gba",
          launcher: "@korri:retroarch/retroarch",
          runtime: "@korri:mgba/mgba",
        },
      },
    };
"#;
        let registry = PluginRegistry::new(
            vec![
                load_plugin_source("@korri", ANDROID_APP_PLUGIN_SOURCE).unwrap(),
                load_plugin_source("@korri", MGBA_PLUGIN_SOURCE).unwrap(),
                load_plugin_source("@korri", RETROARCH_PLUGIN_SOURCE).unwrap(),
                load_plugin_source("@test", conflict).unwrap(),
            ],
            vec![
                "@korri:android-app".into(),
                "@korri:mgba".into(),
                "@korri:retroarch".into(),
                "@test:conflict".into(),
            ],
        )
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("game.gba"), b"rom").unwrap();
        let mut cache = HashCache::default();
        let scanner = FolderScanner::new(&registry, 20, 100);

        let report = scanner.scan(&[("selected".into(), root.path().to_owned())], &mut cache);
        assert!(report.candidates.is_empty());
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiscoveryDiagnosticCode::ClaimConflict));
    }

    #[cfg(unix)]
    #[test]
    fn diagnoses_unreadable_directories_without_aborting_siblings() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("ok.gba"), b"rom").unwrap();
        fs::create_dir(root.path().join("closed")).unwrap();
        fs::set_permissions(
            root.path().join("closed"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        let registry = registry();
        let mut cache = HashCache::default();
        let scanner = FolderScanner::new(&registry, 20, 100);

        let report = scanner.scan(&[("selected".into(), root.path().to_owned())], &mut cache);
        let _ = fs::set_permissions(
            root.path().join("closed"),
            fs::Permissions::from_mode(0o700),
        );
        assert_eq!(report.candidates.len(), 1);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiscoveryDiagnosticCode::EntryUnavailable));
    }

    #[cfg(unix)]
    #[test]
    fn diagnoses_symlink_escapes_without_hashing_target() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("escape.gba"), b"secret").unwrap();
        symlink(
            outside.path().join("escape.gba"),
            root.path().join("escape.gba"),
        )
        .unwrap();
        let mut cache = HashCache::default();
        let registry = registry();
        let scanner = FolderScanner::new(&registry, 20, 100);

        let report = scanner.scan(&[("selected".into(), root.path().to_owned())], &mut cache);
        assert!(report.candidates.is_empty());
        assert_eq!(report.hashed_bytes, 0);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == DiscoveryDiagnosticCode::EntryEscaped
                && !d.message.contains(outside.path().to_str().unwrap())));
    }
}
