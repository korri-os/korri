//! Input-addressed archives split authority per path, never per store invocation.
//! Nix parses cache metadata and imports NARs. Korri selects the only key and
//! source allowed for each path before it lets Nix change the shared store.
use crate::{
    archive,
    package::{self, NarInfo},
    process,
    repository::{self, Configuration, SourceUrl},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) const UPSTREAM: &str = "https://cache.nixos.org";

pub(crate) struct Upstream {
    nix: PathBuf,
    curl: PathBuf,
    ca: Option<PathBuf>,
    notes: tempfile::TempDir,
    seen: BTreeMap<String, Option<NarInfo>>,
    #[cfg(test)]
    file_cache: Option<PathBuf>,
}

impl Upstream {
    pub(crate) fn new(nix: &Path, config: &Configuration) -> Result<Self, String> {
        Ok(Self {
            nix: nix.into(),
            curl: config.curl.clone(),
            ca: config.ca_bundle.clone(),
            notes: tempfile::tempdir().map_err(|e| e.to_string())?,
            seen: BTreeMap::new(),
            #[cfg(test)]
            file_cache: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn file(nix: &Path, cache: &Path) -> Self {
        let mut this = Self::new(
            nix,
            &Configuration {
                curl: "/unavailable-curl".into(),
                ca_bundle: None,
                official: None,
            },
        )
        .unwrap();
        this.file_cache = Some(cache.into());
        this
    }

    pub(crate) fn uri(&self) -> Result<String, String> {
        #[cfg(test)]
        if let Some(cache) = &self.file_cache {
            return archive::path_to_file_uri(cache);
        }
        Ok(UPSTREAM.into())
    }

    /// Nix requires reference metadata to exist at an export destination even
    /// with --no-recursive. Temporarily seed only the already verified upstream
    /// narinfos; the caller removes them after copying the retained outputs.
    pub(crate) fn stage_metadata(&self, destination: &Path) -> Result<Vec<PathBuf>, String> {
        let mut staged = Vec::new();
        for (path, info) in &self.seen {
            if info.is_none() {
                continue;
            }
            let name = format!(
                "{}.narinfo",
                &Path::new(path)
                    .file_name()
                    .ok_or("invalid path")?
                    .to_str()
                    .ok_or("invalid path")?[..32]
            );
            let target = destination.join(&name);
            fs::copy(self.notes.path().join(&name), &target).map_err(|e| e.to_string())?;
            staged.push(target);
        }
        Ok(staged)
    }

    /// A 404 is absence. No TLS, transport, parsing or signing failure can
    /// silently turn a NixOS path into one the publisher is allowed to supply.
    pub(crate) fn info(&mut self, path: &str) -> Result<Option<NarInfo>, String> {
        package::validate_store_path(Path::new(path))?;
        if let Some(info) = self.seen.get(path) {
            return Ok(info.clone());
        }
        let name = format!(
            "{}.narinfo",
            &Path::new(path)
                .file_name()
                .ok_or("invalid path")?
                .to_str()
                .ok_or("invalid path")?[..32]
        );
        let destination = self.notes.path().join(&name);
        #[cfg(test)]
        if let Some(cache) = &self.file_cache {
            if cache.join(&name).is_file() {
                fs::copy(cache.join(&name), &destination).map_err(|e| e.to_string())?;
            } else {
                self.seen.insert(path.into(), None);
                return Ok(None);
            }
            fs::copy(
                cache.join("nix-cache-info"),
                self.notes.path().join("nix-cache-info"),
            )
            .map_err(|e| e.to_string())?;
        }
        #[cfg(test)]
        let https = self.file_cache.is_none();
        #[cfg(not(test))]
        let https = true;
        if https {
            if !self.notes.path().join("nix-cache-info").exists() {
                repository::download(
                    &self.curl,
                    &SourceUrl::parse(&format!("{UPSTREAM}/nix-cache-info"))?,
                    self.ca.as_deref(),
                    &self.notes.path().join("nix-cache-info"),
                    repository::TransferBounds::Limited {
                        max_bytes: 1024 * 1024,
                        timeout: Duration::from_secs(30),
                    },
                )?;
                process::checked(
                    &self.nix,
                    [
                        "--extra-experimental-features",
                        "nix-command",
                        "store",
                        "info",
                        "--store",
                        &archive::path_to_file_uri(self.notes.path())?,
                    ],
                    Duration::from_secs(30),
                )?;
            }
            if !repository::download_optional(
                &self.curl,
                &SourceUrl::parse(&format!("{UPSTREAM}/{name}"))?,
                self.ca.as_deref(),
                &destination,
            )? {
                fs::remove_file(&destination).map_err(|e| e.to_string())?;
                self.seen.insert(path.into(), None);
                return Ok(None);
            }
        }
        let info = package::path_info(
            &self.nix,
            path,
            Some(&archive::path_to_file_uri(self.notes.path())?),
        )?;
        self.seen.insert(path.into(), Some(info.clone()));
        Ok(Some(info))
    }
}

pub(crate) fn normal_keys(nix: &Path, publisher: &str) -> Result<Vec<String>, String> {
    let keys = process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "config",
            "show",
            "trusted-public-keys",
        ],
        Duration::from_secs(10),
    )?;
    let publisher = package::public_key_bytes(publisher)?;
    keys.split_whitespace()
        .filter_map(|key| match package::public_key_bytes(key) {
            Ok(bytes) if bytes != publisher => Some(Ok(key.to_owned())),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

pub(crate) fn signed(path: &str, info: &NarInfo, keys: &[String]) -> Result<bool, String> {
    let fingerprint = package::fingerprint(path, info)?;
    for key in keys {
        if package::signed_by(info, &fingerprint, &package::public_key_bytes(key)?) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn check_local(path: &str, info: &NarInfo, keys: &[String], shared: bool) -> Result<(), String> {
    if signed(path, info, keys)? {
        return Ok(());
    }
    if shared {
        Err(format!(
            "{path} is already installed without a trusted NixOS cache signature"
        ))
    } else {
        Err(format!("{path} is already installed from another publisher; remove that plugin or rebuild with a different output"))
    }
}

fn invocation(nix: &Path, source: &str, keys: &[String], paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let keys = keys.join(" ");
    let mut args = vec![
        "--extra-experimental-features",
        "nix-command",
        "--option",
        "max-jobs",
        "0",
        "--option",
        "builders",
        "",
        "--option",
        "fallback",
        "false",
        "--option",
        "require-sigs",
        "true",
        "--option",
        "trusted-public-keys",
        &keys,
        "--option",
        "extra-trusted-public-keys",
        "",
        "--option",
        "substituters",
        source,
        "--option",
        "extra-substituters",
        "",
        "build",
        "--no-link",
    ];
    args.extend(paths.iter().map(String::as_str));
    process::checked_uncapped(nix, args, None)?;
    Ok(())
}

fn verify(nix: &Path, keys: &[String], paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let keys = keys.join(" ");
    let mut args = vec![
        "--extra-experimental-features",
        "nix-command",
        "--option",
        "trusted-public-keys",
        &keys,
        "--option",
        "extra-trusted-public-keys",
        "",
        "--option",
        "substituters",
        "",
        "store",
        "verify",
        "--sigs-needed",
        "1",
    ];
    args.extend(paths.iter().map(String::as_str));
    process::checked_uncapped(nix, args, None)?;
    Ok(())
}

fn unavailable(missing: usize, error: &str) -> String {
    if missing == 0 {
        format!("cannot check shared package authority at {UPSTREAM}: {error}")
    } else {
        format!("{missing} shared packages need {UPSTREAM}; connect to the network or use a self-contained archive: {error}")
    }
}

#[derive(Clone, Copy)]
enum PrivateSource<'a> {
    Archive { uri: &'a str, paths: &'a [String] },
    Cache(&'a str),
    Local,
}

impl PrivateSource<'_> {
    fn info(
        self,
        nix: &Path,
        path: &str,
        local: &Option<NarInfo>,
    ) -> Result<Option<NarInfo>, String> {
        match self {
            Self::Archive { uri, paths } => {
                if paths.iter().any(|p| p == path) {
                    package::path_info(nix, path, Some(uri)).map(Some)
                } else {
                    Ok(None)
                }
            }
            Self::Cache(uri) => package::optional_path_info(nix, path, uri),
            Self::Local => Ok(local.clone()),
        }
    }
}

/// Resolve the closure from the archive and upstream metadata, check existing
/// store authority before importing, then fetch shared paths before private ones.
pub(crate) fn import(
    nix: &Path,
    cache: &Path,
    root: &Path,
    key: &str,
    upstream: &mut Upstream,
    excluded: &BTreeSet<PathBuf>,
) -> Result<(), String> {
    let paths = archive::cache_paths(cache)?;
    archive::preflight_slim(nix, &paths, cache)?;
    let uri = archive::path_to_file_uri(cache)?;
    acquire(
        nix,
        root,
        key,
        upstream,
        PrivateSource::Archive {
            uri: &uri,
            paths: &paths,
        },
        excluded,
        &BTreeSet::new(),
    )
}

/// Required-plugin acquisition has the same isolation as its parent archive.
/// A configured raw cache supplies only that requirement's private paths;
/// existing repository receipts can be checked entirely from local metadata.
pub(crate) fn import_required(
    nix: &Path,
    cache: Option<&str>,
    root: &Path,
    key: &str,
    upstream: &mut Upstream,
    excluded: &BTreeSet<PathBuf>,
    forbidden: &BTreeSet<PathBuf>,
) -> Result<(), String> {
    if let Some(cache) = cache {
        package::validate_cache_source(cache)?;
    }
    acquire(
        nix,
        root,
        key,
        upstream,
        cache.map_or(PrivateSource::Local, PrivateSource::Cache),
        excluded,
        forbidden,
    )
}

fn acquire(
    nix: &Path,
    root: &Path,
    key: &str,
    upstream: &mut Upstream,
    source: PrivateSource<'_>,
    excluded: &BTreeSet<PathBuf>,
    forbidden: &BTreeSet<PathBuf>,
) -> Result<(), String> {
    let publisher = vec![key.to_owned()];
    let trusted = normal_keys(nix, key)?;
    let mut queue = vec![root.to_string_lossy().into_owned()];
    let mut seen = BTreeSet::new();
    let mut shared = Vec::new();
    let mut private = Vec::new();
    while let Some(path) = queue.pop() {
        if forbidden.contains(Path::new(&path)) {
            return Err("required plugin closure contains its parent".into());
        }
        if !seen.insert(path.clone()) || excluded.contains(Path::new(&path)) {
            continue;
        }
        if seen.len() > archive::MAX_ENTRY_COUNT {
            return Err("package closure exceeds archive entry bound".into());
        }
        let local = if Path::new(&path).exists() {
            Some(package::path_info(nix, &path, None)?)
        } else {
            None
        };
        let supplied = source.info(nix, &path, &local)?;
        let remote = upstream.info(&path).map_err(|error| {
            let missing = match source {
                PrivateSource::Archive { uri, paths } => paths
                    .iter()
                    .filter_map(|p| package::path_info(nix, p, Some(uri)).ok())
                    .flat_map(|i| i.references)
                    .filter(|p| !paths.contains(p) && !Path::new(p).exists())
                    .collect::<BTreeSet<_>>()
                    .len(),
                _ => supplied.as_ref().map_or(0, |info| {
                    info.references
                        .iter()
                        .filter(|p| !Path::new(p).exists())
                        .count()
                }),
            };
            unavailable(missing, &error)
        })?;
        let (info, keys, is_shared) = if let Some(info) = remote {
            if !signed(&path, &info, &trusted)? {
                return Err(format!(
                    "{path} is not signed by a normal trusted NixOS cache key"
                ));
            }
            if let Some(copy) = &supplied {
                match source {
                    PrivateSource::Archive { .. } => {
                        return Err(format!(
                            "{path} is served by {UPSTREAM}; the archive must not supply it"
                        ))
                    }
                    PrivateSource::Cache(_)
                        if !signed(&path, copy, &trusted)?
                            || package::fingerprint(&path, copy)?
                                != package::fingerprint(&path, &info)? =>
                    {
                        return Err(format!("{path} is served by {UPSTREAM}; a publisher-signed cache copy cannot supply an upstream path"));
                    }
                    _ => {}
                }
            }
            shared.push(path.clone());
            (info, &trusted, true)
        } else {
            let info = supplied.or_else(|| local.clone()).ok_or_else(|| {
                format!("archive plus {UPSTREAM} do not complete the closure: missing {path}")
            })?;
            if !signed(&path, &info, &publisher)? {
                return Err(format!("{path} is not signed by the publisher key"));
            }
            private.push(path.clone());
            (info, &publisher, false)
        };
        if let Some(local) = local {
            check_local(&path, &local, keys, is_shared)?;
            if package::fingerprint(&path, &local)? != package::fingerprint(&path, &info)? {
                return Err(format!(
                    "{path} local NAR differs from its authorized cache"
                ));
            }
        }
        queue.extend(info.references);
    }
    invocation(nix, &upstream.uri()?, &trusted, &shared).map_err(|error| {
        unavailable(
            shared.iter().filter(|p| !Path::new(p).exists()).count(),
            &error,
        )
    })?;
    match source {
        PrivateSource::Archive { uri, .. } | PrivateSource::Cache(uri) => {
            invocation(nix, uri, &publisher, &private)?
        }
        PrivateSource::Local => {}
    }
    // Nix's CA exemption must not stand in for an explicit signature in a slim
    // closure, even for a fixed-output dependency.
    for (paths, keys, shared) in [(&shared, &trusted, true), (&private, &publisher, false)] {
        for path in paths {
            check_local(path, &package::path_info(nix, path, None)?, keys, shared)?;
        }
        verify(nix, keys, paths)?;
    }
    package::sync_store()
}
