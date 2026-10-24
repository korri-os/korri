use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    prompt, settings_input,
};
use clap::{Arg, ArgMatches, Command};
use korrid::{plugin_admin::*, *};
use std::io::{self, IsTerminal};

pub struct Plugins;
impl Module for Plugins {
    fn command(&self) -> Command {
        let mut command = Command::new("plugins").about("Review and manage plugins without bypassing host policy").subcommand_required(true)
            .subcommand(Command::new("list").about("List installed plugins, or the catalog at an exact repository URL").arg(Arg::new("url").value_name("SOURCE_URL")))
            .subcommand(source_command("review").about("Inspect publisher policy, native units, ports and dependencies before approval"))
            .subcommand(source_command("install").about("Install disabled using the exact reviewed approval digest; --yes never grants permissions")
                .arg(Arg::new("approval").long("approval").value_name("REVIEWED_DIGEST").help("The exact approval field from plugins review")))
            .subcommand(Command::new("on").about("Enable an already-approved plugin; the host rechecks its approval").arg(Arg::new("id").value_name("PLUGIN_ID")))
            .subcommand(Command::new("off").about("Disable an installed plugin").arg(Arg::new("id").value_name("PLUGIN_ID")));
        let repositories = Command::new("repositories")
            .about("Manage catalog sources; removing a source retains its installed plugins")
            .subcommand_required(true)
            .subcommand(Command::new("list"))
            .subcommand(Command::new("add").arg(Arg::new("url").value_name("SOURCE_URL")))
            .subcommand(
                Command::new("remove")
                    .about("Remove a source, retaining all installed plugins")
                    .arg(Arg::new("url").value_name("SOURCE_URL")),
            );
        command = command.subcommand(repositories);
        command
    }
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        let request = match args.subcommand() {
            Some(("list", args)) => match args.get_one::<String>("url") {
                Some(url) => {
                    RpcRequest::PluginCatalog(PluginRepositoryRequest { url: url.clone() })
                }
                None => RpcRequest::PluginRepositories(PluginRepositoriesRequest {}),
            },
            Some(("review", args)) => RpcRequest::PluginInspect(source(args)?),
            Some(("install", args)) => return install(args, context),
            Some((action @ ("on" | "off"), args)) => {
                RpcRequest::PluginSetEnabled(PluginSetEnabledRequest {
                    id: settings_input::required(args, "id", "Exact plugin ID")?,
                    enabled: action == "on",
                })
            }
            Some(("repositories", args)) => match args.subcommand() {
                Some(("list", _)) => RpcRequest::PluginRepositories(PluginRepositoriesRequest {}),
                Some((action @ ("add" | "remove"), args)) => {
                    let url = settings_input::required(args, "url", "Exact repository source URL")?;
                    if action == "remove" {
                        prompt::confirm("Remove this repository? Installed plugins stay installed and keep their current on/off state.", context.yes)?;
                    }
                    let request = PluginRepositoryRequest { url };
                    if action == "add" {
                        RpcRequest::PluginRepositoryAdd(request)
                    } else {
                        RpcRequest::PluginRepositoryRemove(request)
                    }
                }
                _ => {
                    return Err(CliError::usage(
                        "Choose plugins repositories list, add or remove.",
                    ))
                }
            },
            _ => {
                return Err(CliError::usage(
                    "Choose plugins list, review, install, on, off or repositories.",
                ))
            }
        };
        Ok(result(context.rpc(&request)?))
    }
}
fn source_command(name: &'static str) -> Command {
    Command::new(name)
        .arg(Arg::new("url").value_name("SOURCE_URL"))
        .arg(Arg::new("id").value_name("PLUGIN_ID"))
        .arg(Arg::new("version").value_name("RELEASE_VERSION"))
}
fn source(args: &ArgMatches) -> Result<PluginInspectRequest, CliError> {
    Ok(PluginInspectRequest {
        url: settings_input::required(args, "url", "Exact repository source URL")?,
        plugin_id: settings_input::required(args, "id", "Exact plugin ID")?,
        release_version: settings_input::required(
            args,
            "version",
            "Exact release version from the catalog",
        )?,
    })
}
fn install(args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
    let source = source(args)?;
    let explicit = args.get_one::<String>("approval");
    if explicit.is_none()
        && !prompt::interactive(io::stdin().is_terminal(), io::stdout().is_terminal())
    {
        return Err(CliError::usage("Plugin installation needs --approval with the exact digest you reviewed using plugins review. --yes does not grant permissions."));
    }
    let response = context.rpc(&RpcRequest::PluginInspect(source.clone()))?;
    let inspection = match &response {
        RpcResponse::PluginInspect(PluginInspectOutcome::Ok(inspection)) => inspection,
        _ => return Ok(result(response)),
    };
    let approval = match explicit {
        Some(approval) => approval.clone(),
        None => {
            eprintln!("{}", describe_inspection(inspection));
            prompt::read(
                "Review these permissions, then type the exact approval digest to install disabled",
            )?
        }
    };
    if approval != inspection.approval {
        return Err(CliError::failure(CODE_NEEDS_REVIEW, "The supplied approval digest does not match the current inspection. Nothing was installed.", "Run korri plugins review for this source, plugin and version, review it, then use its exact approval digest."));
    }
    let request = RpcRequest::PluginInstall(PluginInstallRequest {
        url: source.url,
        plugin_id: source.plugin_id,
        release_version: source.release_version,
        approval,
    });
    Ok(result(context.rpc(&request)?))
}
fn result(response: RpcResponse) -> Output {
    let human = match &response {
        RpcResponse::PluginRepositories(PluginRepositoriesOutcome::Ok(repositories)) => {
            let mut lines = vec![format!(
                "Official repository: {}",
                repositories
                    .official
                    .as_deref()
                    .map(output::text)
                    .unwrap_or_else(|| "not configured".into())
            )];
            for url in &repositories.user_added {
                lines.push(format!("Repository: {}", output::text(url)));
            }
            for plugin in &repositories.repository_installs {
                lines.push(format!(
                    "{}  {}  {}  {}",
                    output::text(&plugin.id),
                    state(plugin.enabled),
                    output::text(&plugin.release_version),
                    output::text(&plugin.source_url)
                ));
            }
            for plugin in &repositories.other_installs {
                lines.push(format!(
                    "{}  {}  {}",
                    output::text(&plugin.id),
                    state(plugin.enabled),
                    plugin
                        .title
                        .as_deref()
                        .map(output::text)
                        .unwrap_or_default()
                ));
            }
            lines.join("\n")
        }
        RpcResponse::PluginCatalog(PluginCatalogOutcome::Ok(catalog)) => {
            if catalog.entries.is_empty() {
                "No plugins in this catalog for this device.".into()
            } else {
                catalog
                    .entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{}  {}  {}  {:?}  publisher {}",
                            output::text(&entry.plugin_id),
                            output::text(&entry.release_version),
                            entry.title.as_deref().map(output::text).unwrap_or_default(),
                            entry.installed,
                            if entry.publisher_trusted {
                                "trusted"
                            } else {
                                "not trusted"
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        RpcResponse::PluginInspect(PluginInspectOutcome::Ok(inspection)) => {
            describe_inspection(inspection)
        }
        RpcResponse::PluginInstall(PluginChangeOutcome::Ok(_)) => {
            "Plugin installed, off. Use korri plugins on with its ID to enable it.".into()
        }
        RpcResponse::PluginSetEnabled(PluginChangeOutcome::Ok(_)) => "Plugin state updated.".into(),
        RpcResponse::PluginRepositoryAdd(PluginRepositoryAddOutcome::Ok(result)) => {
            format!("Repository: {result:?}")
        }
        RpcResponse::PluginRepositoryRemove(PluginRepositoryRemoveOutcome::Ok(result)) => format!(
            "Repository: {result:?}. Installed plugins stay installed and keep their on/off state."
        ),
        _ => output::unknown("Plugins", &response),
    };
    Output::one(response, human)
}
fn state(enabled: bool) -> &'static str {
    if enabled {
        "on"
    } else {
        "off"
    }
}
fn describe_inspection(inspection: &PluginInspection) -> String {
    let mut lines = vec![
        format!(
            "{}  {}",
            output::text(&inspection.id),
            inspection
                .title
                .as_deref()
                .map(output::text)
                .unwrap_or_default()
        ),
        format!("Warning: {}", output::text(&inspection.warning)),
        format!(
            "Native units: {}",
            inspection
                .native_units
                .iter()
                .map(|unit| output::text(unit))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        format!("TCP ports: {:?}", inspection.ports.allowed_tcp_ports),
        format!("UDP ports: {:?}", inspection.ports.allowed_udp_ports),
    ];
    for dependency in &inspection.brings {
        lines.push(format!(
            "Dependency: {}  {}  already approved: {}\nWarning: {}",
            output::text(&dependency.id),
            dependency
                .title
                .as_deref()
                .map(output::text)
                .unwrap_or_default(),
            dependency.already_approved,
            output::text(&dependency.warning)
        ));
    }
    lines.push(format!("Approval: {}", output::text(&inspection.approval)));
    lines.join("\n")
}
