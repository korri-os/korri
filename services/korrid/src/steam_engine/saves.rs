//! Steam saves in Korri's save history (ADR 0083 "Consequences", ADR 0014).
//!
//! A game's Steam saves are its Steam Cloud set as the engine reports it.
//! The host packs them and reports `save_export {appid, archive}`; korrid
//! reads the archive with its own archive reader (ADR 0030) into a private
//! scratch folder, never the Steam folder, and captures that folder through
//! the existing save store as one location.
//!
//! A cloud conflict at launch is a split with two heads (contract "Saves",
//! choice recorded here), written when the launch sync reports it: the
//! local side is the snapshot of the files the host exported before the
//! sync; the cloud side stays in Steam Cloud and is a record with the same
//! parents and no files, whose `endedAt` is Steam's remote time. This is the simpler of the contract's two options: no
//! second Steam folder and no engine download outside the game's folder.
//! Both sides stay restorable: the local files are in Korri's history and
//! on disk, the cloud files are in Steam Cloud, and the person's pick is
//! applied with `ResolveSyncConflict` before the next launch.
use crate::saves::{
    record::snapshot_id,
    store::{head_ids, NativeSaveStore},
    types::{SaveKind, SaveLocation, SnapshotLocation, SnapshotRecord},
    SaveError, SaveRepository,
};
use std::{
    collections::BTreeMap,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// The one save location of a Steam engine game. A location key is the
/// runner's own word (launch declarations name theirs); this one names
/// where the files come from.
pub const LOCATION: &str = "steam-cloud";

/// The engine runner's one save location, `<saveRoot>/steam-cloud`, added
/// by korrid to every engine-runner plan: korrid fills it from the host's
/// exports and the existing session capture snapshots it. The runner never
/// reads it; the game's files stay in the Steam folder.
pub fn declare(runner_id: &str, save_root: &str, locations: &mut Vec<SaveLocation>) {
    if super::is_engine_runner(runner_id) && !locations.iter().any(|l| l.key == LOCATION) {
        locations.push(SaveLocation {
            key: LOCATION.into(),
            kind: SaveKind::Save,
            path: Path::new(save_root).join(LOCATION).display().to_string(),
        });
    }
}

/// Read the host's archive into `scratch`, an empty folder korrid made.
pub fn extract(archive: &Path, scratch: &Path) -> Result<(), String> {
    crate::acquisition::archive::extract_save_export(archive, scratch)
}

/// The captured record for the files in `scratch`, not yet written.
pub fn capture(
    store: &NativeSaveStore,
    person: &str,
    scratch: &Path,
    template: SnapshotRecord,
) -> Result<SnapshotRecord, SaveError> {
    store.capture(
        person,
        &[SaveLocation {
            key: LOCATION.into(),
            kind: SaveKind::Save,
            path: scratch.display().to_string(),
        }],
        template,
    )
}

fn content(record: &SnapshotRecord) -> Vec<(String, String)> {
    let mut files: Vec<_> = record
        .locations
        .iter()
        .flat_map(|location| {
            location
                .files
                .iter()
                .map(|file| (file.path.clone(), file.sha256.clone()))
        })
        .collect();
    files.sort();
    files
}

/// Write a session-boundary snapshot whose parents are the current heads,
/// unless its files equal the one current head's: an unchanged boundary
/// adds nothing. Returns the snapshot id that is current afterwards.
pub fn record_boundary(
    store: &NativeSaveStore,
    person: &str,
    mut record: SnapshotRecord,
) -> Result<Option<String>, SaveError> {
    let records = store.records(person, &record.game_id)?;
    let heads = head_ids(&records)?;
    if let [head] = heads.as_slice() {
        if let Some(current) = records
            .iter()
            .find(|r| snapshot_id(r).is_ok_and(|id| &id == head))
        {
            if content(current) == content(&record) {
                return Ok(Some(head.clone()));
            }
        }
    }
    if record.locations.iter().all(|l| l.files.is_empty()) && heads.is_empty() {
        return Ok(None);
    }
    record.parents = heads;
    store.put_record(person, &record).map(Some)
}

/// The cloud side of a Steam Cloud conflict: an engine-runner record whose
/// one location has no files. It names Steam's copy and holds nothing here.
pub fn in_steam_cloud(record: &SnapshotRecord) -> bool {
    super::is_engine_runner(&record.runner_id)
        && matches!(record.locations.as_slice(), [location] if location.key == LOCATION && location.files.is_empty())
        && record.duration_seconds == 0
}

/// One open Steam Cloud conflict: the app, the two sides' snapshots, and
/// the side the person picked, until it is sent as `resolve`.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Conflict {
    pub appid: u32,
    pub local: String,
    pub cloud: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picked: Option<super::protocol::Side>,
}

/// Record a conflict as a split at the moment the launch sync reports it:
/// `local` (the files exported before the sync) and the cloud side (a record
/// with no files, `endedAt` at Steam's remote time), both children of the
/// current heads. The session that hit the conflict then adds no snapshot of
/// its own (`Conflicts::take_launch`), so the split has exactly these heads.
pub fn split(
    store: &NativeSaveStore,
    person: &str,
    appid: u32,
    mut local: SnapshotRecord,
    remote_time: u64,
) -> Result<Conflict, SaveError> {
    let heads = head_ids(&store.records(person, &local.game_id)?)?;
    local.parents = heads.clone();
    let cloud = SnapshotRecord {
        parents: heads,
        ended_at: rfc3339(remote_time),
        duration_seconds: 0,
        interrupted: false,
        locations: vec![SnapshotLocation {
            key: LOCATION.into(),
            kind: SaveKind::Save,
            files: vec![],
        }],
        ..local.clone()
    };
    let local = store.put_record(person, &local)?;
    let cloud = store.put_record(person, &cloud)?;
    if local == cloud {
        return Err(SaveError::Invalid(
            "the two sides of a Steam Cloud conflict are the same".into(),
        ));
    }
    Ok(Conflict {
        appid,
        local,
        cloud,
        picked: None,
    })
}

fn rfc3339(seconds: u64) -> String {
    chrono::DateTime::from_timestamp(i64::try_from(seconds).unwrap_or(0), 0)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// The book of open conflicts, as korrid keeps it on disk next to the save
/// store: `<private state root>/steam-engine-conflicts.json`. A record names
/// a person, a game, the app and the split's two snapshot ids (which are in
/// the save store), and the side the person picked until it is sent. The
/// launches whose session hit a conflict are kept too, so their completion
/// adds no third head, also after a korrid restart.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Book {
    #[serde(default)]
    conflicts: BTreeMap<String, BTreeMap<String, Conflict>>,
    #[serde(default)]
    launches: std::collections::BTreeSet<String>,
}

pub const FILE: &str = "steam-engine-conflicts.json";

#[derive(Default)]
pub struct Conflicts {
    /// Where the book is kept; `None` keeps it in memory only (tests).
    path: Option<PathBuf>,
    book: Mutex<Book>,
    /// Conflicts the surface has not taken up yet: it sends their launch
    /// back to the split choice (in memory: a restart asks at the next launch).
    fresh: Mutex<std::collections::BTreeSet<(String, String)>>,
}

impl Conflicts {
    /// The book in `private_root`, read now. An unreadable book is empty: it
    /// only delays a resolve until the person picks again.
    pub fn at(private_root: &Path) -> Self {
        let path = private_root.join(FILE);
        let book = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            book: Mutex::new(book),
            fresh: Mutex::default(),
        }
    }

    fn change<T>(&self, change: impl FnOnce(&mut Book) -> T) -> T {
        let mut book = self.book.lock().expect("conflicts poisoned");
        let result = change(&mut book);
        if let Some(path) = &self.path {
            if let Err(error) = write_book(path, &book) {
                log::warn!("steam engine: conflicts not kept: {error}");
            }
        }
        result
    }

    /// The conflict a launch's sync reported, and the launch that hit it.
    pub fn open(&self, person: &str, game: &str, launch_id: Option<&str>, conflict: Conflict) {
        if launch_id.is_some() {
            self.fresh
                .lock()
                .expect("conflicts poisoned")
                .insert((person.into(), game.into()));
        }
        self.change(|book| {
            book.conflicts
                .entry(person.into())
                .or_default()
                .insert(game.into(), conflict);
            if let Some(launch) = launch_id {
                book.launches.insert(launch.into());
            }
        })
    }

    /// True once after a launch of this game hit a conflict.
    pub fn take_fresh(&self, person: &str, game: &str) -> bool {
        self.fresh
            .lock()
            .expect("conflicts poisoned")
            .remove(&(person.to_owned(), game.to_owned()))
    }

    /// True once for a launch whose session hit a conflict: its completion
    /// takes no snapshot.
    pub fn take_launch(&self, launch_id: &str) -> bool {
        if !self
            .book
            .lock()
            .expect("conflicts poisoned")
            .launches
            .contains(launch_id)
        {
            return false;
        }
        self.change(|book| book.launches.remove(launch_id))
    }

    /// The person picked `snapshot` in this game's split: the cloud record
    /// means the cloud side, any other side means this device's files.
    pub fn pick(&self, person: &str, game: &str, snapshot: &str) -> Option<super::protocol::Side> {
        self.change(|book| {
            let conflict = book.conflicts.get_mut(person)?.get_mut(game)?;
            let side = if snapshot == conflict.cloud {
                super::protocol::Side::Cloud
            } else {
                super::protocol::Side::Local
            };
            conflict.picked = Some(side);
            Some(side)
        })
    }

    /// Put back a resolve that could not be sent.
    pub fn keep_resolve(&self, person: &str, game: &str, appid: u32, side: super::protocol::Side) {
        self.change(|book| {
            book.conflicts.entry(person.into()).or_default().insert(
                game.into(),
                Conflict {
                    appid,
                    local: String::new(),
                    cloud: String::new(),
                    picked: Some(side),
                },
            );
        })
    }

    /// The resolve to send before the next launch of this game, once. The
    /// conflict is closed when its pick is taken.
    pub fn take_resolve(&self, person: &str, game: &str) -> Option<(u32, super::protocol::Side)> {
        let picked = {
            let book = self.book.lock().expect("conflicts poisoned");
            book.conflicts.get(person)?.get(game)?.picked?
        };
        self.change(|book| {
            let games = book.conflicts.get_mut(person)?;
            let conflict = games.remove(game)?;
            if games.is_empty() {
                book.conflicts.remove(person);
            }
            Some((conflict.appid, picked))
        })
    }
}

fn write_book(path: &Path, book: &Book) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent"))?;
    let temporary = parent.join(format!(".{FILE}.{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(book).map_err(std::io::Error::other)?)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam_engine::protocol::Side;
    use std::fs;

    fn template(game: &str) -> SnapshotRecord {
        SnapshotRecord {
            parents: vec![],
            game_id: game.into(),
            release_id: None,
            runner_id: "@korri:steam/engine".into(),
            device: "a".repeat(64),
            ended_at: "2026-11-18T10:00:00.000Z".into(),
            duration_seconds: 60,
            interrupted: false,
            locations: vec![],
        }
    }

    fn export(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, data) in files {
            writer
                .start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        let path = dir.join(name);
        fs::write(&path, writer.finish().unwrap().into_inner()).unwrap();
        path
    }

    fn scratch(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn an_export_becomes_a_snapshot_and_an_unchanged_boundary_adds_nothing() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("private")).unwrap();
        let store = NativeSaveStore::new(&temp.path().join("private"));
        let person = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9".to_string();
        let archive = export(temp.path(), "launch.zip", &[("remote/SaveData.bin", b"v1")]);
        let first = scratch(temp.path(), "s1");
        extract(&archive, &first).unwrap();
        let record = capture(&store, &person, &first, template("g1")).unwrap();
        assert_eq!(record.locations.len(), 1);
        assert_eq!(record.locations[0].key, LOCATION);
        assert_eq!(record.locations[0].files[0].path, "remote/SaveData.bin");
        let id = record_boundary(&store, &person, record).unwrap().unwrap();

        let same = scratch(temp.path(), "s2");
        extract(&archive, &same).unwrap();
        let again = capture(&store, &person, &same, template("g1")).unwrap();
        assert_eq!(
            record_boundary(&store, &person, again).unwrap(),
            Some(id.clone())
        );
        assert_eq!(store.records(&person, "g1").unwrap().len(), 1);

        let changed_archive = export(temp.path(), "exit.zip", &[("remote/SaveData.bin", b"v2")]);
        let changed = scratch(temp.path(), "s3");
        extract(&changed_archive, &changed).unwrap();
        let next = capture(&store, &person, &changed, template("g1")).unwrap();
        let next_id = record_boundary(&store, &person, next).unwrap().unwrap();
        let records = store.records(&person, "g1").unwrap();
        let child = records
            .iter()
            .find(|r| snapshot_id(r).unwrap() == next_id)
            .unwrap();
        assert_eq!(child.parents, [id]);
        // A hostile export is refused by the archive rules.
        let bad = export(temp.path(), "bad.zip", &[("../escape", b"x")]);
        assert!(extract(&bad, &scratch(temp.path(), "s4")).is_err());
    }

    #[test]
    fn a_cloud_conflict_is_a_split_and_a_pick_names_the_side_across_a_restart() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("private")).unwrap();
        let private = temp.path().join("private");
        let store = NativeSaveStore::new(&private);
        let person = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9".to_string();
        let base = scratch(temp.path(), "base");
        extract(
            &export(temp.path(), "a.zip", &[("remote/s", b"base")]),
            &base,
        )
        .unwrap();
        let base_id = record_boundary(
            &store,
            &person,
            capture(&store, &person, &base, template("g")).unwrap(),
        )
        .unwrap()
        .unwrap();
        // The launch export, then the sync's conflict: both sides at once.
        let local = scratch(temp.path(), "local");
        extract(
            &export(temp.path(), "b.zip", &[("remote/s", b"local edit")]),
            &local,
        )
        .unwrap();
        let record = capture(&store, &person, &local, template("g")).unwrap();
        let conflict = split(&store, &person, 2379780, record, 1_763_390_000).unwrap();
        let records = store.records(&person, "g").unwrap();
        let heads = head_ids(&records).unwrap();
        assert_eq!(heads.len(), 2, "a split has two heads");
        assert!(heads.contains(&conflict.local) && heads.contains(&conflict.cloud));
        let cloud = records
            .iter()
            .find(|r| snapshot_id(r).unwrap() == conflict.cloud)
            .unwrap();
        assert_eq!(cloud.parents, [base_id]);
        assert_eq!(cloud.ended_at, "2025-11-17T14:33:20.000Z");
        assert!(in_steam_cloud(cloud));
        let local_record = records
            .iter()
            .find(|r| snapshot_id(r).unwrap() == conflict.local)
            .unwrap();
        assert!(!in_steam_cloud(local_record));
        assert_eq!(
            store.reconcile(&person, "g").unwrap(),
            crate::saves::types::SaveMark::Split
        );

        let book = Conflicts::at(&private);
        book.open(&person, "g", Some("launch-1"), conflict.clone());
        assert!(book.take_fresh(&person, "g") && !book.take_fresh(&person, "g"));
        assert_eq!(book.pick(&person, "other", &conflict.cloud), None);
        assert_eq!(book.pick(&person, "g", &conflict.cloud), Some(Side::Cloud));
        // korrid restarts: the pick and the launch are read back.
        let again = Conflicts::at(&private);
        assert!(again.take_launch("launch-1"));
        assert!(!Conflicts::at(&private).take_launch("launch-1"));
        assert_eq!(
            again.take_resolve(&person, "g"),
            Some((2379780, Side::Cloud))
        );
        assert_eq!(Conflicts::at(&private).take_resolve(&person, "g"), None);
        // An open conflict with no pick sends nothing.
        again.open(&person, "g", None, conflict.clone());
        assert_eq!(again.take_resolve(&person, "g"), None);
        assert_eq!(again.pick(&person, "g", &conflict.local), Some(Side::Local));
        assert_eq!(
            again.take_resolve(&person, "g"),
            Some((2379780, Side::Local))
        );
    }
}
