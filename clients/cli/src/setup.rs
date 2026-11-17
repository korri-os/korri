//! Setup uses korrid's existing surface producers, not a separate setup record.
//! `clients/portal/src/surface/network-onboarding.ts` owns status/restart/finish;
//! `settings-model.ts` owns seat count and device name. Physical controller
//! testing remains shell-only because no semantic-input CLI RPC exists.
//! The optional offer in `starter-pack-onboarding.ts` has the exact generic
//! equivalent `korri plugins on @korri:starter-pack`, then
//! `korri library folders rescan`. No future ADR 0033 pack schema is assumed.
use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    prompt, recovery_terminal, settings_input, wifi,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use korrid::{network::*, onboarding::*, *};

pub struct Setup;
impl Module for Setup {
    fn command(&self) -> Command {
        Command::new("setup").about("Start or rerun setup using the shell's network, settings and completion calls; keep the current identity unless explicitly switched")
            .after_help("Physical controller testing remains in the shell. The optional existing starter-pack offer is available as: korri plugins on @korri:starter-pack, then korri library folders rescan. A device without an owner stays without one during setup, so it can join. Finishing setup on a device that did not join shows 12 recovery words and their QR code; setup ends when you type back the words it asks for.")
            .args_conflicts_with_subcommands(true)
            .arg(Arg::new("seats").long("seats").value_name("COUNT").help("Configured seat count, not physical controller assignment"))
            .arg(Arg::new("name").long("name").value_name("DEVICE_NAME"))
            .arg(Arg::new("network").long("network").value_name("NETWORK_ID_OR_SSID"))
            .arg(Arg::new("hidden").long("hidden").action(ArgAction::SetTrue).requires("network"))
            .arg(Arg::new("password-stdin").long("password-stdin").action(ArgAction::SetTrue).requires("network"))
            .subcommand(Command::new("status").about("Show setup, seats, name and network without changing them"))
            .subcommand(Command::new("restart").about("Confirm and rerun setup; retain settings, identity, games and saves"))
            .subcommand(Command::new("finish").about("Finish setup; an owned device keeps its owner, a device without one gets its automatic identity after you confirm its recovery words"))
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        if context.device.is_some() {
            return Err(CliError::usage(
                "Setup stays local. Use SSH or local access on the selected device.",
            ));
        }
        match args.subcommand_name() {
            Some("restart") => {
                prompt::confirm(
                    "Run setup again? Current settings, identity, games and saves stay.",
                    context.yes,
                )?;
                return single(
                    context.rpc(&RpcRequest::OnboardingRestart(OnboardingRestartRequest {}))?,
                );
            }
            Some("finish") => return finish(context),
            _ => {}
        }
        if let Some(count) = args.get_one::<String>("seats") {
            config::settings::parse_change(
                config::settings::PLAYER_COUNT_SETTING_ID,
                count.clone(),
            )
            .map_err(|_| CliError::usage("Seat count must be an integer from 1 to 255."))?;
        }
        // Read private input before any effect, not after a partially completed setup.
        let password = if args.get_flag("password-stdin") {
            Some(settings_input::private_line(true, "Wi-Fi password")?)
        } else {
            None
        };
        let status = context.rpc(&RpcRequest::OnboardingStatus(OnboardingStatusRequest {}))?;
        let needed = match &status {
            RpcResponse::OnboardingStatus(OnboardingStatusOutcome::Ok(status)) => status.needed,
            _ => return single(status),
        };
        let mut responses = vec![status];
        if !needed && args.subcommand_name().is_none() {
            prompt::confirm(
                "Run setup again? Current settings, identity, games and saves stay.",
                context.yes,
            )?;
            responses
                .push(context.rpc(&RpcRequest::OnboardingRestart(OnboardingRestartRequest {}))?);
            if responses
                .last()
                .is_some_and(|response| output::failure(response).is_some())
            {
                return aggregate(responses);
            }
        }
        let settings = context.rpc(&RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}))?;
        let mut revision = match &settings {
            RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings)) => {
                settings.revision.clone()
            }
            _ => {
                responses.push(settings);
                return aggregate(responses);
            }
        };
        responses.push(settings);
        for (flag, setting_id) in [
            ("seats", config::settings::PLAYER_COUNT_SETTING_ID),
            ("name", config::settings::DEVICE_NAME_SETTING_ID),
        ] {
            if let Some(value) = args.get_one::<String>(flag) {
                let response = context.rpc(&RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                    expected_revision: revision.clone(),
                    setting_id: setting_id.into(),
                    value: value.clone(),
                }))?;
                if let RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(settings)) = &response
                {
                    revision = settings.revision.clone();
                }
                let failed = output::failure(&response).is_some();
                responses.push(response);
                if failed {
                    return aggregate(responses);
                }
            }
        }
        let request = match args.get_one::<String>("network") {
            Some(target) if args.get_flag("hidden") => {
                RpcRequest::WifiConnectHidden(WifiConnectHiddenRequest {
                    ssid: target.clone(),
                    password,
                })
            }
            Some(target) => RpcRequest::WifiConnect(WifiConnectRequest {
                network_id: target.clone(),
                password,
            }),
            None => RpcRequest::NetworkStatus(NetworkStatusRequest {}),
        };
        responses.push(context.rpc(&request)?);
        aggregate(responses)
    }
}
/// How many times the person may type the asked words before setup stops.
const CONFIRMATION_TRIES: usize = 3;

/// End setup. A device without an owner first shows new recovery words; the
/// person types back the words it asks for, and then the device gets its
/// automatic identity (ADRs 0022 and 0082).
fn finish(context: &Context) -> Result<Output, CliError> {
    let response = context.rpc(&RpcRequest::OnboardingFinish(OnboardingFinishRequest {}))?;
    if !matches!(
        &response,
        RpcResponse::OnboardingFinish(OnboardingStatusOutcome::Err(failure))
            if failure.code == "RecoveryWordsNeeded"
    ) {
        return single(response);
    }
    recovery_terminal::require_terminal()?;
    let shown = match context.rpc(&RpcRequest::OnboardingRecoveryWords(
        OnboardingRecoveryWordsRequest {},
    ))? {
        RpcResponse::OnboardingRecoveryWords(RecoveryWordsOutcome::Ok(shown)) => shown,
        other => return single(other),
    };
    let mut response = None;
    for _ in 0..CONFIRMATION_TRIES {
        recovery_terminal::show(&shown.words)?;
        let answers = recovery_terminal::ask_confirmation(&shown.confirm_positions)?;
        let reply = context.rpc(&RpcRequest::OnboardingConfirmWords(
            OnboardingConfirmWordsRequest {
                words: answers.to_vec(),
            },
        ))?;
        let mismatch = match &reply {
            RpcResponse::OnboardingConfirmWords(OnboardingStatusOutcome::Err(failure))
                if failure.code == "RecoveryWordsMismatch" =>
            {
                Some(failure.message.clone())
            }
            _ => None,
        };
        response = Some(reply);
        match mismatch {
            Some(message) => eprintln!("{} The words show again.", output::text(&message)),
            None => break,
        }
    }
    single(response.expect("at least one confirmation try"))
}

fn single(response: RpcResponse) -> Result<Output, CliError> {
    let human = describe(&response);
    Ok(Output::one(response, human))
}
fn aggregate(responses: Vec<RpcResponse>) -> Result<Output, CliError> {
    let mut lines: Vec<_> = responses.iter().map(describe).collect();
    lines.push("Setup steps: configured seats (korri controllers seats), Wi-Fi (korri wifi scan/connect), device name (korri devices rename), then keep this identity or explicitly use korri identity switch. Finish with korri setup finish. Physical controller testing remains in the shell; no assignment RPC exists.\nOptional existing starter pack: korri plugins on @korri:starter-pack, then korri library folders rescan.".into());
    Ok(Output::many(responses, lines.join("\n")))
}
fn describe(response: &RpcResponse) -> String {
    match response {
        RpcResponse::OnboardingConfirmWords(OnboardingStatusOutcome::Ok(_)) => {
            "Setup: finished. This device has its own person key now. Your paper holds the only copy of it.".into()
        }
        RpcResponse::OnboardingStatus(OnboardingStatusOutcome::Ok(status))
        | RpcResponse::OnboardingRestart(OnboardingStatusOutcome::Ok(status))
        | RpcResponse::OnboardingFinish(OnboardingStatusOutcome::Ok(status)) => format!(
            "Setup: {}",
            if status.needed {
                "not finished"
            } else {
                "finished"
            }
        ),
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings))
        | RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Ok(settings)) => format!(
            "Configured seats: {}\nDevice name: {}",
            settings.player_count,
            settings
                .device_name
                .as_deref()
                .map(output::text)
                .unwrap_or_else(|| "not set".into())
        ),
        RpcResponse::NetworkStatus(NetworkStatusOutcome::Ok(network))
        | RpcResponse::WifiConnect(NetworkStatusOutcome::Ok(network))
        | RpcResponse::WifiConnectHidden(NetworkStatusOutcome::Ok(network)) => {
            wifi::describe(network)
        }
        _ => output::unknown("Setup", response),
    }
}
