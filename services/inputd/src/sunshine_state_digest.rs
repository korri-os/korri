use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::env;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, Read};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const GENERIC_ERROR: &str = "sunshine private state is not safely readable";
const REQUIRED_CONFIG: &[u8] = b"sunshine.conf";
const REQUIRED_STATE: &[u8] = b"sunshine_state.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
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
            gid: raw.st_gid,
            nlink: raw.st_nlink.into(),
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

    fn safe_owner_mode(&self, expected_uid: u32) -> bool {
        self.uid == expected_uid && self.mode & 0o022 == 0
    }
}

enum NodeKind {
    Directory(Vec<Node>),
    File([u8; 32]),
}

struct Node {
    name: Vec<u8>,
    fd: OwnedFd,
    snapshot: Snapshot,
    kind: NodeKind,
}

fn c_name(bytes: &[u8]) -> io::Result<CString> {
    CString::new(bytes).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))
}

fn open_at(parent: RawFd, name: &[u8], flags: i32) -> io::Result<OwnedFd> {
    let name = c_name(name)?;
    let fd = unsafe { libc::openat(parent, name.as_ptr(), flags | libc::O_CLOEXEC, 0) };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

const RESOLVE_NO_SYMLINKS: u64 = 0x04;

fn open_absolute_no_symlinks(path: &Path, flags: i32) -> io::Result<OwnedFd> {
    let path = c_name(path.as_os_str().as_bytes())?;
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

fn list_names(fd: RawFd) -> io::Result<Vec<Vec<u8>>> {
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error());
    }
    let directory = unsafe { libc::fdopendir(duplicate) };
    if directory.is_null() {
        unsafe { libc::close(duplicate) };
        return Err(io::Error::last_os_error());
    }
    unsafe { libc::rewinddir(directory) };
    let mut names = Vec::new();
    loop {
        unsafe { *libc::__errno_location() = 0 };
        let entry = unsafe { libc::readdir(directory) };
        if entry.is_null() {
            let error = unsafe { *libc::__errno_location() };
            unsafe { libc::closedir(directory) };
            if error != 0 {
                return Err(io::Error::from_raw_os_error(error));
            }
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        names.push(name.to_vec());
    }
    names.sort_by(|left, right| left.as_slice().cmp(right.as_slice()));
    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(io::Error::from_raw_os_error(libc::EIO));
    }
    Ok(names)
}

fn validate_directory(fd: RawFd, expected_uid: u32) -> io::Result<Snapshot> {
    let snapshot = Snapshot::from_fd(fd)?;
    if snapshot.kind() != libc::S_IFDIR || !snapshot.safe_owner_mode(expected_uid) {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    Ok(snapshot)
}

fn read_regular(fd: OwnedFd, expected: Snapshot) -> io::Result<(OwnedFd, [u8; 32])> {
    let mut file = File::from(fd);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let fd: OwnedFd = file.into();
    if Snapshot::from_fd(fd.as_raw_fd())? != expected {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    Ok((fd, hasher.finalize().into()))
}

fn load_node(parent: RawFd, name: Vec<u8>, expected_uid: u32) -> io::Result<Node> {
    let probe = open_at(parent, &name, libc::O_PATH | libc::O_NOFOLLOW)?;
    let probe_snapshot = Snapshot::from_fd(probe.as_raw_fd())?;
    if !probe_snapshot.safe_owner_mode(expected_uid) {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    match probe_snapshot.kind() {
        libc::S_IFDIR => {
            let fd = open_at(
                parent,
                &name,
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )?;
            let snapshot = validate_directory(fd.as_raw_fd(), expected_uid)?;
            if snapshot != probe_snapshot {
                return Err(io::Error::from_raw_os_error(libc::ESTALE));
            }
            let names = list_names(fd.as_raw_fd())?;
            let mut children = Vec::with_capacity(names.len());
            for child in names {
                children.push(load_node(fd.as_raw_fd(), child, expected_uid)?);
            }
            if Snapshot::from_fd(fd.as_raw_fd())? != snapshot {
                return Err(io::Error::from_raw_os_error(libc::ESTALE));
            }
            Ok(Node {
                name,
                fd,
                snapshot,
                kind: NodeKind::Directory(children),
            })
        }
        libc::S_IFREG => {
            if probe_snapshot.nlink != 1 {
                return Err(io::Error::from_raw_os_error(libc::EMLINK));
            }
            let fd = open_at(
                parent,
                &name,
                libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW,
            )?;
            let snapshot = Snapshot::from_fd(fd.as_raw_fd())?;
            if snapshot != probe_snapshot || snapshot.kind() != libc::S_IFREG {
                return Err(io::Error::from_raw_os_error(libc::ESTALE));
            }
            let (fd, content) = read_regular(fd, snapshot)?;
            Ok(Node {
                name,
                fd,
                snapshot,
                kind: NodeKind::File(content),
            })
        }
        _ => Err(io::Error::from_raw_os_error(libc::EINVAL)),
    }
}

fn verify_node(node: &Node, expected_uid: u32) -> io::Result<()> {
    let current = Snapshot::from_fd(node.fd.as_raw_fd())?;
    if current != node.snapshot || !current.safe_owner_mode(expected_uid) {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    match &node.kind {
        NodeKind::File(_) => {
            if current.kind() != libc::S_IFREG || current.nlink != 1 {
                return Err(io::Error::from_raw_os_error(libc::ESTALE));
            }
        }
        NodeKind::Directory(children) => {
            if current.kind() != libc::S_IFDIR {
                return Err(io::Error::from_raw_os_error(libc::ESTALE));
            }
            let names = list_names(node.fd.as_raw_fd())?;
            if names.len() != children.len()
                || names
                    .iter()
                    .zip(children)
                    .any(|(name, child)| name != &child.name)
            {
                return Err(io::Error::from_raw_os_error(libc::ESTALE));
            }
            for child in children {
                verify_node(child, expected_uid)?;
            }
            if Snapshot::from_fd(node.fd.as_raw_fd())? != node.snapshot {
                return Err(io::Error::from_raw_os_error(libc::ESTALE));
            }
        }
    }
    Ok(())
}

fn hash_node(hasher: &mut Sha256, prefix: &[u8], node: &Node) {
    let mut relative = Vec::with_capacity(prefix.len() + node.name.len() + 1);
    relative.extend_from_slice(prefix);
    if !relative.is_empty() {
        relative.push(b'/');
    }
    relative.extend_from_slice(&node.name);
    hasher.update((relative.len() as u64).to_be_bytes());
    hasher.update(&relative);
    hasher.update(node.snapshot.uid.to_be_bytes());
    hasher.update(node.snapshot.gid.to_be_bytes());
    hasher.update((node.snapshot.mode & 0o7777).to_be_bytes());
    match &node.kind {
        NodeKind::Directory(children) => {
            hasher.update(b"d");
            for child in children {
                hash_node(hasher, &relative, child);
            }
        }
        NodeKind::File(content) => {
            hasher.update(b"f");
            hasher.update((node.snapshot.size as u64).to_be_bytes());
            hasher.update(content);
        }
    }
}

fn child<'a>(root: &'a Node, name: &[u8]) -> Option<&'a Node> {
    let NodeKind::Directory(children) = &root.kind else {
        return None;
    };
    children
        .binary_search_by(|node| {
            let ordering = node.name.as_slice().cmp(name);
            match ordering {
                Ordering::Less => Ordering::Less,
                Ordering::Equal => Ordering::Equal,
                Ordering::Greater => Ordering::Greater,
            }
        })
        .ok()
        .map(|index| &children[index])
}

fn digest_home_with_hook<F>(home: &Path, expected_uid: u32, hook: F) -> io::Result<String>
where
    F: FnOnce(),
{
    let canonical = std::fs::canonicalize(home)?;
    if !canonical.is_absolute() {
        return Err(io::Error::from_raw_os_error(libc::EINVAL));
    }
    let home_fd = open_absolute_no_symlinks(
        &canonical,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
    )?;
    let home_snapshot = validate_directory(home_fd.as_raw_fd(), expected_uid)?;
    let config_fd = open_at(
        home_fd.as_raw_fd(),
        b".config",
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
    )?;
    let config_snapshot = validate_directory(config_fd.as_raw_fd(), expected_uid)?;
    let root = load_node(config_fd.as_raw_fd(), b"sunshine".to_vec(), expected_uid)?;

    for required in [REQUIRED_CONFIG, REQUIRED_STATE] {
        let node =
            child(&root, required).ok_or_else(|| io::Error::from_raw_os_error(libc::ENOENT))?;
        match node.kind {
            NodeKind::File(_) if node.snapshot.size > 0 => {}
            _ => return Err(io::Error::from_raw_os_error(libc::EINVAL)),
        }
    }

    hook();

    let rebound_home = open_absolute_no_symlinks(
        &canonical,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
    )?;
    if validate_directory(rebound_home.as_raw_fd(), expected_uid)? != home_snapshot
        || Snapshot::from_fd(home_fd.as_raw_fd())? != home_snapshot
    {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    let rebound_config = open_at(
        rebound_home.as_raw_fd(),
        b".config",
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
    )?;
    if validate_directory(rebound_config.as_raw_fd(), expected_uid)? != config_snapshot
        || Snapshot::from_fd(config_fd.as_raw_fd())? != config_snapshot
    {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    let retained_config_entry = open_at(
        home_fd.as_raw_fd(),
        b".config",
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
    )?;
    if validate_directory(retained_config_entry.as_raw_fd(), expected_uid)? != config_snapshot {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    let rebound_root = open_at(
        rebound_config.as_raw_fd(),
        b"sunshine",
        libc::O_PATH | libc::O_NOFOLLOW,
    )?;
    let retained_root_entry = open_at(
        config_fd.as_raw_fd(),
        b"sunshine",
        libc::O_PATH | libc::O_NOFOLLOW,
    )?;
    if Snapshot::from_fd(rebound_root.as_raw_fd())? != root.snapshot
        || Snapshot::from_fd(retained_root_entry.as_raw_fd())? != root.snapshot
        || root.snapshot.kind() != libc::S_IFDIR
    {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    verify_node(&root, expected_uid)?;
    let mut hasher = Sha256::new();
    hash_node(&mut hasher, b"", &root);
    Ok(format!("{:x}", hasher.finalize()))
}

fn digest_home(home: &Path, expected_uid: u32) -> io::Result<String> {
    digest_home_with_hook(home, expected_uid, || {})
}

fn parse_args() -> Option<(PathBuf, u32)> {
    let mut args = env::args_os();
    let _program = args.next()?;
    let home = PathBuf::from(args.next()?);
    let uid = args.next()?.into_string().ok()?.parse().ok()?;
    if args.next().is_some() {
        return None;
    }
    Some((home, uid))
}

fn main() {
    let result = parse_args()
        .ok_or_else(|| io::Error::from_raw_os_error(libc::EINVAL))
        .and_then(|(home, uid)| digest_home(&home, uid));
    match result {
        Ok(digest) => println!("{digest}"),
        Err(_) => {
            eprintln!("{GENERIC_ERROR}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tempfile::TempDir;

    fn fixture() -> (TempDir, PathBuf, u32) {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let root = home.join(".config/sunshine");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("sunshine.conf"), b"config\n").unwrap();
        fs::write(root.join("sunshine_state.json"), b"{}\n").unwrap();
        let uid = unsafe { libc::geteuid() };
        (temp, home, uid)
    }

    #[test]
    fn stable_tree_has_one_digest() {
        let (_temp, home, uid) = fixture();
        let first = digest_home(&home, uid).unwrap();
        let second = digest_home(&home, uid).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    }

    #[test]
    fn rejects_symlink_and_hard_link() {
        let (temp, home, uid) = fixture();
        let root = home.join(".config/sunshine");
        symlink(temp.path().join("outside"), root.join("link")).unwrap();
        assert!(digest_home(&home, uid).is_err());
        fs::remove_file(root.join("link")).unwrap();
        fs::hard_link(root.join("sunshine.conf"), root.join("alias")).unwrap();
        assert!(digest_home(&home, uid).is_err());
    }

    #[test]
    fn rejects_special_node_and_unsafe_mode() {
        let (_temp, home, uid) = fixture();
        let root = home.join(".config/sunshine");
        let fifo = CString::new(root.join("fifo").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(digest_home(&home, uid).is_err());
        fs::remove_file(root.join("fifo")).unwrap();
        fs::set_permissions(
            root.join("sunshine.conf"),
            fs::Permissions::from_mode(0o662),
        )
        .unwrap();
        assert!(digest_home(&home, uid).is_err());
    }

    #[test]
    fn rejects_empty_required_files() {
        let (_temp, home, uid) = fixture();
        fs::write(home.join(".config/sunshine/sunshine.conf"), b"").unwrap();
        assert!(digest_home(&home, uid).is_err());
    }

    #[test]
    fn rejects_replaced_sunshine_root_and_config_parent() {
        let (_temp, home, uid) = fixture();
        let config = home.join(".config");
        let root = config.join("sunshine");
        assert!(digest_home_with_hook(&home, uid, || {
            fs::rename(&root, config.join("sunshine.old")).unwrap();
            fs::create_dir(&root).unwrap();
            fs::write(root.join("sunshine.conf"), b"replacement\n").unwrap();
            fs::write(root.join("sunshine_state.json"), b"{}\n").unwrap();
        })
        .is_err());

        let (_temp, home, uid) = fixture();
        let config = home.join(".config");
        assert!(digest_home_with_hook(&home, uid, || {
            fs::rename(&config, home.join(".config.old")).unwrap();
            let root = config.join("sunshine");
            fs::create_dir_all(&root).unwrap();
            fs::write(root.join("sunshine.conf"), b"replacement\n").unwrap();
            fs::write(root.join("sunshine_state.json"), b"{}\n").unwrap();
        })
        .is_err());
    }

    #[test]
    fn rejects_content_and_directory_races() {
        let (_temp, home, uid) = fixture();
        let config = home.join(".config/sunshine/sunshine.conf");
        assert!(digest_home_with_hook(&home, uid, || {
            fs::write(&config, b"changed\n").unwrap();
        })
        .is_err());

        let (_temp, home, uid) = fixture();
        let root = home.join(".config/sunshine");
        assert!(digest_home_with_hook(&home, uid, || {
            fs::write(root.join("late"), b"late\n").unwrap();
        })
        .is_err());
    }
}
