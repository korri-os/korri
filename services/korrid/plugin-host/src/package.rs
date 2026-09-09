use crate::{declaration::Declaration, process, provenance::Provenance};
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
    pub provenance: Provenance,
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

pub fn validate_cache_source(source: &str) -> Result<(), String> {
    if !(source.starts_with("https://")
        || source.starts_with("http://")
        || source.starts_with("file:///"))
        || source.len() > 4096
        || source.chars().any(char::is_control)
    {
        return Err("source must be an HTTP(S) or local file binary cache".into());
    }
    Ok(())
}

pub fn import(nix: &Path, source: &str, package: &Path) -> Result<(), String> {
    validate_store_path(package)?;
    validate_cache_source(source)?;
    let package_text = package.to_str().ok_or("invalid package path")?;
    // Resolve across the plugin cache and configured upstream substituters.
    // `copy --from` requires one source to contain the entire closure.
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
            "--option",
            "require-sigs",
            "true",
            "build",
            "--no-link",
            "--extra-substituters",
            source,
            package_text,
        ],
        Duration::from_secs(180),
    )?;
    // Realization can reuse paths without locally registered signatures (for
    // example, in a NixOS store image). Unlike build, store verify consults only
    // explicit --substituter arguments, not the configured substituters.
    // `nix config show substituters` prints Nix's effective whitespace-separated
    // list, including extra-substituters; keep that policy owned by Nix.
    let substituters = process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "config",
            "show",
            "substituters",
        ],
        Duration::from_secs(10),
    )?;
    let mut verify_args = vec![
        "--extra-experimental-features",
        "nix-command",
        "store",
        "verify",
        "--recursive",
        "--sigs-needed",
        "1",
        "--substituter",
        source,
    ];
    for substituter in substituters.split_whitespace() {
        verify_args.extend(["--substituter", substituter]);
    }
    verify_args.push(package_text);
    process::checked(nix, verify_args, Duration::from_secs(180))?;
    // The receipt must not become durable before downloaded store contents.
    // Nix may register substituted outputs while their data still lives in the
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

pub fn load_declaration(package: &Path) -> Result<Declaration, String> {
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
    Ok(declaration)
}

pub fn load(package: &Path, provenance: Provenance) -> Result<Report, String> {
    let declaration = load_declaration(package)?;
    let id = declaration.id();
    provenance.validate(&id)?;
    let unit = unit_name(&id);
    let daemon = &declaration.contributes.daemons[0];
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
        provenance,
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
    report.approval = approval_digest(
        &report.package,
        &report.provenance,
        &report.declaration,
        &report.unit_configuration,
    )?;
    Ok(report)
}

fn approval_digest(
    package: &Path,
    provenance: &Provenance,
    declaration: &Declaration,
    unit: &str,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(BASE_POLICY, package, provenance, declaration, unit))
        .map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
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

#[cfg(test)]
mod approval_tests {
    use super::*;
    #[test]
    fn approval_binds_source_release_archive_package_and_effective_policy_not_staging() {
        let declaration = Declaration::evaluate("({namespace:'@test',name:'plugin',contributes:{daemons:[{Type:'exec',ExecStart:['bin/run'],CapabilityBoundingSet:[]}]}})").unwrap();
        let package = Path::new("/nix/store/00000000000000000000000000000000-package");
        let origin = Provenance::Repository {
            source_url: "https://a.example/catalog".into(),
            plugin_id: declaration.id(),
            release_version: "1".into(),
            platform: crate::provenance::current_platform().into(),
            archive_sha256: "a".repeat(64),
        };
        let digest = approval_digest(package, &origin, &declaration, "effective unit").unwrap();
        assert_eq!(
            digest,
            approval_digest(package, &origin.clone(), &declaration, "effective unit").unwrap()
        );
        for field in ["source_url", "release_version", "archive_sha256"] {
            let mut changed = serde_json::to_value(&origin).unwrap();
            changed[field] = serde_json::Value::String("different".into());
            let changed: Provenance = serde_json::from_value(changed).unwrap();
            assert_ne!(
                digest,
                approval_digest(package, &changed, &declaration, "effective unit").unwrap()
            );
        }
        assert_ne!(
            digest,
            approval_digest(
                Path::new("/nix/store/11111111111111111111111111111111-package"),
                &origin,
                &declaration,
                "effective unit"
            )
            .unwrap()
        );
        assert_ne!(
            digest,
            approval_digest(package, &origin, &declaration, "new policy").unwrap()
        );
        assert_ne!(
            digest,
            approval_digest(
                package,
                &Provenance::RawCache {
                    cache_url: "file:///staging/cache".into()
                },
                &declaration,
                "effective unit"
            )
            .unwrap()
        );
    }
}
