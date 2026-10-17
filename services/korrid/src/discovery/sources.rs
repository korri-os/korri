//! Pairing a folder with one system or group (ADR 0001). Owned by the
//! `sources` builder; contract sections 2, 4 and 6.

use std::path::{Path, PathBuf};

use crate::discovery::identify::SourceView;
use crate::plugin::PluginRegistry;

/// A request to pair `root` with `system` (a system id or a group id).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingRequest<'a> {
    pub root: &'a Path,
    pub system: &'a str,
    /// korrid's readable root; `<root>/users` is plugin data (ADR 0003).
    pub readable_root: &'a Path,
    pub private_root: &'a Path,
    /// The source being changed, which does not overlap itself.
    pub replacing: Option<&'a str>,
}

/// Contract section 6 error codes for pairing.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PairingError {
    #[error("the folder overlaps paired source {0}")]
    SourceOverlap(String),
    #[error("the folder holds plugin data")]
    PluginData,
    #[error("no enabled plugin declares system or group {0}")]
    UnknownSystem(String),
    #[error("unknown source {0}")]
    UnknownSource(String),
    #[error("folder is not usable: {0}")]
    InvalidPath(String),
}

/// Refuses overlap, plugin data and unknown systems (contract section 4).
pub fn validate_pairing(
    request: &PairingRequest<'_>,
    existing: &[SourceView],
    registry: &PluginRegistry,
) -> Result<(), PairingError> {
    validate_with(request, existing, |id| {
        registry.pairing_target(id).is_some()
    })
}

fn validate_with(
    request: &PairingRequest<'_>,
    existing: &[SourceView],
    known: impl FnOnce(&str) -> bool,
) -> Result<(), PairingError> {
    let root = request
        .root
        .canonicalize()
        .map_err(|_| PairingError::InvalidPath("folder is unavailable".into()))?;
    if root.to_str().is_none() {
        return Err(PairingError::InvalidPath("path must be UTF-8".into()));
    }
    if !root.is_dir() {
        return Err(PairingError::InvalidPath("not a directory".into()));
    }
    if let Some(id) = request.replacing {
        if !existing.iter().any(|source| source.source_id == id) {
            return Err(PairingError::UnknownSource(id.into()));
        }
    }
    if excluded_roots(request.readable_root, request.private_root)
        .iter()
        .any(|excluded| overlaps(&root, excluded))
    {
        return Err(PairingError::PluginData);
    }
    for source in existing {
        if Some(source.source_id.as_str()) == request.replacing {
            continue;
        }
        let existing_root = source
            .root
            .canonicalize()
            .unwrap_or_else(|_| source.root.clone());
        if overlaps(&root, &existing_root) {
            return Err(PairingError::SourceOverlap(source.source_id.clone()));
        }
    }
    if !known(request.system) {
        return Err(PairingError::UnknownSystem(request.system.into()));
    }
    Ok(())
}

pub fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

// Resolve existing ancestors too: users/ need not exist yet when a folder is paired.
pub(crate) fn canonical_future_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        return canonical_future_path(parent).join(name);
    }
    path.to_owned()
}

pub fn excluded_roots(readable_root: &Path, private_root: &Path) -> Vec<PathBuf> {
    vec![
        canonical_future_path(&readable_root.join("users")),
        canonical_future_path(private_root),
    ]
}

/// Native device.yaml values, kept pure so the catalog publisher owns every write.
/// IDs use the selected system with a numeric suffix when that id is occupied.
pub fn new_source_records(
    root: &Path,
    system: &str,
    occupied: &[String],
) -> (String, serde_yaml::Value, serde_yaml::Value) {
    let mut id = system.to_owned();
    let mut suffix = 2;
    while occupied.contains(&id) {
        id = format!("{system}-{suffix}");
        suffix += 1;
    }
    let storage =
        serde_yaml::to_value(serde_json::json!({"root": root})).expect("path is serializable");
    let source = serde_yaml::to_value(
        serde_json::json!({"kind": ["files"], "storage": id, "system": system}),
    )
    .expect("source is serializable");
    (id, storage, source)
}

/// Account roots have the established launcher layout, not plugin-specific paths.
pub fn account_roots(
    readable_root: &Path,
) -> std::io::Result<Vec<crate::discovery::identify::AccountRoot>> {
    let users = readable_root.join("users");
    let entries = match std::fs::read_dir(&users) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut accounts = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let user = entry.file_name().into_string().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "account name is not UTF-8")
        })?;
        accounts.push(crate::discovery::identify::AccountRoot {
            storage_id: format!("account-{user}"),
            user,
            root: entry.path(),
        });
    }
    accounts.sort_by(|a, b| a.user.cmp(&b.user));
    Ok(accounts)
}

/// Decisions bind to the selected system/group and the resolved folder, not just a source id.
pub fn pairing_key(system: &str, root: &Path) -> String {
    format!("{system}\n{}", canonical_future_path(root).display())
}

pub fn pairings(
    sources: &[SourceView],
    accounts: &[crate::discovery::identify::AccountRoot],
) -> std::collections::BTreeMap<String, String> {
    sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                pairing_key(&source.system, &source.root),
            )
        })
        .chain(
            accounts
                .iter()
                .map(|account| (account.storage_id.clone(), pairing_key("", &account.root))),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(root: &Path) -> SourceView {
        SourceView {
            source_id: "one".into(),
            storage_id: "one".into(),
            title: None,
            system: "gb".into(),
            root: root.into(),
            canonical_root: root.canonicalize().ok(),
            scans_files: true,
        }
    }
    #[test]
    fn device_records_use_system_ids_and_accounts_use_launcher_roots() {
        let readable = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(readable.path().join("users/default")).unwrap();
        let accounts = account_roots(readable.path()).unwrap();
        assert_eq!(accounts[0].storage_id, "account-default");
        assert_eq!(accounts[0].root, readable.path().join("users/default"));
        let (id, storage, source) =
            new_source_records(Path::new("/games"), "gba", &["gba".into(), "gba-2".into()]);
        assert_eq!(id, "gba-3");
        assert_eq!(storage["root"].as_str(), Some("/games"));
        assert_eq!(source["system"].as_str(), Some("gba"));
        assert_eq!(source["storage"].as_str(), Some("gba-3"));
        assert_eq!(source["kind"][0].as_str(), Some("files"));
    }

    #[test]
    fn refuses_equal_nested_and_containing_roots_but_change_excludes_itself() {
        let temp = tempfile::tempdir().unwrap();
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let inner = temp.path().join("inner");
        std::fs::create_dir(&inner).unwrap();
        let request = |root, replacing| PairingRequest {
            root,
            system: "gb",
            readable_root: readable.path(),
            private_root: private.path(),
            replacing,
        };
        for (root, existing) in [
            (temp.path(), temp.path()),
            (&inner, temp.path()),
            (temp.path(), &inner),
        ] {
            assert!(matches!(
                validate_with(&request(root, None), &[view(existing)], |_| true),
                Err(PairingError::SourceOverlap(_))
            ));
        }
        assert_eq!(
            validate_with(
                &request(temp.path(), Some("one")),
                &[view(temp.path())],
                |_| true
            ),
            Ok(())
        );
        assert!(matches!(
            validate_with(&request(temp.path(), None), &[], |_| false),
            Err(PairingError::UnknownSystem(_))
        ));
    }
    #[test]
    fn refuses_plugin_data_in_both_directions_and_symlink_aliases() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let users = readable.path().join("users");
        let account = users.join("default");
        std::fs::create_dir_all(&account).unwrap();
        for root in [readable.path(), &users, &account, private.path()] {
            let request = PairingRequest {
                root,
                system: "gb",
                readable_root: readable.path(),
                private_root: private.path(),
                replacing: None,
            };
            assert_eq!(
                validate_with(&request, &[], |_| true),
                Err(PairingError::PluginData)
            );
        }
        let alias = readable.path().join("alias");
        std::os::unix::fs::symlink(&account, &alias).unwrap();
        assert_eq!(
            validate_with(
                &PairingRequest {
                    root: &alias,
                    system: "gb",
                    readable_root: readable.path(),
                    private_root: private.path(),
                    replacing: None
                },
                &[],
                |_| true
            ),
            Err(PairingError::PluginData)
        );
    }
}
