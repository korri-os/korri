//! Native TOML updates execute with the game's authority, never in plugin JS.
use super::ProvisionedFile;
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};
use toml_edit::{DocumentMut, Item, Table};

pub(super) fn validate(update: &ProvisionedFile) -> Result<(), String> {
    if !Path::new(&update.path).is_absolute() || update.path.contains('\0') {
        return Err("native TOML path must be absolute and contain no NUL".into());
    }
    let patch = update
        .content
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
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

fn merge(base: &mut Table, patch: &Table) -> Result<(), String> {
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

pub(super) fn apply(update: &ProvisionedFile) -> Result<(), String> {
    validate(update)?;
    let path = Path::new(&update.path);
    let parent = path.parent().ok_or("native TOML file has no parent")?;
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("native TOML updates require an existing regular file, not a symlink".into());
    }
    let original = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut document = original.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    let patch = update
        .content
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
    merge(document.as_table_mut(), patch.as_table())?;
    let updated = document.to_string();
    if updated == original {
        return Ok(());
    }
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
            .set_permissions(metadata.permissions())
            .map_err(|e| e.to_string())?;
        output
            .write_all(updated.as_bytes())
            .map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        if !fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .is_file()
            || fs::read_to_string(path).map_err(|e| e.to_string())? != original
        {
            return Err("native TOML file changed during preparation".into());
        }
        let mut saved = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&backup)
            .map_err(|e| e.to_string())?;
        saved
            .set_permissions(metadata.permissions())
            .map_err(|e| e.to_string())?;
        saved
            .write_all(original.as_bytes())
            .map_err(|e| e.to_string())?;
        saved.sync_all().map_err(|e| e.to_string())?;
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
        apply(&patch).unwrap();
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
        apply(&patch).unwrap();
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
            assert!(apply(&update(&path, "[video]\nvsync = true\n")).is_err());
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
        assert!(apply(&update(&link, "[video]\nvsync = true")).is_err());
        apply(&update(&path, "[video]\nvsync = true")).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
