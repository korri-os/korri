//! Native TOML updates execute with the game's authority, never in plugin JS.
use super::ProvisionedFile;
use std::{
    fs,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
};
use xattr::FileExt;

const MAX_NATIVE_BYTES: u64 = 1024 * 1024;
use toml_edit::{DocumentMut, Item, Table};

pub(super) fn validate(update: &ProvisionedFile) -> Result<(), String> {
    if update.content.len() as u64 > MAX_NATIVE_BYTES {
        return Err("native TOML fragment exceeds the 1 MiB limit".into());
    }
    if !Path::new(&update.path).is_absolute() || update.path.contains('\0') {
        return Err("native TOML path must be absolute and contain no NUL".into());
    }
    let patch = update
        .content
        .parse::<DocumentMut>()
        .map_err(|_| "native TOML fragment is invalid".to_owned())?;
    fn scalars(table: &Table) -> bool {
        table.iter().all(|(_, item)| match item {
            Item::Table(table) => scalars(table),
            Item::Value(value) => {
                value.is_bool() || value.is_integer() || value.is_float() || value.is_str()
            }
            _ => false,
        })
    }
    if !scalars(patch.as_table()) {
        return Err("native TOML updates contain only tables and scalar assignments".into());
    }
    Ok(())
}

pub(super) fn merge(base: &mut Table, patch: &Table) -> Result<(), String> {
    for (key, item) in patch.iter() {
        if let Item::Table(table) = item {
            if !base.contains_key(key) {
                base.insert(key, Item::Table(Table::new()));
            }
            let target = base
                .get_mut(key)
                .and_then(Item::as_table_mut)
                .ok_or_else(|| {
                    format!("native TOML table {key} conflicts with an existing value")
                })?;
            merge(target, table)?;
        } else {
            let mut replacement = item.clone();
            if let (Some(old), Some(new)) = (
                base.get(key).and_then(Item::as_value),
                replacement.as_value_mut(),
            ) {
                *new.decor_mut() = old.decor().clone();
            }
            // Replacing an item retains its original key spelling/decoration.
            if let Some(target) = base.get_mut(key) {
                if !target.is_value() {
                    return Err(format!(
                        "native TOML value {key} conflicts with an existing table"
                    ));
                }
                *target = replacement;
            } else {
                base.insert(key, replacement);
            }
        }
    }
    Ok(())
}

pub(super) fn validate_location(account: &Path, update: &ProvisionedFile) -> Result<(), String> {
    let path = Path::new(&update.path);
    if !account.is_absolute()
        || !path.starts_with(account)
        || path == account
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("native TOML updates must stay inside accountRoot".into());
    }
    Ok(())
}

fn checked_file(account: &Path, path: &Path) -> Result<fs::File, String> {
    validate_location(
        account,
        &ProvisionedFile {
            path: path.display().to_string(),
            content: String::new(),
        },
    )?;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("native TOML updates require an existing regular file, not a symlink".into());
    }
    let root = account.canonicalize().map_err(|e| e.to_string())?;
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if canonical == root || !canonical.starts_with(root) {
        return Err("native TOML update resolves outside accountRoot".into());
    }
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err("native TOML file must be owned by the runtime user".into());
    }
    Ok(file)
}
pub(super) fn read_checked(account: &Path, path: &Path) -> Result<String, String> {
    let file = checked_file(account, path)?;
    let mut text = String::new();
    file.take(MAX_NATIVE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > MAX_NATIVE_BYTES {
        return Err("native TOML file exceeds the 1 MiB limit".into());
    }
    Ok(text)
}
pub(super) fn apply(account: &Path, update: &ProvisionedFile) -> Result<(), String> {
    validate(update)?;
    let path = Path::new(&update.path);
    let original = read_checked(account, path)?;
    let mut document = original
        .parse::<DocumentMut>()
        .map_err(|_| "native TOML file is invalid; no changes were written".to_owned())?;
    let patch = update
        .content
        .parse::<DocumentMut>()
        .map_err(|_| "native TOML fragment is invalid".to_owned())?;
    merge(document.as_table_mut(), patch.as_table())?;
    let updated = document.to_string();
    write_checked(account, path, &original, &updated)
}

// Keep the file's readable metadata, including POSIX ACLs. A parent directory's
// newly inherited ACL must not grant access the original file did not grant.
fn copy_metadata(source: &fs::File, target: &fs::File) -> Result<(), String> {
    let source_meta = source.metadata().map_err(|e| e.to_string())?;
    if target.metadata().map_err(|e| e.to_string())?.gid() != source_meta.gid()
        && unsafe { libc::fchown(target.as_raw_fd(), u32::MAX, source_meta.gid()) } != 0
    {
        return Err("cannot preserve the native TOML file group".into());
    }
    target
        .set_permissions(source_meta.permissions())
        .map_err(|e| e.to_string())?;
    let mut attributes = std::collections::BTreeMap::new();
    for name in source.list_xattr().map_err(|e| e.to_string())? {
        let bytes = source
            .get_xattr(&name)
            .map_err(|e| e.to_string())?
            .ok_or("native TOML attributes changed during preparation")?;
        attributes.insert(name, bytes);
    }
    for name in target.list_xattr().map_err(|e| e.to_string())? {
        if !attributes.contains_key(&name) {
            target.remove_xattr(name).map_err(|e| e.to_string())?;
        }
    }
    for (name, bytes) in attributes {
        if target
            .get_xattr(&name)
            .map_err(|e| e.to_string())?
            .as_deref()
            != Some(bytes.as_slice())
        {
            target.set_xattr(name, &bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn write_checked(
    account: &Path,
    path: &Path,
    original: &str,
    updated: &str,
) -> Result<(), String> {
    let source = checked_file(account, path)?;
    let metadata = source.metadata().map_err(|e| e.to_string())?;
    if updated.len() as u64 > MAX_NATIVE_BYTES {
        return Err("native TOML update exceeds the 1 MiB limit".into());
    }
    if updated == original {
        return Ok(());
    }
    let parent = path.parent().ok_or("native TOML file has no parent")?;
    let suffix = format!("{:016x}", rand::random::<u64>());
    let temporary = parent.join(format!(".korri-toml-{suffix}.tmp"));
    let backup = parent.join(format!(".korri-toml-{suffix}.bak"));
    let result = (|| -> Result<(), String> {
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        output
            .write_all(updated.as_bytes())
            .map_err(|e| e.to_string())?;
        copy_metadata(&source, &output)?;
        output.sync_all().map_err(|e| e.to_string())?;
        let mut saved = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&backup)
            .map_err(|e| e.to_string())?;
        saved
            .write_all(original.as_bytes())
            .map_err(|e| e.to_string())?;
        copy_metadata(&source, &saved)?;
        saved.sync_all().map_err(|e| e.to_string())?;
        let current = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !current.is_file()
            || current.dev() != metadata.dev()
            || current.ino() != metadata.ino()
            || current.ctime() != metadata.ctime()
            || current.ctime_nsec() != metadata.ctime_nsec()
            || read_checked(account, path)? != original
        {
            return Err("native TOML file changed during preparation".into());
        }
        fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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
    fn named_user_acl() -> Vec<u8> {
        let mut bytes = 2_u32.to_le_bytes().to_vec();
        for (tag, permissions, id) in [
            (1_u16, 6_u16, u32::MAX),
            (2, 4, 65534),
            (4, 0, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&permissions.to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn keeps_acl_group_and_extended_attributes_on_updated_file_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Config.toml");
        fs::write(&path, "value = 1\n").unwrap();
        let source = fs::File::open(&path).unwrap();
        let acl = named_user_acl();
        source.set_xattr("system.posix_acl_access", &acl).unwrap();
        source.set_xattr("user.korri-test", b"retained").unwrap();
        let metadata = source.metadata().unwrap();
        apply(dir.path(), &update(&path, "value = 2\n")).unwrap();
        for entry in fs::read_dir(dir.path()).unwrap() {
            let file = fs::File::open(entry.unwrap().path()).unwrap();
            assert_eq!(
                file.get_xattr("system.posix_acl_access").unwrap(),
                Some(acl.clone())
            );
            assert_eq!(
                file.get_xattr("user.korri-test").unwrap(),
                Some(b"retained".to_vec())
            );
            let updated = file.metadata().unwrap();
            assert_eq!(
                (updated.uid(), updated.gid(), updated.mode()),
                (metadata.uid(), metadata.gid(), metadata.mode())
            );
        }
    }

    #[test]
    fn a_new_parent_acl_does_not_grant_access_to_the_replacement_or_backup() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Config.toml");
        fs::write(&path, "value = 1\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        fs::File::open(dir.path())
            .unwrap()
            .set_xattr("system.posix_acl_default", &named_user_acl())
            .unwrap();
        apply(dir.path(), &update(&path, "value = 2\n")).unwrap();
        for entry in fs::read_dir(dir.path()).unwrap() {
            let file = fs::File::open(entry.unwrap().path()).unwrap();
            assert_eq!(file.get_xattr("system.posix_acl_access").unwrap(), None);
            assert_eq!(file.metadata().unwrap().mode() & 0o777, 0o640);
        }
    }

    #[test]
    fn oversized_native_documents_refuse_before_creating_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Config.toml");
        let original = format!("#{}\nvalue = 1\n", "x".repeat(MAX_NATIVE_BYTES as usize));
        fs::write(&path, &original).unwrap();
        assert!(apply(dir.path(), &update(&path, "value = 2\n"))
            .unwrap_err()
            .contains("1 MiB"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn refuses_other_accounts_and_symlinked_ancestors() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let account = root.path().join("account");
        let other = root.path().join("other");
        fs::create_dir(&account).unwrap();
        fs::create_dir(&other).unwrap();
        let outside = other.join("Config.toml");
        fs::write(&outside, "[video]\nvsync = false\n").unwrap();
        symlink(&other, account.join("escape")).unwrap();
        for path in [
            &outside,
            &account.join("escape/Config.toml"),
            &account.join("../other/Config.toml"),
        ] {
            assert!(apply(&account, &update(path, "[video]\nvsync = true")).is_err());
        }
        assert_eq!(
            fs::read_to_string(outside).unwrap(),
            "[video]\nvsync = false\n"
        );
        assert_eq!(fs::read_dir(other).unwrap().count(), 1);
    }

    #[test]
    fn parse_errors_do_not_disclose_existing_config_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Config.toml");
        let original = "personal_label = do-not-include-this-in-diagnostics\n";
        fs::write(&path, original).unwrap();
        let error = apply(dir.path(), &update(&path, "[video]\nvsync = true")).unwrap_err();
        assert!(!error.contains("personal_label"));
        assert!(!error.contains("do-not-include"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn changes_only_declared_values_and_retains_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Config.toml");
        let original = "# user comment\n[video]\nresolution_multiplier = 1.0 # keep comment\nwindow_width = 854\n\n[audio]\nvolume = 0.75 # keep this too\n";
        fs::write(&path, original).unwrap();
        let patch = update(
            &path,
            "[video]\nresolution_multiplier = 4.0\nwindow_width = 2448\n",
        );
        apply(dir.path(), &patch).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            original
                .replace("= 1.0", "= 4.0")
                .replace("= 854", "= 2448")
        );
        let files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|p| p.unwrap().path())
            .collect();
        assert_eq!(files.len(), 2);
        assert_eq!(
            fs::read_to_string(
                files
                    .iter()
                    .find(|p| p.extension().is_some_and(|e| e == "bak"))
                    .unwrap()
            )
            .unwrap(),
            original
        );
        apply(dir.path(), &patch).unwrap();
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            2,
            "no backup on unchanged launch"
        );
    }
    #[test]
    fn refuses_malformed_input_and_structural_conflicts_without_writes() {
        for original in ["[broken", "video = 3\n"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("Config.toml");
            fs::write(&path, original).unwrap();
            assert!(apply(dir.path(), &update(&path, "[video]\nvsync = true\n")).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
    #[test]
    fn rejects_symlinks_and_preserves_mode() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Config.toml");
        let link = dir.path().join("link");
        fs::write(&path, "[video]\nvsync = false\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&path, &link).unwrap();
        assert!(apply(dir.path(), &update(&link, "[video]\nvsync = true")).is_err());
        apply(dir.path(), &update(&path, "[video]\nvsync = true")).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
