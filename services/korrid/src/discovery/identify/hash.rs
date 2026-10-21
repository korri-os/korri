use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::{ContentIdentity, IdentifyError};
use crate::plugin::{BytesRule, HeaderRule};

pub(super) const PREFIX_LIMIT: usize = 64 * 1024;

/// One cache for both digests. The header size is part of the cache validity:
/// different group members may interpret the same file under different rules.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HashCache {
    #[serde(default)]
    entries: BTreeMap<String, CacheEntry>,
    #[serde(skip)]
    pub(super) read_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct CacheEntry {
    size: u64,
    modified_nanos: Option<u128>,
    header_size: u64,
    hash: String,
    sha1: String,
}

pub(super) fn matches_bytes(prefix: &[u8], rule: &BytesRule) -> bool {
    let Ok(expected) = hex::decode(&rule.hex) else {
        return false;
    };
    let Ok(offset) = usize::try_from(rule.offset) else {
        return false;
    };
    let Some(end) = offset.checked_add(expected.len()) else {
        return false;
    };
    !expected.is_empty()
        && expected.len() <= 64
        && end <= PREFIX_LIMIT
        && prefix.get(offset..end) == Some(expected.as_slice())
}

pub(super) fn prefix(path: &Path) -> Result<Vec<u8>, IdentifyError> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(PREFIX_LIMIT as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Bytes the first applicable header rule strips (0 when none applies).
pub fn header_size(prefix: &[u8], size: u64, rules: &[HeaderRule]) -> u64 {
    rules
        .iter()
        .find(|rule| {
            rule.bytes
                .as_ref()
                .is_some_and(|bytes| matches_bytes(prefix, bytes))
                || rule.remainder.as_ref().is_some_and(|remainder| {
                    remainder.divisor != 0 && size % remainder.divisor == remainder.value
                })
        })
        .map_or(0, |rule| rule.size)
}

impl HashCache {
    pub(super) fn hash(
        &mut self,
        path: &Path,
        rules: &[HeaderRule],
    ) -> Result<ContentIdentity, IdentifyError> {
        let path = path.canonicalize()?;
        let mut file = File::open(&path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(IdentifyError::Unsupported("not a regular file".into()));
        }
        let size = metadata.len();
        let modified_nanos = metadata.modified().ok().and_then(|time| {
            time.duration_since(UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_nanos())
        });
        let key = path.to_string_lossy().into_owned();
        // Header selection needs only the bounded prefix. With no header rules,
        // an unchanged file requires no content read at all.
        let mut prefix = Vec::new();
        if !rules.is_empty() {
            Read::by_ref(&mut file)
                .take(PREFIX_LIMIT as u64)
                .read_to_end(&mut prefix)?;
        }
        let skip = header_size(&prefix, size, rules);
        if skip > size {
            return Err(IdentifyError::Unsupported(
                "header exceeds file size".into(),
            ));
        }
        if let Some(entry) = self.entries.get(&key) {
            if entry.size == size
                && entry.modified_nanos == modified_nanos
                && entry.header_size == skip
            {
                return Ok(ContentIdentity::Hash {
                    sha256: entry.hash.clone(),
                    sha1: entry.sha1.clone(),
                });
            }
        }
        let mut sha256 = Sha256::new();
        let mut sha1 = Sha1::new();
        if skip <= prefix.len() as u64 {
            let payload = &prefix[skip as usize..];
            sha256.update(payload);
            sha1.update(payload);
        } else {
            file.seek(SeekFrom::Start(skip))?;
        }
        let mut read_bytes = prefix.len() as u64;
        let mut buffer = [0u8; PREFIX_LIMIT];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            sha256.update(&buffer[..read]);
            sha1.update(&buffer[..read]);
            read_bytes += read as u64;
        }
        // A writer racing identification must not install a stale cache entry.
        let after = fs::metadata(&path)?;
        if size != after.len() || metadata.modified().ok() != after.modified().ok() {
            return Err(IdentifyError::Unsupported(
                "file changed during hashing".into(),
            ));
        }
        self.read_bytes += read_bytes;
        let hash = format!("sha256:{}", hex::encode(sha256.finalize()));
        let sha1 = format!("sha1:{}", hex::encode(sha1.finalize()));
        self.entries.insert(
            key,
            CacheEntry {
                size,
                modified_nanos,
                header_size: skip,
                hash: hash.clone(),
                sha1: sha1.clone(),
            },
        );
        Ok(ContentIdentity::Hash { sha256: hash, sha1 })
    }
}

pub(super) fn parts_identity(parts: Vec<String>) -> ContentIdentity {
    let value = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(parts.join("\n").as_bytes()))
    );
    ContentIdentity::Parts { value, parts }
}

pub(super) fn hashes(identity: &ContentIdentity) -> Vec<&str> {
    match identity {
        ContentIdentity::Hash { sha256, sha1 } => vec![sha256, sha1],
        ContentIdentity::Parts { value, .. } => vec![value],
        ContentIdentity::Chd { sha1 } => vec![sha1],
    }
}
