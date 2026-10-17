//! The attention list: what discovery could not identify or decide on one
//! device (CONTEXT.md "Attention list"). Decisions persist in
//! `<private>/game-discovery/attention.json` (contract section 2). Owned by
//! the `sources` builder.
//! Pairing keys also invalidate decisions after direct device.yaml edits.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

/// Contract section 4, "Attention".
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttentionKind {
    Clash,
    Unmatched,
    Unreadable,
    Overlap,
    SystemUnavailable,
    DeclaredMismatch,
}

/// One finding from a scan, before decisions are applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttentionFinding {
    pub kind: AttentionKind,
    pub source_id: String,
    /// Relative to the source root (or the account root for
    /// `declared-mismatch`). `None` for source-level kinds.
    pub path: Option<String>,
    /// The identity value at `path`, when it could be computed.
    pub identity: Option<String>,
    /// Candidate systems for `clash`.
    pub candidates: Vec<String>,
    pub message: String,
}

impl AttentionFinding {
    /// First 16 hex characters of SHA-256 over `kind\nsource\npath`
    /// (contract section 6).
    pub fn item_id(&self) -> String {
        hex::encode(Sha256::digest(format!(
            "{}\n{}\n{}",
            self.kind.as_str(),
            self.source_id,
            self.path.as_deref().unwrap_or("")
        )))
        .chars()
        .take(16)
        .collect()
    }
}

/// A person's choice for a `clash` file, valid while its identity holds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttentionAssignment {
    pub source: String,
    pub pairing: String,
    pub path: String,
    pub identity: String,
    pub system: String,
}

/// An ignored item: a file (`path` and `identity`) or a source-level kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttentionIgnore {
    pub source: String,
    pub pairing: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AttentionKind>,
}

/// `attention.json`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttentionDecisions {
    #[serde(default)]
    pub assigned: Vec<AttentionAssignment>,
    #[serde(default)]
    pub ignored: Vec<AttentionIgnore>,
}

impl AttentionDecisions {
    /// The assigned system for this file, when the identity still matches.
    pub fn assigned_system(
        &self,
        source: &str,
        path: &str,
        identity: &str,
        pairing: &str,
    ) -> Option<&str> {
        self.assigned
            .iter()
            .find(|entry| {
                entry.source == source
                    && entry.path == path
                    && entry.identity == identity
                    && entry.pairing == pairing
            })
            .map(|entry| entry.system.as_str())
    }

    /// Whether this finding is ignored under the current bytes or pairing.
    pub fn is_ignored(&self, finding: &AttentionFinding, pairing: &str) -> bool {
        self.ignored.iter().any(|entry| {
            entry.source == finding.source_id
                && entry.pairing == pairing
                && match (&finding.path, &entry.path) {
                    (Some(path), Some(ignored_path)) => {
                        path == ignored_path && entry.identity == finding.identity
                    }
                    (None, None) => entry.kind == Some(finding.kind),
                    _ => false,
                }
        })
    }
}

impl AttentionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clash => "clash",
            Self::Unmatched => "unmatched",
            Self::Unreadable => "unreadable",
            Self::Overlap => "overlap",
            Self::SystemUnavailable => "system-unavailable",
            Self::DeclaredMismatch => "declared-mismatch",
        }
    }
}

pub fn load(private_root: &Path) -> io::Result<AttentionDecisions> {
    match fs::read(private_root.join("game-discovery/attention.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AttentionDecisions::default()),
        Err(error) => Err(error),
    }
}

pub fn save(private_root: &Path, decisions: &AttentionDecisions) -> io::Result<()> {
    atomic_save_json(private_root, "attention.json", decisions)
}

impl AttentionDecisions {
    pub fn retain_pairings(&mut self, pairings: &std::collections::BTreeMap<String, String>) {
        self.assigned
            .retain(|entry| pairings.get(&entry.source) == Some(&entry.pairing));
        self.ignored
            .retain(|entry| pairings.get(&entry.source) == Some(&entry.pairing));
    }

    pub fn clear_source(&mut self, source: &str) {
        self.assigned.retain(|entry| entry.source != source);
        self.ignored.retain(|entry| entry.source != source);
    }

    pub fn assign(
        &mut self,
        finding: &AttentionFinding,
        system: &str,
        pairing: &str,
    ) -> Result<(), &'static str> {
        if finding.kind != AttentionKind::Clash
            || !finding
                .candidates
                .iter()
                .any(|candidate| candidate == system)
        {
            return Err("NotACandidate");
        }
        let (Some(path), Some(identity)) = (&finding.path, &finding.identity) else {
            return Err("NotACandidate");
        };
        self.assigned
            .retain(|entry| entry.source != finding.source_id || entry.path != *path);
        self.assigned.push(AttentionAssignment {
            source: finding.source_id.clone(),
            pairing: pairing.into(),
            path: path.clone(),
            identity: identity.clone(),
            system: system.into(),
        });
        Ok(())
    }

    pub fn ignore(&mut self, finding: &AttentionFinding, pairing: &str) {
        self.ignored.retain(|entry| {
            !(entry.source == finding.source_id
                && entry.path == finding.path
                && (finding.path.is_some() || entry.kind == Some(finding.kind)))
        });
        self.ignored.push(AttentionIgnore {
            source: finding.source_id.clone(),
            pairing: pairing.into(),
            path: finding.path.clone(),
            identity: finding.identity.clone(),
            kind: finding.path.is_none().then_some(finding.kind),
        });
    }
}

pub fn atomic_save_json<T: Serialize>(
    private_root: &Path,
    name: &str,
    value: &T,
) -> io::Result<()> {
    let directory = private_root.join("game-discovery");
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)?;
    let path = directory.join(name);
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if fs::read(&path).ok().as_deref() == Some(bytes.as_slice()) {
        return Ok(());
    }
    let temporary = directory.join(format!(".{name}.{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)?;
        fs::File::open(&directory)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    const PAIRING: &str = "gameboy\n/games";
    fn finding() -> AttentionFinding {
        AttentionFinding {
            kind: AttentionKind::Clash,
            source_id: "gb".into(),
            path: Some("x.gb".into()),
            identity: Some("sha256:one".into()),
            candidates: vec!["gb".into(), "gbc".into()],
            message: "ambiguous".into(),
        }
    }
    #[test]
    fn decisions_persist_atomically_and_expire_when_bytes_change() {
        let root = tempfile::tempdir().unwrap();
        let mut decisions = load(root.path()).unwrap();
        let mut file = finding();
        assert_eq!(
            decisions.assign(&file, "nes", PAIRING),
            Err("NotACandidate")
        );
        decisions.assign(&file, "gbc", PAIRING).unwrap();
        decisions.ignore(&file, PAIRING);
        save(root.path(), &decisions).unwrap();
        let loaded = load(root.path()).unwrap();
        assert_eq!(loaded, decisions);
        assert!(loaded.is_ignored(&file, PAIRING));
        assert_eq!(
            loaded.assigned_system("gb", "x.gb", "sha256:one", PAIRING),
            Some("gbc")
        );
        let id = file.item_id();
        file.identity = Some("sha256:two".into());
        assert_eq!(file.item_id(), id);
        assert!(!loaded.is_ignored(&file, PAIRING));
        assert_eq!(
            loaded.assigned_system("gb", "x.gb", "sha256:two", PAIRING),
            None
        );
        let modified = fs::metadata(root.path().join("game-discovery/attention.json"))
            .unwrap()
            .modified()
            .unwrap();
        save(root.path(), &loaded).unwrap();
        assert_eq!(
            modified,
            fs::metadata(root.path().join("game-discovery/attention.json"))
                .unwrap()
                .modified()
                .unwrap()
        );
    }
    #[test]
    fn unreadable_file_can_be_ignored_until_an_identity_becomes_available() {
        let mut file = finding();
        file.kind = AttentionKind::Unreadable;
        file.identity = None;
        let mut decisions = AttentionDecisions::default();
        decisions.ignore(&file, PAIRING);
        assert!(decisions.is_ignored(&file, PAIRING));
        file.identity = Some("sha256:readable".into());
        assert!(!decisions.is_ignored(&file, PAIRING));
    }

    #[test]
    fn source_ignore_clears_on_pairing_change_and_ids_match_contract() {
        let mut file = finding();
        file.kind = AttentionKind::SystemUnavailable;
        file.path = None;
        file.identity = None;
        let mut decisions = AttentionDecisions::default();
        decisions.ignore(&file, PAIRING);
        assert!(decisions.is_ignored(&file, PAIRING));
        decisions.clear_source("gb");
        assert!(!decisions.is_ignored(&file, PAIRING));
        assert_eq!(
            file.item_id(),
            &hex::encode(Sha256::digest(b"system-unavailable\ngb\n"))[..16]
        );
    }
}
