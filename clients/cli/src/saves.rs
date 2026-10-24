use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    prompt, settings_input,
};
use clap::{Arg, ArgMatches, Command};
use korrid::{saves::types::*, RpcRequest, RpcResponse};

pub struct Saves;
impl Module for Saves {
    fn command(&self) -> Command {
        let mut command = Command::new("saves")
            .about("Read and select this person's save snapshots on this device")
            .subcommand_required(true)
            .subcommand(Command::new("status").about("Show save status for each game"))
            .subcommand(
                Command::new("history")
                    .about("Show one exact game's save history")
                    .arg(Arg::new("game").value_name("GAME_ID")),
            );
        for (name, about) in [
            ("restore", "Restore a snapshot, overwriting current saves"),
            (
                "choose",
                "Choose a snapshot to continue from a split; retain the other history",
            ),
        ] {
            command = command.subcommand(
                Command::new(name)
                    .about(about)
                    .arg(Arg::new("game").value_name("GAME_ID"))
                    .arg(Arg::new("snapshot").value_name("SNAPSHOT_ID")),
            );
        }
        command
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        if context.device.is_some() {
            return Err(CliError::usage(
                "Saves stay local. Use SSH or local access on the selected device.",
            ));
        }
        let request = match args.subcommand() {
            Some(("status", _)) => RpcRequest::SavesStatus(SaveStatusRequest {}),
            Some(("history", args)) => RpcRequest::SavesHistory(SaveGameRequest {
                game_id: settings_input::required(args, "game", "Exact game ID")?,
            }),
            Some((action @ ("restore" | "choose"), args)) => {
                let game_id = settings_input::required(args, "game", "Exact game ID")?;
                let snapshot_id = settings_input::required(
                    args,
                    "snapshot",
                    "Exact snapshot ID from save history",
                )?;
                prompt::confirm(
                    if action == "restore" {
                        "Restore this snapshot and overwrite current saves?"
                    } else {
                        "Continue from this snapshot of the split? Other snapshots stay in history."
                    },
                    context.yes,
                )?;
                let pick = SavePickRequest {
                    game_id,
                    snapshot_id,
                };
                if action == "restore" {
                    RpcRequest::SavesRestore(pick)
                } else {
                    RpcRequest::SavesChoose(pick)
                }
            }
            _ => {
                return Err(CliError::usage(
                    "Choose saves status, history, restore or choose.",
                ))
            }
        };
        let response = context.rpc(&request)?;
        let human = match &response {
            RpcResponse::SavesStatus(SaveStatusOutcome::Ok(status)) => {
                if status.games.is_empty() {
                    "No save history.".into()
                } else {
                    status
                        .games
                        .iter()
                        .map(|game| {
                            format!(
                                "{}: {:?}  {}",
                                output::text(&game.game_id),
                                game.status,
                                game.heads
                                    .iter()
                                    .map(|id| output::text(id))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            RpcResponse::SavesHistory(SaveHistoryOutcome::Ok(history))
            | RpcResponse::SavesRestore(SaveHistoryOutcome::Ok(history))
            | RpcResponse::SavesChoose(SaveHistoryOutcome::Ok(history)) => describe(history),
            _ => output::unknown("Saves", &response),
        };
        Ok(Output::one(response, human))
    }
}
fn describe(history: &SaveHistory) -> String {
    if history.records.is_empty() {
        return "No save snapshots.".into();
    }
    history
        .records
        .iter()
        .map(|entry| {
            format!(
                "{}{}  {}  {}{}",
                if history.current.as_deref() == Some(&entry.snapshot_id) {
                    "* "
                } else {
                    "  "
                },
                output::text(&entry.snapshot_id),
                output::text(&entry.record.ended_at),
                output::text(&entry.record.device),
                if entry.record.interrupted {
                    " (interrupted)"
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
