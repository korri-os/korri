//! One-off move of a device from the old catalog layout to sources and
//! `catalog/library.yaml` (contract section 7 of
//! `docs/briefs/2026-10-16-discovery-v2-contract.md`). Owned by the
//! `migration` builder. Dry run by default.

use serde_yaml::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::plugin::{
    ContentRules, DeclaredGame, DeclaredRelease, DeclaredReleaseFile, GroupRecord, PluginRegistry,
};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MigrationPlan {
    pub kept: Vec<KeptGame>,
    pub dropped: Vec<DroppedEntry>,
    pub moves: Vec<FileMove>,
    pub sources: Vec<NewSource>,
    pub unresolved: Vec<String>,
    pub play_logs: Vec<PlayLogRename>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct KeptGame {
    pub old_id: String,
    /// Computed during dry run by identification and pure reconciliation.
    pub new_id: Option<String>,
    pub title: String,
    pub files: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DroppedEntry {
    pub old_id: String,
    pub title: String,
    pub file: Option<PathBuf>,
    pub reason: DropReason,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DropReason {
    PluginData,
    Unclaimed,
    Duplicate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileMove {
    pub from: PathBuf,
    pub to: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NewSource {
    pub id: String,
    pub root: PathBuf,
    pub system: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PlayLogRename {
    pub user: String,
    pub old_game: String,
    pub new_game: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MigrationReport {
    pub plan: MigrationPlan,
    pub completed_steps: Vec<String>,
    pub moved: Vec<FileMove>,
    /// Files copied out of the read-only Nix store. Undo deletes `to`.
    #[serde(default)]
    pub copied: Vec<FileMove>,
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("old catalog could not be read: {0}")]
    Read(String),
    #[error("migration step failed: {0}")]
    Step(String),
}

// Old readers are deliberately private: the production decoder accepts only v2.
#[derive(Default, Deserialize)]
struct OldDevice {
    #[serde(default)]
    storage: BTreeMap<String, OldStorage>,
    #[serde(default)]
    locations: BTreeMap<String, Vec<OldLocation>>,
}

#[derive(Deserialize)]
struct OldStorage {
    root: PathBuf,
}

#[derive(Deserialize)]
struct OldLocation {
    storage: Option<String>,
    path: Option<PathBuf>,
    root: Option<PathBuf>,
    files: Option<Vec<OldPart>>,
}

#[derive(Deserialize)]
struct OldPart {
    path: PathBuf,
}

#[derive(Default, Deserialize)]
struct OldGames {
    #[serde(default)]
    games: BTreeMap<String, OldGame>,
}
#[derive(Deserialize)]
struct OldGame {
    title: String,
    releases: Vec<String>,
}
#[derive(Default, Deserialize)]
struct OldReleases {
    #[serde(default)]
    releases: BTreeMap<String, OldRelease>,
}
#[derive(Deserialize)]
struct OldRelease {
    game: String,
}

trait MigrationRules {
    fn systems(&self) -> Vec<&ContentRules>;
    fn declared_games(&self) -> &BTreeMap<String, DeclaredGame>;
    fn target(&self, id: &str) -> Option<crate::plugin::PairingTarget<'_>>;
    fn groups(&self) -> Vec<&GroupRecord>;
    fn declared(&self, hash: &str) -> Vec<&DeclaredRelease>;
    fn hashes(
        &self,
        path: &Path,
        root: &Path,
        excluded: &[PathBuf],
        rules: &ContentRules,
    ) -> Result<Vec<String>, MigrationError>;
}

impl MigrationRules for PluginRegistry {
    fn target(&self, id: &str) -> Option<crate::plugin::PairingTarget<'_>> {
        self.pairing_target(id)
    }
    fn declared_games(&self) -> &BTreeMap<String, DeclaredGame> {
        self.declared_games()
    }
    fn systems(&self) -> Vec<&ContentRules> {
        self.systems()
            .values()
            .map(|system| system.id.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|id| self.content_rules(id))
            .collect()
    }
    fn groups(&self) -> Vec<&GroupRecord> {
        self.groups().values().collect()
    }
    fn declared(&self, hash: &str) -> Vec<&DeclaredRelease> {
        self.declared_hash(hash)
            .into_iter()
            .map(|r| r.release)
            .collect()
    }
    fn hashes(
        &self,
        path: &Path,
        root: &Path,
        excluded: &[PathBuf],
        rules: &ContentRules,
    ) -> Result<Vec<String>, MigrationError> {
        use crate::discovery::identify::{identity_in_source, ContentIdentity};
        let identity = identity_in_source(path, root, excluded, rules, &mut Default::default())
            .map_err(|_| MigrationError::Read("content identity could not be read".into()))?;
        Ok(match identity {
            ContentIdentity::Hash { sha256, sha1 } => vec![sha256, sha1],
            value => vec![value.value().to_owned()],
        })
    }
}

pub fn plan(
    readable_root: &Path,
    private_root: &Path,
    registry: &PluginRegistry,
) -> Result<MigrationPlan, MigrationError> {
    plan_with_rules(readable_root, private_root, registry)
}

fn plan_with_rules(
    readable_root: &Path,
    private_root: &Path,
    registry: &impl MigrationRules,
) -> Result<MigrationPlan, MigrationError> {
    let root = readable_root
        .canonicalize()
        .map_err(|_| read_error("readable root"))?;
    let device: OldDevice = decode_yaml(&root.join("device.yaml"))?;
    let games: OldGames = decode_yaml(&root.join("catalog/games.yaml"))?;
    let releases: OldReleases = decode_yaml(&root.join("catalog/releases.yaml"))?;
    // Read each old discovery journal now, before an operator can apply a plan.
    let _state = read_old_state(private_root)?;
    let mut result = MigrationPlan::default();
    let mut seen = BTreeSet::new();
    let mut sources = BTreeMap::new();
    let mut installations = Vec::new();
    let mut grouped = Vec::new();
    for (id, game) in games.games {
        let mut entries = Vec::new();
        for release in &game.releases {
            if releases.releases.get(release).is_none_or(|r| r.game != id) {
                return Err(read_error("old game/release relationship"));
            }
            for location in device.locations.get(release).into_iter().flatten() {
                let Some(storage_id) = &location.storage else {
                    continue;
                };
                let declared_root = if storage_id == "roms" {
                    root.join("roms")
                } else {
                    device
                        .storage
                        .get(storage_id)
                        .ok_or_else(|| read_error("old location storage"))?
                        .root
                        .clone()
                };
                let storage = declared_root
                    .canonicalize()
                    .map_err(|_| read_error("old storage root"))?;
                let paths: Vec<PathBuf> = if let Some(parts) = &location.files {
                    parts
                        .iter()
                        .map(|p| {
                            location
                                .root
                                .as_deref()
                                .unwrap_or(Path::new(""))
                                .join(&p.path)
                        })
                        .collect()
                } else {
                    location.path.iter().cloned().collect()
                };
                for relative in paths {
                    safe_relative(&relative)?;
                    let file = storage
                        .join(&relative)
                        .canonicalize()
                        .map_err(|_| read_error("old location file"))?;
                    if !file.starts_with(&storage) || !file.is_file() {
                        return Err(read_error("old file escapes its storage"));
                    }
                    entries.push((file, relative, storage.clone()));
                }
            }
        }
        entries.sort_by_key(|(file, _, _)| (descriptor_priority(file), file.clone()));
        grouped.push((id, game, entries));
    }
    grouped.sort_by_key(|(id, _, entries)| {
        (
            entries
                .iter()
                .map(|(file, _, _)| descriptor_priority(file))
                .min()
                .unwrap_or(2),
            id.clone(),
        )
    });
    let excluded = crate::discovery::sources::excluded_roots(&root, private_root);
    for (id, game, entries) in grouped {
        let mut kept = KeptGame {
            old_id: id.clone(),
            new_id: None,
            title: game.title.clone(),
            files: vec![],
        };
        for (file, relative, storage) in entries {
            let reason = if !seen.insert(file.clone()) {
                Some(DropReason::Duplicate)
            } else if let Ok(account_path) = file.strip_prefix(root.join("users")) {
                let mut segments = account_path.components();
                let user = segments.next().ok_or_else(|| read_error("account root"))?;
                let account = root.join("users").join(user.as_os_str());
                let identity =
                    crate::discovery::identify::file_identity(&file, &mut Default::default())
                        .map_err(|_| read_error("prepared file identity"))?;
                let hashes = match identity {
                    crate::discovery::identify::ContentIdentity::Hash { sha256, sha1 } => {
                        vec![sha256, sha1]
                    }
                    _ => unreachable!("ordinary file identity"),
                };
                let mut targets = BTreeSet::new();
                for release in hashes.iter().flat_map(|hash| registry.declared(hash)) {
                    match &release.file {
                        Some(DeclaredReleaseFile::AccountRoot(relative)) => {
                            safe_relative(Path::new(relative))?;
                            targets.insert(account.join(relative));
                        }
                        // Artifact content lives in korrid's cache, never under
                        // an account root, so it gives no migration target.
                        Some(
                            DeclaredReleaseFile::Artifact(_) | DeclaredReleaseFile::Package(_),
                        )
                        | None => (),
                    }
                }
                match targets.len() {
                    0 => Some(DropReason::PluginData),
                    1 => {
                        let target = targets.into_iter().next().unwrap();
                        if let Some(movement) = installation_move(&account, &file, &target) {
                            installations.push((file.clone(), movement));
                            kept.files.push(file.clone());
                        } else {
                            result.unresolved.push(file.display().to_string());
                        }
                        None
                    }
                    _ => {
                        result.unresolved.push(file.display().to_string());
                        None
                    }
                }
            } else {
                let descriptor = descriptor_priority(&file) < 2;
                let files = if descriptor
                    && registry.systems().iter().any(|rule| claims(&file, rule))
                {
                    let closure =
                        crate::discovery::identify::descriptor_files(&file, &storage, &excluded);
                    seen.extend(closure.files.iter().cloned());
                    if closure.error.is_some() || storage.join(&relative) != file {
                        result.unresolved.push(file.display().to_string());
                        continue;
                    }
                    closure.files
                } else {
                    BTreeSet::from([file.clone()])
                };
                match choose_system(&file, &storage, &excluded, registry)? {
                    Choice::Unclaimed => Some(DropReason::Unclaimed),
                    Choice::Unresolved => {
                        result.unresolved.push(file.display().to_string());
                        None
                    }
                    Choice::Target(system) => {
                        safe_relative(Path::new(&system))?;
                        if Path::new(&system).components().count() != 1 {
                            return Err(read_error("system id"));
                        }
                        let target = root.join("games").join(&system);
                        let movements: Vec<_> = files
                            .iter()
                            .map(|member| FileMove {
                                from: member.clone(),
                                to: if member == &file {
                                    target.join(&relative)
                                } else {
                                    target.join(member.strip_prefix(&storage).unwrap())
                                },
                            })
                            .collect();
                        if descriptor
                            && movements.iter().any(|movement| {
                                validate_destination(&root, &movement.to).is_err()
                                    || fs::symlink_metadata(&movement.to).is_ok()
                                    || result.moves.iter().any(|old| {
                                        old.from == movement.from && old.to != movement.to
                                    })
                            })
                        {
                            result.unresolved.push(file.display().to_string());
                            continue;
                        }
                        sources.entry(system.clone()).or_insert_with(|| NewSource {
                            id: system.clone(),
                            root: target.clone(),
                            system,
                        });
                        result.moves.extend(movements);
                        kept.files.extend(files);
                        None
                    }
                }
            };
            if let Some(reason) = reason {
                result.dropped.push(DroppedEntry {
                    old_id: id.clone(),
                    title: game.title.clone(),
                    file: Some(file),
                    reason,
                });
            }
        }
        if !kept.files.is_empty() {
            kept.files.sort();
            kept.files.dedup();
            result.kept.push(kept);
        }
    }
    for (index, (file, movement)) in installations.iter().enumerate() {
        let overlaps = installations
            .iter()
            .enumerate()
            .any(|(other_index, (_, other))| {
                index != other_index
                    && (movement.from != movement.to || other.from != other.to)
                    && (movement.from.starts_with(&other.from)
                        || other.from.starts_with(&movement.from)
                        || movement.to.starts_with(&other.to)
                        || other.to.starts_with(&movement.to))
            });
        if overlaps
            || (movement.from != movement.to
                && (fs::symlink_metadata(&movement.to).is_ok()
                    || movement.to.starts_with(&movement.from)))
        {
            result.unresolved.push(file.display().to_string());
        } else if movement.from != movement.to {
            result.moves.push(movement.clone());
        }
    }
    for movement in &result.moves {
        validate_destination(&root, &movement.to)?;
        if movement.from != movement.to && fs::symlink_metadata(&movement.to).is_ok() {
            result.unresolved.push(movement.from.display().to_string());
        }
    }
    result.sources = sources.into_values().collect();
    result
        .moves
        .sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
    result.moves.dedup();
    result.unresolved.sort();
    result.unresolved.dedup();
    for log in read_play_logs(private_root)? {
        if result.kept.iter().any(|g| g.old_id == log.game_id) {
            result.play_logs.push(PlayLogRename {
                user: log.user_id,
                old_game: log.game_id,
                new_game: None,
            });
        }
    }
    result
        .play_logs
        .sort_by(|a, b| (&a.user, &a.old_game).cmp(&(&b.user, &b.old_game)));
    let forecast = prospective_catalog(&result, &root, private_root, registry)?;
    match_new_ids(&mut result, &paths_by_game(&forecast));
    for game in result.kept.iter().filter(|game| game.new_id.is_none()) {
        result
            .unresolved
            .extend(game.files.iter().map(|file| file.display().to_string()));
    }
    let mut log_targets = BTreeSet::new();
    for log in &result.play_logs {
        if let Some(new_game) = &log.new_game {
            let target = log_path(private_root, &log.user, new_game);
            if new_game != &log.old_game
                && (fs::symlink_metadata(&target).is_ok() || !log_targets.insert(target.clone()))
            {
                result.unresolved.push(target.display().to_string());
            }
        }
    }
    result.unresolved.sort();
    result.unresolved.dedup();
    Ok(result)
}

fn add_sources(
    document: &mut Value,
    sources: &[NewSource],
    owned: &BTreeSet<String>,
) -> Result<(), MigrationError> {
    for source in sources {
        let storage = section(document, "storage")?;
        if storage.contains_key(Value::String(source.id.clone())) && !owned.contains(&source.id) {
            return Err(step_error(
                "new source storage id conflicts with authored storage",
            ));
        }
        storage.insert(
            source.id.clone().into(),
            serde_yaml::to_value(serde_json::json!({"root":source.root}))
                .map_err(|_| step_error("serialize storage"))?,
        );
        let sources = section(document, "sources")?;
        if sources.contains_key(Value::String(source.id.clone())) {
            return Err(step_error("new source id already exists"));
        }
        sources.insert(
            source.id.clone().into(),
            serde_yaml::to_value(
                serde_json::json!({"kind":["files"],"storage":source.id,"system":source.system}),
            )
            .map_err(|_| step_error("serialize source"))?,
        );
    }
    Ok(())
}

fn retire_old_records(
    document: &mut Value,
    retired: &OldReleases,
    owned: &BTreeSet<String>,
    new_ids: &BTreeSet<&str>,
) -> Result<(), MigrationError> {
    // Every old release key belonged to the retired two-file catalog.
    let locations = section(document, "locations")?;
    for id in retired.releases.keys() {
        locations.remove(Value::String(id.clone()));
    }
    let referenced: BTreeSet<_> = locations
        .values()
        .filter_map(Value::as_sequence)
        .flatten()
        .filter_map(|location| location.get("storage").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    for id in owned {
        if !new_ids.contains(id.as_str()) && !referenced.contains(id) {
            section(document, "storage")?.remove(Value::String(id.clone()));
        }
    }
    Ok(())
}

fn previous_path(moves: &[FileMove], future: &Path) -> Option<PathBuf> {
    if let Some(path) = moves.iter().find_map(|movement| {
        future.strip_prefix(&movement.to).ok().map(|suffix| {
            if suffix.as_os_str().is_empty() {
                movement.from.clone()
            } else {
                movement.from.join(suffix)
            }
        })
    }) {
        return Some(path);
    }
    if moves
        .iter()
        .any(|movement| movement.from != movement.to && future.starts_with(&movement.from))
    {
        None
    } else {
        Some(future.to_owned())
    }
}

fn prospective_catalog(
    plan: &MigrationPlan,
    root: &Path,
    private: &Path,
    registry: &impl MigrationRules,
) -> Result<crate::config::ConfigSnapshot, MigrationError> {
    use crate::discovery::{
        identify::{self, ScanReport, SourceInput},
        reconcile,
        scanner::TraversalBudget,
        sources,
    };
    let mut document = read_yaml(&root.join("device.yaml"))?;
    let owned = owned_storages(private, &document)?;
    add_sources(&mut document, &plan.sources, &owned)?;
    let retired: OldReleases = decode_yaml(&root.join("catalog/releases.yaml"))?;
    retire_old_records(
        &mut document,
        &retired,
        &owned,
        &plan
            .sources
            .iter()
            .map(|source| source.id.as_str())
            .collect(),
    )?;
    let library = match fs::symlink_metadata(root.join("catalog/library.yaml")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".into(),
        _ => String::from_utf8(read_regular(&root.join("catalog/library.yaml"))?)
            .map_err(|_| read_error("existing library"))?,
    };
    let current = crate::config::decode_config_documents(
        &serde_yaml::to_string(&document).map_err(|_| read_error("prospective device"))?,
        &library,
    )
    .map_err(|_| read_error("prospective catalog"))?;
    let accounts = sources::account_roots(root).map_err(|_| read_error("account roots"))?;
    let mut report = ScanReport {
        accounts: accounts.clone(),
        declared_games: registry.declared_games().clone(),
        ..Default::default()
    };
    let excluded = sources::excluded_roots(root, private);
    let mut changes = BTreeMap::new();
    for movement in &plan.moves {
        if movement.from.is_file() {
            changes.insert(movement.from.clone(), None);
        }
    }
    for movement in &plan.moves {
        if movement.from.is_file() {
            changes.insert(movement.to.clone(), Some(movement.from.clone()));
        }
    }
    let decisions = crate::discovery::attention::load(private)
        .map_err(|_| read_error("attention decisions"))?;
    let views = reconcile::source_views(&current);
    let roots: Vec<_> = views
        .iter()
        .map(|view| {
            let path = sources::canonical_future_path(&view.root);
            (path.is_dir()
                || changes
                    .iter()
                    .any(|(file, physical)| physical.is_some() && file.starts_with(&path)))
            .then_some(path)
        })
        .collect();
    let mut cache = Default::default();
    for (index, view) in views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.scans_files)
    {
        let Some(root) = roots[index].as_ref() else {
            report.skipped.push(view.source_id.clone());
            continue;
        };
        let (members, group) = match registry.target(&view.system) {
            Some(crate::plugin::PairingTarget::System(rule)) => (vec![rule], false),
            Some(crate::plugin::PairingTarget::Group { members, .. }) => (members, true),
            None => {
                report.skipped.push(view.source_id.clone());
                continue;
            }
        };
        if roots.iter().enumerate().any(|(other, path)| {
            other != index
                && path
                    .as_ref()
                    .is_some_and(|path| sources::overlaps(root, path))
        }) || excluded.iter().any(|path| sources::overlaps(root, path))
        {
            report.skipped.push(view.source_id.clone());
            continue;
        }
        let input = SourceInput {
            source_id: &view.source_id,
            storage_id: &view.storage_id,
            root,
            system: &view.system,
            excluded_roots: &excluded,
        };
        let pairing = sources::pairing_key(&view.system, root);
        let scan = identify::identify_planned_with_rules(
            &input,
            &members,
            group,
            &|source, path, identity| {
                decisions
                    .assigned_system(source, path, identity, &pairing)
                    .map(str::to_owned)
            },
            &|hash| {
                registry
                    .declared(hash)
                    .iter()
                    .filter_map(|release| release.system.clone())
                    .collect()
            },
            &changes,
            &mut cache,
            &TraversalBudget::default(),
        );
        if crate::discovery::scanner::incomplete(&scan) {
            report.skipped.push(view.source_id.clone());
        } else {
            report.scanned.push(scan);
        }
    }
    let mut files = BTreeMap::new();
    for account in &accounts {
        let account_root = account
            .root
            .canonicalize()
            .map_err(|_| read_error("account root"))?;
        for release in registry
            .declared_games()
            .values()
            .flat_map(|game| game.releases.values())
        {
            match &release.file {
                Some(DeclaredReleaseFile::AccountRoot(file)) => {
                    let future = account_root.join(file);
                    if let Some(physical) = previous_path(&plan.moves, &future) {
                        files.insert(future, physical);
                    }
                }
                // A planned move never touches korrid's cache.
                Some(DeclaredReleaseFile::Artifact(_) | DeclaredReleaseFile::Package(_)) | None => {
                    ()
                }
            }
        }
    }
    report.declared = identify::resolve_declared_planned(
        &accounts,
        registry.declared_games(),
        &files,
        &mut cache,
    );
    let change = reconcile::reconcile(
        &report,
        &current,
        &crate::discovery::DiscoveryOptions::default().first_seen_at,
    );
    reconcile::apply_change(&current, &change).map_err(|_| read_error("prospective reconciliation"))
}

fn paths_by_game(snapshot: &crate::config::ConfigSnapshot) -> BTreeMap<PathBuf, BTreeSet<String>> {
    let mut index = BTreeMap::<PathBuf, BTreeSet<String>>::new();
    for (id, game) in &snapshot.library {
        for release in &game.releases {
            let Some(identity) = &release.identity else {
                continue;
            };
            for location in snapshot
                .locations
                .get(identity.value())
                .into_iter()
                .flatten()
            {
                let (storage, paths) = match location {
                    crate::config::Location::File { storage, path, .. } => {
                        (storage, vec![PathBuf::from(&path.0)])
                    }
                    crate::config::Location::FileSet {
                        storage,
                        root,
                        files,
                        ..
                    } => (
                        storage,
                        files
                            .0
                            .iter()
                            .map(|part| {
                                Path::new(root.as_ref().map(|root| root.0.as_str()).unwrap_or(""))
                                    .join(&part.path.0)
                            })
                            .collect(),
                    ),
                    _ => continue,
                };
                if let Some(storage) = snapshot.storage.get(&storage.0) {
                    for path in paths {
                        index
                            .entry(Path::new(&storage.root.0).join(path))
                            .or_default()
                            .insert(id.clone());
                    }
                }
            }
        }
    }
    index
}

fn match_new_ids(plan: &mut MigrationPlan, by_path: &BTreeMap<PathBuf, BTreeSet<String>>) {
    for game in &mut plan.kept {
        let mut candidates: Option<BTreeSet<String>> = None;
        let mut complete = true;
        for file in &game.files {
            match by_path.get(&moved_path(&plan.moves, file)) {
                Some(ids) => {
                    candidates = Some(candidates.map_or_else(
                        || ids.clone(),
                        |current| current.intersection(ids).cloned().collect(),
                    ));
                }
                None => {
                    complete = false;
                }
            }
        }
        game.new_id = candidates
            .filter(|ids| complete && ids.len() == 1)
            .and_then(|ids| ids.into_iter().next());
    }
    for log in &mut plan.play_logs {
        log.new_game = plan
            .kept
            .iter()
            .find(|game| game.old_id == log.old_game)
            .and_then(|game| game.new_id.clone());
    }
}

fn descriptor_priority(path: &Path) -> u8 {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "m3u" => 0,
        "cue" => 1,
        _ => 2,
    }
}

/// Strip the longest common suffix, so native companions move with the installation.
fn installation_move(account: &Path, file: &Path, declared: &Path) -> Option<FileMove> {
    if file == declared {
        return Some(FileMove {
            from: file.into(),
            to: declared.into(),
        });
    }
    let old: Vec<_> = file.strip_prefix(account).ok()?.components().collect();
    let new: Vec<_> = declared.strip_prefix(account).ok()?.components().collect();
    let common = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    if common == old.len() {
        return None;
    }
    let from = account.join(old[..old.len() - common].iter().collect::<PathBuf>());
    if !from.is_dir() {
        return None;
    }
    Some(FileMove {
        from,
        to: account.join(new[..new.len() - common].iter().collect::<PathBuf>()),
    })
}

fn moved_path(moves: &[FileMove], from: &Path) -> PathBuf {
    moves
        .iter()
        .find_map(|movement| {
            from.strip_prefix(&movement.from).ok().map(|suffix| {
                if suffix.as_os_str().is_empty() {
                    movement.to.clone()
                } else {
                    movement.to.join(suffix)
                }
            })
        })
        .unwrap_or_else(|| from.to_owned())
}

enum Choice {
    Unclaimed,
    Unresolved,
    Target(String),
}

fn claims(path: &Path, rules: &ContentRules) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let lower = name.to_lowercase();
            rules
                .extensions
                .iter()
                .any(|extension| lower.ends_with(&format!(".{extension}")))
        })
}

fn choose_system(
    path: &Path,
    root: &Path,
    excluded: &[PathBuf],
    registry: &impl MigrationRules,
) -> Result<Choice, MigrationError> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| read_error("file name"))?;
    let candidates: Vec<_> = registry
        .systems()
        .into_iter()
        .filter(|rules| claims(path, rules))
        .collect();
    if candidates.is_empty() {
        return Ok(Choice::Unclaimed);
    }
    if candidates.len() == 1 {
        return Ok(Choice::Target(candidates[0].system.clone()));
    }
    let mut prefix = vec![];
    fs::File::open(path)
        .and_then(|f| f.take(65536).read_to_end(&mut prefix))
        .map_err(|_| read_error("evidence bytes"))?;
    let mut ranked = Vec::new();
    for rules in candidates {
        let hashes = match registry.hashes(path, root, excluded, rules) {
            Ok(hashes) => hashes,
            Err(_) => return Ok(Choice::Unresolved),
        };
        let hashes: Vec<_> = hashes.iter().map(String::as_str).collect();
        let level =
            crate::discovery::identify::evidence_level(name, &prefix, rules, &hashes, &|hash| {
                registry
                    .declared(hash)
                    .iter()
                    .filter_map(|r| r.system.clone())
                    .collect()
            });
        ranked.push((level, rules.system.clone()));
    }
    let best = ranked.iter().map(|(rank, _)| *rank).max().unwrap();
    let systems: BTreeSet<_> = ranked
        .into_iter()
        .filter(|(rank, _)| *rank == best)
        .map(|(_, system)| system)
        .collect();
    if systems.len() == 1 {
        return Ok(Choice::Target(systems.into_iter().next().unwrap()));
    }
    let mut groups = registry.groups();
    // Prefer the narrowest group, then its id. Plugin order never affects the plan.
    groups.sort_by(|a, b| a.systems.len().cmp(&b.systems.len()).then(a.id.cmp(&b.id)));
    Ok(groups
        .into_iter()
        .find(|g| systems.iter().all(|s| g.systems.contains(s)))
        .map_or(Choice::Unresolved, |g| Choice::Target(g.id.clone())))
}

#[cfg(test)]
fn whole_hash(path: &Path) -> Result<String, MigrationError> {
    let mut file = fs::File::open(path).map_err(|_| read_error("file hash"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| read_error("file hash"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}

pub fn apply(
    plan: &MigrationPlan,
    readable_root: &Path,
    private_root: &Path,
    registry: &PluginRegistry,
) -> Result<MigrationReport, MigrationError> {
    apply_at(
        plan,
        readable_root,
        private_root,
        registry,
        &default_plan_path(private_root),
    )
}

fn apply_at(
    plan: &MigrationPlan,
    readable: &Path,
    private: &Path,
    registry: &PluginRegistry,
    plan_file: &Path,
) -> Result<MigrationReport, MigrationError> {
    apply_with_scan(
        plan,
        readable,
        private,
        &report_path(plan_file),
        registry,
        || {
            crate::discovery::DiscoveryCoordinator::new(readable, private)
                .with_registry_source(crate::plugin_policy::RegistrySource::Selected(
                    std::sync::Arc::new(registry.clone()),
                ))
                .rescan(&crate::discovery::DiscoveryOptions::default())
                .map_err(|_| {
                    MigrationError::Step(
                        "scan and reconcile failed; inspect local state privately".into(),
                    )
                })?;
            Ok(())
        },
    )
}

fn apply_with_scan(
    plan: &MigrationPlan,
    readable: &Path,
    private: &Path,
    report_file: &Path,
    registry: &impl MigrationRules,
    scan: impl FnOnce() -> Result<(), MigrationError>,
) -> Result<MigrationReport, MigrationError> {
    let mut report = MigrationReport {
        plan: plan.clone(),
        ..Default::default()
    };
    prepare_private_state(private)?;
    // Establish that the operator report is writable before moving anything.
    write_json(report_file, &report)?;
    let result = (|| {
        let root = readable
            .canonicalize()
            .map_err(|_| read_error("readable root"))?;
        if !plan.unresolved.is_empty() {
            return Err(step_error("plan has unresolved files"));
        }
        if &plan_with_rules(readable, private, registry)? != plan {
            return Err(step_error("approved plan changed; run dry run again"));
        }
        for movement in &plan.moves {
            validate_destination(&root, &movement.to)?;
            if !fs::symlink_metadata(&movement.from)
                .is_ok_and(|m| (m.is_file() || m.is_dir()) && !m.file_type().is_symlink())
            {
                return Err(step_error("move source is not a regular file or directory"));
            }
            if movement.from == movement.to {
                continue;
            }
            let parent = movement
                .to
                .parent()
                .ok_or_else(|| step_error("move target"))?;
            fs::create_dir_all(parent).map_err(|_| step_error("create move directory"))?;
            if movement.from.starts_with(NIX_STORE) {
                // A plugin's prepared file in the read-only store: copy it into
                // its source and leave the store untouched.
                copy_no_replace(&movement.from, &movement.to)?;
                report.copied.push(movement.clone());
            } else {
                rename_no_replace(&movement.from, &movement.to)?;
                report.moved.push(movement.clone());
            }
            write_json(report_file, &report)?;
        }
        complete(&mut report, report_file, "move-files")?;
        let mut document = read_yaml(&readable.join("device.yaml"))?;
        let new_ids: BTreeSet<_> = plan.sources.iter().map(|s| s.id.as_str()).collect();
        let owned = owned_storages(private, &document)?;
        add_sources(&mut document, &plan.sources, &owned)?;
        write_yaml(&readable.join("device.yaml"), &document)?;
        complete(&mut report, report_file, "write-sources-and-storage")?;
        let retired: OldReleases = decode_yaml(&readable.join("catalog/releases.yaml"))?;
        for file in ["catalog/games.yaml", "catalog/releases.yaml"] {
            remove_if_exists(&readable.join(file))?;
            complete(&mut report, report_file, &format!("remove-{file}"))?;
        }
        retire_old_records(&mut document, &retired, &owned, &new_ids)?;
        write_yaml(&readable.join("device.yaml"), &document)?;
        complete(&mut report, report_file, "remove-old-device-records")?;
        // Cache and repair journals name old paths and old catalog documents.
        // Never replay them into the new catalog or delete unrelated private files.
        for file in [
            "ownership.json",
            "storage-ownership.json",
            "storage-order.json",
            "hash-cache.json",
            "repair.json",
        ] {
            remove_if_exists(&private.join("game-discovery").join(file))?;
            complete(&mut report, report_file, &format!("remove-{file}"))?;
        }
        complete(&mut report, report_file, "remove-old-state")?;
        // Storage written above is discovery-owned, exactly like storage from `pair`.
        crate::discovery::reconcile::record_created_storages(
            private,
            plan.sources.iter().map(|source| source.id.clone()),
        )
        .map_err(|_| step_error("record discovery-owned storage"))?;
        complete(&mut report, report_file, "record-owned-storage")?;
        scan()?;
        complete(&mut report, report_file, "scan-and-reconcile")?;
        resolve_new_games(&mut report.plan, readable)?;
        if report.plan.kept != plan.kept || report.plan.play_logs != plan.play_logs {
            return Err(step_error(
                "published game ids differ from the approved plan",
            ));
        }
        carry_titles(&report.plan, readable, private)?;
        complete(&mut report, report_file, "carry-titles")?;
        for index in 0..report.plan.play_logs.len() {
            let log = &report.plan.play_logs[index];
            let Some(new_game) = &log.new_game else {
                return Err(step_error("play log has no unique new game"));
            };
            if new_game == &log.old_game {
                continue;
            }
            let from = log_path(private, &log.user, &log.old_game);
            let to = log_path(private, &log.user, new_game);
            let mut value: crate::host::play_log::PlayLog = decode_json(&from)?;
            value.game_id = new_game.clone();
            let bytes =
                serde_json::to_vec_pretty(&value).map_err(|_| step_error("serialize play log"))?;
            rename_no_replace(&from, &to)?;
            report.moved.push(FileMove {
                from,
                to: to.clone(),
            });
            write_json(report_file, &report)?;
            atomic_write(&to, &bytes)?;
            complete(
                &mut report,
                report_file,
                &format!("rename-play-log-{index}"),
            )?;
        }
        complete(&mut report, report_file, "rename-play-logs")?;
        Ok(())
    })();
    if let Err(error) = result {
        report.error = Some(error.to_string());
        write_json(report_file, &report)?;
        return Err(error);
    }
    complete(&mut report, report_file, "complete")?;
    Ok(report)
}

fn resolve_new_games(plan: &mut MigrationPlan, readable: &Path) -> Result<(), MigrationError> {
    let device = String::from_utf8(read_regular(&readable.join("device.yaml"))?)
        .map_err(|_| read_error("new device"))?;
    let library = String::from_utf8(read_regular(&readable.join("catalog/library.yaml"))?)
        .map_err(|_| read_error("new library"))?;
    let snapshot = crate::config::decode_config_documents(&device, &library)
        .map_err(|_| read_error("new catalog"))?;
    match_new_ids(plan, &paths_by_game(&snapshot));
    Ok(())
}

fn carry_titles(
    plan: &MigrationPlan,
    readable: &Path,
    private: &Path,
) -> Result<(), MigrationError> {
    let mut titles = BTreeMap::new();
    for game in &plan.kept {
        if let Some(id) = &game.new_id {
            titles
                .entry(id.clone())
                .or_insert_with(|| game.title.clone());
        }
    }
    crate::discovery::DiscoveryCoordinator::new(readable, private)
        .carry_titles(&titles)
        .map_err(|_| step_error("carry titles through catalog publisher"))
}

fn read_play_logs(private: &Path) -> Result<Vec<crate::host::play_log::PlayLog>, MigrationError> {
    let mut logs = vec![];
    let root = private.join("play-log");
    if !root.exists() {
        return Ok(logs);
    }
    require_directory(&root)?;
    for directory in fs::read_dir(&root).map_err(|_| read_error("play log directory"))? {
        let directory = directory.map_err(|_| read_error("play log user"))?;
        if !directory
            .file_type()
            .map_err(|_| read_error("play log user"))?
            .is_dir()
        {
            return Err(read_error("play log user directory"));
        }
        for file in fs::read_dir(directory.path()).map_err(|_| read_error("play log files"))? {
            let file = file.map_err(|_| read_error("play log file"))?;
            if !file
                .file_type()
                .map_err(|_| read_error("play log file"))?
                .is_file()
            {
                return Err(read_error("play log regular file"));
            }
            let log: crate::host::play_log::PlayLog = decode_json(&file.path())?;
            if log_path(private, &log.user_id, &log.game_id) != file.path() {
                return Err(read_error("play log identity"));
            }
            logs.push(log);
        }
    }
    Ok(logs)
}

fn read_old_state(private: &Path) -> Result<BTreeMap<String, serde_json::Value>, MigrationError> {
    let directory = private.join("game-discovery");
    if !directory.exists() {
        return Ok(BTreeMap::new());
    }
    prepare_private_state(private)?;
    let mut state = BTreeMap::new();
    for file in fs::read_dir(directory).map_err(|_| read_error("old discovery directory"))? {
        let file = file.map_err(|_| read_error("old discovery journal"))?;
        let name = file
            .file_name()
            .into_string()
            .map_err(|_| read_error("old journal name"))?;
        if file.path().extension().is_some_and(|e| e == "json")
            && name != "migration-plan.json"
            && !name.ends_with(".report.json")
        {
            state.insert(name, decode_json(&file.path())?);
        }
    }
    Ok(state)
}

fn owned_storages(private: &Path, document: &Value) -> Result<BTreeSet<String>, MigrationError> {
    #[derive(Default, Deserialize)]
    struct Owned {
        #[serde(default)]
        storages: BTreeMap<String, OwnedStorage>,
    }
    #[derive(Deserialize)]
    struct OwnedStorage {
        fingerprint: String,
    }
    let path = private.join("game-discovery/storage-ownership.json");
    let value: Owned = if path.exists() {
        decode_json(&path)?
    } else {
        Owned::default()
    };
    let mut owned = BTreeSet::new();
    for (id, record) in value.storages {
        if let Some(storage) = document.get("storage").and_then(|s| s.get(&id)) {
            if record.fingerprint == old_fingerprint(storage)? {
                owned.insert(id);
            }
        }
    }
    Ok(owned)
}

fn old_fingerprint(value: &Value) -> Result<String, MigrationError> {
    // Exact old discovery fingerprint: SHA-256 of the serialized YAML record.
    let bytes = serde_yaml::to_string(value).map_err(|_| read_error("old storage record"))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes.as_bytes())))
}

fn validate_destination(root: &Path, target: &Path) -> Result<(), MigrationError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| step_error("move target is outside readable root"))?;
    safe_relative(relative)?;
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(step_error("move target contains a symlink"))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(step_error("move target is unavailable")),
        }
    }
    Ok(())
}

fn require_directory(path: &Path) -> Result<(), MigrationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| read_error("private directory"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(read_error("private directory is not a regular directory"));
    }
    Ok(())
}

fn read_regular(path: &Path) -> Result<Vec<u8>, MigrationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| read_error("input file"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(read_error("input is not a regular file"));
    }
    fs::read(path).map_err(|_| read_error("input file"))
}

fn safe_relative(path: &Path) -> Result<(), MigrationError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
    {
        return Err(read_error("unsafe relative path"));
    }
    Ok(())
}
fn log_path(private: &Path, user: &str, game: &str) -> PathBuf {
    use crate::host::play_log::encode_uri_component;
    private
        .join("play-log")
        .join(encode_uri_component(user))
        .join(format!("{}.json", encode_uri_component(game)))
}
fn prepare_private_state(private: &Path) -> Result<(), MigrationError> {
    crate::host::play_log::ensure_private_directory(&private.join("game-discovery"))
        .map_err(|_| step_error("private discovery directory must be a private regular directory"))
}
fn default_plan_path(private: &Path) -> PathBuf {
    private.join("game-discovery/migration-plan.json")
}
fn report_path(plan: &Path) -> PathBuf {
    plan.with_extension("report.json")
}
fn read_error(context: &str) -> MigrationError {
    MigrationError::Read(context.into())
}
fn step_error(context: &str) -> MigrationError {
    MigrationError::Step(context.into())
}
fn decode_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, MigrationError> {
    serde_yaml::from_slice(&read_regular(path)?).map_err(|_| read_error("YAML shape"))
}
fn decode_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, MigrationError> {
    serde_json::from_slice(&read_regular(path)?).map_err(|_| read_error("JSON shape"))
}
fn read_yaml(path: &Path) -> Result<Value, MigrationError> {
    decode_yaml(path)
}
fn section<'a>(
    document: &'a mut Value,
    name: &str,
) -> Result<&'a mut serde_yaml::Mapping, MigrationError> {
    let map = document
        .as_mapping_mut()
        .ok_or_else(|| read_error("document map"))?;
    map.entry(Value::String(name.into()))
        .or_insert_with(|| Value::Mapping(Default::default()))
        .as_mapping_mut()
        .ok_or_else(|| read_error("section map"))
}
fn write_yaml(path: &Path, value: &Value) -> Result<(), MigrationError> {
    atomic_write(
        path,
        serde_yaml::to_string(value)
            .map_err(|_| step_error("serialize YAML"))?
            .as_bytes(),
    )
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), MigrationError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| step_error("serialize JSON"))?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), MigrationError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| step_error("create new file (destination may already exist)"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| step_error("write file"))
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), MigrationError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|_| step_error("create document directory"))?;
    let temporary = parent.join(format!(".migration-{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        write_new(&temporary, bytes)?;
        fs::rename(&temporary, path).map_err(|_| step_error("publish document"))?;
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| step_error("sync document directory"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
const NIX_STORE: &str = "/nix/store";

/// Copies one regular file to a new name, never replacing an existing file.
/// The copy is written beside the target and then renamed into place.
fn copy_no_replace(from: &Path, to: &Path) -> Result<(), MigrationError> {
    let metadata = fs::symlink_metadata(from).map_err(|_| step_error("copy source"))?;
    if !metadata.is_file() {
        return Err(step_error(
            "only regular files are copied out of the Nix store",
        ));
    }
    let parent = to.parent().ok_or_else(|| step_error("copy target"))?;
    let temporary = parent.join(format!(
        ".migration-copy-{:016x}.tmp",
        rand::random::<u64>()
    ));
    let result = (|| {
        let mut source = fs::File::open(from).map_err(|_| step_error("open copy source"))?;
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&temporary)
            .map_err(|_| step_error("create copy"))?;
        std::io::copy(&mut source, &mut target).map_err(|_| step_error("write copy"))?;
        target.sync_all().map_err(|_| step_error("sync copy"))?;
        rename_no_replace(&temporary, to)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn rename_no_replace(from: &Path, to: &Path) -> Result<(), MigrationError> {
    use std::os::unix::ffi::OsStrExt;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())
        .map_err(|_| step_error("move source"))?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())
        .map_err(|_| step_error("move destination"))?;
    // SAFETY: valid, live C strings; renameat2 retains neither pointer.
    let result = crate::rename::renameat2(
        libc::AT_FDCWD,
        &from,
        libc::AT_FDCWD,
        &to,
        libc::RENAME_NOREPLACE as _,
    );
    if result != 0 {
        return Err(step_error(
            "rename refused (destination exists, filesystem differs, or file is unavailable)",
        ));
    }
    Ok(())
}
fn remove_if_exists(path: &Path) -> Result<(), MigrationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(step_error("remove old file")),
    }
}
fn complete(report: &mut MigrationReport, path: &Path, step: &str) -> Result<(), MigrationError> {
    report.completed_steps.push(step.into());
    write_json(path, report)
}

fn summary(plan: &MigrationPlan) -> String {
    format!("Migration plan: {} kept games, {} dropped entries, {} moves, {} sources, {} unresolved files, {} play logs.",plan.kept.len(),plan.dropped.len(),plan.moves.len(),plan.sources.len(),plan.unresolved.len(),plan.play_logs.len())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputKind {
    Plan,
    Report,
}

/// A plan or report never lands among device data: not under the readable
/// root (games, accounts, prepared installations), not under any source or
/// move folder, and it replaces an existing file only when that file is
/// already a plan or report of the same kind.
fn validate_plan_output(
    path: &Path,
    plan: &MigrationPlan,
    readable: &Path,
    private: &Path,
    kind: OutputKind,
) -> Result<(), MigrationError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| step_error("plan output directory"))?
            .join(path)
    };
    // Resolve the existing prefix, including symlinks and `..`, even for a new file.
    let mut prefix = absolute.as_path();
    let mut suffix = Vec::new();
    while !prefix.exists() {
        suffix.push(
            prefix
                .file_name()
                .ok_or_else(|| step_error("plan output path"))?,
        );
        prefix = prefix
            .parent()
            .ok_or_else(|| step_error("plan output directory"))?;
    }
    let mut target = prefix
        .canonicalize()
        .map_err(|_| step_error("plan output path"))?;
    for part in suffix.into_iter().rev() {
        target.push(part);
    }
    let state = private.join("game-discovery");
    let reserved = [
        readable.join("device.yaml"),
        readable.join("catalog/games.yaml"),
        readable.join("catalog/releases.yaml"),
        readable.join("catalog/library.yaml"),
        state.join("ownership.json"),
        state.join("storage-ownership.json"),
        state.join("storage-order.json"),
        state.join("hash-cache.json"),
        state.join("repair.json"),
        state.join("attention.json"),
    ];
    if reserved.contains(&target)
        || target.starts_with(private.join("play-log"))
        || plan
            .moves
            .iter()
            .any(|m| target.starts_with(&m.from) || target.starts_with(&m.to))
        || plan
            .kept
            .iter()
            .flat_map(|game| &game.files)
            .any(|file| file == &target)
        || plan
            .dropped
            .iter()
            .any(|d| d.file.as_ref() == Some(&target))
        || plan.unresolved.iter().any(|p| Path::new(p) == target)
        || target.starts_with(readable)
        || readable
            .canonicalize()
            .is_ok_and(|root| target.starts_with(root))
        || plan.sources.iter().any(|s| target.starts_with(&s.root))
        || plan
            .moves
            .iter()
            .filter_map(|m| m.from.parent())
            .any(|folder| target.starts_with(folder))
    {
        return Err(step_error("plan output would overwrite device data"));
    }
    if let Ok(metadata) = fs::symlink_metadata(&target) {
        let replaceable = metadata.is_file()
            && match kind {
                OutputKind::Plan => decode_json::<MigrationPlan>(&target).is_ok(),
                OutputKind::Report => decode_json::<MigrationReport>(&target).is_ok(),
            };
        if !replaceable {
            return Err(step_error(
                "plan output would replace a file that is not a migration plan or report",
            ));
        }
    }
    Ok(())
}

/// Offline CLI. Only the plan's narrow data is printed, never device settings
/// or parser errors that can quote credentials from the configuration.
pub fn run(
    arguments: &[std::ffi::OsString],
    readable: Option<&std::ffi::OsStr>,
    private: Option<&std::ffi::OsStr>,
    registry_source: crate::plugin_policy::RegistrySource,
) -> Result<String, String> {
    let mut act = false;
    let mut plan_file = None;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--apply" && !act {
            act = true;
        } else if argument == "--plan" && plan_file.is_none() {
            plan_file = Some(PathBuf::from(
                arguments
                    .next()
                    .filter(|p| !p.is_empty() && !p.to_string_lossy().starts_with("--"))
                    .ok_or("--plan requires a file")?,
            ));
        } else {
            return Err("usage: korrid catalog migrate [--apply] [--plan <file>]".into());
        }
    }
    let root = |value: Option<&std::ffi::OsStr>, name: &str| -> Result<PathBuf, String> {
        let path = PathBuf::from(
            value
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("{name} must name an existing directory"))?,
        );
        let path = path
            .canonicalize()
            .map_err(|_| format!("{name} must name an existing directory"))?;
        if !path.is_dir() {
            return Err(format!("{name} must name an existing directory"));
        }
        Ok(path)
    };
    let readable = root(readable, "KORRID_STORAGE_ROOT")?;
    let private = root(private, "KORRID_PRIVATE_STATE_ROOT")?;
    let plan_label = if plan_file.is_some() {
        "selected --plan file"
    } else {
        "<private>/game-discovery/migration-plan.json"
    };
    let plan_file = plan_file.unwrap_or_else(|| default_plan_path(&private));
    let registry = registry_source
        .registry()
        .map_err(|_| "enabled plugin registry could not be loaded".to_owned())?;
    let fresh = plan(&readable, &private, &registry).map_err(|e| e.to_string())?;
    validate_plan_output(&plan_file, &fresh, &readable, &private, OutputKind::Plan)
        .map_err(|e| e.to_string())?;
    validate_plan_output(
        &report_path(&plan_file),
        &fresh,
        &readable,
        &private,
        OutputKind::Report,
    )
    .map_err(|e| e.to_string())?;
    prepare_private_state(&private).map_err(|e| e.to_string())?;
    let plan = if act {
        // Apply only the plan the operator reviewed; never replace it first.
        let reviewed: MigrationPlan = decode_json(&plan_file)
            .map_err(|_| "no reviewed plan found; run a dry run first".to_owned())?;
        if reviewed != fresh {
            return Err(
                "the reviewed plan no longer matches the device; run a dry run again".into(),
            );
        }
        reviewed
    } else {
        write_json(&plan_file, &fresh).map_err(|e| e.to_string())?;
        fresh
    };
    let mut output = format!("{}\nPlan file: {plan_label}", summary(&plan));
    if act {
        let report = apply_at(&plan, &readable, &private, &registry, &plan_file).map_err(|e| {
            format!("{e}; migration report records completed steps beside the plan")
        })?;
        output.push_str(&format!(
            "\nApplied: {} completed steps.",
            report.completed_steps.len()
        ));
    } else {
        output.push_str("\nDry run only. Back up device data before --apply.");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{ContentRules, DeclaredGame, DeclaredRelease, EvidenceRecord, GroupRecord};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Rules {
        systems: Vec<ContentRules>,
        groups: Vec<GroupRecord>,
        games: BTreeMap<String, DeclaredGame>,
    }

    impl MigrationRules for Rules {
        fn target(&self, id: &str) -> Option<crate::plugin::PairingTarget<'_>> {
            if let Some(rule) = self.systems.iter().find(|rule| rule.system == id) {
                return Some(crate::plugin::PairingTarget::System(rule));
            }
            self.groups
                .iter()
                .find(|group| group.id == id)
                .map(|group| crate::plugin::PairingTarget::Group {
                    group,
                    members: self
                        .systems
                        .iter()
                        .filter(|rule| group.systems.contains(&rule.system))
                        .collect(),
                })
        }
        fn declared_games(&self) -> &BTreeMap<String, DeclaredGame> {
            &self.games
        }
        fn systems(&self) -> Vec<&ContentRules> {
            self.systems.iter().collect()
        }
        fn groups(&self) -> Vec<&GroupRecord> {
            self.groups.iter().collect()
        }
        fn declared(&self, hash: &str) -> Vec<&DeclaredRelease> {
            self.games
                .values()
                .flat_map(|g| g.releases.values())
                .filter(|r| r.hashes.iter().any(|h| h == hash))
                .collect()
        }
        fn hashes(
            &self,
            path: &Path,
            _: &Path,
            _: &[PathBuf],
            _: &ContentRules,
        ) -> Result<Vec<String>, MigrationError> {
            Ok(vec![whole_hash(path)?])
        }
    }

    fn rules() -> Rules {
        Rules {
            systems: [
                ("gba", "gba"),
                ("snes", "sfc"),
                ("megadrive", "md"),
                ("lynx", "lnx"),
            ]
            .into_iter()
            .map(|(system, ext)| ContentRules {
                system: system.into(),
                extensions: vec![ext.into()],
                layouts: vec!["**/{title}.{ext}".into()],
                ..Default::default()
            })
            .collect(),
            ..Default::default()
        }
    }

    fn put(path: &Path, bytes: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn mini(root: &Path, private: &Path) -> Rules {
        prepare_private_state(private).unwrap();
        let mut rules = rules();
        let files = [
            "mixed/Wario.gba",
            "mixed/Mario.sfc",
            "mixed/Sonic.md",
            "mixed/Lynx.lnx",
            "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/prepared-arm-x/a.o",
            "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/prepared-arm-x/b.o",
            "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/prepared-arm-x/README.md",
            "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/old/MASTER.DAT",
        ];
        let mut games = BTreeMap::new();
        let mut releases = indexmap::IndexMap::new();
        let mut locations = BTreeMap::new();
        for (index, file) in files.into_iter().enumerate() {
            let game = format!("old-{index}");
            let release = format!("release-{index}");
            put(&root.join(file), file.as_bytes());
            games.insert(game.clone(), serde_json::json!({"title": format!("Carried {index}"), "releases": [release.clone()]}));
            releases.insert(
                release.clone(),
                serde_json::json!({"game": game, "system": "old"}),
            );
            locations.insert(release, serde_json::json!([{"storage":"old","path":file,"discovery":{"first-seen-at":"2026-01-01T00:00:00Z"}}]));
        }
        // An alias of the same physical file is a duplicate, not another move.
        games.insert(
            "old-duplicate".into(),
            serde_json::json!({"title":"Duplicate", "releases":["duplicate"]}),
        );
        releases.insert(
            "duplicate".into(),
            serde_json::json!({"game":"old-duplicate", "system":"old"}),
        );
        locations.insert(
            "duplicate".into(),
            serde_json::json!([{"storage":"old","path":"mixed/Wario.gba"}]),
        );
        put(&root.join("device.yaml"), serde_yaml::to_string(&serde_json::json!({"host":{"title":"DO-NOT-PRINT"},"storage":{"old":{"root":root}},"locations":locations})).unwrap());
        put(
            &root.join("catalog/games.yaml"),
            serde_yaml::to_string(&serde_json::json!({"games":games})).unwrap(),
        );
        put(
            &root.join("catalog/releases.yaml"),
            serde_yaml::to_string(&serde_json::json!({"releases":releases})).unwrap(),
        );
        let device = read_yaml(&root.join("device.yaml")).unwrap();
        put(&private.join("game-discovery/storage-ownership.json"), serde_json::to_vec(&serde_json::json!({"storages":{"old":{"root":root,"fingerprint":old_fingerprint(&device["storage"]["old"]).unwrap()}}})).unwrap());
        put(
            &private.join("game-discovery/ownership.json"),
            r#"{"releases":{},"locations":{}}"#,
        );
        put(
            &private.join("game-discovery/hash-cache.json"),
            r#"{"entries":{}}"#,
        );
        for (user, game) in [("person/one", "old-0"), ("person-two", "old-1")] {
            put(
                &log_path(private, user, game),
                serde_json::to_vec(&crate::host::play_log::PlayLog {
                    user_id: user.into(),
                    game_id: game.into(),
                    entries: vec![],
                })
                .unwrap(),
            );
        }
        rules.games.insert(
            "@test:fallout/fallout".into(),
            DeclaredGame {
                provider: "@test:fallout".into(),
                key: "fallout".into(),
                title: "Fallout".into(),
                releases: indexmap::IndexMap::from([(
                    "prepared".into(),
                    DeclaredRelease {
                        system: None,
                        hashes: vec![
                            whole_hash(&root.join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/old/MASTER.DAT")).unwrap()
                        ],
                        artifacts: Vec::new(),
                        file: Some(crate::plugin::DeclaredReleaseFile::AccountRoot(
                            "fallout-ce/MASTER.DAT".into(),
                        )),
                    },
                )]),
            },
        );
        rules
    }

    fn real_registry(rules: &Rules) -> PluginRegistry {
        let systems: BTreeMap<_, _> = rules.systems.iter().map(|rule| (rule.system.clone(), serde_json::json!({"content":{"extensions":rule.extensions,"layout":rule.layouts}}))).collect();
        let games: BTreeMap<_, _> = rules
            .games
            .values()
            .map(|game| {
                let releases: BTreeMap<_, _> = game
                    .releases
                    .iter()
                    .map(|(key, release)| {
                        let mut value = serde_json::json!({"hashes":release.hashes});
                        match &release.file {
                            Some(DeclaredReleaseFile::AccountRoot(file)) => {
                                value["file"] = file.clone().into();
                            }
                            Some(DeclaredReleaseFile::Artifact(file)) => {
                                value["artifacts"] = release.artifacts.clone().into();
                                value["file"] = serde_json::json!({
                                    "artifact": file.artifact,
                                    "path": file.path,
                                });
                            }
                            Some(DeclaredReleaseFile::Package(file)) => {
                                value["file"] = serde_json::json!({
                                    "packageFile": file.package_file, "path": file.path,
                                });
                            }
                            None => (),
                        }
                        if let Some(system) = &release.system {
                            value["system"] = system.clone().into();
                        }
                        (key, value)
                    })
                    .collect();
                (
                    game.key.clone(),
                    serde_json::json!({"title":game.title,"releases":releases}),
                )
            })
            .collect();
        let source = format!(
            "export const name = 'fallout'; export const systems = {}; export const games = {};",
            serde_json::to_string(&systems).unwrap(),
            serde_json::to_string(&games).unwrap()
        );
        PluginRegistry::new(
            vec![crate::plugin::load_plugin_source("@test", &source).unwrap()],
            ["@test:fallout".into()],
        )
        .unwrap()
    }

    fn prepared_fixture(root: &Path, private: &Path, old_file: &str, declared_file: &str) -> Rules {
        let mut rules = mini(root, private);
        let old = root.join(
            "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/old/MASTER.DAT",
        );
        let from = root
            .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9")
            .join(old_file);
        if from != old {
            fs::create_dir_all(from.parent().unwrap()).unwrap();
            fs::rename(old, &from).unwrap();
        }
        let mut device = read_yaml(&root.join("device.yaml")).unwrap();
        device.as_mapping_mut().unwrap().remove("host");
        device["locations"]["release-7"][0]["path"] = format!(
            "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/{old_file}"
        )
        .into();
        write_yaml(&root.join("device.yaml"), &device).unwrap();
        rules
            .games
            .values_mut()
            .next()
            .unwrap()
            .releases
            .get_mut("prepared")
            .unwrap()
            .file = Some(crate::plugin::DeclaredReleaseFile::AccountRoot(
            declared_file.into(),
        ));
        rules
    }

    #[test]
    fn prepared_migration_moves_the_whole_installation_and_keeps_native_companions() {
        for (old_file, declared, old_directory, new_directory) in [
            (
                "old/MASTER.DAT",
                "fallout-ce/MASTER.DAT",
                "old",
                "fallout-ce",
            ),
            (
                "build/out/jak1/iso/GAME.CGO",
                "opengoal/jak1/iso/GAME.CGO",
                "build/out",
                "opengoal",
            ),
        ] {
            let root = tempfile::tempdir().unwrap();
            let private = tempfile::tempdir().unwrap();
            let rules = prepared_fixture(root.path(), private.path(), old_file, declared);
            let account = root
                .path()
                .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9");
            for companion in ["CRITTER.DAT", "DATA/companion", "fallout.cfg", "saves/save"] {
                put(&account.join(old_directory).join(companion), companion);
            }
            let registry = real_registry(&rules);
            let plan = plan(root.path(), private.path(), &registry).unwrap();
            assert!(
                plan.moves.contains(&FileMove {
                    from: account.join(old_directory),
                    to: account.join(new_directory)
                }),
                "the installation, not its identity file, must move"
            );
            apply(&plan, root.path(), private.path(), &registry).unwrap();
            assert!(!account.join(old_directory).exists());
            assert!(account.join(declared).is_file());
            for companion in ["CRITTER.DAT", "DATA/companion", "fallout.cfg", "saves/save"] {
                assert_eq!(
                    fs::read(account.join(new_directory).join(companion)).unwrap(),
                    companion.as_bytes()
                );
            }
        }
    }

    #[test]
    fn prepared_migration_refuses_empty_prefix_existing_target_and_shared_installation() {
        for case in ["empty", "existing", "shared"] {
            let root = tempfile::tempdir().unwrap();
            let private = tempfile::tempdir().unwrap();
            let (old, declared) = match case {
                "empty" => ("MASTER.DAT", "fallout-ce/MASTER.DAT"),
                "existing" => ("old/MASTER.DAT", "fallout-ce/MASTER.DAT"),
                _ => ("shared/one/MASTER.DAT", "first/one/MASTER.DAT"),
            };
            let mut rules = prepared_fixture(root.path(), private.path(), old, declared);
            let account = root
                .path()
                .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9");
            if case == "existing" {
                put(
                    &account.join("fallout-ce/CRITTER.DAT"),
                    b"existing companion",
                );
            }
            if case == "shared" {
                let second = account.join("shared/two/MASTER.DAT");
                put(&second, b"second prepared release");
                let mut games = read_yaml(&root.path().join("catalog/games.yaml")).unwrap();
                games["games"]["old-second"] = serde_yaml::to_value(
                    serde_json::json!({"title":"Second", "releases":["second"]}),
                )
                .unwrap();
                write_yaml(&root.path().join("catalog/games.yaml"), &games).unwrap();
                let mut releases = read_yaml(&root.path().join("catalog/releases.yaml")).unwrap();
                releases["releases"]["second"] =
                    serde_yaml::to_value(serde_json::json!({"game":"old-second"})).unwrap();
                write_yaml(&root.path().join("catalog/releases.yaml"), &releases).unwrap();
                let mut device = read_yaml(&root.path().join("device.yaml")).unwrap();
                device["locations"]["second"] = serde_yaml::to_value(serde_json::json!([{"storage":"old","path":"users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/shared/two/MASTER.DAT"}])).unwrap();
                write_yaml(&root.path().join("device.yaml"), &device).unwrap();
                rules.games.insert(
                    "@test:fallout/second".into(),
                    DeclaredGame {
                        provider: "@test:fallout".into(),
                        key: "second".into(),
                        title: "Second".into(),
                        releases: indexmap::IndexMap::from([(
                            "prepared".into(),
                            DeclaredRelease {
                                system: None,
                                hashes: vec![whole_hash(&second).unwrap()],
                                artifacts: Vec::new(),
                                file: Some(crate::plugin::DeclaredReleaseFile::AccountRoot(
                                    "second/two/MASTER.DAT".into(),
                                )),
                            },
                        )]),
                    },
                );
            }
            let before = fs::read(root.path().join("device.yaml")).unwrap();
            let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
            assert!(!plan.unresolved.is_empty(), "{case}");
            assert!(
                plan.moves
                    .iter()
                    .all(|movement| !movement.from.starts_with(&account)),
                "{case}"
            );
            assert!(apply_with_scan(
                &plan,
                root.path(),
                private.path(),
                &report_path(&default_plan_path(private.path())),
                &rules,
                || panic!("must not scan unresolved plans")
            )
            .is_err());
            assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), before);
            assert!(account.join(old).is_file());
            assert!(root.path().join("mixed/Wario.gba").is_file());
        }
    }

    #[test]
    fn plan_output_cannot_overwrite_a_prepared_file_that_needs_no_move() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = prepared_fixture(
            root.path(),
            private.path(),
            "fallout-ce/MASTER.DAT",
            "fallout-ce/MASTER.DAT",
        );
        let registry = real_registry(&rules);
        let file = root.path().join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/fallout-ce/MASTER.DAT");
        let before = fs::read(&file).unwrap();
        assert!(run(
            &["--plan".into(), file.clone().into_os_string()],
            Some(root.path().as_os_str()),
            Some(private.path().as_os_str()),
            crate::plugin_policy::RegistrySource::Selected(std::sync::Arc::new(registry))
        )
        .is_err());
        assert_eq!(fs::read(file).unwrap(), before);
    }

    #[test]
    fn store_files_are_copied_without_replacing_and_leave_the_source() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("read-only/cart.p8.png");
        put(&source, b"cartridge");
        let mut readonly = fs::metadata(&source).unwrap().permissions();
        readonly.set_readonly(true);
        fs::set_permissions(&source, readonly).unwrap();
        let target = root.path().join("games/pico8/cart.p8.png");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        copy_no_replace(&source, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"cartridge");
        assert_eq!(fs::read(&source).unwrap(), b"cartridge");
        assert!(!fs::metadata(&target).unwrap().permissions().readonly());
        put(&target, b"player edit");
        assert!(copy_no_replace(&source, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"player edit");
        assert_eq!(
            fs::read_dir(target.parent().unwrap()).unwrap().count(),
            1,
            "no temporary copy is left behind"
        );
        assert!(copy_no_replace(root.path(), &root.path().join("games/dir")).is_err());
        assert!(Path::new("/nix/store/abc-x/cart.p8.png").starts_with(NIX_STORE));
        assert!(!Path::new("/nix/storeish/cart.p8.png").starts_with(NIX_STORE));
    }

    #[test]
    fn plan_output_cannot_overwrite_companions_saves_or_unrelated_files() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = prepared_fixture(
            root.path(),
            private.path(),
            "fallout-ce/MASTER.DAT",
            "fallout-ce/MASTER.DAT",
        );
        let install = root.path().join(
            "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/fallout-ce",
        );
        put(&install.join("CRITTER.DAT"), b"companion");
        put(&install.join("data/SAVEGAME/SLOT01/SAVE.DAT"), b"save");
        let outside = tempfile::tempdir().unwrap();
        let unrelated = outside.path().join("notes.json");
        put(&unrelated, b"{\"mine\":true}");
        let registry = crate::plugin_policy::RegistrySource::Selected(std::sync::Arc::new(
            real_registry(&rules),
        ));
        for target in [
            install.join("CRITTER.DAT"),
            install.join("data/SAVEGAME/SLOT01/SAVE.DAT"),
            install.join("new-plan.json"),
            root.path().join("games/plan.json"),
            unrelated.clone(),
        ] {
            let before = fs::read(&target).ok();
            assert!(
                run(
                    &["--plan".into(), target.clone().into_os_string()],
                    Some(root.path().as_os_str()),
                    Some(private.path().as_os_str()),
                    registry.clone(),
                )
                .is_err(),
                "plan output must be refused"
            );
            assert_eq!(fs::read(&target).ok(), before);
        }
        // An earlier plan at an operator path outside device data may be replaced.
        let chosen = outside.path().join("plan.json");
        for _ in 0..2 {
            run(
                &["--plan".into(), chosen.clone().into_os_string()],
                Some(root.path().as_os_str()),
                Some(private.path().as_os_str()),
                registry.clone(),
            )
            .unwrap();
        }
        assert!(decode_json::<MigrationPlan>(&chosen).is_ok());
    }

    #[test]
    fn cli_apply_uses_the_reviewed_plan_and_refuses_drift() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let mut device = read_yaml(&root.path().join("device.yaml")).unwrap();
        device.as_mapping_mut().unwrap().remove("host");
        write_yaml(&root.path().join("device.yaml"), &device).unwrap();
        let registry = crate::plugin_policy::RegistrySource::Selected(std::sync::Arc::new(
            real_registry(&rules),
        ));
        let execute = |args: &[&str]| {
            run(
                &args
                    .iter()
                    .map(std::ffi::OsString::from)
                    .collect::<Vec<_>>(),
                Some(root.path().as_os_str()),
                Some(private.path().as_os_str()),
                registry.clone(),
            )
        };
        // No reviewed plan yet: apply refuses and moves nothing.
        assert!(execute(&["--apply"]).is_err());
        assert!(root.path().join("mixed/Wario.gba").is_file());
        execute(&[]).unwrap();
        let reviewed = fs::read(default_plan_path(private.path())).unwrap();
        // The device changes after the review.
        put(&root.path().join("mixed/Extra.gba"), b"extra");
        let mut device = read_yaml(&root.path().join("device.yaml")).unwrap();
        device["locations"]["release-extra"] =
            serde_yaml::to_value(serde_json::json!([{"storage":"old","path":"mixed/Extra.gba"}]))
                .unwrap();
        write_yaml(&root.path().join("device.yaml"), &device).unwrap();
        let mut games: Value = read_yaml(&root.path().join("catalog/games.yaml")).unwrap();
        games["games"]["old-extra"] =
            serde_yaml::to_value(serde_json::json!({"title":"Extra","releases":["release-extra"]}))
                .unwrap();
        write_yaml(&root.path().join("catalog/games.yaml"), &games).unwrap();
        let mut releases: Value = read_yaml(&root.path().join("catalog/releases.yaml")).unwrap();
        releases["releases"]["release-extra"] =
            serde_yaml::to_value(serde_json::json!({"game":"old-extra","system":"old"})).unwrap();
        write_yaml(&root.path().join("catalog/releases.yaml"), &releases).unwrap();
        assert!(execute(&["--apply"]).is_err());
        assert_eq!(
            fs::read(default_plan_path(private.path())).unwrap(),
            reviewed
        );
        assert!(root.path().join("mixed/Wario.gba").is_file());
        assert!(root.path().join("catalog/games.yaml").is_file());
    }

    #[test]
    fn prepared_installation_already_at_its_declared_path_needs_no_move() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = prepared_fixture(
            root.path(),
            private.path(),
            "fallout-ce/MASTER.DAT",
            "fallout-ce/MASTER.DAT",
        );
        let registry = real_registry(&rules);
        let plan = plan(root.path(), private.path(), &registry).unwrap();
        assert!(plan.unresolved.is_empty());
        assert!(plan
            .moves
            .iter()
            .all(|movement| !movement.from.starts_with(root.path().join("users"))));
        let report = apply(&plan, root.path(), private.path(), &registry).unwrap();
        assert!(report
            .plan
            .kept
            .iter()
            .find(|game| game.old_id == "old-7")
            .unwrap()
            .new_id
            .is_some());
    }

    fn descriptor_fixture(root: &Path, primary: &str) -> Rules {
        let storage = root.join("roms");
        put(
            &storage.join("set/Game.cue"),
            b"FILE \"tracks/one.bin\" BINARY\nFILE \"tracks/two.bin\" BINARY\n",
        );
        put(&storage.join("set/tracks/one.bin"), b"track one");
        put(&storage.join("set/tracks/two.bin"), b"track two");
        if primary.ends_with("m3u") {
            put(&storage.join("set/Game.m3u"), b"Game.cue\n");
        }
        put(&root.join("device.yaml"), serde_yaml::to_string(&serde_json::json!({"storage":{"old":{"root":storage}},"locations":{"disc-release":[{"storage":"old","path":format!("set/{primary}")}]}})).unwrap());
        put(
            &root.join("catalog/games.yaml"),
            b"games:\n  old-disc:\n    title: Carried Disc\n    releases: [disc-release]\n",
        );
        put(
            &root.join("catalog/releases.yaml"),
            b"releases:\n  disc-release:\n    game: old-disc\n",
        );
        Rules {
            systems: vec![ContentRules {
                system: "psx".into(),
                extensions: vec!["cue".into(), "m3u".into(), "bin".into()],
                layouts: vec!["**/{title}.{ext}".into()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn descriptor_migration_moves_cue_tracks_and_nested_playlist_dependencies_as_one_unit() {
        for primary in ["Game.cue", "Game.m3u"] {
            let root = tempfile::tempdir().unwrap();
            let private = tempfile::tempdir().unwrap();
            let rules = descriptor_fixture(root.path(), primary);
            let registry = real_registry(&rules);
            if primary.ends_with("m3u") {
                let mut device = read_yaml(&root.path().join("device.yaml")).unwrap();
                device["locations"]["disc-release"][0] = serde_yaml::to_value(serde_json::json!({"storage":"old", "root":"set", "files":[{"path":"Game.m3u", "role":"descriptor"}]})).unwrap();
                write_yaml(&root.path().join("device.yaml"), &device).unwrap();
            }
            let plan = plan(root.path(), private.path(), &registry).unwrap();
            let mut members = vec!["set/Game.cue", "set/tracks/one.bin", "set/tracks/two.bin"];
            if primary.ends_with("m3u") {
                members.push("set/Game.m3u");
            }
            assert_eq!(
                plan.moves.len(),
                members.len(),
                "a descriptor-only legacy record still owns its complete dependency closure"
            );
            let report = apply(&plan, root.path(), private.path(), &registry).unwrap();
            let device: OldDevice = decode_yaml(&root.path().join("device.yaml")).unwrap();
            let location = device.locations.values().next().unwrap().first().unwrap();
            let storage = &device.storage[location.storage.as_ref().unwrap()].root;
            let paths: BTreeSet<_> = location
                .files
                .as_ref()
                .unwrap()
                .iter()
                .map(|part| {
                    storage
                        .join(location.root.as_deref().unwrap_or(Path::new("")))
                        .join(&part.path)
                })
                .collect();
            for member in members {
                let moved = root.path().join("games/psx").join(member);
                assert!(moved.is_file());
                assert!(!root.path().join("roms").join(member).exists());
                assert!(paths.contains(&moved));
            }
            assert!(report.plan.kept[0].new_id.is_some());
        }
    }

    #[test]
    fn descriptor_owners_precede_legacy_track_entries_regardless_of_old_id_order() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = descriptor_fixture(root.path(), "Game.m3u");
        let mut games = read_yaml(&root.path().join("catalog/games.yaml")).unwrap();
        games["games"]["a-track"] = serde_yaml::to_value(
            serde_json::json!({"title":"Not a separate game", "releases":["track-release"]}),
        )
        .unwrap();
        write_yaml(&root.path().join("catalog/games.yaml"), &games).unwrap();
        let mut releases = read_yaml(&root.path().join("catalog/releases.yaml")).unwrap();
        releases["releases"]["track-release"] =
            serde_yaml::to_value(serde_json::json!({"game":"a-track"})).unwrap();
        write_yaml(&root.path().join("catalog/releases.yaml"), &releases).unwrap();
        let mut device = read_yaml(&root.path().join("device.yaml")).unwrap();
        device["locations"]["track-release"] = serde_yaml::to_value(
            serde_json::json!([{"storage":"old", "path":"set/tracks/one.bin"}]),
        )
        .unwrap();
        write_yaml(&root.path().join("device.yaml"), &device).unwrap();
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        assert_eq!(plan.kept.len(), 1);
        assert_eq!(plan.kept[0].old_id, "old-disc");
        assert_eq!(plan.moves.len(), 4);
        assert!(plan
            .dropped
            .iter()
            .any(|entry| entry.old_id == "a-track" && entry.reason == DropReason::Duplicate));
    }

    #[test]
    fn prospective_descriptor_ranking_uses_the_storage_boundary_and_preflights_before_hashing() {
        for missing in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let private = tempfile::tempdir().unwrap();
            descriptor_fixture(root.path(), "Game.m3u");
            fs::rename(
                root.path().join("roms/set/tracks"),
                root.path().join("roms/tracks"),
            )
            .unwrap();
            put(
                &root.path().join("roms/set/Game.cue"),
                b"FILE \"../tracks/one.bin\" BINARY\nFILE \"../tracks/two.bin\" BINARY\n",
            );
            if missing {
                fs::remove_file(root.path().join("roms/tracks/two.bin")).unwrap();
            }
            let plugin = crate::plugin::load_plugin_source(
                "@test",
                r#"export const name = "discs"; export const systems = {
                psx:{content:{extensions:["cue","m3u","bin"],evidence:{names:["^Game"]}}},
                saturn:{content:{extensions:["cue","m3u","bin"]}}
            };"#,
            )
            .unwrap();
            let registry = PluginRegistry::new(vec![plugin], ["@test:discs".into()]).unwrap();
            let plan = plan(root.path(), private.path(), &registry).unwrap();
            if missing {
                assert!(!plan.unresolved.is_empty());
                assert!(plan.moves.is_empty());
            } else {
                assert!(plan.unresolved.is_empty());
                assert!(plan.kept[0].new_id.is_some());
                apply(&plan, root.path(), private.path(), &registry).unwrap();
                assert!(root.path().join("games/psx/tracks/two.bin").is_file());
            }
        }
    }

    #[test]
    fn unsafe_descriptor_closures_are_unresolved_without_partial_moves() {
        for case in [
            "missing", "not-file", "escape", "cycle", "oversize", "symlink",
        ] {
            let root = tempfile::tempdir().unwrap();
            let private = tempfile::tempdir().unwrap();
            let rules = descriptor_fixture(root.path(), "Game.m3u");
            let track = root.path().join("roms/set/tracks/two.bin");
            match case {
                "missing" => fs::remove_file(&track).unwrap(),
                "not-file" => {
                    fs::remove_file(&track).unwrap();
                    fs::create_dir(&track).unwrap();
                }
                "escape" => put(
                    &root.path().join("roms/set/Game.cue"),
                    b"FILE \"../../outside.bin\" BINARY\n",
                ),
                "oversize" => put(&root.path().join("roms/set/Game.m3u"), vec![b'#'; 65_537]),
                "symlink" => {
                    fs::remove_file(&track).unwrap();
                    std::os::unix::fs::symlink(root.path().join("roms/set/tracks/one.bin"), &track)
                        .unwrap();
                }
                _ => put(
                    &root.path().join("roms/set/Game.cue"),
                    b"FILE \"Game.m3u\" BINARY\n",
                ),
            }
            let registry = real_registry(&rules);
            let plan = plan(root.path(), private.path(), &registry).unwrap();
            assert!(!plan.unresolved.is_empty(), "{case}");
            assert!(plan.moves.is_empty(), "{case}");
            assert!(apply(&plan, root.path(), private.path(), &registry).is_err());
            assert!(root.path().join("roms/set/Game.m3u").is_file());
            assert!(root.path().join("roms/set/tracks/one.bin").is_file());
            assert!(!root.path().join("games").exists());
        }
    }

    fn tree_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn collect(root: &Path, path: &Path, output: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(root, &path, output);
                } else {
                    output.insert(
                        path.strip_prefix(root).unwrap().into(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut output = BTreeMap::new();
        collect(root, root, &mut output);
        output
    }

    #[test]
    fn dry_run_records_actual_prospective_ids_and_log_destinations_without_other_writes() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        // An unlisted file in the future source takes the unsuffixed id first.
        put(
            &root.path().join("games/gba/00/Wario.gba"),
            b"another Wario release",
        );
        let registry = real_registry(&rules);
        let readable_before = tree_bytes(root.path());
        let private_before = tree_bytes(private.path());
        run(
            &[],
            Some(root.path().as_os_str()),
            Some(private.path().as_os_str()),
            crate::plugin_policy::RegistrySource::Selected(std::sync::Arc::new(registry.clone())),
        )
        .unwrap();
        let plan: MigrationPlan = decode_json(&default_plan_path(private.path())).unwrap();
        assert!(
            plan.kept.iter().all(|game| game.new_id.is_some()),
            "dry run must identify and reconcile before apply"
        );
        assert_eq!(
            plan.kept
                .iter()
                .find(|game| game.old_id == "old-0")
                .unwrap()
                .new_id
                .as_deref(),
            Some("wario-2")
        );
        assert!(plan.play_logs.iter().all(|log| log.new_game
            == plan
                .kept
                .iter()
                .find(|game| game.old_id == log.old_game)
                .unwrap()
                .new_id));
        assert_eq!(tree_bytes(root.path()), readable_before);
        let mut private_after = tree_bytes(private.path());
        private_after.remove(Path::new("game-discovery/migration-plan.json"));
        assert_eq!(private_after, private_before);
        let report = apply(&plan, root.path(), private.path(), &registry).unwrap();
        assert_eq!(report.plan.kept, plan.kept);
        assert_eq!(report.plan.play_logs, plan.play_logs);
    }

    #[test]
    fn changed_prospective_ids_are_refused_before_moving_files_or_play_logs() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let registry = real_registry(&rules);
        let plan = plan(root.path(), private.path(), &registry).unwrap();
        put(
            &root.path().join("games/gba/00/Wario.gba"),
            b"a new id collision after planning",
        );
        let device = fs::read(root.path().join("device.yaml")).unwrap();
        assert!(apply(&plan, root.path(), private.path(), &registry).is_err());
        assert!(plan.moves.iter().all(|movement| movement.from.exists()));
        assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), device);
        assert!(root.path().join("catalog/games.yaml").exists());
        assert!(!root.path().join("catalog/library.yaml").exists());
        assert!(plan.play_logs.iter().all(|log| log_path(
            private.path(),
            &log.user,
            &log.old_game
        )
        .exists()));
    }

    #[test]
    fn edited_game_or_log_destinations_are_refused_before_any_data_move() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let registry = real_registry(&rules);
        let approved = plan(root.path(), private.path(), &registry).unwrap();
        let before = tree_bytes(root.path());
        for edit_log in [false, true] {
            let mut edited = approved.clone();
            if edit_log {
                edited.play_logs[0].new_game = Some("unapproved-log-target".into());
            } else {
                edited.kept[0].new_id = Some("unapproved-game".into());
            }
            assert!(apply(&edited, root.path(), private.path(), &registry).is_err());
            assert_eq!(tree_bytes(root.path()), before);
            let report: MigrationReport =
                decode_json(&report_path(&default_plan_path(private.path()))).unwrap();
            assert!(report.moved.is_empty());
            assert!(report.error.unwrap().contains("approved plan changed"));
        }
    }

    #[test]
    fn prepared_sha1_uses_the_same_byte_identity_in_dry_run_and_apply() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let mut rules = mini(root.path(), private.path());
        let crate::discovery::identify::ContentIdentity::Hash { sha1, .. } =
            crate::discovery::identify::file_identity(
                &root.path().join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/old/MASTER.DAT"),
                &mut Default::default(),
            )
            .unwrap()
        else {
            panic!("ordinary byte identity")
        };
        rules
            .games
            .values_mut()
            .next()
            .unwrap()
            .releases
            .get_mut("prepared")
            .unwrap()
            .hashes = vec![sha1];
        let registry = real_registry(&rules);
        let plan = plan(root.path(), private.path(), &registry).unwrap();
        assert_eq!(plan.kept.len(), 5);
        assert!(plan.kept.iter().all(|game| game.new_id.is_some()));
        let report = apply(&plan, root.path(), private.path(), &registry).unwrap();
        assert_eq!(report.plan.kept, plan.kept);
    }

    #[test]
    fn mini_plan_is_deterministic_and_dry_run_only_writes_the_plan() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let before = fs::read(root.path().join("device.yaml")).unwrap();
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        assert_eq!(
            plan,
            plan_with_rules(root.path(), private.path(), &rules).unwrap()
        );
        assert_eq!(plan.kept.len(), 5);
        assert_eq!(plan.sources.len(), 4);
        assert_eq!(
            plan.dropped
                .iter()
                .filter(|e| e.reason == DropReason::PluginData)
                .count(),
            3
        );
        assert_eq!(
            plan.dropped
                .iter()
                .filter(|e| e.reason == DropReason::Duplicate)
                .count(),
            1
        );
        assert!(plan
            .kept
            .iter()
            .flat_map(|g| &g.files)
            .all(
                |p| p.extension().is_none_or(|e| e != "o") && p.file_name().unwrap() != "README.md"
            ));
        assert!(plan.moves.contains(&FileMove {
            from: root.path().join("mixed/Wario.gba"),
            to: root.path().join("games/gba/mixed/Wario.gba")
        }));
        assert!(plan.moves.contains(&FileMove {
            from: root
                .path()
                .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/old"),
            to: root.path().join(
                "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/fallout-ce"
            )
        }));
        assert_eq!(plan.play_logs.len(), 2);
        let path = default_plan_path(private.path());
        write_json(&path, &plan).unwrap();
        assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), before);
        assert!(plan.moves.iter().all(|m| m.from.exists() && !m.to.exists()));
        assert!(!summary(&plan).contains("DO-NOT-PRINT"));
        assert!(!String::from_utf8(fs::read(path).unwrap())
            .unwrap()
            .contains("DO-NOT-PRINT"));
    }

    #[test]
    fn unclaimed_group_evidence_and_unresolved_use_rules_not_folders() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let mut rules = mini(root.path(), private.path());
        // The README outside users is claimed here by megadrive, since .md is a real extension.
        let snes = ContentRules {
            system: "other".into(),
            extensions: vec!["sfc".into()],
            ..Default::default()
        };
        rules.systems.push(snes);
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        assert_eq!(
            plan.unresolved,
            vec![root.path().join("mixed/Mario.sfc").display().to_string()]
        );
        rules.groups.push(GroupRecord {
            id: "related".into(),
            title: None,
            systems: vec!["other".into(), "snes".into()],
        });
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        assert!(plan.sources.iter().any(|s| s.id == "related"));
        rules.systems.last_mut().unwrap().evidence = EvidenceRecord {
            bytes: vec![crate::plugin::BytesRule {
                offset: 0,
                hex: hex::encode(b"mixed/"),
            }],
            names: vec![],
        };
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        assert!(plan.sources.iter().any(|s| s.id == "other"));
        rules.systems.last_mut().unwrap().evidence = EvidenceRecord {
            bytes: vec![],
            names: vec!["^Mario\\.sfc$".into()],
        };
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        assert!(plan.sources.iter().any(|s| s.id == "other"));
        rules.games.insert(
            "@test:cart/mario".into(),
            DeclaredGame {
                provider: "@test:cart".into(),
                key: "mario".into(),
                title: "Mario".into(),
                releases: indexmap::IndexMap::from([(
                    "original".into(),
                    DeclaredRelease {
                        system: Some("snes".into()),
                        hashes: vec![whole_hash(&root.path().join("mixed/Mario.sfc")).unwrap()],
                        artifacts: Vec::new(),
                        file: None,
                    },
                )]),
            },
        );
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        assert!(plan.sources.iter().any(|s| s.id == "snes"));
        assert!(!plan.sources.iter().any(|s| s.id == "other"));
        rules.systems.retain(|r| r.system != "gba");
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        assert!(plan
            .dropped
            .iter()
            .any(|e| e.old_id == "old-0" && e.reason == DropReason::Unclaimed));
    }

    fn publish_scan(root: &Path, private: &Path, rules: &Rules) -> Result<(), MigrationError> {
        crate::discovery::DiscoveryCoordinator::new(root, private)
            .with_registry_source(crate::plugin_policy::RegistrySource::Selected(
                std::sync::Arc::new(real_registry(rules)),
            ))
            .rescan(&crate::discovery::DiscoveryOptions::default())
            .map_err(|_| step_error("real discovery scan"))?;
        Ok(())
    }

    #[test]
    fn apply_moves_cleans_scans_once_carries_titles_and_rekeys_two_logs() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        let mut scans = 0;
        let report = apply_with_scan(
            &plan,
            root.path(),
            private.path(),
            &report_path(&default_plan_path(private.path())),
            &rules,
            || {
                scans += 1;
                assert!(!root.path().join("catalog/games.yaml").exists());
                assert!(!root.path().join("catalog/releases.yaml").exists());
                let device = read_yaml(&root.path().join("device.yaml"))?;
                assert!(device["storage"].get("old").is_none());
                assert!(device["locations"].as_mapping().unwrap().is_empty());
                publish_scan(root.path(), private.path(), &rules)
            },
        )
        .unwrap();
        assert_eq!(scans, 1);
        assert!(report.error.is_none());
        assert_eq!(&report.moved[..plan.moves.len()], plan.moves.as_slice());
        assert_eq!(report.moved.len(), plan.moves.len() + 2);
        assert!(plan.moves.iter().all(|m| !m.from.exists() && m.to.exists()));
        assert!(root
            .path()
            .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/prepared-arm-x/a.o")
            .is_file());
        let library = read_yaml(&root.path().join("catalog/library.yaml")).unwrap();
        assert_eq!(report.plan.kept, plan.kept);
        for game in &report.plan.kept {
            assert_eq!(
                library["library"][game.new_id.as_ref().unwrap()]["title"].as_str(),
                Some(game.title.as_str())
            );
        }
        for log in &report.plan.play_logs {
            assert!(!log_path(private.path(), &log.user, &log.old_game).exists());
            let bytes = fs::read(log_path(
                private.path(),
                &log.user,
                log.new_game.as_ref().unwrap(),
            ))
            .unwrap();
            let value: crate::host::play_log::PlayLog = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(Some(&value.game_id), log.new_game.as_ref());
            assert_eq!(value.user_id, log.user);
        }
    }

    #[test]
    fn changed_move_destination_is_refused_before_any_move() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        let original = fs::read(root.path().join("device.yaml")).unwrap();
        put(&plan.moves[1].to, b"must not overwrite");
        let path = report_path(&default_plan_path(private.path()));
        assert!(apply_with_scan(
            &plan,
            root.path(),
            private.path(),
            &path,
            &rules,
            || panic!("must not scan")
        )
        .is_err());
        let report: MigrationReport = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert!(report.moved.is_empty());
        assert!(report.completed_steps.is_empty());
        assert!(report.error.is_some());
        assert_eq!(fs::read(&plan.moves[1].to).unwrap(), b"must not overwrite");
        assert_eq!(fs::read(root.path().join("device.yaml")).unwrap(), original);
        assert!(root.path().join("catalog/games.yaml").is_file());
    }

    #[test]
    fn scan_failure_stops_before_play_logs_and_reports_previous_steps() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        let path = report_path(&default_plan_path(private.path()));
        assert!(
            apply_with_scan(&plan, root.path(), private.path(), &path, &rules, || Err(
                MigrationError::Step("scan failed".into())
            ))
            .is_err()
        );
        let report: MigrationReport = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(report.moved, plan.moves);
        assert!(report
            .completed_steps
            .iter()
            .any(|s| s == "remove-old-state"));
        assert!(!report
            .completed_steps
            .iter()
            .any(|s| s == "scan-and-reconcile"));
        for log in &plan.play_logs {
            assert!(log_path(private.path(), &log.user, &log.old_game).is_file());
        }
    }

    #[test]
    fn cli_dry_run_dispatches_writes_only_selected_plan_and_redacts_config_errors() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        put(&root.path().join("roms/Game.gba"), b"rom");
        let device = "host:\n  title: DO-NOT-PRINT\nlocations:\n  old-release:\n    - storage: roms\n      path: Game.gba\n";
        put(&root.path().join("device.yaml"), device);
        put(
            &root.path().join("catalog/games.yaml"),
            "games:\n  old-game:\n    title: Kept title\n    releases: [old-release]\n",
        );
        put(
            &root.path().join("catalog/releases.yaml"),
            "releases:\n  old-release:\n    game: old-game\n    system: gba\n",
        );
        let registry = crate::plugin_policy::RegistrySource::Selected(std::sync::Arc::new(
            PluginRegistry::new(vec![], vec![]).unwrap(),
        ));
        let custom = private.path().join("operator-plan.json");
        let execute = |args: &[std::ffi::OsString]| {
            crate::catalog_cli::run_with_registry_source(
                args,
                Some(root.path().as_os_str()),
                Some(private.path().as_os_str()),
                registry.clone(),
            )
        };
        let output =
            execute(&["migrate".into(), "--plan".into(), custom.as_os_str().into()]).unwrap();
        assert!(output.contains("Dry run only"));
        assert!(!output.contains("DO-NOT-PRINT"));
        for private_value in [
            root.path().to_str().unwrap(),
            private.path().to_str().unwrap(),
            "old-game",
            "old-release",
            "Game.gba",
        ] {
            assert!(
                !output.contains(private_value),
                "migration output exposed private detail"
            );
        }
        assert!(output.contains("Plan file: selected --plan file"));
        assert!(custom.is_file());
        let details: MigrationPlan = decode_json(&custom).unwrap();
        assert_eq!(details.dropped[0].old_id, "old-game");
        assert!(!default_plan_path(private.path()).exists());
        assert!(!report_path(&custom).exists());
        assert_eq!(
            fs::read_to_string(root.path().join("device.yaml")).unwrap(),
            device
        );
        assert!(root.path().join("roms/Game.gba").is_file());
        assert_eq!(fs::read_dir(private.path()).unwrap().count(), 2);
        assert!(execute(&[
            "migrate".into(),
            "--plan".into(),
            root.path().join("device.yaml").into_os_string()
        ])
        .is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("device.yaml")).unwrap(),
            device
        );
        put(&root.path().join("device.yaml"), "host: [DO-NOT-PRINT\n");
        let error = execute(&["migrate".into()]).unwrap_err();
        assert!(!error.contains("DO-NOT-PRINT"));
        assert!(!default_plan_path(private.path()).exists());
    }

    #[test]
    fn symlink_destination_stops_without_moving_files() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("games")).unwrap();
        assert!(plan_with_rules(root.path(), private.path(), &rules).is_err());
        let path = report_path(&default_plan_path(private.path()));
        assert!(apply_with_scan(
            &plan,
            root.path(),
            private.path(),
            &path,
            &rules,
            || panic!("must not scan")
        )
        .is_err());
        let report: MigrationReport = decode_json(&path).unwrap();
        assert!(report.moved.is_empty());
        assert!(plan.moves.iter().all(|m| m.from.exists()));
        assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
    }

    #[test]
    fn play_log_collision_stops_without_replacing_the_other_history() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        let log = &plan.play_logs[0];
        let collision = log_path(private.path(), &log.user, log.new_game.as_ref().unwrap());
        let history = serde_json::to_vec(&crate::host::play_log::PlayLog {
            user_id: log.user.clone(),
            game_id: log.new_game.clone().unwrap(),
            entries: vec![],
        })
        .unwrap();
        put(&collision, &history);
        let path = report_path(&default_plan_path(private.path()));
        assert!(
            apply_with_scan(&plan, root.path(), private.path(), &path, &rules, || {
                publish_scan(root.path(), private.path(), &rules)
            })
            .is_err()
        );
        let report: MigrationReport = decode_json(&path).unwrap();
        assert!(report.moved.is_empty());
        assert_eq!(fs::read(collision).unwrap(), history);
        assert!(log_path(private.path(), &log.user, &log.old_game).is_file());
        assert!(!report
            .completed_steps
            .iter()
            .any(|s| s == "rename-play-logs"));
    }

    #[test]
    fn edited_discovery_storage_survives_ownership_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rules = mini(root.path(), private.path());
        let plan = plan_with_rules(root.path(), private.path(), &rules).unwrap();
        let mut device = read_yaml(&root.path().join("device.yaml")).unwrap();
        device["storage"]["old"].as_mapping_mut().unwrap().insert(
            "path".into(),
            serde_yaml::to_value(serde_json::json!({"authored":"value"})).unwrap(),
        );
        write_yaml(&root.path().join("device.yaml"), &device).unwrap();
        apply_with_scan(
            &plan,
            root.path(),
            private.path(),
            &report_path(&default_plan_path(private.path())),
            &rules,
            || publish_scan(root.path(), private.path(), &rules),
        )
        .unwrap();
        let device = read_yaml(&root.path().join("device.yaml")).unwrap();
        assert_eq!(
            device["storage"]["old"]["path"]["authored"].as_str(),
            Some("value")
        );
    }

    #[test]
    fn private_plan_directory_is_0700_and_unsafe_existing_state_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        put(&root.path().join("device.yaml"), "{}\n");
        put(&root.path().join("catalog/games.yaml"), "{}\n");
        put(&root.path().join("catalog/releases.yaml"), "{}\n");
        let registry = crate::plugin_policy::RegistrySource::Selected(std::sync::Arc::new(
            PluginRegistry::new(vec![], vec![]).unwrap(),
        ));
        let execute = |args: &[std::ffi::OsString]| {
            run(
                args,
                Some(root.path().as_os_str()),
                Some(private.path().as_os_str()),
                registry.clone(),
            )
        };
        execute(&[]).unwrap();
        let state = private.path().join("game-discovery");
        assert_eq!(
            fs::metadata(&state).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let custom = tempfile::tempdir().unwrap();
        fs::set_permissions(custom.path(), fs::Permissions::from_mode(0o755)).unwrap();
        execute(&[
            "--plan".into(),
            custom.path().join("chosen.json").into_os_string(),
        ])
        .unwrap();
        assert_eq!(
            fs::metadata(custom.path()).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(execute(&[]).is_err());
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn real_discovery_migrates_a_paired_source() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        mini(root.path(), private.path());
        let mut device = read_yaml(&root.path().join("device.yaml")).unwrap();
        device.as_mapping_mut().unwrap().remove("host");
        write_yaml(&root.path().join("device.yaml"), &device).unwrap();
        let hash = whole_hash(&root.path().join(
            "users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9/old/MASTER.DAT",
        ))
        .unwrap();
        let source = format!(
            r#"export const name = "migration"; export const title = "Migration";
export const systems = {{gba:{{content:{{extensions:["gba"]}}}},snes:{{content:{{extensions:["sfc"]}}}},megadrive:{{content:{{extensions:["md"]}}}},lynx:{{content:{{extensions:["lnx"]}}}}}};
export const games = {{fallout:{{title:"Fallout", releases:{{prepared:{{hashes:["{hash}"],file:"fallout-ce/MASTER.DAT"}}}}}}}};"#
        );
        let plugin = crate::plugin::load_plugin_source("@test", &source).unwrap();
        let registry = PluginRegistry::new(vec![plugin], ["@test:migration".to_owned()]).unwrap();
        let plan = plan(root.path(), private.path(), &registry).unwrap();
        assert_eq!(
            plan.kept.len(),
            5,
            "four ROMs and the prepared game must be kept"
        );
        assert_eq!(plan.sources.len(), 4);
        let report = apply(&plan, root.path(), private.path(), &registry).unwrap();
        assert_eq!(report.plan.kept.len(), 5);
        assert_eq!(report.plan.sources.len(), 4);
        for (system, file) in [
            ("gba", "Wario.gba"),
            ("snes", "Mario.sfc"),
            ("megadrive", "Sonic.md"),
            ("lynx", "Lynx.lnx"),
        ] {
            assert!(root
                .path()
                .join("games")
                .join(system)
                .join("mixed")
                .join(file)
                .is_file());
        }
        assert!(report
            .plan
            .kept
            .iter()
            .filter(|g| !g.files[0].starts_with(root.path().join("users")))
            .all(|g| g.new_id.is_some()));
        let library = root.path().join("catalog/library.yaml");
        let bytes = fs::read(&library).unwrap();
        let ownership = private.path().join("game-discovery/ownership.json");
        let owned = fs::read(&ownership).unwrap();
        let snapshot =
            crate::config::snapshot::ConfigSnapshotCoordinator::new(root.path()).reload();
        assert!(snapshot.diagnostic.is_none());
        for game in &report.plan.kept {
            if let Some(id) = &game.new_id {
                assert_eq!(
                    snapshot.snapshot.library[id].title.as_deref(),
                    Some(game.title.as_str())
                );
            }
        }
        // Fingerprints are computed from the current title, not persisted in ownership.
        let games = crate::discovery::reconcile::owned_discovery_games(root.path(), private.path())
            .unwrap();
        assert!(games.iter().all(|game| {
            crate::discovery::reconcile::current_owned_discovery_game(
                root.path(),
                private.path(),
                game,
            )
            .unwrap()
            .is_some()
        }));
        let coordinator = crate::discovery::DiscoveryCoordinator::new(root.path(), private.path())
            .with_registry_source(crate::plugin_policy::RegistrySource::Selected(
                std::sync::Arc::new(registry),
            ));
        coordinator.rescan(&Default::default()).unwrap();
        assert_eq!(fs::read(&library).unwrap(), bytes);
        assert_eq!(fs::read(&ownership).unwrap(), owned);
        for source in &report.plan.sources {
            coordinator.unpair(&source.id, &Default::default()).unwrap();
        }
        let snapshot =
            crate::config::snapshot::ConfigSnapshotCoordinator::new(root.path()).reload();
        assert!(snapshot.diagnostic.is_none());
        assert_eq!(
            snapshot.snapshot.library.len(),
            1,
            "only the declared prepared game remains"
        );
        for source in &report.plan.sources {
            assert!(
                !snapshot.snapshot.storage.contains_key(&source.id),
                "migration-created storage {} must be removed on unpair",
                source.id
            );
        }
    }
}
