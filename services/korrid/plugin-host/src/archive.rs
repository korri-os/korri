//! Standard Nix file-cache bundles. Tar confinement and byte limits are checked
//! before Nix imports anything into the system store.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Total files extracted from the outer tar, whose NARs are still compressed.
pub const MAX_EXPANDED_BYTES: u64 = 3 * 1024 * 1024 * 1024;
/// Actual decompressed NAR bytes, not merely the claimed metadata sizes.
pub const MAX_NAR_BYTES: u64 = 3 * 1024 * 1024 * 1024;
pub const MAX_ENTRY_COUNT: usize = 100_000;

pub fn path_to_file_uri(path: &Path) -> Result<String, String> {
    url::Url::from_directory_path(path)
        .map(|uri| uri.to_string())
        .map_err(|_| "file cache requires an absolute directory path".into())
}

pub fn pack(cache_dir: &Path, output_path: &Path) -> Result<String, String> {
    validate_cache_layout(cache_dir)?;
    let parent = output_path
        .parent()
        .ok_or("archive has no parent directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let digest = {
        let writer = ArchiveWriter {
            file: temporary.as_file_mut(),
            hash: Sha256::new(),
            bytes: 0,
        };
        let mut tar = tar::Builder::new(writer);
        let mut paths = Vec::new();
        for item in fs::read_dir(cache_dir).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            let relative = PathBuf::from(item.file_name());
            let metadata = fs::symlink_metadata(item.path()).map_err(|e| e.to_string())?;
            if relative == Path::new("nar") && metadata.is_dir() {
                for nar in fs::read_dir(item.path()).map_err(|e| e.to_string())? {
                    paths.push(relative.join(nar.map_err(|e| e.to_string())?.file_name()));
                }
            } else if metadata.is_dir()
                && matches!(relative.to_str(), Some("log" | "realisations"))
                && fs::read_dir(item.path())
                    .map_err(|e| e.to_string())?
                    .next()
                    .is_none()
            {
                // Nix 2.31 creates these empty directories even when copying
                // only concrete content-addressed outputs, with no build logs.
            } else {
                paths.push(relative);
            }
        }
        if paths.len() > MAX_ENTRY_COUNT {
            return Err("too many cache files".into());
        }
        paths.sort();
        for path in paths {
            validate_cache_entry_path(&path)?;
            let mut file = open_regular(&cache_dir.join(&path))?;
            let mut header = tar::Header::new_gnu();
            header.set_size(file.metadata().map_err(|e| e.to_string())?.len());
            header.set_mode(0o444);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_cksum();
            tar.append_data(&mut header, &path, &mut file)
                .map_err(|e| e.to_string())?;
        }
        let writer = tar.into_inner().map_err(|e| e.to_string())?;
        hex::encode(writer.hash.finalize())
    };
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o444))
        .map_err(|e| e.to_string())?;
    temporary
        .persist_noclobber(output_path)
        .map_err(|e| e.to_string())?;
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(digest)
}

struct ArchiveWriter<'a> {
    file: &'a mut File,
    hash: Sha256,
    bytes: u64,
}
impl Write for ArchiveWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.saturating_add(bytes.len() as u64) >= MAX_ARCHIVE_BYTES {
            return Err(io::Error::other("archive exceeds the size limit"));
        }
        let written = self.file.write(bytes)?;
        self.hash.update(&bytes[..written]);
        self.bytes += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The caller owns cleanup of this fresh staging directory, including errors.
/// Open directory descriptors keep writes confined even if a path is replaced.
pub fn extract(archive_path: &Path, staging_dir: &Path) -> Result<(), String> {
    let file = open_regular(archive_path)?;
    if file.metadata().map_err(|e| e.to_string())?.len() >= MAX_ARCHIVE_BYTES {
        return Err("archive exceeds the size limit".into());
    }
    if fs::canonicalize(staging_dir).map_err(|e| format!("invalid staging directory: {e}"))?
        != staging_dir
    {
        return Err("staging directory must not contain symlinks".into());
    }
    let root = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(staging_dir)
        .map_err(|e| format!("invalid staging directory: {e}"))?;
    let metadata = root.metadata().map_err(|e| e.to_string())?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
        return Err("staging must be caller-owned and not writable by other users".into());
    }
    if fs::read_dir(staging_dir)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("staging directory must be empty".into());
    }
    let mut archive = tar::Archive::new(file.take(MAX_ARCHIVE_BYTES));
    let mut seen = HashSet::new();
    let mut total = 0u64;
    // Raw iteration exposes GNU/PAX headers before the tar library buffers
    // their bodies. Our producer needs only ordinary regular-file headers.
    for item in archive.entries().map_err(|e| e.to_string())?.raw(true) {
        let mut entry = item.map_err(|e| e.to_string())?;
        if !entry.header().entry_type().is_file() {
            return Err("archive entries must be regular files".into());
        }
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        validate_cache_entry_path(&path)?;
        if !seen.insert(path.clone()) {
            return Err("duplicate archive path".into());
        }
        if seen.len() > MAX_ENTRY_COUNT {
            return Err("too many archive entries".into());
        }
        let size = entry.header().size().map_err(|e| e.to_string())?;
        total = total
            .checked_add(size)
            .filter(|n| *n <= MAX_EXPANDED_BYTES)
            .ok_or("tar payload exceeds size limit")?;
        let mut output = create_entry(&root, &path)?;
        let copied = io::copy(&mut entry, &mut output).map_err(|e| e.to_string())?;
        if copied != size {
            return Err("truncated archive entry".into());
        }
    }
    validate_cache_layout(staging_dir)
}

fn create_entry(root: &File, path: &Path) -> Result<File, String> {
    let nar_directory;
    let directory = if path.parent() == Some(Path::new("nar")) {
        let name = c"nar";
        if unsafe { libc::mkdirat(root.as_raw_fd(), name.as_ptr(), 0o700) } != 0
            && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists
        {
            return Err(io::Error::last_os_error().to_string());
        }
        let fd = unsafe {
            libc::openat(
                root.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        nar_directory = unsafe { File::from_raw_fd(fd) };
        &nar_directory
    } else {
        root
    };
    let name = CString::new(
        path.file_name()
            .ok_or("missing archive filename")?
            .as_encoded_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn open_regular(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("expected a regular file".into());
    }
    Ok(file)
}

fn validate_cache_layout(path: &Path) -> Result<(), String> {
    open_regular(&path.join("nix-cache-info"))
        .map_err(|_| "cache lacks regular nix-cache-info".to_string())?;
    if !fs::symlink_metadata(path.join("nar"))
        .map_err(|_| "cache lacks nar directory")?
        .is_dir()
    {
        return Err("cache nar entry must be a real directory".into());
    }
    Ok(())
}

fn validate_cache_entry_path(path: &Path) -> Result<(), String> {
    let parts: Vec<_> = path.components().collect();
    let valid_hash = |text: &str| {
        (32..=64).contains(&text.len())
            && text
                .bytes()
                .all(|b| b"0123456789abcdfghijklmnpqrsvwxyz".contains(&b))
    };
    let valid = match parts.as_slice() {
        [Component::Normal(name)] => {
            name == &"nix-cache-info"
                || name
                    .to_str()
                    .and_then(|s| s.strip_suffix(".narinfo"))
                    .is_some_and(valid_hash)
        }
        [Component::Normal(dir), Component::Normal(name)] if dir == &"nar" => name
            .to_str()
            .and_then(|s| s.strip_suffix(".nar.xz").or_else(|| s.strip_suffix(".nar")))
            .is_some_and(valid_hash),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("invalid Nix file-cache archive path".into())
    }
}

/// This is the observed JSON contract of locked Nix 2.31 file-cache path-info.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheInfo {
    ca: Option<String>,
    nar_size: u64,
    nar_hash: String,
    download_size: u64,
    compression: String,
    url: PathBuf,
}

/// Check real NAR sizes and hashes before system-store import. Metadata alone
/// cannot enforce a decompression bound because its size fields are untrusted.
pub fn preflight_cache(
    nix: &Path,
    store_path: &str,
    cache_dir: &Path,
    max_nar_bytes: u64,
) -> Result<(), String> {
    let max_nar_bytes = max_nar_bytes.min(MAX_NAR_BYTES);
    crate::package::validate_store_path(Path::new(store_path))?;
    let cache = path_to_file_uri(cache_dir)?;
    let json = crate::process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "--store",
            &cache,
            "path-info",
            "--json",
            "--recursive",
            store_path,
        ],
        Duration::from_secs(180),
    )?;
    let infos: BTreeMap<String, CacheInfo> =
        serde_json::from_str(&json).map_err(|e| format!("invalid Nix path-info: {e}"))?;
    if !infos.contains_key(store_path) || infos.len() > MAX_ENTRY_COUNT {
        return Err("invalid cache closure".into());
    }
    let mut total = 0u64;
    for (path, info) in infos {
        crate::package::validate_store_path(Path::new(&path))?;
        if !info.ca.is_some_and(|value| !value.is_empty()) {
            return Err("repository archive must contain only content-addressed paths".into());
        }
        validate_cache_entry_path(&info.url)?;
        if info.url.parent() != Some(Path::new("nar")) {
            return Err("NAR URL must remain inside the cache nar directory".into());
        }
        let file = open_regular(&cache_dir.join(&info.url))?;
        if file.metadata().map_err(|e| e.to_string())?.len() != info.download_size {
            return Err("compressed NAR size mismatch".into());
        }
        let reader: Box<dyn Read> = match info.compression.as_str() {
            "xz" => {
                let stream = xz2::stream::Stream::new_stream_decoder(
                    64 * 1024 * 1024,
                    xz2::stream::CONCATENATED,
                )
                .map_err(|e| e.to_string())?;
                Box::new(xz2::read::XzDecoder::new_stream(file, stream))
            }
            "none" => Box::new(file),
            _ => return Err("unsupported NAR compression".into()),
        };
        let remaining = max_nar_bytes
            .checked_sub(total)
            .ok_or("NAR size limit exceeded")?;
        let mut limited = reader.take(remaining + 1);
        let mut bytes = 0u64;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = limited
                .read(&mut buffer)
                .map_err(|e| format!("invalid compressed NAR: {e}"))?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            bytes += count as u64;
        }
        total += bytes;
        if total > max_nar_bytes {
            return Err("NAR size limit exceeded".into());
        }
        if bytes != info.nar_size {
            return Err("decompressed NAR size mismatch".into());
        }
        if format!("sha256-{}", STANDARD.encode(hash.finalize())) != info.nar_hash {
            return Err("NAR hash mismatch".into());
        }
    }
    crate::process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "--store",
            &cache,
            "--option",
            "trusted-public-keys",
            "",
            "store",
            "verify",
            "--recursive",
            "--sigs-needed",
            "1",
            store_path,
        ],
        Duration::from_secs(180),
    )?;
    Ok(())
}
