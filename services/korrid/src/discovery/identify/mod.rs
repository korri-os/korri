//! Layout, evidence and content identity for explicitly paired sources.

mod disc;
mod evidence;
mod hash;
mod layout;
mod traversal;
pub mod types;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub use evidence::level as evidence_level;
pub use hash::HashCache;
pub use types::*;

use crate::{
    discovery::{
        attention::{AttentionDecisions, AttentionFinding, AttentionKind},
        scanner::{DiscoveryDiagnosticCode, TraversalBudget},
        title,
    },
    plugin::{ContentRules, DeclaredGame, PairingTarget, PluginRegistry},
};
use disc::{Identified, MappedFiles, ReadScope};
use layout::Captures;

pub fn identify_source(
    source: &SourceInput<'_>,
    registry: &PluginRegistry,
    decisions: &AttentionDecisions,
    cache: &mut HashCache,
    budget: &TraversalBudget,
) -> SourceScan {
    let (members, group) = match registry.pairing_target(source.system) {
        Some(PairingTarget::System(rules)) => (vec![rules], false),
        Some(PairingTarget::Group { members, .. }) => (members, true),
        None => {
            let mut scan = SourceScan {
                source_id: source.source_id.into(),
                ..SourceScan::default()
            };
            scan.attention.push(finding(
                source.source_id,
                None,
                AttentionKind::SystemUnavailable,
                None,
                vec![],
                "no enabled plugin declares the paired system or group",
            ));
            return scan;
        }
    };
    let pairing = crate::discovery::sources::pairing_key(source.system, source.root);
    identify_with_rules(
        source,
        &members,
        group,
        &|source, path, identity| {
            decisions
                .assigned_system(source, path, identity, &pairing)
                .map(str::to_owned)
        },
        &|hash| {
            registry
                .declared_hash(hash)
                .iter()
                .filter_map(|release| release.release.system.clone())
                .collect()
        },
        cache,
        budget,
    )
}

/// Core seam for registry-independent consumers and tests. `declared_systems`
/// returns the systems of releases declaring a prefixed digest. Assignment
/// lookup must validate source, path and identity before returning a system.
#[allow(clippy::too_many_arguments)]
pub fn identify_with_rules(
    source: &SourceInput<'_>,
    members: &[&ContentRules],
    group: bool,
    assigned: &dyn Fn(&str, &str, &str) -> Option<String>,
    declared_systems: &dyn Fn(&str) -> Vec<String>,
    cache: &mut HashCache,
    budget: &TraversalBudget,
) -> SourceScan {
    let mut scan = SourceScan {
        source_id: source.source_id.into(),
        ..SourceScan::default()
    };
    let root = match source.root.canonicalize() {
        Ok(root) if root.is_dir() => root,
        _ => {
            traversal::diagnostic(
                &mut scan,
                source,
                DiscoveryDiagnosticCode::StorageUnavailable,
                None,
                "source folder is unavailable",
            );
            return scan;
        }
    };
    let files = traversal::layout(source, &root, budget, &mut scan).files;
    identify_files(
        source,
        members,
        group,
        assigned,
        declared_systems,
        cache,
        budget,
        &root,
        &files,
        None,
        scan,
    )
}

/// A read-only projection of file moves. Existing files and incoming files share
/// the normal layout, descriptor, evidence and multi-disc identification path.
#[allow(clippy::too_many_arguments)]
pub fn identify_planned_with_rules(
    source: &SourceInput<'_>,
    members: &[&ContentRules],
    group: bool,
    assigned: &dyn Fn(&str, &str, &str) -> Option<String>,
    declared_systems: &dyn Fn(&str) -> Vec<String>,
    changes: &BTreeMap<PathBuf, Option<PathBuf>>,
    cache: &mut HashCache,
    budget: &TraversalBudget,
) -> SourceScan {
    let mut scan = SourceScan {
        source_id: source.source_id.into(),
        ..Default::default()
    };
    let layout = if source.root.exists() {
        traversal::layout(source, source.root, budget, &mut scan)
    } else {
        traversal::FileLayout::default()
    };
    let mut mapped: BTreeMap<_, _> = layout
        .files
        .into_iter()
        .map(|path| (path.clone(), path))
        .collect();
    let mut entries = layout.entries;
    for (path, physical) in changes
        .iter()
        .filter(|(path, _)| path.starts_with(source.root))
    {
        match physical {
            Some(physical) => {
                mapped.insert(path.clone(), physical.clone());
                entries.insert(path.clone());
            }
            None => {
                mapped.remove(path);
                entries.remove(path);
            }
        }
    }
    mapped.retain(|path, _| {
        if traversal::relative(source.root, path).is_some() {
            true
        } else {
            traversal::diagnostic(
                &mut scan,
                source,
                DiscoveryDiagnosticCode::PathUnsupported,
                None,
                "file path is not UTF-8",
            );
            false
        }
    });
    let files = mapped.keys().cloned().collect();
    if !traversal::planned_within_budget(&files, &layout.directories, &entries, source.root, budget)
    {
        traversal::diagnostic(
            &mut scan,
            source,
            DiscoveryDiagnosticCode::TraversalLimitReached,
            None,
            "planned source exceeds the traversal budget",
        );
        return scan;
    }
    identify_files(
        source,
        members,
        group,
        assigned,
        declared_systems,
        cache,
        budget,
        source.root,
        &files,
        Some(MappedFiles {
            paths: &mapped,
            physical_root: None,
        }),
        scan,
    )
}

#[allow(clippy::too_many_arguments)]
fn identify_files(
    source: &SourceInput<'_>,
    members: &[&ContentRules],
    group: bool,
    assigned: &dyn Fn(&str, &str, &str) -> Option<String>,
    declared_systems: &dyn Fn(&str) -> Vec<String>,
    cache: &mut HashCache,
    budget: &TraversalBudget,
    root: &Path,
    files: &BTreeSet<PathBuf>,
    mapped: Option<MappedFiles<'_>>,
    mut scan: SourceScan,
) -> SourceScan {
    let before = cache.read_bytes;
    scan.seen_paths = files
        .iter()
        .filter_map(|path| traversal::relative(root, path))
        .collect();
    let scope = ReadScope {
        mapped,
        root,
        excluded: source.excluded_roots,
        allowed: Some(files),
        max_depth: TraversalBudget::default().max_depth,
        max_references: budget.max_entries,
    };
    let mut ordered: Vec<_> = files.iter().collect();
    // Playlists own discs, cues own tracks. Always visit owners before their
    // possible members, irrespective of their filename order.
    ordered.sort_by_key(|path| {
        (
            match disc::kind(path).as_str() {
                "m3u" => 0,
                "cue" => 1,
                _ => 2,
            },
            *path,
        )
    });
    let mut identified = Vec::new();
    let mut consumed = BTreeSet::new();
    for path in ordered {
        let relative = traversal::relative(root, path).unwrap();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if consumed.contains(path) {
            continue;
        }
        if !members
            .iter()
            .any(|rules| layout::extension(name, rules).is_some())
        {
            continue;
        }
        let mut candidates = Vec::new();
        let mut errors = Vec::new();
        // One bounded read shared by all members' evidence checks.
        let prefix = if group {
            disc::checked_path(path, &scope)
                .and_then(|path| hash::prefix(&path))
                .ok()
        } else {
            None
        };
        for rules in members {
            let Some(ext) = layout::extension(name, rules) else {
                continue;
            };
            let Some(captures) = layout::match_layout(&relative, ext, rules) else {
                continue;
            };
            match disc::identify(path, rules, cache, &scope) {
                Ok(content) => {
                    consumed.extend(content.consumed.iter().cloned());
                    let evidence = if group {
                        evidence::level(
                            name,
                            prefix.as_deref().unwrap_or_default(),
                            rules,
                            &hash::hashes(&content.identity),
                            declared_systems,
                        )
                    } else {
                        EvidenceLevel::Extension
                    };
                    candidates.push(Candidate {
                        rules,
                        captures,
                        content,
                        evidence,
                        extension: ext.to_owned(),
                    });
                }
                Err(error) => errors.push(error),
            }
        }
        candidates.sort_by_key(|candidate| &candidate.rules.system);
        let selected = candidates.iter().find_map(|candidate| {
            let system = assigned(
                source.source_id,
                &relative,
                candidate.content.identity.value(),
            )?;
            candidates
                .iter()
                .position(|candidate| candidate.rules.system == system)
        });
        let selected = if let Some(index) = selected {
            candidates[index].evidence = EvidenceLevel::Assigned;
            Some(index)
        } else {
            let best = candidates.iter().map(|candidate| candidate.evidence).max();
            let winners: Vec<_> = candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| Some(candidate.evidence) == best)
                .map(|(index, _)| index)
                .collect();
            match winners.as_slice() {
                [winner] => Some(*winner),
                [] => {
                    let (kind, message) = if errors.is_empty() {
                        (
                            AttentionKind::Unmatched,
                            "claimed file matches no layout".to_owned(),
                        )
                    } else {
                        (AttentionKind::Unreadable, errors[0].to_string())
                    };
                    // An unmatched file can still carry an identity, so an
                    // ignore decision expires when its content changes.
                    let identity = if kind == AttentionKind::Unmatched {
                        members
                            .iter()
                            .filter(|rules| layout::extension(name, rules).is_some())
                            .min_by_key(|rules| &rules.system)
                            .and_then(|rules| disc::identify(path, rules, cache, &scope).ok())
                            .map(|content| {
                                consumed.extend(content.consumed);
                                content.identity
                            })
                    } else {
                        None
                    };
                    scan.attention.push(finding(
                        source.source_id,
                        Some(&relative),
                        kind,
                        identity.as_ref().map(ContentIdentity::value),
                        vec![],
                        &message,
                    ));
                    None
                }
                _ => {
                    scan.attention.push(finding(
                        source.source_id,
                        Some(&relative),
                        AttentionKind::Clash,
                        Some(candidates[winners[0]].content.identity.value()),
                        winners
                            .iter()
                            .map(|index| candidates[*index].rules.system.clone())
                            .collect(),
                        "several systems have the same best evidence",
                    ));
                    None
                }
            }
        };
        if let Some(index) = selected {
            let candidate = candidates.swap_remove(index);
            let unit = FoundUnit {
                source_id: source.source_id.into(),
                storage_id: source.storage_id.into(),
                path: relative,
                parts: candidate
                    .content
                    .parts
                    .iter()
                    .filter_map(|path| traversal::relative(root, path))
                    .collect(),
                tracks: candidate
                    .content
                    .tracks
                    .iter()
                    .filter_map(|path| traversal::relative(root, path))
                    .collect(),
                system: candidate.rules.system.clone(),
                title: candidate
                    .captures
                    .title
                    .clone()
                    .unwrap_or_else(|| title::fallback_title(name, &candidate.extension)),
                version: candidate.captures.version.clone(),
                region: candidate.captures.region.clone(),
                identity: candidate.content.identity,
                evidence: candidate.evidence,
            };
            identified.push((path.clone(), candidate.captures.disc, unit));
        }
    }
    // Remove every consumed descriptor/track even if it sorts before its owner.
    scan.attention.retain(|item| {
        item.path
            .as_ref()
            .is_none_or(|path| !consumed.contains(&root.join(path)))
    });
    identified.retain(|(path, _, _)| !consumed.contains(path));
    scan.units = combine_discs(identified);
    scan.units.sort_by(|a, b| a.path.cmp(&b.path));
    scan.hashed_bytes = cache.read_bytes - before;
    scan
}

struct Candidate<'a> {
    rules: &'a ContentRules,
    captures: Captures,
    content: Identified,
    evidence: EvidenceLevel,
    extension: String,
}

type DiscKey = (String, String, String, Option<String>, Option<String>);

fn combine_discs(identified: Vec<(PathBuf, Option<String>, FoundUnit)>) -> Vec<FoundUnit> {
    let mut units = Vec::new();
    let mut sets: BTreeMap<DiscKey, Vec<(String, FoundUnit)>> = BTreeMap::new();
    for (_, disc, unit) in identified {
        if let Some(disc) = disc {
            let folder = unit
                .path
                .rsplit_once('/')
                .map_or("", |(folder, _)| folder)
                .to_owned();
            sets.entry((
                folder,
                unit.system.clone(),
                unit.title.clone(),
                unit.version.clone(),
                unit.region.clone(),
            ))
            .or_default()
            .push((disc, unit));
        } else {
            units.push(unit);
        }
    }
    for mut discs in sets.into_values() {
        let numeric = discs.iter().all(|(disc, _)| disc.parse::<u64>().is_ok());
        discs.sort_by(|(a, ua), (b, ub)| {
            let order = if numeric {
                a.parse::<u64>().unwrap().cmp(&b.parse::<u64>().unwrap())
            } else {
                a.cmp(b)
            };
            order.then_with(|| ua.path.cmp(&ub.path))
        });
        let identity = hash::parts_identity(
            discs
                .iter()
                .map(|(_, unit)| unit.identity.value().to_owned())
                .collect(),
        );
        let primary = &discs[0].1.path;
        let parts = discs
            .iter()
            .flat_map(|(_, unit)| {
                let descriptor = unit.path.to_ascii_lowercase().ends_with(".cue")
                    || unit.path.to_ascii_lowercase().ends_with(".m3u");
                std::iter::once(unit.path.clone())
                    .filter(move |_| descriptor && &unit.path != primary)
                    .chain(unit.parts.iter().cloned())
            })
            .collect();
        let tracks = discs
            .iter()
            .flat_map(|(_, unit)| unit.tracks.clone())
            .collect();
        let mut unit = discs.remove(0).1;
        unit.tracks = tracks;
        unit.identity = identity;
        unit.parts = parts;
        units.push(unit);
    }
    units
}

pub fn resolve_declared(
    accounts: &[AccountRoot],
    registry: &PluginRegistry,
    cache: &mut HashCache,
) -> DeclaredScan {
    resolve_declared_games(accounts, registry.declared_games(), cache)
}

/// Registry-independent declared-file resolution. Files are always hashed as
/// ordinary bytes, without system headers or descriptor interpretation.
pub fn resolve_declared_games(
    accounts: &[AccountRoot],
    games: &BTreeMap<String, DeclaredGame>,
    cache: &mut HashCache,
) -> DeclaredScan {
    resolve_declared_files(accounts, games, cache, None)
}

pub fn resolve_declared_planned(
    accounts: &[AccountRoot],
    games: &BTreeMap<String, DeclaredGame>,
    paths: &BTreeMap<PathBuf, PathBuf>,
    cache: &mut HashCache,
) -> DeclaredScan {
    resolve_declared_files(accounts, games, cache, Some(paths))
}

fn resolve_declared_files(
    accounts: &[AccountRoot],
    games: &BTreeMap<String, DeclaredGame>,
    cache: &mut HashCache,
    paths: Option<&BTreeMap<PathBuf, PathBuf>>,
) -> DeclaredScan {
    let mut scan = DeclaredScan::default();
    let before = cache.read_bytes;
    for account in accounts {
        let root = account.root.canonicalize();
        for (game_key, game) in games {
            for (release_key, release) in &game.releases {
                let Some(file) = &release.file else { continue };
                let result = match &root {
                    Ok(root) => declared_identity(root, Path::new(file), cache, paths),
                    Err(_) => Err(IdentifyError::Unsupported(
                        "account root is unreadable".into(),
                    )),
                };
                let status = match result {
                    Ok(None) => DeclaredFileStatus::Absent,
                    Ok(Some(identity))
                        if hash::hashes(&identity)
                            .iter()
                            .any(|hash| release.hashes.iter().any(|declared| declared == hash)) =>
                    {
                        DeclaredFileStatus::Found(identity)
                    }
                    Ok(Some(identity)) => {
                        scan.attention.push(finding(
                            &account.storage_id,
                            Some(file),
                            AttentionKind::DeclaredMismatch,
                            Some(identity.value()),
                            vec![],
                            "prepared file does not match its declared hashes",
                        ));
                        DeclaredFileStatus::Mismatch
                    }
                    Err(error) => {
                        scan.attention.push(finding(
                            &account.storage_id,
                            Some(file),
                            AttentionKind::Unreadable,
                            None,
                            vec![],
                            &error.to_string(),
                        ));
                        DeclaredFileStatus::Unreadable
                    }
                };
                scan.files.push(DeclaredFile {
                    game: game_key.clone(),
                    release_key: release_key.clone(),
                    storage_id: account.storage_id.clone(),
                    path: file.clone(),
                    status,
                });
            }
        }
    }
    scan.hashed_bytes = cache.read_bytes - before;
    scan
}

fn declared_identity(
    root: &Path,
    relative: &Path,
    cache: &mut HashCache,
    paths: Option<&BTreeMap<PathBuf, PathBuf>>,
) -> Result<Option<ContentIdentity>, IdentifyError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(IdentifyError::Unsupported(
            "declared file is not a relative child path".into(),
        ));
    }
    let logical = root.join(relative);
    let path = if paths.is_some() {
        logical
    } else {
        match logical.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    };
    let scope = ReadScope {
        mapped: paths.map(|paths| MappedFiles {
            paths,
            physical_root: Some(root),
        }),
        root,
        excluded: &[],
        allowed: None,
        max_depth: 1,
        max_references: 1,
    };
    match disc::checked_path(&path, &scope).and_then(|path| file_identity(&path, cache)) {
        Err(IdentifyError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        result => result.map(Some),
    }
}

/// Prepared files are always ordinary bytes, even when their suffix is a descriptor.
pub fn file_identity(path: &Path, cache: &mut HashCache) -> Result<ContentIdentity, IdentifyError> {
    cache.hash(path, &[])
}

pub use disc::DependencyClosure;

/// Migration uses the same descriptor parser, root rules and default work bounds.
pub fn descriptor_files(path: &Path, root: &Path, excluded_roots: &[PathBuf]) -> DependencyClosure {
    let budget = TraversalBudget::default();
    let scope = ReadScope {
        mapped: None,
        root,
        excluded: excluded_roots,
        allowed: None,
        max_depth: budget.max_depth,
        max_references: budget.max_entries,
    };
    disc::dependencies(path, &scope)
}

pub fn identity_of(
    path: &Path,
    rules: &ContentRules,
    cache: &mut HashCache,
) -> Result<ContentIdentity, IdentifyError> {
    let path = std::path::absolute(path)?;
    let root = path
        .parent()
        .ok_or_else(|| IdentifyError::Unsupported("file has no parent".into()))?
        .canonicalize()?;
    let path = root.join(
        path.file_name()
            .ok_or_else(|| IdentifyError::Unsupported("file has no name".into()))?,
    );
    identity_in_source(&path, &root, &[], rules, cache)
}

pub fn identity_in_source(
    path: &Path,
    root: &Path,
    excluded_roots: &[PathBuf],
    rules: &ContentRules,
    cache: &mut HashCache,
) -> Result<ContentIdentity, IdentifyError> {
    let root = root.canonicalize()?;
    let scope = ReadScope {
        mapped: None,
        root: &root,
        excluded: excluded_roots,
        allowed: None,
        max_depth: TraversalBudget::default().max_depth,
        max_references: TraversalBudget::default().max_entries,
    };
    Ok(disc::identify(path, rules, cache, &scope)?.identity)
}

fn finding(
    source: &str,
    path: Option<&str>,
    kind: AttentionKind,
    identity: Option<&str>,
    candidates: Vec<String>,
    message: &str,
) -> AttentionFinding {
    AttentionFinding {
        kind,
        source_id: source.into(),
        path: path.map(str::to_owned),
        identity: identity.map(str::to_owned),
        candidates,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests;
