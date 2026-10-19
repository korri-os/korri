//! Transient unpack units have deterministic contribution ownership, so a
//! restarted standalone daemon can stop an orphan before cleaning its work.
use super::Failure;
use crate::plugin::ArtifactRecord;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn name(artifact: &ArtifactRecord) -> String {
    let (plugin, key) = artifact.id.split_once('/').expect("validated artifact id");
    let (_, plugin) = plugin.split_once(':').expect("validated plugin id");
    format!("korri-acquire-{plugin}-{key}.service")
}

#[cfg(not(test))]
fn control(
    artifact: &ArtifactRecord,
    verb: &str,
    extra: &[&str],
) -> Result<std::process::Output, Failure> {
    Command::new(std::env::var_os("KORRID_SYSTEMCTL").unwrap_or_else(|| "systemctl".into()))
        .args(["--system", "--no-ask-password", verb])
        .arg(name(artifact))
        .args(extra)
        .env("SYSTEMD_BUS_TIMEOUT", "30s")
        .stdin(Stdio::null())
        .output()
        .map_err(|_| Failure::Unpack)
}
#[cfg(not(test))]
fn active(artifact: &ArtifactRecord) -> Result<bool, Failure> {
    let output = control(
        artifact,
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
    if active(artifact)? {
        // systemctl stop waits for the entire control group to stop. Never
        // discard .tmp-* after a failed stop or an unavailable manager.
        if !control(artifact, "stop", &[])?.status.success() {
            return Err(Failure::Unpack);
        }
        if active(artifact)? {
            return Err(Failure::Unpack);
        }
    }
    Ok(())
}

fn unit_arguments(
    base: &Path,
    private_root: &Path,
    artifact: &ArtifactRecord,
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
    Ok(vec![
        "--system".into(),
        "--no-ask-password".into(),
        "--quiet".into(),
        "--wait".into(),
        "--collect".into(),
        "--service-type=exec".into(),
        format!("--unit={}", name(artifact)),
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
        "--".into(),
    ])
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
        if active(artifact)? {
            while active(artifact)? {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            return Err(Failure::Unpack);
        }
        let mut command = Command::new(
            std::env::var_os("KORRID_SYSTEMD_RUN").unwrap_or_else(|| "systemd-run".into()),
        );
        command
            .args(unit_arguments(base, private_root, artifact)?)
            .arg(program);
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
            &artifact(),
        )
        .unwrap();
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
            &artifact()
        )
        .is_err());
    }
}
