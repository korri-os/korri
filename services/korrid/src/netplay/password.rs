//! Secrets stay outside plugin evaluation and argv. systemd delivers this
//! daemon-owned source as a credential to the game's private mount namespace.
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
pub(crate) const GAME_CONFIG: &str = "/tmp/korri-netplay.cfg";
pub(crate) const CREDENTIAL: &str = "korri-netplay.conf";
pub(crate) fn create(
    root: &Path,
    player: Option<u8>,
    password: Option<&str>,
) -> std::io::Result<PathBuf> {
    let directory = root.join("netplay");
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{:032x}.cfg", rand::random::<u128>()));
    let generated = hex::encode(rand::random::<[u8; 32]>());
    let password = password.unwrap_or(&generated);
    if password.is_empty()
        || password.len() > 128
        || !password.bytes().all(|byte| byte.is_ascii_alphanumeric())
        || player == Some(0)
    {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)?;
    let result = (|| {
        writeln!(file, "netplay_password = \"{password}\"")?;
        if let Some(player) = player {
            writeln!(file, "netplay_request_device_p{player} = \"true\"")?;
        }
        file.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result.map(|()| path)
}
pub(crate) fn read(path: &Path) -> std::io::Result<String> {
    let text = fs::read_to_string(path)?;
    text.lines()
        .find_map(|line| {
            line.strip_prefix("netplay_password = \"")?
                .strip_suffix('"')
        })
        .map(str::to_owned)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidData))
}
pub(crate) fn install() -> Result<(), String> {
    let credentials =
        std::env::var_os("CREDENTIALS_DIRECTORY").ok_or("netplay credential is missing")?;
    let content = fs::read(Path::new(&credentials).join(CREDENTIAL))
        .map_err(|_| "netplay credential is unavailable")?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(GAME_CONFIG)
        .map_err(|_| "netplay config could not be created")?;
    file.write_all(&content)
        .and_then(|()| file.sync_all())
        .map_err(|_| "netplay config could not be written".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn netplay_password_is_random_private_and_never_in_plugin_input() {
        let root = tempfile::tempdir().unwrap();
        let a = create(root.path(), None, None).unwrap();
        let b = create(root.path(), Some(2), None).unwrap();
        assert_eq!(
            fs::metadata(&a).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_ne!(read(&a).unwrap(), read(&b).unwrap());
        assert_eq!(read(&a).unwrap().len(), 64);
        assert!(fs::read_to_string(b)
            .unwrap()
            .contains("netplay_request_device_p2 = \"true\""));
    }
}

pub(crate) fn remove(root: &Path, path: &Path) -> std::io::Result<()> {
    let valid = path.parent() == Some(root.join("netplay").as_path())
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.len() == 36
                    && name.ends_with(".cfg")
                    && name[..32].bytes().all(|byte| byte.is_ascii_hexdigit())
            });
    if !valid {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}
