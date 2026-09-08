use crate::{declaration::Declaration, process};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};

pub const BASE_POLICY: &str = "policy-v1: dynamic unprivileged user; read-only system; private state; private temporary files; no privilege escalation; one managed daemon; no install scripts; no host module loading";

#[derive(Serialize)]
pub struct Report {
    pub id: String,
    pub package: PathBuf,
    pub approval: String,
    pub policy: &'static str,
    pub warning: &'static str,
    pub unit: String,
    pub state_directory: String,
    pub runtime_directory: String,
    pub unit_configuration: String,
    pub declaration: Declaration,
}

pub fn validate_store_path(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("package has no parent")?;
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("invalid store item")?;
    let alphabet = b"0123456789abcdfghijklmnpqrsvwxyz";
    if parent != Path::new("/nix/store")
        || name.len() < 34
        || !name.as_bytes()[..32].iter().all(|c| alphabet.contains(c))
        || name.as_bytes()[32] != b'-'
        || name.ends_with(".drv")
        || name
            .bytes()
            .any(|c| !c.is_ascii_alphanumeric() && !b"+-._?=".contains(&c))
    {
        return Err("package must be an exact /nix/store output directory, never a derivation or installable".into());
    }
    Ok(())
}

pub fn tools(path: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|e| format!("helper is unavailable: {e}"))?;
    let relative = canonical
        .strip_prefix("/nix/store")
        .map_err(|_| "helper must be an immutable store executable")?;
    if relative.components().count() < 2
        || !canonical.is_file()
        || fs::metadata(&canonical)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
    {
        return Err("helper must be an immutable store executable".into());
    }
    Ok(canonical)
}

pub fn import(nix: &Path, source: &str, package: &Path) -> Result<(), String> {
    validate_store_path(package)?;
    if !(source.starts_with("https://")
        || source.starts_with("http://")
        || source.starts_with("file:///"))
        || source.len() > 4096
        || source.chars().any(char::is_control)
    {
        return Err("source must be an HTTP(S) or local file binary cache".into());
    }
    let package_text = package.to_str().ok_or("invalid package path")?;
    process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "--option",
            "max-jobs",
            "0",
            "--option",
            "builders",
            "",
            "--option",
            "fallback",
            "false",
            "copy",
            "--from",
            source,
            package_text,
        ],
        Duration::from_secs(180),
    )?;
    // Copy can skip an already present path. Verify its entire closure too,
    // rather than accidentally accepting an unsigned local installation.
    process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "store",
            "verify",
            "--recursive",
            "--sigs-needed",
            "1",
            "--substituter",
            source,
            package_text,
        ],
        Duration::from_secs(180),
    )?;
    // The receipt must not become durable before downloaded store contents.
    // Nix may register copied outputs while their data still lives in the
    // filesystem's writeback cache. Sync the whole store filesystem, including
    // the transitive closure, before permission approval or activation.
    let store = fs::File::open("/nix/store").map_err(|e| e.to_string())?;
    if unsafe { libc::syncfs(store.as_raw_fd()) } != 0 {
        return Err(format!(
            "cannot persist imported package: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub fn load(package: &Path) -> Result<Report, String> {
    validate_store_path(package)?;
    if fs::canonicalize(package).map_err(|e| e.to_string())? != package || !package.is_dir() {
        return Err("package must be an exact immutable directory".into());
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(package.join("plugin.ts"))
        .map_err(|e| format!("plugin.ts is unavailable: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("plugin.ts must be a regular file".into());
    }
    let mut source = String::new();
    file.take(128 * 1024 + 1)
        .read_to_string(&mut source)
        .map_err(|e| e.to_string())?;
    let declaration = Declaration::evaluate(&source)?;
    let daemon = &declaration.contributes.daemons[0];
    resolve_executable(package, &daemon.start[0])?;
    if let Some(cleanup) = &daemon.cleanup {
        resolve_executable(package, &cleanup[0])?;
    }
    let id = declaration.id();
    let unit = unit_name(&id);
    let mut digest = Sha256::new();
    digest.update(BASE_POLICY.as_bytes());
    digest.update(package.as_os_str().as_encoded_bytes());
    digest.update(serde_json::to_vec(&declaration).map_err(|e| e.to_string())?);
    let warning = if declaration.host_network_admin() {
        "HOST NETWORK ADMINISTRATION: this daemon can change host routes, interfaces and firewall rules. It can interrupt connectivity or redirect traffic. This access is not confined to its own interface."
    } else if daemon.capabilities.iter().any(|c| c == "CAP_NET_RAW") {
        "RAW HOST NETWORK: this daemon can create raw sockets and observe or forge IP traffic."
    } else {
        "This daemon has ordinary host-network access. It has no Linux capabilities."
    };
    let mut report = Report {
        id,
        package: package.into(),
        approval: String::new(),
        policy: BASE_POLICY,
        warning,
        state_directory: format!("/var/lib/{unit}"),
        runtime_directory: format!("/run/{unit}"),
        unit: format!("{unit}.service"),
        unit_configuration: String::new(),
        declaration,
    };
    report.unit_configuration = crate::unit::render(&report)?;
    digest.update(report.unit_configuration.as_bytes());
    report.approval = hex::encode(digest.finalize());
    Ok(report)
}

pub fn unit_name(id: &str) -> String {
    format!(
        "korri-plugin-{}",
        hex::encode(Sha256::digest(id.as_bytes()))
    )
}

pub fn resolve_executable(package: &Path, selected: &str) -> Result<PathBuf, String> {
    let path = fs::canonicalize(package.join(selected))
        .map_err(|e| format!("payload executable is unavailable: {e}"))?;
    if !path.starts_with("/nix/store")
        || path.components().count() < 5
        || !path.is_file()
        || fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
    {
        return Err("payload must resolve to an immutable regular executable".into());
    }
    // Preserve argv[0] for multicall binaries such as coreutils and Tailscale.
    // Validation follows the link; execution uses the selected immutable name.
    Ok(package.join(selected))
}
