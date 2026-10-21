//! Ancestry decides continuation. Clocks are display data only.
use super::{record::snapshot_id, types::*, SaveError};
use std::collections::BTreeMap;

pub(super) fn progress(record: &SnapshotRecord) -> Vec<(String, SaveKind, String, String)> {
    let mut files: Vec<_> = record
        .locations
        .iter()
        .filter(|l| l.kind != SaveKind::Screenshot)
        .flat_map(|l| {
            l.files.iter().map(|f| {
                (
                    l.key.clone(),
                    l.kind.clone(),
                    f.path.clone(),
                    f.sha256.clone(),
                )
            })
        })
        .collect();
    files.sort();
    files
}

/// None means progress differs and must never be merged at file granularity.
pub fn automatic_join(heads: &[SnapshotRecord]) -> Result<Option<SnapshotRecord>, SaveError> {
    let Some(first) = heads.first() else {
        return Ok(None);
    };
    let content = progress(first);
    if heads.iter().any(|r| progress(r) != content) {
        return Ok(None);
    }
    let mut ordered: Vec<_> = heads
        .iter()
        .map(|r| Ok((snapshot_id(r)?, r)))
        .collect::<Result<_, SaveError>>()?;
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    let mut result = ordered[0].1.clone();
    result.parents = ordered.iter().map(|(id, _)| id.clone()).collect();
    result.interrupted = false;
    result.locations.retain(|l| l.kind != SaveKind::Screenshot);
    let mut screenshots: BTreeMap<String, BTreeMap<String, BTreeMap<String, SaveFile>>> =
        BTreeMap::new();
    for record in heads {
        for location in &record.locations {
            if location.kind != SaveKind::Screenshot {
                continue;
            }
            for file in &location.files {
                screenshots
                    .entry(location.key.clone())
                    .or_default()
                    .entry(file.path.clone())
                    .or_default()
                    .insert(file.sha256.clone(), file.clone());
            }
        }
    }
    for (key, paths) in screenshots {
        let mut files: BTreeMap<String, SaveFile> = BTreeMap::new();
        for (path, hashes) in paths {
            for (index, (_, mut file)) in hashes.into_iter().enumerate() {
                let mut name = if index == 0 {
                    path.clone()
                } else {
                    collision_name(&path, &file.sha256)
                };
                while files
                    .get(&name)
                    .is_some_and(|old| old.sha256 != file.sha256)
                {
                    name = collision_name(&name, &file.sha256);
                }
                file.path = name.clone();
                files.insert(name, file);
            }
        }
        result.locations.push(SnapshotLocation {
            key,
            kind: SaveKind::Screenshot,
            files: files.into_values().collect(),
        });
    }
    Ok(Some(result))
}

fn collision_name(path: &str, hash: &str) -> String {
    let path = std::path::Path::new(path);
    let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or("");
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| format!(".{v}"))
        .unwrap_or_default();
    path.with_file_name(format!(
        "{stem}~{}{extension}",
        hash.strip_prefix("sha256:").expect("validated hash")
    ))
    .to_string_lossy()
    .into_owned()
}
