//! Provider locations and the Steam runner use the same installed launch path
//! as file-backed runners. No Steam client or device is needed for this proof.
use korrid::{
    config::{
        decode_config_documents,
        resolver::{linux_route_candidates, resolve_launchable_routes, RouteDiagnosticCode},
        snapshot::ConfigSnapshotCoordinator,
    },
    game_routes::{self, GameRouteSelection, SelectedGameLaunchRequest},
    launcher::{
        linux_plugin::launch_route,
        plugin_launch::{evaluate_snapshot, PluginLaunchInput},
    },
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    script::source::SourceSnapshot,
    GameProviderIdentity,
};
use std::{collections::BTreeMap, fs, path::Path};

const LIBRARY: &str = "library:
  flinthook:
    title: Flinthook
    releases:
      - id: steam
        system: steam
        identity: {kind: provider, value: '@korri:steam/401710'}
";
const DEVICE: &str = "locations:
  '@korri:steam/401710': [{provider: '@korri:steam', ref: '401710'}]
";
const RUNNER_ID: &str = "@korri:steam/steam";

/// A required runtime plugin: the shipped artifact declaration and named files.
fn runtime(root: &Path, name: &str, keys: &[&str]) -> EnabledPackage {
    let package = root.join(format!("plugin-{name}"));
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/fixtures/first-party/{name}/plugin.ts")),
        )
        .unwrap(),
    )
    .unwrap();
    EnabledPackage {
        id: format!("@korri:{name}"),
        package,
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        files: keys
            .iter()
            .map(|key| ((*key).to_owned(), root.join(format!("{name}-{key}"))))
            .collect(),
        requires: Vec::new(),
    }
}

fn installed(root: &Path) -> PluginRegistry {
    let package = root.join("plugin-steam");
    fs::create_dir_all(&package).unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/first-party/steam/plugin.ts");
    fs::copy(source, package.join("plugin.ts")).unwrap();
    let fex = runtime(root, "fex", &["fex-unpack", "config"]);
    let proton = runtime(root, "proton", &["cachyos-unpack"]);
    let registry = PluginRegistry::from_installed(vec![
        EnabledPackage {
            id: "@korri:steam".into(),
            package,
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            files: BTreeMap::from([("steam-session".into(), root.join("steam-session"))]),
            requires: vec![fex.package.clone(), proton.package.clone()],
        },
        fex,
        proton,
    ])
    .unwrap();
    for artifact in registry.artifacts().values() {
        let (owner, key) = artifact.id.split_once('/').unwrap();
        let name = owner.split_once(':').unwrap().1;
        fs::create_dir_all(
            root.join("cache")
                .join(name)
                .join(key)
                .join(&artifact.version),
        )
        .unwrap();
    }
    registry
}

#[test]
fn matching_system_runner_offers_a_provider_route_without_file_storage() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = decode_config_documents(DEVICE, LIBRARY).unwrap();
    let registry = installed(root.path());
    let routes = linux_route_candidates(root.path(), &snapshot, &registry, "flinthook").unwrap();
    assert_eq!(routes.len(), 1);
    let route = &routes[0];
    assert_eq!(route.runner_id, RUNNER_ID);
    assert_eq!(route.release_id, "@korri:steam/401710");
    assert_eq!(route.system_id, "steam");
    assert!(route.file_target.is_none());
    // The route preserves the location, not just the game's presentation identity.
    assert_eq!(
        route.provider_target,
        Some(GameProviderIdentity {
            provider: "@korri:steam".into(),
            provider_ref: "401710".into(),
        })
    );
}

#[test]
fn provider_release_without_an_admitting_runner_reports_a_diagnostic() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = decode_config_documents(DEVICE, LIBRARY).unwrap();
    let registry = PluginRegistry::new(vec![], []).unwrap();
    let catalog = resolve_launchable_routes(root.path(), &snapshot, &registry, std::iter::empty());
    assert!(catalog.routes.is_empty());
    assert_eq!(catalog.diagnostics.len(), 1);
    assert_eq!(
        catalog.diagnostics[0].code,
        RouteDiagnosticCode::LocalRouteUnavailable
    );
    assert_eq!(
        catalog.diagnostics[0].playable_id.as_deref(),
        Some("flinthook")
    );
}

#[test]
fn a_runner_for_another_system_does_not_admit_a_provider_release() {
    let root = tempfile::tempdir().unwrap();
    let snapshot =
        decode_config_documents(DEVICE, &LIBRARY.replace("system: steam", "system: psp")).unwrap();
    let registry = installed(root.path());
    let diagnostic =
        linux_route_candidates(root.path(), &snapshot, &registry, "flinthook").unwrap_err();
    assert_eq!(diagnostic.code, RouteDiagnosticCode::LocalRouteUnavailable);
}

#[test]
fn provider_release_without_a_device_location_is_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = decode_config_documents("{}", LIBRARY).unwrap();
    let registry = installed(root.path());
    let diagnostic =
        linux_route_candidates(root.path(), &snapshot, &registry, "flinthook").unwrap_err();
    assert_eq!(diagnostic.code, RouteDiagnosticCode::LocalRouteUnavailable);
}

#[test]
fn provider_launch_passes_ref_and_no_content_path_to_the_installed_runner() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = decode_config_documents(DEVICE, LIBRARY).unwrap();
    let registry = installed(root.path());
    let route = linux_route_candidates(root.path(), &snapshot, &registry, "flinthook")
        .unwrap()
        .remove(0);
    let launch = launch_route(root.path(), &snapshot, &registry, &route, None, None).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&launch.command[3]).unwrap();
    assert_eq!(wire["ref"], "401710");
    assert!(wire.get("contentPath").is_none());
    let input: PluginLaunchInput = serde_json::from_value(wire).unwrap();
    let package = registry.installed_package(RUNNER_ID).unwrap();
    let source =
        SourceSnapshot::package_plugin(&package.package, &package.entry, &package.sources).unwrap();
    let plan = evaluate_snapshot(&source, &input).unwrap();
    assert_eq!(
        plan.command,
        root.path().join("steam-session").display().to_string()
    );
    assert_eq!(plan.args, ["401710"]);
    assert_eq!(
        plan.env["STEAM_HOME"],
        root.path()
            .join("users/default/steam")
            .display()
            .to_string()
    );
    assert_eq!(plan.directories, [plan.env["STEAM_HOME"].clone()]);
    let path = |name: &str| root.path().join(name).display().to_string();
    assert_eq!(
        plan.env["KORRI_FEX_ROOTFS"],
        input.artifacts["@korri:fex/rootfs"]
    );
    assert_eq!(plan.env["KORRI_FEX_CONFIG"], path("fex-config"));
    assert_eq!(
        plan.env["KORRI_PROTON_CACHYOS"],
        input.artifacts["@korri:proton/cachyos"]
    );
    assert!(plan.env_unset.is_empty());
    assert!(
        !root.path().join("users").exists(),
        "launch planning has no effects"
    );
}

#[test]
fn provider_release_reaches_the_portals_selected_launch_path() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("catalog")).unwrap();
    fs::write(root.path().join("catalog/library.yaml"), LIBRARY).unwrap();
    fs::write(root.path().join("device.yaml"), DEVICE).unwrap();
    let registry = installed(root.path());
    let state = ConfigSnapshotCoordinator::new(root.path()).reload();
    assert!(state.diagnostic.is_none(), "{:?}", state.diagnostic);
    let routes = game_routes::list(root.path(), &registry, "flinthook").unwrap();
    assert!(matches!(routes.selection, GameRouteSelection::Selected(_)));
    assert_eq!(routes.routes.len(), 1);
    assert_eq!(routes.routes[0].runner_id, RUNNER_ID);
    let launch = game_routes::selected_launch(
        root.path(),
        &registry,
        &SelectedGameLaunchRequest {
            game_id: "flinthook".into(),
            runner_id: RUNNER_ID.into(),
            overrides: None,
        },
        None,
    )
    .unwrap();
    let input: serde_json::Value = serde_json::from_str(&launch.command[3]).unwrap();
    assert_eq!(input["ref"], "401710");
    assert!(input.get("contentPath").is_none());
}
