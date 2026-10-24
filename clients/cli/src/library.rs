use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    prompt,
};
use clap::{Arg, ArgMatches, Command};
use korrid::*;

pub struct Library;
pub(crate) fn required(name: &'static str) -> Arg {
    Arg::new(name).required(true)
}
pub(crate) fn value(args: &ArgMatches, name: &str) -> String {
    args.get_one::<String>(name)
        .expect("required argument")
        .clone()
}
pub(crate) fn finish(response: RpcResponse) -> Output {
    use korrid::{acquisition::*, launch_settings::*};
    let mut human = match &response {
        RpcResponse::LibraryFoldersSnapshot(LibraryFoldersSnapshotOutcome::Ok(snapshot))
        | RpcResponse::LibraryFolderAdd(LibraryFoldersSnapshotOutcome::Ok(snapshot))
        | RpcResponse::LibraryFolderRemove(LibraryFoldersSnapshotOutcome::Ok(snapshot))
        | RpcResponse::LibraryFoldersRescan(LibraryFoldersSnapshotOutcome::Ok(snapshot)) => {
            let state = match snapshot.state {
                LibraryFoldersState::Idle {} => "idle",
                LibraryFoldersState::Scanning {} => "scanning",
                LibraryFoldersState::Enriching {} => "enriching",
                LibraryFoldersState::Problem {} => "needs attention",
            };
            let mut lines = vec![format!(
                "Library: {state}; {} attention items",
                snapshot.attention
            )];
            lines.extend(snapshot.folders.iter().map(|folder| {
                format!(
                    "{}\t{}\t{}",
                    output::text(&folder.id),
                    output::text(&folder.label),
                    output::text(&folder.system)
                )
            }));
            lines.extend(
                snapshot
                    .diagnostics
                    .iter()
                    .map(|d| format!("Warning: {}", output::text(&d.message))),
            );
            lines.join("\n")
        }
        RpcResponse::AttentionList(AttentionOutcome::Ok(list))
        | RpcResponse::AttentionAssign(AttentionOutcome::Ok(list))
        | RpcResponse::AttentionIgnore(AttentionOutcome::Ok(list)) => {
            let mut lines = vec![format!("Attention: {} items", list.items.len())];
            lines.extend(list.items.iter().map(|item| {
                format!(
                    "{}\t{}\t{}",
                    output::text(&item.id),
                    item.path
                        .as_deref()
                        .map(output::text)
                        .unwrap_or_else(|| output::text(&item.source)),
                    output::text(&item.message)
                )
            }));
            lines.join("\n")
        }
        RpcResponse::AcquisitionStatus(AcquisitionOutcome::Ok(snapshot))
        | RpcResponse::AcquisitionStart(AcquisitionOutcome::Ok(snapshot))
        | RpcResponse::AcquisitionCancel(AcquisitionOutcome::Ok(snapshot))
        | RpcResponse::AcquisitionDownload(AcquisitionOutcome::Ok(snapshot))
        | RpcResponse::AcquisitionRemove(AcquisitionOutcome::Ok(snapshot)) => {
            let mut lines = vec![format!(
                "Declared artifacts: {} (not provider jobs)",
                snapshot.artifacts.len()
            )];
            lines.extend(snapshot.artifacts.iter().map(|artifact| {
                let state = match &artifact.state {
                    ArtifactState::Missing => "missing".into(),
                    ArtifactState::Queued => "queued".into(),
                    ArtifactState::Unpacking => "unpacking".into(),
                    ArtifactState::Ready { .. } => "ready".into(),
                    ArtifactState::Failed { reason } => {
                        format!("failed: {}", output::text(&reason.message))
                    }
                    ArtifactState::Downloading {
                        bytes_done,
                        bytes_total,
                        ..
                    } => format!(
                        "downloading {bytes_done}/{} bytes",
                        bytes_total
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "unknown".into())
                    ),
                };
                format!(
                    "{}\t{}\t{state}",
                    output::text(&artifact.id),
                    output::text(&artifact.title)
                )
            }));
            lines.join("\n")
        }
        RpcResponse::AcquisitionProviders(PluginListOutcome::Ok(list)) => list
            .providers
            .iter()
            .map(|p| format!("{}\t{}", output::text(&p.id), output::text(&p.title)))
            .collect::<Vec<_>>()
            .join("\n"),
        RpcResponse::AcquisitionSearch(SearchOutcome::Ok(results)) => {
            let mut lines = vec![format!("Provider results: {}", results.claims.len())];
            lines.extend(results.claims.iter().map(|claim| {
                format!(
                    "{}\t{}\t{}",
                    output::text(&claim.provider_id),
                    output::text(&claim.id),
                    output::text(&claim.title)
                )
            }));
            lines.join("\n")
        }
        RpcResponse::LaunchSettingsSet(LaunchSettingsWriteOutcome::Ok(_))
        | RpcResponse::LaunchProfileSave(LaunchSettingsWriteOutcome::Ok(_))
        | RpcResponse::LaunchProfileRemove(LaunchSettingsWriteOutcome::Ok(_))
        | RpcResponse::LaunchProfileSelect(LaunchSettingsWriteOutcome::Ok(_)) => {
            "Launch settings updated.".into()
        }
        _ => String::new(),
    };
    if output::failure(&response).is_some() {
        if !human.is_empty() {
            human.push('\n');
        }
        human.push_str(&output::unknown("Operation", &response));
    }
    if human.is_empty() {
        human = "Korrid returned no displayable operation result. Run with --json to inspect the response.".into();
    }
    Output::one(response, human)
}
impl Module for Library {
    fn command(&self) -> Command {
        Command::new("library")
            .about("Manage library folders, games and attention")
            .subcommand_required(true)
            .subcommand(
                Command::new("folders")
                    .subcommand_required(true)
                    .subcommand(Command::new("list"))
                    .subcommand(
                        Command::new("add")
                            .arg(required("path"))
                            .arg(required("system")),
                    )
                    .subcommand(
                        Command::new("remove")
                            .about("Unregister a folder; never delete game files")
                            .arg(required("folder")),
                    )
                    .subcommand(Command::new("rescan")),
            )
            .subcommand(
                Command::new("games")
                    .subcommand_required(true)
                    .subcommand(Command::new("list")),
            )
            .subcommand(
                Command::new("attention")
                    .subcommand_required(true)
                    .subcommand(Command::new("list"))
                    .subcommand(
                        Command::new("assign")
                            .arg(required("item"))
                            .arg(required("system")),
                    )
                    .subcommand(Command::new("ignore").arg(required("item"))),
            )
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        let (area, args) = args.subcommand().expect("required subcommand");
        let (action, args) = args.subcommand().expect("required subcommand");
        let request = match (area, action) {
            ("folders", "list") => {
                RpcRequest::LibraryFoldersSnapshot(LibraryFoldersSnapshotRequest {})
            }
            ("folders", "add") => {
                if context.device.is_some() {
                    return Err(CliError::usage("Selecting a library folder requires local access. Use SSH or local access on that device."));
                }
                let path = std::path::PathBuf::from(value(args, "path"));
                let path = if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir()
                        .map_err(|_| {
                            CliError::usage("Cannot resolve this folder. Use an absolute path.")
                        })?
                        .join(path)
                };
                let receipt = context.client.select_folder(&path)?;
                RpcRequest::LibraryFolderAdd(LibraryFolderAddRequest {
                    receipt,
                    system: value(args, "system"),
                })
            }
            ("folders", "remove") => {
                prompt::confirm(
                    "Unregister this library folder? Game files will not be deleted.",
                    context.yes,
                )?;
                RpcRequest::LibraryFolderRemove(LibraryFolderRemoveRequest {
                    folder: value(args, "folder"),
                })
            }
            ("folders", "rescan") => {
                RpcRequest::LibraryFoldersRescan(LibraryFoldersRescanRequest {})
            }
            ("games", "list") => RpcRequest::LocalGamesList(LocalGamesListRequest {}),
            ("attention", "list") => RpcRequest::AttentionList(AttentionListRequest {}),
            ("attention", "assign") => RpcRequest::AttentionAssign(AttentionAssignRequest {
                item: value(args, "item"),
                system: value(args, "system"),
            }),
            ("attention", "ignore") => {
                prompt::confirm("Ignore this attention item?", context.yes)?;
                RpcRequest::AttentionIgnore(AttentionIgnoreRequest {
                    item: value(args, "item"),
                })
            }
            _ => return Err(CliError::usage("Choose a library action.")),
        };
        let response = context.rpc(&request)?;
        if let RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(games)) = &response {
            let lines = games
                .games
                .iter()
                .map(|g| {
                    format!(
                        "{}\t{}\t{}",
                        output::text(&g.id),
                        output::text(&g.title),
                        output::text(&g.system)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let lines = if games.failures.as_ref().is_some_and(|v| !v.is_empty()) {
                format!("{lines}\n{}", output::unknown("Library", &response))
            } else {
                lines
            };
            return Ok(Output::one(response, lines));
        }
        Ok(finish(response))
    }
}

pub(crate) fn choose(
    label: &str,
    choices: &[(String, String)],
    json: bool,
) -> Result<String, CliError> {
    if choices.len() == 1 {
        return Ok(choices[0].0.clone());
    }
    let listing = choices
        .iter()
        .map(|(id, title)| format!("{} ({})", output::text(title), output::text(id)))
        .collect::<Vec<_>>()
        .join(", ");
    if choices.is_empty() {
        return Err(CliError::usage(format!(
            "No {label} matches. Check the supplied input and available {label} IDs."
        )));
    }
    use std::io::IsTerminal;
    if json
        || !prompt::interactive(
            std::io::stdin().is_terminal(),
            std::io::stdout().is_terminal(),
        )
    {
        return Err(CliError::usage(format!(
            "Several {label} matches: {listing}. Supply an exact ID."
        )));
    }
    let selected = prompt::read(&format!("Choose {label} by exact ID: {listing}"))?;
    choices
        .iter()
        .find(|(id, _)| id == &selected)
        .map(|(id, _)| id.clone())
        .ok_or_else(|| CliError::usage("Choose one of the listed exact IDs."))
}

pub(crate) fn game(
    context: &Context,
    query: &str,
    json: bool,
) -> Result<Result<String, RpcResponse>, CliError> {
    let response = context.rpc(&RpcRequest::LocalGamesList(LocalGamesListRequest {}))?;
    let RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(games)) = &response else {
        return Ok(Err(response));
    };
    if games.failures.as_ref().is_some_and(|v| !v.is_empty()) {
        return Ok(Err(response));
    }
    if let Some(game) = games.games.iter().find(|g| g.id == query) {
        return Ok(Ok(game.id.clone()));
    }
    let choices = games
        .games
        .iter()
        .filter(|g| g.title.to_lowercase() == query.to_lowercase())
        .map(|g| (g.id.clone(), g.title.clone()))
        .collect::<Vec<_>>();
    Ok(Ok(choose("game", &choices, json)?))
}
