//! The callback and all declared effects run inside the game unit, after
//! systemd has selected the runtime user and applied its existing sandbox.
use super::ProvisionedFile;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, os::unix::process::CommandExt, path::Path, process::Command};
use typeshare::typeshare;

/// Legacy LaunchOverrides.config from library-item.ts; RetroArch rejects replace.
#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LaunchConfigOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginLaunchOverrides {
    #[serde(default)]
    pub settings: HashMap<String, crate::config::cascade::LaunchSettingValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<LaunchConfigOverrides>,
}

/// Selected runner facts and named files for launch preparation.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginLaunchInput {
    pub runner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_id: Option<String>,
    pub program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_path: Option<String>,
    pub content_path: String,
    pub account_root: String,
    pub files: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<PluginLaunchOverrides>,
}

/// Legacy launch plan plus the existing provisioned files/directories.
/// Approval authorizes output; this treaty adds no path or argument policy.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginLaunchOutput {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub env_unset: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default)]
    pub directories: Vec<String>,
    #[serde(default)]
    pub files: Vec<ProvisionedFile>,
}

pub fn evaluate(source: &str, input: &PluginLaunchInput) -> Result<PluginLaunchOutput, String> {
    evaluate_snapshot(
        &crate::script::source::SourceSnapshot::plugin(source)?,
        input,
    )
}

pub fn evaluate_snapshot(
    source: &crate::script::source::SourceSnapshot,
    input: &PluginLaunchInput,
) -> Result<PluginLaunchOutput, String> {
    let input = serde_json::to_string(input).map_err(|e| e.to_string())?;
    let result = crate::script::call_plugin_operation_snapshot(
        source,
        crate::script::LAUNCH_PREPARE,
        &input,
    )
    .map_err(|failure| failure.to_string())?;
    serde_json::from_str(&result).map_err(|e| format!("invalid launch result: {e}"))
}

#[derive(Deserialize)]
struct LaunchManifest {
    entry: String,
    sources: Vec<String>,
}

/// CLI-only entrypoint. Refuse root even when invoked outside the unit.
pub fn execute(source: &Path, input: &str) -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 {
        return Err("plugin launch effects require an unprivileged runtime user".into());
    }
    let input: PluginLaunchInput = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let package = source
        .parent()
        .ok_or("plugin source has no package directory")?;
    // The packaged manifest is the only source authority. There is no
    // single-file fallback: an unpackaged path cannot name its module graph.
    let manifest = fs::read(package.join("manifest.json")).map_err(|e| e.to_string())?;
    if manifest.len() > 512 * 1024 {
        return Err("plugin manifest exceeds 512 KiB".into());
    }
    let manifest: LaunchManifest = serde_json::from_slice(&manifest).map_err(|e| e.to_string())?;
    if source.file_name().and_then(|name| name.to_str()) != Some(manifest.entry.as_str()) {
        return Err("launch source is not the packaged plugin entry".into());
    }
    let snapshot = crate::script::source::SourceSnapshot::package_plugin(
        package,
        &manifest.entry,
        &manifest.sources,
    )?;
    let output = evaluate_snapshot(&snapshot, &input)?;
    for directory in &output.directories {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    }
    for file in &output.files {
        let path = Path::new(&file.path);
        let parent = path.parent().ok_or("launch file has no parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        // Preserve atomic replacement, now with the runtime user's authority.
        use std::io::Write;
        let temporary = parent.join(format!(".korri-launch-{:016x}.tmp", rand::random::<u64>()));
        let result = (|| -> std::io::Result<()> {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            output.write_all(file.content.as_bytes())?;
            output.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|e| e.to_string())?;
        fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|e| e.to_string())?;
    }
    let mut command = Command::new(output.command);
    command.args(output.args);
    for key in output.env_unset {
        command.env_remove(key);
    }
    command.envs(output.env);
    if let Some(cwd) = output.cwd {
        command.current_dir(cwd);
    }
    // The game unit sets PrivatePIDs=yes, so this process is PID 1 of the
    // game's PID namespace. The kernel drops every signal sent to a namespace
    // init that has no handler for it, so a game exec'd here without a SIGTERM
    // handler ignores systemd's stop until the stop timeout and SIGKILL.
    if unsafe { libc::getpid() } == 1 {
        std::process::exit(run_as_namespace_init(command)?);
    }
    Err(command.exec().to_string())
}

/// Run the game as an ordinary process and wait for it, reaping orphans.
///
/// The unit's KillMode=control-group delivers stop signals to every process
/// in the unit, so the game receives SIGTERM itself; this init forwards none.
/// Returns the game's exit status in the shell's encoding.
fn run_as_namespace_init(mut command: Command) -> Result<i32, String> {
    let game = command.spawn().map_err(|e| e.to_string())?.id() as libc::pid_t;
    loop {
        let mut status = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status, 0) };
        if pid == -1 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("waiting for the game failed: {error}"));
        }
        if pid == game {
            return Ok(exit_code(status));
        }
    }
}

fn exit_code(status: libc::c_int) -> i32 {
    if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else if libc::WIFSIGNALED(status) {
        128 + libc::WTERMSIG(status)
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::exit_code;

    #[test]
    fn exit_code_keeps_the_game_status() {
        assert_eq!(exit_code(0), 0);
        assert_eq!(exit_code(7 << 8), 7);
        assert_eq!(exit_code(libc::SIGTERM), 128 + libc::SIGTERM);
        assert_eq!(exit_code(libc::SIGKILL | 0x80), 128 + libc::SIGKILL);
    }
}
