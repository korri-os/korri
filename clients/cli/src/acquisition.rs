use crate::{
    error::CliError,
    library::{self, required, value},
    module::{Context, Module},
    output::{self, Output},
    prompt,
};
use clap::{Arg, ArgMatches, Command};
use korrid::{acquisition::*, *};
pub struct Get;
pub struct Downloads;
impl Module for Get {
    fn command(&self) -> Command {
        Command::new("get")
            .about("Download a library game, or find and acquire it from a provider")
            .subcommand_negates_reqs(true)
            .arg(Arg::new("game").required(true))
            .arg(Arg::new("provider").long("provider"))
            .arg(
                Arg::new("id")
                    .long("id")
                    .requires("provider")
                    .help("Exact provider claim ID; details are fetched before acquiring"),
            )
            .arg(Arg::new("file-name").long("file-name"))
            .subcommand(
                Command::new("search")
                    .arg(required("query"))
                    .arg(Arg::new("provider").long("provider")),
            )
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        let (search, args) = match args.subcommand() {
            Some(("search", sub)) => (true, sub),
            _ => (false, args),
        };
        let query = value(args, if search { "query" } else { "game" });
        if !search && args.get_one::<String>("provider").is_none() {
            let response = context.rpc(&RpcRequest::LocalGamesList(LocalGamesListRequest {}))?;
            let RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(games)) = &response else {
                return Ok(library::finish(response));
            };
            if games.failures.as_ref().is_some_and(|v| !v.is_empty()) {
                return Ok(library::finish(response));
            }
            let exact = games.games.iter().find(|g| g.id == query);
            let choices = if let Some(g) = exact {
                vec![(g.id.clone(), g.title.clone())]
            } else {
                games
                    .games
                    .iter()
                    .filter(|g| g.title.to_lowercase() == query.to_lowercase())
                    .map(|g| (g.id.clone(), g.title.clone()))
                    .collect()
            };
            if !choices.is_empty() {
                let game_id = library::choose("game", &choices, args.get_flag("json"))?;
                return Ok(library::finish(context.rpc(
                    &RpcRequest::AcquisitionDownload(AcquisitionDownloadRequest { game_id }),
                )?));
            }
        }
        let response = context.rpc(&RpcRequest::AcquisitionProviders(
            AcquisitionPluginsPayload {},
        ))?;
        let RpcResponse::AcquisitionProviders(PluginListOutcome::Ok(providers)) = &response else {
            return Ok(library::finish(response));
        };
        let provider_id = if let Some(id) = args.get_one::<String>("provider") {
            if !providers.providers.iter().any(|p| &p.id == id) {
                return Err(CliError::usage(
                    "Unknown provider. Run get search with an installed provider ID.",
                ));
            }
            id.clone()
        } else {
            library::choose(
                "provider",
                &providers
                    .providers
                    .iter()
                    .map(|p| (p.id.clone(), p.title.clone()))
                    .collect::<Vec<_>>(),
                args.get_flag("json"),
            )?
        };
        let id = if !search {
            args.get_one::<String>("id").cloned()
        } else {
            None
        };
        let id = if let Some(id) = id {
            id
        } else {
            let response = context.rpc(&RpcRequest::AcquisitionSearch(SearchRequest {
                query,
                provider_ids: vec![provider_id.clone()],
            }))?;
            if search {
                return Ok(library::finish(response));
            }
            let RpcResponse::AcquisitionSearch(SearchOutcome::Ok(results)) = &response else {
                return Ok(library::finish(response));
            };
            if results.claims.iter().any(|c| c.provider_id != provider_id) {
                return Err(CliError::usage("Provider returned a claim for a different provider. Check the installed plugin."));
            }
            library::choose(
                "provider claim",
                &results
                    .claims
                    .iter()
                    .map(|c| (c.id.clone(), c.title.clone()))
                    .collect::<Vec<_>>(),
                args.get_flag("json"),
            )?
        };
        let response = context.rpc(&RpcRequest::AcquisitionDetails(DetailsRequest {
            provider_id: provider_id.clone(),
            id: id.clone(),
        }))?;
        let RpcResponse::AcquisitionDetails(DetailsOutcome::Ok(details)) = &response else {
            return Ok(library::finish(response));
        };
        if details.provider_id != provider_id || details.id != id {
            return Err(CliError::usage(
                "Provider details do not match the selected claim. Check the installed plugin.",
            ));
        }
        let response = context.rpc(&RpcRequest::AcquisitionAcquire(AcquireArtifactRequest {
            provider_id,
            id,
            file_name: args.get_one::<String>("file-name").cloned(),
        }))?;
        Ok(job_output(response))
    }
}
fn job_output(response: RpcResponse) -> Output {
    let job = match &response {
        RpcResponse::AcquisitionAcquire(AcquireJobOutcome::Ok(job))
        | RpcResponse::AcquisitionAcquireStatus(AcquireJobOutcome::Ok(job))
        | RpcResponse::AcquisitionAcquireCancel(AcquireJobOutcome::Ok(job)) => Some(job),
        _ => None,
    };
    if let Some(job) = job {
        let human = format!(
            "Download job {}: {}\nInspect: korri downloads status --job {}",
            output::text(&job.job_id),
            output::text(&serde_json::to_string(&job.state).expect("job state serializes")),
            output::text(&job.job_id)
        );
        return Output::one(response, human);
    }
    library::finish(response)
}
impl Module for Downloads {
    fn command(&self) -> Command {
        Command::new("downloads").about("Inspect game artifacts and provider jobs without mixing their IDs").subcommand_required(true)
 .subcommand(Command::new("status").arg(Arg::new("job").long("job").conflicts_with("game")).arg(Arg::new("game").long("game")))
 .subcommand(Command::new("cancel").arg(Arg::new("job").long("job").required_unless_present("artifact").conflicts_with("artifact")).arg(Arg::new("artifact").long("artifact").help("Declared artifact ID, not a provider job ID")))
 .subcommand(Command::new("remove").about("Remove downloaded game content, never saves; provider job records cannot be removed").arg(required("game")))
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        let (action, args) = args.subcommand().expect("required subcommand");
        let request = match action {
            "status" => {
                if let Some(job) = args.get_one::<String>("job") {
                    RpcRequest::AcquisitionAcquireStatus(AcquireStatusRequest {
                        job_id: job.clone(),
                    })
                } else {
                    let game_id = if let Some(query) = args.get_one::<String>("game") {
                        Some(
                            match library::game(context, query, args.get_flag("json"))? {
                                Ok(id) => id,
                                Err(r) => return Ok(library::finish(r)),
                            },
                        )
                    } else {
                        None
                    };
                    RpcRequest::AcquisitionStatus(AcquisitionStatusRequest {
                        game_id,
                        runner_id: None,
                    })
                }
            }
            "cancel" => {
                if let Some(job) = args.get_one::<String>("job") {
                    RpcRequest::AcquisitionAcquireCancel(AcquireCancelRequest {
                        job_id: job.clone(),
                    })
                } else {
                    RpcRequest::AcquisitionCancel(AcquisitionCancelRequest {
                        artifact_id: value(args, "artifact"),
                    })
                }
            }
            "remove" => {
                prompt::confirm(
                    "Remove this game's downloaded content? Saves will not be removed.",
                    context.yes,
                )?;
                let game_id =
                    match library::game(context, &value(args, "game"), args.get_flag("json"))? {
                        Ok(id) => id,
                        Err(r) => return Ok(library::finish(r)),
                    };
                RpcRequest::AcquisitionRemove(AcquisitionRemoveRequest { game_id })
            }
            _ => return Err(CliError::usage("Choose a downloads action.")),
        };
        Ok(job_output(context.rpc(&request)?))
    }
}
