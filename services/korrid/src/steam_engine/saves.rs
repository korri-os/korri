//! Steam saves in Korri's save history (ADR 0083 "Consequences", ADR 0014).
//!
//! A game's Steam saves are its Steam Cloud set as the engine reports it.
//! The host packs them and reports `save_export {appid, archive}`; korrid
//! reads the archive with its own archive reader (ADR 0030) into a private
//! scratch folder, never the Steam folder, and captures that folder through
//! the existing save store as one location.
//!
//! A cloud conflict at launch is a split with two heads (contract "Saves",
//! choice recorded here): the local side is the snapshot the host exported
//! before the launch sync; the cloud side stays in Steam Cloud and is a
//! record with the same parents and no files, whose `endedAt` is Steam's
//! remote time. This is the simpler of the contract's two options: no
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
use std::{collections::BTreeMap, path::Path, sync::Mutex};

/// The one save location of a Steam engine game. A location key is the
/// runner's own word (launch declarations name theirs); this one names
/// where the files come from.
pub const LOCATION: &str = "steam-cloud";

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

/// The two sides of one Steam Cloud conflict.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conflict {
    pub appid: u32,
    pub local: String,
    pub cloud: String,
}

/// Record a conflict as a split: `local` (the exported files) and a cloud
/// record with no files, both children of the current heads.
pub fn split(
    store: &NativeSaveStore,
    person: &str,
    appid: u32,
    mut local: SnapshotRecord,
    remote_time: u64,
) -> Result<Conflict, SaveError> {
    let records = store.records(person, &local.game_id)?;
    let heads = head_ids(&records)?;
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
    })
}

fn rfc3339(seconds: u64) -> String {
    chrono::DateTime::from_timestamp(i64::try_from(seconds).unwrap_or(0), 0)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Open conflicts by (person, game), and the side each pick means. Held in
/// memory: after a korrid restart the next launch reports the conflict
/// again, and the person picks again.
#[derive(Default)]
pub struct Conflicts {
    open: Mutex<BTreeMap<(String, String), Conflict>>,
    picked: Mutex<BTreeMap<(String, String), (u32, super::protocol::Side)>>,
}

impl Conflicts {
    pub fn open(&self, person: &str, game: &str, conflict: Conflict) {
        self.open
            .lock()
            .expect("conflicts poisoned")
            .insert((person.into(), game.into()), conflict);
    }

    /// The person picked `snapshot` in the split. Returns the side when it
    /// is one of this conflict's sides.
    pub fn pick(&self, person: &str, game: &str, snapshot: &str) -> Option<super::protocol::Side> {
        let key = (person.to_owned(), game.to_owned());
        let conflict = self.open.lock().expect("conflicts poisoned").remove(&key)?;
        let side = if snapshot == conflict.local {
            super::protocol::Side::Local
        } else if snapshot == conflict.cloud {
            super::protocol::Side::Cloud
        } else {
            self.open
                .lock()
                .expect("conflicts poisoned")
                .insert(key, conflict);
            return None;
        };
        self.picked
            .lock()
            .expect("conflicts poisoned")
            .insert(key, (conflict.appid, side));
        Some(side)
    }

    /// The resolve to send before the next launch of this game, once.
    pub fn take_resolve(&self, person: &str, game: &str) -> Option<(u32, super::protocol::Side)> {
        self.picked
            .lock()
            .expect("conflicts poisoned")
            .remove(&(person.to_owned(), game.to_owned()))
    }
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
    fn a_cloud_conflict_is_a_split_and_a_pick_names_the_side_once() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("private")).unwrap();
        let store = NativeSaveStore::new(&temp.path().join("private"));
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
        assert_eq!(cloud.parents, [base_id.clone()]);
        assert_eq!(cloud.ended_at, "2025-11-17T14:33:20.000Z");
        assert!(cloud.locations[0].files.is_empty());
        assert_eq!(
            store.reconcile(&person, "g").unwrap(),
            crate::saves::types::SaveMark::Split
        );

        let conflicts = Conflicts::default();
        conflicts.open(&person, "g", conflict.clone());
        assert_eq!(conflicts.pick(&person, "g", "not-a-side"), None);
        assert_eq!(
            conflicts.pick(&person, "g", &conflict.cloud),
            Some(Side::Cloud)
        );
        assert_eq!(
            conflicts.take_resolve(&person, "g"),
            Some((2379780, Side::Cloud))
        );
        assert_eq!(conflicts.take_resolve(&person, "g"), None);
    }
}
