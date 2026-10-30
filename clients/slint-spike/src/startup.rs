//! Failure-only evidence before the webOS brain starts. Never redirect logs.
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const PREFIX: &str = "korri-slint-spike-startup-";

#[derive(Clone, Copy)]
pub enum Stage {
    CliParse,
    InheritedEnvironment,
    SdlInstall,
    BrainInit,
}

/// Preserve the original startup result. Success never creates a diagnostic.
pub fn record<T>(result: Result<T, String>, stage: Stage) -> Result<T, String> {
    if let Err(error) = &result {
        report_failure(
            stage,
            matches!(stage, Stage::SdlInstall).then_some(error.as_str()),
        );
    }
    result
}

pub fn report_failure(stage: Stage, sdl_error: Option<&str>) {
    // /tmp resolves inside the app's own jail, not HOME or existing user state.
    match write_failure(Path::new("/tmp"), stage, sdl_error) {
        Ok(path) => eprintln!("startup diagnostic: {}", path.display()),
        Err(_) => eprintln!("could not retain startup diagnostic"),
    }
}

fn write_failure(directory: &Path, stage: Stage, sdl_error: Option<&str>) -> io::Result<PathBuf> {
    // Defense in depth: even an accidental call on root refusal writes nothing.
    // SAFETY: geteuid has no preconditions and reads only the effective uid.
    if unsafe { libc::geteuid() } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "root diagnostic refused",
        ));
    }
    let mut file = tempfile::Builder::new()
        .prefix(PREFIX)
        .suffix(".log")
        .tempfile_in(directory)?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    let label = match stage {
        Stage::CliParse => "cli_parse_rejected",
        Stage::InheritedEnvironment => "inherited_environment_rejected",
        Stage::SdlInstall => "sdl_install_failed",
        Stage::BrainInit => "brain_init_failed",
    };
    writeln!(file, "stage: {label}")?;
    if let (Stage::SdlInstall, Some(error)) = (stage, sdl_error) {
        writeln!(file, "sdl_error: {error}")?;
    }
    file.flush()?;
    file.keep()
        .map(|(_, path)| path)
        .map_err(|error| error.error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    fn artifacts() -> BTreeSet<PathBuf> {
        fs::read_dir("/tmp")
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(PREFIX)
            })
            .collect()
    }

    #[test]
    fn failures_are_private_unique_and_capture_only_the_sdl_message() {
        // The separate user-namespace test proves root is refused.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let stages = [
            (Stage::CliParse, "cli_parse_rejected"),
            (
                Stage::InheritedEnvironment,
                "inherited_environment_rejected",
            ),
            (Stage::BrainInit, "brain_init_failed"),
        ];
        let mut paths = BTreeSet::new();
        for (stage, label) in stages {
            let path = write_failure(
                directory.path(),
                stage,
                Some("private-detail-must-not-be-recorded"),
            )
            .unwrap();
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                format!("stage: {label}\n")
            );
            assert!(paths.insert(path));
        }
        let error = "No available video device";
        for _ in 0..2 {
            let path = write_failure(directory.path(), Stage::SdlInstall, Some(error)).unwrap();
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                format!("stage: sdl_install_failed\nsdl_error: {error}\n")
            );
            assert!(paths.insert(path));
        }
        for path in paths {
            let metadata = fs::symlink_metadata(path).unwrap();
            assert!(metadata.is_file());
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
            assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        }
    }

    #[test]
    fn successful_startup_result_creates_no_diagnostic() {
        let before = artifacts();
        assert_eq!(record(Ok(42), Stage::SdlInstall), Ok(42));
        assert_eq!(artifacts(), before);
    }

    #[test]
    fn root_refusal_creates_no_diagnostic() {
        // Run this test again under unshare --user --map-root-user.
        if unsafe { libc::geteuid() } != 0 {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let error = write_failure(directory.path(), Stage::BrainInit, None).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        let before = artifacts();
        assert_eq!(crate::main(), std::process::ExitCode::from(2));
        assert_eq!(artifacts(), before);
    }
}
