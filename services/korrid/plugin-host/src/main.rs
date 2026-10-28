use korri_plugin_host::plugin_admin::PluginRefusal as Refusal;
use korri_plugin_host::{
    admin,
    host::Host,
    lifecycle::LifecyclePolicy,
    package,
    provenance::{Provenance, SelectionIntent},
    repository::{Configuration, SourceUrl},
    selection::{Desired, Receipt},
    source_store,
};
use std::{env, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    // Image-time seeding computes the approval digest for a package that is
    // already in the store. It reads no host state and changes nothing, so it
    // needs neither administrator access nor nix. The device re-derives the
    // same report with the full closure check before it trusts the receipt.
    if let ["seed", package, cache_url] = args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        return match seed_receipt(package, cache_url) {
            Ok(receipt) => {
                println!("{receipt}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("korri-plugin: {}", error.escape_default());
                ExitCode::FAILURE
            }
        };
    }
    // A product supplies its existing PublisherBindings and the complete exact
    // selection, not a host configuration or an inherited dependency cache.
    if args.first().is_some_and(|word| word == "seed-graph") {
        return match seed_graph(&args[1..]) {
            Ok(receipts) => match print_json(&receipts) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("korri-plugin: {}", error.escape_default());
                    ExitCode::FAILURE
                }
            },
            Err(error) => {
                eprintln!("korri-plugin: {}", error.escape_default());
                ExitCode::FAILURE
            }
        };
    }
    // The image builder needs the same receipt directory and GC-root name as
    // the running host. Keep that name in one producer, not in a shell copy.
    if let ["unit-name", id] = args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        if let Err(error) = korri_plugin_host::declaration::validate_id(id) {
            eprintln!("korri-plugin: {error}");
            return ExitCode::FAILURE;
        }
        println!("{}", package::unit_name(id));
        return ExitCode::SUCCESS;
    }
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("korri-plugin: administrator access is required");
        return ExitCode::FAILURE;
    }
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("korri-plugin: {}", error.escape_default());
            ExitCode::FAILURE
        }
    }
}

fn seed_graph(args: &[String]) -> Result<Vec<Receipt>, String> {
    let (disabled, bindings, packages) = seed_graph_args(args)?;
    let bindings = package::publisher_bindings(Path::new(bindings))?;
    let selected = packages
        .iter()
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let reports = package::load_graph_for_seed(&selected, &bindings)?;
    seed_selections(&reports, &disabled)
}

fn seed_selections(
    reports: &[package::Report],
    disabled: &std::collections::BTreeSet<&str>,
) -> Result<Vec<Receipt>, String> {
    for id in disabled {
        if !reports.iter().any(|report| &report.id == id) {
            return Err(format!(
                "disabled plugin {id} is not among selected packages"
            ));
        }
    }
    for report in reports {
        if disabled.contains(report.id.as_str()) {
            continue;
        }
        for required in &report.requires {
            let dependency = reports
                .iter()
                .find(|candidate| &candidate.package == required)
                .expect("validated complete selection");
            if disabled.contains(dependency.id.as_str()) {
                return Err(format!(
                    "enabled plugin {} requires disabled plugin {}",
                    report.id, dependency.id
                ));
            }
        }
    }
    Ok(reports
        .iter()
        .map(|report| {
            let mut receipt = seed_selection(report);
            if disabled.contains(report.id.as_str()) {
                receipt.desired = Desired::Disabled;
            }
            receipt
        })
        .collect())
}

fn seed_graph_args(
    args: &[String],
) -> Result<(std::collections::BTreeSet<&str>, &str, &[String]), String> {
    let mut disabled = std::collections::BTreeSet::new();
    let mut words = args;
    while let [option, rest @ ..] = words {
        if option != "--disabled" {
            break;
        }
        let [id, rest @ ..] = rest else {
            return Err("--disabled requires a plugin ID".into());
        };
        korri_plugin_host::declaration::validate_id(id)?;
        disabled.insert(id.as_str());
        words = rest;
    }
    let [bindings, packages @ ..] = words else {
        return Err("seed-graph requires publisher bindings and selected packages".into());
    };
    if packages.is_empty() {
        return Err("seed-graph requires at least one selected package".into());
    }
    Ok((disabled, bindings, packages))
}

fn seed_receipt(package: &str, cache_url: &str) -> Result<String, String> {
    let provenance = Provenance::RawCache {
        cache_url: cache_url.into(),
    };
    let report = package::load_for_seed(Path::new(package), provenance.clone())?;
    serde_json::to_string_pretty(&seed_selection(&report)).map_err(|e| e.to_string())
}

fn seed_selection(report: &package::Report) -> Receipt {
    Receipt {
        id: report.id.clone(),
        package: report.package.clone(),
        provenance: report.provenance.clone(),
        approval: report.approval.clone(),
        desired: Desired::Enabled,
        previous: None,
    }
}

fn print_review(host: &Host, report: &package::Report) -> Result<(), Refusal> {
    let mut reviewed = report.clone();
    reviewed.approval = host.review_approval(report)?;
    print_json(&reviewed).map_err(Refusal::from)
}

fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(not(feature = "vm-lifecycle-policy"))]
fn lifecycle_policy() -> Result<LifecyclePolicy, String> {
    // The generic operator CLI has no product or release authority from which
    // to classify required behavior. Keep every removal unavailable rather
    // than silently treating that missing authority as an empty policy.
    Ok(LifecyclePolicy::removal_unavailable())
}

#[cfg(feature = "vm-lifecycle-policy")]
fn lifecycle_policy() -> Result<LifecyclePolicy, String> {
    use korri_plugin_host::lifecycle::RequiredPlugin;

    // Private VM fixture: this compiles an explicit product caller around the
    // same production Host boundary. The normal package never enables it.
    LifecyclePolicy::new(vec![RequiredPlugin::new(
        "@example:required-lifecycle",
        "portal accepts local input",
    )?])
}

fn open_host() -> Result<Host, Refusal> {
    let nix =
        env::var("KORRI_PLUGIN_NIX").map_err(|_| "host package must supply KORRI_PLUGIN_NIX")?;
    let systemctl = env::var("KORRI_PLUGIN_SYSTEMCTL")
        .map_err(|_| "host package must supply KORRI_PLUGIN_SYSTEMCTL")?;
    let iptables = env::var("KORRI_PLUGIN_IPTABLES")
        .map_err(|_| "host package must supply KORRI_PLUGIN_IPTABLES")?;
    let ip6tables = env::var("KORRI_PLUGIN_IP6TABLES")
        .map_err(|_| "host package must supply KORRI_PLUGIN_IP6TABLES")?;
    Host::open(
        Path::new(&nix),
        Path::new(&systemctl),
        Path::new(&iptables),
        Path::new(&ip6tables),
        lifecycle_policy()?,
    )
}

fn repository_configuration() -> Result<Configuration, String> {
    let curl =
        env::var("KORRI_PLUGIN_CURL").map_err(|_| "host package must supply KORRI_PLUGIN_CURL")?;
    Configuration::from_host(Path::new(&curl))
}

/// korrid's uid, supplied by the NixOS module like KORRI_LOCAL_SIGNER_PEER_UID.
fn admin_peer_uid() -> Result<u32, String> {
    let uid = env::var("KORRI_PLUGIN_ADMIN_PEER_UID")
        .map_err(|_| "KORRI_PLUGIN_ADMIN_PEER_UID must be set")?
        .parse::<u32>()
        .map_err(|_| "KORRI_PLUGIN_ADMIN_PEER_UID must be a numeric ID")?;
    if uid == 0 {
        return Err("KORRI_PLUGIN_ADMIN_PEER_UID must identify an unprivileged account".into());
    }
    Ok(uid)
}

fn inherited_listener() -> Result<std::os::unix::net::UnixListener, String> {
    use std::os::fd::FromRawFd;
    let count = env::var("LISTEN_FDS")
        .map_err(|_| "LISTEN_FDS must be set")?
        .parse::<u32>()
        .map_err(|_| "LISTEN_FDS must be a number")?;
    let pid = env::var("LISTEN_PID")
        .map_err(|_| "LISTEN_PID must be set")?
        .parse::<u32>()
        .map_err(|_| "LISTEN_PID must be a number")?;
    if count != 1 || pid != std::process::id() {
        return Err(
            "serve-admin requires exactly one inherited listener for its current PID".into(),
        );
    }
    Ok(unsafe { std::os::unix::net::UnixListener::from_raw_fd(3) })
}

/// One request at a time. Each opens the host, and so takes its lock, only
/// for that request; boot restore and the CLI keep working in between.
fn serve_admin() -> Result<(), Refusal> {
    let peer = admin_peer_uid()?;
    let listener = inherited_listener()?;
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("korri-plugin: admin accept failed: {error}");
                continue;
            }
        };
        let served = admin::serve_connection(stream, peer, |request| {
            match open_host().and_then(|host| Ok((host, repository_configuration()?))) {
                Ok((host, config)) => admin::handle(&host, &config, request),
                Err(error) => admin::host_refused(error),
            }
        });
        if let Err(error) = served {
            eprintln!("korri-plugin: admin connection: {}", error.escape_default());
        }
    }
    Ok(())
}

fn run(args: Vec<String>) -> Result<(), Refusal> {
    if args == ["serve-admin"] {
        return serve_admin();
    }
    let host = open_host()?;
    let words = args.iter().map(String::as_str).collect::<Vec<_>>();
    match words.as_slice() {
        ["repository", commands @ ..] => {
            repository_command(&host, &repository_configuration()?, commands)
        }
        ["inspect", source, path] => {
            print_review(&host, &host.inspect(source, Path::new(path))?)?;
            host.release_download()
        }
        ["install", source, id, "--release", revision] => {
            let curl = env::var("KORRI_PLUGIN_CURL").map_err(|_| "host package must supply KORRI_PLUGIN_CURL")?;
            let report = host.inspect_release(Path::new(&curl), source, id, revision)?;
            print_review(&host, &report)?;
            eprintln!(
                "Release {revision}: not installed. Review this exact package and approval report, then run:\nsudo korri-plugin install '{}' '{}' '{}'\nEnable the plugin separately after installation.",
                source, report.package.display(), host.review_approval(&report)?,
            );
            host.release_download()
        }
        ["install", source, path, approval] => {
            let report = host.inspect(source, Path::new(path))?;
            print_review(&host, &report)?;
            host.install(report, approval, SelectionIntent::Install)
        }
        ["update", id, source, path, approval] => {
            let report = host.inspect(source, Path::new(path))?;
            print_review(&host, &report)?;
            host.install(report, approval, SelectionIntent::Update { id })
        }
        ["enable", id] => host.set_enabled(id, true),
        ["disable", id] => host.set_enabled(id, false),
        ["remove", id] => host.remove(id, false),
        ["remove", id, "--purge"] => host.remove(id, true),
        ["status", id] => print_json(&host.status(id)?.ok_or("plugin is not installed")?).map_err(Refusal::from),
        ["restore-all"] => host.restore_all(),
        ["restore", id] => host.rollback(id),
        ["enabled-packages"] => print_json(&host.enabled_packages()?).map_err(Refusal::from),
        ["unit", id] => {
            korri_plugin_host::declaration::validate_id(id)?;
            println!("{}.service", package::unit_name(id));
            Ok(())
        }
        _ => Err("usage: korri-plugin seed PACKAGE CACHE_URL | seed-graph [--disabled ID]... PUBLISHER_BINDINGS PACKAGE... | inspect CACHE PACKAGE | install CACHE ID --release COMMIT (inspect, then approve the exact path) | install CACHE PACKAGE APPROVAL | update ID CACHE PACKAGE APPROVAL | enable ID | disable ID | remove ID [--purge] | status ID | unit ID | restore-all | restore ID | enabled-packages | repository COMMAND | serve-admin".into()),
    }
}

fn repository_command(host: &Host, config: &Configuration, args: &[&str]) -> Result<(), Refusal> {
    match args {
        ["add", raw] => {
            let source = SourceUrl::parse(raw)?;
            let outcome = host.add_source(config, &source)?;
            println!("repository {outcome:?}: {source}");
            Ok(())
        }
        ["remove", raw] => {
            let source = SourceUrl::parse(raw)?;
            match host.remove_source(config, &source)? {
                source_store::RemoveOutcome::Removed => { println!("repository removed: {source}"); Ok(()) }
                source_store::RemoveOutcome::NotFound => Err(format!("repository not configured: {source}").into()),
            }
        }
        ["list"] => {
            match &config.official {
                Some(source) => println!("official: {source}"),
                None => println!("official: not configured; set services.korri.pluginHost.officialCatalogUrl"),
            }
            for source in host.sources()? {
                if config.official.as_ref() != Some(&source) { println!("user-added: {source}"); }
            }
            Ok(())
        }
        ["catalog", source] => print_json(&host.catalog(config, &SourceUrl::parse(source)?)?).map_err(Refusal::from),
        ["inspect", source, id, release] => {
            print_review(host, &host.inspect_repository(config, &SourceUrl::parse(source)?, id, release)?)?;
            host.release_download()
        }
        ["install", source, id, release, approval] => {
            let report = host.inspect_repository(config, &SourceUrl::parse(source)?, id, release)?;
            print_review(host, &report)?;
            host.install(report, approval, SelectionIntent::Install)
        }
        ["update", id, release, approval] => {
            let source = host.installed_source(id)?;
            let report = host.inspect_repository(config, &source, id, release)?;
            print_review(host, &report)?;
            host.install(report, approval, SelectionIntent::Update { id })
        }
        ["switch", id, source, release, approval] => {
            let report = host.inspect_repository(config, &SourceUrl::parse(source)?, id, release)?;
            print_review(host, &report)?;
            host.install(report, approval, SelectionIntent::SwitchSource { id })
        }
        _ => Err("usage: korri-plugin repository add URL | remove URL | list | catalog URL | inspect URL ID RELEASE | install URL ID RELEASE APPROVAL | update ID RELEASE APPROVAL | switch ID URL RELEASE APPROVAL".into()),
    }
}

#[cfg(test)]
mod seed_tests {
    use super::*;
    fn report(name: &str, requires: Vec<std::path::PathBuf>) -> package::Report {
        package::Report {
            publisher: None,
            id: format!("@korri:{name}"),
            package: format!("/nix/store/{name}").into(),
            requires,
            provenance: Provenance::RawCache {
                cache_url: "https://cache.example".into(),
            },
            approval: "approval".into(),
            policy: package::BASE_POLICY,
            warning: String::new(),
            unit: None,
            state_directory: String::new(),
            runtime_directory: String::new(),
            unit_configuration: String::new(),
            declaration: korri_plugin_host::declaration::Declaration::evaluate(
                "@korri",
                &format!("export const name = {name:?};"),
            )
            .unwrap(),
            native_units: Default::default(),
            ports: Default::default(),
            hosts: Vec::new(),
            packages: Default::default(),
            files: Default::default(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            brings: vec![],
        }
    }

    #[test]
    fn seeding_disables_only_selected_ids_and_refuses_enabled_dependents() {
        let runner = report("fake08", vec![]);
        let pack = report("starter-pack", vec![runner.package.clone()]);
        let reports = [runner, pack];
        let receipts = seed_selections(
            &reports,
            &std::collections::BTreeSet::from(["@korri:starter-pack"]),
        )
        .unwrap();
        assert_eq!(receipts[0].desired, Desired::Enabled);
        assert_eq!(receipts[1].desired, Desired::Disabled);
        assert_eq!(receipts[1].approval, reports[1].approval);
        assert!(seed_selections(
            &reports,
            &std::collections::BTreeSet::from(["@korri:missing"])
        )
        .unwrap_err()
        .contains("not among selected"));
        assert!(seed_selections(
            &reports,
            &std::collections::BTreeSet::from(["@korri:fake08"])
        )
        .unwrap_err()
        .contains("requires disabled"));
        assert!(
            seed_selections(&reports, &std::collections::BTreeSet::new())
                .unwrap()
                .iter()
                .all(|r| r.desired == Desired::Enabled)
        );
        let all_off = std::collections::BTreeSet::from(["@korri:fake08", "@korri:starter-pack"]);
        assert!(seed_selections(&reports, &all_off)
            .unwrap()
            .iter()
            .all(|r| r.desired == Desired::Disabled));
    }

    #[test]
    fn disabled_options_precede_bindings_and_repeat() {
        let args = [
            "--disabled",
            "@korri:starter-pack",
            "--disabled",
            "@games:other",
            "publishers.json",
            "/nix/store/package",
        ]
        .map(String::from);
        let (disabled, bindings, packages) = seed_graph_args(&args).unwrap();
        assert_eq!(
            disabled.into_iter().collect::<Vec<_>>(),
            ["@games:other", "@korri:starter-pack"]
        );
        assert_eq!(bindings, "publishers.json");
        assert_eq!(packages, ["/nix/store/package"]);
        for args in [
            vec![],
            vec!["--disabled"],
            vec!["--disabled", "bad", "bindings", "package"],
            vec!["bindings"],
        ] {
            assert!(
                seed_graph_args(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err()
            );
        }
    }
}
