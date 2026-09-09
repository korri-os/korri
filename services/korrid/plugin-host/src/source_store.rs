//! Configured HTTPS catalog URLs. The host's state lock covers every read/write.
use crate::{
    repository::SourceUrl,
    storage::{self, State},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceList {
    sources: Vec<String>,
}

fn list_path(state: &State) -> Result<PathBuf, String> {
    let root = state.root().join(storage::SOURCES_DIR);
    storage::directory(&root)?;
    Ok(root.join("list.json"))
}

pub fn list_sources(state: &State) -> Result<Vec<SourceUrl>, String> {
    let list = storage::read_json::<SourceList>(&list_path(state)?)?.unwrap_or_default();
    let mut seen = HashSet::new();
    list.sources
        .into_iter()
        .map(|raw| {
            let url = SourceUrl::parse(&raw).map_err(|e| format!("invalid stored source: {e}"))?;
            if url.as_str() != raw || !seen.insert(url.clone()) {
                return Err("stored sources must be canonical and unique".into());
            }
            Ok(url)
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Added,
    AlreadyPresent,
}
#[derive(Debug, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed,
    NotFound,
}

pub fn add(state: &State, url: &SourceUrl) -> Result<AddOutcome, String> {
    let mut sources = list_sources(state)?;
    if sources.contains(url) {
        return Ok(AddOutcome::AlreadyPresent);
    }
    sources.push(url.clone());
    save(state, sources)?;
    Ok(AddOutcome::Added)
}
pub fn remove(state: &State, url: &SourceUrl) -> Result<RemoveOutcome, String> {
    let mut sources = list_sources(state)?;
    let before = sources.len();
    sources.retain(|source| source != url);
    if before == sources.len() {
        return Ok(RemoveOutcome::NotFound);
    }
    save(state, sources)?;
    Ok(RemoveOutcome::Removed)
}
fn save(state: &State, sources: Vec<SourceUrl>) -> Result<(), String> {
    storage::write_json(
        &list_path(state)?,
        &SourceList {
            sources: sources.into_iter().map(|s| s.to_string()).collect(),
        },
    )
}

/// This file is supplied by the trusted NixOS module, not downloaded metadata.
/// Missing configuration is a discovery state, not a reason to block recovery.
pub fn official(path: &Path) -> Result<Option<SourceUrl>, String> {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    };
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read official catalog configuration: {e}")),
    };
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err("official catalog configuration must be a caller-owned regular file without group/other writes".into());
    }
    let mut raw = String::new();
    file.take(4098)
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())?;
    SourceUrl::parse(raw.strip_suffix('\n').unwrap_or(&raw)).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn url(s: &str) -> SourceUrl {
        SourceUrl::parse(s).unwrap()
    }
    fn state() -> (tempfile::TempDir, State) {
        let root = tempfile::tempdir().unwrap();
        let state = State::open(&root.path().join("state")).unwrap();
        (root, state)
    }
    #[test]
    fn source_writes_share_the_host_lock_and_keep_sources_separate() {
        let (root, state) = state();
        assert!(State::open(&root.path().join("state")).is_err());
        let a = url("https://a.example/catalog");
        let b = url("https://b.example/catalog");
        assert!(list_sources(&state).unwrap().is_empty());
        assert_eq!(add(&state, &a).unwrap(), AddOutcome::Added);
        assert_eq!(add(&state, &a).unwrap(), AddOutcome::AlreadyPresent);
        add(&state, &b).unwrap();
        assert_eq!(list_sources(&state).unwrap(), [a.clone(), b.clone()]);
        assert_eq!(remove(&state, &a).unwrap(), RemoveOutcome::Removed);
        assert_eq!(remove(&state, &a).unwrap(), RemoveOutcome::NotFound);
        assert_eq!(list_sources(&state).unwrap(), [b]);
    }
    #[test]
    fn stored_invalid_duplicate_and_noncanonical_sources_fail_without_mutation() {
        for sources in [
            vec!["http://example.com/"],
            vec!["https://example.com/", "https://example.com/"],
            vec!["https://EXAMPLE.com"],
        ] {
            let (_root, state) = state();
            let path = list_path(&state).unwrap();
            storage::write_json(
                &path,
                &SourceList {
                    sources: sources.into_iter().map(str::to_owned).collect(),
                },
            )
            .unwrap();
            let before = std::fs::read(&path).unwrap();
            assert!(add(&state, &url("https://valid.example/")).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
    #[test]
    fn source_writes_never_outgrow_the_reader() {
        let (_root, state) = state();
        let mut rejected = false;
        for number in 0..40 {
            let source = url(&format!(
                "https://example.com/{number}/{}",
                "a".repeat(3900)
            ));
            if add(&state, &source).is_err() {
                rejected = true;
                break;
            }
        }
        assert!(rejected);
        assert!(list_sources(&state).is_ok());
    }
    #[test]
    fn official_status_comes_only_from_explicit_configuration() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("official");
        assert!(official(&path).unwrap().is_none());
        std::fs::write(&path, "https://owner.example/catalog\n").unwrap();
        assert_eq!(
            official(&path).unwrap().unwrap(),
            url("https://owner.example/catalog")
        );
        std::fs::write(&path, "http://owner.example/catalog").unwrap();
        assert!(official(&path).is_err());
    }
}
