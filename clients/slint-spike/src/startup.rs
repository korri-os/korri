//! Private startup failure evidence and location-only webOS panics. No log redirection.
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

#[derive(Clone, Copy)]
struct PanicLocation<'a> {
    file: &'a str,
    line: u32,
    column: u32,
}

enum Diagnostic<'a> {
    Failure(Stage, Option<&'a str>),
    PanicLocation(PanicLocation<'a>),
}

/// Replace, never chain, the default hook: panic payloads may contain secrets.
pub fn install_panic_hook() {
    install_panic_hook_in(PathBuf::from("/tmp"));
}

fn install_panic_hook_in(directory: PathBuf) {
    std::panic::set_hook(Box::new(move |info: &std::panic::PanicHookInfo<'_>| {
        if let Some(location) = info.location() {
            let location = PanicLocation {
                file: location.file(),
                line: location.line(),
                column: location.column(),
            };
            // Neither I/O failure may trigger another panic inside this hook.
            let _ = write_private(&directory, Diagnostic::PanicLocation(location));
            let _ = write_location(&mut io::stderr().lock(), location);
        }
    }));
}

fn write_location(output: &mut impl Write, location: PanicLocation<'_>) -> io::Result<()> {
    for character in location
        .file
        .chars()
        .filter(|character| !character.is_control())
    {
        write!(output, "{character}")?;
    }
    writeln!(output, ":{}:{}", location.line, location.column)
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
    write_private(directory, Diagnostic::Failure(stage, sdl_error))
}

fn write_private(directory: &Path, diagnostic: Diagnostic<'_>) -> io::Result<PathBuf> {
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
    match diagnostic {
        Diagnostic::Failure(stage, sdl_error) => {
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
        }
        Diagnostic::PanicLocation(location) => write_location(&mut file, location)?,
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
    fn panic_locations_are_private_unique_and_sanitized() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let location = PanicLocation {
            file: "src/start\n\tup.rs",
            line: 17,
            column: 9,
        };
        let mut paths = BTreeSet::new();
        for _ in 0..2 {
            let path =
                write_private(directory.path(), Diagnostic::PanicLocation(location)).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "src/startup.rs:17:9\n");
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(metadata.is_file());
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
            assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
            assert!(paths.insert(path));
        }
    }

    const PANIC_PAYLOAD_SENTINEL: &str = "NEVER_CAPTURE_THIS_PANIC_PAYLOAD";

    #[test]
    fn panic_hook_child_probe() {
        let Some(directory) = std::env::var_os("SLINT_PANIC_DIAGNOSTIC_TEST_DIR") else {
            return;
        };
        let directory = PathBuf::from(directory);
        let before = fs::read_dir(&directory).map(|entries| entries.count()).ok();
        install_panic_hook_in(directory.clone());
        assert_eq!(
            fs::read_dir(&directory).map(|entries| entries.count()).ok(),
            before
        );
        let result = std::panic::catch_unwind(|| panic!("{PANIC_PAYLOAD_SENTINEL}"));
        assert!(result.is_err());
    }

    #[test]
    fn controlled_panic_records_only_location_and_survives_logging_io_errors() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        for case in ["normal", "missing-directory", "broken-stderr"] {
            let directory = tempfile::tempdir().unwrap();
            let destination = if case == "missing-directory" {
                directory.path().join("does-not-exist")
            } else {
                directory.path().to_owned()
            };
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "startup::tests::panic_hook_child_probe",
                    "--nocapture",
                ])
                .env("SLINT_PANIC_DIAGNOSTIC_TEST_DIR", &destination);
            if case == "broken-stderr" {
                command.stderr(
                    fs::OpenOptions::new()
                        .write(true)
                        .open("/dev/full")
                        .unwrap(),
                );
            }
            let output = command.output().unwrap();
            assert!(output.status.success());
            assert!(!String::from_utf8_lossy(&output.stdout).contains(PANIC_PAYLOAD_SENTINEL));
            assert!(!String::from_utf8_lossy(&output.stderr).contains(PANIC_PAYLOAD_SENTINEL));
            if case == "missing-directory" {
                assert!(!destination.exists());
            } else {
                let paths: Vec<_> = fs::read_dir(&destination)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .collect();
                assert_eq!(paths.len(), 1);
                let content = fs::read(&paths[0]).unwrap();
                assert!(!String::from_utf8_lossy(&content).contains(PANIC_PAYLOAD_SENTINEL));
                if case == "normal" {
                    assert_eq!(output.stderr, content);
                }
                let mut fields = std::str::from_utf8(&content)
                    .unwrap()
                    .trim_end()
                    .rsplitn(3, ':');
                assert!(fields.next().unwrap().parse::<u32>().unwrap() > 0);
                assert!(fields.next().unwrap().parse::<u32>().unwrap() > 0);
                assert!(fields.next().unwrap().ends_with("startup.rs"));
                assert_eq!(
                    fs::metadata(&paths[0]).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
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
