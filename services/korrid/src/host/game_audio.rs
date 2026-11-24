//! One pre-muted sink per exact launch, controlled through an unprivileged
//! transient unit. The daemon never acquires another user's audio socket.
use super::*;

impl SystemdLaunchUnitBackend {
    fn audio_unit(launch: &str) -> Result<String, LaunchUnitError> {
        validate_launch_id(launch)?;
        Ok(format!("korri-game-audio-{launch}.service"))
    }

    pub(super) fn prepare_audio(&self, launch: &str) -> Result<(), LaunchUnitError> {
        let helper = std::env::var("KORRID_GAME_AUDIO_HELPER").map_err(|_| {
            LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "game audio helper is not configured",
            )
        })?;
        let arguments = self.audio_arguments(launch, &helper)?;
        self.require_success(&self.systemd_run, &arguments)
            .map(|_| ())
    }

    fn audio_arguments(&self, launch: &str, helper: &str) -> Result<Vec<String>, LaunchUnitError> {
        if !Path::new(helper).is_absolute() || helper.chars().any(char::is_whitespace) {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "invalid game audio helper",
            ));
        }
        let arguments = vec![
            "--system".into(),
            "--no-ask-password".into(),
            "--quiet".into(),
            "--collect".into(),
            "--service-type=exec".into(),
            format!("--unit={}", Self::audio_unit(launch)?),
            format!("--property=User={}", self.runtime_uid),
            format!("--property=Group={}", self.runtime_gid),
            format!("--property=RuntimeDirectory=korri-game-audio-{launch}"),
            "--property=RuntimeDirectoryMode=0700".into(),
            "--property=NoNewPrivileges=yes".into(),
            "--property=ProtectSystem=strict".into(),
            "--property=ProtectHome=yes".into(),
            "--property=PrivateDevices=yes".into(),
            format!(
                "--property=BindReadOnlyPaths=/run/user/{0}/pulse:/run/korri-game-audio-control /run/user/{0}/pipewire-0:/run/korri-game-audio-pipewire",
                self.runtime_uid
            ),
            "--property=RestrictAddressFamilies=AF_UNIX".into(),
            "--property=TimeoutStartSec=5s".into(),
            "--property=TimeoutStopSec=5s".into(),
            format!("--property=ExecStartPost={helper} ready {launch}"),
            format!("--property=ExecReload={helper} release {launch}"),
            format!("--property=ExecStopPost={helper} stop {launch}"),
            "--setenv=PULSE_SERVER=unix:/run/korri-game-audio-control/native".into(),
            format!("--setenv=XDG_RUNTIME_DIR=/run/korri-game-audio-{launch}"),
            "--".into(),
            helper.into(),
            "serve".into(),
            launch.into(),
        ];
        Ok(arguments)
    }

    pub(super) fn audio_verb(&self, launch: &str, verb: &str) -> Result<(), LaunchUnitError> {
        let unit = Self::audio_unit(launch)?;
        if std::env::var_os("KORRID_GAME_AUDIO_HELPER").is_none() {
            return Ok(());
        }
        let properties = self.unit_properties(&unit, &["LoadState"])?;
        if properties.values.get("LoadState").map(String::as_str) != Some("loaded") {
            return Ok(());
        }
        self.require_success(
            &self.systemctl,
            &[
                "--system".into(),
                "--no-ask-password".into(),
                verb.into(),
                unit,
            ],
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const LAUNCH: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn the_private_server_is_ready_before_the_game_and_has_only_scoped_audio_binds() {
        let backend =
            SystemdLaunchUnitBackend::new("/bin/true".into(), "/bin/true".into(), 2001, 2002)
                .unwrap();
        let arguments = backend
            .audio_arguments(LAUNCH, "/nix/store/helper/bin/korri-game-audio")
            .unwrap();
        for argument in [
            "--service-type=exec".to_owned(),
            "--property=User=2001".to_owned(),
            "--property=Group=2002".to_owned(),
            "--property=ProtectHome=yes".to_owned(),
            "--property=BindReadOnlyPaths=/run/user/2001/pulse:/run/korri-game-audio-control /run/user/2001/pipewire-0:/run/korri-game-audio-pipewire".to_owned(),
            format!("--property=ExecStartPost=/nix/store/helper/bin/korri-game-audio ready {LAUNCH}"),
            format!("--property=ExecReload=/nix/store/helper/bin/korri-game-audio release {LAUNCH}"),
        ] {
            assert!(arguments.contains(&argument), "{argument}: {arguments:?}");
        }
        assert_eq!(
            &arguments[arguments.len() - 3..],
            ["/nix/store/helper/bin/korri-game-audio", "serve", LAUNCH]
        );
        assert!(backend
            .audio_arguments("not-a-launch", "/bin/true")
            .is_err());
        assert!(backend.audio_arguments(LAUNCH, "relative-helper").is_err());
        assert!(backend
            .audio_arguments(LAUNCH, "/bin/true injected")
            .is_err());
    }
}
