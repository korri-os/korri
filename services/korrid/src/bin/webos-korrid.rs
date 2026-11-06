//! webOS composition: the existing embedded brain, with no local plugins,
//! signer, system service, reach listener or stream viewer. M2 lists peers'
//! libraries over signed HTTP. Playback is not implemented by this edge.
use std::{
    ffi::OsString,
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

// Explicit M2 allocation chosen after the operator inspected the jail mounts.
// Never use HOME (the installed, root-owned app) or the disposable jail /tmp.
const STATE_ROOT: &str = "/media/developer/org.korri.shell";
const ORIGIN: &str = "https://shell.korri.example";
const HELP: &str = "usage: webos-korrid --credentials DIR | identity {status|owner-binding-request|import --stdin|import --file PATH} | setup finish
Hidden dev/operator tools: identity and setup finish. Run only while the app is stopped.
setup finish: Finish setup, keeping the existing identity; no new identity is claimed. This does not test a controller.";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Serve(PathBuf),
    Identity(Vec<OsString>),
    SetupFinish,
}

fn command(args: &[OsString]) -> Result<Command, String> {
    match args {
        [flag, directory] if flag == "--credentials" => Ok(Command::Serve(directory.into())),
        [setup, finish] if setup == "setup" && finish == "finish" => Ok(Command::SetupFinish),
        [identity, operation]
            if identity == "identity"
                && (operation == "status" || operation == "owner-binding-request") =>
        {
            Ok(Command::Identity(vec![operation.clone()]))
        }
        [identity, operation, source]
            if identity == "identity" && operation == "import" && source == "--stdin" =>
        {
            Ok(Command::Identity(vec![operation.clone(), source.clone()]))
        }
        [identity, operation, flag, path]
            if identity == "identity" && operation == "import" && flag == "--file" =>
        {
            Ok(Command::Identity(vec![
                operation.clone(),
                flag.clone(),
                path.clone(),
            ]))
        }
        _ => Err(HELP.into()),
    }
}

fn validate_ancestors(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::other("path must be absolute"));
    }
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::other("state ancestors must be real directories"));
        }
    }
    Ok(())
}

fn private_directory(path: &Path, create: bool) -> io::Result<()> {
    validate_ancestors(
        path.parent()
            .ok_or_else(|| io::Error::other("missing parent"))?,
    )?;
    if create {
        match DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => File::open(path.parent().expect("parent checked"))?.sync_all()?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    // SAFETY: geteuid has no arguments or preconditions.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o7777 != 0o700
    {
        return Err(io::Error::other(
            "private directory must be real, caller-owned and mode 0700",
        ));
    }
    Ok(())
}

struct State {
    private: PathBuf,
    storage: PathBuf,
    // The same edge holds this directory lock during serving AND offline tools.
    // No lock-file schema. Power loss releases the lock without stale PIDs.
    _lock: File,
}

impl State {
    fn open(root: &Path) -> io::Result<Self> {
        private_directory(root, true)?;
        let lock = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(root)?;
        // SAFETY: lock is an open directory fd; flock does not retain pointers.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::other(
                "webOS korrid is already using this state; stop the app before offline tools",
            ));
        }
        let private = root.join("private");
        let storage = root.join("storage");
        private_directory(&private, true)?;
        private_directory(&storage, true)?;
        Ok(Self {
            private,
            storage,
            _lock: lock,
        })
    }
}

fn write_credential(directory: &Path, capability: &str) -> io::Result<()> {
    private_directory(directory, false)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join("KORRID_RPC_CAPABILITY"))?;
    file.write_all(capability.as_bytes())?;
    file.sync_all()
}

fn finish_setup(root: &Path) -> Result<(), String> {
    match korrid::onboarding::finish(root) {
        korrid::onboarding::OnboardingStatusOutcome::Ok(status) if !status.needed => Ok(()),
        korrid::onboarding::OnboardingStatusOutcome::Err(failure) => Err(failure.message),
        _ => Err("Could not finish setup. Try again.".into()),
    }
}

struct Running;
impl Drop for Running {
    fn drop(&mut self) {
        let _ = korrid::stop_local_server();
    }
}

fn start(
    state: &State,
    credentials: &Path,
    relays: Option<korrid::relay::RelayList>,
) -> Result<Running, String> {
    private_directory(credentials, false).map_err(|error| error.to_string())?;
    let mut platform = korrid::EmbeddedPlatform::default();
    platform.relays = relays;
    let port = korrid::start_local_server(
        ORIGIN,
        state.storage.to_str().ok_or("storage path must be UTF-8")?,
        state.private.to_str().ok_or("private path must be UTF-8")?,
        platform,
    )
    .map_err(|error| error.to_string())?;
    let running = Running;
    let capability =
        zeroize::Zeroizing::new(korrid::local_server_capability().ok_or("missing capability")?);
    write_credential(credentials, &capability).map_err(|error| error.to_string())?;
    println!("KORRID_ADDRESS=127.0.0.1:{port}");
    println!("KORRID_SHELL_ORIGIN={ORIGIN}");
    io::stdout().flush().map_err(|error| error.to_string())?;
    Ok(running)
}

async fn run() -> Result<(), String> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments == [OsString::from("--help")] {
        println!("{HELP}");
        return Ok(());
    }
    let command = command(&arguments)?;
    // SAFETY: geteuid and setrlimit use valid arguments and no retained pointers.
    if unsafe { libc::geteuid() } == 0 {
        return Err("webos-korrid refuses root".into());
    }
    let no_core = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &no_core) } != 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    let state = State::open(Path::new(STATE_ROOT)).map_err(|error| error.to_string())?;
    match command {
        Command::Identity(arguments) => {
            // Exact existing bounded request/import implementation and verifier.
            println!(
                "{}",
                korrid::identity_cli::run_from_environment(&arguments, &state.private)?
            );
        }
        Command::SetupFinish => {
            finish_setup(&state.private)?;
            println!("Setup: finished");
        }
        Command::Serve(credentials) => {
            let relays = std::env::var("KORRID_RELAYS")
                .map(Some)
                .or_else(|error| match error {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => Err("KORRID_RELAYS must be UTF-8"),
                })?;
            let relays = korrid::relay::RelayList::from_linux_environment(relays.as_deref())
                .map_err(|error| error.to_string())?;
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .map_err(|error| error.to_string())?;
            let mut hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
                .map_err(|error| error.to_string())?;
            let _running = start(&state, &credentials, relays)?;
            tokio::select! {
                _ = terminate.recv() => {},
                _ = hangup.recv() => {},
                result = tokio::signal::ctrl_c() => result.map_err(|error| error.to_string())?,
            }
        }
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("webos-korrid: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "webos-korrid/tests.rs"]
mod tests;
