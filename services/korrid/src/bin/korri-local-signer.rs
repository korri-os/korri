use std::{os::fd::FromRawFd, path::PathBuf, sync::Arc};

use korrid::local_signer::{publish_public_key, serve_connection, LocalPersonSigner};

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

#[tokio::main]
async fn main() {
    let root = std::env::var_os("KORRI_LOCAL_SIGNER_PRIVATE_STATE_ROOT")
        .map(PathBuf::from)
        .expect("KORRI_LOCAL_SIGNER_PRIVATE_STATE_ROOT must be set");
    let expected_uid = required_unprivileged_id("KORRI_LOCAL_SIGNER_PEER_UID")
        .unwrap_or_else(|error| panic!("{error}"));
    let expected_gid = required_unprivileged_id("KORRI_LOCAL_SIGNER_PEER_GID")
        .unwrap_or_else(|error| panic!("{error}"));
    let expected_device_public_key =
        expected_device_public_key().unwrap_or_else(|error| panic!("{error}"));
    let signer = Arc::new(
        LocalPersonSigner::load_or_create(&root, &expected_device_public_key)
            .unwrap_or_else(|error| panic!("could not open local signer: {error}")),
    );
    let public_key_path = std::env::var_os("KORRI_LOCAL_SIGNER_PUBLIC_KEY_FILE")
        .map(PathBuf::from)
        .expect("KORRI_LOCAL_SIGNER_PUBLIC_KEY_FILE must be set");
    publish_public_key(&public_key_path, &signer.public_key())
        .unwrap_or_else(|error| panic!("could not publish local signer public key: {error}"));
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
                    let signer = signer.clone();
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, signer).await;
                    });
                }
            }
        }
    }
}
