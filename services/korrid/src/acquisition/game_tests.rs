//! ADR 0013 end to end: a declared game whose release comes from an artifact,
//! from reconcile through routes, the local RPC queue, launch and removal.
use crate::*;
use axum::body::{to_bytes, Body};
use axum::http::Request;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path, time::Duration};
use tower::ServiceExt;

const CONTENT: &[u8] = b"rock and roll";
const BONUS: &[u8] = b"bonus stage";
const SECOND: &[u8] = b"second game";
const RUNTIME: &[u8] = b"runtime";
const NOW: &str = "2026-10-20T00:00:00Z";

fn sha(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

struct Fixture {
    root: tempfile::TempDir,
    registry: plugin::PluginRegistry,
    backend: Arc<host::control::InMemoryLaunchUnitBackend>,
    app: Router,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("plugin");
        fs::create_dir(&package).unwrap();
        // `second` names the shared `content` artifact and its own `bonus`.
        fs::write(
            package.join("plugin.ts"),
            format!(
                r#"
            export const name = 'rock';
            export const title = 'Rock';
            const file = (name: string, sha256: string) => [{{url: `https://example.com/${{name}}`, sha256}}];
            export const artifacts = {{
              content: {{id: '@test:rock/content', version: 'v1', title: 'Rock N Roll', files: file('data.pck', '{content}')}},
              bonus: {{id: '@test:rock/bonus', version: 'v1', title: 'Bonus', files: file('bonus.pck', '{bonus}')}},
              runtime: {{id: '@test:rock/runtime', version: 'v1', title: 'Runtime', files: file('runtime.bin', '{runtime}')}},
            }};
            export const runners = {{run: {{id: '@test:rock/run', program: 'run',
              releases: ['{content}', '{second}'], artifacts: ['@test:rock/runtime']}}}};
            export const games = {{
              rock: {{title: 'Rock N Roll', releases: {{v1: {{hashes: ['{content}'],
                artifacts: ['@test:rock/content'], file: {{artifact: '@test:rock/content', path: 'data.pck'}}}}}}}},
              second: {{title: 'Second', releases: {{v1: {{hashes: ['{second}'],
                artifacts: ['@test:rock/content', '@test:rock/bonus'], file: {{artifact: '@test:rock/bonus', path: 'bonus.pck'}}}}}}}},
            }};
            export const handlers = {{'launch.prepare': (input: any) => ({{command: input.program, args: [input.contentPath]}})}};
        "#,
                content = sha(CONTENT),
                bonus = sha(BONUS),
                second = sha(SECOND),
                runtime = sha(RUNTIME),
            ),
        )
        .unwrap();
        let registry =
            plugin::PluginRegistry::from_installed(vec![plugin_installation::EnabledPackage {
                id: "@test:rock".into(),
                package: package.clone(),
                entry: "plugin.ts".into(),
                sources: vec!["plugin.ts".into()],
                requires: vec![],
                files: BTreeMap::from([("run".into(), package.join("run"))]),
            }])
            .unwrap();
        // The library comes from the real reconcile pass, before any download.
        let initial = config::decode_config_documents("{}", "{}").unwrap();
        let change = discovery::reconcile::reconcile(
            &discovery::identify::ScanReport {
                declared_games: registry.declared_games().clone(),
                ..Default::default()
            },
            &initial,
            NOW,
        );
        fs::create_dir_all(root.path().join("catalog")).unwrap();
        fs::write(
            root.path().join("catalog/library.yaml"),
            change.library_yaml.unwrap(),
        )
        .unwrap();
        fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
        let config = root.path().join("host.toml");
        fs::write(&config, "label = \"artifact-games\"\ngames = []\n").unwrap();
        let backend = Arc::new(host::control::InMemoryLaunchUnitBackend::default());
        let runtime = host::HostRuntime::from_paths_with_backend(
            &config,
            Some(root.path().into()),
            root.path().join("private"),
            backend.clone(),
        )
        .with_route_registry(root.path().into(), registry.clone());
        let app = secure_host_routers(
            runtime,
            &root.path().join("private"),
            Some(PortalAccess::new(
                "artifact-games",
                "http://127.0.0.1:8099",
                PortalPermission::LocalSessions,
            )),
        )
        .0;
        Self {
            root,
            registry,
            backend,
            app,
        }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    async fn call(&self, tag: &str, payload: Value) -> Value {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer artifact-games")
                    .header("Origin", "http://127.0.0.1:8099")
                    .body(Body::from(
                        json!({"_tag":tag, "payload":payload}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        serde_json::from_slice(&body).unwrap()
    }

    /// The pinned URLs are public; this places the verified file where the
    /// download engine finds and re-verifies it, so no network is used.
    fn place_verified(&self, key: &str, bytes: &[u8]) {
        let downloads = self.path().join(format!("cache/rock/{key}/.downloads"));
        fs::create_dir_all(&downloads).unwrap();
        fs::write(
            downloads.join(sha(bytes).trim_start_matches("sha256:")),
            bytes,
        )
        .unwrap();
    }

    fn ready(&self, key: &str) -> std::path::PathBuf {
        self.path().join(format!("cache/rock/{key}/v1"))
    }

    async fn wait_ready(&self, game: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = self
                    .call("app.acquisition.status", json!({"gameId": game}))
                    .await;
                let artifacts = status["outcome"]["payload"]["artifacts"]
                    .as_array()
                    .unwrap()
                    .clone();
                if artifacts.iter().all(|a| a["state"]["_tag"] == "Ready") {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    }

    async fn launch(&self, game: &str) -> Value {
        let reserved = self
            .call("app.session.reserve", json!({"gameId": game}))
            .await;
        self.call(
            "app.session.start",
            json!({"gameId": game, "expectedLaunchId": reserved["outcome"]["payload"]["launchId"]}),
        )
        .await
    }
}

fn artifacts(status: &Value) -> Vec<(String, String, Option<bool>)> {
    status["outcome"]["payload"]["artifacts"]
        .as_array()
        .unwrap_or_else(|| panic!("{status}"))
        .iter()
        .map(|a| {
            (
                a["id"].as_str().unwrap().to_owned(),
                a["state"]["_tag"].as_str().unwrap().to_owned(),
                a.get("release").map(|flag| flag.as_bool().unwrap()),
            )
        })
        .collect()
}

#[tokio::test]
async fn an_artifact_game_shows_before_download_downloads_launches_and_removes() {
    let fixture = Fixture::new();
    let (snapshot, _) = config::settings::runner_choice_snapshot(fixture.path()).unwrap();
    assert!(snapshot.locations.is_empty());
    assert_eq!(
        snapshot.library["rock-n-roll"].releases[0]
            .identity
            .as_ref()
            .unwrap()
            .value(),
        sha(CONTENT)
    );

    // The route needs no system and no location: the runner matches the hash.
    let route = config::resolver::resolve_linux_route(
        fixture.path(),
        &snapshot,
        &fixture.registry,
        "rock-n-roll",
        None,
    )
    .unwrap();
    assert!(route.file_target.is_none() && route.provider_target.is_none());
    let target = route.artifact_target.clone().unwrap();
    assert_eq!(target.artifact_id, "@test:rock/content");
    assert_eq!(target.path, "data.pck");

    // Status merges the release's and the runner's artifacts, release first;
    // only the release artifact carries `release: true`.
    let status = fixture
        .call("app.acquisition.status", json!({"gameId":"rock-n-roll"}))
        .await;
    assert_eq!(
        artifacts(&status),
        [
            ("@test:rock/content".into(), "Missing".into(), Some(true)),
            ("@test:rock/runtime".into(), "Missing".into(), None),
        ]
    );
    let all = fixture.call("app.acquisition.status", json!({})).await;
    assert!(artifacts(&all)
        .iter()
        .all(|(_, _, release)| release.is_none()));

    let refused = fixture.launch("rock-n-roll").await;
    assert_eq!(refused["outcome"]["payload"]["code"], "ArtifactsNotReady");

    // Catalog and runner listings preflight every route before any download.
    // A game that still needs its files stays listable; only the session
    // start above refuses (RG353M regression: "The game is not downloaded
    // yet" from the catalog).
    let preflight = launcher::linux_plugin::launch_route(
        fixture.path(),
        &snapshot,
        &fixture.registry,
        &route,
        None,
        None,
    )
    .expect("listing preflight must not require downloaded artifacts");
    let planned: launcher::plugin_launch::PluginLaunchInput =
        serde_json::from_str(&preflight.command[3]).unwrap();
    assert_eq!(
        planned.content_path.as_deref(),
        Some(
            fixture
                .ready("content")
                .join("data.pck")
                .display()
                .to_string()
                .as_str()
        )
    );

    // Download without launching fetches the whole default route.
    fixture.place_verified("content", CONTENT);
    fixture.place_verified("runtime", RUNTIME);
    let queued = fixture
        .call("app.acquisition.download", json!({"gameId":"rock-n-roll"}))
        .await;
    assert_eq!(queued["outcome"]["_tag"], "Ok", "{queued}");
    assert_eq!(artifacts(&queued)[0].2, Some(true));
    fixture.wait_ready("rock-n-roll").await;
    assert_eq!(
        fs::read(fixture.ready("content").join("data.pck")).unwrap(),
        CONTENT
    );

    // contentPath is the ready directory joined with file.path, nothing else.
    let launch = launcher::linux_plugin::launch_route(
        fixture.path(),
        &snapshot,
        &fixture.registry,
        &route,
        None,
        None,
    )
    .unwrap();
    let input: launcher::plugin_launch::PluginLaunchInput =
        serde_json::from_str(&launch.command[3]).unwrap();
    assert_eq!(
        input.content_path.as_deref(),
        Some(
            fixture
                .ready("content")
                .join("data.pck")
                .display()
                .to_string()
                .as_str()
        )
    );
    assert!(input.provider_ref.is_none());
    // The runner still receives only its own artifacts.
    assert_eq!(
        input
            .artifacts
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["runtime"]
    );

    // The second game shares `content` and adds `bonus`.
    fixture.place_verified("bonus", BONUS);
    fixture
        .call("app.acquisition.download", json!({"gameId":"second"}))
        .await;
    fixture.wait_ready("second").await;

    let started = fixture.launch("rock-n-roll").await;
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    // A running game cannot lose its files.
    let running = fixture
        .call("app.acquisition.remove", json!({"gameId":"rock-n-roll"}))
        .await;
    assert_eq!(
        running["outcome"]["payload"]["code"], "ActiveSessionConflict",
        "{running}"
    );
    assert!(fixture.ready("content").is_dir());

    // Another game can be removed: its own artifact goes, but the artifact
    // the running game's release names and the runner artifact stay.
    let removed = fixture
        .call("app.acquisition.remove", json!({"gameId":"second"}))
        .await;
    assert_eq!(removed["outcome"]["_tag"], "Ok", "{removed}");
    assert!(!fixture.ready("bonus").exists());
    assert!(fixture.ready("content").is_dir());
    assert!(fixture.ready("runtime").is_dir());
    let states: BTreeMap<_, _> = artifacts(&removed)
        .into_iter()
        .map(|(id, state, release)| (id, (state, release)))
        .collect();
    assert_eq!(
        states["@test:rock/bonus"],
        ("Missing".to_owned(), Some(true))
    );
    assert_eq!(
        states["@test:rock/content"],
        ("Ready".to_owned(), Some(true))
    );
    assert_eq!(states["@test:rock/runtime"], ("Ready".to_owned(), None));
    // Saves and account roots are never touched.
    assert!(!fixture.path().join("users").exists());
}

#[tokio::test]
async fn remove_download_refuses_while_queued_and_keeps_runner_artifacts() {
    let fixture = Fixture::new();
    fixture.place_verified("runtime", RUNTIME);
    fixture
        .call(
            "app.acquisition.start",
            json!({"artifactIds":["@test:rock/runtime"]}),
        )
        .await;
    // Another writer holds `content`'s .lock, so its job waits before any
    // network use and stays observable.
    let content = fixture.registry.artifact("@test:rock/content").unwrap();
    let base = super::placement::prepare(fixture.path(), content).unwrap();
    let lock = super::placement::lock(&base).unwrap().unwrap();
    let queued = fixture
        .call("app.acquisition.download", json!({"gameId":"rock-n-roll"}))
        .await;
    assert!(artifacts(&queued)
        .iter()
        .any(|(id, state, _)| id == "@test:rock/content" && state != "Ready"));
    let marker = base.join(".downloads").join(format!(
        "{}.part",
        content.files[0].sha256.trim_start_matches("sha256:")
    ));
    assert!(marker.exists());
    let busy = fixture
        .call("app.acquisition.remove", json!({"gameId":"rock-n-roll"}))
        .await;
    assert_eq!(
        busy["outcome"]["payload"]["code"], "AcquisitionBusy",
        "{busy}"
    );
    let cancelled = fixture
        .call(
            "app.acquisition.cancel",
            json!({"artifactId":"@test:rock/content"}),
        )
        .await;
    assert_eq!(cancelled["outcome"]["_tag"], "Ok", "{cancelled}");
    // The other writer still owns the partial files; once it lets go, a
    // second Cancel deletes them.
    assert!(marker.exists());
    drop(lock);
    let again = fixture
        .call(
            "app.acquisition.cancel",
            json!({"artifactId":"@test:rock/content"}),
        )
        .await;
    assert_eq!(again["outcome"]["_tag"], "Ok", "{again}");
    assert!(
        !marker.exists(),
        "{again} {:?}",
        std::fs::read_dir(base.join(".downloads"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>()
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !fixture.ready("runtime").is_dir() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let removed = fixture
        .call("app.acquisition.remove", json!({"gameId":"rock-n-roll"}))
        .await;
    assert_eq!(removed["outcome"]["_tag"], "Ok", "{removed}");
    // The runner artifact is never removed, even when no other game needs it.
    assert!(fixture.ready("runtime").is_dir());
}

#[tokio::test]
async fn a_saved_file_location_is_tried_before_the_artifact_route() {
    let fixture = Fixture::new();
    let games = fixture.path().join("games");
    fs::create_dir(&games).unwrap();
    fs::write(games.join("data.pck"), CONTENT).unwrap();
    fs::write(
        fixture.path().join("device.yaml"),
        format!(
            "storage:\n  games:\n    root: {}\nlocations:\n  '{}':\n    - storage: games\n      path: data.pck\n",
            games.display(),
            sha(CONTENT)
        ),
    )
    .unwrap();
    let (snapshot, _) = config::settings::runner_choice_snapshot(fixture.path()).unwrap();
    let route = config::resolver::resolve_linux_route(
        fixture.path(),
        &snapshot,
        &fixture.registry,
        "rock-n-roll",
        None,
    )
    .unwrap();
    assert_eq!(route.file_target.as_ref().unwrap().path, "data.pck");
    assert!(route.artifact_target.is_none());
    // The file route needs only the runner's artifacts.
    let status = fixture
        .call("app.acquisition.status", json!({"gameId":"rock-n-roll"}))
        .await;
    assert_eq!(
        artifacts(&status),
        [("@test:rock/runtime".into(), "Missing".into(), None)]
    );
    // A missing saved file falls back to the artifact route.
    fs::remove_file(games.join("data.pck")).unwrap();
    let route = config::resolver::resolve_linux_route(
        fixture.path(),
        &snapshot,
        &fixture.registry,
        "rock-n-roll",
        None,
    )
    .unwrap();
    assert!(route.file_target.is_none());
    assert_eq!(
        route.artifact_target.unwrap().artifact_id,
        "@test:rock/content"
    );
}

#[tokio::test]
async fn remove_download_refuses_while_session_recovery_is_unresolved() {
    let fixture = Fixture::new();
    for (key, bytes) in [("content", CONTENT), ("bonus", BONUS), ("runtime", RUNTIME)] {
        fixture.place_verified(key, bytes);
    }
    for game in ["rock-n-roll", "second"] {
        fixture
            .call("app.acquisition.download", json!({"gameId": game}))
            .await;
        fixture.wait_ready(game).await;
    }
    let started = fixture.launch("rock-n-roll").await;
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    // korrid can no longer prove which game owns the session.
    fixture.backend.set_state_unavailable(true);
    let blocked = fixture
        .call("app.acquisition.remove", json!({"gameId":"second"}))
        .await;
    assert_eq!(
        blocked["outcome"]["payload"]["code"], "ActiveSessionConflict",
        "{blocked}"
    );
    assert!(fixture.ready("bonus").is_dir());
}
