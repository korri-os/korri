//! Resolve local readable routes through the production snapshot, policy, registry, and resolver.
//!
//! This stops before launch-spec construction and Android effects. It explains
//! which checkpoint routes are available through enabled plugin declarations and
//! which diagnostics remain when policy disables those declarations.

use std::{env, process};

use korrid::config::resolver::{resolve_launchable_routes, resolve_route, ResolvedRoute};
use korrid::config::snapshot::ConfigSnapshotCoordinator;
use korrid::plugin::PluginRegistry;
use korrid::plugin_policy::{
    bundled_plugin_policy_layer, bundled_plugins, resolve_enabled_plugin_ids, PluginPolicyLayer,
};

const CHECKPOINT_PLAYABLE_ID: &str = "01K4J6K8Y00000000000000001";
const STATIC_PLAYABLE_IDS: [&str; 0] = [];

#[derive(Clone, Copy)]
enum ReportMode {
    Enabled,
    Disabled,
    Review,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("PLUGIN ROUTE FAILED: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let root = args.next().ok_or_else(|| {
        "usage: plugin_route_probe <storage-root> [--disabled|--review]".to_owned()
    })?;
    let mode = match args.next().as_deref() {
        None => ReportMode::Enabled,
        Some("--disabled") => ReportMode::Disabled,
        Some("--review") => ReportMode::Review,
        Some(other) => return Err(format!("unknown argument {other}")),
    };
    if args.next().is_some() {
        return Err("too many arguments".to_owned());
    }

    match mode {
        ReportMode::Enabled => print_report(&root, true),
        ReportMode::Disabled => print_report(&root, false),
        ReportMode::Review => {
            println!("== enabled route ==");
            print_report(&root, true)?;
            println!();
            println!("== disabled route ==");
            print_report(&root, false)
        }
    }
}

fn print_report(root: &str, enabled: bool) -> Result<(), String> {
    let root = std::path::Path::new(root);
    let coordinator = ConfigSnapshotCoordinator::new(root);
    let state = coordinator.reload();
    println!("snapshot generation: {}", state.generation);
    match &state.diagnostic {
        Some(diagnostic) => println!(
            "snapshot diagnostic: {:?}: {}",
            diagnostic.code, diagnostic.message
        ),
        None => println!("snapshot diagnostic: none"),
    }

    let registry = registry(enabled)?;
    println!(
        "android plugin enabled: {}",
        if registry
            .enabled_plugin_ids()
            .contains(&"@korri:android-app")
        {
            "yes"
        } else {
            "no"
        }
    );

    let catalog = resolve_launchable_routes(root, &state.snapshot, &registry, STATIC_PLAYABLE_IDS);
    if catalog.routes.is_empty() {
        println!("route: none");
    } else {
        for route in &catalog.routes {
            print_route(route);
        }
    }
    if catalog.diagnostics.is_empty() {
        println!("diagnostic: none");
    } else {
        for diagnostic in &catalog.diagnostics {
            println!(
                "diagnostic: {:?}: {}{}",
                diagnostic.code,
                diagnostic.message,
                diagnostic
                    .playable_id
                    .as_deref()
                    .map(|id| format!(" [{id}]"))
                    .unwrap_or_default()
            );
        }
    }

    match resolve_route(
        root,
        &state.snapshot,
        &registry,
        STATIC_PLAYABLE_IDS,
        CHECKPOINT_PLAYABLE_ID,
    ) {
        Ok(route) => println!(
            "direct {CHECKPOINT_PLAYABLE_ID}: {}",
            route.flattened_target
        ),
        Err(diagnostic) => println!(
            "direct {CHECKPOINT_PLAYABLE_ID}: {:?}: {}",
            diagnostic.code, diagnostic.message
        ),
    }

    Ok(())
}

fn registry(enabled: bool) -> Result<PluginRegistry, String> {
    let plugins = bundled_plugins().map_err(|error| error.to_string())?;
    let enabled_ids = if enabled {
        resolve_enabled_plugin_ids([bundled_plugin_policy_layer()])
    } else {
        resolve_enabled_plugin_ids([
            bundled_plugin_policy_layer(),
            PluginPolicyLayer::from_enabled([("@korri:android-app", false)]),
        ])
    };
    PluginRegistry::new(plugins, enabled_ids).map_err(|error| error.to_string())
}

fn print_route(route: &ResolvedRoute) {
    println!("route: {}", route.playable_id);
    if let Some(title) = &route.title {
        println!("title: {title}");
    }
    println!("release: {}", route.release_id);
    println!("provider: {}", route.provider_id);
    println!("system: {}", route.system_id);
    if let Some(system_title) = &route.system_title {
        println!("system title: {system_title}");
    }
    println!("runner: {}", route.runner_id);
    println!("runner family: {:?}", route.family_id.as_deref());
    println!("integration token: {}", route.integration_token);
    println!("target: {}", route.flattened_target);
}
