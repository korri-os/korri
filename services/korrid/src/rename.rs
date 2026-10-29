//! Atomic path operations on Linux and Android, with identical kernel semantics.
use std::ffi::CStr;

pub(crate) fn renameat2(
    from_dir: libc::c_int,
    from: &CStr,
    to_dir: libc::c_int,
    to: &CStr,
    flags: libc::c_uint,
) -> libc::c_int {
    // The Linux syscall predates glibc's wrapper (2.28) and Android's wrapper
    // (API 30). Keep the same atomic exchange/no-replace kernel operation.
    // SAFETY: CStr supplies live NUL-terminated paths; syscall retains neither.
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
        assert_eq!(fs::read(&right).unwrap(), b"right");
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

    #[test]
    fn competing_no_replace_publications_keep_exactly_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("published");
        let sources = [dir.path().join("first"), dir.path().join("second")];
        for (index, source) in sources.iter().enumerate() {
            fs::write(source, [index as u8]).unwrap();
        }
        let start = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let attempts: Vec<_> = sources
                .iter()
                .map(|source| {
                    let start = &start;
                    let target = &target;
                    scope.spawn(move || {
                        let from = std::ffi::CString::new(source.as_os_str().as_bytes()).unwrap();
                        let to = std::ffi::CString::new(target.as_os_str().as_bytes()).unwrap();
                        start.wait();
                        if renameat2(
                            libc::AT_FDCWD,
                            &from,
                            libc::AT_FDCWD,
                            &to,
                            libc::RENAME_NOREPLACE as _,
                        ) == 0
                        {
                            Ok(())
                        } else {
                            Err(std::io::Error::last_os_error().raw_os_error().unwrap())
                        }
                    })
                })
                .collect();
            attempts
                .into_iter()
                .map(|attempt| attempt.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        for (index, result) in results.iter().enumerate() {
            match result {
                Ok(()) => {
                    assert!(!sources[index].exists());
                    assert_eq!(fs::read(&target).unwrap(), [index as u8]);
                }
                Err(errno) => {
                    assert_eq!(*errno, libc::EEXIST);
                    assert_eq!(fs::read(&sources[index]).unwrap(), [index as u8]);
                }
            }
        }
    }

    #[test]
    fn exchange_with_a_missing_destination_preserves_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("absent");
        fs::write(&source, b"unchanged").unwrap();
        let from = std::ffi::CString::new(source.as_os_str().as_bytes()).unwrap();
        let to = std::ffi::CString::new(target.as_os_str().as_bytes()).unwrap();
        assert_eq!(
            renameat2(
                libc::AT_FDCWD,
                &from,
                libc::AT_FDCWD,
                &to,
                libc::RENAME_EXCHANGE as _,
            ),
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ENOENT)
        );
        assert_eq!(fs::read(source).unwrap(), b"unchanged");
        assert!(!target.exists());
    }
}
