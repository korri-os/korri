//! ADR 0054: install a published plugin release on a device without Nix.
//!
//! The Android app has no `nix` program and no `/nix/store`. It still installs
//! the same signed releases that Linux devices install (ADR 0043, ADR 0048):
//! the plugin catalog's file-cache archive. This module only *reads* Nix
//! formats. It checks, in order:
//!
//! 1. the archive bytes against the catalog's `archive_sha256`;
//! 2. the root narinfo's ed25519 signature over Nix's path fingerprint,
//!    against the publisher key that the device owner trusts (ADR 0044);
//! 3. the NAR bytes against the signed `NarHash` and `NarSize`;
//!
//! and only then unpacks the NAR into a fresh directory in app storage.
//!
//! An Android release is declaration-only: it has no store references, and
//! its manifest has no `packages`, `files` or `services`. Dependencies are
//! named by release (`requiresReleases`), never by store path.
//!
//! The fingerprint and signature rules mirror `plugin-host/src/package.rs`
//! (`fingerprint`, `signed_by`), which is Nix's `ValidPathInfo::fingerprint`.
//! A test in this module checks a real Nix-signed archive when one is given.

use crate::plugin_installation::EnabledPackage;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

/// A declaration-only release is a few source files. Bound everything read.
pub const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_NARINFO_BYTES: usize = 64 * 1024;
const MAX_NAR_BYTES: u64 = 16 * 1024 * 1024;
const MAX_NAR_ENTRIES: usize = 4096;
const NIX32: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InstallError {
    #[error("plugin archive is invalid: {0}")]
    Archive(String),
    #[error("plugin archive hash does not match the catalog")]
    ArchiveHash,
    #[error("plugin release is not signed by the trusted publisher key")]
    Signature,
    #[error("plugin NAR does not match its signed hash")]
    NarHash,
    #[error("plugin NAR is invalid: {0}")]
    Nar(String),
    #[error("Android plugin releases carry no native files: {0}")]
    NotDeclarationOnly(String),
    #[error("plugin storage is unavailable: {0}")]
    Storage(String),
}

/// The fields of a catalog record this installer needs (plugin-host
/// `catalog.rs` `CatalogRecord`). Kept separate so korrid need not link the
/// root plugin host.
#[derive(Clone, Debug)]
pub struct ReleaseRef<'a> {
    pub plugin_id: &'a str,
    pub store_path: &'a str,
    pub archive_sha256: &'a str,
}

/// One parsed narinfo. Field names follow Nix's text format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NarInfo {
    pub store_path: String,
    pub url: String,
    pub compression: String,
    pub nar_hash: String,
    pub nar_size: u64,
    pub references: Vec<String>,
    pub signatures: Vec<String>,
    pub ca: Option<String>,
}

pub fn parse_narinfo(text: &str) -> Result<NarInfo, InstallError> {
    let bad = |m: &str| InstallError::Archive(format!("narinfo {m}"));
    if text.len() > MAX_NARINFO_BYTES {
        return Err(bad("exceeds its size bound"));
    }
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    let mut signatures = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (key, value) = line
            .split_once(": ")
            .ok_or_else(|| bad("line has no field"))?;
        if key == "Sig" {
            signatures.push(value.to_owned());
        } else if fields.insert(key, value).is_some() {
            return Err(bad("repeats a field"));
        }
    }
    let get = |key: &str| {
        fields
            .get(key)
            .copied()
            .ok_or_else(|| bad(&format!("lacks {key}")))
    };
    let store_path = get("StorePath")?.to_owned();
    validate_store_path(&store_path)?;
    let references = fields
        .get("References")
        .map(|refs| refs.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
    Ok(NarInfo {
        store_path,
        url: get("URL")?.to_owned(),
        compression: fields
            .get("Compression")
            .copied()
            .unwrap_or("bzip2")
            .to_owned(),
        nar_hash: get("NarHash")?.to_owned(),
        nar_size: get("NarSize")?
            .parse()
            .map_err(|_| bad("NarSize is not a number"))?,
        references,
        signatures,
        ca: fields.get("CA").map(|ca| (*ca).to_owned()),
    })
}

fn validate_store_path(path: &str) -> Result<(), InstallError> {
    let name = path
        .strip_prefix("/nix/store/")
        .ok_or_else(|| InstallError::Archive("store path is outside /nix/store".into()))?;
    let (hash, rest) = name.split_at_checked(32).unwrap_or(("", ""));
    if hash.len() != 32
        || !hash.bytes().all(|b| NIX32.contains(&b))
        || !rest.starts_with('-')
        || rest.len() < 2
        || rest.contains('/')
    {
        return Err(InstallError::Archive("invalid store path".into()));
    }
    Ok(())
}

/// Nix's `ValidPathInfo::fingerprint()`: `1;<path>;<narHash>;<narSize>;<refs>`,
/// with references as full store paths, sorted and comma-separated.
pub fn fingerprint(info: &NarInfo) -> Result<String, InstallError> {
    if !info.nar_hash.starts_with("sha256:") || info.nar_size == 0 {
        return Err(InstallError::Archive("unsupported NAR hash".into()));
    }
    let mut references: Vec<String> = info
        .references
        .iter()
        .map(|r| format!("/nix/store/{r}"))
        .collect();
    for reference in &references {
        validate_store_path(reference)?;
    }
    references.sort();
    references.dedup();
    Ok(format!(
        "1;{};{};{};{}",
        info.store_path,
        info.nar_hash,
        info.nar_size,
        references.join(",")
    ))
}

/// A full Nix public key, `name:base64(32 bytes)`, as publisher bindings hold.
pub fn public_key_bytes(key: &str) -> Result<Vec<u8>, InstallError> {
    use base64::Engine;
    let (label, encoded) = key.split_once(':').ok_or(InstallError::Signature)?;
    if label.is_empty() {
        return Err(InstallError::Signature);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| InstallError::Signature)?;
    if bytes.len() != 32 {
        return Err(InstallError::Signature);
    }
    Ok(bytes)
}

/// True when any `Sig` line verifies the fingerprint under `key`. The key
/// name in the `Sig` line is not trusted; only the key bytes decide.
pub fn verify_signature(info: &NarInfo, public_key: &str) -> Result<(), InstallError> {
    use base64::Engine;
    let key = public_key_bytes(public_key)?;
    let message = fingerprint(info)?;
    let ok = info.signatures.iter().any(|sig| {
        let Some((_, encoded)) = sig.split_once(':') else {
            return false;
        };
        let Ok(signature) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
            return false;
        };
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &key)
            .verify(message.as_bytes(), &signature)
            .is_ok()
    });
    if ok {
        Ok(())
    } else {
        Err(InstallError::Signature)
    }
}

pub fn nix32(bytes: &[u8]) -> String {
    let length = (bytes.len() * 8 - 1) / 5 + 1;
    let mut out = String::with_capacity(length);
    for n in (0..length).rev() {
        let bit = n * 5;
        let (i, j) = (bit / 8, bit % 8);
        let low = u16::from(bytes[i]) >> j;
        let high = bytes.get(i + 1).map_or(0, |b| u16::from(*b) << (8 - j));
        out.push(char::from(NIX32[usize::from((low | high) & 0x1f)]));
    }
    out
}

/// The file-cache archive, read into memory: its narinfos and NAR files.
struct Cache {
    narinfos: Vec<NarInfo>,
    files: BTreeMap<String, Vec<u8>>,
}

fn read_cache(bytes: &[u8]) -> Result<Cache, InstallError> {
    let bad = |m: String| InstallError::Archive(m);
    let mut archive = tar::Archive::new(bytes);
    let mut narinfos = Vec::new();
    let mut files = BTreeMap::new();
    let mut saw_cache_info = false;
    for entry in archive.entries().map_err(|e| bad(e.to_string()))?.raw(true) {
        let mut entry = entry.map_err(|e| bad(e.to_string()))?;
        if !entry.header().entry_type().is_file() {
            return Err(bad("archive entries must be regular files".into()));
        }
        let path = entry.path().map_err(|e| bad(e.to_string()))?.into_owned();
        let name = path
            .to_str()
            .ok_or_else(|| bad("non-UTF-8 path".into()))?
            .to_owned();
        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|e| bad(e.to_string()))?;
        if name == "nix-cache-info" {
            saw_cache_info = true;
        } else if name.ends_with(".narinfo") && !name.contains('/') {
            let text =
                String::from_utf8(content).map_err(|_| bad("narinfo is not UTF-8".into()))?;
            narinfos.push(parse_narinfo(&text)?);
        } else if let Some(nar) = name.strip_prefix("nar/") {
            if nar.contains('/') || !(nar.ends_with(".nar") || nar.ends_with(".nar.xz")) {
                return Err(bad(format!("unexpected NAR file {name}")));
            }
            if files.insert(name, content).is_some() {
                return Err(bad("duplicate archive path".into()));
            }
        } else {
            return Err(bad(format!("unexpected archive path {name}")));
        }
    }
    if !saw_cache_info {
        return Err(bad("archive lacks nix-cache-info".into()));
    }
    Ok(Cache { narinfos, files })
}

fn decompress(info: &NarInfo, bytes: &[u8]) -> Result<Vec<u8>, InstallError> {
    let mut out = Vec::new();
    match info.compression.as_str() {
        "none" => out.extend_from_slice(bytes),
        "xz" => lzma_rs::xz_decompress(
            &mut io::BufReader::new(bytes),
            &mut LimitedWriter::new(&mut out),
        )
        .map_err(|e| InstallError::Nar(format!("xz: {e}")))?,
        other => {
            return Err(InstallError::Nar(format!(
                "unsupported compression {other}"
            )))
        }
    }
    Ok(out)
}

struct LimitedWriter<'a> {
    out: &'a mut Vec<u8>,
}
impl<'a> LimitedWriter<'a> {
    fn new(out: &'a mut Vec<u8>) -> Self {
        Self { out }
    }
}
impl Write for LimitedWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.out.len() as u64 + buf.len() as u64 > MAX_NAR_BYTES {
            return Err(io::Error::other("NAR exceeds its size bound"));
        }
        self.out.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Verify a release archive and unpack its one package into `dest`, which
/// must not exist yet. Returns the package directory and its manifest.
pub fn install_archive(
    archive: &[u8],
    release: &ReleaseRef<'_>,
    publisher_key: &str,
    dest: &Path,
) -> Result<EnabledPackage, InstallError> {
    if archive.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(InstallError::Archive(
            "archive exceeds its size bound".into(),
        ));
    }
    if hex::encode(Sha256::digest(archive)) != release.archive_sha256 {
        return Err(InstallError::ArchiveHash);
    }
    let cache = read_cache(archive)?;
    let root = cache
        .narinfos
        .iter()
        .find(|info| info.store_path == release.store_path)
        .ok_or_else(|| InstallError::Archive("archive lacks the catalog's store path".into()))?;
    let self_name = &root.store_path["/nix/store/".len()..];
    if root.references.iter().any(|r| r != self_name) {
        return Err(InstallError::NotDeclarationOnly(
            "the release references other store paths".into(),
        ));
    }
    verify_signature(root, publisher_key)?;
    let compressed = cache
        .files
        .get(&root.url)
        .ok_or_else(|| InstallError::Archive("archive lacks the release NAR".into()))?;
    let nar = decompress(root, compressed)?;
    let digest = format!("sha256:{}", nix32(&Sha256::digest(&nar)));
    if digest != root.nar_hash || nar.len() as u64 != root.nar_size {
        return Err(InstallError::NarHash);
    }
    unpack_nar(&nar, dest)?;
    let manifest = read_manifest(dest)?;
    Ok(EnabledPackage {
        id: release.plugin_id.to_owned(),
        package: dest.to_path_buf(),
        files: BTreeMap::new(),
        entry: manifest.entry,
        sources: manifest.sources,
        requires: Vec::new(),
        hosts: manifest.hosts,
        credential_host: manifest.credential_host,
    })
}

/// The fields of the generated `manifest.json` (plugin-host `package.rs`
/// `Manifest`) that an Android install keeps. Native fields must be empty.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[allow(dead_code)]
    publisher: serde_json::Value,
    entry: String,
    sources: Vec<String>,
    #[serde(default)]
    packages: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    files: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    services: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    ports: serde_json::Value,
    #[serde(default)]
    hosts: Vec<String>,
    /// ADR 0034: one host name, never `null`.
    #[serde(
        default,
        rename = "credentialHost",
        deserialize_with = "credential_host"
    )]
    credential_host: Option<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default, rename = "requiresReleases")]
    #[allow(dead_code)]
    requires_releases: Vec<serde_json::Value>,
}

fn credential_host<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    <String as serde::Deserialize>::deserialize(deserializer).map(Some)
}

fn read_manifest(package: &Path) -> Result<Manifest, InstallError> {
    let bytes = fs::read(package.join("manifest.json"))
        .map_err(|_| InstallError::Archive("package lacks manifest.json".into()))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|e| InstallError::Archive(format!("invalid manifest: {e}")))?;
    for (field, empty) in [
        ("packages", manifest.packages.is_empty()),
        ("files", manifest.files.is_empty()),
        ("services", manifest.services.is_empty()),
        ("requires", manifest.requires.is_empty()),
        (
            "ports",
            manifest.ports.is_null()
                || manifest
                    .ports
                    .as_object()
                    .is_some_and(|o| o.values().all(|v| v.as_array().is_some_and(Vec::is_empty))),
        ),
    ] {
        if !empty {
            return Err(InstallError::NotDeclarationOnly(format!(
                "manifest has {field}"
            )));
        }
    }
    // The secret goes to one of the hosts the plugin may reach (ADR 0034).
    if manifest
        .credential_host
        .as_ref()
        .is_some_and(|host| !manifest.hosts.contains(host))
    {
        return Err(InstallError::Archive(
            "credentialHost must be one of the declared hosts".into(),
        ));
    }
    Ok(manifest)
}

/// Unpack a NAR into `dest`. Regular files and directories only: a
/// declaration-only package has no symlinks. Nothing is made executable.
pub fn unpack_nar(nar: &[u8], dest: &Path) -> Result<(), InstallError> {
    let storage = |e: io::Error| InstallError::Storage(e.to_string());
    if dest.exists() {
        return Err(InstallError::Storage("destination already exists".into()));
    }
    let mut reader = NarReader {
        bytes: nar,
        at: 0,
        entries: 0,
    };
    reader.expect("nix-archive-1")?;
    reader.node(dest, true).map_err(|e| match e {
        NarError::Format(m) => InstallError::Nar(m),
        NarError::Io(e) => storage(e),
    })?;
    if reader.at != nar.len() {
        return Err(InstallError::Nar("trailing bytes".into()));
    }
    Ok(())
}

enum NarError {
    Format(String),
    Io(io::Error),
}
impl From<InstallError> for NarError {
    fn from(e: InstallError) -> Self {
        NarError::Format(e.to_string())
    }
}

struct NarReader<'a> {
    bytes: &'a [u8],
    at: usize,
    entries: usize,
}

impl NarReader<'_> {
    fn bytes(&mut self) -> Result<&[u8], InstallError> {
        let bad = || InstallError::Nar("truncated".into());
        let len_bytes = self.bytes.get(self.at..self.at + 8).ok_or_else(bad)?;
        let len = u64::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
        let start = self.at + 8;
        let end = start.checked_add(len).ok_or_else(bad)?;
        let padded = end.checked_add((8 - len % 8) % 8).ok_or_else(bad)?;
        let value = self.bytes.get(start..end).ok_or_else(bad)?;
        if self
            .bytes
            .get(end..padded)
            .ok_or_else(bad)?
            .iter()
            .any(|b| *b != 0)
        {
            return Err(InstallError::Nar("non-zero padding".into()));
        }
        self.at = padded;
        Ok(value)
    }
    fn string(&mut self) -> Result<String, InstallError> {
        String::from_utf8(self.bytes()?.to_vec())
            .map_err(|_| InstallError::Nar("non-UTF-8 token".into()))
    }
    fn expect(&mut self, token: &str) -> Result<(), InstallError> {
        let got = self.string()?;
        if got == token {
            Ok(())
        } else {
            Err(InstallError::Nar(format!(
                "expected {token:?}, found {got:?}"
            )))
        }
    }

    fn node(&mut self, path: &Path, top: bool) -> Result<(), NarError> {
        self.entries += 1;
        if self.entries > MAX_NAR_ENTRIES {
            return Err(NarError::Format("too many entries".into()));
        }
        self.expect("(")?;
        self.expect("type")?;
        match self.string()?.as_str() {
            "directory" => {
                fs::create_dir(path).map_err(NarError::Io)?;
                let mut previous: Option<String> = None;
                loop {
                    match self.string()?.as_str() {
                        ")" => return Ok(()),
                        "entry" => {
                            self.expect("(")?;
                            self.expect("name")?;
                            let name = self.string()?;
                            if name.is_empty()
                                || name == "."
                                || name == ".."
                                || name.contains('/')
                                || name.contains('\0')
                                || previous
                                    .as_ref()
                                    .is_some_and(|p| p.as_str() >= name.as_str())
                            {
                                return Err(NarError::Format(format!(
                                    "invalid entry name {name:?}"
                                )));
                            }
                            let child = path.join(&name);
                            if !matches!(
                                Path::new(&name).components().next(),
                                Some(Component::Normal(_))
                            ) {
                                return Err(NarError::Format("invalid entry name".into()));
                            }
                            self.expect("node")?;
                            self.node(&child, false)?;
                            self.expect(")")?;
                            previous = Some(name);
                        }
                        other => return Err(NarError::Format(format!("unexpected {other:?}"))),
                    }
                }
            }
            "regular" => {
                if top {
                    return Err(NarError::Format("package root must be a directory".into()));
                }
                let mut token = self.string()?;
                if token == "executable" {
                    // Recorded but never applied: Android plugins run no files.
                    self.expect("")?;
                    token = self.string()?;
                }
                if token != "contents" {
                    return Err(NarError::Format(format!(
                        "expected contents, found {token:?}"
                    )));
                }
                let contents = self.bytes()?.to_vec();
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(NarError::Io)?;
                file.write_all(&contents).map_err(NarError::Io)?;
                self.expect(")")?;
                Ok(())
            }
            "symlink" => Err(NarError::Format(
                "symlinks are not allowed in Android plugins".into(),
            )),
            other => Err(NarError::Format(format!("unknown node type {other:?}"))),
        }
    }
}

/// Where the Android app keeps installed plugins and its enabled list.
static ANDROID_STORE: std::sync::OnceLock<AndroidPluginStore> = std::sync::OnceLock::new();

/// Called once by the Android edge with the app's private state root. From
/// then on `plugin_installation::read` reads this store.
pub fn use_android_store(private_root: &Path) {
    let _ = ANDROID_STORE.set(AndroidPluginStore::new(private_root));
    crate::plugin_installation::override_read(|| {
        ANDROID_STORE
            .get()
            .expect("Android plugin store set before its reader")
            .read()
    });
}

pub fn android_store() -> Option<&'static AndroidPluginStore> {
    ANDROID_STORE.get()
}

pub struct AndroidPluginStore {
    root: PathBuf,
}

impl AndroidPluginStore {
    pub fn new(private_root: &Path) -> Self {
        Self {
            root: private_root.join("plugins"),
        }
    }

    pub fn registry_path(&self) -> PathBuf {
        self.root.join("enabled-packages.json")
    }

    pub fn read(&self) -> Result<Vec<EnabledPackage>, String> {
        read_list(&self.registry_path())
    }

    /// Install one release and enable it, replacing an earlier release of the
    /// same plugin. The new package directory is named by its store path, so
    /// a reinstall of the same release is a no-op unpack.
    pub fn install(
        &self,
        archive: &[u8],
        release: &ReleaseRef<'_>,
        publisher_key: &str,
    ) -> Result<EnabledPackage, InstallError> {
        let storage = |e: io::Error| InstallError::Storage(e.to_string());
        fs::create_dir_all(self.root.join("packages")).map_err(storage)?;
        let name = &release.store_path["/nix/store/".len().min(release.store_path.len())..];
        let dest = self.root.join("packages").join(name);
        let staging = self.root.join("packages").join(format!(".staging-{name}"));
        let _ = fs::remove_dir_all(&staging);
        let mut package = install_archive(archive, release, publisher_key, &staging)?;
        if dest.exists() {
            fs::remove_dir_all(&staging).map_err(storage)?;
        } else {
            fs::rename(&staging, &dest).map_err(storage)?;
        }
        package.package = dest;
        for path in [self.installed_path(), self.registry_path()] {
            let mut list = read_list(&path).map_err(InstallError::Storage)?;
            list.retain(|p| p.id != package.id);
            list.push(package.clone());
            write_list(&path, list).map_err(InstallError::Storage)?;
        }
        Ok(package)
    }

    fn installed_path(&self) -> PathBuf {
        self.root.join("installed-packages.json")
    }

    /// Enable or disable an installed plugin. Disabling keeps its files.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let installed = read_list(&self.installed_path())?;
        let package = installed
            .into_iter()
            .find(|p| p.id == id)
            .ok_or("this plugin is not installed")?;
        let mut list = self.read()?;
        list.retain(|p| p.id != id);
        if enabled {
            list.push(package);
        }
        write_list(&self.registry_path(), list)
    }
}

fn read_list(path: &Path) -> Result<Vec<EnabledPackage>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

fn write_list(path: &Path, mut list: Vec<EnabledPackage>) -> Result<(), String> {
    list.sort_by(|a, b| a.id.cmp(&b.id));
    let bytes = crate::plugin_installation::encode(&list)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "nixfree_install_tests.rs"]
pub(crate) mod tests;
