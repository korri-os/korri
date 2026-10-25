use crate::{
    error::CliError,
    library::{self, required, value},
    module::{Context, Module},
    output::Output,
    prompt,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use korrid::{launch_settings::*, RpcRequest, RpcResponse};
use serde_json::Value;
use std::collections::HashMap;
pub struct LaunchSettings;
fn flags(command: Command) -> Command {
    command
        .arg(Arg::new("system").long("system").conflicts_with_all(["release", "profile"]).help("System write layer; optional --game supplies resolution context, not the write layer"))
        .arg(Arg::new("game").long("game"))
        .arg(
            Arg::new("release")
                .long("release")
                .requires("game")
                .conflicts_with("profile"),
        )
        .arg(Arg::new("profile").long("profile").requires("game"))
        .arg(Arg::new("runner").long("runner").conflicts_with("family"))
        .arg(Arg::new("family").long("family"))
}
impl Module for LaunchSettings {
    fn command(&self) -> Command {
        Command::new("launch-settings")
            .about("Show effective launch values and edit authorized settings")
            .subcommand_required(true)
            .subcommand(flags(Command::new("show")))
            .subcommand(flags(
                Command::new("set")
                    .arg(required("assignment").help("KEY=VALUE, or KEY with --inherit"))
                    .arg(inherit_flag()),
            ))
            .subcommand(flags(
                Command::new("runner")
                    .about("Set the runner choice at an any-runner place")
                    .arg(
                        Arg::new("id")
                            .required_unless_present("inherit")
                            .conflicts_with("inherit"),
                    )
                    .arg(inherit_flag()),
            ))
            .subcommand(
                Command::new("profiles")
                    .subcommand_required(true)
                    .subcommand(
                        Command::new("list").arg(Arg::new("game").long("game").required(true)),
                    )
                    .subcommand(
                        Command::new("new")
                            .arg(required("title"))
                            .arg(Arg::new("game").long("game").required(true)),
                    )
                    .subcommand(
                        Command::new("delete")
                            .arg(required("id"))
                            .arg(Arg::new("game").long("game").required(true)),
                    )
                    .subcommand(
                        Command::new("use")
                            .arg(
                                Arg::new("id")
                                    .required_unless_present("none")
                                    .conflicts_with("none")
                                    .help("Exact profile ID"),
                            )
                            .arg(
                                Arg::new("none")
                                    .long("none")
                                    .action(ArgAction::SetTrue)
                                    .help("Select no profile; keep all profiles"),
                            )
                            .arg(Arg::new("game").long("game").required(true)),
                    ),
            )
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        let (mut action, mut args) = args.subcommand().expect("required subcommand");
        let profiles = action == "profiles";
        if profiles {
            (action, args) = args.subcommand().expect("required subcommand");
        }
        if action == "set" {
            let assignment = value(args, "assignment");
            if args.get_flag("inherit") {
                if assignment.is_empty() || assignment.contains('=') {
                    return Err(CliError::usage(
                        "Use a bare key with --inherit; do not also assign a value.",
                    ));
                }
            } else {
                split(&assignment)?;
            }
        }
        let mut game_id = args.get_one::<String>("game").cloned();
        if let Some(query) = &game_id {
            game_id = Some(
                match library::game(context, query, args.get_flag("json"))? {
                    Ok(id) => id,
                    Err(r) => return Ok(library::finish(r)),
                },
            );
        }
        let response = context.rpc(&RpcRequest::LaunchSettingsSnapshot(
            LaunchSettingsSnapshotRequest {
                game_id: game_id.clone(),
            },
        ))?;
        let RpcResponse::LaunchSettingsSnapshot(LaunchSettingsSnapshotOutcome::Ok(snapshot)) =
            &response
        else {
            return Ok(library::finish(response));
        };
        if profiles && action == "list" {
            let human = snapshot
                .profiles
                .iter()
                .map(|p| {
                    format!(
                        "{}\t{}{}",
                        crate::output::text(&p.id),
                        crate::output::text(&p.title),
                        if snapshot.selected_profile.as_ref() == Some(&p.id) {
                            " (selected)"
                        } else {
                            ""
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(Output::one(response, human));
        }
        if action == "show" {
            let place = selected_place(args, game_id.clone());
            require_place(snapshot, &place)?;
            let mut lines = vec![
                format!(
                    "Place: {}",
                    serde_json::to_string(&place).expect("place serializes")
                ),
                if game_id.is_some() {
                    "Effective values (resolved game context, with origins):".into()
                } else {
                    "Effective values (Device context, with origins; authored place below):".into()
                },
            ];
            let mut values = snapshot.resolved.values.iter().collect::<Vec<_>>();
            values.sort_by_key(|(key, _)| *key);
            for (key, value) in values {
                lines.push(format!(
                    "{} = {} [origin: {}]{}",
                    crate::output::text(key),
                    value.value,
                    serde_json::to_string(&value.source).expect("source serializes"),
                    if value.adjusted { " (adjusted)" } else { "" }
                ));
            }
            lines.push("Authorized places and plugin schemas:".into());
            lines.push(serde_json::to_string_pretty(snapshot).expect("snapshot serializes"));
            return Ok(Output::one(
                response,
                lines
                    .join("\n")
                    .lines()
                    .map(crate::output::text)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }
        let request = if profiles {
            let game_id = game_id.expect("required game");
            let expected_revision = snapshot.revisions.games.clone();
            match action {
                "new" => RpcRequest::LaunchProfileSave(LaunchProfileSaveRequest {
                    game_id,
                    profile_id: None,
                    title: value(args, "title"),
                    expected_revision,
                }),
                "delete" => {
                    let id = value(args, "id");
                    if !snapshot.profiles.iter().any(|p| p.id == id) {
                        return Err(CliError::usage("Unknown profile ID. List profiles first."));
                    }
                    prompt::confirm("Delete this launch profile?", context.yes)?;
                    RpcRequest::LaunchProfileRemove(LaunchProfileRemoveRequest {
                        game_id,
                        profile_id: id,
                        expected_revision,
                    })
                }
                "use" => {
                    let id = args.get_one::<String>("id").cloned();
                    if id
                        .as_ref()
                        .is_some_and(|id| !snapshot.profiles.iter().any(|p| &p.id == id))
                    {
                        return Err(CliError::usage("Unknown profile ID. List profiles first."));
                    }
                    RpcRequest::LaunchProfileSelect(LaunchProfileSelectRequest {
                        game_id,
                        profile_id: id,
                        expected_revision,
                    })
                }
                _ => return Err(CliError::usage("Choose a profiles action.")),
            }
        } else {
            let place = selected_place(args, game_id);
            require_place(snapshot, &place)?;
            let inherit = args.get_flag("inherit");
            let (key, input) = if action == "runner" {
                (
                    "runner".into(),
                    if inherit {
                        None
                    } else {
                        Some(value(args, "id"))
                    },
                )
            } else if inherit {
                (value(args, "assignment"), None)
            } else {
                let (key, input) = split(&value(args, "assignment"))?;
                (key, Some(input))
            };
            let (path, val) = setting(snapshot, &place, &key, input.as_deref())?;
            let expected_revision = match place.layer {
                LaunchLayer::Device | LaunchLayer::System(_) => snapshot.revisions.device.clone(),
                _ => snapshot.revisions.games.clone(),
            };
            RpcRequest::LaunchSettingsSet(LaunchSetRequest {
                place,
                path,
                value: val,
                expected_revision,
            })
        };
        let response = context.rpc(&request)?;
        if let RpcRequest::LaunchSettingsSet(change) = &request {
            if crate::output::failure(&response).is_none() {
                let key = if action == "runner" {
                    "runner".into()
                } else {
                    value(args, "assignment")
                        .split('=')
                        .next()
                        .expect("key")
                        .to_owned()
                };
                let action = match &change.value {
                    None => format!("Cleared the {key} override"),
                    Some(value) => format!("Set {key} = {value}"),
                };
                let mut show = "korri launch-settings show".to_owned();
                for name in [
                    "system", "game", "release", "profile", "runner", "family", "socket",
                ] {
                    if let Some(value) = args.get_one::<String>(name) {
                        show.push_str(&format!(" --{name} {}", quote(value)));
                    }
                }
                if let Some(device) = &context.device {
                    show.push_str(&format!(" --device {}", quote(device)));
                }
                let human = format!(
                    "{} at {}.\nNext: {show} to inspect current effective values and origins.",
                    crate::output::text(&action),
                    serde_json::to_string(&change.place).expect("place serializes")
                );
                return Ok(Output::one(response, human));
            }
        }
        Ok(library::finish(response))
    }
}
fn inherit_flag() -> Arg {
    Arg::new("inherit")
        .long("inherit")
        .action(ArgAction::SetTrue)
        .help("Clear this authored override; keep inherited values")
}
fn quote(value: &str) -> String {
    format!("'{}'", crate::output::text(value).replace('\'', "'\\''"))
}
fn selected_place(args: &ArgMatches, game_id: Option<String>) -> LaunchPlace {
    let layer = if let Some(id) = args.get_one::<String>("system") {
        LaunchLayer::System(id.clone())
    } else if let Some(game_id) = game_id {
        if let Some(id) = args.get_one::<String>("release") {
            LaunchLayer::Release {
                game_id,
                release_id: id.clone(),
            }
        } else if let Some(id) = args.get_one::<String>("profile") {
            LaunchLayer::Profile {
                game_id,
                profile_id: id.clone(),
            }
        } else {
            LaunchLayer::Game(game_id)
        }
    } else {
        LaunchLayer::Device
    };
    let scope = if let Some(id) = args.get_one::<String>("runner") {
        LaunchRunnerScope::Runner(id.clone())
    } else if let Some(id) = args.get_one::<String>("family") {
        LaunchRunnerScope::Family(id.clone())
    } else {
        LaunchRunnerScope::Any
    };
    LaunchPlace { layer, scope }
}
fn require_place(snapshot: &LaunchSnapshot, place: &LaunchPlace) -> Result<(), CliError> {
    if snapshot.places.iter().filter(|p| p.place == *place).count() != 1 {
        return Err(CliError::usage("The snapshot does not expose this exact layer and runner scope. Check the system ID and available runner scopes; optional --game must resolve to that system. No setting was changed."));
    }
    Ok(())
}
pub(crate) fn split(input: &str) -> Result<(String, String), CliError> {
    let (key, value) = input
        .split_once('=')
        .ok_or_else(|| CliError::usage("Use key=value."))?;
    if key.is_empty() {
        return Err(CliError::usage("A setting key is required."));
    }
    Ok((key.into(), value.into()))
}
fn runners<'a>(
    snapshot: &'a LaunchSnapshot,
    scope: &LaunchRunnerScope,
) -> Vec<&'a LaunchRunnerSnapshot> {
    snapshot
        .runners
        .iter()
        .filter(|r| match scope {
            LaunchRunnerScope::Any => r.selected,
            LaunchRunnerScope::Runner(id) => &r.id == id,
            LaunchRunnerScope::Family(id) => r.family.as_ref() == Some(id),
        })
        .collect()
}
fn scalar(schema: &Value, input: &str) -> Result<Value, CliError> {
    let value = match schema["type"].as_str() {
        Some("string") => Value::String(input.into()),
        Some("boolean" | "integer" | "number") => serde_json::from_str(input)
            .map_err(|_| CliError::usage("Value does not match the plugin setting schema."))?,
        None => serde_json::from_str(input).unwrap_or_else(|_| Value::String(input.into())),
        _ => {
            return Err(CliError::usage(
                "This setting has no supported scalar schema; edit its config file instead.",
            ))
        }
    };
    let valid = match schema["type"].as_str() {
        Some("boolean") => value.is_boolean(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("number") => value.is_number(),
        Some("string") => value.is_string(),
        None => value.is_boolean() || value.is_string() || value.is_number(),
        _ => false,
    };
    if !valid
        || schema["enum"]
            .as_array()
            .is_some_and(|v| !v.contains(&value))
        || value.as_f64().is_some_and(|n| {
            schema["minimum"].as_f64().is_some_and(|m| n < m)
                || schema["maximum"].as_f64().is_some_and(|m| n > m)
        })
    {
        return Err(CliError::usage(
            "Value does not match the plugin setting schema.",
        ));
    }
    Ok(value)
}
pub(crate) fn setting(
    snapshot: &LaunchSnapshot,
    place: &LaunchPlace,
    key: &str,
    input: Option<&str>,
) -> Result<(LaunchPath, Option<Value>), CliError> {
    if ["hooks", "env", "args", "cwd", "config"]
        .iter()
        .any(|k| key == *k || key.starts_with(&format!("{k}.")))
    {
        return Err(CliError::usage("Hooks, environment, arguments, working directory and raw config are read-only. Edit the device config files."));
    }
    let one_launch = matches!(place.layer, LaunchLayer::Launch);
    if !one_launch {
        require_place(snapshot, place)?;
    }
    let authorized = |kind: &str| {
        snapshot.places.iter().any(|p| {
            (if one_launch {
                p.place.scope == place.scope
            } else {
                p.place == *place
            }) && p.editable_kinds.iter().any(|k| k == kind)
        })
    };
    let mut candidates: Vec<(LaunchPath, Option<Value>)> = vec![];
    let mut options: Vec<(LaunchPath, Value)> = vec![];
    // Match whole schema keys and whole modifier IDs, never split plugin IDs on dots.
    for runner in runners(snapshot, &place.scope) {
        if let Some(property) = runner.schema["properties"].get(key) {
            if authorized("settings") {
                options.push((LaunchPath::Setting { key: key.into() }, property.clone()));
            }
        }
    }
    for modifier in &snapshot.modifiers {
        if !authorized("modifiers") {
            continue;
        }
        if key == format!("{}.enabled", modifier.id) {
            options.push((
                LaunchPath::ModifierEnabled {
                    id: modifier.id.clone(),
                },
                serde_json::json!({"type":"boolean"}),
            ));
        }
        if let Some(properties) = modifier
            .schema
            .as_ref()
            .and_then(|s| s["properties"].as_object())
        {
            for (name, property) in properties {
                if key == format!("{}.{name}", modifier.id) {
                    options.push((
                        LaunchPath::ModifierSetting {
                            id: modifier.id.clone(),
                            key: name.clone(),
                        },
                        property.clone(),
                    ));
                }
            }
        }
    }
    // Family runners may advertise one path with several schemas. Do not guess
    // a scalar type, or conflate that path with a modifier's dotted setting.
    let paths = options
        .iter()
        .map(|(path, _)| serde_json::to_string(path).expect("path serializes"))
        .collect::<std::collections::BTreeSet<_>>();
    if paths.len() > 1 {
        return Err(CliError::usage("This key can name both a runner setting and a modifier setting. Choose a narrower runner scope; Korri will not guess."));
    }
    let mut parsed = Vec::new();
    for (path, schema) in options {
        if let Some(input) = input {
            if let Ok(value) = scalar(&schema, input) {
                parsed.push((path, Some(value)));
            }
        } else {
            parsed.push((path, None));
        }
    }
    if parsed.iter().any(|(_, v)| v != &parsed[0].1) {
        return Err(CliError::usage("Runner family schemas give this value different meanings. Use an exact --runner scope."));
    }
    if let Some(candidate) = parsed.pop() {
        candidates.push(candidate);
    } else if !paths.is_empty() {
        return Err(CliError::usage(
            "Value does not match the plugin setting schema.",
        ));
    }
    let builtin = match key {
        "cpu.min" if authorized("cpu") => Some(LaunchPath::CpuMin),
        "cpu.max" if authorized("cpu") => Some(LaunchPath::CpuMax),
        "gpu.min" if authorized("gpu") => Some(LaunchPath::GpuMin),
        "gpu.max" if authorized("gpu") => Some(LaunchPath::GpuMax),
        "cpu.governor"
            if !one_launch
                && authorized("cpu")
                && matches!(place.layer, LaunchLayer::Device)
                && matches!(place.scope, LaunchRunnerScope::Any) =>
        {
            Some(LaunchPath::CpuGovernor)
        }
        "runner"
            if !one_launch
                && authorized("runner")
                && matches!(place.scope, LaunchRunnerScope::Any) =>
        {
            Some(LaunchPath::Runner)
        }
        _ => None,
    };
    if let Some(path) = builtin {
        let val = if let Some(input) = input {
            Some(match path {
                LaunchPath::CpuGovernor => {
                    if !snapshot.device_facts.as_ref().is_some_and(|f| {
                        !f.cpu_policies.is_empty()
                            && f.cpu_policies
                                .iter()
                                .all(|p| p.available_governors.iter().any(|v| v == input))
                    }) {
                        return Err(CliError::usage(
                            "CPU governor is not advertised by every device CPU policy.",
                        ));
                    }
                    Value::String(input.into())
                }
                LaunchPath::Runner => {
                    if !snapshot.runners.iter().any(|r| r.id == input) {
                        return Err(CliError::usage(
                            "Choose an installed runner ID from launch-settings show.",
                        ));
                    }
                    Value::String(input.into())
                }
                _ => {
                    let n = input
                        .parse::<u8>()
                        .ok()
                        .filter(|n| {
                            *n <= 100
                                && (*n > 0
                                    || matches!(path, LaunchPath::CpuMin | LaunchPath::GpuMin))
                        })
                        .ok_or_else(|| {
                            CliError::usage(
                                "Limits are integer percentages: minimum 0..100, maximum 1..100.",
                            )
                        })?;
                    Value::from(n)
                }
            })
        } else {
            None
        };
        candidates.push((path, val));
    }
    if candidates.len() != 1 {
        return Err(CliError::usage("Unknown, ambiguous or read-only launch setting. Check launch-settings show and the plugin schema; Korri will not guess."));
    }
    Ok(candidates.remove(0))
}
pub(crate) fn overrides(
    snapshot: &LaunchSnapshot,
    runner: &str,
    assignments: &[String],
) -> Result<LaunchOverride, CliError> {
    let place = LaunchPlace {
        layer: LaunchLayer::Launch,
        scope: LaunchRunnerScope::Runner(runner.into()),
    };
    let mut launch = LaunchOverride::default();
    for assignment in assignments {
        let (key, input) = split(assignment)?;
        let (path, value) = setting(snapshot, &place, &key, Some(&input))?;
        let value = value.expect("one-launch assignment");
        match path {
            LaunchPath::CpuMin => {
                launch.cpu.get_or_insert_default().min =
                    Some(value.as_u64().expect("percent") as u8)
            }
            LaunchPath::CpuMax => {
                launch.cpu.get_or_insert_default().max =
                    Some(value.as_u64().expect("percent") as u8)
            }
            LaunchPath::GpuMin => {
                launch.gpu.get_or_insert_default().min =
                    Some(value.as_u64().expect("percent") as u8)
            }
            LaunchPath::GpuMax => {
                launch.gpu.get_or_insert_default().max =
                    Some(value.as_u64().expect("percent") as u8)
            }
            LaunchPath::Setting { key } => {
                let settings = launch.settings.get_or_insert_with(|| LaunchScopedSettings {
                    scope: place.scope.clone(),
                    values: HashMap::new(),
                });
                settings.values.insert(
                    key,
                    serde_json::from_value(value)
                        .map_err(|_| CliError::usage("Setting must be scalar."))?,
                );
            }
            LaunchPath::ModifierEnabled { id } => {
                launch.modifiers.entry(id).or_default().enabled = value.as_bool()
            }
            LaunchPath::ModifierSetting { id, key } => {
                launch.modifiers.entry(id).or_default().settings.insert(
                    key,
                    serde_json::from_value(value)
                        .map_err(|_| CliError::usage("Setting must be scalar."))?,
                );
            }
            _ => {
                return Err(CliError::usage(
                    "This setting cannot be changed for one launch.",
                ))
            }
        }
    }
    for limits in [&launch.cpu, &launch.gpu].into_iter().flatten() {
        if limits.min.zip(limits.max).is_some_and(|(a, b)| a > b) {
            return Err(CliError::usage(
                "Minimum limit cannot exceed maximum limit.",
            ));
        }
    }
    Ok(launch)
}
