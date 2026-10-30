use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

pub const STATE_ROOT: &str = "/var/lib/korri-plugin-host";
pub const ROOTS: &str = "/nix/var/nix/gcroots/korri-plugin-host";
pub const OFFICIAL_CATALOG: &str = "/etc/korri-plugin-host/official-catalog-url";
pub const SOURCES_DIR: &str = "sources";
pub const STAGING_DIR: &str = "staging";
pub const MAX_JSON_BYTES: usize = 128 * 1024;

/// Every state mutation, including source writes and stale staging cleanup,
/// requires the same exclusive lock for this root.
pub struct State {
    root: PathBuf,
    _lock: File,
}

impl State {
    pub fn open(root: &Path) -> Result<Self, String> {
        directory(root)?;
        let lock = lock(&root.join("lock"))?;
        Ok(Self {
            root: root.into(),
            _lock: lock,
        })
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub fn staging(&self) -> Result<tempfile::TempDir, String> {
        let root = self.root.join(STAGING_DIR);
        directory(&root)?;
        tempfile::Builder::new()
            .prefix("download-")
            .rand_bytes(8)
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(root)
            .map_err(|e| e.to_string())
    }
    pub fn cleanup_staging(&self) -> Result<(), String> {
        let root = self.root.join(STAGING_DIR);
        if fs::symlink_metadata(&root).is_err() {
            return Ok(());
        }
        directory(&root)?;
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            if !name
                .to_str()
                .and_then(|s| s.strip_prefix("download-"))
                .is_some_and(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
            {
                return Err("unexpected entry in plugin staging directory".into());
            }
            directory(&entry.path())?;
            fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
        }
        File::open(root)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    }
}

pub fn directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| e.to_string())?;
        sync_parent(path)?;
    }
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "{} must be a private directory owned by the caller",
            path.display()
        ));
    }
    Ok(())
}

pub fn lock(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    protect_file(&file)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("another plugin operation is in progress".into());
    }
    Ok(file)
}

fn protect_file(file: &File) -> Result<(), String> {
    let m = file.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.permissions().mode() & 0o077 != 0
    {
        return Err("plugin state must be a private regular file owned by the caller".into());
    }
    Ok(())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    protect_file(&file)?;
    let mut bytes = Vec::new();
    file.take(MAX_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err("plugin state JSON is too large".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("invalid plugin receipt: {e}"))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err("plugin state JSON is too large".into());
    }
    write_atomic(path, &bytes)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_atomic_mode(path, bytes, 0o600)
}

pub fn write_atomic_mode(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let temporary = path.with_extension("new");
    // This path is inside the locked, root-owned host directory. Leftovers are
    // never followed, even after an interrupted write.
    remove(&temporary)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.set_permissions(fs::Permissions::from_mode(mode))
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())?;
    sync_parent(path)
}

pub fn root_link(path: &Path, target: &Path) -> Result<(), String> {
    let temporary = path.with_extension("new");
    remove(&temporary)?;
    symlink(target, &temporary).map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())?;
    sync_parent(path)
}

pub fn remove(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove {}: {e}", path.display())),
    }
}

pub fn sync_parent(path: &Path) -> Result<(), String> {
    File::open(path.parent().ok_or("path has no parent")?)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    struct ForkChild(libc::pid_t);

    impl ForkChild {
        fn wait(&mut self) -> i32 {
            let mut status = 0;
            loop {
                if unsafe { libc::waitpid(self.0, &mut status, 0) } == self.0 {
                    self.0 = 0;
                    return status;
                }
                assert_eq!(
                    std::io::Error::last_os_error().kind(),
                    std::io::ErrorKind::Interrupted
                );
            }
        }
    }

    impl Drop for ForkChild {
        fn drop(&mut self) {
            if self.0 > 0 {
                unsafe { libc::kill(self.0, libc::SIGKILL) };
                self.wait();
            }
        }
    }

    #[test]
    fn state_drop_releases_the_lock_while_a_fork_child_retains_its_descriptor() {
        let root = tempfile::tempdir().unwrap();
        let state = State::open(&root.path().join("state")).unwrap();
        assert_ne!(
            unsafe { libc::fcntl(state._lock.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        let (mut parent, child) = UnixStream::pair().unwrap();
        let parent_fd = parent.as_raw_fd();
        let child_fd = child.as_raw_fd();
        // Command::spawn can fork concurrently with another thread's State.
        // Hold that real inherited descriptor before exec/exit, without timing.
        // The post-fork child uses only async-signal-safe calls, never Rust Drop.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "{}", std::io::Error::last_os_error());
        if pid == 0 {
            unsafe {
                libc::close(parent_fd);
                let mut byte = 0_u8;
                loop {
                    let read = libc::read(child_fd, (&mut byte as *mut u8).cast(), 1);
                    if read >= 0 {
                        libc::_exit(if read == 1 { 0 } else { 1 });
                    }
                    if *libc::__errno_location() != libc::EINTR {
                        libc::_exit(2);
                    }
                }
            }
        }
        let mut process = ForkChild(pid);
        drop(child);
        assert!(State::open(state.root()).is_err());
        drop(state);
        let successor = State::open(&root.path().join("state")).unwrap();
        assert!(State::open(successor.root()).is_err());
        parent.write_all(&[1]).unwrap();
        assert_eq!(process.wait(), 0);
        // A child's later descriptor close must not release its successor.
        assert!(State::open(successor.root()).is_err());
        drop(successor);
        State::open(&root.path().join("state")).unwrap();
    }
}
