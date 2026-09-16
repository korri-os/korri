//! Evaluate one declaration-only plugin and explain its local announcement.
//!
//! This intentionally stops at the registry boundary: it does not load
//! persisted configuration, resolve content, perform a launch, or publish to
//! federation peers.

use std::{env, fs, process};

use korrid::plugin::{load_plugin_source, Plugin, PluginRegistry};
use korrid::plugin_policy::{MGBA_PLUGIN_ID, RETROARCH_PLUGIN_ID, RETROARCH_PLUGIN_SOURCE};

enum ReportMode {
    Enabled,
    Disabled,
    Review,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("PLUGIN REGISTRY FAILED: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let namespace = args.next().ok_or_else(|| {
        "usage: plugin_registry_probe <publisher-namespace> <plugin.ts> [--disabled|--review]"
            .to_owned()
    })?;
    let path = args.next().ok_or("missing plugin.ts path")?;
    let mode = match args.next().as_deref() {
        None => ReportMode::Enabled,
        Some("--disabled") => ReportMode::Disabled,
        Some("--review") => ReportMode::Review,
        Some(other) => return Err(format!("unknown argument {other}")),
    };
    if args.next().is_some() {
        return Err("too many arguments".to_owned());
    }

    let source =
        fs::read_to_string(&path).map_err(|error| format!("cannot read {path}: {error}"))?;
    // This read-only probe has no package authority. The reviewer supplies
    // the namespace explicitly; only compiled bundled source uses @korri.
    let plugin = load_plugin_source(&namespace, &source).map_err(|error| error.to_string())?;

    match mode {
        ReportMode::Enabled => print_report(plugin, true),
        ReportMode::Disabled => print_report(plugin, false),
        ReportMode::Review => {
            println!("== enabled plugin ==");
            print_report(plugin.clone(), true)?;
            println!();
            println!("== disabled plugin ==");
            print_report(plugin, false)
        }
    }
}

fn print_report(plugin: Plugin, enabled: bool) -> Result<(), String> {
    let plugin_id = plugin.id().to_owned();
    let mut plugins = Vec::new();
    let mut enabled_ids = Vec::new();
    if plugin_id == MGBA_PLUGIN_ID {
        plugins.push(
            load_plugin_source("@korri", RETROARCH_PLUGIN_SOURCE)
                .map_err(|error| error.to_string())?,
        );
        enabled_ids.push(RETROARCH_PLUGIN_ID.to_owned());
    }
    if enabled {
        enabled_ids.push(plugin_id.clone());
    }
    plugins.push(plugin);
    let registry = PluginRegistry::new(plugins, enabled_ids).map_err(|error| error.to_string())?;

    println!("plugin: {plugin_id}");
    println!(
        "registered: {}",
        yes_no(
            registry
                .registered_plugin_ids()
                .contains(&plugin_id.as_str())
        )
    );
    println!(
        "enabled: {}",
        yes_no(registry.enabled_plugin_ids().contains(&plugin_id.as_str()))
    );
    print_records(
        "registered-session-control",
        registry
            .registered_session_controls()
            .values()
            .map(|record| record.id.as_str()),
    );
    print_records(
        "provider",
        registry
            .providers()
            .iter()
            .filter(|(id, _)| id.as_str() == plugin_id)
            .map(|(_, record)| record.id.as_str()),
    );
    let owned_prefix = format!("{plugin_id}/");
    print_records(
        "system",
        registry
            .systems()
            .iter()
            .filter(|(id, _)| id.starts_with(&owned_prefix))
            .map(|(_, record)| record.id.as_str()),
    );
    print_records(
        "family",
        registry
            .families()
            .values()
            .filter(|record| record.id == plugin_id)
            .map(|record| record.id.as_str()),
    );
    print_records(
        "transport",
        registry
            .transports()
            .values()
            .map(|record| record.id.as_str()),
    );
    print_records(
        "runner",
        registry
            .runners()
            .iter()
            .filter(|(id, _)| id.starts_with(&owned_prefix))
            .map(|(_, record)| record.id.as_str()),
    );
    print_records(
        "discovery",
        registry
            .file_release_discovery_claims()
            .values()
            .filter(|claim| claim.id.starts_with(&owned_prefix))
            .map(|claim| claim.id.as_str()),
    );
    print_records(
        "session-control",
        registry
            .session_controls()
            .values()
            .map(|record| record.id.as_str()),
    );

    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn print_records<'a>(label: &str, records: impl Iterator<Item = &'a str>) {
    let records: Vec<&str> = records.collect();
    if records.is_empty() {
        println!("{label}: none");
    } else {
        for record in records {
            println!("{label}: {record}");
        }
    }
}
