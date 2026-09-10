use korri_plugin_host::{
    host::Host,
    package,
    provenance::SelectionIntent,
    repository::{Configuration, SourceUrl},
    source_store,
};
use std::{env, path::Path, process::ExitCode};

fn main() -> ExitCode {
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("korri-plugin: administrator access is required");
        return ExitCode::FAILURE;
    }
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("korri-plugin: {}", error.escape_default());
            ExitCode::FAILURE
        }
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn run(args: Vec<String>) -> Result<(), String> {
    let nix =
        env::var("KORRI_PLUGIN_NIX").map_err(|_| "host package must supply KORRI_PLUGIN_NIX")?;
    let systemctl = env::var("KORRI_PLUGIN_SYSTEMCTL")
        .map_err(|_| "host package must supply KORRI_PLUGIN_SYSTEMCTL")?;
    let iptables = env::var("KORRI_PLUGIN_IPTABLES")
        .map_err(|_| "host package must supply KORRI_PLUGIN_IPTABLES")?;
    let ip6tables = env::var("KORRI_PLUGIN_IP6TABLES")
        .map_err(|_| "host package must supply KORRI_PLUGIN_IP6TABLES")?;
    let host = Host::open(
        Path::new(&nix),
        Path::new(&systemctl),
        Path::new(&iptables),
        Path::new(&ip6tables),
    )?;
    let words = args.iter().map(String::as_str).collect::<Vec<_>>();
    match words.as_slice() {
        ["repository", commands @ ..] => {
            let curl = env::var("KORRI_PLUGIN_CURL").map_err(|_| "host package must supply KORRI_PLUGIN_CURL")?;
            repository_command(&host, &Configuration::from_host(Path::new(&curl))?, commands)
        }
        ["inspect", source, path] => {
            print_json(&host.inspect(source, Path::new(path))?)?;
            host.release_download()
        }
        ["install", source, id, "--release", revision] => {
            let curl = env::var("KORRI_PLUGIN_CURL").map_err(|_| "host package must supply KORRI_PLUGIN_CURL")?;
            let report = host.inspect_release(Path::new(&curl), source, id, revision)?;
            print_json(&report)?;
            eprintln!(
                "Release {revision}: not installed. Review this exact package and approval report, then run:\nsudo korri-plugin install '{}' '{}' '{}'\nEnable the plugin separately after installation.",
                source, report.package.display(), report.approval,
            );
            host.release_download()
        }
        ["install", source, path, approval] => {
            let report = host.inspect(source, Path::new(path))?;
            print_json(&report)?;
            host.install(report, approval, SelectionIntent::Install)
        }
        ["update", id, source, path, approval] => {
            let report = host.inspect(source, Path::new(path))?;
            print_json(&report)?;
            host.install(report, approval, SelectionIntent::Update { id })
        }
        ["enable", id] => host.set_enabled(id, true),
        ["disable", id] => host.set_enabled(id, false),
        ["remove", id] => host.remove(id, false),
        ["remove", id, "--purge"] => host.remove(id, true),
        ["status", id] => print_json(&host.status(id)?.ok_or("plugin is not installed")?),
        ["restore-all"] => host.restore_all(),
        ["restore", id] => host.rollback(id),
        ["enabled-packages"] => print_json(&host.enabled_packages()?),
        ["unit", id] => {
            korri_plugin_host::declaration::validate_id(id)?;
            println!("{}.service", package::unit_name(id));
            Ok(())
        }
        _ => Err("usage: korri-plugin inspect CACHE PACKAGE | install CACHE ID --release COMMIT (inspect, then approve the exact path) | install CACHE PACKAGE APPROVAL | update ID CACHE PACKAGE APPROVAL | enable ID | disable ID | remove ID [--purge] | status ID | unit ID | restore-all | restore ID | enabled-packages | repository COMMAND".into()),
    }
}

fn repository_command(host: &Host, config: &Configuration, args: &[&str]) -> Result<(), String> {
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
                source_store::RemoveOutcome::NotFound => Err(format!("repository not configured: {source}")),
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
        ["catalog", source] => print_json(&host.catalog(config, &SourceUrl::parse(source)?)?),
        ["inspect", source, id, release] => {
            print_json(&host.inspect_repository(config, &SourceUrl::parse(source)?, id, release)?)?;
            host.release_download()
        }
        ["install", source, id, release, approval] => {
            let report = host.inspect_repository(config, &SourceUrl::parse(source)?, id, release)?;
            print_json(&report)?;
            host.install(report, approval, SelectionIntent::Install)
        }
        ["update", id, release, approval] => {
            let source = host.installed_source(id)?;
            let report = host.inspect_repository(config, &source, id, release)?;
            print_json(&report)?;
            host.install(report, approval, SelectionIntent::Update { id })
        }
        ["switch", id, source, release, approval] => {
            let report = host.inspect_repository(config, &SourceUrl::parse(source)?, id, release)?;
            print_json(&report)?;
            host.install(report, approval, SelectionIntent::SwitchSource { id })
        }
        _ => Err("usage: korri-plugin repository add URL | remove URL | list | catalog URL | inspect URL ID RELEASE | install URL ID RELEASE APPROVAL | update ID RELEASE APPROVAL | switch ID URL RELEASE APPROVAL".into()),
    }
}
