//! Atomic path operations on Linux and Android, with identical kernel semantics.
use std::ffi::CStr;

pub(crate) fn renameat2(
    from_dir: libc::c_int,
    from: &CStr,
    to_dir: libc::c_int,
    to: &CStr,
    flags: libc::c_uint,
) -> libc::c_int {
    // Android exposes the libc wrapper only from API 30. Use the same kernel
    // operation on API 21+, without weakening exchange/no-replace guarantees.
    // SAFETY: CStr supplies live NUL-terminated paths; neither call retains them.
    #[cfg(target_os = "android")]
    unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            from_dir,
            from.as_ptr(),
            to_dir,
            to.as_ptr(),
            flags,
        ) as libc::c_int
    }
    #[cfg(not(target_os = "android"))]
    unsafe {
        libc::renameat2(from_dir, from.as_ptr(), to_dir, to.as_ptr(), flags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::ffi::OsStrExt};

    #[test]
    fn exchange_and_no_replace_keep_the_atomic_contract() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left");
        let right = dir.path().join("right");
        fs::write(&left, b"left").unwrap();
        fs::write(&right, b"right").unwrap();
        let a = std::ffi::CString::new(left.as_os_str().as_bytes()).unwrap();
        let b = std::ffi::CString::new(right.as_os_str().as_bytes()).unwrap();
        assert_eq!(
            renameat2(
                libc::AT_FDCWD,
                &a,
                libc::AT_FDCWD,
                &b,
                libc::RENAME_NOREPLACE as _
            ),
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EEXIST)
        );
        assert_eq!(fs::read(&left).unwrap(), b"left");
        assert_eq!(
            renameat2(
                libc::AT_FDCWD,
                &a,
                libc::AT_FDCWD,
                &b,
                libc::RENAME_EXCHANGE as _
            ),
            0
        );
        assert_eq!(fs::read(&left).unwrap(), b"right");
        assert_eq!(fs::read(&right).unwrap(), b"left");
    }
}
