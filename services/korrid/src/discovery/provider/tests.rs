use super::*;
use crate::plugin_discovery::STEAM_INSTALLED_APPS;
use std::os::unix::fs::symlink;

const PLUGIN: &str = include_str!("../../../tests/fixtures/first-party/steam/plugin.ts");

fn file(text: &str) -> ReleaseDiscoveryFile {
    ReleaseDiscoveryFile {
        relative_path: "steamapps/appmanifest_401710.acf".into(),
        text: text.into(),
    }
}
fn manifest(fields: &str) -> String {
    format!(r#""AppState" {{ "appid" "401710" "StateFlags" "4" "name" "Flinthook" {fields} }}"#)
}

#[test]
fn real_steam_callback_ports_legacy_manifest_rules_without_io() {
    let source = SourceSnapshot::plugin(PLUGIN).unwrap();
    let found = discover(&source, "@korri:steam", file(&manifest(r#""type" "Game" "installdir" "Flinthook" "InstalledDepots" { "1" { "manifest" "2" } }"#))).unwrap();
    assert_eq!(found[0].release.title, "Flinthook");
    assert_eq!(found[0].target.provider_ref, "401710");
    for text in [
        manifest("").replace("\"4\"", "\"1026\""),
        manifest("").replace("401710", "401711"),
        manifest("").replace("Flinthook", "  "),
        manifest("").replace("\"name\" \"Flinthook\"", ""),
        "\"AppState\" {".into(),
        manifest(r#""type" "Tool""#),
        manifest(r#""type" "Proton""#),
        manifest(r#""type" "Steam Runtime""#),
        manifest(r#""type" "Steamworks Redistributables""#),
    ] {
        assert!(
            discover(&source, "@korri:steam", file(&text))
                .unwrap()
                .is_empty(),
            "{text}"
        );
    }
    let escaped = discover(
        &source,
        "@korri:steam",
        file(&manifest("").replace("Flinthook", r#"Flint \"hook\" \\ Game"#)),
    )
    .unwrap();
    assert_eq!(escaped[0].release.title, "Flint \"hook\" \\ Game");
}

#[test]
fn declaration_admits_only_the_closed_first_party_provider_and_requires_handler() {
    crate::plugin::load_plugin_source("@korri", PLUGIN).unwrap();
    assert!(crate::plugin::load_plugin_source("@thirdparty", PLUGIN).is_err());
    assert!(crate::plugin::load_plugin_source(
        "@korri",
        &PLUGIN.replace("@korri:steam/installed-apps", "@korri:steam/other")
    )
    .is_err());
    assert!(crate::plugin::load_plugin_source(
        "@korri",
        &PLUGIN.replace("\"discovery.discover\": discoverInstalledApps,", "")
    )
    .is_err());
    assert!(crate::plugin::load_plugin_source(
        "@korri",
        &PLUGIN.replace(
            "export const discovery = [",
            "export const discovery = [null,"
        )
    )
    .is_err());
    assert!(crate::plugin::load_plugin_source("@korri", &PLUGIN.replace("{ id: \"@korri:steam/installed-apps\", title: \"Steam installed apps\" },", "{ id: \"@korri:steam/installed-apps\", title: \"Steam installed apps\" }, { id: \"@korri:steam/installed-apps\", title: \"duplicate\" },")).is_err());
}

#[test]
fn host_rejects_foreign_refs_empty_titles_unknown_fields_and_unrelated_sources() {
    let source = SourceSnapshot::plugin(PLUGIN).unwrap();
    let expected =
        serde_json::to_value(discover(&source, "@korri:steam", file(&manifest(""))).unwrap())
            .unwrap();
    for (path, replacement) in [
        ("/0/target/provider", serde_json::json!("@other:steam")),
        ("/0/target/ref", serde_json::json!("not-numeric")),
        ("/0/target/ref", serde_json::json!("401711")),
        ("/0/release/title", serde_json::json!(" \t")),
        ("/0/release/system", serde_json::json!("other")),
        ("/0/release/id", serde_json::json!("other")),
        (
            "/0/source/relativePath",
            serde_json::json!("steamapps/appmanifest_1.acf"),
        ),
        ("/0/launch/use", serde_json::json!("@other:runner/run")),
    ] {
        let mut bad = expected.clone();
        *bad.pointer_mut(path).unwrap() = replacement;
        let source = SourceSnapshot::plugin(&format!("export const name = 'steam'; export const handlers = {{ 'discovery.discover': () => ({bad}) }};")).unwrap();
        assert!(
            discover(&source, "@korri:steam", file(&manifest(""))).is_err(),
            "{path}"
        );
    }
    let mut bad = expected.clone();
    bad[0]["unexpected"] = true.into();
    let source = SourceSnapshot::plugin(&format!("export const name = 'steam'; export const handlers = {{ 'discovery.discover': () => ({bad}) }};")).unwrap();
    assert!(discover(&source, "@korri:steam", file(&manifest(""))).is_err());
}

#[test]
fn files_are_bounded_regular_and_confined_to_the_steam_root() {
    let root = tempfile::tempdir().unwrap();
    let steamapps = root.path().join("steam/steamapps");
    fs::create_dir_all(&steamapps).unwrap();
    let directory = manifest_directory(root.path(), "steam/steamapps/appmanifest_*.acf").unwrap();
    fs::write(steamapps.join("appmanifest_401710.acf"), manifest("")).unwrap();
    assert!(read_manifest(&directory, "appmanifest_401710.acf").is_ok());
    let oversized = File::create(steamapps.join("appmanifest_2.acf")).unwrap();
    oversized.set_len(MAX_FILE_BYTES as u64 + 1).unwrap();
    assert!(read_manifest(&directory, "appmanifest_2.acf")
        .unwrap_err()
        .contains("65536"));
    fs::write(root.path().join("outside"), manifest("")).unwrap();
    symlink(
        root.path().join("outside"),
        steamapps.join("appmanifest_3.acf"),
    )
    .unwrap();
    assert!(read_manifest(&directory, "appmanifest_3.acf").is_err());
    fs::create_dir(steamapps.join("appmanifest_4.acf")).unwrap();
    assert!(read_manifest(&directory, "appmanifest_4.acf").is_err());
    let fifo = CString::new(steamapps.join("appmanifest_5.acf").to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert!(read_manifest(&directory, "appmanifest_5.acf").is_err());
    assert!(manifest_appid("appmanifest_not-an-app.acf").is_none());
    assert!(manifest_appid("appmanifest_1.acf/extra").is_none());
    fs::rename(
        root.path().join("steam"),
        root.path().join("original-steam"),
    )
    .unwrap();
    symlink(
        root.path().join("original-steam"),
        root.path().join("steam"),
    )
    .unwrap();
    assert!(manifest_directory(root.path(), "steam/steamapps/appmanifest_*.acf").is_err());
}

#[test]
fn scan_skips_oversized_files_reports_caps_and_ignores_disabled_plugins() {
    let root = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    let registry = crate::plugin_test_fixtures::steam(package.path());
    let steamapps = root.path().join("steam/steamapps");
    fs::create_dir_all(&steamapps).unwrap();
    File::create(steamapps.join("appmanifest_1.acf"))
        .unwrap()
        .set_len(MAX_FILE_BYTES as u64 + 1)
        .unwrap();
    let accounts = vec![AccountRoot {
        user: "default".into(),
        storage_id: "account-f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"
            .into(),
        root: root.path().into(),
    }];
    let mut diagnostics = Vec::new();
    assert!(
        scan(&accounts, &registry, std::iter::empty(), &mut diagnostics)[0]
            .observations
            .is_empty()
    );
    assert!(diagnostics.iter().any(|d| d.message.contains("65536")));
    for i in 2..=MAX_FILES + 1 {
        fs::write(steamapps.join(format!("appmanifest_{i}.acf")), "").unwrap();
    }
    diagnostics.clear();
    scan(&accounts, &registry, std::iter::empty(), &mut diagnostics);
    assert!(diagnostics.iter().any(|d| d.message.contains("4096")));
    let disabled = PluginRegistry::new(
        vec![crate::plugin::load_plugin_source("@korri", PLUGIN).unwrap()],
        vec![],
    )
    .unwrap();
    diagnostics.clear();
    assert!(scan(
        &accounts,
        &disabled,
        &[STEAM_INSTALLED_APPS.to_string()],
        &mut diagnostics
    )[0]
    .observations
    .is_empty());
    assert!(diagnostics.is_empty());
}

/// ADR 0083: an app the engine installed in the person's engine folder is
/// the same Steam provider's release, launched with the engine runner.
#[test]
fn engine_folder_installs_are_found_and_run_on_the_engine_runner() {
    let root = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    let registry = crate::plugin_test_fixtures::steam(package.path());
    let client = root.path().join("steam/steamapps");
    let engine = root.path().join("steam-engine/steam/steamapps");
    fs::create_dir_all(&client).unwrap();
    fs::create_dir_all(&engine).unwrap();
    fs::write(client.join("appmanifest_401710.acf"), manifest("")).unwrap();
    fs::write(
        engine.join("appmanifest_78000.acf"),
        r#""AppState" { "appid" "78000" "StateFlags" "4" "name" "Bejeweled 3" }"#,
    )
    .unwrap();
    let accounts = vec![AccountRoot {
        user: "default".into(),
        storage_id: "account-f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"
            .into(),
        root: root.path().into(),
    }];
    let mut diagnostics = Vec::new();
    let scans = scan(&accounts, &registry, std::iter::empty(), &mut diagnostics);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(scans.len(), 1);
    assert_eq!(scans[0].id, STEAM_INSTALLED_APPS);
    let runner = |appid: &str| {
        scans[0]
            .observations
            .iter()
            .find(|o| o.target.provider_ref == appid)
            .map(|o| o.launch.runner.clone())
    };
    assert_eq!(runner("401710").as_deref(), Some("@korri:steam/steam"));
    assert_eq!(runner("78000").as_deref(), Some("@korri:steam/engine"));
}

/// ADR 0083 and ADR 0045: the Steam plugin under another namespace, in its
/// derived-id shape, discovers both folders under `@other:steam`.
#[test]
fn a_steam_plugin_in_another_namespace_discovers_under_its_own_ids() {
    let root = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    let registry = crate::plugin_test_fixtures::steam_in(
        package.path(),
        "@other",
        crate::plugin_test_fixtures::DERIVED_STEAM_PLUGIN,
    );
    let client = root.path().join("steam/steamapps");
    let engine = root.path().join("steam-engine/steam/steamapps");
    fs::create_dir_all(&client).unwrap();
    fs::create_dir_all(&engine).unwrap();
    fs::write(client.join("appmanifest_401710.acf"), manifest("")).unwrap();
    fs::write(
        engine.join("appmanifest_78000.acf"),
        r#""AppState" { "appid" "78000" "StateFlags" "4" "name" "Bejeweled 3" }"#,
    )
    .unwrap();
    let accounts = vec![AccountRoot {
        user: "default".into(),
        storage_id: "account-f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"
            .into(),
        root: root.path().into(),
    }];
    let mut diagnostics = Vec::new();
    // A scanner korrid knew under `@korri:steam` is withdrawn: that plugin is gone.
    let known = [STEAM_INSTALLED_APPS.to_string()];
    let scans = scan(&accounts, &registry, &known, &mut diagnostics);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(scans.len(), 2);
    let withdrawn = scans
        .iter()
        .find(|scan| scan.id == STEAM_INSTALLED_APPS)
        .unwrap();
    assert!(withdrawn.observations.is_empty());
    let other = scans
        .iter()
        .find(|scan| scan.id == "@other:steam/installed-apps")
        .unwrap();
    let by_ref = |appid: &str| {
        other
            .observations
            .iter()
            .find(|o| o.target.provider_ref == appid)
            .unwrap()
    };
    assert_eq!(by_ref("401710").target.provider, "@other:steam");
    assert_eq!(by_ref("401710").launch.runner, "@other:steam/steam");
    assert_eq!(by_ref("78000").launch.runner, "@other:steam/engine");
}
