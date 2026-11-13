// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux filesystem and peer authority for one viewer/controller pair.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::CString,
    io,
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
};

struct PrivateDirectory {
    path: PathBuf,
    identity: (u64, u64),
}
impl PrivateDirectory {
    fn create(path: &Path) -> Result<Self> {
        ensure!(
            !path.as_os_str().is_empty(),
            "lifecycle socket requires a new parent directory"
        );
        // mkdir's mode is atomic, even with umask=0. Do not reuse or chmod a
        // pre-existing directory: it may belong to another launch.
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .context("create private lifecycle directory")?;
        Ok(Self {
            path: path.into(),
            identity: identity(path)?,
        })
    }
    fn remove(&self) {
        if identity(&self.path).ok() == Some(self.identity) {
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}
impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        self.remove();
    }
}

pub(super) struct SocketPath {
    path: PathBuf,
    identity: (u64, u64),
    directory: PrivateDirectory,
}
impl SocketPath {
    pub fn bind(path: &Path, controller_uid: u32) -> Result<(UnixListener, Self)> {
        let directory =
            PrivateDirectory::create(path.parent().context("lifecycle socket has no parent")?)?;
        let listener = UnixListener::bind(path).context("bind viewer lifecycle socket")?;
        let paths = Self {
            path: path.into(),
            identity: identity(path)?,
            directory,
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        if controller_uid != unsafe { libc::geteuid() } {
            // Grant the socket first, while the directory is still private.
            // Only the final directory ACL makes it reachable by the controller.
            grant_controller(path, controller_uid, 0o6, 0o6)?;
            grant_controller(&paths.directory.path, controller_uid, 0o7, 0o1)?;
        }
        Ok((listener, paths))
    }
    pub fn remove(&self) {
        if identity(&self.path).ok() == Some(self.identity) {
            let _ = std::fs::remove_file(&self.path);
        }
        self.directory.remove();
    }
}
impl Drop for SocketPath {
    fn drop(&mut self) {
        self.remove();
    }
}
fn identity(path: &Path) -> io::Result<(u64, u64)> {
    let metadata = std::fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

fn grant_controller(path: &Path, uid: u32, owner: u16, controller: u16) -> Result<()> {
    // Linux UAPI linux/posix_acl_xattr.h: version 2, little-endian tag/perm/id.
    // Group and other get no rights. The mask grants rights only to this named
    // user; it is not a grant to the socket's owning group.
    let mut acl = 2u32.to_le_bytes().to_vec();
    for (tag, permissions, id) in [
        (0x01u16, owner, u32::MAX),   // ACL_USER_OBJ
        (0x02, controller, uid),      // ACL_USER
        (0x04, 0, u32::MAX),          // ACL_GROUP_OBJ
        (0x10, controller, u32::MAX), // ACL_MASK
        (0x20, 0, u32::MAX),          // ACL_OTHER
    ] {
        acl.extend(tag.to_le_bytes());
        acl.extend(permissions.to_le_bytes());
        acl.extend(id.to_le_bytes());
    }
    let path = CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: the kernel reads these live, bounded buffers only during the call.
    let result = unsafe {
        libc::lsetxattr(
            path.as_ptr(),
            c"system.posix_acl_access".as_ptr(),
            acl.as_ptr().cast(),
            acl.len(),
            0,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error()).context("grant lifecycle controller ACL");
    }
    Ok(())
}

pub(super) fn check_peer(socket: &UnixStream, expected_uid: u32) -> io::Result<()> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of_val(&credentials) as libc::socklen_t;
    // SAFETY: getsockopt writes only into the supplied initialized ucred and length.
    let result = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    if length as usize != std::mem::size_of_val(&credentials) {
        return Err(io::Error::other("invalid lifecycle peer credentials"));
    }
    if credentials.uid != expected_uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("UID {}; expected {expected_uid}", credentials.uid),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permissive_umask_cannot_publish_the_private_directory() {
        use std::os::unix::process::CommandExt;
        let mut process = std::process::Command::new(std::env::current_exe().unwrap());
        process.args([
            "--exact",
            "lifecycle::access::tests::observe_directory_birth",
            "--ignored",
        ]);
        // umask is process-global. Change it only in this isolated child,
        // before exec, not in the multithreaded test runner or the viewer.
        unsafe {
            process.pre_exec(|| {
                libc::umask(0);
                Ok(())
            });
        }
        let output = process.output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[ignore = "subprocess entry for the umask-zero directory-birth test"]
    fn observe_directory_birth() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("private");
        let _directory = PrivateDirectory::create(&path).unwrap();
        // Observe before bind or any later chmod/ACL operation can hide a
        // publication window. The real mkdir call must already be private.
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn authenticates_the_kernel_peer_uid_not_the_socket_path() {
        let (socket, _peer) = UnixStream::pair().unwrap();
        let uid = unsafe { libc::geteuid() };
        check_peer(&socket, uid).unwrap();
        assert_eq!(
            check_peer(&socket, uid + 1).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
