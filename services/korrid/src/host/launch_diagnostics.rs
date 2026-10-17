//! Runtime-only diagnostics for one exact modifier launch. No journal authority.
use crate::launcher::redaction::redact_text;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
};

pub(crate) const DIRECTORY_ENV: &str = "KORRI_LAUNCH_DIAGNOSTICS_DIRECTORY";
pub(crate) const READER_ENV: &str = "KORRI_LAUNCH_DIAGNOSTICS_READER_UID";
pub(crate) const STDERR: &str = "stderr";
const WORDS: &str = "redaction-words.json";
// /run is tmpfs: a long-running chatty game must not keep consuming handheld RAM.
// The existing one-second session watcher checks this, retaining the last 32 KiB.
pub(crate) const MAX_STDERR_BYTES: u64 = 64 * 1024;
const RETAIN_STDERR_BYTES: u64 = 32 * 1024;
const MAX_WORDS_BYTES: u64 = 1024 * 1024;

/// Linux's native five-entry ACL shape is also used by systemd credentials
/// (main.rs). Group and other have no access; mask bits are not group grants.
fn private_acl(owner: u16, user: u32, permission: u16) -> Vec<u8> {
    let mut bytes = 2_u32.to_le_bytes().to_vec();
    for (tag, permission, id) in [
        (1_u16, owner, u32::MAX),
        (2, permission, user),
        (4, 0, u32::MAX),
        (16, permission, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(permission.to_le_bytes());
        bytes.extend(id.to_le_bytes());
    }
    bytes
}
fn grant_user(file: &File, owner: u16, user: u32, permission: u16) -> io::Result<()> {
    let acl = private_acl(owner, user, permission);
    let result = unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            c"system.posix_acl_access".as_ptr(),
            acl.as_ptr().cast(),
            acl.len(),
            0,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn regular(path: &Path, write: bool, owner: u32) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(write)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != owner {
        return Err(io::Error::other("launch diagnostic type or owner mismatch"));
    }
    Ok(file)
}

pub(super) fn create(directory: &Path, runtime_uid: u32) -> io::Result<()> {
    let parent = directory
        .parent()
        .ok_or_else(|| io::Error::other("diagnostics directory has no parent"))?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    fs::create_dir(directory)?;
    // Sticky protects korrid-owned stderr from unlink/rename by the runtime
    // uid, while still allowing that uid to create its own words file.
    fs::set_permissions(directory, fs::Permissions::from_mode(0o1700))?;
    // The source lives under korrid's private runtime. A game gets only its
    // own directory bind. Named runtime uid rwx is needed to create its words
    // file; neither the owning group nor other users can enter this directory.
    grant_user(&File::open(directory)?, 7, runtime_uid, 7)?;
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join(STDERR))?;
    // korrid owns stderr; the runtime uid has read access and systemd supplies
    // its writable stderr fd. The ACL mask makes stat report 0640, but group
    // and other entries remain zero. No system journal access is granted.
    grant_user(&stderr, 6, runtime_uid, 4)
}

/// The runtime-user plugin-launch process records facts before exec. The game
/// uid can rewrite this file, but can only spoil its own error text: every
/// reader uses one exact unit directory and refuses symlinks/special files.
pub(crate) fn write_redaction_words(words: &[String]) -> Result<(), String> {
    let Some(directory) = std::env::var_os(DIRECTORY_ENV) else {
        return Ok(());
    };
    let reader: u32 = std::env::var(READER_ENV)
        .map_err(|_| "diagnostics reader uid is missing")?
        .parse()
        .map_err(|_| "diagnostics reader uid is not numeric")?;
    write_words(Path::new(&directory), reader, words)
        .map_err(|e| format!("cannot record launch redaction words: {e}"))
}
pub(super) fn write_words(directory: &Path, reader: u32, words: &[String]) -> io::Result<()> {
    let bytes = serde_json::to_vec(words)?;
    if bytes.len() as u64 > MAX_WORDS_BYTES {
        return Err(io::Error::other("launch redaction words exceed 1 MiB"));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join(WORDS))?;
    // Created 0600 by the runtime uid. Only the named korrid uid gains read
    // access; 0640 after this is the ACL mask, never owning-group permission.
    grant_user(&file, 6, reader, 4)?;
    file.write_all(&bytes)
}

pub(super) fn maintain(directory: &Path) -> io::Result<()> {
    match regular(&directory.join(STDERR), true, unsafe { libc::geteuid() }) {
        Ok(mut file) => retain_tail(&mut file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
fn retain_tail(file: &mut File) -> io::Result<()> {
    let size = file.metadata()?.len();
    if size <= MAX_STDERR_BYTES {
        return Ok(());
    }
    file.seek(SeekFrom::Start(size - RETAIN_STDERR_BYTES))?;
    let mut tail = vec![0; RETAIN_STDERR_BYTES as usize];
    file.read_exact(&mut tail)?;
    // Keep only complete lines. A prefix cut inside a private word could not
    // be matched against its original value when the failure is reported.
    let start = tail
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(tail.len());
    let tail = &tail[start..];
    file.seek(SeekFrom::Start(0))?;
    file.write_all(tail)?;
    // StandardError=append: keeps the writer on this inode using O_APPEND.
    // Do not rename or rotate it, which would leave the unit writing elsewhere.
    file.set_len(tail.len() as u64)
}

pub(super) fn failure_reason(directory: &Path, runtime_uid: u32) -> io::Result<Option<String>> {
    if !directory.exists() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    regular(&directory.join(WORDS), false, runtime_uid)?
        .take(MAX_WORDS_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_WORDS_BYTES {
        return Err(io::Error::other("redaction words exceed 1 MiB"));
    }
    let words: Vec<String> = serde_json::from_slice(&bytes)?;
    let mut file = regular(&directory.join(STDERR), false, unsafe { libc::geteuid() })?;
    let size = file.metadata()?.len();
    let offset = size.saturating_sub(MAX_STDERR_BYTES);
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(MAX_STDERR_BYTES).read_to_end(&mut bytes)?;
    // Do not show a word cut at the bounded read's start: it could be a partial
    // credential that no complete-word redactor could recognize.
    let mut tail = String::from_utf8_lossy(&bytes).into_owned();
    if offset != 0 {
        tail = tail
            .split_once('\n')
            .map(|(_, tail)| tail.to_owned())
            .unwrap_or_default();
    }
    let redacted = redact_text(&tail, &words);
    let lines = redacted
        .lines()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    let mut chars = lines
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n')
        .rev()
        .take(300)
        .collect::<Vec<_>>();
    chars.reverse();
    let reason: String = chars.into_iter().collect();
    Ok((!reason.trim().is_empty()).then_some(reason))
}

pub(super) fn cleanup(directory: &Path) -> io::Result<()> {
    // Non-modifier units have no diagnostic source. This also avoids probing
    // a different daemon user's protected /run/korrid from local CLI tests.
    if !directory.exists() {
        return Ok(());
    }
    match fs::remove_dir_all(directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn acl(file: &File) -> Vec<u8> {
        let mut bytes = vec![0; 44];
        let count = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                c"system.posix_acl_access".as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        assert_eq!(count, 44, "{}", io::Error::last_os_error());
        bytes
    }
    #[test]
    fn runtime_files_grant_only_the_two_exact_users_not_groups_or_other_users() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("unit");
        let runtime_uid = unsafe { libc::geteuid() } + 1;
        create(&directory, runtime_uid).unwrap();
        assert_eq!(
            acl(&File::open(&directory).unwrap()),
            private_acl(7, runtime_uid, 7)
        );
        assert_eq!(fs::metadata(&directory).unwrap().mode() & 0o7777, 0o1770);
        let stderr = File::open(directory.join(STDERR)).unwrap();
        assert_eq!(acl(&stderr), private_acl(6, runtime_uid, 4));
        assert_eq!(stderr.metadata().unwrap().mode() & 0o777, 0o640);
        write_words(&directory, runtime_uid, &["private".into()]).unwrap();
        let words = File::open(directory.join(WORDS)).unwrap();
        let actual = acl(&words);
        assert_eq!(actual, private_acl(6, runtime_uid, 4));
        let entries: Vec<_> = actual[4..]
            .chunks_exact(8)
            .map(|entry| {
                (
                    u16::from_le_bytes(entry[..2].try_into().unwrap()),
                    u16::from_le_bytes(entry[2..4].try_into().unwrap()),
                    u32::from_le_bytes(entry[4..8].try_into().unwrap()),
                )
            })
            .collect();
        assert_eq!(
            entries,
            [
                (1, 6, u32::MAX),
                (2, 4, runtime_uid),
                (4, 0, u32::MAX),
                (16, 4, u32::MAX),
                (32, 0, u32::MAX)
            ]
        );
        assert_eq!(words.metadata().unwrap().mode() & 0o777, 0o640);
    }
    #[test]
    fn truncation_retains_32_kib_in_place_and_append_keeps_working() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut writer = OpenOptions::new().append(true).open(file.path()).unwrap();
        writer
            .write_all(&vec![b'\n'; MAX_STDERR_BYTES as usize])
            .unwrap();
        retain_tail(file.as_file_mut()).unwrap();
        assert_eq!(file.as_file().metadata().unwrap().len(), MAX_STDERR_BYTES);
        writer.write_all(b"last native failure\n").unwrap();
        retain_tail(file.as_file_mut()).unwrap();
        let length = file.as_file().metadata().unwrap().len();
        assert!(length <= RETAIN_STDERR_BYTES);
        writer.write_all(b"after truncation\n").unwrap();
        assert_eq!(file.as_file().metadata().unwrap().len(), length + 17);
        file.seek(SeekFrom::End(-37)).unwrap();
        let mut tail = String::new();
        file.read_to_string(&mut tail).unwrap();
        assert!(tail.ends_with("last native failure\nafter truncation\n"));
    }
    #[test]
    fn failure_tail_is_bounded_redacted_and_refuses_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("unit");
        create(&directory, unsafe { libc::geteuid() } + 1).unwrap();
        write_words(
            &directory,
            unsafe { libc::geteuid() } + 1,
            &["--private-flag".into(), "env-secret".into()],
        )
        .unwrap();
        fs::write(
            directory.join(STDERR),
            format!(
                "old line\n{}\nfirst\nsecond env-secret\nunknown option --private-flag",
                "a".repeat(500)
            ),
        )
        .unwrap();
        let reason = failure_reason(&directory, unsafe { libc::geteuid() })
            .unwrap()
            .unwrap();
        assert_eq!(
            reason,
            "first\nsecond [redacted]\nunknown option [redacted]"
        );
        fs::write(
            directory.join(STDERR),
            format!("{} env-secret", "界".repeat(500)),
        )
        .unwrap();
        let reason = failure_reason(&directory, unsafe { libc::geteuid() })
            .unwrap()
            .unwrap();
        assert!(reason.chars().count() <= 300);
        assert!(!reason.contains("env-secret"));
        fs::remove_file(directory.join(WORDS)).unwrap();
        std::os::unix::fs::symlink(directory.join(STDERR), directory.join(WORDS)).unwrap();
        assert!(failure_reason(&directory, unsafe { libc::geteuid() }).is_err());
        cleanup(&directory).unwrap();
        assert!(!directory.exists());
    }
}

#[cfg(test)]
mod owner_hardening_tests {
    use super::*;
    #[test]
    fn either_regular_file_with_the_wrong_owner_is_rejected_before_reading() {
        let root = tempfile::tempdir().unwrap();
        let wrong_owner = unsafe { libc::geteuid() } + 1;
        for filename in [STDERR, WORDS] {
            let path = root.path().join(filename);
            fs::write(&path, "DO_NOT_READ_PRIVATE_CONTENT").unwrap();
            let error = regular(&path, false, wrong_owner).unwrap_err();
            assert!(error.to_string().contains("owner mismatch"));
            assert!(!error.to_string().contains("DO_NOT_READ"));
        }
    }
}
