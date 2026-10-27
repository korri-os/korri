//! The callback and all declared effects run inside the game unit, after
//! systemd has selected the runtime user and applied its existing sandbox.
use super::{
    redaction::{status_text, LaunchRedaction},
    ProvisionedFile,
};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub netplay: Option<crate::netplay::PluginLaunchNetplay>,
    pub runner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_id: Option<String>,
    pub program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_path: Option<String>,
    /// File routes supply contentPath; provider locations supply ref instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_path: Option<String>,
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub provider_ref: Option<String>,
    pub account_root: String,
    /// This person's save root for the selected game, independent of its ROM path.
    pub save_root: String,
    /// Reconstructible downloads: the storage root's `cache` directory.
    pub cache_root: String,
    /// The runner's own files under their bare keys, plus each exactly
    /// required, installed and enabled plugin's files under `<pluginId>/<key>`.
    pub files: HashMap<String, String>,
    /// Ready runtime paths, under own bare keys or full foreign artifact ids.
    #[typeshare(serialized_as = "HashMap<String, String>")]
    pub artifacts: std::collections::BTreeMap<String, String>,
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
    /// Whole folders under saveRoot, identified by stable runner-declared keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub save_locations: Vec<crate::saves::types::SaveLocation>,
}

/// The compositor's logical output size, after transform and scaling.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginDisplay {
    pub width: u32,
    pub height: u32,
}

/// ADR 0007 and the first launch.compose treaty. Modifiers own their settings.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginComposeInput {
    pub modifier_id: String,
    pub runner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_id: Option<String>,
    pub plan: PluginLaunchOutput,
    pub settings: HashMap<String, crate::config::cascade::LaunchSettingValue>,
    pub config: crate::config::modifiers::ModifierConfigAppend,
    pub files: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<PluginDisplay>,
}

/// Named plugin settings operations share the nested settings vocabulary.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingsDescribeInput {
    pub id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingsDescribeOutput {
    #[typeshare(serialized_as = "LaunchJsonValue")]
    pub schema: serde_json::Value,
    pub revision: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingsValidateInput {
    pub values: HashMap<String, crate::config::cascade::LaunchSettingValue>,
    pub revision: String,
}

#[typeshare]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginSettingsDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PluginSettingsDiagnostic {
    pub code: String,
    pub severity: PluginSettingsDiagnosticSeverity,
    pub message: String,
    #[serde(default)]
    pub path: Vec<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PluginSettingsValidateOutput {
    pub valid: bool,
    #[serde(default)]
    pub diagnostics: Vec<PluginSettingsDiagnostic>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingsRenderInput {
    pub id: String,
    pub settings: HashMap<String, crate::config::cascade::LaunchSettingValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<PluginDisplay>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingsRenderOutput {
    pub args: Vec<String>,
}

/// Private CLI handoff, not a plugin declaration or persisted configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedLaunchModifier {
    pub modifier_id: String,
    pub manifest: std::path::PathBuf,
    pub entry: String,
    pub settings: HashMap<String, crate::config::cascade::LaunchSettingValue>,
    pub config: crate::config::modifiers::ModifierConfigAppend,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchComposition {
    /// Private native credential source; never passed to a plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub netplay_config_source: Option<std::path::PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunshine_args: Option<Vec<String>>,
    pub modifiers: Vec<ResolvedLaunchModifier>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub args: crate::launch_settings::LaunchArgs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<PluginDisplay>,
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
    let mut input = input.clone();
    if let Some(overrides) = &mut input.overrides {
        super::typed_settings::validate(source, &mut overrides.settings, &input.runner_id, "")?;
    }
    let encoded = serde_json::to_string(&input).map_err(|e| e.to_string())?;
    let result = crate::script::call_plugin_operation_snapshot(
        source,
        crate::script::LAUNCH_PREPARE,
        &encoded,
    )
    .map_err(|failure| failure.to_string())?;
    let plan: PluginLaunchOutput =
        serde_json::from_str(&result).map_err(|e| format!("invalid launch result: {e}"))?;
    validate_save_locations(&input.save_root, &plan.save_locations)?;
    Ok(plan)
}

#[derive(Deserialize)]
struct LaunchManifest {
    entry: String,
    sources: Vec<String>,
    #[serde(default)]
    files: HashMap<String, String>,
}

fn package_snapshot(
    manifest: &Path,
    entry: &str,
) -> Result<(crate::script::source::SourceSnapshot, LaunchManifest), String> {
    let package = manifest
        .parent()
        .ok_or("plugin manifest has no package directory")?;
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(manifest)
        .and_then(|file| file.take(512 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("cannot read plugin manifest {}: {e}", manifest.display()))?;
    if bytes.len() > 512 * 1024 {
        return Err("plugin manifest exceeds 512 KiB".into());
    }
    let manifest: LaunchManifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid plugin manifest: {e}"))?;
    if entry != manifest.entry {
        return Err("launch source is not the packaged plugin entry".into());
    }
    let snapshot = crate::script::source::SourceSnapshot::package_plugin(
        package,
        &manifest.entry,
        &manifest.sources,
    )?;
    Ok((snapshot, manifest))
}

/// All preparation and composition must finish before any declared effect.
pub fn prepare(
    source: &Path,
    input: &PluginLaunchInput,
    composition: &LaunchComposition,
) -> Result<PluginLaunchOutput, String> {
    prepare_with_diagnostics(source, input, composition).map(|prepared| prepared.plan)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreparedLaunch {
    pub(crate) plan: PluginLaunchOutput,
    runner_argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) release_id: Option<String>,
}

/// Private CLI handoff: evaluate once in the host, before journaling saves.
pub(crate) fn prepare_handoff(
    source: &Path,
    input: &PluginLaunchInput,
    composition: &LaunchComposition,
    release_id: &str,
) -> Result<String, String> {
    let mut prepared = prepare_with_diagnostics(source, input, composition)?;
    prepared.release_id = Some(release_id.into());
    serde_json::to_string(&prepared).map_err(|e| e.to_string())
}

/// Host-only preparation stage. The unit accepts only the resulting plugin-launch
/// handoff, never this declaration request. This is not a plugin operation.
pub fn prepare_command(command: &[String]) -> Result<Vec<String>, String> {
    if command.get(1).map(String::as_str) != Some("plugin-prepare") {
        return Ok(command.to_vec());
    }
    if command.len() != 6 {
        return Err("invalid plugin preparation request".into());
    }
    let input: PluginLaunchInput = serde_json::from_str(&command[3]).map_err(|e| e.to_string())?;
    let composition: LaunchComposition =
        serde_json::from_str(&command[4]).map_err(|e| e.to_string())?;
    let handoff = prepare_handoff(Path::new(&command[2]), &input, &composition, &command[5])?;
    let mut command = command.to_vec();
    command[1] = "plugin-launch".into();
    command[5] = handoff;
    Ok(command)
}

/// The runner's own defaults, before place values and modifier composition.
/// Snapshot facts use this effect-free launch.prepare operation, not the final plan.
pub fn runner_prepare_plan(command: &[String]) -> Result<PluginLaunchOutput, String> {
    if !matches!(
        command.get(1).map(String::as_str),
        Some("plugin-prepare" | "plugin-launch")
    ) || command.len() != 6
    {
        return Err("invalid plugin preparation command".into());
    }
    let source = Path::new(&command[2]);
    let package = source
        .parent()
        .ok_or("plugin source has no package directory")?;
    let entry = source
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid plugin entry")?;
    let (snapshot, _) = package_snapshot(&package.join("manifest.json"), entry)?;
    let input: PluginLaunchInput = serde_json::from_str(&command[3]).map_err(|e| e.to_string())?;
    let plan = evaluate_snapshot(&snapshot, &input)?;
    validate_plan(&plan)?;
    Ok(plan)
}

/// Effect-free inspection of the same prepared handoff that the unit executes.
pub fn prepared_plan(command: &[String]) -> Result<PluginLaunchOutput, String> {
    let command = prepare_command(command)?;
    declared_saves(&command)?
        .map(|(_, v)| v.plan)
        .ok_or_else(|| "launch command has no plugin preparation".into())
}

pub fn merge_place_values(plan: &mut PluginLaunchOutput, composition: &LaunchComposition) {
    for (key, value) in &composition.env {
        match value {
            Some(value) => {
                plan.env_unset.retain(|v| v != key);
                plan.env.insert(key.clone(), value.clone());
            }
            None => {
                plan.env.remove(key);
                if !plan.env_unset.contains(key) {
                    plan.env_unset.push(key.clone());
                }
            }
        }
    }
    plan.args = composition
        .args
        .prepend
        .iter()
        .chain(&plan.args)
        .chain(&composition.args.append)
        .cloned()
        .collect();
    if composition.cwd.is_some() {
        plan.cwd = composition.cwd.clone();
    }
}

pub(crate) fn declared_saves(
    command: &[String],
) -> Result<Option<(PluginLaunchInput, PreparedLaunch)>, String> {
    if command.get(1).map(String::as_str) != Some("plugin-launch") {
        return Ok(None);
    }
    if command.len() != 6 {
        return Err("invalid prepared plugin handoff".into());
    }
    let input: PluginLaunchInput = serde_json::from_str(&command[3]).map_err(|e| e.to_string())?;
    let prepared: PreparedLaunch = serde_json::from_str(&command[5]).map_err(|e| e.to_string())?;
    validate_plan(&prepared.plan)?;
    validate_save_locations(&input.save_root, &prepared.plan.save_locations)?;
    Ok(Some((input, prepared)))
}

fn prepare_with_diagnostics(
    source: &Path,
    input: &PluginLaunchInput,
    composition: &LaunchComposition,
) -> Result<PreparedLaunch, String> {
    let mut redaction = LaunchRedaction::from_composition(composition)?;
    let package = source
        .parent()
        .ok_or("plugin source has no package directory")?;
    let entry = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid plugin entry")?;
    let (snapshot, _) = package_snapshot(&package.join("manifest.json"), entry)?;
    let mut plan = evaluate_snapshot(&snapshot, input)?;
    merge_place_values(&mut plan, composition);
    validate_plan(&plan)?;
    redaction.add_env(plan.env.values().cloned());
    let runner_argv = std::iter::once(plan.command.clone())
        .chain(plan.args.clone())
        .collect();
    for modifier in &composition.modifiers {
        let compose = || -> Result<PluginLaunchOutput, String> {
            let (snapshot, manifest) = package_snapshot(&modifier.manifest, &modifier.entry)?;
            for (key, path) in &manifest.files {
                if !Path::new(path).is_absolute() || !Path::new(path).exists() {
                    return Err(format!(
                        "modifier file {key} is missing or not absolute: {path}"
                    ));
                }
            }
            let mut settings = modifier.settings.clone();
            super::typed_settings::validate(
                &snapshot,
                &mut settings,
                &modifier.modifier_id,
                &modifier.manifest.display().to_string(),
            )?;
            let request = PluginComposeInput {
                modifier_id: modifier.modifier_id.clone(),
                runner_id: input.runner_id.clone(),
                family_id: input.family_id.clone(),
                plan: plan.clone(),
                settings,
                config: modifier.config.clone(),
                files: manifest.files,
                display: composition.display.clone(),
            };
            let result = crate::script::call_plugin_operation_snapshot(
                &snapshot,
                crate::script::LAUNCH_COMPOSE,
                &serde_json::to_string(&request).map_err(|e| e.to_string())?,
            )
            .map_err(|failure| redaction.operation_failure(&failure))?;
            let output: PluginLaunchOutput = serde_json::from_str(&result).map_err(|error| {
                status_text(
                    &redaction.text(&format!("invalid compose result: {error}")),
                    300,
                )
            })?;
            validate_plan(&output)?;
            validate_save_locations(&input.save_root, &output.save_locations)?;
            Ok(output)
        };
        plan = compose().map_err(|reason| {
            format!(
                "modifier {}: {}",
                modifier.modifier_id,
                status_text(&reason, 1024)
            )
        })?;
    }
    redaction.add_env(plan.env.values().cloned());
    Ok(PreparedLaunch {
        plan,
        runner_argv,
        release_id: None,
    })
}

pub(crate) fn sunshine_args(command: &[String]) -> Result<Option<Vec<String>>, String> {
    if command.get(1).map(String::as_str) != Some("plugin-launch") {
        return Ok(None);
    }
    let composition: LaunchComposition =
        serde_json::from_str(command.get(4).ok_or("missing launch composition")?)
            .map_err(|e| e.to_string())?;
    Ok(composition.sunshine_args)
}

/// Resolve existing ancestors too, because launch declarations precede mkdir.
/// Canonicalization follows symlinks before checking the save-root boundary.
pub fn validate_save_locations(
    root: &str,
    locations: &[crate::saves::types::SaveLocation],
) -> Result<(), String> {
    use std::path::{Component, PathBuf};
    fn resolve(path: &Path) -> Result<PathBuf, String> {
        if !path.is_absolute() {
            return Err("save paths must be absolute".into());
        }
        let mut result = PathBuf::new();
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    result.pop();
                }
                Component::CurDir => {}
                other => {
                    result.push(other.as_os_str());
                    match fs::symlink_metadata(&result) {
                        Ok(_) => {
                            result = result.canonicalize().map_err(|e| e.to_string())?;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
            }
        }
        Ok(result)
    }
    let root = resolve(Path::new(root))?;
    let mut keys = std::collections::BTreeSet::new();
    for location in locations {
        if location.key.is_empty() || location.key.contains('\0') || !keys.insert(&location.key) {
            return Err("save location keys must be non-empty and unique".into());
        }
        let path = resolve(Path::new(&location.path))?;
        if path == root || !path.starts_with(&root) {
            return Err("save location must resolve inside saveRoot".into());
        }
        if path.exists() && !path.is_dir() {
            return Err("save location must be a whole folder".into());
        }
    }
    Ok(())
}

fn validate_plan(plan: &PluginLaunchOutput) -> Result<(), String> {
    if plan.command.is_empty() || plan.command.contains('\0') {
        return Err("launch command must be non-empty and contain no NUL".into());
    }
    if plan.args.iter().any(|v| v.contains('\0'))
        || plan.cwd.as_ref().is_some_and(|v| v.contains('\0'))
    {
        return Err("launch arguments and cwd must contain no NUL".into());
    }
    if plan
        .env
        .iter()
        .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
        || plan
            .env_unset
            .iter()
            .any(|key| key.is_empty() || key.contains(['=', '\0']))
    {
        return Err("launch environment contains an invalid name or NUL".into());
    }
    Ok(())
}

/// Used by the host before the unit starts. The handoff is authored only by korrid.
pub(crate) fn has_modifiers(command: &[String]) -> bool {
    command
        .get(1)
        .is_some_and(|v| v == "plugin-launch" || v == "plugin-prepare")
        && command
            .get(4)
            .and_then(|v| serde_json::from_str::<LaunchComposition>(v).ok())
            .is_some_and(|v| !v.modifiers.is_empty())
}

/// CLI-only entrypoint. Refuse root even when invoked outside the unit.
pub fn execute(
    _source: &Path,
    input: &str,
    composition: &str,
    prepared: &str,
) -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 {
        return Err("plugin launch effects require an unprivileged runtime user".into());
    }
    let input: PluginLaunchInput = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let composition: LaunchComposition =
        serde_json::from_str(composition).map_err(|e| e.to_string())?;
    let PreparedLaunch {
        plan: output,
        runner_argv,
        ..
    } = serde_json::from_str::<PreparedLaunch>(prepared).map_err(|e| e.to_string())?;
    validate_plan(&output)?;
    validate_save_locations(&input.save_root, &output.save_locations)?;
    let mut redaction = LaunchRedaction::from_composition(&composition)?;
    redaction.add_env(output.env.values().cloned());
    let mut environment: std::collections::BTreeMap<_, _> = std::env::vars().collect();
    for key in &output.env_unset {
        environment.remove(key);
    }
    environment.extend(output.env.clone());
    redaction.add_env(environment.into_values());
    crate::host::launch_diagnostics::write_redaction_words(&redaction.words())?;
    // Only native append words and environment values are private. Keep the
    // fixed/typed flags and the runner command/arguments useful for diagnosis.
    eprintln!(
        "korrid plugin-launch plan: {}",
        serde_json::json!({
            "command": output.command,
            "args": redaction.argv(&output.args, &runner_argv),
            "env": output.env.keys().map(|key| (key, "[redacted]")).collect::<std::collections::BTreeMap<_,_>>(),
            "envUnset": output.env_unset, "cwd": output.cwd,
        })
    );
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
    if input.netplay.is_some() {
        crate::netplay::password::install()?;
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
        let status = run_as_namespace_init(command)?;
        if let Err(error) = crate::host::launch_diagnostics::write_exit_status(status) {
            eprintln!("korrid plugin-launch exit diagnostics: {error}");
        }
        std::process::exit(status);
    }
    Err(format!("cannot exec composed launch: {}", command.exec()))
}

/// Native systemd status survives this process's early exit. It is then read
/// by the same UnitFailed path as ExecMainStatus, without reading the journal.
pub fn notify_failure(reason: &str) {
    use std::os::{
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStrExt,
            net::{SocketAddr, UnixDatagram},
        },
    };
    let Some(socket) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };
    let send = || -> std::io::Result<()> {
        let socket = socket.as_bytes();
        let address = match socket.strip_prefix(b"@") {
            Some(name) => SocketAddr::from_abstract_name(name)?,
            None => SocketAddr::from_pathname(std::ffi::OsStr::from_bytes(socket))?,
        };
        let datagram = UnixDatagram::unbound()?;
        datagram.connect_addr(&address)?;
        let reason = status_text(reason, 1024);
        datagram.send(format!("STATUS=plugin-launch: {reason}").as_bytes())?;
        Ok(())
    };
    if let Err(error) = send() {
        eprintln!("korrid plugin-launch: cannot report failure status: {error}");
    }
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

#[cfg(test)]
mod place_merge_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn layer_environment_unsets_arguments_and_cwd_merge_once_around_plugin_values() {
        let mut plan:PluginLaunchOutput=serde_json::from_value(json!({"command":"/bin/true","args":["plugin"],"env":{"KEEP":"plugin","CHANGE":"plugin","REMOVE":"plugin"},"envUnset":["SET"],"cwd":"/plugin"})).unwrap();
        let composition:LaunchComposition=serde_json::from_value(json!({"modifiers":[],"env":{"CHANGE":"layer","REMOVE":null,"SET":"layer"},"args":{"prepend":["before"],"append":["after"]},"cwd":"/layer"})).unwrap();
        merge_place_values(&mut plan, &composition);
        assert_eq!(plan.args, ["before", "plugin", "after"]);
        assert_eq!(plan.cwd.as_deref(), Some("/layer"));
        assert_eq!(
            plan.env,
            HashMap::from([
                ("KEEP".into(), "plugin".into()),
                ("CHANGE".into(), "layer".into()),
                ("SET".into(), "layer".into())
            ])
        );
        assert_eq!(plan.env_unset, ["REMOVE"]);
    }
}
