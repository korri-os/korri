//! Closed bytes supplied by admission, never an evaluator filesystem loader.
//! The caller owns authority; selecting a directory does not select its files.

use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::OpenOptionsExt,
    },
    path::Path,
    sync::Arc,
};

pub const PLUGIN_SOURCE_BYTES: usize = 128 * 1024;
pub const PLUGIN_GRAPH_SOURCE_BYTES: usize = 4 * 1024 * 1024;
pub const PLUGIN_GRAPH_SOURCE_ENTRIES: usize = 512;

/// In-memory admission limits, not plugin configuration or a persisted schema.
/// Entry slots bound fixed host overhead; path_bytes bounds cumulative owned
/// names and traversal scratch. Content is charged once per canonical identity.
#[derive(Clone, Copy, Debug)]
pub struct SnapshotLimits {
    pub bytes: usize,
    pub entries: usize,
    pub path_bytes: usize,
    pub steps: usize,
}

impl SnapshotLimits {
    fn plugin() -> Self {
        Self {
            bytes: PLUGIN_SOURCE_BYTES,
            entries: 1,
            // Use the OS path ceiling, not a new graph retention allowance.
            path_bytes: libc::PATH_MAX as usize,
            steps: libc::PATH_MAX as usize,
        }
    }

    fn package_graph() -> Self {
        Self {
            bytes: PLUGIN_GRAPH_SOURCE_BYTES,
            entries: PLUGIN_GRAPH_SOURCE_ENTRIES,
            path_bytes: 256 * 1024,
            steps: 8192,
        }
    }
}

struct Entry {
    name: String,
    identity: String,
    bytes: Arc<Vec<u8>>,
}

pub struct SourceSnapshot {
    entries: Vec<Entry>,
}

impl std::fmt::Debug for SourceSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceSnapshot")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl SourceSnapshot {
    pub fn plugin(source: &str) -> Result<Self, String> {
        Self::from_memory(
            &[("plugin.ts", source.as_bytes())],
            SnapshotLimits::plugin(),
        )
    }

    pub(super) fn javascript(source: &str) -> Result<Self, String> {
        if source.len() > super::preparation::JAVASCRIPT_BYTES {
            return Err("plugin JavaScript exceeds 512 KiB".into());
        }
        Self::from_memory(
            &[("plugin.js", source.as_bytes())],
            SnapshotLimits {
                bytes: super::preparation::JAVASCRIPT_BYTES,
                ..SnapshotLimits::plugin()
            },
        )
    }

    /// Only sources named by the generated manifest are admitted. The caller
    /// validates the manifest and publisher before supplying this selection.
    pub fn package_plugin(package: &Path, entry: &str, sources: &[String]) -> Result<Self, String> {
        if entry != "plugin.ts" {
            return Err("plugin entry must be plugin.ts".into());
        }
        if sources.is_empty() || !sources.iter().any(|name| name == entry) {
            return Err("plugin source inventory must contain plugin.ts".into());
        }
        if sources.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("plugin source inventory must be sorted and unique".into());
        }
        let names: Vec<_> = sources.iter().map(String::as_str).collect();
        Self::from_directory(package, &names, SnapshotLimits::package_graph())
    }

    /// Stable approval input for the complete retained graph. Names and
    /// canonical identities are included because both affect module resolution.
    pub fn canonical_entries(&self) -> Vec<(&str, &str, &[u8])> {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.name.as_str(),
                    entry.identity.as_str(),
                    entry.bytes.as_slice(),
                )
            })
            .collect();
        entries.sort_by_key(|(name, _, _)| *name);
        entries
    }

    pub fn from_memory(sources: &[(&str, &[u8])], limits: SnapshotLimits) -> Result<Self, String> {
        let mut budget = Budget(limits);
        let mut snapshot = Self::empty(sources.len(), &budget)?;
        for (name, bytes) in sources {
            validate_name(name)?;
            snapshot.unique(name)?;
            budget.content(bytes.len())?;
            let name = budget.name(name)?;
            let identity = budget.name(&name)?;
            let mut owned = content_buffer(bytes.len())?;
            owned.copy_from_slice(bytes);
            snapshot.entries.push(Entry {
                name,
                identity,
                bytes: Arc::new(owned),
            });
        }
        Ok(snapshot)
    }

    /// Read an explicit set from one admitted immutable output. Symlinks may
    /// alias only another selected canonical name inside that output. No tree
    /// walk, ambient package discovery, or cross-output authority is implied.
    pub fn from_directory(
        root: &Path,
        names: &[&str],
        limits: SnapshotLimits,
    ) -> Result<Self, String> {
        let mut budget = Budget(limits);
        let mut snapshot = Self::empty(names.len(), &budget)?;
        // Validate the entire selection before any filesystem access.
        for (index, name) in names.iter().enumerate() {
            validate_name(name)?;
            if names[..index].contains(name) {
                return Err("duplicate source identity".into());
            }
        }
        let directory = open_root(root, &mut budget)?;
        for name in names {
            let selected_name = budget.name(name)?;
            let (mut file, identity) = resolve(&directory, name, &mut budget)?;
            if !names.contains(&identity.as_str()) {
                return Err("canonical source is not selected".into());
            }
            let bytes =
                if let Some(entry) = snapshot.entries.iter().find(|e| e.identity == identity) {
                    Arc::clone(&entry.bytes)
                } else {
                    let metadata = file.metadata().map_err(|_| "source metadata unavailable")?;
                    if !metadata.is_file() {
                        return Err("source must be a regular file".into());
                    }
                    let length = usize::try_from(metadata.len())
                        .map_err(|_| "source byte budget exceeded")?;
                    budget.content(length)?;
                    let mut bytes = content_buffer(length)?;
                    file.read_exact(&mut bytes)
                        .map_err(|_| "source changed or could not be read")?;
                    // Do not read a limit+1 sentinel byte outside the reservation.
                    // Immutability comes from admission, not this best-effort race
                    // check: mutable files cannot supply an atomic package view.
                    if file
                        .metadata()
                        .map_err(|_| "source metadata unavailable")?
                        .len()
                        != metadata.len()
                    {
                        return Err("source changed while reading".into());
                    }
                    Arc::new(bytes)
                };
            snapshot.entries.push(Entry {
                name: selected_name,
                identity,
                bytes,
            });
        }
        Ok(snapshot)
    }

    pub fn bytes(&self, name: &str) -> Result<&[u8], String> {
        Ok(self.entry(name)?.bytes.as_slice())
    }

    pub fn text(&self, name: &str) -> Result<&str, String> {
        std::str::from_utf8(self.bytes(name)?).map_err(|_| "source is not UTF-8".into())
    }

    pub fn identity(&self, name: &str) -> Result<&str, String> {
        Ok(&self.entry(name)?.identity)
    }

    fn entry(&self, name: &str) -> Result<&Entry, String> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .ok_or_else(|| "source is not selected".into())
    }

    fn unique(&self, name: &str) -> Result<(), String> {
        if self.entries.iter().any(|entry| entry.name == name) {
            return Err("duplicate source identity".into());
        }
        Ok(())
    }

    fn empty(count: usize, budget: &Budget) -> Result<Self, String> {
        if count > budget.0.entries {
            return Err("source entry budget exceeded".into());
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(count)
            .map_err(|_| "source entry allocation failed")?;
        Ok(Self { entries })
    }
}

struct Budget(SnapshotLimits);

impl Budget {
    fn content(&mut self, length: usize) -> Result<(), String> {
        self.0.bytes = self
            .0
            .bytes
            .checked_sub(length)
            .ok_or("source byte budget exceeded")?;
        Ok(())
    }

    fn paths(&mut self, length: usize) -> Result<(), String> {
        self.0.path_bytes = self
            .0
            .path_bytes
            .checked_sub(length)
            .ok_or("source path budget exceeded")?;
        Ok(())
    }

    fn step(&mut self) -> Result<(), String> {
        self.0.steps = self
            .0
            .steps
            .checked_sub(1)
            .ok_or("source traversal budget exceeded")?;
        Ok(())
    }

    fn name(&mut self, name: &str) -> Result<String, String> {
        self.paths(name.len())?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(name.len())
            .map_err(|_| "source path allocation failed")?;
        owned.push_str(name);
        Ok(owned)
    }
}

fn content_buffer(length: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| "source content allocation failed")?;
    bytes.resize(length, 0);
    Ok(bytes)
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.contains('\0')
        || name.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err("source identity must be a normalized relative path".into());
    }
    Ok(())
}

fn component(directory: &File, name: &str) -> Result<File, std::io::Error> {
    // This helper never follows a symlink, including in intermediate paths.
    // Each caller charges the CString and the syscall before entering here.
    let name = CString::new(name).expect("validated source component");
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // openat returned a new, exclusively owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn open_root(root: &Path, budget: &mut Budget) -> Result<File, String> {
    let root = root.to_str().ok_or("source root must be UTF-8")?;
    let relative = root
        .strip_prefix('/')
        .ok_or("source root must be absolute")?;
    if !relative.is_empty() {
        validate_name(relative)?;
    }
    budget.paths(root.len())?;
    budget.step()?;
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(|_| "source root unavailable")?;
    for part in relative.split('/').filter(|part| !part.is_empty()) {
        budget.step()?;
        budget.paths(part.len() + 1)?;
        directory = component(&directory, part)
            .map_err(|_| "source root must be an exact directory without symlinks")?;
        if !directory
            .metadata()
            .map_err(|_| "source root metadata unavailable")?
            .is_dir()
        {
            return Err("source root must be a directory".into());
        }
    }
    Ok(directory)
}

fn resolve(root: &File, name: &str, budget: &mut Budget) -> Result<(File, String), String> {
    let mut current = budget.name(name)?;
    'restart: loop {
        let mut parent = None;
        let mut offset = 0;
        for part in current.split('/') {
            let directory = parent.as_ref().unwrap_or(root);
            budget.step()?;
            if matches!(part, "" | "." | "..") {
                // Resolve earlier symlinks first: redirect/../file is not
                // lexically equivalent to file when redirect is a symlink.
                let prefix = current[..offset].trim_end_matches('/');
                let prefix = if part == ".." {
                    if prefix.is_empty() {
                        return Err("source symlink escapes its selection".into());
                    }
                    prefix.rsplit_once('/').map_or("", |(parent, _)| parent)
                } else {
                    prefix
                };
                let suffix = current[offset + part.len()..].trim_start_matches('/');
                budget.paths(current.len())?;
                let mut resolved = String::new();
                resolved
                    .try_reserve_exact(current.len())
                    .map_err(|_| "source path allocation failed")?;
                resolved.push_str(prefix);
                if !prefix.is_empty() && !suffix.is_empty() {
                    resolved.push('/');
                }
                resolved.push_str(suffix);
                if resolved.is_empty() {
                    return Err("source must be a regular file".into());
                }
                current = resolved;
                continue 'restart;
            }
            budget.paths(part.len() + 1)?;
            match component(directory, part) {
                Ok(file) => {
                    if offset + part.len() == current.len() {
                        if !file
                            .metadata()
                            .map_err(|_| "source metadata unavailable")?
                            .is_file()
                        {
                            return Err("source must be a regular file".into());
                        }
                        return Ok((file, current));
                    }
                    if !file
                        .metadata()
                        .map_err(|_| "source metadata unavailable")?
                        .is_dir()
                    {
                        return Err("source ancestor must be a directory".into());
                    }
                    parent = Some(file);
                }
                Err(error) if error.raw_os_error() == Some(libc::ELOOP) => {
                    budget.step()?;
                    budget.paths(part.len() + 1)?;
                    let part_name = CString::new(part).expect("validated source component");
                    // Reserve the maximum metadata read before asking the
                    // filesystem for a link target, even though scratch is on
                    // the stack. It must not bypass aggregate path accounting.
                    budget.paths(libc::PATH_MAX as usize)?;
                    let mut target = [0u8; libc::PATH_MAX as usize];
                    let count = unsafe {
                        libc::readlinkat(
                            directory.as_raw_fd(),
                            part_name.as_ptr(),
                            target.as_mut_ptr().cast(),
                            target.len(),
                        )
                    };
                    if count <= 0 || count as usize == target.len() {
                        return Err("source symlink unavailable or too long".into());
                    }
                    let target = std::str::from_utf8(&target[..count as usize])
                        .map_err(|_| "source symlink must be UTF-8")?;
                    // Absolute and cross-output links cannot introduce authority.
                    if target.starts_with('/') || target.contains('\0') {
                        return Err("source symlink escapes its selection".into());
                    }
                    let suffix = &current[offset + part.len()..];
                    let capacity = current
                        .len()
                        .checked_add(target.len())
                        .and_then(|n| n.checked_add(1))
                        .ok_or("source path budget exceeded")?;
                    budget.paths(capacity)?;
                    let mut resolved = String::new();
                    resolved
                        .try_reserve_exact(capacity)
                        .map_err(|_| "source path allocation failed")?;
                    resolved.push_str(&current[..offset]);
                    resolved.push_str(target);
                    resolved.push_str(suffix);
                    current = resolved;
                    continue 'restart;
                }
                Err(_) => return Err("selected source unavailable".into()),
            }
            offset += part.len() + 1;
        }
        return Err("selected source unavailable".into());
    }
}
