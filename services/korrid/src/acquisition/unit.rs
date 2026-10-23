//! Transient unpack units have deterministic contribution ownership, so a
//! restarted standalone daemon can stop an orphan before cleaning its work.
use super::Failure;
use crate::plugin::ArtifactRecord;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

/// The programs that start and stop units. Tests use fakes.
#[derive(Clone)]
pub(super) struct Programs {
    pub systemd_run: PathBuf,
    pub systemctl: PathBuf,
}
impl Programs {
    #[cfg(not(test))]
    fn device() -> Self {
        Self {
            systemd_run: std::env::var_os("KORRID_SYSTEMD_RUN")
                .map(PathBuf::from)
                .unwrap_or_else(|| "systemd-run".into()),
            systemctl: systemctl_path(),
        }
    }
}

/// Review P1-3: launch and cancel of one store reader are mutually
/// exclusive. The worker starts the unit under the lock only when no cancel
/// came first; a cancel under the same lock either prevents the start or
/// reports that the unit must be stopped (`stop_launched`).
#[derive(Default)]
pub(super) struct Launch {
    state: Mutex<LaunchState>,
    /// Read by the in-process test reader between writes.
    cancelled: Arc<AtomicBool>,
    done: AtomicBool,
}
#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum LaunchState {
    #[default]
    Waiting,
    Started,
    Cancelled,
}
impl Launch {
    fn start<T>(&self, start: impl FnOnce() -> std::io::Result<T>) -> Result<T, Failure> {
        let mut state = self.state.lock().expect("launch mutex poisoned");
        if *state == LaunchState::Cancelled {
            return Err(Failure::Cancelled);
        }
        let started = start()?;
        *state = LaunchState::Started;
        Ok(started)
    }
    /// True when the unit was launched and must be stopped.
    pub(super) fn cancel(&self) -> bool {
        let mut state = self.state.lock().expect("launch mutex poisoned");
        self.cancelled.store(true, Ordering::SeqCst);
        if *state == LaunchState::Started {
            true
        } else {
            *state = LaunchState::Cancelled;
            false
        }
    }
    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    /// The worker is past the unit: it has exited or never started.
    pub(super) fn done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
}

/// Marks the launch done on every exit of the worker.
struct Finished<'a>(&'a Launch);
impl Drop for Finished<'_> {
    fn drop(&mut self) {
        self.0.done.store(true, Ordering::SeqCst);
    }
}

/// After `Launch::cancel` said the unit was launched: stop it until the
/// worker has seen it exit. `systemd-run` may not have registered the unit
/// at the first stop, so korrid tries again every 50 ms. `RuntimeMaxSec=`
/// bounds the wait if systemd never answers.
pub(super) fn stop_launched(programs: &Programs, unit: &str, launch: &Launch) {
    while !launch.done() {
        if let Err(failure) = reclaim_with(&programs.systemctl, unit) {
            log::debug!("acquisition: stopping {unit}: {failure:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

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
/// `runtime` stops a stuck reader so it cannot hold the queue forever.
#[derive(Clone, Copy, Debug)]
pub(super) struct ReaderLimits {
    pub memory: u64,
    pub runtime: Duration,
}

/// The reader's runtime limit, 9000 s (2.5 h). Measured on 2026-10-23 in a
/// release build on an i9-11900H (`archive::tests::reader_throughput`, 64 MiB
/// of half-compressible data): zip 589 MiB/s, rar 5 65 MiB/s, 7z LZMA2
/// 38.7 MiB/s, the slowest. The device rate is not measured; korrid assumes
/// 20 times slower (1.9 MiB/s), which covers ADR 0016's estimate of the
/// RG353M as 4 to 6 times slower than the fuji builder with fuji up to about
/// 3 times slower than this host (inferred). 8 GiB unpacked (two DVD-size
/// ISOs) at 1.9 MiB/s is about 4300 s; a factor 2 margin gives 9000 s.
pub(super) const READER_RUNTIME: Duration = Duration::from_secs(9000);

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
        arguments.push(format!(
            "--property=RuntimeMaxSec={}",
            reader.runtime.as_secs().max(1)
        ));
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
        let mut command = sandboxed(&Programs::device(), base, private_root, &unit, None)?;
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
fn sandboxed(
    programs: &Programs,
    base: &Path,
    private_root: &Path,
    unit: &str,
    reader: Option<ReaderLimits>,
) -> Result<Command, Failure> {
    // Recovery normally stopped an orphan before creating this work dir.
    // A still-active named unit is never replaced or mistaken for this run.
    if active(&programs.systemctl, unit)? {
        while active(&programs.systemctl, unit)? {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return Err(Failure::Unpack);
    }
    let mut command = Command::new(&programs.systemd_run);
    command.args(unit_arguments(base, private_root, unit, reader)?);
    Ok(command)
}

/// The production reader: `korrid unpack-archive <work> <archive>` in the
/// sandbox unit, launched only if no cancel came first.
#[allow(clippy::too_many_arguments)]
fn run_reader(
    programs: &Programs,
    base: &Path,
    private_root: &Path,
    unit: &str,
    work: &Path,
    archive: &Path,
    korrid: &Path,
    limits: ReaderLimits,
    launch: &Launch,
) -> Result<(), Failure> {
    let _finished = Finished(launch);
    let mut command = sandboxed(programs, base, private_root, unit, Some(limits))?;
    command
        .arg(korrid)
        .arg("unpack-archive")
        .arg(work)
        .arg(archive)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let mut child = launch.start(|| command.spawn())?;
    let status = child.wait()?;
    if launch.is_cancelled() {
        return Err(Failure::Cancelled);
    }
    if status.success() {
        Ok(())
    } else {
        Err(exit_failure(work))
    }
}

/// ADR 0030: unpack a store download with korrid's own reader,
/// `korrid unpack-archive <work-dir> <file>`, in the sandbox unit. Tests run
/// the same reader in this process, where cancel stops it between writes.
pub(super) fn unpack_archive(
    base: &Path,
    private_root: &Path,
    unit: &str,
    work: &Path,
    archive: &Path,
    launch: &Launch,
) -> Result<(), Failure> {
    #[cfg(test)]
    {
        let _ = (base, private_root, unit);
        let _finished = Finished(launch);
        launch.start(|| Ok(()))?;
        let limits = super::archive::Limits {
            cancel: Some(launch.cancelled.clone()),
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
        run_reader(
            &Programs::device(),
            base,
            private_root,
            unit,
            work,
            archive,
            &std::env::current_exe()?,
            ReaderLimits {
                memory,
                runtime: READER_RUNTIME,
            },
            launch,
        )
    }
}

/// Cancel a store reader: prevent its launch, or stop the launched unit
/// and wait for the worker to see it exit.
pub(super) fn cancel_reader(unit: &str, launch: &Launch) {
    if launch.cancel() {
        #[cfg(not(test))]
        stop_launched(&Programs::device(), unit, launch);
        #[cfg(test)]
        let _ = unit;
    }
}

/// The reader exits 1 for every failure and leaves its own outcome in the
/// result marker beside the work folder. korrid reads that, not a fresh
/// floor: the reader's counted reservations can reach the floor while the
/// file system's figure has not moved. Anything unrecognised is `Unpack`.
fn exit_failure(work: &Path) -> Failure {
    let marker = super::archive::result_marker(work);
    let outcome = std::fs::read_to_string(&marker).ok();
    let _ = std::fs::remove_file(&marker);
    match outcome.as_deref().map(str::trim) {
        Some(super::archive::NO_SPACE_MARKER) => Failure::NoSpace,
        _ => Failure::Unpack,
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
                runtime: Duration::from_secs(5400),
            }),
        )
        .unwrap();
        // Review P1-2: a hard memory cap from MemAvailable, no swap, so any
        // decoder that overreaches is killed inside the sandbox.
        assert!(args.contains(&"--property=MemoryMax=123456789".into()));
        assert!(args.contains(&"--property=MemorySwapMax=0".into()));
        assert!(args.contains(&"--property=RuntimeMaxSec=5400".into()));
        assert_eq!(READER_RUNTIME.as_secs(), 9000);
        assert!(args.contains(&"--unit=korri-acquire-roms-wowroms.service".into()));
        assert!(args.contains(&"--property=ReadWritePaths=/storage/cache/roms/wowroms".into()));
        assert!(args.contains(&"--property=PrivateNetwork=yes".into()));
    }
    /// Review P2 (round 2): the reader's own outcome reaches korrid. A fake
    /// `systemd-run` plays the reader: it writes `marker` next to the work
    /// folder, as `korrid unpack-archive` does, and exits 1. korrid's
    /// production mapping reads it whatever the file system reports: on
    /// Btrfs, 100 empty files left the reported free space unchanged, so a
    /// fresh floor said Unpack.
    #[test]
    fn a_failed_reader_reports_its_own_no_space_through_the_result_marker() {
        use std::{fs, os::unix::fs::PermissionsExt};
        for (marker, expected) in [
            (Some("no-space\n"), Failure::NoSpace),
            (Some("something-else\n"), Failure::Unpack),
            (None, Failure::Unpack),
        ] {
            let root = tempfile::tempdir().unwrap();
            let mut programs = fake_systemd(root.path());
            let run = root.path().join("reader-run");
            let write = marker.map_or(String::new(), |marker| {
                format!("printf '{marker}' > \"$work.result\"\n")
            });
            fs::write(
                &run,
                format!("#!/bin/sh\nwhile [ \"$1\" != unpack-archive ]; do shift; done\nwork=\"$2\"\n{write}exit 1\n"),
            )
            .unwrap();
            fs::set_permissions(&run, fs::Permissions::from_mode(0o755)).unwrap();
            programs.systemd_run = run;
            let launch = Arc::new(Launch::default());
            let result = reader_run(root.path(), programs, launch).join().unwrap();
            assert_eq!(result, Err(expected), "{marker:?}");
            assert!(
                !root.path().join("work.result").exists(),
                "korrid removes the marker"
            );
        }
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
    /// Fake `systemd-run` and `systemctl` with a delayed unit registration:
    /// the unit appears 300 ms after systemd-run starts and "extracts" for
    /// 3 s unless it is stopped.
    fn fake_systemd(root: &Path) -> Programs {
        use std::{fs, os::unix::fs::PermissionsExt};
        let (calls, active, wrote) = (root.join("calls"), root.join("active"), root.join("wrote"));
        let run = root.join("systemd-run");
        fs::write(&run, format!("#!/bin/sh\necho spawned >> '{c}'\nsleep 0.3\ntouch '{a}'\ni=0\nwhile [ $i -lt 60 ]; do\n  [ -e '{a}' ] || exit 1\n  sleep 0.05; i=$((i+1))\ndone\ntouch '{w}'\nrm -f '{a}'\n", c = calls.display(), a = active.display(), w = wrote.display())).unwrap();
        let ctl = root.join("systemctl");
        fs::write(&ctl, format!("#!/bin/sh\ncase \"$3\" in\nshow) if [ -e '{a}' ]; then printf 'LoadState=loaded\\nActiveState=active\\n'; else printf 'LoadState=not-found\\n'; fi ;;\nstop) echo stop >> '{c}'; rm -f '{a}' ;;\nesac\n", a = active.display(), c = calls.display())).unwrap();
        for program in [&run, &ctl] {
            fs::set_permissions(program, fs::Permissions::from_mode(0o755)).unwrap();
        }
        Programs {
            systemd_run: run,
            systemctl: ctl,
        }
    }

    fn reader_run(
        root: &Path,
        programs: Programs,
        launch: Arc<Launch>,
    ) -> std::thread::JoinHandle<Result<(), Failure>> {
        let (work, archive) = (root.join("work"), root.join("archive"));
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(&archive, b"PK\x03\x04").unwrap();
        let base = root.to_path_buf();
        std::thread::spawn(move || {
            run_reader(
                &programs,
                &base,
                Path::new("/private"),
                "korri-acquire-roms-wowroms.service",
                &work,
                &archive,
                Path::new("/nix/store/korrid/bin/korrid"),
                ReaderLimits {
                    memory: 1 << 30,
                    runtime: std::time::Duration::from_secs(60),
                },
                &launch,
            )
        })
    }

    /// Review P1-3: cancel during a delayed launch stops the unit once it
    /// exists and waits for it; the reader never finishes its work.
    #[test]
    fn cancel_during_a_delayed_launch_stops_the_unit_and_waits_for_it() {
        let root = tempfile::tempdir().unwrap();
        let programs = fake_systemd(root.path());
        let launch = Arc::new(Launch::default());
        let worker = reader_run(root.path(), programs.clone(), launch.clone());
        let started = std::time::Instant::now();
        while !std::fs::read_to_string(root.path().join("calls"))
            .is_ok_and(|c| c.contains("spawned"))
        {
            assert!(started.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // The unit is not registered yet: one stop would find nothing.
        assert!(launch.cancel(), "the unit was launched");
        stop_launched(&programs, "korri-acquire-roms-wowroms.service", &launch);
        assert_eq!(worker.join().unwrap(), Err(Failure::Cancelled));
        assert!(launch.done());
        assert!(
            !root.path().join("wrote").exists(),
            "the reader never finished"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    /// Cancel before the launch: the worker never starts the unit.
    #[test]
    fn cancel_before_the_launch_prevents_it() {
        let root = tempfile::tempdir().unwrap();
        let programs = fake_systemd(root.path());
        let launch = Arc::new(Launch::default());
        assert!(!launch.cancel(), "nothing was launched yet");
        let worker = reader_run(root.path(), programs, launch.clone());
        assert_eq!(worker.join().unwrap(), Err(Failure::Cancelled));
        assert!(!root.path().join("calls").exists(), "systemd-run never ran");
    }
}
