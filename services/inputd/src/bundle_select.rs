use std::{
    ffi::OsString,
    fs::{self, File},
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{ExitCode, Output},
    time::Duration,
};

use korri_inputd::bundle::{is_inside_store_item, resolve_bundle};
use tokio::{process::Command, time::Instant};

#[path = "bundle_select/offline.rs"]
mod offline;

const STATE_ROOT: &str = "/nix/var/nix/gcroots/korri-bundle";
const STORE_ROOT: &str = "/nix/store";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_POLL: Duration = Duration::from_millis(250);
const UNITS: [&str; 3] = [
    "inputplumber.service",
    "korri-inputd.service",
    "korrid.service",
];
const OPTIONAL_INPUT_SEAT_UNIT: &str = "korri-input-seat-receiver.service";

#[tokio::main]
async fn main() -> ExitCode {
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("korri-bundle-select: root is required");
        return ExitCode::FAILURE;
    }
    match run(std::env::args_os().collect()).await {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("korri-bundle-select: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(arguments: Vec<OsString>) -> Result<String, String> {
    let command = arguments
        .get(1)
        .and_then(|value| value.to_str())
        .ok_or_else(usage)?;
    // Offline selection must not create or chmod the root, and must never enter
    // the normal service restart / automatic fallback path below.
    if command == "offline-select" {
        if arguments.len() != 5 || arguments[4] != "--acknowledge-exclusive-quiescence" {
            return Err(usage());
        }
        return offline::select(&path_argument(&arguments, 2)?, &path_argument(&arguments, 3)?)
            .map_err(|error| format!("{error}; keep consumers stopped; no rollback attempted"));
    }
    let state_root = Path::new(STATE_ROOT);
    let store_root = Path::new(STORE_ROOT);
    prepare_state_root(state_root, true)?;
    match command {
        "initialize" => {
            let bundle = one_path_argument(&arguments, 2)?;
            let active = state_root.join("active");
            if active.symlink_metadata().is_ok() {
                validate_owned_selector(&active, store_root, true)?;
                return Ok(format!("active={}", canonical_text(&active)?));
            }
            set_selector(&active, &bundle, store_root)?;
            Ok(format!("active={}", canonical_text(&active)?))
        }
        "switch" => {
            let bundle = path_argument(&arguments, 2)?;
            let systemctl = path_argument(&arguments, 3)?;
            verify_systemctl(&systemctl, store_root)?;
            switch_bundle(state_root, &bundle, &systemctl, store_root).await?;
            Ok(format!(
                "active={}",
                canonical_text(&state_root.join("active"))?
            ))
        }
        "rollback" => {
            if arguments.len() != 3 {
                return Err(usage());
            }
            let systemctl = path_argument(&arguments, 2)?;
            verify_systemctl(&systemctl, store_root)?;
            rollback_bundle(state_root, &systemctl, store_root).await?;
            Ok(format!(
                "active={}",
                canonical_text(&state_root.join("active"))?
            ))
        }
        "status" if arguments.len() == 2 => {
            validate_owned_selector(&state_root.join("active"), store_root, true)?;
            Ok(format!(
                "active={}",
                canonical_text(&state_root.join("active"))?
            ))
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: korri-bundle-select initialize <bundle> | switch <bundle> <systemctl> | rollback <systemctl> | status | offline-select <expected-current> <new-bundle> --acknowledge-exclusive-quiescence".into()
}

fn path_argument(arguments: &[OsString], index: usize) -> Result<PathBuf, String> {
    arguments.get(index).map(PathBuf::from).ok_or_else(usage)
}

fn one_path_argument(arguments: &[OsString], index: usize) -> Result<PathBuf, String> {
    if arguments.len() != index + 1 {
        return Err(usage());
    }
    path_argument(arguments, index)
}

fn prepare_state_root(state_root: &Path, require_root_owner: bool) -> Result<(), String> {
    fs::create_dir_all(state_root)
        .map_err(|error| format!("could not create bundle state directory: {error}"))?;
    let link_metadata = fs::symlink_metadata(state_root)
        .map_err(|error| format!("could not inspect bundle state directory: {error}"))?;
    if !link_metadata.file_type().is_dir() {
        return Err("bundle state path must be a directory, not a symbolic link".into());
    }
    fs::set_permissions(state_root, fs::Permissions::from_mode(0o711))
        .map_err(|error| format!("could not protect bundle state directory: {error}"))?;
    let metadata = fs::metadata(state_root)
        .map_err(|error| format!("could not inspect bundle state directory: {error}"))?;
    if require_root_owner && metadata.uid() != 0 {
        return Err("bundle state directory must be owned by root".into());
    }
    Ok(())
}

fn validate_owned_selector(
    selector: &Path,
    store_root: &Path,
    require_root_owner: bool,
) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(selector)
        .map_err(|error| format!("bundle selector is unavailable: {error}"))?;
    if !metadata.file_type().is_symlink() {
        return Err("bundle selector must be a symbolic link".into());
    }
    if require_root_owner && metadata.uid() != 0 {
        return Err("bundle selector must be owned by root".into());
    }
    resolve_bundle(selector, store_root)
}

fn set_selector(selector: &Path, bundle: &Path, store_root: &Path) -> Result<(), String> {
    let bundle = resolve_bundle(bundle, store_root)?;
    let parent = selector
        .parent()
        .ok_or_else(|| "bundle selector has no parent directory".to_owned())?;
    let temporary = parent.join(format!(
        ".{}.{}",
        selector
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("selector"),
        std::process::id()
    ));
    let _ = fs::remove_file(&temporary);
    symlink(&bundle, &temporary)
        .map_err(|error| format!("could not create temporary bundle selector: {error}"))?;
    fs::rename(&temporary, selector)
        .map_err(|error| format!("could not activate bundle selector: {error}"))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("could not sync bundle selector directory: {error}"))?;
    Ok(())
}

async fn switch_bundle(
    state_root: &Path,
    requested: &Path,
    systemctl: &Path,
    store_root: &Path,
) -> Result<(), String> {
    resolve_bundle(requested, store_root)?;
    let active = state_root.join("active");
    let previous = state_root.join("previous");
    let current = validate_owned_selector(&active, store_root, true)?;
    let requested = fs::canonicalize(requested)
        .map_err(|error| format!("requested bundle is unavailable: {error}"))?;
    if current == requested {
        return Ok(());
    }
    set_selector(&previous, &current, store_root)?;
    set_selector(&active, &requested, store_root)?;
    if let Err(error) = restart_and_wait(systemctl).await {
        set_selector(&active, &current, store_root)?;
        let rollback = restart_and_wait(systemctl).await;
        return match rollback {
            Ok(()) => Err(format!("candidate services failed; prior bundle restored: {error}")),
            Err(rollback_error) => Err(format!(
                "candidate services failed and prior bundle restart failed: {error}; {rollback_error}"
            )),
        };
    }
    Ok(())
}

async fn rollback_bundle(
    state_root: &Path,
    systemctl: &Path,
    store_root: &Path,
) -> Result<(), String> {
    let active = state_root.join("active");
    let previous = state_root.join("previous");
    let current = validate_owned_selector(&active, store_root, true)?;
    let prior = validate_owned_selector(&previous, store_root, true)?;
    set_selector(&active, &prior, store_root)?;
    if let Err(error) = restart_and_wait(systemctl).await {
        set_selector(&active, &current, store_root)?;
        let restored = restart_and_wait(systemctl).await;
        return match restored {
            Ok(()) => Err(format!("rollback services failed; current bundle restored: {error}")),
            Err(restore_error) => Err(format!(
                "rollback services failed and current bundle restart failed: {error}; {restore_error}"
            )),
        };
    }
    set_selector(&previous, &current, store_root)?;
    Ok(())
}

fn verify_systemctl(systemctl: &Path, store_root: &Path) -> Result<(), String> {
    let executable = fs::canonicalize(systemctl)
        .map_err(|error| format!("systemctl is unavailable: {error}"))?;
    if !is_inside_store_item(&executable, store_root) {
        return Err("systemctl must be an immutable store executable".into());
    }
    let metadata =
        fs::metadata(&executable).map_err(|error| format!("systemctl is unavailable: {error}"))?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err("systemctl must be a regular executable".into());
    }
    Ok(())
}

async fn restart_and_wait(systemctl: &Path) -> Result<(), String> {
    restart_and_wait_with(systemctl, HEALTH_TIMEOUT, HEALTH_POLL).await
}

async fn restart_and_wait_with(
    systemctl: &Path,
    health_timeout: Duration,
    health_poll: Duration,
) -> Result<(), String> {
    let mut units = UNITS.to_vec();
    if run_systemctl(
        systemctl,
        ["is-enabled", "--quiet", OPTIONAL_INPUT_SEAT_UNIT],
    )
    .await?
    {
        units.push(OPTIONAL_INPUT_SEAT_UNIT);
    }
    run_systemctl(
        systemctl,
        ["--no-block", "restart"]
            .into_iter()
            .chain(units.iter().copied()),
    )
    .await
    .and_then(|success| {
        success
            .then_some(())
            .ok_or_else(|| "service restart was rejected".into())
    })?;
    let deadline = Instant::now() + health_timeout;
    loop {
        let mut all_active = true;
        for unit in &units {
            if !run_systemctl(systemctl, ["is-active", "--quiet", unit]).await? {
                all_active = false;
                break;
            }
        }
        if all_active {
            let status = run_systemctl_output(
                systemctl,
                [
                    "show",
                    "--property=StatusText",
                    "--value",
                    "korri-inputd.service",
                ],
            )
            .await?;
            if status.status.success()
                && String::from_utf8_lossy(&status.stdout).trim_end() == "Ready"
            {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err("Korri services did not become active before the deadline".into());
        }
        tokio::time::sleep(health_poll).await;
    }
}

async fn run_systemctl<I, S>(systemctl: &Path, arguments: I) -> Result<bool, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new(systemctl);
    command.args(arguments);
    let status = tokio::time::timeout(COMMAND_TIMEOUT, command.status())
        .await
        .map_err(|_| "systemctl operation exceeded its deadline".to_owned())?
        .map_err(|error| format!("systemctl operation failed: {error}"))?;
    Ok(status.success())
}

async fn run_systemctl_output<I, S>(systemctl: &Path, arguments: I) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new(systemctl);
    command.args(arguments);
    tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| "systemctl operation exceeded its deadline".to_owned())?
        .map_err(|error| format!("systemctl operation failed: {error}"))
}

fn canonical_text(path: &Path) -> Result<String, String> {
    fs::canonicalize(path)
        .map_err(|error| format!("bundle selector is unavailable: {error}"))?
        .into_os_string()
        .into_string()
        .map_err(|_| "bundle selector is not valid UTF-8".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn bundle(store: &Path, name: &str) -> PathBuf {
        let bundle = store.join(name);
        let package = store.join(format!("{name}-packages"));
        fs::create_dir_all(bundle.join("bin")).unwrap();
        fs::create_dir_all(bundle.join("share")).unwrap();
        fs::create_dir_all(package.join("bin")).unwrap();
        fs::create_dir_all(package.join("share/inputplumber/profiles")).unwrap();
        for component in [
            "inputplumber",
            "korri-inputd",
            "korri-input-seat-receiver",
            "korrid",
        ] {
            let executable = package.join("bin").join(component);
            fs::write(&executable, b"fixture").unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o555)).unwrap();
            symlink(&executable, bundle.join("bin").join(component)).unwrap();
        }
        symlink(
            package.join("share/inputplumber"),
            bundle.join("share/inputplumber"),
        )
        .unwrap();
        let profile = package
            .join("share/inputplumber/profiles")
            .join(korri_inputd::bundle::INPUT_PROFILE_NAME);
        fs::write(&profile, b"profile").unwrap();
        symlink(
            &profile,
            bundle.join(korri_inputd::bundle::INPUT_PROFILE_BUNDLE_PATH),
        )
        .unwrap();
        bundle
    }

    #[test]
    fn state_root_rejects_a_symbolic_link() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let state = root.path().join("state");
        fs::create_dir(&target).unwrap();
        symlink(&target, &state).unwrap();

        let error = prepare_state_root(&state, false).unwrap_err();

        assert_eq!(
            error,
            "bundle state path must be a directory, not a symbolic link"
        );
    }

    #[test]
    fn state_root_is_traversable_without_granting_write_access() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");

        prepare_state_root(&state, false).unwrap();

        assert_eq!(
            fs::metadata(&state).unwrap().permissions().mode() & 0o777,
            0o711
        );
    }

    #[test]
    fn initialization_does_not_replace_an_existing_selector() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let store = root.path().join("store");
        prepare_state_root(&state, false).unwrap();
        let first = bundle(&store, "first");
        let second = bundle(&store, "second");
        set_selector(&state.join("active"), &first, &store).unwrap();

        validate_owned_selector(&state.join("active"), &store, false).unwrap();

        assert_eq!(fs::canonicalize(state.join("active")).unwrap(), first);
        assert_ne!(fs::canonicalize(state.join("active")).unwrap(), second);
    }

    #[test]
    fn selector_switch_records_the_previous_bundle() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let store = root.path().join("store");
        prepare_state_root(&state, false).unwrap();
        let first = bundle(&store, "first");
        let second = bundle(&store, "second");
        set_selector(&state.join("active"), &first, &store).unwrap();

        let current = validate_owned_selector(&state.join("active"), &store, false).unwrap();
        set_selector(&state.join("previous"), &current, &store).unwrap();
        set_selector(&state.join("active"), &second, &store).unwrap();

        assert_eq!(fs::canonicalize(state.join("active")).unwrap(), second);
        assert_eq!(fs::canonicalize(state.join("previous")).unwrap(), first);
    }

    fn fake_systemctl(root: &Path, status: &str) -> PathBuf {
        let script = root.join(format!("systemctl-{status}"));
        let shell = std::env::var("SHELL").unwrap();
        fs::write(
            &script,
            format!("#!{shell}\n[ \"$1\" != --no-block ] || shift\ncase \"$1\" in\n  restart) exit 0 ;;\n  is-active) exit 0 ;;\n  show) printf '%s\n' '{status}'; exit 0 ;;\nesac\nexit 1\n"),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o555)).unwrap();
        script
    }

    #[tokio::test]
    async fn active_inputd_must_publish_ready() {
        let root = tempfile::tempdir().unwrap();
        let ready = fake_systemctl(root.path(), "Ready");
        restart_and_wait_with(&ready, Duration::from_secs(1), Duration::from_millis(1))
            .await
            .unwrap();
        for status in ["Missing", "Ambiguous", "Recovering"] {
            let script = fake_systemctl(root.path(), status);
            assert!(restart_and_wait_with(
                &script,
                Duration::from_millis(15),
                Duration::from_millis(1)
            )
            .await
            .unwrap_err()
            .contains("did not become active"));
        }
    }
}
