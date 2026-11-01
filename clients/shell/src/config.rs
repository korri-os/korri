//! How the shell finds its korrid: exactly as `clients/linux` gives the
//! portal its binding (contract 2 and 6).
//!
//! | Input | Producer |
//! | --- | --- |
//! | `CREDENTIALS_DIRECTORY/KORRID_RPC_CAPABILITY` | systemd `LoadCredential` |
//! | `KORRID_ADDRESS` | korrid's socket address; only its nonzero port is used, on `127.0.0.1` |
//! | `KORRID_PORTAL_ORIGIN` | korrid's allowed origin; the shell sends it as the WebSocket `Origin` |
//!
//! The capability never enters a URL, file, log, argument or the child
//! environment. It is held in zeroising memory, compared nowhere, and
//! formatted only into the `Authorization` header and the first WebSocket
//! frame. `Debug` redacts it.
//!
//! The credential checks are a port of `clients/linux/src/main.rs`
//! (`credential_is_private`, `credential_acl_is_private`); keep the two equal.

use std::{
    env, fmt, fs,
    io::{self, Read},
    net::SocketAddr,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
};
use zeroize::Zeroizing;

pub type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone)]
pub struct Connection {
    pub port: u16,
    pub origin: String,
    capability: Zeroizing<String>,
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("port", &self.port)
            .field("origin", &self.origin)
            .field("capability", &"<redacted>")
            .finish()
    }
}

impl Connection {
    /// For tests and the fixture korrid: an explicit binding.
    pub fn new(port: u16, origin: String, capability: String) -> Self {
        Self {
            port,
            origin,
            capability: Zeroizing::new(capability),
        }
    }

    pub fn from_environment() -> Result<Self> {
        let directory = env::var_os("CREDENTIALS_DIRECTORY").ok_or("systemd credentials are required")?;
        let capability = read_credential(&Path::new(&directory).join("KORRID_RPC_CAPABILITY"))?;
        let address: SocketAddr = env::var("KORRID_ADDRESS")
            .map_err(|_| "KORRID_ADDRESS is required")?
            .parse()
            .map_err(|_| "KORRID_ADDRESS must be a socket address")?;
        if address.port() == 0 {
            return Err("korrid must have a nonzero port");
        }
        let origin = env::var("KORRID_PORTAL_ORIGIN").map_err(|_| "KORRID_PORTAL_ORIGIN is required")?;
        validate_origin(&origin)?;
        Ok(Self {
            port: address.port(),
            origin,
            capability,
        })
    }

    /// `Bearer <capability>`, for the Authorization header and the first
    /// WebSocket frame.
    pub fn bearer(&self) -> Zeroizing<String> {
        Zeroizing::new(format!("Bearer {}", self.capability.as_str()))
    }
}

/// Remove the binding names from this process's environment, so nothing the
/// shell starts can inherit them.
pub fn scrub_environment() {
    for name in ["KORRID_RPC_CAPABILITY", "CREDENTIALS_DIRECTORY"] {
        env::remove_var(name);
    }
}

fn validate_origin(origin: &str) -> Result<()> {
    let address: SocketAddr = origin
        .strip_prefix("http://")
        .ok_or("the portal origin must use local HTTP")?
        .parse()
        .map_err(|_| "the portal origin must identify the local HTTP server")?;
    if address.ip() != std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        || address.port() == 0
        || origin != format!("http://{address}")
    {
        return Err("the portal origin must be a canonical 127.0.0.1 HTTP origin");
    }
    Ok(())
}

pub fn read_credential(path: &Path) -> Result<Zeroizing<String>> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "cannot open the RPC credential")?;
    if !credential_is_private(&file).map_err(|_| "cannot inspect the RPC credential")? {
        return Err("the RPC credential must be a private regular file accessible only to root and this service");
    }
    let mut capability = Zeroizing::new(String::new());
    file.take(4097)
        .read_to_string(&mut capability)
        .map_err(|_| "cannot read the RPC credential")?;
    if capability.ends_with('\n') {
        capability.pop();
    }
    if capability.is_empty() || capability.len() > 4096 || !capability.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        return Err("the RPC credential is invalid");
    }
    Ok(capability)
}

fn credential_is_private(file: &fs::File) -> io::Result<bool> {
    let metadata = file.metadata()?;
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_file() || (metadata.uid() != 0 && metadata.uid() != uid) {
        return Ok(false);
    }
    // Linux POSIX ACL xattrs use a version header and eight-byte entries.
    // Bound the read to systemd's observed five-entry credential ACL. Extra
    // entries, unsupported shapes and xattr errors fail closed.
    let mut acl = [0_u8; 44];
    let size = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            c"system.posix_acl_access".as_ptr(),
            acl.as_mut_ptr().cast(),
            acl.len(),
        )
    };
    if size < 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ENODATA | libc::EOPNOTSUPP) => Ok(metadata.uid() == uid && metadata.mode() & 0o077 == 0),
            Some(libc::ERANGE) => Ok(false),
            _ => Err(error),
        };
    }
    Ok(metadata.mode() & 0o777 == 0o440 && credential_acl_is_private(&acl[..size as usize], uid))
}

fn credential_acl_is_private(acl: &[u8], uid: u32) -> bool {
    // systemd 258 keeps ownership with root and grants the consumer read
    // access with a named-user ACL. Mode 0440's group bits are its mask.
    if acl.len() != 44 || acl[..4] != 2_u32.to_le_bytes() {
        return false;
    }
    let expected = [
        (1_u16, 4_u16, u32::MAX), // owner
        (2, 4, uid),              // consuming user
        (4, 0, u32::MAX),         // owning group: no access
        (16, 4, u32::MAX),        // mask: read only
        (32, 0, u32::MAX),        // other: no access
    ];
    acl[4..].chunks_exact(8).zip(expected).all(|(entry, (tag, permission, id))| {
        entry[..2] == tag.to_le_bytes() && entry[2..4] == permission.to_le_bytes() && entry[4..] == id.to_le_bytes()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn reads_a_private_credential_and_rejects_shared_ones() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("KORRID_RPC_CAPABILITY");
        fs::write(&path, "abc123\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_credential(&path).unwrap().as_str(), "abc123");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_credential(&path).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, "two words").unwrap();
        assert!(read_credential(&path).is_err());
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_credential(&link).is_err());
    }

    #[test]
    fn origin_must_be_canonical_loopback() {
        assert!(validate_origin("http://127.0.0.1:8080").is_ok());
        assert!(validate_origin("http://localhost:8080").is_err());
        assert!(validate_origin("https://127.0.0.1:8080").is_err());
        assert!(validate_origin("http://127.0.0.1:0").is_err());
    }

    #[test]
    fn debug_never_shows_the_capability() {
        let connection = Connection::new(1, "http://127.0.0.1:1".into(), "secret-value".into());
        assert!(!format!("{connection:?}").contains("secret-value"));
    }
}
