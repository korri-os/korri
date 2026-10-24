//! Viewer-only systemd effects. Public certificates use unit credentials;
//! neither the daemon nor a game unit opens Moonlight's private identity.
use super::*;

const PIN: &str = "moonlight-server.pem";
impl SystemdLaunchUnitBackend {
    pub(super) fn viewer_arguments(
        &self,
        launch: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
        certificate: Option<&str>,
    ) -> Result<Vec<String>, LaunchUnitError> {
        if !self.stream_viewer {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "Moonlight effects require the viewer sandbox",
            ));
        }
        let mut env = environment.clone();
        if certificate.is_some() {
            // Transient Environment assignments keep %d literal. These units
            // use the system manager, whose credential directory names the unit.
            let unit = Self::unit_name(launch)?;
            env.insert(
                "KORRI_MOONLIGHT_SERVER_CERTIFICATE".into(),
                format!("/run/credentials/{unit}/{PIN}"),
            );
        }
        let mut args = self.launch_arguments_in_sandbox(launch, command, &env, sandbox)?;
        if let Some(certificate) = certificate {
            crate::host::moonlight_certificate::validate_single_pem(certificate).map_err(|_| {
                LaunchUnitError::new(
                    LaunchUnitErrorKind::InvalidConfiguration,
                    "Invalid Moonlight host certificate",
                )
            })?;
            let position = args
                .iter()
                .position(|arg| arg == "--")
                .expect("unit command separator");
            let escaped = certificate
                .replace('\\', "\\\\")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            args.insert(
                position,
                format!("--property=SetCredential={PIN}:{escaped}"),
            );
        }
        Ok(args)
    }
    pub(super) fn run_viewer_bootstrap(
        &self,
        launch: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
    ) -> Result<String, LaunchUnitError> {
        let mut args = self.viewer_arguments(launch, command, environment, sandbox, None)?;
        args.splice(
            0..0,
            [
                "--wait".into(),
                "--pipe".into(),
                "--collect".into(),
                "--property=RuntimeMaxSec=15s".into(),
                "--property=UMask=0077".into(),
                "--property=TimeoutStopSec=5s".into(),
            ],
        );
        // run() bounds both output streams to 64 KiB and has a hard deadline.
        let output = match self.run(&self.systemd_run, &args) {
            Ok(output) if output.status.success() => output,
            other => {
                let cause = match other {
                    Err(error) => error.message,
                    _ => "bootstrap helper failed".into(),
                };
                self.stop(launch)?;
                if self.state(launch)? != LaunchUnitState::Completed {
                    return Err(LaunchUnitError::new(
                        LaunchUnitErrorKind::Failed,
                        "Moonlight bootstrap failed and its unit stop was not confirmed",
                    ));
                }
                return Err(LaunchUnitError::new(
                    LaunchUnitErrorKind::Failed,
                    format!("Moonlight identity bootstrap failed: {cause}"),
                ));
            }
        };
        let certificate = String::from_utf8(output.stdout).map_err(|_| {
            LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "Moonlight bootstrap returned non-text output",
            )
        })?;
        crate::host::moonlight_certificate::validate_single_pem(&certificate).map_err(|_| {
            LaunchUnitError::new(
                LaunchUnitErrorKind::Protocol,
                "Moonlight bootstrap must return exactly one bounded certificate",
            )
        })?;
        Ok(certificate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linux_viewer_host_pin_is_a_credential_not_a_keydir_file() {
        let backend = SystemdLaunchUnitBackend::new(
            "/nix/store/systemd/bin/systemd-run".into(),
            "/nix/store/systemd/bin/systemctl".into(),
            1001,
            1002,
        )
        .unwrap();
        let launch = "0123456789abcdef0123456789abcdef";
        let command = vec!["/nix/store/viewer/bin/moonlight".into()];
        let env = BTreeMap::new();
        let sandbox = LaunchSandbox::new(&command, &env);
        let cert = "-----BEGIN CERTIFICATE-----\nY2VydA==\n-----END CERTIFICATE-----\n";
        assert!(backend
            .viewer_arguments(launch, &command, &env, &sandbox, Some(cert))
            .is_err());
        let viewer = backend.for_stream_viewer();
        assert!(viewer
            .viewer_arguments(launch, &command, &env, &sandbox, Some("not a certificate"))
            .is_err());
        let args = viewer
            .viewer_arguments(launch, &command, &env, &sandbox, Some(cert))
            .unwrap();
        assert!(args.iter().any(
            |arg| arg == "--setenv=KORRI_MOONLIGHT_SERVER_CERTIFICATE=/run/credentials/korri-game-0123456789abcdef0123456789abcdef.service/moonlight-server.pem"
        ));
        assert!(args.iter().any(|arg| arg
            == &format!(
                "--property=SetCredential=moonlight-server.pem:{}",
                cert.replace('\n', "\\n")
            )));
        assert!(!args
            .iter()
            .any(|arg| arg.contains("server-0123456789abcdef")));
        let bootstrap = viewer
            .viewer_arguments(launch, &command, &env, &sandbox, None)
            .unwrap();
        assert!(!bootstrap.iter().any(|arg| arg.contains("SetCredential=")
            || arg.contains("KORRI_MOONLIGHT_SERVER_CERTIFICATE=")));
    }
    #[test]
    fn linux_viewer_pin_argument_is_an_absolute_native_credential_path() {
        let backend = SystemdLaunchUnitBackend::new(
            "/nix/store/systemd/bin/systemd-run".into(),
            "/nix/store/systemd/bin/systemctl".into(),
            1001,
            1002,
        )
        .unwrap()
        .for_stream_viewer();
        let launch = "0123456789abcdef0123456789abcdef";
        let command = vec!["/nix/store/viewer/bin/moonlight".into()];
        let environment = BTreeMap::from([(
            "KORRI_MOONLIGHT_SERVER_CERTIFICATE".into(),
            "/ignored-caller-pin.pem".into(),
        )]);
        let sandbox = LaunchSandbox::new(&command, &environment);
        let certificate = "-----BEGIN CERTIFICATE-----\nY2VydA==\n-----END CERTIFICATE-----\n";
        let args = backend
            .viewer_arguments(launch, &command, &environment, &sandbox, Some(certificate))
            .unwrap();
        let unit = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--unit="))
            .unwrap();
        let pin = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--setenv=KORRI_MOONLIGHT_SERVER_CERTIFICATE="))
            .expect("pinned viewer must receive the native credential path");
        assert_eq!(
            Path::new(pin),
            Path::new("/run/credentials").join(unit).join(PIN)
        );
        assert!(
            Path::new(pin).is_absolute(),
            "transient Environment values do not expand unit specifiers: {pin}"
        );
        assert!(!pin.contains('%'), "unexpanded unit specifier in pin path");
        let other_args = backend
            .viewer_arguments(
                &"f".repeat(32),
                &command,
                &environment,
                &sandbox,
                Some(certificate),
            )
            .unwrap();
        let other_unit = other_args
            .iter()
            .find_map(|arg| arg.strip_prefix("--unit="))
            .unwrap();
        let other_pin = other_args
            .iter()
            .find_map(|arg| arg.strip_prefix("--setenv=KORRI_MOONLIGHT_SERVER_CERTIFICATE="))
            .unwrap();
        assert_eq!(
            Path::new(other_pin),
            Path::new("/run/credentials").join(other_unit).join(PIN)
        );
        assert_ne!(
            pin, other_pin,
            "a viewer must not use another unit's credential"
        );
    }

    #[test]
    fn linux_viewer_bootstrap_timeout_stops_the_exact_named_sandbox_unit() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let run = root.path().join("systemd-run");
        let ctl = root.path().join("systemctl");
        let args_path = root.path().join("args");
        let calls_path = root.path().join("calls");
        std::fs::write(
            &run,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nsleep 2\n",
                args_path.display()
            ),
        )
        .unwrap();
        std::fs::write(
            &ctl,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nprintf 'LoadState=not-found\\n'\n",
                calls_path.display()
            ),
        )
        .unwrap();
        for path in [&run, &ctl] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut backend =
            SystemdLaunchUnitBackend::with_timeout(run, ctl, 1001, 1002, Duration::from_millis(50))
                .unwrap()
                .for_stream_viewer();
        backend.diagnostics_root = root.path().join("diagnostics");
        let id = "0123456789abcdef0123456789abcdef";
        let command = vec!["/nix/store/viewer/bootstrap".into()];
        let env = BTreeMap::new();
        let error = backend
            .bootstrap_viewer(id, &command, &env, &LaunchSandbox::new(&command, &env))
            .unwrap_err();
        assert!(error.message.contains("bootstrap failed"), "{error:?}");
        assert!(error.message.contains("timed out"), "{error:?}");
        let args = std::fs::read_to_string(args_path).unwrap();
        for expected in [
            "--unit=korri-game-0123456789abcdef0123456789abcdef.service",
            "--wait",
            "--pipe",
            "--property=RuntimeMaxSec=15s",
            "--property=UMask=0077",
            "--uid=1001",
        ] {
            assert!(
                args.lines().any(|arg| arg == expected),
                "missing {expected}"
            );
        }
        assert!(!args.contains("-/home/korri/.config/moonlight"));
        assert!(std::fs::read_to_string(calls_path)
            .unwrap()
            .contains("stop korri-game-0123456789abcdef0123456789abcdef.service"));
    }
}
