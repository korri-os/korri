//! Account sign-ins from a terminal (korri#603, ADR 0083, ADR 0089).
//!
//! A device with no shell has no screen for the account sign-in card. This
//! module reads the same `SurfaceModel.accountSignIns` the shell reads and
//! runs the same `RunAction` requests, through `app.surface.model` and
//! `app.surface.request`. It adds no call and no state to korrid. A QR code
//! is drawn from the modules korrid already made; the terminal never sees the
//! address inside it.
use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    settings_input,
};
use clap::{Arg, ArgMatches, Command};
use korri_surface_treaty::{
    SurfaceAccountSignIn, SurfaceAccountState, SurfaceAction, SurfaceModel, SurfaceQrCode,
    SurfaceRequest,
};
use korrid::{
    surface::{
        SurfaceAcceptedOutcome, SurfaceDeclareRequest, SurfaceModelOutcome, SurfaceModelRequest,
        SurfaceRequestRequest,
    },
    RpcRequest, RpcResponse,
};
use std::{
    io::IsTerminal,
    time::{Duration, Instant},
};

/// The surface id this module reads the model under. It declares no
/// preferences and no panes.
const SURFACE_ID: &str = "korri-cli";
/// A sign-in waits this long for the person and the service.
const SIGN_IN_WAIT: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_millis(500);
/// The light border the QR standard asks for, in modules.
const QUIET_ZONE: usize = 4;

pub struct Accounts;
impl Module for Accounts {
    fn command(&self) -> Command {
        Command::new("accounts")
            .about("Show this person's account sign-ins on this device, and sign in or out")
            .after_help(
                "A sign-in shows the service's QR code in this terminal. Approve it on your phone.",
            )
            .subcommand(Command::new("list").about("Show each account sign-in and its state"))
            .subcommand(
                Command::new("sign-in")
                    .about("Sign in to one account with its QR code")
                    .arg(Arg::new("account").value_name("ACCOUNT_ID")),
            )
            .subcommand(
                Command::new("sign-out")
                    .about("Sign out of one account on this device")
                    .arg(Arg::new("account").value_name("ACCOUNT_ID")),
            )
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        if context.device.is_some() {
            return Err(CliError::usage(
                "Account sign-ins stay local. Use SSH or local access on the selected device.",
            ));
        }
        declare(context)?;
        match args.subcommand() {
            None | Some(("list", _)) => {
                let (response, model) = model(context)?;
                let human = if model.account_sign_ins.is_empty() {
                    "No account sign-ins on this device.".into()
                } else {
                    model
                        .account_sign_ins
                        .iter()
                        .map(describe)
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Ok(Output::one(response, human))
            }
            Some(("sign-in", args)) => sign_in(context, &account_id(args)?),
            Some(("sign-out", args)) => sign_out(context, &account_id(args)?),
            _ => Err(CliError::usage(
                "Choose accounts list, sign-in or sign-out.",
            )),
        }
    }
}

fn account_id(args: &ArgMatches) -> Result<String, CliError> {
    settings_input::required(
        args,
        "account",
        "Exact account ID from `korri accounts list`",
    )
}

fn declare(context: &Context) -> Result<(), CliError> {
    let response = context.rpc(&RpcRequest::SurfaceDeclare(SurfaceDeclareRequest {
        surface_id: SURFACE_ID.into(),
        preferences: Default::default(),
        panes: None,
    }))?;
    match response {
        RpcResponse::SurfaceDeclare(SurfaceAcceptedOutcome::Ok(_)) => Ok(()),
        RpcResponse::SurfaceDeclare(SurfaceAcceptedOutcome::Err(failure)) => {
            Err(CliError::rpc(failure))
        }
        other => Err(unexpected(&other)),
    }
}

fn model(context: &Context) -> Result<(RpcResponse, Box<SurfaceModel>), CliError> {
    let response = context.rpc(&RpcRequest::SurfaceModel(SurfaceModelRequest {
        surface_id: SURFACE_ID.into(),
    }))?;
    match &response {
        RpcResponse::SurfaceModel(SurfaceModelOutcome::Ok(model)) => {
            let model = model.clone();
            Ok((response, model))
        }
        RpcResponse::SurfaceModel(SurfaceModelOutcome::Err(failure)) => {
            Err(CliError::rpc(failure.clone()))
        }
        other => Err(unexpected(other)),
    }
}

fn unexpected(response: &RpcResponse) -> CliError {
    CliError::failure(
        "AccountsUnavailable",
        output::unknown("Accounts", response),
        "Check korri status and try again.",
    )
}

fn entry(context: &Context, id: &str) -> Result<SurfaceAccountSignIn, CliError> {
    let (_, model) = model(context)?;
    let ids: Vec<String> = model
        .account_sign_ins
        .iter()
        .map(|account| account.id.clone())
        .collect();
    model
        .account_sign_ins
        .iter()
        .find(|account| account.id == id)
        .cloned()
        .ok_or_else(|| {
            CliError::usage(if ids.is_empty() {
                "This device has no account sign-ins.".to_owned()
            } else {
                format!(
                    "No account sign-in {}. Available: {}.",
                    output::text(id),
                    ids.iter()
                        .map(|id| output::text(id))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
        })
}

fn actions(state: &SurfaceAccountState) -> &[SurfaceAction] {
    match state {
        SurfaceAccountState::SignedOut { actions, .. }
        | SurfaceAccountState::Code { actions, .. }
        | SurfaceAccountState::Working { actions, .. }
        | SurfaceAccountState::SignedIn { actions, .. }
        | SurfaceAccountState::Problem { actions, .. } => actions,
    }
}

fn describe(account: &SurfaceAccountSignIn) -> String {
    let state = match &account.state {
        SurfaceAccountState::SignedOut { kicker, .. } => match kicker {
            Some(kicker) => format!("signed out ({})", output::text(kicker)),
            None => "signed out".into(),
        },
        SurfaceAccountState::Code { .. } => "waiting for approval of its QR code".into(),
        SurfaceAccountState::Working { kicker, .. } => output::text(kicker),
        SurfaceAccountState::SignedIn { account_name, .. } => {
            format!("signed in as {}", output::text(account_name))
        }
        SurfaceAccountState::Problem { kicker, reason, .. } => {
            format!("{}: {}", output::text(kicker), output::text(reason))
        }
    };
    format!(
        "{}\t{}\t{state}",
        output::text(&account.id),
        output::text(&account.title)
    )
}

/// Run the state's one enabled action: a surface infers nothing from a label,
/// and each of these states holds one action (the treaty's state list).
fn run_only_action(
    context: &Context,
    account: &SurfaceAccountSignIn,
) -> Result<RpcResponse, CliError> {
    let enabled: Vec<&SurfaceAction> = actions(&account.state)
        .iter()
        .filter(|action| action.enabled)
        .collect();
    let [action] = enabled.as_slice() else {
        return Err(CliError::failure(
            "AccountActionUnavailable",
            format!(
                "{} offers {} actions in this state.",
                output::text(&account.title),
                enabled.len()
            ),
            "Run korri accounts list and try again.",
        ));
    };
    let response = context.rpc(&RpcRequest::SurfaceRequest(SurfaceRequestRequest {
        surface_id: SURFACE_ID.into(),
        request: SurfaceRequest::RunAction {
            action_id: action.id.clone(),
        },
    }))?;
    match &response {
        RpcResponse::SurfaceRequest(SurfaceAcceptedOutcome::Ok(_)) => Ok(response),
        RpcResponse::SurfaceRequest(SurfaceAcceptedOutcome::Err(failure)) => {
            Err(CliError::rpc(failure.clone()))
        }
        other => Err(unexpected(other)),
    }
}

fn sign_out(context: &Context, id: &str) -> Result<Output, CliError> {
    let account = entry(context, id)?;
    if !matches!(account.state, SurfaceAccountState::SignedIn { .. }) {
        return Err(CliError::usage(format!(
            "{} is not signed in.",
            output::text(&account.title)
        )));
    }
    crate::prompt::confirm(
        &format!(
            "Sign out of {} on this device?",
            output::text(&account.title)
        ),
        context.yes,
    )?;
    let response = run_only_action(context, &account)?;
    Ok(Output::one(
        response,
        format!("Signing out of {}.", output::text(&account.title)),
    ))
}

fn sign_in(context: &Context, id: &str) -> Result<Output, CliError> {
    let account = entry(context, id)?;
    if let SurfaceAccountState::SignedIn { account_name, .. } = &account.state {
        return Ok(Output::text(format!(
            "{} is already signed in as {}.",
            output::text(&account.title),
            output::text(account_name)
        )));
    }
    if !std::io::stdout().is_terminal() {
        return Err(CliError::usage(
            "A sign-in shows a QR code. Run it in a terminal.",
        ));
    }
    // A code that is already on screen elsewhere is shown here too.
    if matches!(
        account.state,
        SurfaceAccountState::SignedOut { .. } | SurfaceAccountState::Problem { .. }
    ) {
        run_only_action(context, &account)?;
    }
    let deadline = Instant::now() + SIGN_IN_WAIT;
    let mut shown = String::new();
    let mut said = String::new();
    let mut started = false;
    loop {
        let account = entry(context, id)?;
        match &account.state {
            SurfaceAccountState::SignedIn { account_name, .. } => {
                return Ok(Output::text(format!(
                    "Signed in to {} as {}.",
                    output::text(&account.title),
                    output::text(account_name)
                )));
            }
            SurfaceAccountState::Code {
                qr, kicker, detail, ..
            } => {
                started = true;
                if qr.modules != shown {
                    shown.clone_from(&qr.modules);
                    println!("\n{}", output::text(kicker));
                    if let Some(detail) = detail {
                        println!("{}", output::text(detail));
                    }
                    print!("{}", draw(qr)?);
                }
            }
            SurfaceAccountState::Working { kicker, .. } => {
                started = true;
                if *kicker != said {
                    said.clone_from(kicker);
                    println!("{}", output::text(kicker));
                }
            }
            SurfaceAccountState::Problem { kicker, reason, .. } if started => {
                return Err(CliError::failure(
                    "AccountSignInFailed",
                    format!("{}: {}", output::text(kicker), output::text(reason)),
                    "Run the sign-in again.",
                ));
            }
            SurfaceAccountState::SignedOut { kicker, .. } if started => {
                return Err(CliError::failure(
                    "AccountSignInEnded",
                    kicker
                        .as_deref()
                        .map(output::text)
                        .unwrap_or_else(|| "The sign-in ended without an account.".into()),
                    "Run the sign-in again.",
                ));
            }
            // The request is accepted; the model has not changed yet.
            SurfaceAccountState::SignedOut { .. } | SurfaceAccountState::Problem { .. } => {}
        }
        if Instant::now() >= deadline {
            return Err(CliError::failure(
                "AccountSignInTimedOut",
                "No sign-in within 10 minutes.",
                "Run the sign-in again.",
            ));
        }
        std::thread::sleep(POLL);
    }
}

/// Draw korrid's QR modules with half blocks, two module rows to a line.
/// Light modules are drawn and dark modules stay blank, as the recovery QR
/// code is drawn (`recovery_qr.rs`), so the code reads right on a terminal
/// with light text on a dark background.
fn draw(qr: &SurfaceQrCode) -> Result<String, CliError> {
    let modules = qr.size as usize;
    let cells: Vec<bool> = qr.modules.bytes().map(|cell| cell == b'1').collect();
    if modules == 0 || cells.len() != modules * modules {
        return Err(CliError::failure(
            "AccountQrUnavailable",
            "Korri sent a QR code this terminal cannot draw.",
            "Check korri status and try again.",
        ));
    }
    let size = modules + 2 * QUIET_ZONE;
    let light = |x: usize, y: usize| {
        let inside = |place: usize| (QUIET_ZONE..QUIET_ZONE + modules).contains(&place);
        !(inside(x) && inside(y)) || !cells[(y - QUIET_ZONE) * modules + (x - QUIET_ZONE)]
    };
    let mut drawn = String::with_capacity(size.div_ceil(2) * (size * 3 + 1));
    for y in (0..size).step_by(2) {
        for x in 0..size {
            let top = light(x, y);
            let bottom = y + 1 < size && light(x, y + 1);
            drawn.push(match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        drawn.push('\n');
    }
    Ok(drawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(state: SurfaceAccountState) -> SurfaceAccountSignIn {
        SurfaceAccountSignIn {
            id: "@korri:steam".into(),
            title: "Steam".into(),
            logo: None,
            offer: None,
            state,
        }
    }

    #[test]
    fn a_list_line_names_the_id_the_title_and_the_state() {
        let signed_in = account(SurfaceAccountState::SignedIn {
            account_name: "person".into(),
            actions: vec![SurfaceAction::new("out", "Sign out", true)],
        });
        assert_eq!(
            describe(&signed_in),
            "@korri:steam\tSteam\tsigned in as person"
        );
        let signed_out = account(SurfaceAccountState::SignedOut {
            kicker: None,
            actions: vec![SurfaceAction::new("in", "Sign in", true)],
        });
        assert_eq!(describe(&signed_out), "@korri:steam\tSteam\tsigned out");
        assert_eq!(actions(&signed_out.state)[0].id, "in");
    }

    #[test]
    fn the_qr_code_is_drawn_from_korrids_modules_with_a_light_border() {
        // 2 x 2: dark, light / light, dark.
        let drawn = draw(&SurfaceQrCode {
            size: 2,
            modules: "1001".into(),
        })
        .unwrap();
        let lines: Vec<&str> = drawn.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(lines.iter().all(|line| line.chars().count() == 10));
        // Rows 4 and 5 hold the code: (4,4) and (5,5) are dark.
        assert_eq!(lines[0], "██████████");
        assert_eq!(lines[2], "████▄▀████");
        for bad in ["100", "10011"] {
            assert!(draw(&SurfaceQrCode {
                size: 2,
                modules: bad.into()
            })
            .is_err());
        }
    }
}
