// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux filesystem and peer authority for one viewer/controller pair.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{CStr, CString, OsStr},
    fs::{File, Metadata},
    io,
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{
            ffi::OsStrExt,
            fs::MetadataExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
};

const ACL: &CStr = c"system.posix_acl_access";

/// One directory, by its name in an open directory. A symlink is refused.
fn open_directory(parent: RawFd, name: &CStr, access: libc::c_int) -> io::Result<File> {
    let flags = access | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: the name is a live C string. The new descriptor has no other owner.
    let descriptor = unsafe { libc::openat(parent, name.as_ptr(), flags) };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

/// Reach `path` from `/`, one component at a time. Each directory is judged
/// through its own descriptor before the next name is looked up in it. A name
/// that changes later changes neither what was judged nor what is used.
fn open_from_root(path: &Path, admit: impl Fn(&Metadata, &Path) -> Result<()>) -> Result<File> {
    ensure!(path.is_absolute(), "lifecycle socket has no directory");
    let mut reached = PathBuf::new();
    let mut directory = None;
    for component in path.components() {
        reached.push(component);
        let name = CString::new(component.as_os_str().as_bytes())?;
        let parent = directory
            .as_ref()
            .map_or(libc::AT_FDCWD, AsRawFd::as_raw_fd);
        // O_PATH: the viewer needs search access to an ancestor, not read access.
        let opened = open_directory(parent, &name, libc::O_PATH).with_context(|| {
            format!(
                "lifecycle socket ancestor {} is not a directory the viewer can open",
                reached.display()
            )
        })?;
        admit(&opened.metadata()?, &reached)?;
        directory = Some(opened);
    }
    directory.context("lifecycle socket requires a new parent directory")
}

/// Another local user who can write an ancestor can replace a name in it.
/// Only root and the viewer may be able to. A sticky directory is no
/// exception: another user can own a name there and replace that one.
fn require_trusted(ancestor: &Metadata, shown: &Path) -> Result<()> {
    let shown = shown.display();
    let owner = ancestor.uid();
    ensure!(
        owner == 0 || owner == unsafe { libc::geteuid() },
        "lifecycle socket ancestor {shown} is owned by UID {owner}, not root or the viewer"
    );
    ensure!(
        ancestor.mode() & 0o022 == 0,
        "lifecycle socket ancestor {shown} is writable by its group or other users"
    );
    Ok(())
}

fn entry(directory: &File, name: &CStr) -> io::Result<libc::stat> {
    // SAFETY: stat is plain data; fstatat fills it before it is read.
    let mut status: libc::stat = unsafe { std::mem::zeroed() };
    succeeded(unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            &mut status,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    })?;
    Ok(status)
}

/// The new directory and its trusted parent, both held by descriptor. No
/// pathname is resolved after the walk from `/`.
struct PrivateDirectory {
    parent: File,
    name: CString,
    pinned: File,
    identity: (u64, u64),
}
impl PrivateDirectory {
    fn create(path: &Path) -> Result<Self> {
        let path = std::path::absolute(path).context("lifecycle socket has no directory")?;
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            anyhow::bail!("lifecycle socket requires a new parent directory");
        };
        Self::create_in(open_from_root(parent, require_trusted)?, name)
    }
    fn create_in(parent: File, name: &OsStr) -> Result<Self> {
        let name = CString::new(name.as_bytes())?;
        // mkdir's mode is atomic, even with umask=0. Do not reuse or chmod a
        // pre-existing directory: it may belong to another launch.
        // SAFETY: the descriptor and the name are live for these calls.
        succeeded(unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) })
            .context("create private lifecycle directory")?;
        let pinned = open_directory(parent.as_raw_fd(), &name, libc::O_RDONLY)
            .inspect_err(|_| unsafe {
                libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR);
            })
            .context("open private lifecycle directory")?;
        let metadata = pinned.metadata()?;
        ensure!(
            metadata.uid() == unsafe { libc::geteuid() },
            "private lifecycle directory was replaced after its creation"
        );
        Ok(Self {
            parent,
            name,
            pinned,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    /// For system calls that take only a pathname: resolution starts at the
    /// descriptor. A missing /proc makes those calls fail.
    fn pinned_path(&self, name: &OsStr) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.pinned.as_raw_fd())).join(name)
    }
    /// Its name in the parent can hold another launch's directory by now.
    fn is_in_its_parent(&self) -> bool {
        entry(&self.parent, &self.name)
            .is_ok_and(|entry| (entry.st_dev, entry.st_ino) == self.identity)
    }
    fn remove(&self) {
        if self.is_in_its_parent() {
            // SAFETY: the descriptor and the name are live for the call.
            unsafe {
                libc::unlinkat(
                    self.parent.as_raw_fd(),
                    self.name.as_ptr(),
                    libc::AT_REMOVEDIR,
                )
            };
        }
    }
}
impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        self.remove();
    }
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
        let socket = entry(&directory.pinned, &name)
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
        // Only root or this UID can take the directory out of its parent, so
        // this is no defence. It tells a launcher at once that no controller
        // can reach the socket, instead of after the Start deadline.
        ensure!(
            paths.directory.is_in_its_parent(),
            "private lifecycle directory left its parent while the viewer bound its socket"
        );
        Ok((listener, paths))
    }
    pub fn remove(&self) {
        let bound = entry(&self.directory.pinned, &self.name)
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
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
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
        // umask=0 also published this test's own parent, which the viewer
        // refuses. This test observes only the new directory's mode.
        let _directory =
            PrivateDirectory::create_in(open_unjudged(root.path()), OsStr::new("private")).unwrap();
        // Observe before bind or any later chmod/ACL operation can hide a
        // publication window. The real mkdir call must already be private.
        assert_eq!(std::fs::metadata(path).unwrap().mode() & 0o777, 0o700);
    }

    // The reviewed race (#435): another actor replaces the new directory with
    // a symlink while bind() is delayed. The test is that actor, between the
    // two steps, so no timing is involved. The ancestor rule already keeps
    // another UID out; this proves the descriptor alone, against any actor.
    #[test]
    fn directory_replaced_before_a_delayed_bind_is_refused_with_no_socket_outside() {
        let root = tempfile::tempdir().unwrap();
        let private = root.path().join("control");
        let moved = root.path().join("moved");
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let directory =
            PrivateDirectory::create_in(open_unjudged(root.path()), OsStr::new("control")).unwrap();
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
            "a viewer whose directory left its parent must refuse to serve"
        );
        assert!(
            std::fs::read_dir(&moved).unwrap().next().is_none(),
            "the refused viewer must remove the socket from its own directory"
        );
        assert!(std::fs::symlink_metadata(&private).unwrap().is_symlink());
    }

    // The third review's race (#435): an ancestor is replaced with a symlink
    // after the walk judged it and before the directory is created. The test
    // is that actor, between the two steps, so no timing is involved.
    #[test]
    fn ancestor_replaced_after_its_check_cannot_move_the_directory_or_its_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let launcher = root.path().join("launcher");
        let held = root.path().join("held");
        let attacker = root.path().join("attacker");
        std::fs::create_dir(&launcher).unwrap();
        std::fs::create_dir(&attacker).unwrap();
        let parent = open_unjudged(&launcher);
        std::fs::rename(&launcher, &held).unwrap();
        std::os::unix::fs::symlink(&attacker, &launcher).unwrap();

        let bound =
            PrivateDirectory::create_in(parent, OsStr::new("control")).and_then(|directory| {
                SocketPath::bind_in(directory, OsStr::new("viewer.sock"), unsafe {
                    libc::geteuid()
                })
            });

        assert!(
            std::fs::read_dir(&attacker).unwrap().next().is_none(),
            "nothing may be created through the replaced ancestor"
        );
        assert!(
            std::fs::symlink_metadata(held.join("control/viewer.sock"))
                .unwrap()
                .file_type()
                .is_socket(),
            "the socket belongs in the parent that the walk opened"
        );
        drop(bound);
        assert!(std::fs::read_dir(&attacker).unwrap().next().is_none());
        assert!(
            std::fs::read_dir(&held).unwrap().next().is_none(),
            "cleanup must remove the directory from the parent that was judged"
        );
    }

    /// The descriptor walk alone. Who may own an ancestor is a separate rule,
    /// and the Nix build sandbox has no location that passes it.
    fn open_unjudged(path: &Path) -> File {
        open_from_root(path, |_, _| Ok(())).unwrap()
    }

    #[test]
    fn walk_refuses_a_symlink_and_judges_every_directory_it_opens() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, root.path().join("link")).unwrap();
        let through_link = open_from_root(&root.path().join("link"), |_, _| Ok(()));
        assert!(format!("{:#}", through_link.unwrap_err()).contains("link is not a directory"));

        let judged = std::cell::RefCell::new(Vec::new());
        open_from_root(&real, |_, reached| {
            judged.borrow_mut().push(reached.to_owned());
            Ok(())
        })
        .unwrap();
        let mut expected: Vec<_> = real.ancestors().map(Path::to_owned).collect();
        expected.reverse();
        assert_eq!(judged.into_inner(), expected);
        assert!(open_from_root(Path::new("real"), |_, _| Ok(())).is_err());
    }

    #[test]
    fn only_root_and_the_viewer_may_be_able_to_write_an_ancestor() {
        let root = tempfile::tempdir().unwrap();
        for (mode, trusted) in [
            (0o700, true),
            (0o755, true),
            (0o770, false),
            (0o707, false),
            // No sticky exception: /tmp is 1777.
            (0o1777, false),
        ] {
            let path = root.path().join(format!("{mode:o}"));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            let metadata = std::fs::metadata(&path).unwrap();
            assert_eq!(
                require_trusted(&metadata, &path).is_ok(),
                trusted,
                "{mode:o}"
            );
        }
        // The Nix build sandbox shows `/` with an unmapped owner.
        let top = std::fs::metadata("/").unwrap();
        let foreign = top.uid() != 0 && top.uid() != unsafe { libc::geteuid() };
        assert_eq!(require_trusted(&top, Path::new("/")).is_err(), foreign);
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
