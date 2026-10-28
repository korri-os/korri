use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
    plugin_author, prompt, settings_input,
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
        command = command
            .subcommand(repositories)
            .subcommand(
                Command::new("publishers").about("List this device's bound and trusted publishers"),
            )
            .subcommand(
                Command::new("trust")
                    .about("Trust a publisher on this device with its full Nix public key")
                    .arg(Arg::new("namespace").required(true))
                    .arg(
                        Arg::new("key")
                            .long("key")
                            .required(true)
                            .value_name("PUBLIC_KEY"),
                    ),
            )
            .subcommand(
                Command::new("revoke")
                    .about("Revoke a publisher pin; its plugins can no longer start")
                    .arg(Arg::new("namespace").required(true)),
            );
        for author in plugin_author::commands() {
            command = command.subcommand(author);
        }
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
            Some(("review", args)) => return review(args, context),
            Some(("sideload", args)) => return crate::plugin_sideload::run(args, context),
            Some(("publishers", _)) => RpcRequest::PluginPublishers(PluginPublishersRequest {}),
            Some(("trust", args)) => {
                let namespace = args
                    .get_one::<String>("namespace")
                    .expect("required")
                    .clone();
                plugin_author::validate_namespace(&namespace)?;
                let public_key = args.get_one::<String>("key").expect("required").clone();
                if plugin_author::fingerprint(&public_key).is_err() {
                    return Err(CliError::failure("PluginPublicKeyRequired", "A fingerprint is not a full publisher public key.", "Run korri plugins review <source> <plugin> <release> to trust the key it shows, or pass the full public key."));
                }
                RpcRequest::PluginTrustPublisher(PluginTrustPublisherRequest {
                    namespace,
                    public_key,
                })
            }
            Some(("revoke", args)) => {
                let namespace = args
                    .get_one::<String>("namespace")
                    .expect("required")
                    .clone();
                plugin_author::validate_namespace(&namespace)?;
                prompt::confirm("Revoke this publisher? Its plugins can no longer start; stop and cleanup remain available.", context.yes)?;
                RpcRequest::PluginRevokePublisher(PluginRevokePublisherRequest { namespace })
            }
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
        && (args.get_flag("json")
            || !prompt::interactive(io::stdin().is_terminal(), io::stdout().is_terminal()))
    {
        return Err(CliError::usage("Plugin installation needs --approval with the exact digest you reviewed using plugins review. --yes does not grant permissions."));
    }
    let response = inspect_with_trust(
        context,
        &RpcRequest::PluginInspect(source.clone()),
        args.get_flag("json"),
    )?;
    let inspection = match &response {
        RpcResponse::PluginInspect(PluginInspectOutcome::Ok(inspection)) => inspection,
        _ => return Ok(result(response)),
    };
    let approval = approval(args, inspection)?;
    let request = RpcRequest::PluginInstall(PluginInstallRequest {
        url: source.url,
        plugin_id: source.plugin_id,
        release_version: source.release_version,
        approval,
    });
    Ok(result(context.rpc(&request)?))
}
pub(crate) fn result(response: RpcResponse) -> Output {
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
                            match entry.publisher.state {
                                PluginPublisherState::Bound => "bound",
                                PluginPublisherState::Trusted => "trusted",
                                _ => "not trusted yet",
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
        RpcResponse::PluginPublishers(PluginPublishersOutcome::Ok(publishers)) => {
            if publishers.entries.is_empty() { "No publishers trusted on this device.".into() }
            else { publishers.entries.iter().map(|entry| format!("{}  {}  {}  installed: {}",
                output::text(&entry.namespace), output::text(&entry.fingerprint),
                if entry.state == PluginPublisherState::Bound { "bound" } else { "pinned" },
                entry.installed.iter().map(|id| output::text(id)).collect::<Vec<_>>().join(", ")
            )).collect::<Vec<_>>().join("\n") }
        }
        RpcResponse::PluginTrustPublisher(PluginChangeOutcome::Ok(_)) => "Publisher trusted on this device. Plugin permissions still need review and approval.".into(),
        RpcResponse::PluginRevokePublisher(PluginChangeOutcome::Ok(_)) => "Publisher revoked on this device. Its plugins can no longer start; stop and cleanup remain available.".into(),
        RpcResponse::PluginArchiveInspect(PluginInspectOutcome::Ok(inspection)) => describe_inspection(inspection),
        RpcResponse::PluginArchiveInstall(PluginChangeOutcome::Ok(_)) => "Plugin installed, off. Use korri plugins on with its ID to enable it.".into(),
        _ => match crate::output::failure(&response) {
            Some(failure) => format!("{}\nNext: {}", output::text(&failure.message), next_step(&failure.code)),
            None => "Plugins: unknown.\nNext: Check that korri and korrid use the same version.".into(),
        },
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
pub(crate) fn describe_inspection(inspection: &PluginInspection) -> String {
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
        describe_publisher(&inspection.publisher),
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
            "Dependency: {}  {}  already approved: {}\n{}\nWarning: {}",
            output::text(&dependency.id),
            dependency
                .title
                .as_deref()
                .map(output::text)
                .unwrap_or_default(),
            dependency.already_approved,
            describe_publisher(&dependency.publisher),
            output::text(&dependency.warning)
        ));
    }
    lines.push(format!("Approval: {}", output::text(&inspection.approval)));
    lines.join("\n")
}

fn review(args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
    Ok(result(inspect_with_trust(
        context,
        &RpcRequest::PluginInspect(source(args)?),
        args.get_flag("json"),
    )?))
}

pub(crate) fn can_prompt(json: bool) -> bool {
    !json && prompt::interactive(io::stdin().is_terminal(), io::stdout().is_terminal())
}

pub(crate) fn inspection(response: &RpcResponse) -> Option<&PluginInspection> {
    match response {
        RpcResponse::PluginInspect(PluginInspectOutcome::Ok(inspection))
        | RpcResponse::PluginArchiveInspect(PluginInspectOutcome::Ok(inspection)) => {
            Some(inspection)
        }
        _ => None,
    }
}

/// Trust is always an explicit terminal choice, independent of --yes. Scripts
/// use the separate trust command. Re-inspection obtains the new trust facts.
pub(crate) fn inspect_with_trust(
    context: &Context,
    request: &RpcRequest,
    json: bool,
) -> Result<RpcResponse, CliError> {
    let response = context.rpc(request)?;
    let Some(report) = inspection(&response) else {
        return Ok(response);
    };
    if !can_prompt(json) {
        return Ok(response);
    }
    let pending = std::iter::once(&report.publisher)
        .chain(report.brings.iter().map(|dependency| &dependency.publisher))
        .filter(|publisher| {
            matches!(
                publisher.state,
                PluginPublisherState::New | PluginPublisherState::KeyChanged
            )
        })
        .collect::<Vec<_>>();
    if pending.is_empty() {
        return Ok(response);
    }
    eprintln!("{}", describe_inspection(report));
    let mut trusted = std::collections::BTreeSet::new();
    for publisher in pending {
        if !trusted.insert(&publisher.namespace) {
            continue;
        }
        let key = publisher.public_key.clone().ok_or_else(|| {
            CliError::failure(
                "PluginPublicKeyRequired",
                "This inspection did not include the publisher's public key.",
                "Check that korri and korrid use the same version, then review again.",
            )
        })?;
        prompt::confirm(
            &format!(
                "Trust {} with key {} on this device?",
                output::text(&publisher.namespace),
                publisher
                    .fingerprint
                    .as_deref()
                    .map(output::text)
                    .unwrap_or_else(|| "unknown".into())
            ),
            false,
        )?;
        let applied = context.rpc(&RpcRequest::PluginTrustPublisher(
            PluginTrustPublisherRequest {
                namespace: publisher.namespace.clone(),
                public_key: key,
            },
        ))?;
        if crate::output::failure(&applied).is_some() {
            return Ok(applied);
        }
    }
    context.rpc(request)
}

pub(crate) fn approval(args: &ArgMatches, report: &PluginInspection) -> Result<String, CliError> {
    let approval = match args.get_one::<String>("approval") {
        Some(approval) => approval.clone(),
        None if !can_prompt(args.get_flag("json")) => return Err(CliError::usage("Plugin installation needs --approval with the exact digest you reviewed. --yes does not grant permissions.")),
        None => {
            eprintln!("{}", describe_inspection(report));
            prompt::read("Review these permissions, then type the exact approval digest to install disabled")?
        }
    };
    if approval != report.approval {
        return Err(CliError::failure(CODE_NEEDS_REVIEW, "The supplied approval digest does not match the current inspection. Nothing was installed.", "Review the current plugin permissions, then use its exact approval digest."));
    }
    Ok(approval)
}

fn describe_publisher(publisher: &PluginPublisher) -> String {
    let state = match publisher.state {
        PluginPublisherState::Bound => "Bound",
        PluginPublisherState::Trusted => "Trusted",
        PluginPublisherState::New => "New",
        PluginPublisherState::KeyChanged => "Key changed",
        PluginPublisherState::Unsigned => "Unsigned",
    };
    let mut lines = vec![format!(
        "Publisher: {}  {}  {}",
        output::text(&publisher.namespace),
        state,
        publisher
            .fingerprint
            .as_deref()
            .map(output::text)
            .unwrap_or_else(|| "no fingerprint".into())
    )];
    match publisher.state {
        PluginPublisherState::Unsigned => lines.push(output::text(publisher.disclosure.as_deref().unwrap_or("This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from."))),
        PluginPublisherState::KeyChanged => lines.push(format!("The publisher key for {} changed. Before: {}. Now: {}. This can mean the author replaced their key, or that someone else is publishing as them. Trust the new key only if the author told you about the change.",
            output::text(&publisher.namespace), publisher.previous_fingerprint.as_deref().map(output::text).unwrap_or_else(|| "unknown".into()), publisher.fingerprint.as_deref().map(output::text).unwrap_or_else(|| "unknown".into()))),
        _ => {}
    }
    lines.join("\n")
}
fn next_step(code: &str) -> &'static str {
    match code {
        CODE_PUBLISHER_NOT_TRUSTED | CODE_PUBLISHER_KEY_CHANGED => "Review the publisher key, then explicitly trust it on this device with korri plugins trust NAMESPACE --key PUBLIC_KEY.",
        CODE_NAMESPACE_TAKEN => "Remove the unsigned plugins in this namespace before trusting its publisher.",
        CODE_REQUIREMENT_CONFLICT => "Review the installed plugins that lock this dependency before changing releases.",
        CODE_NEEDS_REVIEW => "Run korri plugins review and approve its exact current digest.",
        _ => "Check korri status and the repository address, then try again.",
    }
}
