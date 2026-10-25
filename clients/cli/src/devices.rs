use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    target,
};
use clap::{Arg, ArgMatches, Command};
use korrid::*;

pub struct Devices;

impl Module for Devices {
    fn command(&self) -> Command {
        Command::new("devices")
            .about("List and name verified devices of this owner")
            .subcommand_required(true)
            .subcommand(Command::new("list").about("List owner devices, including this device"))
            .subcommand(
                Command::new("rename")
                    .about("Rename this device or one exact owner device")
                    .arg(
                        Arg::new("device-or-name")
                            .required(true)
                            .value_name("DEVICE_OR_NAME"),
                    )
                    .arg(Arg::new("name").value_name("NAME")),
            )
    }

    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        match args.subcommand() {
            Some(("list", _)) => list(context),
            Some(("rename", args)) => rename(context, args),
            _ => Err(CliError::usage("Choose devices list or devices rename.")),
        }
    }
}

fn list(context: &Context) -> Result<Output, CliError> {
    if context.device.is_some() {
        return Err(CliError::usage("Devices list includes this device's local identity. Run it locally on the selected device."));
    }
    let requests = [
        RpcRequest::IdentityStatus(IdentityStatusRequest {}),
        RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
        RpcRequest::PeerList(PeerListRequest {}),
    ];
    let responses = context.collect(&requests)?;
    let key = match &responses[0] {
        RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(status)) => {
            target::key(&status.identity)
        }
        _ => None,
    };
    let name = match &responses[1] {
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings)) => {
            settings.device_name.as_deref()
        }
        _ => None,
    };
    let mut lines = vec![format!(
        "* {} {} (current)",
        name.map(output::text)
            .unwrap_or_else(|| "unknown name".into()),
        key.unwrap_or("unknown key")
    )];
    for response in &responses {
        match response {
            RpcResponse::PeerList(PeerListOutcome::Ok(list)) => {
                for peer in &list.peers {
                    lines.push(format!(
                        "  {} {} ({:?}; reachability unknown)",
                        output::text(&peer.label),
                        peer.device_public_key,
                        peer.state
                    ));
                }
            }
            _ if output::failure(response).is_some() => {
                lines.push(output::unknown("Device facts", response))
            }
            _ => {}
        }
    }
    Ok(Output::many(responses, lines.join("\n")))
}

fn rename(context: &Context, args: &ArgMatches) -> Result<Output, CliError> {
    let first = args
        .get_one::<String>("device-or-name")
        .expect("required argument");
    let (device, name) = match args.get_one::<String>("name") {
        Some(name) => {
            if context.device_flag {
                return Err(CliError::usage(
                    "Use either --device or the rename device argument, not both.",
                ));
            }
            (
                target::resolve(
                    &target::read(&context.client)?,
                    first,
                    args.get_flag("json"),
                )?,
                name,
            )
        }
        None => (context.device.clone(), first),
    };
    let request = RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {});
    let snapshot = match context.client.request_on(device.as_deref(), &request)? {
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(snapshot)) => snapshot,
        response => {
            return Ok(Output::one(
                response.clone(),
                output::unknown("Device name", &response),
            ))
        }
    };
    let response = context.client.request_on(
        device.as_deref(),
        &RpcRequest::SettingsUpdate(SettingsUpdateRequest {
            expected_revision: snapshot.revision,
            setting_id: korrid::config::settings::DEVICE_NAME_SETTING_ID.into(),
            value: name.clone(),
        }),
    )?;
    let human = match &response {
        RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(snapshot)) => format!(
            "Device name: {}",
            snapshot
                .device_name
                .as_deref()
                .map(output::text)
                .unwrap_or_else(|| "unknown".into())
        ),
        _ => output::unknown("Device name", &response),
    };
    Ok(Output::one(response, human))
}
