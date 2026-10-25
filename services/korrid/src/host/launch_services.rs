//! Only the approved Sunshine unit can consume these launch service arguments.
use super::systemd_unit::{LaunchUnitError, LaunchUnitErrorKind, SystemdLaunchUnitBackend};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::Path};

fn failure(message: impl ToString) -> LaunchUnitError {
    LaunchUnitError::new(LaunchUnitErrorKind::Failed, message.to_string())
}

fn encoded_args(args: &[String]) -> Result<Vec<u8>, LaunchUnitError> {
    let mut bytes = Vec::new();
    for arg in args {
        let (key, value) = arg
            .split_once('=')
            .ok_or_else(|| failure("Sunshine setting must be key=value"))?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
            || value.contains(['\n', '\r', '\0'])
        {
            return Err(failure("invalid Sunshine setting argument"));
        }
        bytes.extend_from_slice(arg.as_bytes());
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn write_changed(path: &Path, bytes: &[u8]) -> Result<bool, LaunchUnitError> {
    let previous = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(failure(error)),
    };
    if previous == bytes {
        return Ok(false);
    }
    let parent = path
        .parent()
        .ok_or_else(|| failure("Sunshine arguments have no parent"))?;
    let temporary = parent.join(format!(".sunshine-{:016x}.tmp", rand::random::<u64>()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.set_permissions(fs::Permissions::from_mode(0o644))?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(failure)?;
    Ok(true)
}

impl SystemdLaunchUnitBackend {
    pub(super) fn apply_sunshine_at(
        &self,
        path: &Path,
        args: &[String],
    ) -> Result<(), LaunchUnitError> {
        let bytes = encoded_args(args)?;
        if write_changed(path, &bytes)? {
            let output = self.run(
                &self.systemctl,
                &[
                    "--no-ask-password".into(),
                    "restart".into(),
                    "korri-sunshine.service".into(),
                ],
            )?;
            if !output.status.success() {
                return Err(failure(
                    "Sunshine restart failed; the game will still start",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sunshine_args_are_atomic_change_only_and_restart_the_exact_unit() {
        use std::os::unix::fs::MetadataExt;
        let root = tempfile::tempdir().unwrap();
        let args = root.path().join("sunshine.args");
        let calls = root.path().join("calls");
        let systemctl = root.path().join("systemctl");
        fs::write(
            &systemctl,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n",
                calls.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755)).unwrap();
        let backend =
            SystemdLaunchUnitBackend::new("/bin/true".into(), systemctl, 1001, 1002).unwrap();
        backend.apply_sunshine_at(&args, &[]).unwrap();
        assert!(!args.exists());
        assert!(!calls.exists());
        let values = vec!["max_bitrate=3000".into(), "encoder=auto".into()];
        backend.apply_sunshine_at(&args, &values).unwrap();
        let first = fs::metadata(&args).unwrap().ino();
        backend.apply_sunshine_at(&args, &values).unwrap();
        assert_eq!(fs::metadata(&args).unwrap().ino(), first);
        assert_eq!(
            fs::read_to_string(&args).unwrap(),
            "max_bitrate=3000\nencoder=auto\n"
        );
        assert_eq!(
            fs::read_to_string(&calls).unwrap(),
            "--no-ask-password restart korri-sunshine.service\n"
        );
        backend.apply_sunshine_at(&args, &[]).unwrap();
        assert_ne!(fs::metadata(&args).unwrap().ino(), first);
        assert_eq!(fs::read(&args).unwrap(), b"");
        assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 2);
        assert!(fs::read_dir(root.path()).unwrap().all(|v| !v
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }
    #[test]
    fn sunshine_rejects_untrusted_lines_and_reports_restart_failure() {
        for value in [
            "no-assignment",
            "bad-key=1",
            "KEY=1",
            "=1",
            "key=x\ninject=yes",
            "key=x\r",
            "key=x\0",
        ] {
            assert!(encoded_args(&[value.into()]).is_err(), "{value}");
        }
        let root = tempfile::tempdir().unwrap();
        let fail = root.path().join("systemctl");
        fs::write(&fail, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&fail, fs::Permissions::from_mode(0o755)).unwrap();
        let backend = SystemdLaunchUnitBackend::new("/bin/sh".into(), fail, 1001, 1002).unwrap();
        assert!(backend
            .apply_sunshine_at(&root.path().join("args"), &["encoder=auto".into()])
            .unwrap_err()
            .message
            .contains("restart failed"));
    }
}
