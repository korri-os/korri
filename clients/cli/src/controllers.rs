use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
};
use clap::{Arg, ArgMatches, Command};
use korrid::*;

pub struct Controllers;
impl Module for Controllers {
    fn command(&self) -> Command {
        Command::new("controllers").about("Configure Korri controller seats").subcommand_required(true)
            .subcommand(Command::new("seats").about("Show or set the configured seat count, not physical controller assignments")
                .arg(Arg::new("count").value_name("COUNT")))
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        let (_, args) = args
            .subcommand()
            .ok_or_else(|| CliError::usage("Choose controllers seats."))?;
        let count = args.get_one::<String>("count");
        // The settings producer owns both validation and the accepted range.
        if let Some(count) = count {
            config::settings::parse_change(
                config::settings::PLAYER_COUNT_SETTING_ID,
                count.clone(),
            )
            .map_err(|_| CliError::usage("Seat count must be an integer from 1 to 255."))?;
        }
        let response = context.rpc(&RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}))?;
        let response = match (&response, count) {
            (RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(snapshot)), Some(count)) => {
                context.rpc(&RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                    expected_revision: snapshot.revision.clone(),
                    setting_id: config::settings::PLAYER_COUNT_SETTING_ID.into(),
                    value: count.clone(),
                }))?
            }
            _ => response,
        };
        let human = match &response {
            RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings))
            | RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(settings)) => format!(
                "Configured seats: {} (not physical controller assignments)",
                settings.player_count
            ),
            _ => output::unknown("Configured seats", &response),
        };
        Ok(Output::one(response, human))
    }
}
