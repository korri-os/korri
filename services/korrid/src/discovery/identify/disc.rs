use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};

use super::{
    hash::{parts_identity, PREFIX_LIMIT},
    ContentIdentity, HashCache, IdentifyError,
};
use crate::plugin::ContentRules;

#[derive(Clone, Copy)]
pub(super) struct MappedFiles<'a> {
    pub paths: &'a BTreeMap<PathBuf, PathBuf>,
    pub physical_root: Option<&'a Path>,
}

pub(super) struct ReadScope<'a> {
    pub mapped: Option<MappedFiles<'a>>,
    pub root: &'a Path,
    pub excluded: &'a [PathBuf],
    /// Files admitted by traversal. Descriptors cannot bypass its budget.
    pub allowed: Option<&'a BTreeSet<PathBuf>>,
    pub max_depth: usize,
    pub max_references: usize,
}

pub(super) struct Identified {
    pub identity: ContentIdentity,
    pub parts: Vec<PathBuf>,
    pub tracks: BTreeSet<PathBuf>,
    pub consumed: BTreeSet<PathBuf>,
}

pub(super) fn checked_path(path: &Path, scope: &ReadScope<'_>) -> Result<PathBuf, IdentifyError> {
    if scope.allowed.is_some_and(|allowed| !allowed.contains(path)) {
        return Err(IdentifyError::Unsupported(
            "descriptor target was not admitted by traversal".into(),
        ));
    }
    let physical = if let Some(mapped) = scope.mapped {
        if !path.starts_with(scope.root) {
            return Err(IdentifyError::Unsupported(
                "path escapes the planned source".into(),
            ));
        }
        mapped
            .paths
            .get(path)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?
            .as_path()
    } else {
        path
    };
    let canonical = physical.canonicalize()?;
    let physical_root = scope
        .mapped
        .map_or(Some(scope.root), |mapped| mapped.physical_root);
    if physical_root.is_some_and(|root| !canonical.starts_with(root))
        || scope
            .excluded
            .iter()
            .any(|root| canonical.starts_with(root) || path.starts_with(root))
    {
        return Err(IdentifyError::Unsupported(
            "path escapes the source or enters plugin data".into(),
        ));
    }
    if !canonical.is_file() {
        return Err(IdentifyError::Unsupported("not a regular file".into()));
    }
    Ok(canonical)
}

pub(super) fn identify(
    path: &Path,
    rules: &ContentRules,
    cache: &mut HashCache,
    scope: &ReadScope<'_>,
) -> Result<Identified, IdentifyError> {
    let mut remaining = scope.max_references;
    identify_inner(
        path,
        rules,
        cache,
        scope,
        &mut BTreeSet::new(),
        &mut remaining,
    )
}

fn identify_inner(
    path: &Path,
    rules: &ContentRules,
    cache: &mut HashCache,
    scope: &ReadScope<'_>,
    active: &mut BTreeSet<PathBuf>,
    remaining: &mut usize,
) -> Result<Identified, IdentifyError> {
    if *remaining == 0 {
        return Err(IdentifyError::Unsupported(
            "descriptor reference budget exhausted".into(),
        ));
    }
    *remaining -= 1;
    let canonical = checked_path(path, scope)?;
    if active.len() >= scope.max_depth || !active.insert(canonical.clone()) {
        return Err(IdentifyError::Unsupported(
            "descriptor cycle or depth limit".into(),
        ));
    }
    let result = match kind(path).as_str() {
        "cue" | "m3u" => {
            let references = references(path, scope)?;
            let mut values = Vec::new();
            let mut parts = Vec::new();
            let mut tracks = BTreeSet::new();
            let mut consumed = BTreeSet::new();
            for reference in references {
                let identified =
                    identify_inner(&reference, rules, cache, scope, active, remaining)?;
                values.push(identified.identity.value().to_owned());
                // A playlist must retain its disc descriptors as launch dependencies,
                // even though descriptors contribute no identity bytes.
                if matches!(kind(&reference).as_str(), "cue" | "m3u") {
                    parts.push(reference.clone());
                }
                parts.extend(identified.parts);
                tracks.extend(identified.tracks);
                if kind(path) == "cue" {
                    tracks.insert(reference.clone());
                }
                consumed.insert(reference);
                consumed.extend(identified.consumed);
            }
            Ok(Identified {
                identity: parts_identity(values),
                parts,
                tracks,
                consumed,
            })
        }
        "chd" => {
            let mut bytes = [0; 0x54];
            File::open(&canonical)?.read_exact(&mut bytes)?;
            if &bytes[..8] != b"MComprHD"
                || u32::from_be_bytes(bytes[12..16].try_into().unwrap()) != 5
            {
                return Err(IdentifyError::Unsupported(
                    "CHD requires a version 5 MComprHD header".into(),
                ));
            }
            cache.read_bytes += bytes.len() as u64;
            Ok(Identified {
                identity: ContentIdentity::Chd {
                    sha1: format!("sha1:{}", hex::encode(&bytes[0x40..0x54])),
                },
                parts: vec![path.to_owned()],
                tracks: BTreeSet::new(),
                consumed: BTreeSet::new(),
            })
        }
        _ => Ok(Identified {
            identity: cache.hash(&canonical, &rules.header)?,
            parts: vec![path.to_owned()],
            tracks: BTreeSet::new(),
            consumed: BTreeSet::new(),
        }),
    };
    active.remove(&canonical);
    result
}

pub(super) fn kind(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// A complete unit, or all references known before a bounded preflight failed.
pub struct DependencyClosure {
    pub files: BTreeSet<PathBuf>,
    pub error: Option<IdentifyError>,
}

pub(super) fn dependencies(path: &Path, scope: &ReadScope<'_>) -> DependencyClosure {
    let mut files = BTreeSet::new();
    let mut remaining = scope.max_references;
    let result = collect_dependencies(
        path,
        scope,
        &mut BTreeSet::new(),
        &mut remaining,
        &mut files,
    );
    DependencyClosure {
        files,
        error: result.err(),
    }
}

fn collect_dependencies(
    path: &Path,
    scope: &ReadScope<'_>,
    active: &mut BTreeSet<PathBuf>,
    remaining: &mut usize,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), IdentifyError> {
    files.insert(path.to_owned());
    if *remaining == 0 {
        return Err(IdentifyError::Unsupported(
            "descriptor reference budget exhausted".into(),
        ));
    }
    *remaining -= 1;
    let canonical = checked_path(path, scope)?;
    if canonical != path {
        return Err(IdentifyError::Unsupported(
            "descriptor dependency uses a symlink".into(),
        ));
    }
    if active.len() >= scope.max_depth || !active.insert(canonical.clone()) {
        return Err(IdentifyError::Unsupported(
            "descriptor cycle or depth limit".into(),
        ));
    }
    let result = (|| {
        if !matches!(kind(path).as_str(), "cue" | "m3u") {
            File::open(&canonical)?;
            return Ok(());
        }
        let references = reference_paths(path, scope)?;
        files.extend(references.iter().cloned());
        let mut error = None;
        for reference in references {
            if let Err(next) = collect_dependencies(&reference, scope, active, remaining, files) {
                if error.is_none() {
                    error = Some(next);
                }
            }
        }
        error.map_or(Ok(()), Err)
    })();
    active.remove(&canonical);
    result
}

pub(super) fn references(
    path: &Path,
    scope: &ReadScope<'_>,
) -> Result<Vec<PathBuf>, IdentifyError> {
    let paths = reference_paths(path, scope)?;
    for path in &paths {
        checked_path(path, scope)?;
    }
    Ok(paths)
}

/// Descriptor reads are bounded too. They carry references, never identity bytes.
fn reference_paths(path: &Path, scope: &ReadScope<'_>) -> Result<Vec<PathBuf>, IdentifyError> {
    let canonical = checked_path(path, scope)?;
    let mut bytes = Vec::new();
    File::open(canonical)?
        .take(PREFIX_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > PREFIX_LIMIT {
        return Err(IdentifyError::Unsupported(
            "descriptor exceeds 64 KiB".into(),
        ));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| IdentifyError::Unsupported("descriptor is not UTF-8".into()))?;
    let mut paths = Vec::new();
    let cue = kind(path) == "cue";
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        let name = if cue {
            let boundary = line.find(char::is_whitespace).unwrap_or(line.len());
            if !line[..boundary].eq_ignore_ascii_case("FILE") {
                continue;
            }
            let rest = line[boundary..].trim_start();
            if let Some(quoted) = rest.strip_prefix('"') {
                let end = quoted.find('"').ok_or_else(|| {
                    IdentifyError::Unsupported("unterminated cue FILE name".into())
                })?;
                &quoted[..end]
            } else {
                rest.split_whitespace().next().unwrap_or_default()
            }
        } else {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            line
        };
        let relative = Path::new(name);
        if name.is_empty() || relative.is_absolute() {
            return Err(IdentifyError::Unsupported(
                "descriptor target must be relative".into(),
            ));
        }
        // Normalize `./` and `../` before traversal admission. Parent references
        // are valid only while the path remains inside this source.
        let mut target = path.parent().unwrap_or(scope.root).to_owned();
        for part in relative.components() {
            match part {
                Component::Normal(name) => target.push(name),
                Component::CurDir => {}
                Component::ParentDir if target != scope.root => {
                    target.pop();
                }
                _ => {
                    return Err(IdentifyError::Unsupported(
                        "descriptor target escapes the source".into(),
                    ))
                }
            }
        }
        paths.push(target);
    }
    if paths.is_empty() {
        return Err(IdentifyError::Unsupported(
            "descriptor has no content parts".into(),
        ));
    }
    Ok(paths)
}
