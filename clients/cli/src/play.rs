use crate::{
    error::CliError,
    launch_settings,
    library::{self, required, value},
    module::{Context, Module},
    output::{self, Output},
    prompt,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use korrid::{
    game_routes::*,
    launch_settings::{LaunchSettingsSnapshotOutcome, LaunchSettingsSnapshotRequest},
    saves::types::{SaveCheckOutcome, SaveCheckRequest, SaveMark},
    *,
};
pub struct Play;
pub struct Leave;
pub struct Resume;
pub struct End;
impl Module for Play {
    fn command(&self) -> Command {
        Command::new("play")
            .about("Ask korrid to launch a library game; acceptance is not gameplay readiness")
            .arg(required("game"))
            .arg(
                Arg::new("set")
                    .long("set")
                    .action(ArgAction::Append)
                    .value_name("KEY=VALUE"),
            )
            .arg(
                Arg::new("keep")
                    .long("keep")
                    .num_args(0..=1)
                    .require_equals(true)
                    .default_missing_value("game")
                    .requires("set")
                    .value_name("PLACE")
                    .help("Keep --set values only after launch succeeds; default game, or device/system:ID/release:ID/profile:ID"),
            )
            .arg(
                Arg::new("runner")
                    .long("runner")
                    .help("Exact installed runner ID; never a device or streaming route"),
            )
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        let json = args.get_flag("json");
        let assignments = args
            .get_many::<String>("set")
            .map(|v| v.cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for assignment in &assignments {
            launch_settings::split(assignment)?;
        }
        let game_id = match library::game(context, &value(args, "game"), json)? {
            Ok(id) => id,
            Err(r) => return Ok(preflight(r)),
        };
        let response = context.rpc(&RpcRequest::GameRoutes(GameRoutesRequest {
            game_id: game_id.clone(),
        }))?;
        let RpcResponse::GameRoutes(GameRoutesOutcome::Ok(routes)) = &response else {
            return Ok(preflight(response));
        };
        let runner_id = if let Some(id) = args.get_one::<String>("runner") {
            if !routes.routes.iter().any(|r| &r.runner_id == id) {
                return Err(CliError::usage("Runner does not provide a route for this game. Check launch-settings show --game."));
            }
            id.clone()
        } else {
            match &routes.selection {
                GameRouteSelection::Selected(id) => {
                    if !routes.routes.iter().any(|r| &r.runner_id == id) {
                        return Err(CliError::usage("Selected runner has no observed route. Select an exact available --runner."));
                    }
                    id.clone()
                }
                GameRouteSelection::Choose => library::choose(
                    "runner",
                    &routes
                        .routes
                        .iter()
                        .map(|r| (r.runner_id.clone(), r.runner_id.clone()))
                        .collect::<Vec<_>>(),
                    json,
                )?,
            }
        };
        let response = context.rpc(&RpcRequest::LaunchSettingsSnapshot(
            LaunchSettingsSnapshotRequest {
                game_id: Some(game_id.clone()),
            },
        ))?;
        let RpcResponse::LaunchSettingsSnapshot(LaunchSettingsSnapshotOutcome::Ok(snapshot)) =
            &response
        else {
            return Ok(preflight(response));
        };
        let launch = if assignments.is_empty() {
            None
        } else {
            Some(launch_settings::overrides(
                snapshot,
                &runner_id,
                &assignments,
            )?)
        };
        let keep_at = args.get_one::<String>("keep");
        let keep_requests = match (keep_at, launch.as_ref()) {
            (Some(place), Some(launch)) => {
                crate::keep_launch::requests(snapshot, &game_id, launch, place)?
            }
            _ => Vec::new(),
        };
        let mut keep_revisions = snapshot.revisions.clone();
        let check_request = RpcRequest::SavesCheck(SaveCheckRequest {
            game_id: game_id.clone(),
            runner_id: Some(runner_id.clone()),
        });
        let check = match context.rpc(&check_request) {
            Ok(response) => response,
            // Remote preflight is intentionally refused: save administration is
            // local-only. Preserve that failed observation, not a launch lock.
            Err(error) if context.device.is_some() => {
                korrid::cli::failure_response(&check_request, error.failure)
                    .map_err(|_| CliError::usage("Could not encode the failed save check."))?
            }
            Err(error) => return Err(error),
        };
        let mut warnings = vec![];
        match &check {
            RpcResponse::SavesCheck(SaveCheckOutcome::Ok(check)) => {
                if check.status == SaveMark::Split {
                    return Err(CliError::failure(
                        "SavesSplit",
                        "This game has a save split.",
                        "Use korri saves to choose the save snapshot before playing.",
                    ));
                }
                warnings.extend(check.warnings.iter().cloned());
                warnings.extend(check.in_use.iter().map(|w| {
                    format!(
                        "Game or newer saves in use on device {}.",
                        output::text(&w.device)
                    )
                }));
                if check.status == SaveMark::NotChecked {
                    warnings.push(
                        "Saves were not checked; offline play can produce a save split.".into(),
                    );
                }
            }
            _ => {
                warnings.push(output::unknown("Save check", &check));
                if let Some(device) = &context.device {
                    warnings.push(format!(
                        "The save check could not run on device {}. This device did not confirm the save state.",
                        output::text(device)
                    ));
                }
            }
        }
        let request = RpcRequest::SelectedGameLaunch(SelectedGameLaunchRequest {
            game_id,
            runner_id,
            launch,
        });
        let response = match context.rpc(&request) {
            Ok(response) => response,
            Err(error) => korrid::cli::failure_response(&request, error.failure)
                .map_err(|_| CliError::usage("Could not encode the failed launch result."))?,
        };
        let mut kept = Vec::new();
        if matches!(
            &response,
            RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Ok(_))
        ) {
            for mut write in keep_requests {
                write.expected_revision = if matches!(
                    write.place.layer,
                    korrid::launch_settings::LaunchLayer::Device
                        | korrid::launch_settings::LaunchLayer::System(_)
                ) {
                    keep_revisions.device.clone()
                } else {
                    keep_revisions.games.clone()
                };
                let request = RpcRequest::LaunchSettingsSet(write);
                let result = match context.rpc(&request) {
                    Ok(result) => result,
                    Err(error) => {
                        korrid::cli::failure_response(&request, error.failure).map_err(|_| {
                            CliError::usage(
                                "Launch succeeded, but the keep result could not be decoded.",
                            )
                        })?
                    }
                };
                match &result {
                    RpcResponse::LaunchSettingsSet(
                        korrid::launch_settings::LaunchSettingsWriteOutcome::Ok(revisions),
                    ) => keep_revisions = revisions.clone(),
                    _ => {
                        warnings.push(format!(
                            "Launch succeeded, but keeping values stopped: {}",
                            output::unknown("Keep", &result)
                        ));
                        kept.push(result);
                        break;
                    }
                }
                kept.push(result);
            }
        }
        let human = match &response {
            RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Ok(result)) => {
                let mut lines = vec![format!(
                    "Launch accepted{}: {}. Gameplay readiness is not yet observed.",
                    context
                        .device
                        .as_ref()
                        .map(|device| format!(" on device {}", output::text(device)))
                        .unwrap_or_default(),
                    output::text(&result.session.launch_id)
                )];

                lines.extend(
                    result
                        .warnings
                        .iter()
                        .map(|w| format!("Warning: {}", output::text(&w.message))),
                );
                lines.join("\n")
            }
            _ => output::unknown("Launch", &response),
        };
        let human = std::iter::once(human)
            .chain(
                warnings
                    .into_iter()
                    .map(|v| format!("Warning: {}", output::text(&v))),
            )
            .collect::<Vec<_>>()
            .join("\n");
        let human = if !kept.is_empty()
            && kept.iter().all(|result| {
                matches!(
                    result,
                    RpcResponse::LaunchSettingsSet(
                        korrid::launch_settings::LaunchSettingsWriteOutcome::Ok(_)
                    )
                )
            }) {
            format!(
                "{human}\nKept {} one-launch values at {}.",
                kept.len(),
                output::text(keep_at.map(String::as_str).unwrap_or("game"))
            )
        } else {
            human
        };
        let mut responses = vec![check, response];
        responses.extend(kept);
        Ok(Output::many(responses, human))
    }
}
fn preflight(response: RpcResponse) -> Output {
    let human = library::finish(response.clone()).human;
    Output::many(vec![response], human)
}
fn control(context: &Context, action: &str) -> Result<Output, CliError> {
    // Pin live control to its observed ID before consent. Failed/blocked
    // administration below can only retire an identity proven no longer live.
    let response = context.rpc(&RpcRequest::SessionStatus(SessionStatusRequest {}))?;
    if let Some(failure) = output::failure(&response) {
        if action != "end"
            || !matches!(
                failure.code.as_str(),
                "HostLaunchFailed" | "HostRecoveryBlocked"
            )
        {
            return Ok(library::finish(response));
        }
        // There is no live identity in this status outcome. The local-only
        // recovery branch can select only a failed state or an absent journal
        // unit under host authority; it cannot target a live replacement.
        prompt::confirm(
            &format!(
                "Resolve the failed or blocked host session? Observed: {}",
                output::text(&failure.message)
            ),
            context.yes,
        )?;
        let response = context.rpc(&RpcRequest::SessionEnd(SessionEndRequest {
            expected_launch_id: None,
            force: None,
        }))?;
        let human = match &response {
            RpcResponse::SessionEnd(SessionEndOutcome::Ok(result)) if matches!(result.phase, SessionEndPhase::Ended) =>
                "Cleared the failed or blocked host session. See journalctl -u korrid for the exact launch ID.".into(),
            _ => match output::failure(&response) {
                Some(failure) => format!("Host session not cleared: {}: {}", output::text(&failure.code), output::text(&failure.message)),
                None => output::unknown("Host session end", &response),
            },
        };
        return Ok(Output::one(response, human));
    }
    let RpcResponse::SessionStatus(SessionStatusOutcome::Ok(status)) = &response else {
        return Ok(library::finish(response));
    };
    let launch_id = status
        .active
        .as_ref()
        .ok_or_else(|| {
            CliError::failure(
                "NoLiveSession",
                "There is no observed live session.",
                "Run korri status to inspect pending launches.",
            )
        })?
        .launch_id
        .clone();
    if action == "end" {
        prompt::confirm(
            &format!("End live session {launch_id}? Unsaved progress will be lost."),
            context.yes,
        )?;
    }
    let request = match action {
        "leave" => RpcRequest::SessionLeave(SessionLeaveRequest {
            expected_launch_id: Some(launch_id),
        }),
        "resume" => RpcRequest::SessionResume(SessionResumeRequest {
            expected_launch_id: Some(launch_id),
        }),
        _ => RpcRequest::SessionEnd(SessionEndRequest {
            expected_launch_id: Some(launch_id),
            force: None,
        }),
    };
    let response = context.rpc(&request)?;
    let human = match &response {
        RpcResponse::SessionEnd(SessionEndOutcome::Ok(result)) => match result.phase {
            SessionEndPhase::Pending => "End requested; teardown is still pending.".into(),
            SessionEndPhase::Ended => "Live session ended.".into(),
        },
        RpcResponse::SessionLeave(SessionTransitionOutcome::Ok(_)) => {
            "Live session left (frozen).".into()
        }
        RpcResponse::SessionResume(SessionTransitionOutcome::Ok(_)) => {
            "Live session resumed.".into()
        }
        _ => output::unknown("Live session", &response),
    };
    Ok(Output::one(response, human))
}
impl Module for Leave {
    fn command(&self) -> Command {
        Command::new("leave").about("Leave the exact observed live session without ending it")
    }
    fn run(&self, _: &ArgMatches, c: &Context) -> Result<Output, CliError> {
        control(c, "leave")
    }
}
impl Module for Resume {
    fn command(&self) -> Command {
        Command::new("resume").about("Resume the exact observed live session")
    }
    fn run(&self, _: &ArgMatches, c: &Context) -> Result<Output, CliError> {
        control(c, "resume")
    }
}
impl Module for End {
    fn command(&self) -> Command {
        Command::new("end")
            .about("End a live session or resolve an ended failed or blocked host session")
    }
    fn run(&self, _: &ArgMatches, c: &Context) -> Result<Output, CliError> {
        control(c, "end")
    }
}
