//! A join (ADRs 0069 and 0070) from a terminal. `join start` runs on an
//! owned device: it shows the code, then asks for one Accept. `join enter`
//! runs on the new device with the 10 digits. Both follow korrid's join
//! status until the join ends. Codes and progress go to stderr; the final
//! response goes to stdout, as for every command.
use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    prompt,
};
use clap::{Arg, ArgMatches, Command};
use korrid::{
    join::{
        JoinAcceptRequest, JoinEnterRequest, JoinOutcome, JoinStartRequest, JoinStatus,
        JoinStatusRequest, JoiningDevice,
    },
    network::NetworkConnection,
    RpcRequest, RpcResponse,
};
use std::time::Duration;

const POLL: Duration = Duration::from_millis(1000);
/// The proof-only switch. It accepts the first device whose code matches,
/// with no person in the loop. Only a build with the `proof` feature has it;
/// the product build never does.
#[cfg(feature = "proof")]
const PROOF_ACCEPT: &str = "proof-accept-without-asking";

pub struct Join;
impl Module for Join {
    fn command(&self) -> Command {
        let start = Command::new("start").about(
            "On a device that has an owner: show a code, then accept or refuse the device that enters it",
        );
        #[cfg(feature = "proof")]
        let start = start.arg(
            Arg::new(PROOF_ACCEPT)
                .long(PROOF_ACCEPT)
                .action(clap::ArgAction::SetTrue)
                .help("Proof build only: accept the matched device without asking"),
        );
        Command::new("join")
            .about("Join a new device to this person's devices with a 10-digit code")
            .subcommand_required(true)
            .subcommand(start)
            .subcommand(
                Command::new("enter")
                    .about("On a new device without an owner: type the code that the other device shows")
                    .arg(Arg::new("code").value_name("CODE").help(
                        "For scripts only. Omit it and type the code at the prompt: a code on the command line is visible to other users of this device",
                    )),
            )
    }

    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        if context.device.is_some() {
            return Err(CliError::usage(
                "A join stays local. Run it on the device itself, at its terminal or over SSH.",
            ));
        }
        match args.subcommand() {
            Some(("start", args)) => start(context, proof_accept(args)),
            Some(("enter", args)) => {
                let code = match args.get_one::<String>("code") {
                    Some(code) => code.clone(),
                    None => prompt::read("Join code (10 digits)")?,
                };
                enter(context, code)
            }
            _ => Err(CliError::usage("Choose join start or join enter.")),
        }
    }
}

#[cfg(feature = "proof")]
fn proof_accept(args: &ArgMatches) -> bool {
    let accept = args.get_flag(PROOF_ACCEPT);
    if accept {
        eprintln!(
            "WARNING: PROOF BUILD. This join accepts the device whose code matches without asking. Never use it to join a real device."
        );
    }
    accept
}

#[cfg(not(feature = "proof"))]
fn proof_accept(_: &ArgMatches) -> bool {
    false
}

fn start(context: &Context, proof_accept: bool) -> Result<Output, CliError> {
    let mut response = context.rpc(&RpcRequest::JoinStart(JoinStartRequest {}))?;
    let mut shown_code = None;
    let mut decided = false;
    loop {
        match status(&response) {
            Some(JoinStatus::ShowingCode(shown)) => {
                if shown_code.as_ref() != Some(&shown.code) {
                    if shown.wrong_code_tried {
                        eprintln!("A wrong code was tried. That code no longer works.");
                    }
                    eprintln!(
                        "Join code: {}  (enter it on the new device; valid for {})",
                        output::text(&shown.code),
                        remaining(shown.expires_at)
                    );
                    shown_code = Some(shown.code.clone());
                }
            }
            // korrid is still signing and posting the vouch.
            Some(JoinStatus::AwaitingAccept(_)) if decided => {}
            Some(JoinStatus::AwaitingAccept(device)) => {
                let device = describe_device(device);
                eprintln!("The code matched. Joining device: {device}");
                let accept = proof_accept
                    || prompt::confirm(
                        "Accept this device? It becomes one of this person's devices",
                        false,
                    )
                    .is_ok();
                response = context.rpc(&RpcRequest::JoinAccept(JoinAcceptRequest { accept }))?;
                decided = true;
                continue;
            }
            Some(JoinStatus::Starting) => {}
            _ => return Ok(finish(response, Side::Owned)),
        }
        std::thread::sleep(POLL);
        response = context.rpc(&RpcRequest::JoinStatus(JoinStatusRequest {}))?;
    }
}

fn enter(context: &Context, code: String) -> Result<Output, CliError> {
    let mut response = context.rpc(&RpcRequest::JoinEnter(JoinEnterRequest { code }))?;
    let mut waiting_shown = false;
    loop {
        match status(&response) {
            Some(JoinStatus::Matching) => {}
            Some(JoinStatus::WaitingForAccept) => {
                if !waiting_shown {
                    eprintln!("The code matched. Accept this device on the other device.");
                    waiting_shown = true;
                }
            }
            _ => return Ok(finish(response, Side::New)),
        }
        std::thread::sleep(POLL);
        response = context.rpc(&RpcRequest::JoinStatus(JoinStatusRequest {}))?;
    }
}

fn status(response: &RpcResponse) -> Option<&JoinStatus> {
    match response {
        RpcResponse::JoinStart(JoinOutcome::Ok(status))
        | RpcResponse::JoinEnter(JoinOutcome::Ok(status))
        | RpcResponse::JoinStatus(JoinOutcome::Ok(status))
        | RpcResponse::JoinAccept(JoinOutcome::Ok(status)) => Some(status),
        _ => None,
    }
}

enum Side {
    Owned,
    New,
}

fn finish(response: RpcResponse, side: Side) -> Output {
    let human = match (status(&response), side) {
        (Some(JoinStatus::Joined(joined)), Side::Owned) => format!(
            "Joined. Device {} now belongs to person key {}.",
            output::text(&joined.device_public_key),
            output::text(&joined.owner_public_key)
        ),
        (Some(JoinStatus::Joined(joined)), Side::New) => format!(
            "Joined. This device now belongs to person key {}. Korri restarts to use it.",
            output::text(&joined.owner_public_key)
        ),
        (Some(JoinStatus::Refused), Side::Owned) => "The device was refused.".into(),
        (Some(JoinStatus::Refused), Side::New) => "The other device refused this device.".into(),
        (Some(JoinStatus::Idle), _) => "The join ended.".into(),
        (Some(JoinStatus::Failed(failure)), _) => {
            format!("The join failed: {}", output::text(&failure.message))
        }
        _ => output::unknown("Join", &response),
    };
    Output::one(response, human)
}

fn describe_device(device: &JoiningDevice) -> String {
    let connection = match &device.connection {
        Some(NetworkConnection::Wired) => "wired network".into(),
        Some(NetworkConnection::Wifi { ssid, .. }) => format!("Wi-Fi {}", output::text(ssid)),
        Some(NetworkConnection::Offline) => "offline".into(),
        None => "network unknown".into(),
    };
    format!(
        "{} ({}, {})",
        device
            .name
            .as_deref()
            .map(output::text)
            .unwrap_or_else(|| "no name".into()),
        output::text(&device.kind),
        connection
    )
}

fn remaining(expires_at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let seconds = expires_at.saturating_sub(now);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "proof"))]
    #[test]
    fn the_product_build_has_no_proof_accept_flag() {
        let parsed = crate::command().try_get_matches_from([
            "korri",
            "join",
            "start",
            "--proof-accept-without-asking",
        ]);
        assert!(parsed.is_err());
        assert!(crate::command()
            .try_get_matches_from(["korri", "join", "start"])
            .is_ok());
    }

    #[test]
    fn the_joining_device_is_described_by_its_facts() {
        let device = JoiningDevice {
            name: Some("Den\u{7}".into()),
            kind: "linux aarch64".into(),
            connection: Some(NetworkConnection::Wifi {
                ssid: "Home".into(),
                strength_percent: Some(70),
            }),
        };
        assert_eq!(describe_device(&device), "Den� (linux aarch64, Wi-Fi Home)");
        let unnamed = JoiningDevice {
            name: None,
            kind: "linux x86_64".into(),
            connection: None,
        };
        assert_eq!(
            describe_device(&unnamed),
            "no name (linux x86_64, network unknown)"
        );
    }

    #[test]
    fn a_failed_join_ends_with_its_message() {
        let response =
            RpcResponse::JoinEnter(JoinOutcome::Ok(JoinStatus::Failed(korrid::RpcFailure {
                code: "JoinCodeMismatch".into(),
                message: "The code did not match.".into(),
            })));
        assert_eq!(
            output::failure(&response).map(|failure| failure.code),
            Some("JoinCodeMismatch".into())
        );
        assert_eq!(
            finish(response, Side::New).human,
            "The join failed: The code did not match."
        );
    }
}
