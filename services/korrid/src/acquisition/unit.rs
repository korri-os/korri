//! Transient unpack units have deterministic contribution ownership, so a
//! restarted standalone daemon can stop an orphan before cleaning its work.
use super::Failure;
use crate::plugin::ArtifactRecord;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{atomic::AtomicBool, Arc},
};

/// `korri-acquire-<plugin short>-<key>.service`, from the two segments of the
/// artifact's cache path `cache/<plugin short>/<key>`.
fn name(artifact: &ArtifactRecord) -> String {
    let (plugin, key) = artifact.id.split_once('/').expect("validated artifact id");
    let (_, plugin) = plugin.split_once(':').expect("validated plugin id");
    format!("korri-acquire-{plugin}-{key}.service")
}

/// ADR 0030: a store job's unit, `korri-acquire-<plugin short>-<provider
/// name>.service`, from the two segments of `placement::provider_cache`, in
/// the same way as an artifact's. Provider names and artifact keys share one
/// grammar, and a plugin's artifact key never equals one of its provider
/// names, so the two kinds of unit never share a name.
pub(super) fn provider_name(plugin_id: &str, provider_id: &str) -> String {
    let (_, plugin) = plugin_id.split_once(':').expect("validated plugin id");
    let (_, provider) = provider_id.split_once(':').expect("validated provider id");
    format!("korri-acquire-{plugin}-{provider}.service")
}

fn control(
    systemctl: &Path,
    unit: &str,
    verb: &str,
    extra: &[&str],
) -> Result<std::process::Output, Failure> {
    Command::new(systemctl)
        .args(["--system", "--no-ask-password", verb])
        .arg(unit)
        .args(extra)
        .env("SYSTEMD_BUS_TIMEOUT", "30s")
        .stdin(Stdio::null())
        .output()
        .map_err(|_| Failure::Unpack)
}
fn active(systemctl: &Path, unit: &str) -> Result<bool, Failure> {
    let output = control(
        systemctl,
        unit,
        "show",
        &["--property=LoadState", "--property=ActiveState"],
    )?;
    let text = String::from_utf8_lossy(&output.stdout);
    if text.lines().any(|line| line == "LoadState=not-found") {
        return Ok(false);
    }
    if !output.status.success() {
        return Err(Failure::Unpack);
    }
    match text
        .lines()
        .find_map(|line| line.strip_prefix("ActiveState="))
    {
        Some("inactive" | "failed") => Ok(false),
        Some("active" | "activating" | "reloading" | "deactivating") => Ok(true),
        _ => Err(Failure::Unpack),
    }
}

pub(super) fn reclaim(artifact: &ArtifactRecord) -> Result<(), Failure> {
    if artifact.unpack.is_none() {
        return Ok(());
    }
    #[cfg(test)]
    let _ = artifact;
    #[cfg(not(test))]
    reclaim_with(&systemctl_path(), &name(artifact))?;
    Ok(())
}

/// Stop a store job's unit: at startup before `.tmp-*` folders go, and when
/// the person cancels a job in `Unpacking`.
pub(super) fn reclaim_provider(unit: &str) -> Result<(), Failure> {
    #[cfg(test)]
    let _ = unit;
    #[cfg(not(test))]
    reclaim_with(&systemctl_path(), unit)?;
    Ok(())
}

fn reclaim_with(systemctl: &Path, unit: &str) -> Result<(), Failure> {
    if active(systemctl, unit)? {
        // systemctl stop waits for the entire control group to stop. Never
        // discard .tmp-* after a failed stop or an unavailable manager.
        if !control(systemctl, unit, "stop", &[])?.status.success() {
            return Err(Failure::Unpack);
        }
        if active(systemctl, unit)? {
            return Err(Failure::Unpack);
        }
    }
    Ok(())
}

#[cfg(not(test))]
fn systemctl_path() -> PathBuf {
    std::env::var_os("KORRID_SYSTEMCTL")
        .map(PathBuf::from)
        .unwrap_or_else(|| "systemctl".into())
}

/// ADR 0030: the store reader's unit bounds. `memory` is `MemAvailable` at
/// launch, the same rule as the dictionary check and never above it.
#[derive(Clone, Copy, Debug)]
pub(super) struct ReaderLimits {
    pub memory: u64,
}

fn unit_arguments(
    base: &Path,
    private_root: &Path,
    unit: &str,
    reader: Option<ReaderLimits>,
) -> Result<Vec<String>, Failure> {
    // systemd's path list grammar is not argv grammar. Refuse whitespace and
    // specifier expansion rather than inserting a second writable path.
    for path in [base, private_root] {
        if !path.is_absolute()
            || path
                .to_str()
                .is_none_or(|value| value.contains('%') || value.chars().any(char::is_whitespace))
        {
            return Err(Failure::UnsafePath);
        }
    }
    let mut arguments = vec![
        "--system".into(),
        "--no-ask-password".into(),
        "--quiet".into(),
        "--wait".into(),
        "--collect".into(),
        "--service-type=exec".into(),
        format!("--unit={unit}"),
        format!("--uid={}", unsafe { libc::geteuid() }),
        format!("--gid={}", unsafe { libc::getegid() }),
        "--property=KillMode=control-group".into(),
        "--property=NoNewPrivileges=yes".into(),
        "--property=CapabilityBoundingSet=".into(),
        "--property=AmbientCapabilities=".into(),
        "--property=ProtectSystem=strict".into(),
        "--property=ProtectHome=yes".into(),
        "--property=PrivateNetwork=yes".into(),
        "--property=PrivateDevices=yes".into(),
        "--property=PrivatePIDs=yes".into(),
        "--property=ProtectProc=invisible".into(),
        "--property=ProtectKernelTunables=yes".into(),
        "--property=ProtectKernelModules=yes".into(),
        "--property=ProtectControlGroups=yes".into(),
        "--property=RestrictSUIDSGID=yes".into(),
        "--property=RestrictAddressFamilies=AF_UNIX".into(),
        "--property=UMask=0022".into(),
        format!("--property=ReadWritePaths={}", base.display()),
        format!(
            "--property=InaccessiblePaths={} /run /tmp /var/tmp /dev/shm",
            private_root.display()
        ),
        "--property=StandardOutput=null".into(),
        "--property=StandardError=journal".into(),
    ];
    if let Some(reader) = reader {
        // Review P1-2: a decoder that overreaches (a 7z header decodes with
        // no limit) is killed inside the sandbox, not on the device.
        arguments.push(format!("--property=MemoryMax={}", reader.memory));
        arguments.push("--property=MemorySwapMax=0".into());
    }
    arguments.push("--".into());
    Ok(arguments)
}

pub(super) fn unpack(
    base: &Path,
    private_root: &Path,
    artifact: &ArtifactRecord,
    program: &Path,
    work: &Path,
    files: &[PathBuf],
) -> Result<(), Failure> {
    #[cfg(test)]
    let mut command = Command::new(program);
    #[cfg(not(test))]
    let mut command = {
        // Recovery normally stopped an orphan before creating this work dir.
        // A still-active named unit is never replaced or mistaken for this run.
        let unit = name(artifact);
        let mut command = sandboxed(base, private_root, &unit, None)?;
        command.arg(program);
        command
    };
    #[cfg(test)]
    let _ = (base, private_root, artifact);
    // Stderr remains diagnostics; never interpolate it into a wire failure.
    let status = command
        .arg(work)
        .args(files)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Unpack)
    }
}

/// `systemd-run` with the sandbox properties, after any older unit with this
/// name has stopped.
#[cfg(not(test))]
fn sandboxed(
    base: &Path,
    private_root: &Path,
    unit: &str,
    reader: Option<ReaderLimits>,
) -> Result<Command, Failure> {
    // Recovery normally stopped an orphan before creating this work dir.
    // A still-active named unit is never replaced or mistaken for this run.
    let systemctl = systemctl_path();
    if active(&systemctl, unit)? {
        while active(&systemctl, unit)? {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return Err(Failure::Unpack);
    }
    let mut command = Command::new(
        std::env::var_os("KORRID_SYSTEMD_RUN").unwrap_or_else(|| "systemd-run".into()),
    );
    command.args(unit_arguments(base, private_root, unit, reader)?);
    Ok(command)
}

/// ADR 0030: unpack a store download with korrid's own reader,
/// `korrid unpack-archive <work-dir> <file>`, in the sandbox unit. Tests run
/// the same reader in this process; `cancel` stops it between writes.
pub(super) fn unpack_archive(
    base: &Path,
    private_root: &Path,
    unit: &str,
    work: &Path,
    archive: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(), Failure> {
    #[cfg(test)]
    {
        let _ = (base, private_root, unit);
        let limits = super::archive::Limits {
            cancel: Some(cancel),
            ..super::archive::Limits::device().map_err(|stop| stop.failure)?
        };
        super::archive::extract(archive, work, &limits).map_err(|stop| {
            log::debug!("korrid unpack-archive: {}", stop.reason);
            stop.failure
        })
    }
    #[cfg(not(test))]
    {
        let memory = super::archive::device_memory().map_err(|stop| stop.failure)?;
        let mut command = sandboxed(base, private_root, unit, Some(ReaderLimits { memory }))?;
        let status = command
            .arg(std::env::current_exe()?)
            .arg("unpack-archive")
            .arg(work)
            .arg(archive)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()?;
        if status.success() {
            return Ok(());
        }
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Failure::Cancelled);
        }
        Err(exit_failure(work, None))
    }
}

/// The reader exits 1 for every failure. korrid reads the same floor after
/// the exit: less room than one write means the reader stopped at the floor.
fn exit_failure(work: &Path, probe: Option<super::space::Probe>) -> Failure {
    match super::space::Floor::new(work, probe).and_then(|floor| floor.room()) {
        Ok(room) if room < super::space::WRITE_CHUNK as u64 => Failure::NoSpace,
        Ok(_) => Failure::Unpack,
        Err(failure) => failure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn artifact() -> ArtifactRecord {
        ArtifactRecord {
            id: "@korri:fex/rootfs".into(),
            version: "v1".into(),
            files: vec![],
            unpack: None,
            title: None,
        }
    }
    #[test]
    fn unpack_unit_has_one_writable_directory_and_no_network_or_host_secrets() {
        let args = unit_arguments(
            Path::new("/storage/cache/fex/rootfs"),
            Path::new("/private"),
            &name(&artifact()),
            None,
        )
        .unwrap();
        assert!(!args.iter().any(|arg| arg.starts_with("--property=Memory")));
        assert_eq!(
            args.iter()
                .filter(|arg| arg.starts_with("--property=ReadWritePaths="))
                .collect::<Vec<_>>(),
            ["--property=ReadWritePaths=/storage/cache/fex/rootfs"]
        );
        assert!(args.contains(&"--property=PrivateNetwork=yes".into()));
        assert!(args.contains(&"--unit=korri-acquire-fex-rootfs.service".into()));
        assert!(args
            .contains(&"--property=InaccessiblePaths=/private /run /tmp /var/tmp /dev/shm".into()));
        assert!(unit_arguments(
            Path::new("/storage/other path"),
            Path::new("/private"),
            &name(&artifact()),
            None,
        )
        .is_err());
    }
    #[test]
    fn a_store_job_unit_is_named_from_its_provider_cache_path() {
        assert_eq!(
            provider_name("@simonwjackson:roms", "@simonwjackson:wowroms"),
            "korri-acquire-roms-wowroms.service"
        );
        let args = unit_arguments(
            Path::new("/storage/cache/roms/wowroms"),
            Path::new("/private"),
            &provider_name("@simonwjackson:roms", "@simonwjackson:wowroms"),
            Some(ReaderLimits {
                memory: 123_456_789,
            }),
        )
        .unwrap();
        // Review P1-2: a hard memory cap from MemAvailable, no swap, so any
        // decoder that overreaches is killed inside the sandbox.
        assert!(args.contains(&"--property=MemoryMax=123456789".into()));
        assert!(args.contains(&"--property=MemorySwapMax=0".into()));
        assert!(args.contains(&"--unit=korri-acquire-roms-wowroms.service".into()));
        assert!(args.contains(&"--property=ReadWritePaths=/storage/cache/roms/wowroms".into()));
        assert!(args.contains(&"--property=PrivateNetwork=yes".into()));
    }
    #[test]
    fn a_failed_reader_is_no_space_only_at_the_floor() {
        let work = tempfile::tempdir().unwrap();
        assert_eq!(
            exit_failure(work.path(), Some(super::super::space::fixed(1024))),
            Failure::NoSpace
        );
        assert_eq!(
            exit_failure(work.path(), Some(super::super::space::fixed(1 << 20))),
            Failure::Unpack
        );
    }
    #[test]
    fn orphan_recovery_stops_exact_unit_and_refuses_cleanup_when_stop_fails() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let helper = root.path().join("systemctl");
        let reply = root.path().join("reply");
        let calls = root.path().join("calls");
        for (stop, expected) in [
            ("exit 1".to_owned(), false),
            (
                format!("printf 'LoadState=not-found\\n' > '{}'", reply.display()),
                true,
            ),
        ] {
            fs::write(&reply, "LoadState=loaded\nActiveState=active\n").unwrap();
            fs::write(&calls, "").unwrap();
            fs::write(&helper, format!("#!/bin/sh\nprintf '%s\\n' \"$3 $4\" >> '{}'\ncase \"$3\" in\nshow) cat '{}' ;;\nstop) {stop} ;;\nesac\n", calls.display(), reply.display())).unwrap();
            fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(reclaim_with(&helper, &name(&artifact())).is_ok(), expected);
            let calls = fs::read_to_string(&calls).unwrap();
            assert!(calls.contains("stop korri-acquire-fex-rootfs.service"));
            assert_eq!(calls.lines().count(), if expected { 3 } else { 2 });
        }
    }
}
