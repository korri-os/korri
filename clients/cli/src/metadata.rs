//! SteamGridDB uses the current host credential producer in lib.rs and
//! config/settings.rs. ADR 0032 proposes a future move to plugin secrets; it
//! does not define an implemented replacement API on this tested base.
use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    prompt, settings_input,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use korrid::*;

pub struct Metadata;
impl Module for Metadata {
    fn command(&self) -> Command {
        Command::new("metadata").about("Manage metadata access").subcommand_required(true)
            .subcommand(Command::new("key").about("Show or change the SteamGridDB key; never show its value")
                .subcommand(Command::new("show"))
                .subcommand(Command::new("set").arg(Arg::new("key-stdin").long("key-stdin").action(ArgAction::SetTrue).help("Read the key from one private stdin line; otherwise use hidden terminal input")))
                .subcommand(Command::new("clear").about("Remove the stored key")))
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        if context.device.is_some() {
            return Err(CliError::usage(
                "The metadata key stays local. Use SSH or local access on the selected device.",
            ));
        }
        let (_, args) = args
            .subcommand()
            .ok_or_else(|| CliError::usage("Choose metadata key."))?;
        let request = match args.subcommand() {
            None | Some(("show", _)) => RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
            Some(("set", args)) => {
                RpcRequest::SteamGridDbCredentialSet(SteamGridDbCredentialSetRequest {
                    token: settings_input::private_line(
                        args.get_flag("key-stdin"),
                        "SteamGridDB key",
                    )?,
                })
            }
            Some(("clear", _)) => {
                prompt::confirm("Remove the stored SteamGridDB key?", context.yes)?;
                RpcRequest::SteamGridDbCredentialClear(SteamGridDbCredentialClearRequest {})
            }
            _ => return Err(CliError::usage("Choose metadata key show, set or clear.")),
        };
        let response = context.rpc(&request)?;
        let status = match &response {
            RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings)) => {
                Some(settings.steam_grid_db_credential)
            }
            RpcResponse::SteamGridDbCredentialSet(SensitiveSettingOutcome::Ok(result))
            | RpcResponse::SteamGridDbCredentialClear(SensitiveSettingOutcome::Ok(result)) => {
                Some(result.status)
            }
            _ => None,
        };
        let human = match status {
            Some(config::settings::SecretSettingStatus::Configured) => {
                "SteamGridDB key: configured".into()
            }
            Some(config::settings::SecretSettingStatus::NotConfigured) => {
                "SteamGridDB key: not configured".into()
            }
            None => output::unknown("SteamGridDB key", &response),
        };
        Ok(Output::one(response, human))
    }
}
