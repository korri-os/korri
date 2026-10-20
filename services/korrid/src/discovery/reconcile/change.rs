//! Pure reconciliation. Only completed scans and explicit unpairs can change sources.
use std::collections::{BTreeMap, BTreeSet};

use crate::config::{
    library::{
        LibraryIdentity, LibraryItemPayload, LibraryReleasePayload, ProviderLinkPayload,
        ProviderLinkRef, ProviderLinkRefKind,
    },
    ConfigSnapshot, FileSetPart, FileTargetDiscovery, InheritableLayer, Location, NonEmptyString,
    NonEmptyUniqueFileSetParts, StoragePayload, TargetString,
};
use crate::discovery::identify::{ContentIdentity, FoundUnit, PathRecord, ScanReport};
use crate::plugin::DeclaredReleaseFile;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogChange {
    pub created_storages: BTreeSet<String>,
    pub device_yaml: Option<String>,
    pub library_yaml: Option<String>,
    pub added_games: usize,
    pub added_releases: usize,
    pub removed_games: usize,
    pub missing_locations: usize,
    pub origins: BTreeMap<String, BTreeSet<String>>,
    pub paths: BTreeMap<String, PathRecord>,
}

pub fn reconcile(report: &ScanReport, current: &ConfigSnapshot, now: &str) -> CatalogChange {
    let mut next = current.clone();
    let mut change = CatalogChange {
        created_storages: report.created_storages.clone(),
        origins: report.origins.clone(),
        paths: report.paths.clone(),
        ..CatalogChange::default()
    };
    // Seed authored-at-start discovery locations too. Provenance survives replacement.
    let mut indexed: BTreeSet<_> = change
        .paths
        .iter()
        .filter_map(|(path, record)| {
            path.split_once('\n')
                .map(|(source, _)| (source.to_owned(), record.identity.clone()))
        })
        .collect();
    for (key, locations) in &current.locations {
        for location in locations.iter().filter(|l| discovery(l).is_some()) {
            for (source, record) in &current.sources {
                if report.skipped.contains(source)
                    || (!report.unpaired.contains(source)
                        && !report.scanned.iter().any(|s| &s.source_id == source))
                {
                    continue;
                }
                if storage(location) == Some(record.storage.0.as_str()) {
                    change
                        .origins
                        .entry(key.clone())
                        .or_default()
                        .insert(source.clone());
                    if matches!(location, Location::File { .. })
                        || !indexed.contains(&(source.clone(), key.clone()))
                    {
                        let primary = match location {
                            Location::File { path, .. } => Some(path.0.as_str()),
                            Location::FileSet { files, .. } => files
                                .0
                                .iter()
                                .find(|p| p.role.as_ref().is_some_and(|r| r.0 == "descriptor"))
                                .or_else(|| files.0.first())
                                .map(|p| p.path.0.as_str()),
                            _ => None,
                        };
                        if let Some(path) = primary {
                            change
                                .paths
                                .entry(format!("{source}\n{path}"))
                                .or_insert_with(|| PathRecord {
                                    identity: key.clone(),
                                    files: location_paths(location)
                                        .iter()
                                        .map(|p| (*p).to_owned())
                                        .collect(),
                                });
                        }
                        indexed.insert((source.clone(), key.clone()));
                    }
                }
            }
        }
    }
    let unpaired: BTreeSet<_> = report.unpaired.iter().cloned().collect();
    for source in &unpaired {
        if let Some(record) = next.sources.remove(source) {
            for locations in next.locations.values_mut() {
                locations.retain(|l| {
                    storage(l) != Some(record.storage.0.as_str()) || discovery(l).is_none()
                });
            }
            if change.created_storages.contains(&record.storage.0)
                && !next.sources.values().any(|s| s.storage == record.storage)
                && !next
                    .locations
                    .values()
                    .flatten()
                    .any(|l| storage(l) == Some(record.storage.0.as_str()))
            {
                next.storage.remove(&record.storage.0);
                change.created_storages.remove(&record.storage.0);
            }
        }
        change
            .paths
            .retain(|key, _| !key.starts_with(&format!("{source}\n")));
    }
    next.locations.retain(|_, locations| !locations.is_empty());
    for game in next.library.values_mut() {
        game.releases.retain(|release| {
            let Some(identity) = &release.identity else {
                return true;
            };
            let remove = !next.locations.contains_key(identity.value())
                && change
                    .origins
                    .get(identity.value())
                    .is_some_and(|origins| !origins.is_empty() && origins.is_subset(&unpaired));
            if remove {
                change.origins.remove(identity.value());
            }
            !remove
        });
    }
    next.library.retain(|_, game| !game.releases.is_empty());
    next.provider_links
        .retain(|_, link| next.library.contains_key(&link.playable.0));

    let mut release_games: BTreeMap<_, _> = next
        .library
        .iter()
        .flat_map(|(id, game)| {
            game.releases.iter().filter_map(move |r| {
                r.identity
                    .as_ref()
                    .map(|i| (i.value().to_owned(), id.clone()))
            })
        })
        .collect();
    for scan in &report.scanned {
        if report.skipped.contains(&scan.source_id) {
            continue;
        }
        let Some(source) = next.sources.get(&scan.source_id) else {
            continue;
        };
        let source_storage = source.storage.0.clone();
        // seen_paths includes files that could not be identified. Do not mark them missing.
        let seen: BTreeSet<_> = scan
            .seen_paths
            .iter()
            .map(String::as_str)
            .chain(scan.units.iter().flat_map(|u| {
                std::iter::once(u.path.as_str()).chain(u.parts.iter().map(String::as_str))
            }))
            .collect();
        for locations in next.locations.values_mut() {
            for location in locations {
                if storage(location) != Some(source_storage.as_str()) {
                    continue;
                }
                let absent = location_paths(location).iter().any(|p| !seen.contains(p));
                if let Some(meta) = discovery_mut(location) {
                    if absent && meta.missing_since.is_none() {
                        meta.missing_since = Some(NonEmptyString(now.into()));
                        change.missing_locations += 1;
                    } else if !absent {
                        meta.missing_since = None;
                    }
                }
            }
        }
        // Keep the old primary index until all replacements have been processed.
        for unit in &scan.units {
            add_unit(
                report,
                &mut next,
                &mut change,
                &mut release_games,
                unit,
                None,
                now,
            );
        }
        change.paths.retain(|key, _| {
            key.strip_prefix(&format!("{}\n", scan.source_id))
                .is_none_or(|path| seen.contains(path))
        });
    }
    for (found, identity) in report.declared.found() {
        let Some(game) = report.declared_games.get(&found.game) else {
            continue;
        };
        let Some(release) = game.releases.get(&found.release_key) else {
            continue;
        };
        let Some(account) = report
            .accounts
            .iter()
            .find(|a| a.storage_id == found.storage_id)
        else {
            continue;
        };
        // Only resolve_declared can produce these records after checking the declared hash.
        next.storage.insert(
            found.storage_id.clone(),
            StoragePayload {
                root: NonEmptyString(account.root.display().to_string()),
                path: None,
            },
        );
        let unit = FoundUnit {
            source_id: found.storage_id.clone(),
            storage_id: found.storage_id.clone(),
            path: found.path.clone(),
            parts: vec![found.path.clone()],
            tracks: Vec::new(),
            system: release.system.clone().unwrap_or_default(),
            title: game.title.clone(),
            version: None,
            region: None,
            identity: identity.clone(),
            evidence: crate::discovery::identify::EvidenceLevel::DeclaredHash,
        };
        add_unit(
            report,
            &mut next,
            &mut change,
            &mut release_games,
            &unit,
            Some((&found.game, &found.release_key)),
            now,
        );
    }
    // Prepared files are checked every pass, independently of paired sources.
    for locations in next.locations.values_mut() {
        for location in locations {
            let absent = report.declared.files.iter().any(|file| {
                file.status == crate::discovery::identify::DeclaredFileStatus::Absent
                    && storage(location) == Some(file.storage_id.as_str())
                    && location_paths(location).contains(&file.path.as_str())
            });
            if absent {
                if let Some(meta) = discovery_mut(location) {
                    if meta.missing_since.is_none() {
                        meta.missing_since = Some(NonEmptyString(now.into()));
                        change.missing_locations += 1;
                    }
                }
            }
        }
    }
    reconcile_artifact_releases(report, &mut next, &mut change, &mut release_games);
    reconcile_providers(report, &mut next, &mut change, &mut release_games);
    change.added_games = next
        .library
        .keys()
        .filter(|id| !current.library.contains_key(*id))
        .count();
    change.removed_games = current
        .library
        .keys()
        .filter(|id| !next.library.contains_key(*id))
        .count();
    if next.library != current.library {
        change.library_yaml = Some(
            serde_yaml::to_string(&BTreeMap::from([("library", &next.library)]))
                .expect("serializable library"),
        );
    }
    let next_device = device_value(&next);
    let old_device = device_value(current);
    if next_device != old_device {
        // The publisher patches only changed sections, preserving the other document bytes' meaning.
        change.device_yaml =
            Some(serde_yaml::to_string(&next_device).expect("serializable device"));
    }
    change
}

/// ADR 0013: a release that comes from artifacts is in the library before its
/// download. It uses only the existing declared-game records (library item,
/// release keyed by its first hash, provider link) and saves no location: the
/// route comes from the declaration and readiness from the cache. A release
/// that is no longer declared is never removed here; it stays unavailable.
fn reconcile_artifact_releases(
    report: &ScanReport,
    next: &mut ConfigSnapshot,
    change: &mut CatalogChange,
    release_games: &mut BTreeMap<String, String>,
) {
    for (declared_id, declared) in &report.declared_games {
        for (index, (release_key, release)) in declared.releases.iter().enumerate() {
            match &release.file {
                Some(DeclaredReleaseFile::Artifact(_)) => (),
                // A prepared file joins the library only when resolve_declared
                // finds and verifies it under an account root (ADR 0003).
                Some(DeclaredReleaseFile::AccountRoot(_)) | None => continue,
            }
            let Some(key) = release.hashes.first() else {
                continue;
            };
            // A discovered copy may already carry this release under any of
            // its declared hashes; it is then the same release.
            if let Some(game_id) = release
                .hashes
                .iter()
                .find_map(|hash| release_games.get(hash))
                .cloned()
            {
                next.provider_links
                    .entry(declared_id.clone())
                    .or_insert_with(|| declared_link(declared, &game_id));
                continue;
            }
            let game_id = next
                .provider_links
                .get(declared_id)
                .map(|link| link.playable.0.clone())
                .filter(|id| next.library.contains_key(id))
                .unwrap_or_else(|| {
                    let id = unique_id(&slug(&declared.title), |id| next.library.contains_key(id));
                    next.library.insert(
                        id.clone(),
                        LibraryItemPayload {
                            modifiers: Default::default(),
                            title: Some(declared.title.clone()),
                            version_of: None,
                            relation: None,
                            contains: None,
                            display: None,
                            metadata: None,
                            runner: None,
                            families: BTreeMap::new(),
                            runners: BTreeMap::new(),
                            releases: Vec::new(),
                        },
                    );
                    id
                });
            let game = next.library.get_mut(&game_id).expect("target game");
            // Keep declaration order: go before the first release that this
            // game declares later. Other releases keep their places.
            let position = game
                .releases
                .iter()
                .position(|existing| {
                    existing.identity.as_ref().is_some_and(|identity| {
                        declared.releases.values().position(|other| {
                            other.hashes.iter().any(|hash| hash == identity.value())
                        }) > Some(index)
                    })
                })
                .unwrap_or(game.releases.len());
            let id = unique_id(release_key, |id| game.releases.iter().any(|r| r.id.0 == id));
            game.releases.insert(
                position,
                LibraryReleasePayload {
                    id: NonEmptyString(id),
                    system: release.system.clone().map(NonEmptyString),
                    identity: Some(LibraryIdentity::Hash { value: key.clone() }),
                    display: None,
                    inheritable: InheritableLayer::default(),
                },
            );
            change.added_releases += 1;
            release_games.insert(key.clone(), game_id.clone());
            next.provider_links
                .insert(declared_id.clone(), declared_link(declared, &game_id));
        }
    }
}

fn declared_link(game: &crate::plugin::DeclaredGame, playable: &str) -> ProviderLinkPayload {
    ProviderLinkPayload {
        provider: NonEmptyString(game.provider.clone()),
        playable: NonEmptyString(playable.into()),
        release: None,
        refs: vec![ProviderLinkRef {
            kind: ProviderLinkRefKind::ProviderItemId,
            value: NonEmptyString(game.key.clone()),
            scope: None,
            target_part: None,
        }],
    }
}

fn reconcile_providers(
    report: &ScanReport,
    next: &mut ConfigSnapshot,
    change: &mut CatalogChange,
    release_games: &mut BTreeMap<String, String>,
) {
    for scan in &report.providers {
        let seen: BTreeSet<_> = scan
            .observations
            .iter()
            .map(|o| format!("{}/{}", o.target.provider, o.target.provider_ref))
            .collect();
        for (key, origins) in &mut change.origins {
            if origins.contains(&scan.id) && !seen.contains(key) {
                if let Some(locations) = next.locations.get_mut(key) {
                    locations.retain(|location| !matches!(location, Location::ProviderRef { .. }));
                    if locations.is_empty() {
                        next.locations.remove(key);
                    }
                }
                // Withdrawal ends ownership. A person can author this same
                // location later without discovery claiming or removing it.
                origins.remove(&scan.id);
            }
        }
        change.origins.retain(|_, origins| !origins.is_empty());
        // Keep the release and game when a manifest disappears, just as missing
        // files keep their history. ProviderRef has no missing-since field:
        // absence of its location is the existing unavailable representation.
        for observation in &scan.observations {
            let key = format!(
                "{}/{}",
                observation.target.provider, observation.target.provider_ref
            );
            if !release_games.contains_key(&key) {
                let id = unique_id(&slug(&observation.release.title), |id| {
                    next.library.contains_key(id)
                });
                next.library.insert(
                    id.clone(),
                    LibraryItemPayload {
                        modifiers: Default::default(),
                        title: Some(observation.release.title.clone()),
                        version_of: None,
                        relation: None,
                        contains: None,
                        display: None,
                        metadata: None,
                        runner: None,
                        families: BTreeMap::new(),
                        runners: BTreeMap::new(),
                        releases: vec![LibraryReleasePayload {
                            id: NonEmptyString(observation.release.id.clone()),
                            system: Some(NonEmptyString(observation.release.system.clone())),
                            identity: Some(LibraryIdentity::Provider { value: key.clone() }),
                            display: None,
                            inheritable: InheritableLayer::default(),
                        }],
                    },
                );
                release_games.insert(key.clone(), id);
                change.added_releases += 1;
            }
            let locations = next.locations.entry(key.clone()).or_default();
            if locations.is_empty() {
                locations.push(Location::ProviderRef {
                    provider: crate::config::ProviderIdString(observation.target.provider.clone()),
                    provider_ref: NonEmptyString(observation.target.provider_ref.clone()),
                });
                change
                    .origins
                    .entry(key)
                    .or_default()
                    .insert(scan.id.clone());
            }
            // Existing authored provider locations are not discovery-owned.
        }
    }
}

pub(super) fn device_value(snapshot: &ConfigSnapshot) -> serde_yaml::Mapping {
    let mut value = serde_yaml::to_value(snapshot)
        .expect("serializable snapshot")
        .as_mapping()
        .expect("snapshot map")
        .clone();
    value.remove("library");
    if snapshot.host.is_none() {
        value.remove("host");
    }
    value
}

fn add_unit(
    report: &ScanReport,
    next: &mut ConfigSnapshot,
    change: &mut CatalogChange,
    release_games: &mut BTreeMap<String, String>,
    unit: &FoundUnit,
    declared: Option<(&str, &str)>,
    now: &str,
) {
    let key = unit.identity.value().to_owned();
    let path_key = format!("{}\n{}", unit.source_id, unit.path);
    let previous_record = change.paths.get(&path_key).cloned();
    let previous_key = previous_record.as_ref().map(|r| r.identity.clone());
    let declared = declared
        .map(|(g, r)| (g.to_owned(), r.to_owned()))
        .or_else(|| {
            report.declared_games.iter().find_map(|(id, game)| {
                game.releases
                    .iter()
                    .find(|(_, release)| {
                        release
                            .hashes
                            .iter()
                            .any(|hash| identity_matches(&unit.identity, hash))
                    })
                    .map(|(release, _)| (id.clone(), release.clone()))
            })
        });
    let identity_game = release_games.get(&key).cloned();
    let previous_game = previous_key
        .as_deref()
        .and_then(|key| release_games.get(key).cloned());
    let linked_game = declared
        .as_ref()
        .and_then(|(id, _)| next.provider_links.get(id))
        .map(|link| link.playable.0.clone());
    let game_id = linked_game
        .or_else(|| identity_game.clone())
        .or_else(|| previous_game.clone())
        .unwrap_or_else(|| {
            let title = declared
                .as_ref()
                .and_then(|(id, _)| report.declared_games.get(id))
                .map(|g| g.title.as_str())
                .unwrap_or(&unit.title);
            let id = unique_id(&slug(title), |id| next.library.contains_key(id));
            next.library.insert(
                id.clone(),
                LibraryItemPayload {
                    modifiers: Default::default(),
                    title: Some(title.into()),
                    version_of: None,
                    relation: None,
                    contains: None,
                    display: None,
                    metadata: None,
                    runner: None,
                    families: BTreeMap::new(),
                    runners: BTreeMap::new(),
                    releases: Vec::new(),
                },
            );
            id
        });
    // A newly learned declared identity can join games that were previously separate.
    if declared.is_some() {
        if let Some(other) = identity_game.filter(|other| *other != game_id) {
            if let Some(old) = next.library.remove(&other) {
                for mut release in old.releases {
                    let game = next.library.get_mut(&game_id).expect("target game");
                    release.id.0 = unique_id(&release.id.0, |id| {
                        game.releases.iter().any(|r| r.id.0 == id)
                    });
                    if let Some(identity) = &release.identity {
                        release_games.insert(identity.value().to_owned(), game_id.clone());
                    }
                    game.releases.push(release);
                }
                for link in next
                    .provider_links
                    .values_mut()
                    .filter(|l| l.playable.0 == other)
                {
                    link.playable.0 = game_id.clone();
                }
            }
        }
    }
    if let Some((id, _)) = &declared {
        next.provider_links.insert(
            id.clone(),
            declared_link(&report.declared_games[id], &game_id),
        );
    }
    if !release_games.contains_key(&key) {
        let game = next.library.get_mut(&game_id).expect("target game");
        let base = declared
            .as_ref()
            .map(|(_, release)| release.as_str())
            .unwrap_or(&unit.system);
        let id = unique_id(base, |id| game.releases.iter().any(|r| r.id.0 == id));
        let display: BTreeMap<_, _> = [("version", &unit.version), ("region", &unit.region)]
            .into_iter()
            .filter_map(|(k, v)| {
                v.as_ref()
                    .map(|v| (k.into(), serde_json::Value::String(v.clone())))
            })
            .collect();
        game.releases.push(LibraryReleasePayload {
            id: NonEmptyString(id),
            system: (!unit.system.is_empty()).then(|| NonEmptyString(unit.system.clone())),
            identity: Some(library_identity(&unit.identity)),
            display: (!display.is_empty()).then_some(display),
            inheritable: InheritableLayer::default(),
        });
        change.added_releases += 1;
        release_games.insert(key.clone(), game_id.clone());
    }
    if let Some(previous) = previous_key.filter(|previous| *previous != key) {
        if let Some(locations) = next.locations.get_mut(&previous) {
            locations.retain(|l| {
                if storage(l) != Some(&unit.storage_id) || discovery(l).is_none() {
                    return true;
                }
                if let Some(previous) = &previous_record {
                    location_paths(l)
                        != previous
                            .files
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                } else {
                    !location_paths(l)
                        .iter()
                        .any(|p| *p == unit.path || unit.parts.iter().any(|part| part == p))
                }
            });
        }
        if next.locations.get(&previous).is_some_and(Vec::is_empty) {
            next.locations.remove(&previous);
        }
    }
    let new_location = location(unit, now);
    let locations = next.locations.entry(key.clone()).or_default();
    if let Some(existing) = locations
        .iter_mut()
        .find(|l| same_location(l, &new_location))
    {
        if let Some(meta) = discovery_mut(existing) {
            meta.missing_since = None;
        }
    } else {
        locations.push(new_location);
    }
    change
        .origins
        .entry(key.clone())
        .or_default()
        .insert(unit.source_id.clone());
    change.paths.insert(
        path_key,
        PathRecord {
            identity: key,
            files: location_paths(&location(unit, now))
                .iter()
                .map(|p| (*p).to_owned())
                .collect(),
        },
    );
}

fn identity_matches(identity: &ContentIdentity, hash: &str) -> bool {
    identity.value() == hash
        || matches!(identity, ContentIdentity::Hash { sha1, .. } if sha1 == hash)
}
fn library_identity(identity: &ContentIdentity) -> LibraryIdentity {
    match identity {
        ContentIdentity::Hash { sha256, .. } => LibraryIdentity::Hash {
            value: sha256.clone(),
        },
        ContentIdentity::Parts { value, parts } => LibraryIdentity::Parts {
            value: value.clone(),
            parts: parts.clone(),
        },
        ContentIdentity::Chd { sha1 } => LibraryIdentity::Chd {
            value: sha1.clone(),
        },
    }
}
fn location(unit: &FoundUnit, now: &str) -> Location {
    let meta = Some(FileTargetDiscovery {
        first_seen_at: NonEmptyString(now.into()),
        missing_since: None,
    });
    if matches!(unit.identity, ContentIdentity::Parts { .. }) {
        let cue = unit.path.to_ascii_lowercase().ends_with(".cue");
        let descriptor = (cue || unit.path.to_ascii_lowercase().ends_with(".m3u"))
            && !unit.parts.contains(&unit.path);
        let mut files = Vec::new();
        if descriptor {
            files.push(FileSetPart {
                id: NonEmptyString("descriptor".into()),
                role: Some(NonEmptyString("descriptor".into())),
                path: TargetString(unit.path.clone()),
            });
        }
        let tracks: BTreeSet<_> = unit.tracks.iter().map(String::as_str).collect();
        files.extend(unit.parts.iter().enumerate().map(|(i, path)| {
            FileSetPart {
                id: NonEmptyString((i + 1).to_string()),
                role: Some(NonEmptyString(
                    if tracks.contains(path.as_str()) {
                        "track"
                    } else {
                        "disc"
                    }
                    .into(),
                )),
                path: TargetString(path.clone()),
            }
        }));
        Location::FileSet {
            storage: NonEmptyString(unit.storage_id.clone()),
            root: None,
            files: NonEmptyUniqueFileSetParts(files),
            discovery: meta,
        }
    } else {
        Location::File {
            storage: NonEmptyString(unit.storage_id.clone()),
            path: TargetString(unit.path.clone()),
            discovery: meta,
        }
    }
}
fn same_location(a: &Location, b: &Location) -> bool {
    storage(a) == storage(b) && location_paths(a) == location_paths(b)
}
fn storage(location: &Location) -> Option<&str> {
    match location {
        Location::File { storage, .. } | Location::FileSet { storage, .. } => Some(&storage.0),
        _ => None,
    }
}
fn location_paths(location: &Location) -> Vec<&str> {
    match location {
        Location::File { path, .. } => vec![&path.0],
        Location::FileSet { files, .. } => files.0.iter().map(|f| f.path.0.as_str()).collect(),
        _ => Vec::new(),
    }
}
fn discovery(location: &Location) -> Option<&FileTargetDiscovery> {
    match location {
        Location::File { discovery, .. } | Location::FileSet { discovery, .. } => {
            discovery.as_ref()
        }
        _ => None,
    }
}
fn discovery_mut(location: &mut Location) -> Option<&mut FileTargetDiscovery> {
    match location {
        Location::File { discovery, .. } | Location::FileSet { discovery, .. } => {
            discovery.as_mut()
        }
        _ => None,
    }
}
pub(super) fn slug(title: &str) -> String {
    let mut result = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            result.push(c);
        } else if !result.is_empty() && !result.ends_with('-') {
            result.push('-');
        }
    }
    let result = result.trim_end_matches('-');
    if result.is_empty() {
        "game".into()
    } else {
        result.into()
    }
}
pub(super) fn unique_id(base: &str, occupied: impl Fn(&str) -> bool) -> String {
    if !occupied(base) {
        return base.into();
    }
    for suffix in 2.. {
        let id = format!("{base}-{suffix}");
        if !occupied(&id) {
            return id;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests;
