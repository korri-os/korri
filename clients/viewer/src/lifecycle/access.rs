// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux filesystem and peer authority for one viewer/controller pair.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{CStr, CString, OsStr},
    fs::File,
    io,
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
};

const ACL: &CStr = c"system.posix_acl_access";

/// The new directory, held by descriptor. Its pathname is resolved once, to
/// open it. A later replacement of that name cannot redirect an operation.
struct PrivateDirectory {
    path: PathBuf,
    pinned: File,
    identity: (u64, u64),
}
impl PrivateDirectory {
    fn create(path: &Path) -> Result<Self> {
        let path = std::path::absolute(path).context("lifecycle socket has no directory")?;
        let parent = path
            .parent()
            .context("lifecycle socket requires a new parent directory")?;
        require_trusted_ancestors(parent)?;
        Self::create_pinned(path)
    }
    fn create_pinned(path: PathBuf) -> Result<Self> {
        // mkdir's mode is atomic, even with umask=0. Do not reuse or chmod a
        // pre-existing directory: it may belong to another launch.
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .context("create private lifecycle directory")?;
        // The ancestors let only root and this UID replace the new name, and
        // O_NOFOLLOW refuses a symlink there. rmdir never follows one either.
        let pinned = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(&path)
            .inspect_err(|_| drop(std::fs::remove_dir(&path)))
            .context("open private lifecycle directory")?;
        let metadata = pinned.metadata()?;
        ensure!(
            metadata.uid() == unsafe { libc::geteuid() },
            "private lifecycle directory was replaced after its creation"
        );
        Ok(Self {
            path,
            pinned,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    /// For system calls that take only a pathname: resolution starts at the
    /// descriptor. A missing /proc makes those calls fail.
    fn pinned_path(&self, name: &OsStr) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.pinned.as_raw_fd())).join(name)
    }
    fn entry(&self, name: &CStr) -> io::Result<libc::stat> {
        // SAFETY: stat is plain data; fstatat fills it before it is read.
        let mut status: libc::stat = unsafe { std::mem::zeroed() };
        succeeded(unsafe {
            libc::fstatat(
                self.pinned.as_raw_fd(),
                name.as_ptr(),
                &mut status,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;
        Ok(status)
    }
    fn is_at_its_path(&self) -> bool {
        std::fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| (metadata.dev(), metadata.ino()) == self.identity)
    }
    fn remove(&self) {
        if self.is_at_its_path() {
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}
impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Another local user who can write an ancestor can replace the private
/// directory by name. Refuse such a location before anything is created.
fn require_trusted_ancestors(parent: &Path) -> Result<()> {
    let viewer = unsafe { libc::geteuid() };
    for ancestor in parent.ancestors() {
        let shown = ancestor.display();
        // lstat: a symlink is not a directory, so no component is followed
        // to a location whose own ancestors this walk would not see.
        let metadata = std::fs::symlink_metadata(ancestor)
            .with_context(|| format!("inspect lifecycle socket ancestor {shown}"))?;
        ensure!(
            metadata.is_dir(),
            "lifecycle socket ancestor {shown} is not a directory"
        );
        let owner = metadata.uid();
        ensure!(
            owner == 0 || owner == viewer,
            "lifecycle socket ancestor {shown} is owned by UID {owner}, not root or the viewer"
        );
        // A root-owned sticky directory, such as /tmp, lets only an entry's
        // owner rename or remove that entry.
        let sticky_root = owner == 0 && metadata.mode() & libc::S_ISVTX != 0;
        ensure!(
            metadata.mode() & 0o022 == 0 || sticky_root,
            "lifecycle socket ancestor {shown} is writable by its group or other users"
        );
    }
    Ok(())
}

pub(super) struct SocketPath {
    name: CString,
    identity: (u64, u64),
    directory: PrivateDirectory,
}
impl SocketPath {
    pub fn bind(path: &Path, controller_uid: u32) -> Result<(UnixListener, Self)> {
        let name = path.file_name().context("lifecycle socket has no name")?;
        let directory =
            PrivateDirectory::create(path.parent().context("lifecycle socket has no parent")?)?;
        Self::bind_in(directory, name, controller_uid)
    }
    fn bind_in(
        directory: PrivateDirectory,
        name: &OsStr,
        controller_uid: u32,
    ) -> Result<(UnixListener, Self)> {
        let viewer_uid = unsafe { libc::geteuid() };
        let descriptor = directory.pinned.as_raw_fd();
        // A name too long for sun_path fails here; it is not shortened.
        let pinned = directory.pinned_path(name);
        let listener = UnixListener::bind(&pinned)
            .context("bind viewer lifecycle socket in its pinned directory")?;
        let name = CString::new(name.as_bytes())?;
        let socket = directory
            .entry(&name)
            .context("viewer lifecycle socket is not in its private directory")?;
        ensure!(
            socket.st_mode & libc::S_IFMT == libc::S_IFSOCK && socket.st_uid == viewer_uid,
            "private lifecycle directory holds an entry the viewer did not bind"
        );
        // From here a failure unlinks the socket: Drop owns this identity.
        let paths = Self {
            name,
            identity: (socket.st_dev, socket.st_ino),
            directory,
        };
        // SAFETY: the descriptor and the name outlive these calls.
        succeeded(unsafe { libc::fchmodat(descriptor, paths.name.as_ptr(), 0o600, 0) })
            .context("restrict viewer lifecycle socket")?;
        if controller_uid != viewer_uid {
            // Grant the socket first, while the directory is still private.
            // Only the final directory ACL makes it reachable by the controller.
            let pinned = CString::new(pinned.as_os_str().as_bytes())?;
            let acl = controller_acl(controller_uid, 0o6, 0o6);
            // SAFETY: the kernel reads these live, bounded buffers only during the call.
            succeeded(unsafe {
                libc::lsetxattr(
                    pinned.as_ptr(),
                    ACL.as_ptr(),
                    acl.as_ptr().cast(),
                    acl.len(),
                    0,
                )
            })
            .context("grant lifecycle controller ACL on the socket")?;
            let acl = controller_acl(controller_uid, 0o7, 0o1);
            // SAFETY: as above.
            succeeded(unsafe {
                libc::fsetxattr(descriptor, ACL.as_ptr(), acl.as_ptr().cast(), acl.len(), 0)
            })
            .context("grant lifecycle controller ACL on the directory")?;
        }
        // The socket is in the pinned directory whatever its name now holds.
        // A controller that follows the name must still arrive there.
        ensure!(
            paths.directory.is_at_its_path(),
            "private lifecycle directory was replaced while the viewer bound its socket"
        );
        Ok((listener, paths))
    }
    pub fn remove(&self) {
        let bound = self
            .directory
            .entry(&self.name)
            .is_ok_and(|entry| (entry.st_dev, entry.st_ino) == self.identity);
        if bound {
            // SAFETY: the descriptor and the name are live for the call.
            unsafe { libc::unlinkat(self.directory.pinned.as_raw_fd(), self.name.as_ptr(), 0) };
        }
        self.directory.remove();
    }
}
impl Drop for SocketPath {
    fn drop(&mut self) {
        self.remove();
    }
}

fn succeeded(result: libc::c_int) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn controller_acl(uid: u32, owner: u16, controller: u16) -> Vec<u8> {
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
    acl
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
        // umask=0 also published this test's own parent, which the ancestor
        // check refuses. This test observes only the new directory's mode.
        let _directory = PrivateDirectory::create_pinned(path.clone()).unwrap();
        // Observe before bind or any later chmod/ACL operation can hide a
        // publication window. The real mkdir call must already be private.
        assert_eq!(std::fs::metadata(path).unwrap().mode() & 0o777, 0o700);
    }

    // The reviewed race (#435): another actor replaces the new directory with
    // a symlink while bind() is delayed. The test is that actor, between the
    // two steps, so no timing is involved. The ancestor check already keeps
    // another UID out; this proves the descriptor alone, against any actor.
    #[test]
    fn directory_replaced_before_a_delayed_bind_is_refused_with_no_socket_outside() {
        let root = tempfile::tempdir().unwrap();
        let private = root.path().join("control");
        let moved = root.path().join("moved");
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let directory = PrivateDirectory::create_pinned(private.clone()).unwrap();
        std::fs::rename(&private, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &private).unwrap();

        let result = SocketPath::bind_in(directory, OsStr::new("viewer.sock"), unsafe {
            libc::geteuid()
        });

        assert!(
            std::fs::read_dir(&outside).unwrap().next().is_none(),
            "the socket must not be created through the replaced path"
        );
        assert!(
            result.is_err(),
            "a viewer whose directory left its path must refuse to serve"
        );
        assert!(
            std::fs::read_dir(&moved).unwrap().next().is_none(),
            "the refused viewer must remove the socket from its own directory"
        );
        assert!(std::fs::symlink_metadata(&private).unwrap().is_symlink());
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
