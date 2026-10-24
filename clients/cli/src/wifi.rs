use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    settings_input,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use korrid::{network::*, RpcRequest, RpcResponse};

pub struct Wifi;
impl Module for Wifi {
    fn command(&self) -> Command {
        Command::new("wifi").about("Manage this device's Wi-Fi through korrid").subcommand_required(true)
            .subcommand(Command::new("status").about("Show the current network"))
            .subcommand(Command::new("scan").about("Scan for networks and show their exact IDs"))
            .subcommand(Command::new("connect").about("Join an exact network ID, or a hidden SSID with --hidden; omit password to use a saved or open network")
                .arg(Arg::new("network").value_name("NETWORK_ID_OR_SSID"))
                .arg(Arg::new("hidden").long("hidden").action(ArgAction::SetTrue))
                .arg(Arg::new("password-stdin").long("password-stdin").action(ArgAction::SetTrue).conflicts_with("ask-password").help("Read the password from one private stdin line"))
                .arg(Arg::new("ask-password").long("ask-password").action(ArgAction::SetTrue).help("Read a password without terminal echo")))
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        if context.device.is_some() {
            return Err(CliError::usage(
                "Wi-Fi stays local. Use SSH or local access on the selected device.",
            ));
        }
        let request = match args.subcommand() {
            Some(("status", _)) => RpcRequest::NetworkStatus(NetworkStatusRequest {}),
            Some(("scan", _)) => RpcRequest::WifiScan(WifiScanRequest {}),
            Some(("connect", args)) => {
                let target =
                    settings_input::required(args, "network", "Exact network ID (or hidden SSID)")?;
                let password = if args.get_flag("password-stdin") || args.get_flag("ask-password") {
                    Some(settings_input::private_line(
                        args.get_flag("password-stdin"),
                        "Wi-Fi password",
                    )?)
                } else {
                    None
                };
                if args.get_flag("hidden") {
                    RpcRequest::WifiConnectHidden(WifiConnectHiddenRequest {
                        ssid: target,
                        password,
                    })
                } else {
                    RpcRequest::WifiConnect(WifiConnectRequest {
                        network_id: target,
                        password,
                    })
                }
            }
            _ => return Err(CliError::usage("Choose wifi status, scan or connect.")),
        };
        Ok(result(context.rpc(&request)?))
    }
}

fn result(response: RpcResponse) -> Output {
    let status = match &response {
        RpcResponse::NetworkStatus(NetworkStatusOutcome::Ok(status))
        | RpcResponse::WifiScan(NetworkStatusOutcome::Ok(status))
        | RpcResponse::WifiConnect(NetworkStatusOutcome::Ok(status))
        | RpcResponse::WifiConnectHidden(NetworkStatusOutcome::Ok(status)) => Some(status),
        _ => None,
    };
    let human = status
        .map(describe)
        .unwrap_or_else(|| output::unknown("Network", &response));
    Output::one(response, human)
}

pub(crate) fn describe(status: &NetworkSnapshot) -> String {
    let mut lines = vec![match &status.connection {
        NetworkConnection::Offline => "Network: offline".into(),
        NetworkConnection::Wired => "Network: wired".into(),
        NetworkConnection::Wifi { ssid, .. } => format!("Network: {}", output::text(ssid)),
    }];
    match &status.wifi {
        korrid::network::Wifi::NoAdapter => lines.push("Wi-Fi: no adapter".into()),
        korrid::network::Wifi::Available {
            scanning,
            networks,
            attempt,
        } => {
            if *scanning {
                lines.push("Wi-Fi scan in progress; run wifi scan again for results.".into());
            }
            for network in networks {
                lines.push(format!(
                    "{}  {}  {}%  {}{}",
                    output::text(&network.id),
                    output::text(&network.ssid),
                    network.strength_percent,
                    if network.secured { "secured" } else { "open" },
                    if network.known { " (saved)" } else { "" }
                ));
            }
            match attempt {
                WifiAttempt::Idle => {}
                WifiAttempt::Connecting { ssid } => lines.push(format!(
                    "Connecting: {}. Run korri wifi status to check.",
                    output::text(ssid)
                )),
                WifiAttempt::Failed {
                    ssid,
                    message,
                    password_rejected,
                } => lines.push(format!(
                    "Could not join {}: {}\nNext: {}",
                    output::text(ssid),
                    output::text(message),
                    if *password_rejected {
                        "Check the Wi-Fi password, then try korri wifi connect again."
                    } else {
                        "Run korri wifi scan, then try korri wifi connect again."
                    }
                )),
            }
        }
    }
    lines.join("\n")
}
