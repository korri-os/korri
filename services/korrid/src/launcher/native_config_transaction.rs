//! Durable one-launch TOML recovery, using the native writer's before/after bytes.
//! Receipts belong to accountRoot and are never sent to a plugin or put in argv.
use super::{native_toml, ProvisionedFile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, Table};

const DIRECTORY: &str = ".korri-native-config";
const MAX_RECEIPT_BYTES: u64 = 16 * 1024 * 1024;

/// Counterfactual effects from the same callback and display, not a second key map.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TemporaryToml {
    pub persistent: ProvisionedFile,
    pub effective: ProvisionedFile,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    original: ProvisionedFile,
    applied: ProvisionedFile,
}

struct Lease {
    _file: fs::File,
    receipt: PathBuf,
}

pub(super) struct Transaction {
    account: PathBuf,
    leases: Vec<Lease>,
    finished: bool,
}

fn document(text: &str) -> Result<DocumentMut, String> {
    text.parse()
        .map_err(|_| "native TOML recovery document is invalid".into())
}

fn semantic(item: &Item) -> Result<toml::Value, String> {
    let mut one = DocumentMut::new();
    one["value"] = item.clone();
    let value: toml::Value =
        toml::from_str(&one.to_string()).map_err(|_| "native TOML value is invalid".to_owned())?;
    value
        .get("value")
        .cloned()
        .ok_or_else(|| "missing native TOML value".into())
}

fn leaves(doc: &DocumentMut) -> Result<BTreeMap<Vec<String>, Item>, String> {
    fn walk(table: &Table, path: &mut Vec<String>, out: &mut BTreeMap<Vec<String>, Item>) {
        for (key, item) in table {
            path.push(key.into());
            if let Item::Table(table) = item {
                walk(table, path, out);
            } else {
                out.insert(path.clone(), item.clone());
            }
            path.pop();
        }
    }
    let mut result = BTreeMap::new();
    walk(doc.as_table(), &mut Vec::new(), &mut result);
    Ok(result)
}
fn equal(a: Option<&Item>, b: Option<&Item>) -> Result<bool, String> {
    match (a, b) {
        (None, None) => Ok(true),
        (Some(a), Some(b)) => {
            let (a, b) = (semantic(a)?, semantic(b)?);
            // TOML permits nan. Matching config values must remain restorable
            // even though IEEE arithmetic says NaN is unequal to itself.
            Ok(matches!((&a, &b), (toml::Value::Float(a), toml::Value::Float(b)) if a.is_nan() && b.is_nan()) || a == b)
        }
        _ => Ok(false),
    }
}
fn set(table: &mut Table, path: &[String], value: Option<&Item>) -> Result<(), String> {
    if path.len() == 1 {
        if let Some(value) = value {
            let mut value = value.clone();
            if let (Some(old), Some(new)) = (
                table.get(&path[0]).and_then(Item::as_value),
                value.as_value_mut(),
            ) {
                *new.decor_mut() = old.decor().clone();
            }
            if let Some(old) = table.get_mut(&path[0]) {
                if !old.is_value() {
                    return Err("native TOML update conflicts with an existing table".into());
                }
                *old = value;
            } else {
                table.insert(&path[0], value);
            }
        } else {
            table.remove(&path[0]);
        }
        return Ok(());
    }
    if !table.contains_key(&path[0]) {
        if value.is_none() {
            return Ok(());
        }
        table.insert(&path[0], Item::Table(Table::new()));
    }
    let child = table
        .get_mut(&path[0])
        .and_then(Item::as_table_mut)
        .ok_or("native TOML table changed shape during the launch")?;
    set(child, &path[1..], value)
}

/// Restore only changed leaves still equal to the applied state. Other keys and
/// later menu edits survive, including keys absent before the temporary change.
fn restore_text(original: &str, applied: &str, current: &str) -> Result<String, String> {
    let before = leaves(&document(original)?)?;
    let after = leaves(&document(applied)?)?;
    let mut current_doc = document(current)?;
    let now = leaves(&current_doc)?;
    for path in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        if !equal(before.get(path), after.get(path))? && equal(now.get(path), after.get(path))? {
            let mut parent = current_doc.as_table();
            let mut intact = true;
            for segment in &path[..path.len() - 1] {
                if let Some(table) = parent.get(segment).and_then(Item::as_table) {
                    parent = table;
                } else {
                    intact = false;
                    break;
                }
            }
            if !intact {
                continue;
            } // A later edit removed/replaced the whole section.
            set(current_doc.as_table_mut(), path, before.get(path))?;
        }
    }
    Ok(current_doc.to_string())
}

/// Merge each program's native fragments in effect order before comparing plans.
fn grouped(updates: &[ProvisionedFile]) -> Result<BTreeMap<String, DocumentMut>, String> {
    let mut result = BTreeMap::new();
    for update in updates {
        native_toml::validate(update)?;
        let doc = result
            .entry(update.path.clone())
            .or_insert_with(DocumentMut::new);
        native_toml::merge(doc.as_table_mut(), document(&update.content)?.as_table())?;
    }
    Ok(result)
}
pub(super) fn changes(
    persistent: &[ProvisionedFile],
    effective: &[ProvisionedFile],
) -> Result<Vec<TemporaryToml>, String> {
    let before = grouped(persistent)?;
    let after = grouped(effective)?;
    let mut result = Vec::new();
    for path in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        if before.contains_key(path) && !after.contains_key(path) {
            return Err("one-launch native effects must retain their file targets; use an empty TOML fragment when clearing settings".into());
        }
        let original = before
            .get(path)
            .map(ToString::to_string)
            .unwrap_or_default();
        let applied = after.get(path).map(ToString::to_string).unwrap_or_default();
        let a = leaves(&document(&original)?)?;
        let b = leaves(&document(&applied)?)?;
        let mut different = false;
        for key in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
            different |= !equal(a.get(key), b.get(key))?;
        }
        if different {
            result.push(TemporaryToml {
                persistent: ProvisionedFile {
                    path: path.clone(),
                    content: original,
                },
                effective: ProvisionedFile {
                    path: path.clone(),
                    content: applied,
                },
            });
        }
    }
    Ok(result)
}

fn directory(account: &Path, create: bool) -> Result<Option<PathBuf>, String> {
    let path = account.join(DIRECTORY);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_dir()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
            {
                return Err(
                    "native recovery directory must be private and owned by the runtime user"
                        .into(),
                );
            }
            Ok(Some(path))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !create => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .map_err(|e| e.to_string())?;
            sync_dir(account)?;
            Ok(Some(path))
        }
        Err(e) => Err(e.to_string()),
    }
}
fn key(account: &Path, update: &ProvisionedFile) -> Result<String, String> {
    native_toml::validate_location(account, update)?;
    let root = account.canonicalize().map_err(|e| e.to_string())?;
    let path = Path::new(&update.path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if path == root || !path.starts_with(root) {
        return Err("native recovery path escapes accountRoot".into());
    }
    Ok(hex::encode(Sha256::digest(path.as_os_str().as_bytes())))
}
fn acquire(dir: &Path, key: &str) -> Result<Option<Lease>, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join(format!("{key}.lock")))
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err("invalid native configuration lease".into());
    }
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            return Ok(None);
        }
        return Err(error.to_string());
    }
    Ok(Some(Lease {
        _file: file,
        receipt: dir.join(format!("{key}.json")),
    }))
}
fn sync_dir(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())
}
fn write_receipt(path: &Path, receipt: &Receipt) -> Result<(), String> {
    let bytes = serde_json::to_vec(receipt).map_err(|_| "cannot encode native recovery receipt")?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("native recovery receipt is too large".into());
    }
    let parent = path.parent().ok_or("receipt has no parent")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    file.persist_noclobber(path)
        .map_err(|_| "native recovery receipt already exists".to_owned())?;
    sync_dir(parent)
}
fn recover(account: &Path, lease: &Lease) -> Result<(), String> {
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&lease.receipt)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err("invalid native recovery receipt ownership".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("native recovery receipt is too large".into());
    }
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .map_err(|_| "native recovery receipt is invalid; no values were discarded")?;
    let account = account.canonicalize().map_err(|e| e.to_string())?;
    native_toml::validate_location(&account, &receipt.original)?;
    let path = Path::new(&receipt.original.path);
    let expected = hex::encode(Sha256::digest(path.as_os_str().as_bytes()));
    if receipt.original.path != receipt.applied.path
        || lease.receipt.file_stem().and_then(|s| s.to_str()) != Some(&expected)
    {
        return Err("native recovery receipt does not match its file".into());
    }
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Deleting the file is a later edit too. Never recreate it, but
            // retain the receipt: it can be the last copy of the original.
            let archive = lease
                .receipt
                .with_extension(format!("removed-{:016x}.bak", rand::random::<u64>()));
            fs::hard_link(&lease.receipt, archive).map_err(|e| e.to_string())?;
            sync_dir(lease.receipt.parent().unwrap())?;
            fs::remove_file(&lease.receipt).map_err(|e| e.to_string())?;
            return sync_dir(lease.receipt.parent().unwrap());
        }
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    if path.canonicalize().map_err(|e| e.to_string())? != path {
        return Err("native recovery target changed identity".into());
    }
    let current = native_toml::read_checked(&account, path)?;
    let restored = restore_text(
        &receipt.original.content,
        &receipt.applied.content,
        &current,
    )?;
    native_toml::write_checked(&account, path, &current, &restored)?;
    fs::remove_file(&lease.receipt).map_err(|e| e.to_string())?;
    sync_dir(lease.receipt.parent().unwrap())
}

impl Transaction {
    pub fn begin(
        account: &Path,
        persistent: &[ProvisionedFile],
        temporary: &[TemporaryToml],
    ) -> Result<Self, String> {
        if !account.is_absolute() {
            return Err("accountRoot must be absolute".into());
        }
        for update in persistent {
            native_toml::validate(update)?;
            native_toml::validate_location(account, update)?;
        }
        for change in temporary {
            if change.persistent.path != change.effective.path {
                return Err("temporary native file target changed".into());
            }
            native_toml::validate(&change.persistent)?;
            native_toml::validate(&change.effective)?;
            native_toml::validate_location(account, &change.effective)?;
        }
        let mut transaction = Self {
            account: account.into(),
            leases: Vec::new(),
            finished: false,
        };
        // Recovery also runs when this launch authors no settings at all.
        if let Some(dir) = directory(account, false)? {
            for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("invalid recovery receipt name")?;
                if name.len() != 64 || !name.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err("invalid recovery receipt name".into());
                }
                if let Some(lease) = acquire(&dir, name)? {
                    recover(account, &lease)?;
                }
            }
        }
        if persistent.is_empty() && temporary.is_empty() {
            return Ok(transaction);
        }
        let dir = directory(account, true)?.unwrap();
        let mut targets = BTreeMap::new();
        for update in persistent
            .iter()
            .chain(temporary.iter().map(|change| &change.effective))
        {
            targets.insert(key(account, update)?, update.path.clone());
        }
        for name in targets.keys() {
            let lease = acquire(&dir, name)?
                .ok_or("this native configuration is still used by another launch")?;
            recover(account, &lease)?;
            transaction.leases.push(lease);
        }
        for update in persistent {
            native_toml::apply(account, update)?;
        }
        for change in temporary {
            if change.persistent.path != change.effective.path {
                return Err("temporary native file target changed".into());
            }
            native_toml::validate(&change.persistent)?;
            native_toml::validate(&change.effective)?;
            let path = Path::new(&change.effective.path);
            let original = native_toml::read_checked(account, path)?;
            let mut applied = document(&original)?;
            let before = leaves(&document(&change.persistent.content)?)?;
            let after = leaves(&document(&change.effective.content)?)?;
            for key in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
                if !equal(before.get(key), after.get(key))? {
                    set(applied.as_table_mut(), key, after.get(key))?;
                }
            }
            let applied = applied.to_string();
            if applied == original {
                continue;
            }
            let name = key(account, &change.effective)?;
            let lease = transaction
                .leases
                .iter()
                .find(|l| l.receipt.file_stem().and_then(|s| s.to_str()) == Some(&name))
                .unwrap();
            let canonical = path
                .canonicalize()
                .map_err(|e| e.to_string())?
                .into_os_string()
                .into_string()
                .map_err(|_| "native configuration path must be UTF-8")?;
            write_receipt(
                &lease.receipt,
                &Receipt {
                    original: ProvisionedFile {
                        path: canonical.clone(),
                        content: original.clone(),
                    },
                    applied: ProvisionedFile {
                        path: canonical,
                        content: applied.clone(),
                    },
                },
            )?;
            native_toml::write_checked(account, path, &original, &applied)?;
        }
        Ok(transaction)
    }
    fn restore(&self) -> Result<(), String> {
        let mut first = None;
        for lease in &self.leases {
            if let Err(error) = recover(&self.account, lease) {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
    pub fn finish(mut self) -> Result<(), String> {
        let result = self.restore();
        self.finished = true;
        result
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.restore() {
                eprintln!("native configuration recovery remains pending: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn update(path: &Path, content: &str) -> ProvisionedFile {
        ProvisionedFile {
            path: path.display().to_string(),
            content: content.into(),
        }
    }
    #[test]
    fn a_temporary_nan_restores_like_other_valid_toml_scalars() {
        let restored = restore_text("value = 1.0\n", "value = nan\n", "value = nan\n").unwrap();
        assert_eq!(restored, "value = 1.0\n");
        assert_eq!(restore_text("value = 1.0\n", "value = nan\n", "value = 3.0\n").unwrap(), "value = 3.0\n");
    }

    #[test]
    fn one_launch_reverts_to_permanent_values_without_reverting_menu_edits() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Config.toml");
        fs::write(
            &path,
            "# owner\n[video]\nscale = 1\n[audio]\nvolume = 0.5\n",
        )
        .unwrap();
        let base = vec![update(&path, "[video]\nscale = 2\n")];
        let effective = vec![update(&path, "[video]\nscale = 4\nextra = true\n")];
        let tx =
            Transaction::begin(root.path(), &base, &changes(&base, &effective).unwrap()).unwrap();
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("volume = 0.5", "volume = 0.75");
        fs::write(&path, text).unwrap();
        tx.finish().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("scale = 2"));
        assert!(text.contains("volume = 0.75"));
        assert!(!text.contains("extra ="));
        assert!(text.starts_with("# owner"));
    }
    #[test]
    fn a_later_menu_value_wins_over_rollback_and_cleared_keys_return() {
        let before = "[video]\nscale = 2\nvsync = true\n";
        let applied = "[video]\nscale = 4\n";
        let current = "[video]\nscale = 6 # edited in menu\n";
        let restored = restore_text(before, applied, current).unwrap();
        assert!(restored.contains("scale = 6 # edited in menu"));
        assert!(restored.contains("vsync = true"));
    }
    #[test]
    fn recovery_with_no_new_settings_repairs_an_interrupted_write() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Config.toml");
        let before = "[video]\nscale = 1\n";
        let after = "[video]\nscale = 4\n";
        fs::write(&path, after).unwrap();
        let dir = directory(root.path(), true).unwrap().unwrap();
        let value = update(&path, before);
        let lease = acquire(&dir, &key(root.path(), &value).unwrap())
            .unwrap()
            .unwrap();
        write_receipt(
            &lease.receipt,
            &Receipt {
                original: value,
                applied: update(&path, after),
            },
        )
        .unwrap();
        drop(lease);
        Transaction::begin(root.path(), &[], &[])
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), before);
    }
    #[test]
    fn recovery_never_changes_a_file_while_its_launch_holds_the_lease() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Config.toml");
        fs::write(&path, "x = 1\n").unwrap();
        let update = update(&path, "x = 4\n");
        let temp = changes(&[], &[update.clone()]).unwrap();
        let active = Transaction::begin(root.path(), &[], &temp).unwrap();
        Transaction::begin(root.path(), &[], &[])
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "x = 4\n");
        assert!(Transaction::begin(root.path(), &[update], &[]).is_err());
        active.finish().unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "x = 1\n");
    }
    #[test]
    fn temporary_writes_refuse_structural_conflicts_without_losing_child_keys() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Config.toml");
        let original = "[video.scale]\nowned = 1\n";
        fs::write(&path, original).unwrap();
        let temp = changes(&[], &[update(&path, "[video]\nscale = 4\n")]).unwrap();
        assert!(Transaction::begin(root.path(), &[], &temp).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }
    #[test]
    fn recovery_preserves_a_removed_section_or_a_deleted_file() {
        assert_eq!(
            restore_text("[video]\nscale = 2\n", "[video]\n", "[audio]\nvolume = 1\n").unwrap(),
            "[audio]\nvolume = 1\n"
        );
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Config.toml");
        fs::write(&path, "x = 1\n").unwrap();
        let tx = Transaction::begin(
            root.path(),
            &[],
            &changes(&[], &[update(&path, "x = 4\n")]).unwrap(),
        )
        .unwrap();
        fs::remove_file(&path).unwrap();
        tx.finish().unwrap();
        assert!(!path.exists());
    }
    #[test]
    fn failure_on_a_later_file_restores_earlier_temporary_updates() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a.toml");
        let b = root.path().join("b.toml");
        fs::write(&a, "x = 1\n").unwrap();
        fs::write(&b, "broken [").unwrap();
        let temp = changes(&[], &[update(&a, "x = 4\n"), update(&b, "x = 4\n")]).unwrap();
        assert!(Transaction::begin(root.path(), &[], &temp).is_err());
        assert_eq!(fs::read_to_string(a).unwrap(), "x = 1\n");
        assert_eq!(fs::read_to_string(b).unwrap(), "broken [");
    }
    #[test]
    fn a_receipt_durable_before_the_write_does_not_change_unapplied_values() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Config.toml");
        fs::write(&path, "x = 1\n").unwrap();
        let dir = directory(root.path(), true).unwrap().unwrap();
        let original = update(&path, "x = 1\n");
        let lease = acquire(&dir, &key(root.path(), &original).unwrap())
            .unwrap()
            .unwrap();
        write_receipt(
            &lease.receipt,
            &Receipt {
                original,
                applied: update(&path, "x = 4\n"),
            },
        )
        .unwrap();
        drop(lease);
        Transaction::begin(root.path(), &[], &[])
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "x = 1\n");
    }
    #[test]
    fn corrupt_receipts_are_retained_instead_of_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Config.toml");
        fs::write(&path, "x = 1\n").unwrap();
        let dir = directory(root.path(), true).unwrap().unwrap();
        let id = key(root.path(), &update(&path, "")).unwrap();
        let receipt = dir.join(format!("{id}.json"));
        fs::write(&receipt, "broken").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&receipt, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(Transaction::begin(root.path(), &[], &[]).is_err());
        assert_eq!(fs::read_to_string(receipt).unwrap(), "broken");
    }
}
