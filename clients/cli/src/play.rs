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
        let check = context.rpc(&RpcRequest::SavesCheck(SaveCheckRequest {
            game_id: game_id.clone(),
            runner_id: Some(runner_id.clone()),
        }))?;
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
            _ => warnings.push(output::unknown("Save check", &check)),
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
        let human = match &response {
            RpcResponse::SelectedGameLaunch(SelectedGameLaunchOutcome::Ok(result)) => {
                let mut lines = vec![format!(
                    "Launch accepted: {}. Gameplay readiness is not yet observed.",
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
        Ok(Output::many(vec![check, response], human))
    }
}
fn preflight(response: RpcResponse) -> Output {
    let human = library::finish(response.clone()).human;
    Output::many(vec![response], human)
}
fn control(context: &Context, action: &str) -> Result<Output, CliError> {
    // Pin the observation before asking for consent. A replacement during the
    // prompt is a stale-identity refusal, never a reason to observe and retarget.
    let response = context.rpc(&RpcRequest::SessionStatus(SessionStatusRequest {}))?;
    if output::failure(&response).is_some() {
        return Ok(library::finish(response));
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
        Command::new("end").about("End the exact observed live session; unsaved progress is lost")
    }
    fn run(&self, _: &ArgMatches, c: &Context) -> Result<Output, CliError> {
        control(c, "end")
    }
}
