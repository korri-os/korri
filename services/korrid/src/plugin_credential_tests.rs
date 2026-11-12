//! A plugin's secret over the RPC (ADR 0034): the real handlers, the real
//! store and real files, in host mode and in brain mode.
use super::*;
use authorization::{AuthorizationContext, Principal};
use std::os::unix::fs::PermissionsExt;

const PLUGIN: &str = plugin_test_fixtures::ART_PLUGIN_ID;
const SECRET: &str = "plugin-credential-test-not-a-real-secret";
const ROW: &str = "plugin-credential:%40korri%3Asteamgriddb";
const ART_SOURCE: &str = "export const name = 'steamgriddb'; export const title = 'SteamGridDB'; export const handlers = {'game-assets.candidates': () => ({candidates: []})};";

fn art_registry(root: &std::path::Path) -> plugin::PluginRegistry {
    plugin_test_fixtures::installed_with_art(
        root,
        plugin_test_fixtures::art_package(
            root,
            ART_SOURCE,
            &["www.steamgriddb.com", "cdn2.steamgriddb.com"],
            Some("www.steamgriddb.com"),
        ),
    )
}

/// A device korrid (host mode) with the artwork plugin installed.
fn host_fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    config::test_fixtures::gba(root.path());
    let registry = art_registry(root.path());
    let host_config = root.path().join("host.toml");
    std::fs::write(&host_config, "label = \"secret-device\"\n").unwrap();
    let backend = Arc::new(host::control::InMemoryLaunchUnitBackend::default());
    let runtime = host::HostRuntime::from_paths_with_backend(
        &host_config,
        Some(root.path().into()),
        root.path().join("private"),
        backend.clone(),
    )
    .with_route_registry(root.path().into(), registry);
    runtime.claim_save_owner();
    Fixture {
        root,
        runtime,
        backend,
    }
}

fn brain_router(root: &std::path::Path, permission: SurfaceHostPermission) -> Router {
    config::test_fixtures::gba(root);
    let (mut state, _) = brain_app_state(
        TOKEN,
        ORIGIN,
        root,
        root.join("private"),
        plugin_policy::RegistrySource::Selected(Arc::new(art_registry(root))),
        config::snapshot::ConfigSnapshotCoordinator::new(root),
        discovery::FolderSelectionGrantStore::default(),
        None,
        None,
        None,
    );
    state.surface_host_access = Some(SurfaceHostAccess::new(TOKEN, ORIGIN, permission));
    surface_host_router_from_state(state)
}

fn set(plugin_id: &str, token: &str) -> Value {
    json!({"_tag":"system.settings.credential.set","payload":{"pluginId":plugin_id,"token":token}})
}

fn clear(plugin_id: &str) -> Value {
    json!({"_tag":"system.settings.credential.clear","payload":{"pluginId":plugin_id}})
}

fn plugin_of<'a>(snapshot: &'a Value, id: &str) -> &'a Value {
    snapshot["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|plugin| plugin["id"] == id)
        .unwrap_or_else(|| panic!("{id} is not in {snapshot}"))
}

fn mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// Every file under `root` except korrid's private folder: what a person, a
/// game or a plugin's native program can read.
fn readable_bytes(root: &std::path::Path, private: &std::path::Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path == private {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
            } else if let Ok(content) = std::fs::read(&path) {
                bytes.extend(content);
            }
        }
    }
    bytes
}

/// The whole life of one secret through one surface host router.
async fn set_read_and_clear(app: &Router, root: &std::path::Path) {
    let private = root.join("private");
    let before = snapshot(app).await;
    assert_eq!(plugin_of(&before, PLUGIN)["credential"], "NotConfigured");
    // A plugin that declares no secret has no such field, and no row.
    assert!(plugin_of(&before, "@korri:mgba")
        .get("credential")
        .is_none());
    let editable = before["editableSettingIds"].as_array().unwrap();
    assert!(editable.contains(&json!(ROW)), "{before}");
    assert_eq!(
        editable
            .iter()
            .filter(|id| id.as_str().unwrap().starts_with("plugin-credential:"))
            .count(),
        1
    );

    let stored = rpc(app, set(PLUGIN, &format!("  {SECRET}\n"))).await;
    assert_eq!(stored["outcome"]["_tag"], "Ok", "{stored}");
    assert_eq!(
        stored["outcome"]["payload"],
        json!({"status": "Configured"})
    );
    assert!(!stored.to_string().contains(SECRET));
    let folder = private.join(PLUGIN);
    assert_eq!(mode(&folder), 0o700);
    assert_eq!(mode(&folder.join("credential")), 0o600);
    assert_eq!(
        std::fs::read_to_string(folder.join("credential")).unwrap(),
        SECRET
    );

    let after = snapshot(app).await;
    assert_eq!(plugin_of(&after, PLUGIN)["credential"], "Configured");
    assert!(!after.to_string().contains(SECRET));
    // Not in device.yaml, the library or any other readable file.
    let readable = readable_bytes(root, &private);
    assert!(!String::from_utf8_lossy(&readable).contains(SECRET));

    for _ in 0..2 {
        let cleared = rpc(app, clear(PLUGIN)).await;
        assert_eq!(cleared["outcome"]["_tag"], "Ok", "{cleared}");
        assert_eq!(
            cleared["outcome"]["payload"],
            json!({"status": "NotConfigured"})
        );
    }
    assert!(!folder.join("credential").exists());
    assert_eq!(
        plugin_of(&snapshot(app).await, PLUGIN)["credential"],
        "NotConfigured"
    );
}

#[tokio::test]
async fn a_device_korrid_sets_and_clears_a_plugin_secret_and_answers_only_its_status() {
    let fixture = host_fixture();
    let (app, _) = fixture.routers(SurfaceHostPermission::Full);
    set_read_and_clear(&app, fixture.root.path()).await;
}

#[tokio::test]
async fn brain_mode_sets_and_clears_a_plugin_secret_the_same_way() {
    let root = tempfile::tempdir().unwrap();
    let app = brain_router(root.path(), SurfaceHostPermission::Full);
    set_read_and_clear(&app, root.path()).await;
}

#[tokio::test]
async fn a_refused_secret_changes_nothing_and_no_failure_repeats_it() {
    let fixture = host_fixture();
    let (app, _) = fixture.routers(SurfaceHostPermission::Full);
    let private = fixture.root.path().join("private");
    let kept = rpc(&app, set(PLUGIN, "kept-value")).await;
    assert_eq!(kept["outcome"]["_tag"], "Ok", "{kept}");
    let overlong = format!("{SECRET}{}", "x".repeat(256));
    for (plugin_id, token) in [
        // Empty, too long, or not sendable as a header.
        (PLUGIN, "  "),
        (PLUGIN, overlong.as_str()),
        (PLUGIN, "plugin-credential-test\nnot-a-real-secret"),
        // A plugin that declares no secret, and one that is not installed.
        ("@korri:mgba", SECRET),
        ("@gone:art", SECRET),
        // Not one plugin id: it never becomes a path.
        ("../identity", SECRET),
        ("@korri:../identity", SECRET),
        ("", SECRET),
    ] {
        let refused = rpc(&app, set(plugin_id, token)).await;
        assert_error(&refused, "SettingsInvalid");
        let text = refused.to_string();
        assert!(!text.contains(SECRET), "{text}");
        assert!(!text.contains("not-a-real-secret"), "{text}");
    }
    assert_eq!(
        std::fs::read_to_string(private.join(PLUGIN).join("credential")).unwrap(),
        "kept-value"
    );
    // Only the plugin's own folder was ever made for a secret.
    for unmade in ["@korri:mgba", "@gone:art", "identity/credential"] {
        assert!(!private.join(unmade).exists(), "{unmade}");
    }
    // Clear answers for any plugin id, because removing a plugin keeps its
    // secret; an id that is not a plugin id is refused.
    let gone = rpc(&app, clear("@gone:art")).await;
    assert_eq!(gone["outcome"]["payload"]["status"], "NotConfigured");
    assert_error(&rpc(&app, clear("../identity")).await, "SettingsInvalid");
    assert_eq!(
        std::fs::read_to_string(private.join(PLUGIN).join("credential")).unwrap(),
        "kept-value"
    );
}

#[tokio::test]
async fn a_shell_without_full_permission_sees_the_status_and_cannot_write() {
    let fixture = host_fixture();
    let private = fixture.root.path().join("private");
    plugin_secrets::PluginSecrets::new(&private)
        .set(PLUGIN, SECRET)
        .unwrap();
    for permission in [
        SurfaceHostPermission::LocalSessions,
        SurfaceHostPermission::ReadOnly,
    ] {
        let (app, _) = fixture.routers(permission);
        let current = snapshot(&app).await;
        assert_eq!(plugin_of(&current, PLUGIN)["credential"], "Configured");
        assert!(!current["editableSettingIds"]
            .as_array()
            .unwrap()
            .contains(&json!(ROW)));
        assert!(!current.to_string().contains(SECRET));
        for body in [set(PLUGIN, "replacement"), clear(PLUGIN)] {
            let (status, _) =
                request(&app, body, Some(&format!("Bearer {TOKEN}")), Some(ORIGIN)).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
        }
    }
    assert_eq!(
        std::fs::read_to_string(private.join(PLUGIN).join("credential")).unwrap(),
        SECRET
    );
}

#[tokio::test]
async fn the_persons_other_device_may_set_a_secret_and_nobody_elses_device_may() {
    let fixture = host_fixture();
    let private = fixture.root.path().join("private");
    let (lan, _) = app_states(fixture.runtime.clone());
    let owner = AuthorizationContext::Peer(Principal::OwnerDevice {
        device_public_key: "peer".into(),
        owner_statement: "verified upstream".into(),
        owner_public_key: "owner".into(),
    });
    for (body, status) in [
        (set(PLUGIN, SECRET), "Configured"),
        (clear(PLUGIN), "NotConfigured"),
        (set(PLUGIN, SECRET), "Configured"),
    ] {
        let response = dispatch(&lan, &owner, serde_json::from_value(body).unwrap())
            .await
            .unwrap();
        let response = serde_json::to_value(response).unwrap();
        assert_eq!(
            response["outcome"]["payload"]["status"], status,
            "{response}"
        );
        assert!(!response.to_string().contains(SECRET));
    }
    let pass = authorization::PersonPass {
        event_id: "pass".into(),
        event_json: "verified upstream".into(),
        expires_at: u64::MAX,
        scopes: vec![
            authorization::Scope::CatalogRead,
            authorization::Scope::StreamLaunch,
        ],
        person_public_key: "person".into(),
    };
    for principal in [
        Principal::Unknown {
            device_public_key: "peer".into(),
        },
        Principal::Guest {
            device_public_key: "peer".into(),
            owner_statement: "verified upstream".into(),
            person_pass: pass.clone(),
        },
        Principal::Household {
            device_public_key: "peer".into(),
            owner_statement: "verified upstream".into(),
            person_pass: pass,
        },
    ] {
        let context = AuthorizationContext::Peer(principal);
        for body in [set(PLUGIN, "theirs"), clear(PLUGIN)] {
            assert!(
                dispatch(&lan, &context, serde_json::from_value(body).unwrap())
                    .await
                    .is_err()
            );
        }
    }
    assert_eq!(
        std::fs::read_to_string(private.join(PLUGIN).join("credential")).unwrap(),
        SECRET
    );
}

#[test]
fn a_set_request_never_prints_its_token() {
    let request: RpcRequest = serde_json::from_value(set(PLUGIN, SECRET)).unwrap();
    let printed = format!("{request:?}");
    assert!(printed.contains(PLUGIN), "{printed}");
    assert!(!printed.contains(SECRET), "{printed}");
    // Both changes are security mutations, as the old pair was.
    assert!(authorization::is_security_mutation(&request));
    assert!(authorization::is_security_mutation(
        &serde_json::from_value(clear(PLUGIN)).unwrap()
    ));
}
