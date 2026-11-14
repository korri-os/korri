//! Viewer-only systemd effects. Public certificates use unit credentials;
//! neither the daemon nor a game unit opens Moonlight's private identity.
use super::*;

const PIN: &str = "moonlight-server.pem";

/// Where a lifecycle viewer binds its socket (ADR 0060). systemd creates the
/// unit's runtime directory and removes it when the unit stops. The viewer
/// creates `control/` inside it itself (clients/viewer/README.md): that name
/// must not exist before the viewer starts.
pub(crate) fn viewer_lifecycle_socket(launch: &str) -> Result<PathBuf, LaunchUnitError> {
    Ok(Path::new("/run")
        .join(runtime_directory(launch)?)
        .join("control/viewer.sock"))
}

/// The unit's own name, as plugin launches name theirs.
fn runtime_directory(launch: &str) -> Result<String, LaunchUnitError> {
    let unit = SystemdLaunchUnitBackend::unit_name(launch)?;
    Ok(unit.trim_end_matches(UNIT_SUFFIX).to_owned())
}

impl SystemdLaunchUnitBackend {
    /// `controller` is korrid's UID for a lifecycle viewer, None for Moonlight.
    pub(super) fn viewer_arguments(
        &self,
        launch: &str,
        command: &[String],
        environment: &BTreeMap<String, String>,
        sandbox: &LaunchSandbox,
        certificate: Option<&str>,
        controller: Option<u32>,
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
        if let Some(controller) = controller {
            let setfacl = self
                .acl_editor
                .as_ref()
                .filter(|path| valid_protected_path(path))
                .ok_or_else(|| {
                    LaunchUnitError::new(
                        LaunchUnitErrorKind::InvalidConfiguration,
                        "a lifecycle viewer needs KORRID_SETFACL",
                    )
                })?;
            let runtime = runtime_directory(launch)?;
            let position = args
                .iter()
                .position(|arg| arg == "--")
                .expect("unit command separator");
            // The viewer refuses a parent that its group or other users can
            // write, and korrid needs search access to reach the socket. systemd
            // applies RuntimeDirectoryMode again before each Exec command, which
            // resets an ACL mask, so the grant follows the main process start.
            // systemd-run returns only after ExecStartPost has finished.
            args.splice(
                position..position,
                [
                    format!("--property=RuntimeDirectory={runtime}"),
                    "--property=RuntimeDirectoryMode=0700".into(),
                    format!(
                        "--property=ExecStartPost={} -m u:{controller}:x /run/{runtime}",
                        setfacl.display()
                    ),
                ],
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
        let mut args = self.viewer_arguments(launch, command, environment, sandbox, None, None)?;
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
            .viewer_arguments(launch, &command, &env, &sandbox, Some(cert), None)
            .is_err());
        let viewer = backend.for_stream_viewer();
        assert!(viewer
            .viewer_arguments(
                launch,
                &command,
                &env,
                &sandbox,
                Some("not a certificate"),
                None
            )
            .is_err());
        let args = viewer
            .viewer_arguments(launch, &command, &env, &sandbox, Some(cert), None)
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
            .viewer_arguments(launch, &command, &env, &sandbox, None, None)
            .unwrap();
        assert!(!bootstrap.iter().any(|arg| arg.contains("SetCredential=")
            || arg.contains("KORRI_MOONLIGHT_SERVER_CERTIFICATE=")));
    }
    #[test]
    fn lifecycle_viewer_gets_its_runtime_directory_and_a_search_grant_for_korrid_only() {
        let launch = "0123456789abcdef0123456789abcdef";
        let command = vec!["/nix/store/viewer/bin/korri-viewer".into()];
        let env = BTreeMap::new();
        let sandbox = LaunchSandbox::new(&command, &env);
        let cert = "-----BEGIN CERTIFICATE-----\nY2VydA==\n-----END CERTIFICATE-----\n";
        let backend = SystemdLaunchUnitBackend::new(
            "/nix/store/systemd/bin/systemd-run".into(),
            "/nix/store/systemd/bin/systemctl".into(),
            1001,
            1002,
        )
        .unwrap()
        .for_stream_viewer();
        let refused = backend
            .viewer_arguments(launch, &command, &env, &sandbox, Some(cert), Some(976))
            .unwrap_err();
        assert!(refused.message.contains("KORRID_SETFACL"), "{refused:?}");
        let backend = backend.with_acl_editor(Some("/nix/store/acl/bin/setfacl".into()));
        let args = backend
            .viewer_arguments(launch, &command, &env, &sandbox, Some(cert), Some(976))
            .unwrap();
        let separator = args.iter().position(|arg| arg == "--").unwrap();
        let unit = format!("korri-game-{launch}");
        for expected in [
            format!("--property=RuntimeDirectory={unit}"),
            "--property=RuntimeDirectoryMode=0700".into(),
            format!("--property=ExecStartPost=/nix/store/acl/bin/setfacl -m u:976:x /run/{unit}"),
        ] {
            let position = args.iter().position(|arg| *arg == expected);
            assert!(
                position.is_some_and(|position| position < separator),
                "missing unit property {expected}"
            );
        }
        // The certificate reaches the lifecycle viewer as it reaches Moonlight.
        assert!(args
            .iter()
            .any(|arg| arg.starts_with("--property=SetCredential=moonlight-server.pem:")));
        assert_eq!(
            viewer_lifecycle_socket(launch).unwrap(),
            Path::new("/run").join(&unit).join("control/viewer.sock")
        );
        assert!(viewer_lifecycle_socket("../escape").is_err());
        // Moonlight's unit is unchanged: no runtime directory, no grant.
        let moonlight = backend
            .viewer_arguments(launch, &command, &env, &sandbox, Some(cert), None)
            .unwrap();
        assert!(!moonlight
            .iter()
            .any(|arg| arg.contains("RuntimeDirectory") || arg.contains("ExecStartPost")));
    }

    #[test]
    fn linux_viewer_sandbox_keeps_render_nodes_and_libva_drivers_visible() {
        let backend = SystemdLaunchUnitBackend::new(
            "/nix/store/systemd/bin/systemd-run".into(),
            "/nix/store/systemd/bin/systemctl".into(),
            1001,
            1002,
        )
        .unwrap()
        .for_stream_viewer();
        let command = vec!["/nix/store/viewer/bin/moonlight".into()];
        let environment = BTreeMap::new();
        let sandbox = LaunchSandbox::new(&command, &environment);
        let args = backend
            .viewer_arguments(
                "0123456789abcdef0123456789abcdef",
                &command,
                &environment,
                &sandbox,
                None,
                None,
            )
            .unwrap();
        // The runtime user's render group grants device access. Neither a
        // private /dev nor a device allowlist may remove it in this unit.
        for property in [
            "PrivateDevices=",
            "DevicePolicy=",
            "DeviceAllow=",
            "RootDirectory=",
            "RootImage=",
            "TemporaryFileSystem=",
            "SupplementaryGroups=",
        ] {
            assert!(!args
                .iter()
                .any(|arg| arg.starts_with(&format!("--property={property}"))));
        }
        for target in [
            "/dev/dri/renderD128",
            "/run/opengl-driver/lib/dri/iHD_drv_video.so",
            "/nix/store",
        ] {
            for hidden in args
                .iter()
                .filter_map(|arg| arg.strip_prefix("--property=InaccessiblePaths="))
                .flat_map(str::split_whitespace)
            {
                assert!(
                    !Path::new(target).starts_with(hidden.trim_start_matches('-')),
                    "{hidden} masks {target}"
                );
            }
        }
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
            .viewer_arguments(
                launch,
                &command,
                &environment,
                &sandbox,
                Some(certificate),
                None,
            )
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
                None,
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
