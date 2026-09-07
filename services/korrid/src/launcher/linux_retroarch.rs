use super::LaunchError;
use crate::config::{resolver::ResolvedRoute, storage, ConfigSnapshot};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path},
};

const RETROARCH_PROVIDER: &str = "@korri:retroarch";
const RETROARCH_LAUNCHER: &str = "@korri:retroarch/retroarch";
const LIBRETRO_CORE_KIND: &str = "libretro-core";
const DEFAULT_ACCOUNT: &str = "default";
const RETROARCH_AUTOCONFIG_ENV: &str = "KORRI_RETROARCH_AUTOCONFIG";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxLaunchSpec {
    pub command: Vec<String>,
}

pub(crate) fn launch_route_with_env(
    root: &Path,
    snapshot: &ConfigSnapshot,
    route: &ResolvedRoute,
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Result<LinuxLaunchSpec, LaunchError> {
    if route.provider_id != RETROARCH_PROVIDER
        || route.launcher_kind != RETROARCH_PROVIDER
        || route.launcher_id != RETROARCH_LAUNCHER
    {
        return Err(LaunchError::RouteUnavailable(format!(
            "route {} is not owned by the RetroArch plugin",
            route.playable_id
        )));
    }
    let launcher = route.linux_launcher.as_ref().ok_or_else(|| {
        LaunchError::RouteUnavailable(format!(
            "RetroArch route {} has no plugin-provided Linux launcher",
            route.playable_id
        ))
    })?;
    let runtime = route.runtime.as_ref().ok_or_else(|| {
        LaunchError::RouteUnavailable(format!(
            "RetroArch route {} has no plugin-provided runtime",
            route.playable_id
        ))
    })?;
    if runtime.kind != LIBRETRO_CORE_KIND || runtime.app != route.launcher_id {
        return Err(LaunchError::RouteUnavailable(format!(
            "runtime {} is incompatible with launcher {}",
            runtime.id, route.launcher_id
        )));
    }
    let core_env = runtime.linux_path_env.as_deref().ok_or_else(|| {
        LaunchError::RouteUnavailable(format!(
            "runtime {} has no Linux implementation",
            runtime.id
        ))
    })?;
    let target = route.file_target.as_ref().ok_or_else(|| {
        LaunchError::RouteUnavailable(format!(
            "RetroArch route {} has no file target",
            route.playable_id
        ))
    })?;
    let rom = storage::resolve_file_target(root, snapshot, target)
        .map_err(|error| {
            if error.is_missing_target() {
                LaunchError::RomMissing(error.to_string())
            } else if error.is_storage_access() {
                LaunchError::StorageAccess(error.to_string())
            } else {
                LaunchError::RouteUnavailable(error.to_string())
            }
        })?
        .path;

    let executable = environment_path(&lookup, &launcher.executable_env)?;
    let core = environment_path(&lookup, core_env)?;
    require_file(&executable, "RetroArch executable")?;
    require_file(&core, "mGBA core")?;
    let autoconfig = environment_path(&lookup, RETROARCH_AUTOCONFIG_ENV)?;
    require_directory(&autoconfig, "RetroArch joypad autoconfig")?;

    let account_root = root.join("users").join(DEFAULT_ACCOUNT);
    for directory in ["system", "saves", "states", "screenshots"] {
        fs::create_dir_all(account_root.join(directory))
            .map_err(|error| LaunchError::Config(error.to_string()))?;
    }
    let config = account_root.join("retroarch.cfg");
    write_atomically(
        &config,
        config_content(&account_root, &autoconfig).as_bytes(),
    )
    .map_err(|error| LaunchError::Config(error.to_string()))?;

    Ok(LinuxLaunchSpec {
        command: vec![
            executable.display().to_string(),
            "--config".into(),
            config.display().to_string(),
            "-L".into(),
            core.display().to_string(),
            rom.display().to_string(),
        ],
    })
}

fn environment_path(
    lookup: &impl Fn(&str) -> Option<OsString>,
    key: &str,
) -> Result<std::path::PathBuf, LaunchError> {
    let value = lookup(key).ok_or_else(|| {
        LaunchError::RouteUnavailable(format!(
            "plugin-required environment value {key} is missing"
        ))
    })?;
    let path = std::path::PathBuf::from(value);
    if !safe_absolute_path(&path) {
        return Err(LaunchError::RouteUnavailable(format!(
            "plugin-required environment value {key} is not a safe absolute path"
        )));
    }
    Ok(path)
}

fn require_file(path: &Path, label: &str) -> Result<(), LaunchError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(LaunchError::RomMissing(format!(
            "{label} is not a file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(LaunchError::RomMissing(
            format!("{label} is missing: {}", path.display()),
        )),
        Err(error) => Err(LaunchError::StorageAccess(error.to_string())),
    }
}

fn require_directory(path: &Path, label: &str) -> Result<(), LaunchError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(LaunchError::RomMissing(format!(
            "{label} is not a directory: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(LaunchError::RomMissing(
            format!("{label} is missing: {}", path.display()),
        )),
        Err(error) => Err(LaunchError::StorageAccess(error.to_string())),
    }
}

fn safe_absolute_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
}

fn config_content(root: &Path, autoconfig: &Path) -> String {
    format!(
        r#"# generated by Korri
system_directory = "{system}"
savefile_directory = "{saves}"
savestate_directory = "{states}"
screenshot_directory = "{screenshots}"

input_driver = "udev"
input_joypad_driver = "udev"
joypad_autoconfig_dir = "{autoconfig}"
input_autodetect_enable = "true"
input_max_users = "4"
input_player1_joypad_index = "0"
input_player1_analog_dpad_mode = "1"
# RetroArch value 2 is L3 + R3. Guide remains exclusive to Korri.
input_menu_toggle_gamepad_combo = "2"
# Select + Start exits to Korri (RetroArch 1.22.2 INPUT_COMBO_START_SELECT = 4).
input_quit_gamepad_combo = "4"
quit_press_twice = "false"

kiosk_mode_enable = "true"
menu_driver = "null"
input_overlay_enable = "false"
video_fullscreen = "true"
quit_on_close_content = "true"
config_save_on_exit = "false"
pause_nonactive = "true"
autosave_interval = "10"
savestate_auto_save = "true"
savestate_auto_load = "true"
video_driver = "gl"
"#,
        system = root.join("system").display(),
        saves = root.join("saves").display(),
        states = root.join("states").display(),
        screenshots = root.join("screenshots").display(),
        autoconfig = autoconfig.display(),
    )
}

fn write_atomically(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "config has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".retroarch.cfg.{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
#[path = "linux_retroarch_tests.rs"]
mod storage_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::resolver::{ResolvedFileTarget, ResolvedLinuxLauncher, ResolvedRuntime};
    use std::collections::HashMap;

    fn route() -> ResolvedRoute {
        ResolvedRoute {
            playable_id: "wl4".into(),
            title: Some("Wario Land 4".into()),
            release_id: "gba".into(),
            identity: None,
            provider_id: RETROARCH_PROVIDER.into(),
            system_id: "gba".into(),
            system_title: Some("Game Boy Advance".into()),
            launcher_id: RETROARCH_LAUNCHER.into(),
            launcher_kind: RETROARCH_PROVIDER.into(),
            integration_token: "retroarch".into(),
            flattened_target: "roms:wl4.gba".into(),
            android_component: None,
            linux_launcher: Some(ResolvedLinuxLauncher {
                executable_env: "KORRI_RETROARCH_EXECUTABLE".into(),
            }),
            runtime: Some(ResolvedRuntime {
                id: "@korri:mgba/mgba".into(),
                kind: LIBRETRO_CORE_KIND.into(),
                app: RETROARCH_LAUNCHER.into(),
                path: "/android/mgba.so".into(),
                linux_path_env: Some("KORRI_MGBA_CORE".into()),
            }),
            file_target: Some(ResolvedFileTarget {
                storage_id: storage::IMPLICIT_ROMS_STORAGE_ID.into(),
                path: "wl4.gba".into(),
            }),
        }
    }

    #[test]
    fn builds_a_linux_command_from_plugin_declared_environment_keys() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("roms")).unwrap();
        fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let executable = root.path().join("bin/retroarch");
        let core = root.path().join("cores/mgba.so");
        let autoconfig = root.path().join("share/libretro/autoconfig");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::create_dir_all(core.parent().unwrap()).unwrap();
        fs::create_dir_all(&autoconfig).unwrap();
        fs::write(&executable, b"binary").unwrap();
        fs::write(&core, b"core").unwrap();
        let environment = HashMap::from([
            ("KORRI_RETROARCH_EXECUTABLE", executable.as_os_str()),
            ("KORRI_MGBA_CORE", core.as_os_str()),
            ("KORRI_RETROARCH_AUTOCONFIG", autoconfig.as_os_str()),
        ]);

        let launch =
            launch_route_with_env(root.path(), &ConfigSnapshot::default(), &route(), |key| {
                environment.get(key).map(|value| OsString::from(value))
            })
            .unwrap();

        assert_eq!(launch.command[0], executable.display().to_string());
        assert_eq!(launch.command[4], core.display().to_string());
        assert_eq!(
            launch.command[5],
            root.path().join("roms/wl4.gba").display().to_string()
        );
        let config = fs::read_to_string(root.path().join("users/default/retroarch.cfg")).unwrap();
        assert!(config.contains("users/default/saves"));
        assert!(config.contains("input_driver = \"udev\""));
        assert!(config.contains("input_joypad_driver = \"udev\""));
        assert!(config.contains("input_autodetect_enable = \"true\""));
        assert!(config.contains("input_max_users = \"4\""));
        assert!(config.contains(&format!(
            "joypad_autoconfig_dir = \"{}\"",
            autoconfig.display()
        )));
        // RetroArch value 2 is L3 + R3. Guide remains exclusive to Korri.
        assert!(config.contains("input_menu_toggle_gamepad_combo = \"2\""));
        assert!(!config.contains("input_menu_toggle_btn"));
        let settings: HashMap<_, _> = config
            .lines()
            .filter_map(|line| line.split_once(" = "))
            .collect();
        assert_eq!(settings.get("input_quit_gamepad_combo"), Some(&"\"4\""));
        assert_eq!(settings.get("quit_press_twice"), Some(&"\"false\""));
        // Keep Guide unbound: exit uses semantic Select + Start, not raw buttons.
        assert!(!settings.keys().any(|key| key.ends_with("_btn")));
        assert!(!settings.keys().any(|key| key.contains("guide")));
    }

    #[test]
    fn missing_nix_supplied_core_is_not_fulfillable() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("roms")).unwrap();
        fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let executable = root.path().join("retroarch");
        fs::write(&executable, b"binary").unwrap();

        let error = launch_route_with_env(
            root.path(),
            &ConfigSnapshot::default(),
            &route(),
            |key| match key {
                "KORRI_RETROARCH_EXECUTABLE" => Some(executable.as_os_str().into()),
                "KORRI_MGBA_CORE" => Some(root.path().join("missing.so").into_os_string()),
                _ => None,
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("mGBA core is missing"));
    }
}
