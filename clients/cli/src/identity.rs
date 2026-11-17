//! Ordinary operations use the live coordinator's existing request types.
//!
//! Offline reset is NOT implemented. The installed native configuration in
//! services/korrid/nixos-module.nix maps KORRID_PRIVATE_STATE_ROOT and service
//! User/Group and defines korrid, identity preparation, signer and activation
//! units. It does not enumerate all configured identity writers or grant an
//! exclusive maintenance interval. DeviceIdentity::reset (identity.rs) removes
//! and recreates identity files as its caller. Recovery therefore also needs a
//! proven service-owned path/account mapping and ownership-preserving execution.
//! A stopped-unit observation or root's HOME is not either guarantee. Until a
//! native contract supplies them, reset must refuse without state access.
use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    prompt, recovery_terminal, settings_input,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use korrid::*;

pub struct Identity;
impl Module for Identity {
    fn command(&self) -> Command {
        Command::new("identity").about("Manage this device's person key and person name").subcommand_required(true)
            .subcommand(Command::new("show").about("Show this device's key and its person key"))
            .subcommand(Command::new("name").about("Show or edit the person name on this device")
                .arg(Arg::new("name").value_name("NAME").value_parser(person_name_input)
                    .help("At most 64 characters; an explicit empty string is passed unchanged"))
                .after_help("Omit NAME to show the name returned by korrid, without a fallback name. Edits require the owner's available signer and use one settings revision without retry. This does not switch the person key or device owner."))
            .subcommand(Command::new("switch").about("Switch through the live coordinator; choose data disposition and confirm loss of device trust").subcommand_required(true)
                .subcommand(switch_command("words").about("Move this device to the person key that 12 recovery words make (ADR 0073); private stdin is the existing IdentityWordsSwitchRequest JSON")
                    .arg(Arg::new("current-owner-bunker").long("current-owner-bunker").action(ArgAction::SetTrue).help("Ask for the current owner's bunker URI, for a device that keeps no connection to its owner's NIP-46 signer; the URI is used once and never kept"))
                    .after_help("The words go to this device's local signer, which signs the new owner statement and erases the key. The current owner's NIP-46 signer approves the removal of the old statement. Private stdin fields (the existing RPC type): words, currentOwnerBunkerUri (optional), dataDisposition (transfer or delete), trustLossConfirmed (boolean). --yes never supplies these choices or acknowledgements."))
                .subcommand(switch_command("nip46").about("Use a remote signer; private stdin is the existing IdentityNip46SwitchRequest JSON")
                    .after_help("Private stdin fields (the existing RPC type): bunkerUri, dataDisposition (transfer or delete), trustLossConfirmed (boolean). --yes never supplies these choices or acknowledgements.")))
            .subcommand(Command::new("reset").about("Offline device identity recovery (unavailable until exclusive writer authority is proven; does not delete games or saves)"))
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        if context.device.is_some() {
            return Err(CliError::usage(
                "Identity stays local. Use SSH or local access on the selected device.",
            ));
        }
        let request = match args.subcommand() {
            Some(("show", _)) => RpcRequest::IdentityStatus(IdentityStatusRequest {}),
            Some(("name", args)) => return name(args, context),
            Some(("switch", args)) => switch_request(args, context)?,
            // Known native units and state mapping are grounded in
            // services/korrid/nixos-module.nix. It has no exhaustive configured
            // writer contract or exclusive maintenance authority. Checking that
            // units look stopped is racy. Do not open any state or guess HOME.
            Some(("reset", _)) => return Err(CliError::failure("IdentityRecoveryUnavailable", "Device identity reset is unavailable: exclusive authority over all identity users, sockets and configured writers cannot be proven.", "Recovery needs an approved native maintenance contract that prevents reactivation and preserves the installed service's private-state ownership. No state was opened or changed.")),
            _ => return Err(CliError::usage("Choose identity show, name, switch or reset.")),
        };
        let response = context.rpc(&request)?;
        let human = match &response {
            RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(status)) => describe(status),
            RpcResponse::IdentityWordsSwitch(IdentitySwitchOutcome::Ok(completed))
            | RpcResponse::IdentityNip46Switch(IdentitySwitchOutcome::Ok(completed)) => format!(
                "Device owner: {}. Korri will restart to use the new identity.",
                output::text(&completed.owner_public_key)
            ),
            _ => output::unknown("Identity", &response),
        };
        Ok(Output::one(response, human))
    }
}
// The existing person-name producer validates Unicode scalar count, accepts an
// empty string, and does not trim it (stream_play/person_name/mod.rs). Mirror
// only that input boundary here; signing and record resolution stay in korrid.
fn person_name_input(value: &str) -> Result<String, &'static str> {
    if value.chars().count() > 64 {
        Err("Person name must have at most 64 characters")
    } else {
        Ok(value.to_owned())
    }
}

fn name(args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
    // This is the existing portal SettingsUpdate ID, not an identity claim.
    let setting_id = "person-name";
    let response = context.rpc(&RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}))?;
    let response = match (&response, args.get_one::<String>("name")) {
        (RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(snapshot)), Some(value)) => {
            // The producer advertises this handler only when the owner's signer
            // is available. Never bypass a missing handler with a blind write.
            if !snapshot
                .editable_setting_ids
                .iter()
                .any(|id| id == setting_id)
            {
                return Err(CliError::failure(
                    "PersonSignerUnavailable",
                    "This device cannot sign for the owner.",
                    "Make the owner's signer available on this device, then try again.",
                ));
            }
            context.rpc(&RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                expected_revision: snapshot.revision.clone(),
                setting_id: setting_id.into(),
                value: value.clone(),
            }))?
        }
        _ => response,
    };
    let human = match &response {
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(snapshot))
        | RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(snapshot)) => {
            match snapshot.person_name.as_deref() {
                Some(name) => format!("Person name: {}", output::text(name)),
                None => "No person name was returned.".into(),
            }
        }
        _ => output::unknown("Person name", &response),
    };
    Ok(Output::one(response, human))
}

fn switch_command(name: &'static str) -> Command {
    Command::new(name).arg(Arg::new("private-stdin").long("private-stdin").action(ArgAction::SetTrue).help("Read one existing typed request JSON from private stdin, including dataDisposition and trustLossConfirmed; never pass secrets as arguments"))
}
fn disposition() -> Result<IdentityDataDisposition, CliError> {
    match prompt::read("Existing person data: transfer or delete")?.as_str() {
        "transfer" => Ok(IdentityDataDisposition::Transfer),
        "delete" => Ok(IdentityDataDisposition::Delete),
        _ => Err(CliError::usage(
            "Choose transfer or delete for existing person data.",
        )),
    }
}
fn switch_request(args: &ArgMatches, context: &Context) -> Result<RpcRequest, CliError> {
    let (kind, args) = args
        .subcommand()
        .ok_or_else(|| CliError::usage("Choose identity switch words or nip46."))?;
    let request = if args.get_flag("private-stdin") {
        match kind {
            "words" => RpcRequest::IdentityWordsSwitch(settings_input::private_request::<
                IdentityWordsSwitchRequest,
            >()?),
            "nip46" => RpcRequest::IdentityNip46Switch(settings_input::private_request::<
                IdentityNip46SwitchRequest,
            >()?),
            _ => return Err(CliError::usage("Choose words or nip46.")),
        }
    } else {
        let data_disposition = disposition()?;
        // This acknowledgement is not the destructive --yes confirmation.
        let trust_loss_confirmed =
            prompt::read("Switching loses existing device trust. Type confirm to acknowledge")?
                == "confirm";
        if !trust_loss_confirmed {
            return Err(CliError::usage(
                "Device trust loss must be explicitly acknowledged.",
            ));
        }
        match kind {
            "words" => {
                let current_owner_bunker_uri = if args.get_flag("current-owner-bunker") {
                    Some(settings_input::private_line(
                        false,
                        "The current owner's bunker URI",
                    )?)
                } else {
                    None
                };
                RpcRequest::IdentityWordsSwitch(IdentityWordsSwitchRequest {
                    words: recovery_terminal::read_words(
                        "The 12 recovery words of the new person key",
                    )?
                    .to_string(),
                    current_owner_bunker_uri,
                    data_disposition,
                    trust_loss_confirmed,
                })
            }
            "nip46" => RpcRequest::IdentityNip46Switch(IdentityNip46SwitchRequest {
                bunker_uri: settings_input::private_line(false, "Remote signer bunker URI")?,
                data_disposition,
                trust_loss_confirmed,
            }),
            _ => return Err(CliError::usage("Choose words or nip46.")),
        }
    };
    prompt::confirm("Replace this device's owner, apply the selected data disposition and lose existing device trust?", context.yes)?;
    Ok(request)
}
fn describe(status: &IdentityStatus) -> String {
    match &status.identity {
        identity::IdentityState::Owned {
            device_public_key,
            owner_public_key,
            ..
        } => format!(
            "Device key: {}\nPerson key: {}",
            output::text(device_public_key),
            output::text(owner_public_key)
        ),
        identity::IdentityState::Unowned { device_public_key } => format!(
            "Device key: {}\nPerson key: not bound",
            output::text(device_public_key)
        ),
        _ => "Identity: unavailable".into(),
    }
}
