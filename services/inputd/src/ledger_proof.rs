use std::env;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const GENERIC_ERROR: &str = "ledger proof is not safely readable or writable";
const MAX_PROOF_BYTES: usize = 1024 * 1024;
const BASELINE: &[u8] = b"baseline.predicates";
const ACCEPTED: &[u8] = b"sunshine-private-state.accepted";
const STATE: &[u8] = b"state";
const CANDIDATE_AUTOMATED: &[u8] = b"candidate-automated.txt";
const PERSISTENT_AUTOMATED: &[u8] = b"persistent-automated.txt";
const CANDIDATE_REBOOT: &[u8] = b"candidate-reboot.txt";
const RECONCILE_CANDIDATE_REBOOT: &[u8] = b"reconcile-candidate-reboot.txt";
const FINGERPRINT_EXPECTED: &[u8] = b"fingerprint.expected";
const FINGERPRINT_CURRENT: &[u8] = b"fingerprint.current";
const CONTROLLER_ACCEPTED: &[u8] = b"candidate-controller.accepted";
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RENAME_NOREPLACE: u32 = 1;

#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    nlink: u64,
    size: i64,
    mtime_sec: i64,
    mtime_nsec: i64,
    ctime_sec: i64,
    ctime_nsec: i64,
}

impl Snapshot {
    fn from_fd(fd: RawFd) -> io::Result<Self> {
        let mut raw = MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(fd, raw.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let raw = unsafe { raw.assume_init() };
        Ok(Self {
            dev: raw.st_dev,
            ino: raw.st_ino,
            mode: raw.st_mode,
            uid: raw.st_uid,
            nlink: raw.st_nlink,
            size: raw.st_size,
            mtime_sec: raw.st_mtime,
            mtime_nsec: raw.st_mtime_nsec,
            ctime_sec: raw.st_ctime,
            ctime_nsec: raw.st_ctime_nsec,
        })
    }

    fn kind(&self) -> u32 {
        self.mode & libc::S_IFMT
    }

    fn identity(&self) -> String {
        format!("{}:{}", self.dev, self.ino)
    }
}

fn c_string(bytes: &[u8]) -> io::Result<CString> {
    CString::new(bytes).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))
}

fn open_absolute_no_symlinks(path: &Path, flags: i32) -> io::Result<OwnedFd> {
    let path = c_string(path.as_os_str().as_bytes())?;
    let how = OpenHow {
        flags: (flags | libc::O_CLOEXEC) as u64,
        mode: 0,
        resolve: RESOLVE_NO_SYMLINKS,
    };
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            libc::AT_FDCWD,
            path.as_ptr(),
            &how,
            std::mem::size_of::<OpenHow>(),
        ) as i32
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

fn open_at(parent: RawFd, name: &[u8], flags: i32, mode: u32) -> io::Result<OwnedFd> {
    let name = c_string(name)?;
    let fd = unsafe { libc::openat(parent, name.as_ptr(), flags | libc::O_CLOEXEC, mode) };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

fn proof_name(value: &str) -> io::Result<&'static [u8]> {
    match value.as_bytes() {
        BASELINE => Ok(BASELINE),
        ACCEPTED => Ok(ACCEPTED),
        STATE => Ok(STATE),
        CANDIDATE_AUTOMATED => Ok(CANDIDATE_AUTOMATED),
        PERSISTENT_AUTOMATED => Ok(PERSISTENT_AUTOMATED),
        CANDIDATE_REBOOT => Ok(CANDIDATE_REBOOT),
        RECONCILE_CANDIDATE_REBOOT => Ok(RECONCILE_CANDIDATE_REBOOT),
        FINGERPRINT_EXPECTED => Ok(FINGERPRINT_EXPECTED),
        FINGERPRINT_CURRENT => Ok(FINGERPRINT_CURRENT),
        CONTROLLER_ACCEPTED => Ok(CONTROLLER_ACCEPTED),
        _ => Err(io::Error::from_raw_os_error(libc::EINVAL)),
    }
}

fn parse_identity(value: &str) -> io::Result<(u64, u64)> {
    let (dev, ino) = value
        .split_once(':')
        .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
    Ok((
        dev.parse()
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?,
        ino.parse()
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?,
    ))
}

fn open_ledger(path: &Path, expected: Option<(u64, u64)>) -> io::Result<(OwnedFd, Snapshot)> {
    let fd =
        open_absolute_no_symlinks(path, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW)?;
    let snapshot = Snapshot::from_fd(fd.as_raw_fd())?;
    if snapshot.kind() != libc::S_IFDIR
        || snapshot.uid != unsafe { libc::geteuid() }
        || snapshot.mode & 0o777 != 0o700
        || expected.is_some_and(|identity| identity != (snapshot.dev, snapshot.ino))
    {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    Ok((fd, snapshot))
}

fn validate_proof(snapshot: Snapshot) -> io::Result<()> {
    if snapshot.kind() != libc::S_IFREG
        || snapshot.uid != unsafe { libc::geteuid() }
        || snapshot.mode & 0o777 != 0o600
        || snapshot.nlink != 1
        || snapshot.size <= 0
        || snapshot.size as usize > MAX_PROOF_BYTES
    {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    Ok(())
}

fn same_entry_identity(left: Snapshot, right: Snapshot) -> bool {
    left.dev == right.dev
        && left.ino == right.ino
        && left.kind() == right.kind()
        && left.uid == right.uid
        && left.mode & 0o777 == right.mode & 0o777
}

fn same_proof_identity(left: Snapshot, right: Snapshot) -> bool {
    same_entry_identity(left, right) && left.nlink == right.nlink && left.size == right.size
}

fn rebind_ledger(path: &Path, expected: Snapshot) -> io::Result<()> {
    let rebound =
        open_absolute_no_symlinks(path, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW)?;
    if !same_entry_identity(Snapshot::from_fd(rebound.as_raw_fd())?, expected) {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    Ok(())
}

fn read_from_open_ledger(
    ledger_fd: RawFd,
    ledger_snapshot: Snapshot,
    name: &[u8],
) -> io::Result<(Vec<u8>, Snapshot)> {
    let fd = open_at(
        ledger_fd,
        name,
        libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW,
        0,
    )?;
    let snapshot = Snapshot::from_fd(fd.as_raw_fd())?;
    validate_proof(snapshot)?;
    let mut file = File::from(fd);
    let mut content = Vec::with_capacity(snapshot.size as usize);
    (&mut file)
        .take((MAX_PROOF_BYTES + 1) as u64)
        .read_to_end(&mut content)?;
    let fd: OwnedFd = file.into();
    if content.is_empty()
        || content.len() > MAX_PROOF_BYTES
        || Snapshot::from_fd(fd.as_raw_fd())? != snapshot
        || !same_entry_identity(Snapshot::from_fd(ledger_fd)?, ledger_snapshot)
    {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    let rebound = open_at(ledger_fd, name, libc::O_PATH | libc::O_NOFOLLOW, 0)?;
    if Snapshot::from_fd(rebound.as_raw_fd())? != snapshot {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    Ok((content, snapshot))
}

fn read_proof_with_hook<F>(
    ledger: &Path,
    expected: (u64, u64),
    name: &[u8],
    hook: F,
) -> io::Result<Vec<u8>>
where
    F: FnOnce(),
{
    let (ledger_fd, ledger_snapshot) = open_ledger(ledger, Some(expected))?;
    let (content, proof_snapshot) =
        read_from_open_ledger(ledger_fd.as_raw_fd(), ledger_snapshot, name)?;
    hook();
    let rebound = open_at(
        ledger_fd.as_raw_fd(),
        name,
        libc::O_PATH | libc::O_NOFOLLOW,
        0,
    )?;
    if Snapshot::from_fd(rebound.as_raw_fd())? != proof_snapshot
        || !same_entry_identity(Snapshot::from_fd(ledger_fd.as_raw_fd())?, ledger_snapshot)
    {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    rebind_ledger(ledger, ledger_snapshot)?;
    Ok(content)
}

fn read_proof(ledger: &Path, expected: (u64, u64), name: &[u8]) -> io::Result<Vec<u8>> {
    read_proof_with_hook(ledger, expected, name, || {})
}

fn random_suffix() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    let count =
        unsafe { libc::getrandom(bytes.as_mut_ptr().cast::<libc::c_void>(), bytes.len(), 0) };
    if count != bytes.len() as isize {
        return Err(io::Error::last_os_error());
    }
    Ok(bytes.iter().map(|value| format!("{value:02x}")).collect())
}

struct TempEntry {
    directory: RawFd,
    name: CString,
    active: bool,
}

impl Drop for TempEntry {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                libc::unlinkat(self.directory, self.name.as_ptr(), 0);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum WritePhase {
    BeforeRename,
    AfterRenameBeforeFsync,
    BeforeFinalRebind,
}

fn write_proof_with_hook<F>(
    ledger: &Path,
    expected: (u64, u64),
    name: &[u8],
    content: &[u8],
    replace: bool,
    mut hook: F,
) -> io::Result<()>
where
    F: FnMut(WritePhase) -> io::Result<()>,
{
    if content.is_empty() || content.len() > MAX_PROOF_BYTES {
        return Err(io::Error::from_raw_os_error(libc::EINVAL));
    }
    let (ledger_fd, ledger_snapshot) = open_ledger(ledger, Some(expected))?;
    let suffix = random_suffix()?;
    let mut temp_name = b".korri-proof.next.".to_vec();
    temp_name.extend_from_slice(suffix.as_bytes());
    let temp_c = c_string(&temp_name)?;
    let temp_fd = open_at(
        ledger_fd.as_raw_fd(),
        &temp_name,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
        0o600,
    )?;
    let mut guard = TempEntry {
        directory: ledger_fd.as_raw_fd(),
        name: temp_c,
        active: true,
    };
    let snapshot = Snapshot::from_fd(temp_fd.as_raw_fd())?;
    if snapshot.kind() != libc::S_IFREG
        || snapshot.uid != unsafe { libc::geteuid() }
        || snapshot.mode & 0o777 != 0o600
        || snapshot.nlink != 1
    {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    let mut file = File::from(temp_fd);
    file.write_all(content)?;
    file.sync_all()?;
    let temp_fd: OwnedFd = file.into();
    let written = Snapshot::from_fd(temp_fd.as_raw_fd())?;
    validate_proof(written)?;
    if written.size as usize != content.len()
        || !same_entry_identity(Snapshot::from_fd(ledger_fd.as_raw_fd())?, ledger_snapshot)
    {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    rebind_ledger(ledger, ledger_snapshot)?;
    hook(WritePhase::BeforeRename)?;
    if !same_entry_identity(Snapshot::from_fd(ledger_fd.as_raw_fd())?, ledger_snapshot) {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    rebind_ledger(ledger, ledger_snapshot)?;

    let target = c_string(name)?;
    let flags = if replace { 0 } else { RENAME_NOREPLACE };
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            ledger_fd.as_raw_fd(),
            guard.name.as_ptr(),
            ledger_fd.as_raw_fd(),
            target.as_ptr(),
            flags,
        ) as i32
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        if !replace && error.raw_os_error() == Some(libc::EEXIST) {
            let (existing, _) =
                read_from_open_ledger(ledger_fd.as_raw_fd(), ledger_snapshot, name)?;
            if existing == content {
                if unsafe { libc::fsync(ledger_fd.as_raw_fd()) } != 0 {
                    return Err(io::Error::last_os_error());
                }
                rebind_ledger(ledger, ledger_snapshot)?;
                return Ok(());
            }
        }
        return Err(error);
    }
    guard.active = false;

    let rebound = open_at(
        ledger_fd.as_raw_fd(),
        name,
        libc::O_PATH | libc::O_NOFOLLOW,
        0,
    )?;
    if !same_proof_identity(Snapshot::from_fd(rebound.as_raw_fd())?, written) {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    hook(WritePhase::AfterRenameBeforeFsync)?;
    if !same_entry_identity(Snapshot::from_fd(ledger_fd.as_raw_fd())?, ledger_snapshot) {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    rebind_ledger(ledger, ledger_snapshot)?;
    if unsafe { libc::fsync(ledger_fd.as_raw_fd()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    hook(WritePhase::BeforeFinalRebind)?;
    if !same_entry_identity(Snapshot::from_fd(ledger_fd.as_raw_fd())?, ledger_snapshot) {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    rebind_ledger(ledger, ledger_snapshot)?;
    Ok(())
}

fn write_new_proof(
    ledger: &Path,
    expected: (u64, u64),
    name: &[u8],
    content: &[u8],
) -> io::Result<()> {
    write_proof_with_hook(ledger, expected, name, content, false, |_| Ok(()))
}

fn write_replace_proof(
    ledger: &Path,
    expected: (u64, u64),
    name: &[u8],
    content: &[u8],
) -> io::Result<()> {
    write_proof_with_hook(ledger, expected, name, content, true, |_| Ok(()))
}

fn read_stdin() -> io::Result<Vec<u8>> {
    let mut content = Vec::new();
    io::stdin()
        .take((MAX_PROOF_BYTES + 1) as u64)
        .read_to_end(&mut content)?;
    Ok(content)
}

fn run() -> io::Result<()> {
    let mut args = env::args_os();
    let _program = args.next();
    let command = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
    let ledger = PathBuf::from(
        args.next()
            .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?,
    );
    if command == "identity" {
        if args.next().is_some() {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        let (_, snapshot) = open_ledger(&ledger, None)?;
        println!("{}", snapshot.identity());
        return Ok(());
    }

    let expected = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
    let expected = parse_identity(&expected)?;
    let name = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))?;
    let name = proof_name(&name)?;
    if args.next().is_some() {
        return Err(io::Error::from_raw_os_error(libc::EINVAL));
    }
    match command.as_str() {
        "read" => io::stdout().write_all(&read_proof(&ledger, expected, name)?)?,
        "read-optional" => match read_proof(&ledger, expected, name) {
            Ok(content) => io::stdout().write_all(&content)?,
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => {}
            Err(error) => return Err(error),
        },
        "write-new" => write_new_proof(&ledger, expected, name, &read_stdin()?)?,
        "write-replace" => write_replace_proof(&ledger, expected, name, &read_stdin()?)?,
        _ => return Err(io::Error::from_raw_os_error(libc::EINVAL)),
    }
    Ok(())
}

fn main() {
    if run().is_err() {
        eprintln!("{GENERIC_ERROR}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tempfile::TempDir;

    fn fixture() -> (TempDir, PathBuf, (u64, u64)) {
        let temp = TempDir::new().unwrap();
        let ledger = temp.path().join("ledger");
        fs::create_dir(&ledger).unwrap();
        fs::set_permissions(&ledger, fs::Permissions::from_mode(0o700)).unwrap();
        let (_, snapshot) = open_ledger(&ledger, None).unwrap();
        (temp, ledger, (snapshot.dev, snapshot.ino))
    }

    fn replace_ledger(ledger: &Path) -> PathBuf {
        let old = ledger.with_extension("old");
        fs::rename(ledger, &old).unwrap();
        fs::create_dir(ledger).unwrap();
        fs::set_permissions(ledger, fs::Permissions::from_mode(0o700)).unwrap();
        old
    }

    #[test]
    fn writes_reads_and_idempotently_reuses_exact_proof() {
        let (_temp, ledger, identity) = fixture();
        write_new_proof(&ledger, identity, BASELINE, b"proof\n").unwrap();
        assert_eq!(read_proof(&ledger, identity, BASELINE).unwrap(), b"proof\n");
        write_new_proof(&ledger, identity, BASELINE, b"proof\n").unwrap();
        assert!(write_new_proof(&ledger, identity, BASELINE, b"other\n").is_err());
        write_replace_proof(&ledger, identity, STATE, b"state=next\n").unwrap();
        assert_eq!(
            read_proof(&ledger, identity, STATE).unwrap(),
            b"state=next\n"
        );
    }

    #[test]
    fn rejects_wrong_session_identity_and_directory_replacement() {
        let (_temp, ledger, identity) = fixture();
        write_new_proof(&ledger, identity, BASELINE, b"proof\n").unwrap();
        assert!(read_proof(&ledger, (identity.0, identity.1 + 1), BASELINE).is_err());
        replace_ledger(&ledger);
        assert!(read_proof(&ledger, identity, BASELINE).is_err());
        assert!(write_new_proof(&ledger, identity, ACCEPTED, b"digest\n").is_err());
    }

    #[test]
    fn never_follows_next_or_final_symlinks() {
        let (temp, ledger, identity) = fixture();
        let outside = temp.path().join("outside");
        fs::write(&outside, b"outside\n").unwrap();
        symlink(&outside, ledger.join("baseline.predicates.next")).unwrap();
        write_new_proof(&ledger, identity, BASELINE, b"proof\n").unwrap();
        assert_eq!(fs::read(&outside).unwrap(), b"outside\n");

        let (temp, ledger, identity) = fixture();
        let outside = temp.path().join("outside-final");
        fs::write(&outside, b"outside\n").unwrap();
        symlink(&outside, ledger.join("sunshine-private-state.accepted")).unwrap();
        assert!(write_new_proof(&ledger, identity, ACCEPTED, b"digest\n").is_err());
        assert!(read_proof(&ledger, identity, ACCEPTED).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"outside\n");
    }

    #[test]
    fn rejects_hard_links_and_wrong_modes() {
        for name in [BASELINE, ACCEPTED, STATE] {
            let (temp, ledger, identity) = fixture();
            let outside = temp.path().join("outside");
            fs::write(&outside, b"proof\n").unwrap();
            fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
            fs::hard_link(&outside, ledger.join(std::str::from_utf8(name).unwrap())).unwrap();
            assert!(read_proof(&ledger, identity, name).is_err());

            let (_temp, ledger, identity) = fixture();
            let path = ledger.join(std::str::from_utf8(name).unwrap());
            fs::write(&path, b"proof\n").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            assert!(read_proof(&ledger, identity, name).is_err());
        }
    }

    #[test]
    fn detects_replacement_before_and_after_rename_without_outside_overwrite() {
        for phase in [
            WritePhase::BeforeRename,
            WritePhase::AfterRenameBeforeFsync,
            WritePhase::BeforeFinalRebind,
        ] {
            let (temp, ledger, identity) = fixture();
            let outside = temp.path().join("outside");
            fs::write(&outside, b"outside\n").unwrap();
            let wanted = Cell::new(true);
            let result =
                write_proof_with_hook(&ledger, identity, ACCEPTED, b"digest\n", false, |current| {
                    if std::mem::discriminant(&current) == std::mem::discriminant(&phase)
                        && wanted.replace(false)
                    {
                        replace_ledger(&ledger);
                        symlink(&outside, ledger.join("sunshine-private-state.accepted")).unwrap();
                    }
                    Ok(())
                });
            assert!(result.is_err());
            assert_eq!(fs::read(&outside).unwrap(), b"outside\n");
        }
    }

    #[test]
    fn post_rename_failure_is_resumable_with_exact_content() {
        let (_temp, ledger, identity) = fixture();
        let failed =
            write_proof_with_hook(&ledger, identity, ACCEPTED, b"digest\n", false, |phase| {
                if matches!(phase, WritePhase::AfterRenameBeforeFsync) {
                    return Err(io::Error::from_raw_os_error(libc::EIO));
                }
                Ok(())
            });
        assert!(failed.is_err());
        write_new_proof(&ledger, identity, ACCEPTED, b"digest\n").unwrap();
        assert!(write_new_proof(&ledger, identity, ACCEPTED, b"other\n").is_err());
    }

    #[test]
    fn rejects_entry_replacement_after_read() {
        for name in [BASELINE, ACCEPTED, STATE] {
            let (_temp, ledger, identity) = fixture();
            write_new_proof(&ledger, identity, name, b"proof\n").unwrap();
            let target = ledger.join(std::str::from_utf8(name).unwrap());
            assert!(read_proof_with_hook(&ledger, identity, name, || {
                fs::rename(&target, target.with_extension("old")).unwrap();
                fs::write(&target, b"replacement\n").unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
            })
            .is_err());
        }
    }
}
