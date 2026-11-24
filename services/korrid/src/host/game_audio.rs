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
        if !Path::new(&helper).is_absolute() || helper.chars().any(char::is_whitespace) {
            return Err(LaunchUnitError::new(
                LaunchUnitErrorKind::InvalidConfiguration,
                "invalid game audio helper",
            ));
        }
        let arguments = vec![
            "--system".into(),
            "--no-ask-password".into(),
            "--quiet".into(),
            "--service-type=oneshot".into(),
            format!("--unit={}", Self::audio_unit(launch)?),
            "--property=RemainAfterExit=yes".into(),
            format!("--property=User={}", self.runtime_uid),
            format!("--property=Group={}", self.runtime_gid),
            format!("--property=RuntimeDirectory=korri-game-audio-{launch}"),
            "--property=RuntimeDirectoryMode=0700".into(),
            "--property=NoNewPrivileges=yes".into(),
            "--property=ProtectSystem=strict".into(),
            "--property=ProtectHome=yes".into(),
            "--property=PrivateDevices=yes".into(),
            format!(
                "--property=BindReadOnlyPaths=/run/user/{}/pulse:/run/korri-game-audio",
                self.runtime_uid
            ),
            "--property=RestrictAddressFamilies=AF_UNIX".into(),
            "--property=TimeoutStartSec=5s".into(),
            "--property=TimeoutStopSec=5s".into(),
            format!("--property=ExecReload={helper} release {launch}"),
            format!("--property=ExecStopPost={helper} stop {launch}"),
            "--setenv=PULSE_SERVER=unix:/run/korri-game-audio/native".into(),
            "--".into(),
            helper,
            "prepare".into(),
            launch.into(),
        ];
        self.require_success(&self.systemd_run, &arguments)
            .map(|_| ())
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
