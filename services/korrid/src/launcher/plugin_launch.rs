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
    )?;
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
    Err(command.exec().to_string())
}
