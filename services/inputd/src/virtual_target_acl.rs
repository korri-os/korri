use std::{
    ffi::{CString, OsStr, OsString},
    fs,
    os::{
        fd::RawFd,
        unix::ffi::OsStrExt,
        unix::fs::{FileTypeExt, MetadataExt},
    },
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use evdev::raw_stream::RawDevice;

const TARGET_NAME: &str = "Microsoft X-Box 360 pad";
const TARGET_KEYS: [u16; 15] = [
    0x130, 0x131, 0x133, 0x134, 0x136, 0x137, 0x13a, 0x13b, 0x13c, 0x13d, 0x13e, 0x2c0, 0x2c1,
    0x2c2, 0x2c3,
];
const TARGET_ABS: [u16; 8] = [0, 1, 2, 3, 4, 5, 0x10, 0x11];

#[derive(Clone, Debug, Eq, PartialEq)]
struct Facts {
    character: bool,
    virtual_sysfs: bool,
    empty_phys: bool,
    empty_uniq: bool,
    name: String,
    bus: u16,
    vendor: u16,
    product: u16,
    version: u16,
    keys: Vec<u16>,
    abs: Vec<u16>,
    force_feedback: bool,
}

impl Facts {
    fn validated(&self) -> bool {
        self.character
            && self.virtual_sysfs
            && self.empty_phys
            && self.empty_uniq
            && self.name == TARGET_NAME
            && (self.bus, self.vendor, self.product, self.version)
                == (0x0003, 0x045e, 0x028e, 0x0001)
            && self.keys == TARGET_KEYS
            && self.abs == TARGET_ABS
            && self.force_feedback
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Binding {
    dev: u64,
    ino: u64,
    rdev: u64,
}

struct HeldTarget {
    fd: RawFd,
    requested: PathBuf,
    binding: Binding,
    sysfs: PathBuf,
}

impl Drop for HeldTarget {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("korri-virtual-target-acl: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut args: Vec<std::ffi::OsString>) -> Result<(), String> {
    let mut device_root = PathBuf::from("/dev/input");
    let mut sys_root = PathBuf::from("/sys");
    let mut setfacl = PathBuf::from("setfacl");
    let mut action_users = Vec::new();
    while args
        .first()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.starts_with("--"))
    {
        let flag = args.remove(0);
        let value = args.first().cloned().ok_or("option value is missing")?;
        args.remove(0);
        match flag.to_str() {
            Some("--device-root") => device_root = value.into(),
            Some("--sys-root") => sys_root = value.into(),
            Some("--setfacl") => setfacl = value.into(),
            Some("--action-user") => action_users.push(value),
            _ => return Err("unknown option".into()),
        }
    }
    let operation = args
        .first()
        .and_then(|v| v.to_str())
        .ok_or("operation is missing")?;
    match operation {
        "grant" if args.len() == 4 => {
            let users = grant_ids(&args[1], &args[2], &action_users)?;
            mutate_one(
                &args[3],
                &device_root,
                &sys_root,
                &setfacl,
                Some(&users),
            )
        }
        "reapply" if args.len() == 3 => {
            let users = grant_ids(&args[1], &args[2], &action_users)?;
            for entry in fs::read_dir(&device_root).map_err(generic)? {
                let entry = entry.map_err(generic)?;
                if !event_name(&entry.file_name()) {
                    continue;
                }
                if let Ok(held) = open_requested(&entry.path(), &device_root, &sys_root) {
                    mutate_held(&held, &sys_root, &setfacl, Some(&users))?;
                }
            }
            Ok(())
        }
        "revoke" if args.len() == 1 => {
            for entry in fs::read_dir(&device_root).map_err(generic)? {
                let entry = entry.map_err(generic)?;
                if !event_name(&entry.file_name()) {
                    continue;
                }
                if let Ok(held) = open_requested(&entry.path(), &device_root, &sys_root) {
                    mutate_held(&held, &sys_root, &setfacl, None)?;
                }
            }
            Ok(())
        }
        _ => Err(
            "usage: [--action-user NAME]... {grant INPUTD_UID ACTION_UID DEVICE|reapply INPUTD_UID ACTION_UID|revoke}"
                .into(),
        ),
    }
}

fn grant_ids(inputd: &OsStr, action: &OsStr, extra: &[OsString]) -> Result<Vec<u32>, String> {
    let mut users = vec![numeric_id(inputd)?, numeric_id(action)?];
    for name in extra {
        let uid = action_user_id(name)?;
        if !users.contains(&uid) {
            users.push(uid);
        }
    }
    Ok(users)
}

fn action_user_id(name: &OsStr) -> Result<u32, String> {
    let name = CString::new(name.as_bytes()).map_err(|_| "action user name is invalid")?;
    if name.as_bytes().is_empty() {
        return Err("action user name is invalid".into());
    }
    // NixOS can assign system UIDs at activation. Resolve the trusted name once,
    // before any mutation, and pass only numeric IDs to setfacl. Revoke does not
    // resolve names: account removal must not prevent removal of its ACL.
    let mut buffer = vec![0u8; 1024];
    loop {
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result = std::ptr::null_mut();
        let status = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                &mut entry,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && buffer.len() < 1024 * 1024 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0 || result.is_null() {
            return Err(format!(
                "action user {} could not be resolved",
                name.to_string_lossy()
            ));
        }
        if entry.pw_uid == 0 || entry.pw_uid == u32::MAX {
            return Err("action user must have an unprivileged UID".into());
        }
        return Ok(entry.pw_uid);
    }
}

fn numeric_id(value: &OsStr) -> Result<u32, String> {
    let value = value.to_str().ok_or("identity is invalid")?;
    let parsed = value.parse::<u32>().map_err(|_| "identity is invalid")?;
    if parsed == 0 || parsed.to_string() != value {
        return Err("identity is invalid".into());
    }
    Ok(parsed)
}

fn event_name(value: &OsStr) -> bool {
    value.to_str().is_some_and(|name| {
        name.strip_prefix("event")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

fn mutate_one(
    requested: &OsStr,
    device_root: &Path,
    sys_root: &Path,
    setfacl: &Path,
    grant: Option<&[u32]>,
) -> Result<(), String> {
    let requested = PathBuf::from(requested);
    let held = open_requested(&requested, device_root, sys_root)?;
    mutate_held(&held, sys_root, setfacl, grant)
}

fn open_requested(
    requested: &Path,
    device_root: &Path,
    sys_root: &Path,
) -> Result<HeldTarget, String> {
    if requested.parent() != Some(device_root) || !requested.file_name().is_some_and(event_name) {
        return Err("target path is outside the event device root".into());
    }
    open_and_validate(requested, sys_root)
}

fn mutate_held(
    held: &HeldTarget,
    sys_root: &Path,
    setfacl: &Path,
    grant: Option<&[u32]>,
) -> Result<(), String> {
    rebind(held, sys_root)?;
    let procfd = PathBuf::from(format!("/proc/self/fd/{}", held.fd));
    run_setfacl(
        setfacl,
        [OsStr::new("-b"), OsStr::new("--"), procfd.as_os_str()],
    )?;
    if let Some(users) = grant {
        rebind(held, sys_root)?;
        let mut acl = users
            .iter()
            .map(|uid| format!("u:{uid}:r"))
            .collect::<Vec<_>>()
            .join(",");
        acl.push_str(",m::r");
        run_setfacl(
            setfacl,
            [
                OsStr::new("-m"),
                OsStr::new(&acl),
                OsStr::new("--"),
                procfd.as_os_str(),
            ],
        )?;
    }
    Ok(())
}

fn open_and_validate(requested: &Path, sys_root: &Path) -> Result<HeldTarget, String> {
    let path =
        CString::new(requested.as_os_str().as_bytes()).map_err(|_| "target path is invalid")?;
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_PATH | libc::O_NOFOLLOW) };
    if fd < 0 {
        return Err("target could not be opened safely".into());
    }
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut stat) } != 0 {
        unsafe { libc::close(fd) };
        return Err("target metadata is unavailable".into());
    }
    let binding = Binding {
        dev: stat.st_dev,
        ino: stat.st_ino,
        rdev: stat.st_rdev,
    };
    let major = libc::major(stat.st_rdev);
    let minor = libc::minor(stat.st_rdev);
    let sysfs_link = sys_root.join("dev/char").join(format!("{major}:{minor}"));
    let sysfs = fs::canonicalize(&sysfs_link).map_err(|_| "target sysfs binding is unavailable")?;
    let virtual_root = fs::canonicalize(sys_root.join("devices/virtual/input")).map_err(generic)?;
    let procfd = format!("/proc/self/fd/{fd}");
    let device = RawDevice::open(&procfd).map_err(|_| "target evdev facts are unavailable")?;
    let id = device.input_id();
    let facts = Facts {
        character: (stat.st_mode & libc::S_IFMT) == libc::S_IFCHR,
        virtual_sysfs: sysfs.starts_with(&virtual_root),
        empty_phys: device.physical_path().unwrap_or("").is_empty(),
        empty_uniq: device.unique_name().unwrap_or("").is_empty(),
        name: device.name().unwrap_or("").to_owned(),
        bus: id.bus_type().0,
        vendor: id.vendor(),
        product: id.product(),
        version: id.version(),
        keys: device
            .supported_keys()
            .map(|v| v.iter().map(|k| k.0).collect())
            .unwrap_or_default(),
        abs: device
            .supported_absolute_axes()
            .map(|v| v.iter().map(|a| a.0).collect())
            .unwrap_or_default(),
        force_feedback: device
            .supported_events()
            .contains(evdev::EventType::FORCEFEEDBACK),
    };
    if !facts.validated() {
        unsafe { libc::close(fd) };
        return Err("target is not the exact InputPlumber virtual Xbox device".into());
    }
    Ok(HeldTarget {
        fd,
        requested: requested.to_owned(),
        binding,
        sysfs,
    })
}

fn rebind(held: &HeldTarget, sys_root: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&held.requested).map_err(|_| "target path was replaced")?;
    let now = Binding {
        dev: metadata.dev(),
        ino: metadata.ino(),
        rdev: metadata.rdev(),
    };
    if now != held.binding || !metadata.file_type().is_char_device() {
        return Err("target path was replaced".into());
    }
    let major = libc::major(held.binding.rdev);
    let minor = libc::minor(held.binding.rdev);
    let sysfs = fs::canonicalize(sys_root.join("dev/char").join(format!("{major}:{minor}")))
        .map_err(|_| "target sysfs binding was replaced")?;
    if sysfs != held.sysfs {
        return Err("target sysfs binding was replaced".into());
    }
    Ok(())
}

fn run_setfacl<const N: usize>(setfacl: &Path, args: [&OsStr; N]) -> Result<(), String> {
    let status = Command::new(setfacl).args(args).status().map_err(generic)?;
    if status.success() {
        Ok(())
    } else {
        Err("ACL mutation failed".into())
    }
}

fn generic(error: impl std::fmt::Display) -> String {
    format!("operation failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> Facts {
        Facts {
            character: true,
            virtual_sysfs: true,
            empty_phys: true,
            empty_uniq: true,
            name: TARGET_NAME.into(),
            bus: 3,
            vendor: 0x045e,
            product: 0x028e,
            version: 1,
            keys: TARGET_KEYS.to_vec(),
            abs: TARGET_ABS.to_vec(),
            force_feedback: true,
        }
    }
    #[test]
    fn physical_xbox_lookalike_is_rejected() {
        let mut f = valid();
        f.virtual_sysfs = false;
        assert!(!f.validated());
        f.virtual_sysfs = true;
        f.empty_phys = false;
        assert!(!f.validated());
    }
    #[test]
    fn exact_capabilities_are_required() {
        let mut f = valid();
        f.keys.pop();
        assert!(!f.validated());
        let mut f = valid();
        f.force_feedback = false;
        assert!(!f.validated());
    }
    #[test]
    fn event_replacement_binding_is_detected() {
        let a = Binding {
            dev: 1,
            ino: 2,
            rdev: 3,
        };
        let b = Binding {
            dev: 1,
            ino: 4,
            rdev: 3,
        };
        assert_ne!(a, b);
    }
    #[test]
    fn named_action_user_is_resolved_before_reapply() {
        let root = tempfile::tempdir().unwrap();
        // Exercise the real account database without assuming a fixed system UID.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result = std::ptr::null_mut();
        let mut buffer = vec![0u8; 16384];
        let uid = unsafe { libc::geteuid() };
        if uid == 0 {
            return;
        }
        assert_eq!(
            unsafe {
                libc::getpwuid_r(
                    uid,
                    &mut entry,
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &mut result,
                )
            },
            0
        );
        assert!(!result.is_null());
        let name = unsafe { std::ffi::CStr::from_ptr(entry.pw_name) }
            .to_str()
            .unwrap();
        assert_eq!(action_user_id(OsStr::new(name)).unwrap(), uid);
        let mut expected = vec![977, 1001];
        if !expected.contains(&uid) {
            expected.push(uid);
        }
        assert_eq!(
            grant_ids(
                OsStr::new("977"),
                OsStr::new("1001"),
                &[name.into(), name.into()]
            )
            .unwrap(),
            expected
        );
        run(vec![
            "--action-user".into(),
            name.into(),
            "--device-root".into(),
            root.path().into(),
            "reapply".into(),
            "977".into(),
            "1001".into(),
        ])
        .unwrap();
    }

    #[test]
    fn invalid_named_users_fail_closed_but_cannot_block_revoke() {
        for name in ["root", "", "korri-acl-no-such-user", "bad\0name"] {
            assert!(action_user_id(OsStr::new(name)).is_err());
        }
        let root = tempfile::tempdir().unwrap();
        run(vec![
            "--action-user".into(),
            "korri-acl-no-such-user".into(),
            "--device-root".into(),
            root.path().into(),
            "revoke".into(),
        ])
        .unwrap();
        assert!(run(vec![
            "--action-user".into(),
            "korri-acl-no-such-user".into(),
            "--device-root".into(),
            root.path().into(),
            "reapply".into(),
            "977".into(),
            "1001".into(),
        ])
        .is_err());
    }

    #[test]
    fn extra_readers_use_held_fd_and_revoke_clears_every_acl() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let sysfs = root.path().join("virtual");
        fs::create_dir(&sysfs).unwrap();
        fs::create_dir_all(root.path().join("dev/char")).unwrap();
        let metadata = fs::metadata("/dev/null").unwrap();
        symlink(
            &sysfs,
            root.path().join(format!(
                "dev/char/{}:{}",
                libc::major(metadata.rdev()),
                libc::minor(metadata.rdev())
            )),
        )
        .unwrap();
        let held = HeldTarget {
            fd: unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_PATH) },
            requested: "/dev/null".into(),
            binding: Binding {
                dev: metadata.dev(),
                ino: metadata.ino(),
                rdev: metadata.rdev(),
            },
            sysfs,
        };
        assert!(held.fd >= 0);
        let log = root.path().join("calls");
        let command = root.path().join("setfacl");
        // Record a real subprocess invocation, but never mutate /dev/null's ACL.
        fs::write(&command, format!("#!/bin/sh\nfor arg do last=$arg; done\n[ -c \"$last\" ] || exit 1\nprintf '%s\\n' \"$@\" >> '{}'\n", log.display())).unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o700)).unwrap();
        let procfd = format!("/proc/self/fd/{}", held.fd);
        for (users, acl) in [
            (vec![977, 1001], "u:977:r,u:1001:r,m::r"),
            (
                vec![977, 1001, 1234, 1235],
                "u:977:r,u:1001:r,u:1234:r,u:1235:r,m::r",
            ),
        ] {
            fs::write(&log, "").unwrap();
            mutate_held(&held, root.path(), &command, Some(&users)).unwrap();
            mutate_held(&held, root.path(), &command, None).unwrap();
            assert_eq!(
                fs::read_to_string(&log).unwrap(),
                format!("-b\n--\n{procfd}\n-m\n{acl}\n--\n{procfd}\n-b\n--\n{procfd}\n")
            );
        }
        // Replacement must fail before the configured ACL command can run.
        let mut held = held;
        held.requested = root.path().join("event9");
        fs::write(&held.requested, "replacement").unwrap();
        fs::write(&log, "").unwrap();
        assert!(mutate_held(&held, root.path(), &command, Some(&[977, 1001, 1234])).is_err());
        assert_eq!(fs::read_to_string(&log).unwrap(), "");
    }

    #[test]
    fn ids_are_canonical_and_unprivileged() {
        assert_eq!(numeric_id(OsStr::new("1001")).unwrap(), 1001);
        assert!(numeric_id(OsStr::new("0")).is_err());
        assert!(numeric_id(OsStr::new("01")).is_err());
    }
}
