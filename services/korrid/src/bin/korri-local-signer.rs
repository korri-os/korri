//! The local signer service (`local_signer.rs`). It keeps no state on disk:
//! a person key lives only in its memory, so it also turns off core dumps.
use std::{os::fd::FromRawFd, path::PathBuf, sync::Arc};

use korrid::local_signer::{serve_connection, LocalSigner};

fn required_unprivileged_id(name: &str) -> Result<u32, String> {
    let value = std::env::var(name).map_err(|_| format!("{name} must be set"))?;
    let id = value
        .parse::<u32>()
        .map_err(|_| format!("{name} must be a numeric ID"))?;
    if id == 0 {
        return Err(format!("{name} must identify an unprivileged account"));
    }
    Ok(id)
}

fn inherited_listener() -> Result<std::os::unix::net::UnixListener, String> {
    let count = std::env::var("LISTEN_FDS")
        .map_err(|_| "LISTEN_FDS must be set".to_owned())?
        .parse::<u32>()
        .map_err(|_| "LISTEN_FDS must be a number".to_owned())?;
    let pid = std::env::var("LISTEN_PID")
        .map_err(|_| "LISTEN_PID must be set".to_owned())?
        .parse::<u32>()
        .map_err(|_| "LISTEN_PID must be a number".to_owned())?;
    if count != 1 || pid != std::process::id() {
        return Err(
            "local signer requires exactly one inherited listener for its current PID".into(),
        );
    }
    let listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(3) };
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("could not configure inherited listener: {error}"))?;
    Ok(listener)
}

fn expected_device_public_key() -> Result<String, String> {
    let directory = std::env::var_os("CREDENTIALS_DIRECTORY")
        .map(PathBuf::from)
        .ok_or_else(|| "CREDENTIALS_DIRECTORY must be set".to_owned())?;
    let path = directory.join("expected-device-public-key");
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| "expected device public key credential is unavailable".to_owned())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 128 {
        return Err("expected device public key credential is invalid".into());
    }
    std::fs::read_to_string(path)
        .map_err(|_| "expected device public key credential is unreadable".to_owned())
}

fn disable_core_dumps() -> Result<(), String> {
    let no_core = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: a valid resource and a live plain rlimit struct for this process.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &no_core) } != 0 {
        return Err("local signer could not turn off core dumps".into());
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    disable_core_dumps().unwrap_or_else(|error| panic!("{error}"));
    let expected_uid = required_unprivileged_id("KORRI_LOCAL_SIGNER_PEER_UID")
        .unwrap_or_else(|error| panic!("{error}"));
    let expected_gid = required_unprivileged_id("KORRI_LOCAL_SIGNER_PEER_GID")
        .unwrap_or_else(|error| panic!("{error}"));
    let signer = Arc::new(
        LocalSigner::new(&expected_device_public_key().unwrap_or_else(|error| panic!("{error}")))
            .unwrap_or_else(|error| panic!("expected device public key: {error}")),
    );
    let listener = tokio::net::UnixListener::from_std(
        inherited_listener().unwrap_or_else(|error| panic!("{error}")),
    )
    .expect("adopt local signer listener");
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .expect("install SIGINT handler");

    loop {
        tokio::select! {
            _ = terminate.recv() => break,
            _ = interrupt.recv() => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted.expect("accept local signer connection");
                let authorized = stream.peer_cred().is_ok_and(|credentials| {
                    credentials.uid() == expected_uid && credentials.gid() == expected_gid
                });
                if authorized {
                    let signer = Arc::clone(&signer);
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, signer).await;
                    });
                }
            }
        }
    }
}
