use korrid::{
    config::{resolver, snapshot::ConfigSnapshotCoordinator},
    discovery::{DiscoveryCoordinator, DiscoveryOptions},
    game_routes::{self, GameRouteSelection, SelectedGameLaunchRequest},
    launcher::plugin_launch::{evaluate_snapshot, PluginLaunchInput},
    plugin::{load_plugin_source, PluginRegistry},
    plugin_installation::EnabledPackage,
    plugin_policy::RegistrySource,
    script::source::SourceSnapshot,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

fn hash(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn content_source(releases: serde_json::Value) -> String {
    format!(
        "export const name = 'content';
         export const runners = {{main: {{
           id: '@test:content/main', program: 'program', releases: {releases}
         }}}};
         export const handlers = {{'launch.prepare': input => ({{
           command: input.program, args: [input.contentPath]
         }})}};"
    )
}

#[test]
fn runner_accepts_a_list_of_whole_file_release_hashes() {
    let source = content_source(serde_json::json!([hash(b"first"), hash(b"second")]));
    let plugin = load_plugin_source("@test", &source).unwrap();
    let registry = PluginRegistry::new(vec![plugin], ["@test:content".into()]).unwrap();
    assert!(registry.runners().contains_key("@test:content/main"));
}

#[test]
fn runner_rejects_empty_duplicate_or_malformed_release_hash_lists() {
    let good = hash(b"first");
    for invalid in [
        serde_json::json!([]),
        serde_json::json!([good, good]),
        serde_json::json!(null),
        serde_json::json!(good),
        serde_json::json!([null]),
        serde_json::json!([42]),
        serde_json::json!([""]),
        serde_json::json!(["@test:content/native"]),
        serde_json::json!([good.trim_start_matches("sha256:")]),
        serde_json::json!([good.to_uppercase()]),
        serde_json::json!([format!("{good}0")]),
        serde_json::json!([format!("sha256:{}", "g".repeat(64))]),
    ] {
        load_plugin_source("@test", &content_source(invalid.clone()))
            .expect_err(&format!("must reject releases: {invalid}"));
    }
}

#[test]
fn runner_cannot_declare_both_systems_and_releases() {
    let source = content_source(serde_json::json!([hash(b"first")]));
    for systems in ["['gba']", "[]"] {
        let invalid = source.replace(
            "program: 'program'",
            &format!("program: 'program', systems: {systems}"),
        );
        let error = load_plugin_source("@test", &invalid).unwrap_err();
        assert!(
            error.to_string().contains("systems and releases"),
            "{error}"
        );
    }
}

#[test]
fn release_hash_matching_is_a_native_runner_contract() {
    let source = content_source(serde_json::json!([hash(b"first")]))
        .replace("program: 'program'", "command: 'component'");
    let error = load_plugin_source("@test", &source).unwrap_err();
    assert!(error.to_string().contains("native"), "{error}");
}

const SYSTEM_SOURCE: &str = "
    export const name = 'system';
    export const systems = {gba: {id: 'gba', title: 'Game Boy Advance'}};
    export const runners = {main: {
      id: '@test:system/main', program: 'program', systems: ['gba']
    }};
    export const discovery = {fileReleases: {gba: {
      id: '@test:system/gba', extensions: ['gba'], system: 'gba',
      runners: ['@test:system/main']
    }}};
    export const handlers = {'launch.prepare': input => ({
      command: input.program, args: [input.contentPath]
    })};
";

fn install(root: &Path, name: &str, source: &str) -> EnabledPackage {
    let package = root.join(format!("plugin-{name}"));
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    // Use a real native program. Admission in these tests starts at the
    // already-enabled package projection, just like installed_game_launch.
    let program = package.join("program");
    let native = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("true"))
        .find(|path| path.is_file())
        .expect("coreutils true must be on the test PATH");
    fs::copy(native, &program).unwrap();
    EnabledPackage {
        id: format!("@test:{name}"),
        package,
        files: BTreeMap::from([("program".into(), program)]),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
    }
}

fn setup(root: &Path) -> (korrid::config::ConfigSnapshot, PluginRegistry) {
    let registry = PluginRegistry::from_installed(vec![
        install(root, "system", SYSTEM_SOURCE),
        install(
            root,
            "content",
            &content_source(serde_json::json!([hash(b"first"), hash(b"second")])),
        ),
    ])
    .unwrap();
    let roms = root.join("roms");
    fs::create_dir_all(&roms).unwrap();
    fs::write(roms.join("first.gba"), b"first").unwrap();
    fs::write(roms.join("second.gba"), b"second").unwrap();
    fs::write(roms.join("unlisted.gba"), b"unlisted").unwrap();
    let report = DiscoveryCoordinator::new(root, root.join("private"))
        .with_registry_source(RegistrySource::Selected(Arc::new(registry.clone())))
        .add_location(&roms, &DiscoveryOptions::default())
        .unwrap();
    assert!(
        report.scan.diagnostics.is_empty(),
        "{:?}",
        report.scan.diagnostics
    );
    let loaded = ConfigSnapshotCoordinator::new(root).reload();
    assert!(loaded.diagnostic.is_none(), "{:?}", loaded.diagnostic);
    ((*loaded.snapshot).clone(), registry)
}

fn game_id(snapshot: &korrid::config::ConfigSnapshot, bytes: &[u8]) -> String {
    snapshot.releases[&hash(bytes)].game.0.clone()
}

#[test]
fn each_accepted_dump_offers_the_launcher_beside_the_system_runner() {
    let root = tempfile::tempdir().unwrap();
    let (snapshot, registry) = setup(root.path());
    for bytes in [b"first".as_slice(), b"second".as_slice()] {
        let id = game_id(&snapshot, bytes);
        let routes = game_routes::list(root.path(), &registry, &id).unwrap();
        assert!(matches!(routes.selection, GameRouteSelection::Choose));
        assert_eq!(
            routes
                .routes
                .iter()
                .map(|route| route.runner_id.as_str())
                .collect::<Vec<_>>(),
            ["@test:content/main", "@test:system/main"]
        );
        let launch = game_routes::selected_launch(
            root.path(),
            &registry,
            &SelectedGameLaunchRequest {
                game_id: id,
                runner_id: "@test:content/main".into(),
                overrides: None,
            },
        )
        .unwrap();
        let input: PluginLaunchInput = serde_json::from_str(&launch.command[3]).unwrap();
        let path = root
            .path()
            .join("roms")
            .join(format!("{}.gba", std::str::from_utf8(bytes).unwrap()));
        assert_eq!(input.content_path, path.display().to_string());
        let package = registry.installed_package("@test:content/main").unwrap();
        let source =
            SourceSnapshot::package_plugin(&package.package, &package.entry, &package.sources)
                .unwrap();
        let plan = evaluate_snapshot(&source, &input).unwrap();
        assert_eq!(plan.command, input.program);
        assert_eq!(plan.args, [input.content_path]);
    }
}

#[test]
fn one_accepted_dump_is_enough_when_the_other_dump_is_absent() {
    let root = tempfile::tempdir().unwrap();
    let (snapshot, registry) = setup(root.path());
    fs::remove_file(root.path().join("roms/second.gba")).unwrap();
    let routes = game_routes::list(root.path(), &registry, &game_id(&snapshot, b"first")).unwrap();
    assert!(routes
        .routes
        .iter()
        .any(|route| route.runner_id == "@test:content/main"));
}

#[test]
fn an_unlisted_hash_keeps_only_the_system_runner_and_refuses_the_content_launcher() {
    let root = tempfile::tempdir().unwrap();
    let (snapshot, registry) = setup(root.path());
    let id = game_id(&snapshot, b"unlisted");
    let routes = game_routes::list(root.path(), &registry, &id).unwrap();
    assert_eq!(routes.routes.len(), 1);
    assert_eq!(routes.routes[0].runner_id, "@test:system/main");
    let error = game_routes::selected_launch(
        root.path(),
        &registry,
        &SelectedGameLaunchRequest {
            game_id: id,
            runner_id: "@test:content/main".into(),
            overrides: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "LocalRouteUnavailable");
}

#[test]
fn a_missing_accepted_dump_keeps_its_diagnostic_when_another_release_can_play() {
    let root = tempfile::tempdir().unwrap();
    let (snapshot, registry) = setup(root.path());
    let id = game_id(&snapshot, b"first");
    let other_id = game_id(&snapshot, b"unlisted");
    // Put two discovered releases under one catalog game, using the existing
    // bidirectional game/releases links rather than creating another schema.
    let games_path = root.path().join("catalog/games.yaml");
    let releases_path = root.path().join("catalog/releases.yaml");
    let mut games: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&games_path).unwrap()).unwrap();
    let mut releases: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&releases_path).unwrap()).unwrap();
    games["games"]
        .as_mapping_mut()
        .unwrap()
        .remove(serde_yaml::Value::String(other_id));
    games["games"][&id]["releases"]
        .as_sequence_mut()
        .unwrap()
        .push(serde_yaml::Value::String(hash(b"unlisted")));
    releases["releases"][hash(b"unlisted")]["game"] = serde_yaml::Value::String(id.clone());
    fs::write(games_path, serde_yaml::to_string(&games).unwrap()).unwrap();
    fs::write(releases_path, serde_yaml::to_string(&releases).unwrap()).unwrap();
    fs::remove_file(root.path().join("roms/first.gba")).unwrap();

    let routes = game_routes::list(root.path(), &registry, &id).unwrap();
    assert_eq!(routes.routes.len(), 1);
    assert_eq!(routes.routes[0].runner_id, "@test:system/main");
    let error = game_routes::selected_launch(
        root.path(),
        &registry,
        &SelectedGameLaunchRequest {
            game_id: id,
            runner_id: "@test:content/main".into(),
            overrides: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "LocalRomMissing");
}

#[test]
fn an_accepted_hash_with_a_missing_file_is_not_playable() {
    let root = tempfile::tempdir().unwrap();
    let (snapshot, registry) = setup(root.path());
    fs::remove_file(root.path().join("roms/first.gba")).unwrap();
    let id = game_id(&snapshot, b"first");
    let error =
        resolver::linux_route_candidates(root.path(), &snapshot, &registry, &id).unwrap_err();
    assert_eq!(error.code, resolver::RouteDiagnosticCode::LocalRomMissing);
    let error = game_routes::selected_launch(
        root.path(),
        &registry,
        &SelectedGameLaunchRequest {
            game_id: id,
            runner_id: "@test:content/main".into(),
            overrides: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "LocalRomMissing");
}
