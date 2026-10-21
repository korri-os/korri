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
            fs::DirBuilder::new().mode(0o700).create(path).map_err(io)?;
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
        records.sort_by_key(|r| snapshot_id(r).expect("validated record"));
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
        confirmations
            .entry(id.into())
            .or_default()
            .insert(device.into());
        let bytes = serde_json::to_vec(&confirmations).map_err(|e| invalid(&e.to_string()))?;
        publish(&path, &mut bytes.as_slice(), None)
    }
}
