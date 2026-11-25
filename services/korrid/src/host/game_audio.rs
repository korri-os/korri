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

    /// Runs the shipped helper against a scripted `pactl` on PATH. The script
    /// plays the shared server: it prints a module list and records effects.
    fn helper(operation: &str, modules: &str, default_sink: &str) -> (bool, Vec<String>) {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("modules"), modules).unwrap();
        std::fs::write(root.path().join("default"), default_sink).unwrap();
        let pactl = root.path().join("pactl");
        std::fs::write(
            &pactl,
            format!(
                r#"#!/bin/sh
cd '{}' || exit 1
case "$*" in
  "list modules short") cat modules ;;
  get-default-sink) cat default ;;
  *) printf '%s\n' "$*" >> effects ;;
esac
"#,
                root.path().display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&pactl, std::fs::Permissions::from_mode(0o755)).unwrap();
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("game-audio.sh");
        let status = std::process::Command::new("bash")
            .arg(script)
            .args([operation, LAUNCH])
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    root.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .status()
            .unwrap();
        let effects = std::fs::read_to_string(root.path().join("effects")).unwrap_or_default();
        (
            status.success(),
            effects.lines().map(str::to_owned).collect(),
        )
    }

    #[test]
    fn cleanup_unloads_only_the_shared_servers_modules_for_this_exact_launch() {
        let modules = format!(
            "10\tmodule-null-sink\tsink_name=korri-game-{LAUNCH} sink_properties=node.description=Korri-game\n\
             11\tmodule-loopback\tsource=korri-game-{LAUNCH}.monitor sink=alsa_out source_dont_move=true\n\
             12\tmodule-null-sink\tsink_name=korri-game-ffffffffffffffffffffffffffffffff\n\
             13\tmodule-null-sink\tsink_name=korri-game-{LAUNCH}x\n\
             14\tmodule-loopback\tsource=alsa_in sink=korri-game-{LAUNCH}\n\
             15\tmodule-null-sink\tsink_properties=sink_name=korri-game-{LAUNCH}\n"
        );
        let (ok, effects) = helper("stop", &modules, "alsa_out");
        assert!(ok);
        // The loopback reads the sink's monitor, so it is unloaded first.
        assert_eq!(effects, ["unload-module 11", "unload-module 10"]);
        let (ok, effects) = helper("stop", "", "alsa_out");
        assert!(ok);
        assert!(effects.is_empty(), "{effects:?}");
    }

    #[test]
    fn release_links_the_current_default_output_once_and_never_the_launch_sink() {
        let sink = format!("1\tmodule-null-sink\tsink_name=korri-game-{LAUNCH}\n");
        let (ok, effects) = helper("release", &sink, "alsa_out");
        assert!(ok);
        assert_eq!(
            effects,
            [
                format!("set-sink-mute korri-game-{LAUNCH} false"),
                format!("load-module module-loopback source=korri-game-{LAUNCH}.monitor sink=alsa_out source_dont_move=true sink_dont_move=true"),
            ]
        );
        let linked =
            format!("{sink}2\tmodule-loopback\tsource=korri-game-{LAUNCH}.monitor sink=alsa_out\n");
        let (ok, effects) = helper("release", &linked, "alsa_out");
        assert!(ok);
        assert_eq!(
            effects,
            [format!("set-sink-mute korri-game-{LAUNCH} false")]
        );
        let (ok, effects) = helper("release", &sink, &format!("korri-game-{LAUNCH}"));
        assert!(!ok);
        assert!(!effects
            .iter()
            .any(|effect| effect.starts_with("load-module")));
    }

    #[test]
    fn the_helper_refuses_a_malformed_launch_before_any_effect() {
        use std::process::Command;
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("game-audio.sh");
        for launch in ["../x", "0123", &format!("{LAUNCH} ")] {
            let status = Command::new("bash")
                .arg(&script)
                .args(["stop", launch])
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(2));
        }
    }
}
