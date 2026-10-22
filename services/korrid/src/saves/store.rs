//! Private, durable snapshot records and streaming content-addressed objects.
use super::{
    record::{canonical_record, hash_hex, invalid, snapshot_id},
    types::*,
    SaveError, SaveRepository,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

const MAX_RECORD: u64 = 8 * 1024 * 1024;
static STORAGE: Mutex<()> = Mutex::new(());
// Lock order is STORAGE -> FACTS, never the reverse. FACTS covers only small
// record/advisory publications and reads, never object transfer or verification.
static FACTS: Mutex<()> = Mutex::new(());
fn facts_lock() -> Result<MutexGuard<'static, ()>, SaveError> {
    FACTS
        .lock()
        .map_err(|_| invalid("save facts lock poisoned"))
}
fn lock() -> Result<MutexGuard<'static, ()>, SaveError> {
    STORAGE.lock().map_err(|_| invalid("storage lock poisoned"))
}
fn io(e: std::io::Error) -> SaveError {
    SaveError::Storage(e.to_string())
}
fn codec(s: &str) -> String {
    crate::host::play_log::encode_uri_component(s)
}
fn key(s: &str) -> Result<String, SaveError> {
    let encoded = codec(s);
    if encoded.is_empty() || encoded == "." || encoded == ".." || encoded.len() > 255 {
        return Err(invalid("history path identity"));
    }
    Ok(encoded)
}
fn directory(path: &Path, create: bool) -> Result<bool, SaveError> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if !create {
                return Ok(false);
            }
            match fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io(error)),
            }
            if let Some(parent) = path.parent() {
                File::open(parent).and_then(|f| f.sync_all()).map_err(io)?;
            }
        }
        Err(e) => return Err(io(e)),
        Ok(_) => {}
    }
    let m = fs::symlink_metadata(path).map_err(io)?;
    if !m.is_dir() || m.file_type().is_symlink() || m.permissions().mode() & 0o077 != 0 {
        return Err(invalid("save directory is not private"));
    }
    Ok(true)
}
fn regular(path: &Path) -> Result<File, SaveError> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SaveError::Missing
            } else {
                io(e)
            }
        })?;
    let m = f.metadata().map_err(io)?;
    if !m.is_file() || m.permissions().mode() & 0o077 != 0 {
        return Err(invalid("save object is not a private regular file"));
    }
    Ok(f)
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, SaveError> {
    let f = regular(path)?;
    if f.metadata().map_err(io)?.len() > MAX_RECORD {
        return Err(invalid("oversized record"));
    }
    let mut bytes = Vec::new();
    f.take(MAX_RECORD + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err(invalid("oversized record"));
    }
    serde_json::from_slice(&bytes).map_err(|e| invalid(&e.to_string()))
}
fn publish(path: &Path, input: &mut dyn Read, expected: Option<&str>) -> Result<(), SaveError> {
    if let Ok(m) = fs::symlink_metadata(path) {
        if !m.is_file() || m.file_type().is_symlink() || m.permissions().mode() & 0o077 != 0 {
            return Err(invalid("unsafe save destination"));
        }
    }
    let parent = path.parent().ok_or_else(|| invalid("save parent"))?;
    let temp = parent.join(format!(".save-{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)
            .map_err(io)?;
        let mut sha = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.read(&mut buffer).map_err(io)?;
            if count == 0 {
                break;
            }
            sha.update(&buffer[..count]);
            file.write_all(&buffer[..count]).map_err(io)?;
        }
        if let Some(hash) = expected {
            if hex::encode(sha.finalize()) != hash_hex(hash)? {
                return Err(invalid("object hash mismatch"));
            }
        }
        file.sync_all().map_err(io)?;
        fs::rename(&temp, path).map_err(io)?;
        File::open(parent).and_then(|f| f.sync_all()).map_err(io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[derive(Clone, Debug)]
pub struct NativeSaveStore {
    root: PathBuf,
}
impl NativeSaveStore {
    pub fn new(private_state_root: &Path) -> Self {
        Self {
            root: private_state_root.join("saves"),
        }
    }
    pub fn reconcile(&self, person: &str, game: &str) -> Result<SaveMark, SaveError> {
        let _guard = lock()?;
        let records = self.read_records(person, game)?;
        let ids = head_ids(&records)?;
        if ids.len() <= 1 {
            return Ok(SaveMark::NotChecked);
        }
        let heads: Vec<_> = records
            .into_iter()
            .filter(|r| ids.contains(&snapshot_id(r).expect("validated record")))
            .collect();
        match super::history::automatic_join(&heads)? {
            None => Ok(SaveMark::Split),
            Some(joined) => {
                // Never claim continuation until every side's content is durable.
                for head in &heads {
                    if !self.record_complete(head)? {
                        return Ok(SaveMark::NotChecked);
                    }
                }
                self.write_record(person, &joined)?;
                Ok(SaveMark::NotChecked)
            }
        }
    }

    /// Offline launch may continue the last complete local snapshot when a
    /// newer peer head is known but its content has not arrived. The resulting
    /// session keeps that older base, so ancestry still detects a later split.
    pub fn launch_base(
        &self,
        person: &str,
        game: &str,
        device: &str,
    ) -> Result<Option<String>, SaveError> {
        let _guard = lock()?;
        let records = self.read_records(person, game)?;
        let heads = head_ids(&records)?;
        let current: Vec<_> = records
            .iter()
            .filter(|r| heads.contains(&snapshot_id(r).expect("validated record")))
            .cloned()
            .collect();
        if current.len() > 1 && super::history::automatic_join(&current)?.is_none() {
            return Err(SaveError::Split);
        }
        for id in &heads {
            if let Some(record) = records
                .iter()
                .find(|r| snapshot_id(r).is_ok_and(|value| &value == id))
            {
                if self.progress_complete(record)? {
                    return Ok(Some(id.clone()));
                }
            }
        }
        let by_id: BTreeMap<_, _> = records
            .iter()
            .map(|r| (snapshot_id(r).expect("validated record"), r))
            .collect();
        let mut local = Vec::new();
        for (id, record) in &by_id {
            if record.device == device && self.record_complete(record)? {
                local.push(id.clone());
            }
        }
        fn ancestor(
            candidate: &str,
            descendant: &str,
            records: &BTreeMap<String, &SnapshotRecord>,
        ) -> bool {
            let mut pending = vec![descendant.to_owned()];
            let mut seen = BTreeSet::new();
            while let Some(id) = pending.pop() {
                if !seen.insert(id.clone()) {
                    continue;
                }
                if let Some(record) = records.get(&id) {
                    for parent in &record.parents {
                        if parent == candidate {
                            return true;
                        }
                        pending.push(parent.clone());
                    }
                }
            }
            false
        }
        let newest: Vec<_> = local
            .iter()
            .filter(|id| {
                !local
                    .iter()
                    .any(|other| other != *id && ancestor(id, other, &by_id))
            })
            .cloned()
            .collect();
        match newest.as_slice() {
            [] => Ok(None),
            [id] => Ok(Some(id.clone())),
            _ => Err(SaveError::Split),
        }
    }

    /// A choice records both heads; the unpicked content remains in history.
    pub fn choose(
        &self,
        person: &str,
        game: &str,
        id: &str,
        device: &str,
        ended_at: &str,
    ) -> Result<String, SaveError> {
        let _guard = lock()?;
        let records = self.read_records(person, game)?;
        let heads = head_ids(&records)?;
        if heads.len() < 2 || !heads.iter().any(|head| head == id) {
            return Err(invalid("choice no longer names an open split side"));
        }
        self.pick(person, &records, heads, id, device, ended_at)
    }
    /// Restore changes continuation by appending, never by rewriting history.
    pub fn restore(
        &self,
        person: &str,
        game: &str,
        id: &str,
        device: &str,
        ended_at: &str,
    ) -> Result<String, SaveError> {
        let _guard = lock()?;
        let records = self.read_records(person, game)?;
        let heads = head_ids(&records)?;
        self.pick(person, &records, heads, id, device, ended_at)
    }
    fn pick(
        &self,
        person: &str,
        records: &[SnapshotRecord],
        parents: Vec<String>,
        id: &str,
        device: &str,
        ended_at: &str,
    ) -> Result<String, SaveError> {
        let mut picked = records
            .iter()
            .find(|r| snapshot_id(r).is_ok_and(|v| v == id))
            .cloned()
            .ok_or(SaveError::Missing)?;
        if !self.record_complete(&picked)? {
            return Err(SaveError::Missing);
        }
        let sides: Vec<_> = records
            .iter()
            .filter(|record| parents.contains(&snapshot_id(record).expect("validated record")))
            .cloned()
            .collect();
        picked = super::history::with_screenshot_union(&picked, &sides)?;
        picked.parents = parents;
        picked.device = device.into();
        picked.ended_at = ended_at.into();
        picked.duration_seconds = 0;
        picked.interrupted = false;
        self.write_record(person, &picked)
    }

    /// Capture only after the executor proves exit. Objects become durable first;
    /// the caller journals this record before publishing it, for exact replay.
    pub fn capture(
        &self,
        person: &str,
        locations: &[SaveLocation],
        mut record: SnapshotRecord,
    ) -> Result<SnapshotRecord, SaveError> {
        let _guard = lock()?;
        record.locations.clear();
        fn walk(
            store: &NativeSaveStore,
            root: &Path,
            path: &Path,
            files: &mut Vec<SaveFile>,
        ) -> Result<(), SaveError> {
            let metadata = match fs::symlink_metadata(path) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && path == root => return Ok(()),
                Err(e) => return Err(io(e)),
            };
            if metadata.file_type().is_symlink() {
                return Err(invalid("symlink in save location"));
            }
            if metadata.is_dir() {
                for entry in fs::read_dir(path).map_err(io)? {
                    walk(store, root, &entry.map_err(io)?.path(), files)?;
                }
            } else if metadata.is_file() {
                let mut file = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(path)
                    .map_err(io)?;
                if !file.metadata().map_err(io)?.is_file() {
                    return Err(invalid("save is not a regular file"));
                }
                let mut sha = Sha256::new();
                let mut bytes = 0u64;
                let mut buffer = [0u8; 65536];
                loop {
                    let n = file.read(&mut buffer).map_err(io)?;
                    if n == 0 {
                        break;
                    }
                    sha.update(&buffer[..n]);
                    bytes += n as u64;
                }
                let hash = format!("sha256:{}", hex::encode(sha.finalize()));
                file.seek(SeekFrom::Start(0)).map_err(io)?;
                publish(&store.object_path(&hash, true)?, &mut file, Some(&hash))?;
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| invalid("save escaped location"))?
                    .to_str()
                    .ok_or_else(|| invalid("save filename is not UTF-8"))?
                    .to_owned();
                files.push(SaveFile {
                    path: relative,
                    sha256: hash,
                    bytes,
                });
            } else {
                return Err(invalid("special file in save location"));
            }
            Ok(())
        }
        for location in locations {
            let root = Path::new(&location.path);
            // A persisted declaration is trusted, but its ancestors may have changed.
            for ancestor in root.ancestors() {
                if let Ok(m) = fs::symlink_metadata(ancestor) {
                    if m.file_type().is_symlink() {
                        return Err(invalid("symlink in save ancestors"));
                    }
                }
            }
            let mut files = Vec::new();
            walk(self, root, root, &mut files)?;
            files.sort_by(|a, b| a.path.cmp(&b.path));
            record.locations.push(SnapshotLocation {
                key: location.key.clone(),
                kind: location.kind.clone(),
                files,
            });
        }
        let inherited: Vec<_> = self
            .read_records(person, &record.game_id)?
            .into_iter()
            .filter(|r| {
                record
                    .parents
                    .contains(&snapshot_id(r).expect("validated record"))
            })
            .collect();
        record = super::history::with_screenshot_union(&record, &inherited)?;
        canonical_record(&record)?;
        Ok(record)
    }

    /// Materialize a whole snapshot before launch, never while the executor is
    /// live. Stage all content first. Each folder replacement is atomic; retrying
    /// after interruption completes the same snapshot before any game can run.
    pub fn materialize(
        &self,
        person: &str,
        game: &str,
        id: &str,
        save_root: &str,
        locations: &[SaveLocation],
    ) -> Result<(), SaveError> {
        let _guard = lock()?;
        crate::launcher::plugin_launch::validate_save_locations(save_root, locations)
            .map_err(SaveError::Invalid)?;
        let record = self
            .read_records(person, game)?
            .into_iter()
            .find(|r| snapshot_id(r).is_ok_and(|v| v == id))
            .ok_or(SaveError::Missing)?;
        if !self.progress_complete(&record)? {
            return Err(SaveError::Missing);
        }
        let save_root = runtime_save_directory(Path::new(save_root))?;
        let users = restore_users_root(&save_root, person, game)?;
        super::access::prepare_account(
            users.parent().ok_or_else(|| invalid("library root"))?,
            person,
        )?;
        let mut staged = Vec::new();
        let result = (|| {
            for location in locations {
                let target = runtime_save_directory(Path::new(&location.path))?;
                if !target.starts_with(&save_root) || target == save_root {
                    return Err(invalid(
                        "save location must stay inside its per-person save root",
                    ));
                }
                let parent = target
                    .parent()
                    .ok_or_else(|| invalid("save location parent"))?;
                runtime_directory(parent, &users)?;
                let stage = parent.join(format!(".save-{:016x}.tmp", rand::random::<u64>()));
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&stage)
                    .map_err(io)?;
                staged.push((stage.clone(), target.to_path_buf()));
                if let Some(content) = record
                    .locations
                    .iter()
                    .find(|l| l.key == location.key && l.kind == location.kind)
                {
                    for expected in &content.files {
                        let path = stage.join(&expected.path);
                        runtime_directory(path.parent().unwrap(), &users)?;
                        let mut source = match self.verified_object(&expected.sha256) {
                            Ok(file) => file,
                            Err(SaveError::Missing) if location.kind == SaveKind::Screenshot => {
                                continue
                            }
                            Err(error) => return Err(error),
                        };
                        let mut output = OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .mode(0o600)
                            .open(&path)
                            .map_err(io)?;
                        std::io::copy(&mut source, &mut output).map_err(io)?;
                        inherited_write_access(&output)?;
                        output.sync_all().map_err(io)?;
                    }
                }
                let directory = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
                    .open(&stage)
                    .map_err(io)?;
                inherited_write_access(&directory)?;
                directory.sync_all().map_err(io)?;
            }
            for (stage, target) in &staged {
                match fs::symlink_metadata(target) {
                    Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {
                        use std::os::unix::ffi::OsStrExt;
                        let a = std::ffi::CString::new(stage.as_os_str().as_bytes())
                            .map_err(|_| invalid("save path"))?;
                        let b = std::ffi::CString::new(target.as_os_str().as_bytes())
                            .map_err(|_| invalid("save path"))?;
                        if unsafe {
                            libc::renameat2(
                                libc::AT_FDCWD,
                                a.as_ptr(),
                                libc::AT_FDCWD,
                                b.as_ptr(),
                                libc::RENAME_EXCHANGE,
                            )
                        } != 0
                        {
                            return Err(io(std::io::Error::last_os_error()));
                        }
                    }
                    Ok(_) => return Err(invalid("save location is not a directory")),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        fs::rename(stage, target).map_err(io)?
                    }
                    Err(e) => return Err(io(e)),
                }
                File::open(target.parent().unwrap())
                    .and_then(|f| f.sync_all())
                    .map_err(io)?;
            }
            Ok(())
        })();
        for (stage, _) in staged {
            if stage.exists() {
                fs::remove_dir_all(stage).map_err(io)?;
            }
        }
        result
    }

    /// Payload retention never removes record ancestry, unpicked split sides,
    /// or a snapshot without another device's durable acknowledgement.
    pub fn prune(&self, person: &str, game: &str, local_device: &str) -> Result<(), SaveError> {
        let _guard = lock()?;
        let mut records = self.read_records(person, game)?;
        if records.len() <= 10 {
            return Ok(());
        }
        let path = self.game(person, game, false)?.ok_or(SaveError::Missing)?;
        let confirmations: BTreeMap<String, BTreeSet<String>> =
            match read_json(&path.join("confirmations.json")) {
                Ok(v) => v,
                Err(SaveError::Missing) => BTreeMap::new(),
                Err(e) => return Err(e),
            };
        let mut protected: BTreeSet<String> = head_ids(&records)?.into_iter().collect();
        let parents: BTreeMap<_, _> = records
            .iter()
            .map(|record| {
                (
                    snapshot_id(record).expect("validated record"),
                    record.parents.clone(),
                )
            })
            .collect();
        let mut distance = BTreeMap::new();
        let mut pending: std::collections::VecDeque<_> =
            protected.iter().map(|id| (id.clone(), 0usize)).collect();
        while let Some((id, depth)) = pending.pop_front() {
            if distance.get(&id).is_some_and(|known| *known <= depth) {
                continue;
            }
            distance.insert(id.clone(), depth);
            if let Some(parents) = parents.get(&id) {
                pending.extend(parents.iter().map(|parent| (parent.clone(), depth + 1)));
            }
        }
        // Clocks are display data, including during retention. The nearest
        // ancestry stays; equal-distance records have a stable content-id order.
        records.sort_by_key(|record| {
            let id = snapshot_id(record).expect("validated record");
            (distance.get(&id).copied().unwrap_or(usize::MAX), id)
        });
        for record in records.iter().take(10) {
            protected.insert(snapshot_id(record)?);
        }
        for merge in records.iter().filter(|r| r.parents.len() > 1) {
            for parent in &merge.parents {
                if records.iter().any(|r| {
                    snapshot_id(r).is_ok_and(|id| &id == parent)
                        && super::history::progress(r) != super::history::progress(merge)
                }) {
                    protected.insert(parent.clone());
                }
            }
        }
        let mut prunable = BTreeSet::new();
        let mut candidates = BTreeSet::new();
        for record in &records {
            let id = snapshot_id(record)?;
            if !protected.contains(&id)
                && confirmations
                    .get(&id)
                    .is_some_and(|devices| devices.iter().any(|device| device != local_device))
            {
                prunable.insert(id);
                candidates.extend(
                    record
                        .locations
                        .iter()
                        .flat_map(|l| &l.files)
                        .map(|f| f.sha256.clone()),
                );
            }
        }
        // Objects are shared across all people and games. A retained reference
        // anywhere protects its bytes, including the previous owner's history.
        for entry in fs::read_dir(&self.root).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| invalid("person directory"))?;
            if name == "objects" {
                continue;
            }
            for history in self.all_records(name)? {
                for record in history {
                    if name == person
                        && record.game_id == game
                        && prunable.contains(&snapshot_id(&record)?)
                    {
                        continue;
                    }
                    for file in record.locations.iter().flat_map(|l| &l.files) {
                        candidates.remove(&file.sha256);
                    }
                }
            }
        }
        for hash in candidates {
            let path = self.object_path(&hash, false)?;
            match regular(&path) {
                Ok(_) => fs::remove_file(path).map_err(io)?,
                Err(SaveError::Missing) => {}
                Err(e) => return Err(e),
            }
        }
        if self.root.join("objects").exists() {
            File::open(self.root.join("objects"))
                .and_then(|f| f.sync_all())
                .map_err(io)?;
        }
        Ok(())
    }

    pub fn remember_in_use(
        &self,
        person: &str,
        game: &str,
        device: &str,
        since: Option<u64>,
    ) -> Result<(), SaveError> {
        let _guard = facts_lock()?;
        super::account_root(Path::new("/"), device).map_err(SaveError::Invalid)?;
        let path = self.game(person, game, true)?.unwrap().join("in-use.json");
        let mut devices: BTreeMap<String, u64> = match read_json(&path) {
            Ok(v) => v,
            Err(SaveError::Missing) => BTreeMap::new(),
            Err(e) => return Err(e),
        };
        match since {
            Some(since) => {
                devices.insert(device.into(), since);
            }
            None => {
                devices.remove(device);
            }
        }
        let bytes = serde_json::to_vec(&devices).map_err(|e| invalid(&e.to_string()))?;
        publish(&path, &mut bytes.as_slice(), None)
    }
    pub fn in_use_warnings(
        &self,
        person: &str,
        game: &str,
    ) -> Result<Vec<SaveInUseWarning>, SaveError> {
        let (mut devices, records) = {
            let _facts = facts_lock()?;
            let Some(path) = self.game(person, game, false)? else {
                return Ok(vec![]);
            };
            let devices: BTreeMap<String, u64> = match read_json(&path.join("in-use.json")) {
                Ok(value) => value,
                Err(SaveError::Missing) => BTreeMap::new(),
                Err(error) => return Err(error),
            };
            (devices, self.read_records(person, game)?)
        };
        let heads = head_ids(&records)?;
        for record in &records {
            if heads.contains(&snapshot_id(record)?) && !self.progress_present(record) {
                let time = chrono::DateTime::parse_from_rfc3339(&record.ended_at)
                    .map_err(|_| invalid("end time"))?
                    .timestamp_millis();
                if let Ok(since) = u64::try_from(time) {
                    devices.entry(record.device.clone()).or_insert(since);
                }
            }
        }
        Ok(devices
            .into_iter()
            .map(|(device, since)| SaveInUseWarning { device, since })
            .collect())
    }
    pub fn confirmed_elsewhere(
        &self,
        person: &str,
        game: &str,
        local_device: &str,
    ) -> Result<bool, SaveError> {
        let _guard = lock()?;
        let Some(path) = self.game(person, game, false)? else {
            return Ok(false);
        };
        let heads = head_ids(&self.read_records(person, game)?)?;
        let confirmations: BTreeMap<String, BTreeSet<String>> =
            match read_json(&path.join("confirmations.json")) {
                Ok(v) => v,
                Err(SaveError::Missing) => BTreeMap::new(),
                Err(e) => return Err(e),
            };
        for record in self.read_records(person, game)? {
            if heads.contains(&snapshot_id(&record)?) && !self.record_complete(&record)? {
                return Ok(false);
            }
        }
        Ok(!heads.is_empty()
            && heads.iter().all(|id| {
                confirmations
                    .get(id)
                    .is_some_and(|devices| devices.iter().any(|d| d != local_device))
            }))
    }

    fn person(&self, person: &str, create: bool) -> Result<Option<PathBuf>, SaveError> {
        super::account_root(Path::new("/"), person).map_err(SaveError::Invalid)?;
        if !directory(&self.root, create)? {
            return Ok(None);
        }
        let path = self.root.join(codec(person));
        if !directory(&path, create)? {
            return Ok(None);
        }
        Ok(Some(path))
    }
    fn game(&self, person: &str, game: &str, create: bool) -> Result<Option<PathBuf>, SaveError> {
        let game = key(game)?;
        let Some(parent) = self.person(person, create)? else {
            return Ok(None);
        };
        let path = parent.join(game);
        if !directory(&path, create)? {
            return Ok(None);
        }
        Ok(Some(path))
    }
    fn records_at(&self, path: &Path) -> Result<Vec<SnapshotRecord>, SaveError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(path).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| invalid("record filename"))?;
            if name == "confirmations.json"
                || name == "in-use.json"
                || (name.starts_with(".save-") && name.ends_with(".tmp"))
            {
                continue;
            }
            let hex = name
                .strip_suffix(".json")
                .ok_or_else(|| invalid("record filename"))?;
            let id = format!("sha256:{hex}");
            hash_hex(&id)?;
            let record: SnapshotRecord = read_json(&entry.path())?;
            if snapshot_id(&record)? != id
                || Some(codec(&record.game_id).as_str())
                    != path.file_name().and_then(|v| v.to_str())
            {
                return Err(invalid("record identity mismatch"));
            }
            records.push(record);
        }
        // Canonicalization and hashing are expensive. Compute each immutable id
        // once for sorting, not once per comparison in every history read.
        records.sort_by_cached_key(|r| snapshot_id(r).expect("validated record"));
        Ok(records)
    }
    fn read_records(&self, person: &str, game: &str) -> Result<Vec<SnapshotRecord>, SaveError> {
        match self.game(person, game, false)? {
            Some(path) => self.records_at(&path),
            None => Ok(vec![]),
        }
    }
    fn all_records(&self, person: &str) -> Result<Vec<Vec<SnapshotRecord>>, SaveError> {
        let Some(path) = self.person(person, false)? else {
            return Ok(vec![]);
        };
        let mut records = Vec::new();
        for entry in fs::read_dir(path).map_err(io)? {
            let entry = entry.map_err(io)?;
            directory(&entry.path(), false)?;
            records.push(self.records_at(&entry.path())?);
        }
        Ok(records)
    }
    fn write_record(&self, person: &str, record: &SnapshotRecord) -> Result<String, SaveError> {
        let bytes = canonical_record(record)?;
        if bytes.len() as u64 > MAX_RECORD {
            return Err(invalid("oversized record"));
        }
        let id = super::record::hash(&bytes);
        let _facts = facts_lock()?;
        let path = self
            .game(person, &record.game_id, true)?
            .unwrap()
            .join(format!("{}.json", hash_hex(&id)?));
        publish(&path, &mut bytes.as_slice(), None)?;
        Ok(id)
    }
    fn object_path(&self, hash: &str, create: bool) -> Result<PathBuf, SaveError> {
        let hex = hash_hex(hash)?;
        if !directory(&self.root, create)? {
            return Err(SaveError::Missing);
        }
        let objects = self.root.join("objects");
        if !directory(&objects, create)? {
            return Err(SaveError::Missing);
        }
        Ok(objects.join(hex))
    }
    fn verified_object(&self, hash: &str) -> Result<File, SaveError> {
        let mut file = regular(&self.object_path(hash, false)?)?;
        let mut sha = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = file.read(&mut buffer).map_err(io)?;
            if count == 0 {
                break;
            }
            sha.update(&buffer[..count]);
        }
        if hex::encode(sha.finalize()) != hash_hex(hash)? {
            return Err(invalid("stored object hash mismatch"));
        }
        file.seek(SeekFrom::Start(0)).map_err(io)?;
        Ok(file)
    }
    fn authorize_object(&self, person: &str, hash: &str) -> Result<(), SaveError> {
        hash_hex(hash)?;
        if self
            .all_records(person)?
            .iter()
            .flatten()
            .flat_map(|r| &r.locations)
            .flat_map(|l| &l.files)
            .any(|f| f.sha256 == hash)
        {
            Ok(())
        } else {
            Err(SaveError::Missing)
        }
    }
    /// Advisory availability only: objects enter this private store after hash
    /// verification and atomic publication. Presence/size checks never read
    /// payloads or wait for their mutex. Enrichment and launch still verify them.
    fn progress_present(&self, record: &SnapshotRecord) -> bool {
        record
            .locations
            .iter()
            .filter(|location| location.kind != SaveKind::Screenshot)
            .flat_map(|location| &location.files)
            .all(|file| {
                self.object_path(&file.sha256, false)
                    .ok()
                    .and_then(|path| fs::symlink_metadata(path).ok())
                    .is_some_and(|metadata| {
                        metadata.is_file()
                            && !metadata.file_type().is_symlink()
                            && metadata.permissions().mode() & 0o077 == 0
                            && metadata.len() == file.bytes
                    })
            })
    }
    fn progress_complete(&self, record: &SnapshotRecord) -> Result<bool, SaveError> {
        let mut progress = record.clone();
        progress
            .locations
            .retain(|l| l.kind != SaveKind::Screenshot);
        self.record_complete(&progress)
    }
    fn record_complete(&self, record: &SnapshotRecord) -> Result<bool, SaveError> {
        for expected in record.locations.iter().flat_map(|l| &l.files) {
            match self.verified_object(&expected.sha256) {
                Ok(file) if file.metadata().map_err(io)?.len() == expected.bytes => {}
                Ok(_) => return Err(invalid("object size mismatch")),
                Err(SaveError::Missing) => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }
}

pub fn head_ids(records: &[SnapshotRecord]) -> Result<Vec<String>, SaveError> {
    let parents: BTreeSet<_> = records.iter().flat_map(|r| &r.parents).collect();
    let mut heads = Vec::new();
    for record in records {
        let id = snapshot_id(record)?;
        if !parents.contains(&id) {
            heads.push(id);
        }
    }
    heads.sort();
    Ok(heads)
}
impl SaveRepository for NativeSaveStore {
    fn heads(&self, person: &str) -> Result<SaveHeads, SaveError> {
        let _guard = lock()?;
        let mut games = Vec::new();
        for records in self.all_records(person)? {
            if let Some(record) = records.first() {
                games.push(GameSaveHeads {
                    game_id: record.game_id.clone(),
                    heads: head_ids(&records)?,
                });
            }
        }
        games.sort_by(|a, b| a.game_id.cmp(&b.game_id));
        Ok(SaveHeads { games })
    }
    fn records(&self, person: &str, game: &str) -> Result<Vec<SnapshotRecord>, SaveError> {
        let _guard = lock()?;
        self.read_records(person, game)
    }
    fn put_record(&self, person: &str, record: &SnapshotRecord) -> Result<String, SaveError> {
        let _guard = lock()?;
        self.write_record(person, record)
    }
    fn object(&self, person: &str, hash: &str) -> Result<File, SaveError> {
        let _guard = lock()?;
        self.authorize_object(person, hash)?;
        self.verified_object(hash)
    }
    fn put_object(&self, person: &str, hash: &str, input: &mut dyn Read) -> Result<(), SaveError> {
        let _guard = lock()?;
        self.authorize_object(person, hash)?;
        publish(&self.object_path(hash, true)?, input, Some(hash))
    }
    fn complete(&self, person: &str, game: &str, id: &str) -> Result<bool, SaveError> {
        let _guard = lock()?;
        hash_hex(id)?;
        let record = self
            .read_records(person, game)?
            .into_iter()
            .find(|r| snapshot_id(r).is_ok_and(|value| value == id))
            .ok_or(SaveError::Missing)?;
        self.record_complete(&record)
    }
    fn confirmed_snapshots(
        &self,
        person: &str,
        game: &str,
        device: &str,
    ) -> Result<BTreeSet<String>, SaveError> {
        let _guard = lock()?;
        let Some(path) = self.game(person, game, false)? else {
            return Ok(BTreeSet::new());
        };
        // Read the existing signed-receipt projection, not a second cache.
        let confirmations: BTreeMap<String, BTreeSet<String>> =
            match read_json(&path.join("confirmations.json")) {
                Ok(value) => value,
                Err(SaveError::Missing) => BTreeMap::new(),
                Err(error) => return Err(error),
            };
        Ok(confirmations
            .into_iter()
            .filter_map(|(id, devices)| devices.contains(device).then_some(id))
            .collect())
    }
    fn confirm(&self, person: &str, game: &str, id: &str, device: &str) -> Result<(), SaveError> {
        let _guard = lock()?;
        hash_hex(id)?;
        super::account_root(Path::new("/"), device).map_err(SaveError::Invalid)?;
        if !self
            .read_records(person, game)?
            .iter()
            .any(|r| snapshot_id(r).is_ok_and(|value| value == id))
        {
            return Err(SaveError::Missing);
        }
        let path = self
            .game(person, game, false)?
            .ok_or(SaveError::Missing)?
            .join("confirmations.json");
        let mut confirmations: BTreeMap<String, BTreeSet<String>> = match read_json(&path) {
            Ok(v) => v,
            Err(SaveError::Missing) => BTreeMap::new(),
            Err(e) => return Err(e),
        };
        if !confirmations
            .entry(id.into())
            .or_default()
            .insert(device.into())
        {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&confirmations).map_err(|e| invalid(&e.to_string()))?;
        publish(&path, &mut bytes.as_slice(), None)
    }
}

/// Resolve lexical components, rejecting every symlink and non-directory
/// ancestor before any effect. No directory is created just to resolve `..`.
pub(super) fn runtime_save_directory(path: &Path) -> Result<PathBuf, SaveError> {
    use std::path::Component;
    if !path.is_absolute() {
        return Err(invalid("save root must be absolute"));
    }
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
        match fs::symlink_metadata(&normalized) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(invalid("save ancestor is a symlink or not a directory")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(normalized)
}

fn restore_users_root(save_root: &Path, person: &str, game: &str) -> Result<PathBuf, SaveError> {
    let saves = save_root
        .parent()
        .ok_or_else(|| invalid("per-person save root"))?;
    let account = saves
        .parent()
        .ok_or_else(|| invalid("per-person account root"))?;
    let users = account.parent().ok_or_else(|| invalid("users root"))?;
    if save_root.file_name().and_then(|s| s.to_str()) != Some(codec(game).as_str())
        || saves.file_name().and_then(|s| s.to_str()) != Some("saves")
        || account.file_name().and_then(|s| s.to_str()) != Some(codec(person).as_str())
        || users.file_name().and_then(|s| s.to_str()) != Some("users")
    {
        return Err(invalid("restore requires the per-person root inside users"));
    }
    // tmpfiles provisions users/; account scaffolding gives defaults only to
    // the person root (legacy runtime group) and saves/ (named sharing).
    let metadata = fs::symlink_metadata(users).map_err(io)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("users root is not a directory"));
    }
    Ok(users.to_owned())
}

/// Only called on entries we create beneath the validated users/ boundary.
/// 0700/0600 creation masks inherited named-user grants. Reopen that mask,
/// without chown and without changing owner, owning-group, other, or named
/// entries. As in launch_diagnostics.rs, stat's group bits are ACL mask bits.
fn inherited_write_access(file: &File) -> Result<(), SaveError> {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    let size = unsafe {
        libc::fgetxattr(
            fd,
            c"system.posix_acl_access".as_ptr(),
            std::ptr::null_mut(),
            0,
        )
    };
    if size < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENODATA) {
            return Ok(());
        }
        return Err(io(error));
    }
    if !(4..=65536).contains(&size) || (size - 4) % 8 != 0 {
        return Err(invalid("invalid inherited ACL size"));
    }
    let mut acl = vec![0u8; size as usize];
    if unsafe {
        libc::fgetxattr(
            fd,
            c"system.posix_acl_access".as_ptr(),
            acl.as_mut_ptr().cast(),
            acl.len(),
        )
    } != size
    {
        return Err(io(std::io::Error::last_os_error()));
    }
    if acl[..4] != 2u32.to_le_bytes() {
        return Err(invalid("unsupported inherited ACL"));
    }
    let metadata = file.metadata().map_err(io)?;
    let permissions = if metadata.is_dir() {
        7u16
    } else if metadata.is_file() {
        6u16
    } else {
        return Err(invalid("save entry is not a regular file or directory"));
    };
    let mut changed = false;
    for entry in acl[4..].chunks_exact_mut(8) {
        if u16::from_le_bytes([entry[0], entry[1]]) == 16 {
            entry[2..4].copy_from_slice(&permissions.to_le_bytes());
            changed = true;
        }
    }
    if changed
        && unsafe {
            libc::fsetxattr(
                fd,
                c"system.posix_acl_access".as_ptr(),
                acl.as_ptr().cast(),
                acl.len(),
                0,
            )
        } != 0
    {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(())
}

fn runtime_directory(path: &Path, users: &Path) -> Result<(), SaveError> {
    if !path.starts_with(users) {
        return Err(invalid("save directory is outside users"));
    }
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {
            for ancestor in path.ancestors() {
                let m = fs::symlink_metadata(ancestor).map_err(io)?;
                if m.file_type().is_symlink() {
                    return Err(invalid("symlink in save ancestors"));
                }
            }
            Ok(())
        }
        Ok(_) => Err(invalid("save ancestor is not a directory")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && path != users => {
            runtime_directory(path.parent().ok_or_else(|| invalid("save parent"))?, users)?;
            fs::DirBuilder::new().mode(0o700).create(path).map_err(io)?;
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
                .open(path)
                .map_err(io)?;
            inherited_write_access(&file)?;
            file.sync_all().map_err(io)?;
            File::open(path.parent().unwrap())
                .and_then(|f| f.sync_all())
                .map_err(io)
        }
        Err(e) => Err(io(e)),
    }
}
