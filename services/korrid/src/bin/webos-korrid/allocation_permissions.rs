//! webOS devmode.conf chmods /media/developer recursively on every boot.
//! Audit the entire allocation before changing modes. Keep descriptors open:
//! permission changes must never follow a replaced name or a symlink.
use std::{
    ffi::{CStr, CString},
    fs::{File, Metadata},
    io,
    mem::MaybeUninit,
    os::{
        fd::{AsRawFd, FromRawFd, IntoRawFd},
        unix::fs::{MetadataExt, PermissionsExt},
    },
};

fn unsafe_entry() -> io::Error {
    io::Error::other(
        "allocation entries must be caller-owned directories or singly-linked regular files; symlinks are refused",
    )
}

fn validate(metadata: &Metadata, uid: u32) -> io::Result<()> {
    if metadata.uid() != uid || !(metadata.is_dir() || metadata.is_file() && metadata.nlink() == 1)
    {
        return Err(unsafe_entry());
    }
    Ok(())
}

struct Directory(*mut libc::DIR);
impl Drop for Directory {
    fn drop(&mut self) {
        // SAFETY: fdopendir created this stream and closedir closes it once.
        unsafe { libc::closedir(self.0) };
    }
}

fn names(directory: &File) -> io::Result<Vec<CString>> {
    let fd = directory.try_clone()?.into_raw_fd();
    // SAFETY: fd is an owned duplicate; the stream owns it on success.
    let pointer = unsafe { libc::fdopendir(fd) };
    if pointer.is_null() {
        let error = io::Error::last_os_error();
        // SAFETY: failed fdopendir did not consume fd.
        unsafe { libc::close(fd) };
        return Err(error);
    }
    let stream = Directory(pointer);
    let mut names = Vec::new();
    loop {
        // SAFETY: errno is thread-local. The stream is live and used only here.
        unsafe { *libc::__errno_location() = 0 };
        let entry = unsafe { libc::readdir64(stream.0) };
        if entry.is_null() {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(0) {
                Ok(names)
            } else {
                Err(error)
            };
        }
        // SAFETY: readdir supplies a nul-terminated name until its next call.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if !matches!(name.to_bytes(), b"." | b"..") {
            names.push(name.to_owned());
        }
    }
}

fn collect(file: File, uid: u32, root: bool, plan: &mut Vec<File>) -> io::Result<()> {
    let metadata = file.metadata()?;
    validate(&metadata, uid)?;
    if metadata.is_dir() {
        for name in names(&file)? {
            let mut expected = MaybeUninit::<libc::stat64>::uninit();
            // SAFETY: name is a single readdir component; expected is output.
            if unsafe {
                libc::fstatat64(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    expected.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful fstatat initialized expected.
            let expected = unsafe { expected.assume_init() };
            let kind = expected.st_mode & libc::S_IFMT;
            if expected.st_uid != uid
                || !(kind == libc::S_IFDIR || kind == libc::S_IFREG && expected.st_nlink == 1)
                || root
                    && matches!(name.to_bytes(), b"private" | b"storage")
                    && kind != libc::S_IFDIR
            {
                return Err(unsafe_entry());
            }
            let flags = libc::O_RDONLY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_NOCTTY
                | if kind == libc::S_IFDIR {
                    libc::O_DIRECTORY
                } else {
                    0
                };
            // SAFETY: parent is open, name is one component, no creation occurs.
            let fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: fd is newly owned and has not been wrapped elsewhere.
            let child = unsafe { File::from_raw_fd(fd) };
            let opened = child.metadata()?;
            if opened.dev() != expected.st_dev
                || opened.ino() != expected.st_ino
                || opened.mode() & libc::S_IFMT != kind
            {
                return Err(unsafe_entry());
            }
            collect(child, uid, false, plan)?;
        }
    }
    plan.push(file);
    Ok(())
}

pub(super) fn tighten(root: &File) -> io::Result<()> {
    // SAFETY: geteuid has no arguments or retained pointers.
    let uid = unsafe { libc::geteuid() };
    if !root.metadata()?.is_dir() {
        return Err(unsafe_entry());
    }
    let mut plan = Vec::new();
    collect(root.try_clone()?, uid, true, &mut plan)?;
    // Recheck the whole audit before making even the first permission change.
    for file in &plan {
        validate(&file.metadata()?, uid)?;
    }
    let mut logged = false;
    // collect is postorder: restrict the allocation root first, then children.
    for file in plan.into_iter().rev() {
        let metadata = file.metadata()?;
        let mode = if metadata.is_dir() { 0o700 } else { 0o600 };
        if metadata.mode() & 0o7777 != mode {
            file.set_permissions(std::fs::Permissions::from_mode(mode))?;
            if !logged {
                eprintln!("webos-korrid: re-tightened allocation permissions after /etc/init/devmode.conf boot chmod");
                logged = true;
            }
            file.sync_all()?;
        }
    }
    Ok(())
}
