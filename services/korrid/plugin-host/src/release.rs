//! Exact-commit lookup for github-cache.py's published batch evidence.
//! revision.txt and paths-SYSTEM.txt locate candidates; only package inspection
//! grants publisher identity. No catalog, mutable pointer or release trust.
use crate::{
    declaration::validate_id,
    package,
    repository::{self, SourceUrl},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_PATHS_BYTES: usize = 64 * 1024;

fn validate_revision(revision: &str) -> Result<(), String> {
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("release must be a full lowercase 40-character publishing commit".into());
    }
    Ok(())
}

/// The Versions brief fixes build-<rev12>; github-cache.py fixes this URL shape.
/// Custom batch tags remain available through the exact-path raw-cache route.
pub fn batch_url(cache: &str, revision: &str) -> Result<SourceUrl, String> {
    validate_revision(revision)?;
    package::validate_cache_source(cache)?;
    let url = crate::https_url::parse(cache)?;
    let parts = url
        .path()
        .trim_end_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    let repo_component = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    };
    if url.host_str() != Some("github.com")
        || url.port().is_some()
        || url.query().is_some()
        || parts.len() != 6
        || !parts[0].is_empty()
        || !repo_component(parts[1])
        || !repo_component(parts[2])
        || parts[3] != "releases"
        || parts[4] != "download"
        || parts[5].is_empty()
        || parts[5].len() > 64
        || !parts[5].as_bytes()[0].is_ascii_alphanumeric()
        || !parts[5]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
        || parts[5] == format!("build-{}", &revision[..12])
        || cache != url.as_str()
        || url.path().ends_with("//")
    {
        return Err("cache must be an exact GitHub Releases metadata cache URL, distinct from the selected batch".into());
    }
    SourceUrl::parse(&format!(
        "https://github.com/{}/{}/releases/download/build-{}/",
        parts[1],
        parts[2],
        &revision[..12]
    ))
}

/// Parse the publisher's newline-terminated Nix stdout, not guessed path names.
pub fn parse_paths(text: &str) -> Result<Vec<PathBuf>, String> {
    if text.is_empty() || text.len() > MAX_PATHS_BYTES || !text.ends_with('\n') {
        return Err(
            "batch paths must be nonempty newline-terminated output paths within 64 KiB".into(),
        );
    }
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for line in text[..text.len() - 1].split('\n') {
        let path = PathBuf::from(line);
        package::validate_store_path(&path)?;
        // Path normalizes repeated separators and trailing slashes. Evidence
        // must instead match the producer's exact lexical store-path grammar.
        if line
            .strip_prefix("/nix/store/")
            .is_none_or(|name| name.contains('/'))
        {
            return Err("batch path must name one exact store output".into());
        }
        if !seen.insert(path.clone()) {
            return Err("batch contains a duplicate output path".into());
        }
        paths.push(path);
    }
    Ok(paths)
}

/// Transport takes an explicit batch location so local HTTPS acceptance uses
/// the same downloader and parser as production, without network interception.
pub fn fetch_paths(
    curl: &Path,
    batch: &SourceUrl,
    revision: &str,
    system: &str,
    ca: Option<&Path>,
) -> Result<Vec<PathBuf>, String> {
    validate_revision(revision)?;
    if !matches!(system, "x86_64-linux" | "aarch64-linux") {
        return Err("release has no supported host architecture".into());
    }
    let base = crate::https_url::parse(batch.as_str())?;
    if !base.path().ends_with('/') || base.query().is_some() {
        return Err("batch must be an HTTPS directory URL without a query".into());
    }
    let fetch = |name: &str, limit| -> Result<String, String> {
        let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        let url = SourceUrl::parse(base.join(name).map_err(|e| e.to_string())?.as_str())?;
        repository::download(
            curl,
            &url,
            ca,
            output.path(),
            limit,
            Duration::from_secs(30),
        )?;
        std::fs::read_to_string(output.path()).map_err(|e| format!("invalid batch {name}: {e}"))
    };
    if fetch("revision.txt", 41)? != format!("{revision}\n") {
        return Err("batch revision.txt differs from the requested full commit".into());
    }
    parse_paths(&fetch(
        &format!("paths-{system}.txt"),
        MAX_PATHS_BYTES as u64,
    )?)
}

/// Inspect every candidate: a first match cannot hide ambiguity or a broken,
/// untrusted later output. The caller owns import, publisher checks and GC roots.
pub fn select_output(
    paths: &[PathBuf],
    id: &str,
    mut inspect: impl FnMut(&Path) -> Result<String, String>,
) -> Result<PathBuf, String> {
    validate_id(id)?;
    let mut selected = None;
    for path in paths {
        let candidate_id =
            inspect(path).map_err(|e| format!("release candidate {}: {e}", path.display()))?;
        validate_id(&candidate_id)?;
        if candidate_id == id && selected.replace(path.clone()).is_some() {
            return Err(format!(
                "release contains multiple outputs for {id}; inspect an exact path instead"
            ));
        }
    }
    selected.ok_or_else(|| format!("release has no verified output for {id}"))
}
