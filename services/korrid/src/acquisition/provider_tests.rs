//! ADR 0016 end to end with a real loopback provider site and a fixture
//! provider plugin: search -> details -> acquire -> download published in
//! korrid's cache -> game, release and provider link written once -> route
//! to contentPath -> Remove download, the shared queue and restarts.
use super::{
    download::{DownloadEngine, STAGING_PREFIX},
    game::{GameAcquisition, GameContext},
    http_fixture::{HttpFixture, Route},
    placement, sleep,
    types::{
        AcquireArtifactRequest, AcquireJobState, AcquireJobStatus, DetailsRequest, SearchRequest,
    },
    AcquisitionCoordinator, ArtifactState,
};
use crate::{config, plugin::PluginRegistry, plugin_installation::EnabledPackage};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
const HOST: &str = "provider.test";
const PROVIDER: &str = "@test:shop";

/// A provider plugin in the shape the real ones use: every fact comes from a
/// response korrid fetched; the plugin only parses text.
const PROVIDER_SOURCE: &str = r#"
export const name = "shop";
export const providers = { "@test:shop": { id: "@test:shop", title: "Test Shop" } };
const base = (url) => url.replace(/\/[^/]*$/, "");
const claim = (site, game, tag) => ({
  _tag: tag, providerId: "@test:shop", id: game.id, title: game.title,
  url: site + "/game/" + game.id,
  artifact: { kind: "content", system: game.system, format: { id: game.system } },
});
export const handlers = {
  "claims.search": (input) => {
    if (input.responses.length === 0) {
      return { _tag: "HttpRequests", requests: [{ url: "SITE/search", query: { q: input.query } }] };
    }
    const response = input.responses[0];
    if (!response.ok) throw new Error("search failed");
    const site = base(response.url);
    return JSON.parse(response.text).map((game) => claim(site, game, "ProviderClaim"));
  },
  "claims.details": (input) => {
    if (input.responses.length === 0) {
      return { _tag: "HttpRequests", requests: [{ url: "SITE/game/" + input.id }] };
    }
    const response = input.responses[0];
    const site = base(base(response.url));
    return { ...claim(site, JSON.parse(response.text), "ProviderClaimDetails"), description: "A game." };
  },
  "artifact.resolve-download": (input) => {
    if (input.responses.length === 0) {
      return { _tag: "HttpRequests", requests: [{ url: input.candidateUrl }] };
    }
    const response = input.responses[0];
    const game = JSON.parse(response.text);
    const site = base(base(response.url));
    if (game.state === "gone") {
      return { _tag: "FailedDownload", providerId: input.providerId, reason: "not-found", message: "gone at " + site };
    }
    if (game.state === "page") {
      return { _tag: "NonFinalDownload", providerId: input.providerId, reason: "interstitial", url: site };
    }
    return {
      _tag: "FinalDownload", providerId: input.providerId, url: site + game.file,
      ...(game.filename ? { filename: game.filename } : {}),
      ...(game.form ? { method: "POST", body: game.form } : {}),
    };
  },
};
"#;

/// The system and a runner for it. `PINNED` adds an ADR 0012/0013 artifact
/// so the queue can be shared with a pinned download.
fn core_source(pinned: Option<(&str, &str)>) -> String {
    // With a pinned artifact the plugin also declares an ADR 0013 game whose
    // content file is that artifact's file.
    let artifacts = pinned.map_or(String::new(), |(url, sha256)| {
        format!("export const artifacts = {{ big: {{ id: '@test:core/big', version: 'v1', files: [{{ url: '{url}', sha256: '{sha256}' }}] }} }};
export const games = {{ big: {{ title: 'Big Game', releases: {{ v1: {{ hashes: ['{sha256}'],
  artifacts: ['@test:core/big'], file: {{ artifact: '@test:core/big', path: 'big.bin' }} }} }} }} }};")
    });
    format!(
        r#"
export const name = "core";
export const systems = {{
  gba: {{ id: "gba", title: "Game Boy Advance", content: {{ extensions: ["gba"] }} }},
  gb: {{ id: "gb", title: "Game Boy", content: {{ extensions: ["gb"] }} }},
  nes: {{ id: "nes", title: "NES", content: {{ extensions: ["nes"],
    header: [{{ size: 16, bytes: {{ offset: 0, hex: "4e45531a" }} }}] }} }},
  psx: {{ id: "psx", title: "PlayStation", content: {{ extensions: ["m3u", "cue", "bin", "chd"],
    layout: ["{{title}}.{{ext}}"] }} }},
  md: {{ id: "md", title: "Mega Drive", content: {{ extensions: ["md", "gen", "smd", "bin"] }} }},
  sat: {{ id: "sat", title: "Saturn", content: {{ extensions: ["cue", "bin"],
    layout: ["{{title}} (Disc {{disc}}).{{ext}}"] }} }},
  nativecart: {{ id: "nativecart", title: "Native container", content: {{ extensions: ["cartz"],
    evidence: {{ bytes: [{{ offset: 0, hex: "504b0304" }}] }} }} }},
}};
export const runners = {{ run: {{ id: "@test:core/run", program: "run", systems: ["gba", "nes", "psx", "md", "sat", "nativecart"] }} }};
export const handlers = {{ "launch.prepare": (input: any) => ({{ command: input.program, args: [input.contentPath] }}) }};
{artifacts}
"#
    )
}

const ROM: &[u8] = b"\x00\x01 a lawful homebrew rom image";
const TITLE: &str = "2048 Deluxe";

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn package(root: &Path, name: &str, source: &str, hosts: Vec<String>) -> EnabledPackage {
    let package = root.join(format!("package-{name}"));
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    fs::write(package.join("manifest.json"),serde_json::json!({"entry":"plugin.ts","sources":["plugin.ts"],"files":{"run":package.join("run")}}).to_string()).unwrap();
    EnabledPackage {
        id: format!("@test:{name}"),
        package: package.clone(),
        files: BTreeMap::from([("run".into(), package.join("run"))]),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: vec![],
        hosts,
    }
}

fn site() -> HttpFixture {
    let server = HttpFixture::start([]);
    let games = [
        ("2048", TITLE, "gba", "/files/2048.gba", "ok"),
        ("copy", "2048 Again", "gba", "/files/copy.gba", "ok"),
        ("tetris", "Tetris", "gb", "/files/tetris.gb", "ok"),
        ("nosys", "Unknown", "unknown", "", "ok"),
        ("gone", "Gone", "gba", "", "gone"),
        ("page", "Page", "gba", "", "page"),
        ("slow", "Slow", "gba", "/files/slow.gba", "ok"),
        ("web", "Web", "gba", "/files/web.gba", "ok"),
        ("exe", "Program", "gba", "/files/game.exe", "ok"),
    ];
    server.route(
        "/search?q=2048",
        Route::ok(
            "application/json",
            format!(
                "[{}]",
                games
                    .iter()
                    .map(|(id, title, system, _, _)| format!(
                        r#"{{"id":"{id}","title":"{title}","system":"{system}"}}"#
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
    );
    for (id, title, system, file, state) in games {
        server.route(
            &format!("/game/{id}"),
            Route::ok(
                "application/json",
                format!(
                    r#"{{"id":"{id}","title":"{title}","system":"{system}","file":"{file}","state":"{state}"}}"#
                ),
            ),
        );
    }
    server.route(
        "/files/2048.gba",
        Route::ok("application/octet-stream", ROM),
    );
    server.route(
        "/files/copy.gba",
        Route::ok("application/octet-stream", ROM),
    );
    server.route(
        "/files/tetris.gb",
        Route::ok("application/octet-stream", ROM),
    );
    server.route("/files/web.gba", Route::ok("text/html", "<html></html>"));
    server.route(
        "/files/game.exe",
        Route::ok("application/octet-stream", ROM),
    );
    server.route(
        "/files/slow.gba",
        Route::ok("application/octet-stream", vec![1u8; 64 * 1024])
            .paced(1024, Duration::from_millis(50)),
    );
    server
}

struct World {
    _dirs: Vec<tempfile::TempDir>,
    root: PathBuf,
    server: HttpFixture,
    context: GameContext,
    coordinator: AcquisitionCoordinator,
    blocks: Arc<sleep::InMemoryBlocks>,
    /// The device's local `/rpc` with the portal capability.
    app: axum::Router,
    runtime: crate::host::HostRuntime,
    backend: Arc<crate::host::control::InMemoryLaunchUnitBackend>,
}

fn engine() -> DownloadEngine {
    DownloadEngine::for_test(Duration::from_secs(5)).with_name(HOST, "127.0.0.1".parse().unwrap())
}

fn world_with(server: HttpFixture, pinned: Option<(&str, &str)>) -> World {
    world_gated(server, pinned, GameAcquisition::with_engine(engine()))
}

fn world_gated(server: HttpFixture, pinned: Option<(&str, &str)>, games: GameAcquisition) -> World {
    let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let root = dirs[0].path().to_path_buf();
    let private = dirs[1].path().to_path_buf();
    let source = PROVIDER_SOURCE.replace("SITE", server.url(HOST, "").trim_end_matches('/'));
    let registry = PluginRegistry::from_installed(vec![
        package(dirs[2].path(), "shop", &source, vec![HOST.into()]),
        package(dirs[2].path(), "core", &core_source(pinned), vec![]),
    ])
    .unwrap();
    // ADR 0017: the device korrid (host mode, installed library root) runs
    // discovery and the store on its own library.
    let config = root.join("host.toml");
    fs::write(&config, "label = \"device\"\ngames = []\n").unwrap();
    let backend = Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default());
    let runtime = crate::host::HostRuntime::from_paths_with_backend(
        &config,
        Some(root.clone()),
        private.clone(),
        backend.clone(),
    )
    .with_route_registry(root.clone(), registry.clone())
    .with_library_discovery();
    let discovery = runtime.library_discovery().unwrap().clone();
    discovery.wait_until_idle(Duration::from_secs(10));
    let blocks = Arc::new(sleep::InMemoryBlocks::default());
    let coordinator = AcquisitionCoordinator {
        engine: DownloadEngine::for_test(Duration::from_secs(5)),
        sleep: sleep::SleepBlock::InMemory(blocks.clone()),
        games,
        ..Default::default()
    };
    let app = crate::secure_host_routers_with_acquisition(
        runtime.clone(),
        &private,
        Some(crate::PortalAccess::new(
            "device",
            "http://127.0.0.1:8099",
            crate::PortalPermission::Full,
        )),
        coordinator.clone(),
    )
    .0;
    World {
        _dirs: dirs,
        root: root.clone(),
        server,
        context: GameContext {
            root,
            private_root: private.clone(),
            registry,
            discovery,
        },
        coordinator,
        blocks,
        app,
        runtime,
        backend,
    }
}

fn world() -> World {
    world_with(site(), None)
}

impl World {
    async fn call(&self, tag: &str, payload: serde_json::Value) -> serde_json::Value {
        use axum::body::{to_bytes, Body};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .extension(axum::extract::ConnectInfo(
                        "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
                    ))
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer device")
                    .header("Origin", "http://127.0.0.1:8099")
                    .body(Body::from(
                        serde_json::json!({"_tag": tag, "payload": payload}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{tag}");
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }
    async fn launch(&self, game_id: &str) -> serde_json::Value {
        let reserved = self
            .call(
                "app.session.reserve",
                serde_json::json!({"gameId": game_id}),
            )
            .await;
        self.call(
            "app.session.start",
            serde_json::json!({"gameId": game_id, "expectedLaunchId": reserved["outcome"]["payload"]["launchId"]}),
        )
        .await
    }
    fn api(&self) -> &GameAcquisition {
        &self.coordinator.games
    }
    async fn search(&self) {
        self.api()
            .search(
                &self.context.registry,
                &SearchRequest {
                    query: "2048".into(),
                    provider_ids: vec![PROVIDER.into()],
                },
            )
            .await
            .unwrap();
    }
    /// Serve one more game and hold its claim through details.
    async fn offer(&self, id: &str, system: &str, file: &str, bytes: Vec<u8>) {
        self.server.route(
            &format!("/game/{id}"),
            Route::ok(
                "application/json",
                format!(r#"{{"id":"{id}","title":"Offered","system":"{system}","file":"{file}"}}"#),
            ),
        );
        self.server
            .route(file, Route::ok("application/octet-stream", bytes));
        self.api()
            .details(
                &self.context.registry,
                &DetailsRequest {
                    provider_id: PROVIDER.into(),
                    id: id.into(),
                },
            )
            .await
            .unwrap();
    }
    async fn get(&self, id: &str) -> AcquireJobStatus {
        self.finish(self.acquire(id).await.unwrap()).await
    }
    fn route(&self, game_id: &str) -> Option<PathBuf> {
        config::resolver::resolve_linux_route(
            &self.root,
            &self.snapshot(),
            &self.context.registry,
            game_id,
            None,
        )
        .ok()
        .and_then(|route| route.cache_target)
    }
    async fn acquire(&self, id: &str) -> Result<AcquireJobStatus, crate::RpcFailure> {
        self.coordinator.acquire(&self.context, &request(id)).await
    }
    async fn finish(&self, job: AcquireJobStatus) -> AcquireJobStatus {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let status = self.api().status(&job.job_id).unwrap();
                if !matches!(
                    status.state,
                    AcquireJobState::Queued
                        | AcquireJobState::Resolving
                        | AcquireJobState::Downloading { .. }
                        | AcquireJobState::Unpacking
                        | AcquireJobState::Placing
                ) {
                    return status;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap()
    }
    fn cache(&self) -> PathBuf {
        self.root.join("cache/shop/shop")
    }
    fn downloads(&self) -> usize {
        self.server
            .received()
            .iter()
            .filter(|r| r.path.starts_with("/files/"))
            .count()
    }
    fn snapshot(&self) -> config::ConfigSnapshot {
        self.context.discovery.published().unwrap()
    }
}

fn request(id: &str) -> AcquireArtifactRequest {
    AcquireArtifactRequest {
        provider_id: PROVIDER.into(),
        id: id.into(),
        file_name: None,
    }
}

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn imported(status: &AcquireJobStatus) -> String {
    match &status.state {
        AcquireJobState::Imported { game_id } => game_id.clone(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_is_published_in_the_cache_and_recorded_once_with_the_claim_title() {
    let world = world();
    let api = world.api();
    let providers = api.providers(&world.context.registry).providers;
    assert_eq!(
        (
            providers[0].id.as_str(),
            providers[0].title.as_str(),
            providers[0].plugin_id.as_str()
        ),
        (PROVIDER, "Test Shop", "@test:shop")
    );
    world.search().await;
    let details = api
        .details(
            &world.context.registry,
            &DetailsRequest {
                provider_id: PROVIDER.into(),
                id: "2048".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(details.description.as_deref(), Some("A game."));

    let started = world.acquire("2048").await.unwrap();
    assert!(matches!(started.state, AcquireJobState::Queued));
    let done = world.finish(started).await;
    let game_id = imported(&done);
    assert_eq!(done.file_name.as_deref(), Some("2048.gba"));
    assert_eq!(done.message, None);
    // Q1: cache/<plugin>/<provider>/<sha256 hex>/<URL basename>.
    let hex = sha_hex(ROM);
    let ready = world.cache().join(&hex);
    assert_eq!(fs::read(ready.join("2048.gba")).unwrap(), ROM);
    assert_eq!(names(&ready), ["2048.gba"]);
    assert!(names(&world.cache().join(".downloads")).is_empty());

    // Q2: game (claim title), release (sha256, claim system), provider link.
    let snapshot = world.snapshot();
    let game = &snapshot.library[&game_id];
    assert_eq!(game.title.as_deref(), Some(TITLE));
    assert_eq!(game.releases.len(), 1);
    assert_eq!(game.releases[0].system.as_ref().unwrap().0, "gba");
    assert_eq!(
        game.releases[0].identity.as_ref().unwrap().value(),
        format!("sha256:{hex}")
    );
    let link = &snapshot.provider_links["@test:shop/2048"];
    assert_eq!(
        (link.provider.0.as_str(), link.playable.0.as_str()),
        (PROVIDER, game_id.as_str())
    );
    assert_eq!(link.refs[0].value.0, "2048");
    assert!(snapshot.locations.is_empty(), "no saved location");
    assert_eq!(
        snapshot.game_identity(&game_id),
        Some(crate::GameIdentity::Provider(crate::GameProviderIdentity {
            provider: PROVIDER.into(),
            provider_ref: "2048".into(),
        }))
    );

    // Identical bytes from another claim: one cache directory, one release,
    // the same game, a second provider link. Nothing else is rewritten.
    let copy = world.finish(world.acquire("copy").await.unwrap()).await;
    assert_eq!(imported(&copy), game_id);
    assert_eq!(
        names(&world.cache()),
        [".downloads".to_owned(), hex.clone()]
    );
    let after = world.snapshot();
    assert_eq!(after.library.len(), 1);
    assert_eq!(after.library[&game_id].releases.len(), 1);
    assert_eq!(after.library[&game_id].title.as_deref(), Some(TITLE));
    assert_eq!(after.provider_links["@test:shop/copy"].playable.0, game_id);

    // A reconcile pass keeps the records: no location is needed.
    world.context.discovery.rescan();
    world
        .context
        .discovery
        .wait_until_idle(Duration::from_secs(10));
    assert_eq!(world.snapshot(), after);

    // Already in the library: Get answers Imported, no second download.
    assert_eq!(world.downloads(), 2);
    assert_eq!(imported(&world.acquire("2048").await.unwrap()), game_id);
    assert_eq!(world.downloads(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_route_reaches_the_download_by_hash_and_passes_it_as_content_path() {
    let world = world();
    world.search().await;
    let game_id = imported(&world.finish(world.acquire("2048").await.unwrap()).await);
    let snapshot = world.snapshot();
    let route = config::resolver::resolve_linux_route(
        &world.root,
        &snapshot,
        &world.context.registry,
        &game_id,
        None,
    )
    .unwrap();
    let download = world.cache().join(sha_hex(ROM)).join("2048.gba");
    assert_eq!(route.cache_target.as_ref(), Some(&download));
    assert!(
        route.file_target.is_none()
            && route.provider_target.is_none()
            && route.artifact_target.is_none()
    );
    assert_eq!(route.runner_id, "@test:core/run");
    let launch = crate::launcher::linux_plugin::launch_route(
        &world.root,
        &snapshot,
        &world.context.registry,
        &route,
        None,
        None,
        PERSON,
    )
    .unwrap();
    let input: crate::launcher::plugin_launch::PluginLaunchInput =
        serde_json::from_str(&launch.command[3]).unwrap();
    assert_eq!(
        input.content_path.as_deref(),
        Some(download.display().to_string().as_str())
    );
    assert!(input.provider_ref.is_none());
    let account = crate::saves::account_root(&world.root, PERSON).unwrap();
    assert_eq!(input.account_root, account.display().to_string());
    assert_eq!(
        input.save_root,
        account
            .join("saves")
            .join(crate::host::play_log::encode_uri_component(&game_id))
            .display()
            .to_string()
    );
    assert!(
        !world.root.join("users").exists(),
        "route inspection must not create account files"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_download_keeps_the_game_in_history_and_get_downloads_into_it_again() {
    let world = world();
    world.search().await;
    let game_id = imported(&world.finish(world.acquire("2048").await.unwrap()).await);
    let ready = world.cache().join(sha_hex(ROM));
    let removal = |world: &World| {
        super::game_artifacts_for_removal(&world.root, &world.context.registry, &game_id).unwrap()
    };
    world
        .coordinator
        .remove(
            &world.root,
            &world.context.registry,
            &game_id,
            &removal(&world),
        )
        .await
        .unwrap();
    assert!(!ready.exists());
    let snapshot = world.snapshot();
    // Q4: the game and release stay as history; the route is unavailable.
    assert_eq!(snapshot.library[&game_id].releases.len(), 1);
    assert_eq!(
        snapshot.provider_links["@test:shop/2048"].playable.0,
        game_id
    );
    assert!(config::resolver::resolve_linux_route(
        &world.root,
        &snapshot,
        &world.context.registry,
        &game_id,
        None
    )
    .is_err());

    // Q6: Get from the store downloads again into the same game.
    let again = world.finish(world.acquire("2048").await.unwrap()).await;
    assert_eq!(imported(&again), game_id);
    assert!(ready.join("2048.gba").is_file());
    assert_eq!(world.downloads(), 2);

    // While a job for this game works, Remove download asks for Cancel.
    placement::discard_work(&ready).unwrap();
    world.server.route(
        "/files/2048.gba",
        Route::ok("application/octet-stream", ROM.repeat(2048))
            .paced(1024, Duration::from_millis(50)),
    );
    let slow = world.acquire("2048").await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !matches!(world.api().status(&slow.job_id).unwrap().state, AcquireJobState::Downloading { bytes_done, .. } if bytes_done > 0) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let busy = world
        .coordinator
        .remove(
            &world.root,
            &world.context.registry,
            &game_id,
            &removal(&world),
        )
        .await
        .unwrap_err();
    assert_eq!(busy.code, "AcquisitionBusy");
    world.api().cancel(&slow.job_id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_downloads_wait_their_turn_behind_a_pinned_download_and_cancel_leaves_the_queue() {
    // A pinned ADR 0012 artifact served slowly from the same loopback site.
    let server = site();
    let big = vec![7u8; 64 * 1024];
    server.route(
        "/pinned/big.bin",
        Route::ok("application/octet-stream", big.clone()).paced(4096, Duration::from_millis(40)),
    );
    let url = format!("http://{}/pinned/big.bin", server.address);
    let sha256 = format!("sha256:{}", sha_hex(&big));
    let world = world_with(server, Some((&url, &sha256)));
    world.search().await;

    let pinned = ["@test:core/big".to_owned()];
    world
        .coordinator
        .start(
            &world.root,
            &world.root.join("private"),
            &world.context.registry,
            &pinned,
        )
        .unwrap();
    let state = |world: &World| {
        world
            .coordinator
            .status(&world.root, &world.context.registry, Some(&pinned))
            .unwrap()
            .artifacts[0]
            .state
            .clone()
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        while !matches!(state(&world), ArtifactState::Downloading { .. }) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    // Behind the pinned download: no provider request yet, state Queued.
    let first = world.acquire("2048").await.unwrap();
    let cancelled = world.acquire("copy").await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    for job in [&first, &cancelled] {
        assert!(matches!(
            world.api().status(&job.job_id).unwrap().state,
            AcquireJobState::Queued
        ));
    }
    assert!(!world
        .server
        .received()
        .iter()
        .any(|r| r.path.starts_with("/game/")));
    // Cancel while queued leaves the line; the other jobs continue.
    let gone = world.api().cancel(&cancelled.job_id).await.unwrap();
    assert!(matches!(gone.state, AcquireJobState::Cancelled));

    let done = world.finish(first).await;
    imported(&done);
    assert!(matches!(state(&world), ArtifactState::Ready { .. }));
    // The download was resolved only after the pinned download finished.
    let order: Vec<_> = world
        .server
        .received()
        .into_iter()
        .map(|r| r.path)
        .filter(|path| path.starts_with("/pinned/") || path.starts_with("/game/2048"))
        .collect();
    assert_eq!(order, ["/pinned/big.bin", "/game/2048"]);
    assert!(!world
        .server
        .received()
        .iter()
        .any(|r| r.path == "/game/copy"));
    // The queue held the sleep block and releases it when it empties.
    tokio::time::timeout(Duration::from_secs(5), async {
        while world.blocks.held.load(Ordering::SeqCst) != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(world.blocks.taken.load(Ordering::SeqCst) >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_during_download_leaves_no_bytes() {
    let world = world();
    world.search().await;
    let job = world.acquire("slow").await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !matches!(world.api().status(&job.job_id).unwrap().state, AcquireJobState::Downloading { bytes_done, .. } if bytes_done > 0) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // A live job answers a repeated Get.
    assert_eq!(world.acquire("slow").await.unwrap().job_id, job.job_id);
    let cancelled = world.api().cancel(&job.job_id).await.unwrap();
    assert!(
        matches!(cancelled.state, AcquireJobState::Cancelled),
        "{cancelled:?}"
    );
    assert!(names(&world.cache().join(".downloads")).is_empty());
    assert_eq!(names(&world.cache()), [".downloads"]);
    assert!(world.snapshot().library.is_empty());
    assert!(matches!(
        world.api().cancel(&job.job_id).await.unwrap().state,
        AcquireJobState::Cancelled
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn refusals_happen_before_any_download_and_failures_are_plain() {
    let world = world();
    let api = world.api();
    let registry = &world.context.registry;
    for ids in [vec![], vec![PROVIDER.to_owned(), PROVIDER.to_owned()]] {
        let error = api
            .search(
                registry,
                &SearchRequest {
                    query: "2048".into(),
                    provider_ids: ids,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "AcquisitionCaller");
    }
    let unknown = api
        .search(
            registry,
            &SearchRequest {
                query: "2048".into(),
                provider_ids: vec!["@test:core".into()],
            },
        )
        .await
        .unwrap_err();
    assert_eq!(unknown.code, "AcquisitionUnknownProvider");
    assert_eq!(
        world.acquire("2048").await.unwrap_err().code,
        "AcquisitionUnknownClaim"
    );
    world.search().await;
    assert_eq!(
        world.acquire("nosys").await.unwrap_err().code,
        "AcquisitionSystemUnknown"
    );
    let mut bad = request("2048");
    bad.file_name = Some("../x.gba".into());
    assert_eq!(
        world
            .coordinator
            .acquire(&world.context, &bad)
            .await
            .unwrap_err()
            .code,
        "AcquisitionCaller"
    );
    assert_eq!(world.downloads(), 0);

    // Neither a system nor an archive extension: refused by name, never
    // downloaded.
    let exe = world.finish(world.acquire("exe").await.unwrap()).await;
    assert!(
        matches!(&exe.state, AcquireJobState::Failed { reason } if reason.message.contains("not a game for this system")),
        "{exe:?}"
    );
    assert_eq!(world.downloads(), 0);
    for (id, code) in [
        ("gone", "DownloadResolutionFailed"),
        ("page", "DownloadNotFinal"),
        ("web", "AcquisitionFailed"),
    ] {
        let done = world.finish(world.acquire(id).await.unwrap()).await;
        let AcquireJobState::Failed { reason } = &done.state else {
            panic!("{id}: {done:?}");
        };
        assert_eq!(reason.code, code, "{id}");
        assert_eq!(done.message.as_deref(), Some(reason.message.as_str()));
        for secret in ["http", HOST, "/files", "127.0.0.1", "gone at"] {
            assert!(!reason.message.contains(secret), "{id}: {}", reason.message);
        }
        let retry = world.acquire(id).await.unwrap();
        assert_ne!(retry.job_id, done.job_id);
        world.finish(retry).await;
    }
    assert!(world.snapshot().library.is_empty());
    assert_eq!(
        api.status("no-such-job").unwrap_err().code,
        "AcquisitionUnknownJob"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_drops_jobs_cleans_staging_and_get_answers_play_through_the_link() {
    let world = world();
    world.search().await;
    let game_id = imported(&world.finish(world.acquire("2048").await.unwrap()).await);
    let staging = placement::game_staging(&world.root, "@test:shop", PROVIDER).unwrap();
    assert_eq!(staging, world.cache().join(".downloads"));
    fs::write(
        staging.join(format!("{STAGING_PREFIX}interrupted")),
        b"partial",
    )
    .unwrap();
    fs::create_dir(world.cache().join(".tmp-interrupted")).unwrap();

    // A new process: fresh claims and jobs, startup recovery.
    let restarted = AcquisitionCoordinator {
        games: GameAcquisition::with_engine(engine()),
        ..Default::default()
    };
    // A folder published just before a crash, with no library record yet:
    // recovery reports it and deletes nothing.
    let unrecorded = world.cache().join("b".repeat(64));
    fs::create_dir(&unrecorded).unwrap();
    assert_eq!(
        restarted
            .games
            .recover(&world.root, &world.context.registry)
            .unwrap(),
        [(PROVIDER.to_owned(), "b".repeat(64))]
    );
    assert!(unrecorded.is_dir());
    restarted
        .recover(
            &world.root,
            &world.root.join("private"),
            &world.context.registry,
        )
        .unwrap();
    assert!(names(&staging).is_empty());
    assert!(!world.cache().join(".tmp-interrupted").exists());
    assert!(world.cache().join(sha_hex(ROM)).join("2048.gba").is_file());
    assert_eq!(
        restarted.games.status("any").unwrap_err().code,
        "AcquisitionUnknownJob"
    );
    // Get after the restart answers Play through the provider link, with no
    // claim in memory and no network.
    let requests = world.server.received().len();
    let job = restarted
        .acquire(&world.context, &request("2048"))
        .await
        .unwrap();
    assert_eq!(imported(&job), game_id);
    assert_eq!(world.server.received().len(), requests);
    // A claim never held and never acquired still needs search or details.
    assert_eq!(
        restarted
            .acquire(&world.context, &request("copy"))
            .await
            .unwrap_err()
            .code,
        "AcquisitionUnknownClaim"
    );
}

/// The device's local RPC refuses Remove download while the game runs.
#[tokio::test(flavor = "multi_thread")]
async fn remove_download_refuses_while_the_game_runs() {
    let world = world();
    world.search().await;
    let game_id = imported(&world.finish(world.acquire("2048").await.unwrap()).await);
    world.runtime.claim_save_owner();
    let started = world.launch(&game_id).await;
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    let refused = world
        .call(
            "app.acquisition.remove",
            serde_json::json!({"gameId": game_id}),
        )
        .await;
    assert_eq!(
        refused["outcome"]["payload"]["code"], "ActiveSessionConflict",
        "{refused}"
    );
    assert!(world.cache().join(sha_hex(ROM)).join("2048.gba").is_file());
}

/// A link that names game Y never reports Imported{Y} for bytes game X
/// already holds: the link moves to X, the game that has the release.
#[tokio::test(flavor = "multi_thread")]
async fn a_link_moves_to_the_game_that_already_holds_the_bytes() {
    let world = world();
    let other = b"\x00\x02 another homebrew rom".to_vec();
    world.server.route(
        "/files/tetris.gb",
        Route::ok("application/octet-stream", other),
    );
    world.search().await;
    let holder = imported(&world.finish(world.acquire("2048").await.unwrap()).await);
    let linked = imported(&world.finish(world.acquire("tetris").await.unwrap()).await);
    assert_ne!(holder, linked);
    // The provider now serves the 2048 bytes for tetris; its old download goes.
    world.server.route(
        "/files/tetris.gb",
        Route::ok("application/octet-stream", ROM),
    );
    let removal =
        super::game_artifacts_for_removal(&world.root, &world.context.registry, &linked).unwrap();
    world
        .coordinator
        .remove(&world.root, &world.context.registry, &linked, &removal)
        .await
        .unwrap();
    let again = world.finish(world.acquire("tetris").await.unwrap()).await;
    assert_eq!(imported(&again), holder);
    let snapshot = world.snapshot();
    assert_eq!(
        snapshot.provider_links["@test:shop/tetris"].playable.0,
        holder
    );
    assert_eq!(snapshot.library[&holder].releases.len(), 1);
    // The other game keeps its own release as history.
    assert_eq!(snapshot.library[&linked].releases.len(), 1);
}

/// ADR 0017 end to end over the device's own RPC: the store finds a game,
/// downloads it and the same process launches it, with no restart.
#[tokio::test(flavor = "multi_thread")]
async fn the_device_finds_downloads_and_launches_a_store_game_over_its_rpc() {
    let world = world();
    let providers = world
        .call("app.acquisition.providers", serde_json::json!({}))
        .await;
    assert_eq!(
        providers["outcome"]["payload"]["providers"][0]["id"], PROVIDER,
        "{providers}"
    );
    let search = world
        .call(
            "app.acquisition.search",
            serde_json::json!({"query": "2048", "providerIds": [PROVIDER]}),
        )
        .await;
    assert_eq!(
        search["outcome"]["payload"]["claims"][0]["title"], TITLE,
        "{search}"
    );
    let started = world
        .call(
            "app.acquisition.acquire",
            serde_json::json!({"providerId": PROVIDER, "id": "2048"}),
        )
        .await;
    let job = started["outcome"]["payload"]["jobId"]
        .as_str()
        .unwrap()
        .to_owned();
    let done = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let status = world
                .call(
                    "app.acquisition.acquire-status",
                    serde_json::json!({"jobId": job}),
                )
                .await;
            if status["outcome"]["payload"]["state"]["_tag"] == "Imported" {
                break status;
            }
            assert_ne!(
                status["outcome"]["payload"]["state"]["_tag"], "Failed",
                "{status}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let game_id = done["outcome"]["payload"]["state"]["gameId"]
        .as_str()
        .unwrap();
    // Acquired content must take the same owner gate as file and package
    // games, on every public fresh-start path. Claiming later needs no restart.
    for owned in [false, true] {
        if owned {
            world.runtime.claim_save_owner();
        }
        for method in [
            "app.session.start",
            "app.local-games.launch.selected",
            "app.session.prepare",
        ] {
            let launched = match method {
                "app.session.start" => world.launch(game_id).await,
                "app.local-games.launch.selected" => {
                    world
                        .call(
                            method,
                            serde_json::json!({"gameId":game_id,"runnerId":"@test:core/run"}),
                        )
                        .await
                }
                _ => {
                    world
                        .call(method, serde_json::json!({"gameId":game_id}))
                        .await
                }
            };
            if !owned {
                assert_eq!(
                    launched["outcome"]["payload"]["code"], "DeviceNeedsOwner",
                    "{launched}"
                );
                assert_eq!(world.backend.launch_count(), 0);
                assert!(!world.root.join("users").exists());
            } else {
                assert_eq!(launched["outcome"]["_tag"], "Ok", "{launched}");
                let payload = &launched["outcome"]["payload"];
                let launch_id = &payload["session"]["launchId"];
                let stopped = world
                    .call(
                        "app.session.end",
                        serde_json::json!({"expectedLaunchId":launch_id}),
                    )
                    .await;
                assert_eq!(stopped["outcome"]["_tag"], "Ok", "{stopped}");
            }
        }
    }
    assert_eq!(world.backend.launch_count(), 3);
}

/// ADR 0017: the device pairs a folder and rescans its own library, and the
/// game discovery found is launchable by the same process.
#[tokio::test(flavor = "multi_thread")]
async fn the_device_pairs_and_rescans_its_library_and_launches_what_it_found() {
    let world = world();
    let folder = tempfile::tempdir().unwrap();
    let rom = b"\x00\x03 found in a folder".to_vec();
    fs::write(folder.path().join("Found.gba"), &rom).unwrap();
    let receipt = world
        .context
        .discovery
        .grants()
        .issue_approved_path(folder.path())
        .unwrap()
        .token;
    let paired = world
        .call(
            "app.library.folders.add",
            serde_json::json!({"receipt": receipt, "system": "gba"}),
        )
        .await;
    assert_eq!(paired["outcome"]["_tag"], "Ok", "{paired}");
    let idle = || async {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let snapshot = world
                    .call("app.library.folders.snapshot", serde_json::json!({}))
                    .await;
                if snapshot["outcome"]["payload"]["state"]["_tag"] == "Idle" {
                    break snapshot;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap()
    };
    let snapshot = idle().await;
    assert_eq!(
        snapshot["outcome"]["payload"]["folders"][0]["system"], "gba",
        "{snapshot}"
    );
    let key = format!("sha256:{}", sha_hex(&rom));
    let library = world.snapshot();
    let (game_id, _) = library
        .library
        .iter()
        .find(|(_, game)| {
            game.releases
                .iter()
                .any(|r| r.identity.as_ref().is_some_and(|i| i.value() == key))
        })
        .expect("discovery recorded the found game");
    world.runtime.claim_save_owner();
    let launched = world.launch(game_id).await;
    assert_eq!(launched["outcome"]["_tag"], "Ok", "{launched}");
    let rescan = world
        .call("app.library.folders.rescan", serde_json::json!({}))
        .await;
    assert_eq!(rescan["outcome"]["_tag"], "Ok", "{rescan}");
    idle().await;
}

/// ADR 0017 with ADR 0013: a declared-artifact game is in the device's
/// library after the startup scan, before its download.
#[tokio::test(flavor = "multi_thread")]
async fn a_declared_artifact_game_appears_in_the_device_library() {
    let big = b"big content".to_vec();
    let sha256 = format!("sha256:{}", sha_hex(&big));
    let world = world_with(site(), Some(("https://example.com/big.bin", &sha256)));
    let library = world.snapshot();
    let game = library
        .library
        .values()
        .find(|game| game.title.as_deref() == Some("Big Game"))
        .expect("the declared game is in the library");
    assert_eq!(game.releases[0].identity.as_ref().unwrap().value(), sha256);
    assert!(library.locations.is_empty());
}

/// ADR 0005: acquisition and discovery identify the payload, not its header.
#[tokio::test(flavor = "multi_thread")]
async fn acquired_headered_and_headerless_dumps_join_the_discovered_release() {
    let world = world();
    let payload = vec![42; 128 * 1024];
    let mut headered = vec![0; 16];
    headered[..4].copy_from_slice(b"NES\x1a");
    headered.extend_from_slice(&payload);
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("Headered.nes"), &headered).unwrap();
    fs::write(folder.path().join("Plain.nes"), &payload).unwrap();
    let rules = world.context.registry.content_rules("nes").unwrap();
    let mut cache = crate::discovery::identify::HashCache::default();
    let identity = crate::discovery::identify::identity_of(
        &folder.path().join("Headered.nes"),
        rules,
        &mut cache,
    )
    .unwrap();
    assert_eq!(identity.value(), format!("sha256:{}", sha_hex(&payload)));
    assert_eq!(
        crate::discovery::identify::identity_of(
            &folder.path().join("Plain.nes"),
            rules,
            &mut cache,
        )
        .unwrap(),
        identity,
    );

    let mut game_id = None;
    for (id, bytes) in [("headered", &headered), ("plain", &payload)] {
        world.server.route(
            &format!("/game/{id}"),
            Route::ok(
                "application/json",
                format!(
                    r#"{{"id":"{id}","title":"NES Game","system":"nes","file":"/files/{id}.nes"}}"#,
                ),
            ),
        );
        world.server.route(
            &format!("/files/{id}.nes"),
            Route::ok("application/octet-stream", bytes.clone()),
        );
        world
            .api()
            .details(
                &world.context.registry,
                &DetailsRequest {
                    provider_id: PROVIDER.into(),
                    id: id.into(),
                },
            )
            .await
            .unwrap();
        let acquired = imported(&world.finish(world.acquire(id).await.unwrap()).await);
        if let Some(previous) = &game_id {
            assert_eq!(&acquired, previous);
        }
        let snapshot = world.snapshot();
        let releases = &snapshot.library[&acquired].releases;
        assert_eq!(releases.len(), 1);
        assert_eq!(
            releases[0].identity.as_ref().unwrap().value(),
            identity.value()
        );
        game_id = Some(acquired);
    }
    let game_id = game_id.unwrap();
    let ready = world.cache().join(sha_hex(&payload));
    assert_eq!(names(&ready), ["headered.nes"]);
    assert_eq!(fs::read(ready.join("headered.nes")).unwrap(), headered);
    assert_eq!(
        names(&world.cache()),
        [".downloads".to_owned(), sha_hex(&payload)]
    );
    let route = config::resolver::resolve_linux_route(
        &world.root,
        &world.snapshot(),
        &world.context.registry,
        &game_id,
        None,
    )
    .unwrap();
    assert_eq!(route.cache_target, Some(ready.join("headered.nes")));

    let restarted = AcquisitionCoordinator::default();
    assert!(restarted
        .games
        .recover(&world.root, &world.context.registry)
        .unwrap()
        .is_empty());
    let requests = world.server.received().len();
    assert_eq!(
        imported(
            &restarted
                .acquire(&world.context, &request("headered"))
                .await
                .unwrap()
        ),
        game_id
    );
    assert_eq!(world.server.received().len(), requests);

    // A real discovery scan adds both dumps as locations of that release.
    let receipt = world
        .context
        .discovery
        .grants()
        .issue_approved_path(folder.path())
        .unwrap()
        .token;
    let paired = world
        .call(
            "app.library.folders.add",
            serde_json::json!({"receipt": receipt, "system": "nes"}),
        )
        .await;
    assert_eq!(paired["outcome"]["_tag"], "Ok", "{paired}");
    world
        .context
        .discovery
        .wait_until_idle(Duration::from_secs(20));
    let snapshot = world.snapshot();
    assert_eq!(snapshot.library.len(), 1);
    assert_eq!(snapshot.library[&game_id].releases.len(), 1);
    assert_eq!(snapshot.locations[identity.value()].len(), 2);
}

// ADR 0030: archives and multi-file games.

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn seven_z(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).unwrap();
    for (name, data) in entries {
        writer
            .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(*data))
            .unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn rar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = rars::Builder::new(rars::ArchiveVersion::Rar50);
    for (name, data) in entries {
        builder
            .add_bytes(name.as_bytes().to_vec(), data.to_vec(), None, None)
            .unwrap();
    }
    builder.to_bytes().unwrap()
}

/// Discovery's identity of the same files in a plain folder, the way a
/// paired source would give it.
fn discovered(world: &World, system: &str, files: &[(&str, &[u8])], primary: &str) -> String {
    let folder = tempfile::tempdir().unwrap();
    for (name, data) in files {
        let path = folder.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }
    crate::discovery::identify::identity_of(
        &folder.path().join(primary),
        world.context.registry.content_rules(system).unwrap(),
        &mut crate::discovery::identify::HashCache::default(),
    )
    .unwrap()
    .value()
    .to_owned()
}

fn failed_with(status: &AcquireJobStatus, copy: &str) {
    assert!(
        matches!(&status.state, AcquireJobState::Failed { reason } if reason.code == "AcquisitionFailed" && reason.message == copy),
        "{status:?}"
    );
}

const NOT_A_GAME: &str = "The downloaded file is not a game for this system.";
const UNPACK: &str = "The downloaded files could not be unpacked.";

#[tokio::test(flavor = "multi_thread")]
async fn a_byte_identified_native_container_keeps_its_bytes_and_routes_as_one_file() {
    let world = world();
    // A native application package owns its startup marker and module layout.
    // Extracting it would leave no .cartz file for discovery or the runner.
    let cart = zip(&[
        ("game/.startup", b"src/main.py"),
        ("game/src/main.py", b"print('original test fixture')"),
        ("game/src/module.py", b"VALUE = 1"),
    ]);
    world
        .offer("native", "nativecart", "/files/game.cartz", cart.clone())
        .await;
    let game = imported(&world.get("native").await);
    let route = world.route(&game).unwrap();
    assert_eq!(route.file_name().unwrap(), "game.cartz");
    assert_eq!(fs::read(&route).unwrap(), cart);
    assert_eq!(names(route.parent().unwrap()), ["game.cartz"]);
    assert_eq!(
        world.snapshot().library[&game].releases[0]
            .identity
            .as_ref()
            .unwrap()
            .value(),
        format!("sha256:{}", sha_hex(&cart))
    );

    // A transport ZIP still unpacks, even when its lead matches the system's
    // byte evidence. The selected content is the inner native package.
    world
        .offer(
            "transport",
            "nativecart",
            "/files/package.zip",
            zip(&[("game.cartz", &cart)]),
        )
        .await;
    assert_eq!(imported(&world.get("transport").await), game);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_native_container_does_not_bypass_archive_entry_validation() {
    let world = world();
    world
        .offer(
            "unsafe-native",
            "nativecart",
            "/files/unsafe.cartz",
            zip(&[("game/main.txt", b"original fixture"), ("../escape", ROM)]),
        )
        .await;
    failed_with(&world.get("unsafe-native").await, UNPACK);
    assert_eq!(names(&world.cache()), [".downloads"]);
    assert!(!world.cache().join("escape").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_native_filename_with_nonmatching_byte_evidence_still_unpacks_transport() {
    let world = world();
    let cart = zip(&[("game/main.txt", b"original native content")]);
    world
        .offer(
            "different-container",
            "nativecart",
            "/files/transport.cartz",
            seven_z(&[("actual.cartz", &cart)]),
        )
        .await;
    let game = imported(&world.get("different-container").await);
    let route = world.route(&game).unwrap();
    assert_eq!(route.file_name().unwrap(), "actual.cartz");
    assert_eq!(fs::read(route).unwrap(), cart);
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_magic_still_wins_over_an_extension_without_byte_evidence() {
    let world = world();
    world
        .offer(
            "disguised",
            "gba",
            "/files/disguised.gba",
            zip(&[("actual.gba", ROM)]),
        )
        .await;
    let game = imported(&world.get("disguised").await);
    let route = world.route(&game).unwrap();
    assert_eq!(route.file_name().unwrap(), "actual.gba");
    assert_eq!(fs::read(route).unwrap(), ROM);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_zipped_rom_is_unpacked_identified_and_routed_and_joins_the_plain_copy() {
    let world = world();
    world.search().await;
    let plain = imported(&world.get("2048").await);
    world
        .offer(
            "zipped",
            "gba",
            "/files/2048-usa.zip",
            zip(&[("2048 (USA).gba", ROM), ("readme.txt", b"hi")]),
        )
        .await;
    let done = world.get("zipped").await;
    // The same ROM bytes are one release: the zipped copy joins the game and
    // reuses the ready folder the plain download made.
    assert_eq!(imported(&done), plain);
    assert_eq!(done.file_name.as_deref(), Some("2048-usa.zip"));
    let hex = sha_hex(ROM);
    assert_eq!(
        names(&world.cache()),
        [".downloads".to_owned(), hex.clone()]
    );
    assert!(
        names(&world.cache().join(".downloads")).is_empty(),
        "the archive is deleted"
    );
    assert_eq!(
        world.snapshot().provider_links["@test:shop/zipped"]
            .playable
            .0,
        plain
    );

    // On a fresh device the zip publishes its whole content.
    let fresh = world_with(site(), None);
    fresh
        .offer(
            "zipped",
            "gba",
            "/files/2048-usa.zip",
            zip(&[("2048 (USA).gba", ROM), ("readme.txt", b"hi")]),
        )
        .await;
    let game = imported(&fresh.get("zipped").await);
    let ready = fresh.cache().join(&hex);
    assert_eq!(names(&ready), ["2048 (USA).gba", "readme.txt"]);
    assert_eq!(
        fresh.snapshot().library[&game].releases[0]
            .identity
            .as_ref()
            .unwrap()
            .value(),
        format!("sha256:{hex}")
    );
    assert_eq!(fresh.route(&game), Some(ready.join("2048 (USA).gba")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_7z_disc_under_a_zip_name_is_one_parts_release_routed_to_its_cue() {
    let world = world();
    let cue: &[u8] = b"FILE \"Game (Track 1).bin\" BINARY\n TRACK 01 MODE2/2352\nFILE \"Game (Track 2).bin\" BINARY\n TRACK 02 AUDIO\n";
    let files: [(&str, &[u8]); 3] = [
        ("Game/Game.cue", cue),
        ("Game/Game (Track 1).bin", b"data track"),
        ("Game/Game (Track 2).bin", b"audio track"),
    ];
    // romhustler sends 7z bytes under a .zip name: the bytes decide.
    world
        .offer("disc", "psx", "/files/disc.zip", seven_z(&files))
        .await;
    let game = imported(&world.get("disc").await);
    let key = discovered(&world, "psx", &files, "Game/Game.cue");
    let release = &world.snapshot().library[&game].releases[0];
    assert!(matches!(
        release.identity.as_ref().unwrap(),
        config::library::LibraryIdentity::Parts { value, parts } if *value == key && parts.len() == 2
    ));
    let ready = world.cache().join(key.strip_prefix("sha256:").unwrap());
    // The system's own layout "{title}.{ext}" matches no subfolder; the
    // appended default layout accepts the archive's folder.
    assert_eq!(world.route(&game), Some(ready.join("Game/Game.cue")));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_m3u_set_in_a_rar_routes_to_the_playlist() {
    let world = world();
    let files: [(&str, &[u8]); 5] = [
        ("Game.m3u", b"Game (Disc 1).cue\nGame (Disc 2).cue\n"),
        ("Game (Disc 1).cue", b"FILE \"Game (Disc 1).bin\" BINARY\n"),
        ("Game (Disc 1).bin", b"first disc"),
        ("Game (Disc 2).cue", b"FILE \"Game (Disc 2).bin\" BINARY\n"),
        ("Game (Disc 2).bin", b"second disc"),
    ];
    world
        .offer("set", "psx", "/files/set.rar", rar(&files))
        .await;
    let game = imported(&world.get("set").await);
    let key = discovered(&world, "psx", &files, "Game.m3u");
    assert_eq!(
        world.snapshot().library[&game].releases[0]
            .identity
            .as_ref()
            .unwrap()
            .value(),
        key
    );
    let ready = world.cache().join(key.strip_prefix("sha256:").unwrap());
    assert_eq!(world.route(&game), Some(ready.join("Game.m3u")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plain_chd_gets_its_header_identity_and_a_40_digit_ready_folder() {
    let world = world();
    let mut chd = vec![0; 124];
    chd[..8].copy_from_slice(b"MComprHD");
    chd[8..12].copy_from_slice(&124u32.to_be_bytes());
    chd[12..16].copy_from_slice(&5u32.to_be_bytes());
    chd[0x40..0x54].fill(0x12);
    world.offer("chd", "psx", "/files/Game.chd", chd).await;
    let game = imported(&world.get("chd").await);
    let sha1 = "12".repeat(20);
    let release = &world.snapshot().library[&game].releases[0];
    assert!(matches!(
        release.identity.as_ref().unwrap(),
        config::library::LibraryIdentity::Chd { value } if *value == format!("sha1:{sha1}")
    ));
    let ready = world.cache().join(&sha1);
    assert_eq!(names(&ready), ["Game.chd"]);
    assert_eq!(world.route(&game), Some(ready.join("Game.chd")));

    // Recovery reports a 40-digit folder that no release names.
    let orphan = "ab".repeat(20);
    fs::create_dir(world.cache().join(&orphan)).unwrap();
    assert_eq!(
        GameAcquisition::default()
            .recover(&world.root, &world.context.registry)
            .unwrap(),
        [(PROVIDER.to_owned(), orphan)]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_that_do_not_give_exactly_one_game_leave_nothing() {
    let world = world();
    let mut cases: Vec<(&str, &str, &str, Vec<u8>, &str)> = vec![
        // Two games.
        (
            "two",
            "gba",
            "/files/two.zip",
            zip(&[("A.gba", ROM), ("B.gba", b"\x00\x02 other")]),
            NOT_A_GAME,
        ),
        // ADR 0030: README.md beside a Mega Drive ROM is a second unit.
        (
            "sonic",
            "md",
            "/files/sonic.zip",
            zip(&[("Sonic (USA).md", ROM), ("README.md", b"# notes")]),
            NOT_A_GAME,
        ),
        // Two discs with no .m3u stay two units.
        (
            "discs",
            "psx",
            "/files/discs.7z",
            seven_z(&[
                ("Game (Disc 1).cue", b"FILE \"Game (Disc 1).bin\" BINARY\n"),
                ("Game (Disc 1).bin", b"first"),
                ("Game (Disc 2).cue", b"FILE \"Game (Disc 2).bin\" BINARY\n"),
                ("Game (Disc 2).bin", b"second"),
            ]),
            NOT_A_GAME,
        ),
        // A lone cue has no tracks.
        (
            "cue",
            "psx",
            "/files/Game.cue",
            b"FILE \"Game.bin\" BINARY\n".to_vec(),
            NOT_A_GAME,
        ),
        // A nested archive is not a game.
        (
            "nested",
            "gba",
            "/files/nested.zip",
            zip(&[("inner.zip", &zip(&[("A.gba", ROM)]))]),
            NOT_A_GAME,
        ),
        // An archive name with bytes in no archive format.
        ("fake", "gba", "/files/fake.zip", ROM.to_vec(), NOT_A_GAME),
        // An unsafe entry refuses the whole archive.
        (
            "escape",
            "gba",
            "/files/escape.zip",
            zip(&[("A.gba", ROM), ("../escape.gba", ROM)]),
            UNPACK,
        ),
    ];
    for (id, system, file, bytes, _) in &mut cases {
        world.offer(id, system, file, std::mem::take(bytes)).await;
    }
    for (id, _, _, _, copy) in &cases {
        let done = world.get(id).await;
        failed_with(&done, copy);
    }
    assert_eq!(world.downloads(), cases.len(), "each was downloaded once");
    assert!(world.snapshot().library.is_empty());
    assert_eq!(
        names(&world.cache()),
        [".downloads"],
        "no ready or work folder"
    );
    assert!(names(&world.cache().join(".downloads")).is_empty());
    assert!(!world.root.join("cache/shop/escape.gba").exists());
}

/// ADR 0030 Changes 2 and 3 with the WoWROMs shape: an opaque token URL, the
/// name in `filename`, and the game only for a form POST.
#[tokio::test(flavor = "multi_thread")]
async fn a_form_post_download_is_named_by_filename_not_by_its_token_url() {
    let world = world();
    let archive = zip(&[("Super Mario World (USA).gba", ROM)]);
    for (id, filename) in [
        ("named", r#","filename":"Super Mario World (USA).zip""#),
        ("token", ""),
    ] {
        world.server.route(
            &format!("/game/{id}"),
            Route::ok(
                "application/json",
                format!(r#"{{"id":"{id}","title":"Super Mario World","system":"gba","file":"/dl/7bf4e0908e024c5"{filename},"form":{{"emuid":"1","id":"42","file":"Super Mario World (USA).zip"}}}}"#),
            ),
        );
        world
            .api()
            .details(
                &world.context.registry,
                &DetailsRequest {
                    provider_id: PROVIDER.into(),
                    id: id.into(),
                },
            )
            .await
            .unwrap();
    }
    world
        .server
        .route("/dl/7bf4e0908e024c5", Route::ok("application/zip", archive));

    let done = world.get("named").await;
    imported(&done);
    assert_eq!(
        done.file_name.as_deref(),
        Some("Super Mario World (USA).zip")
    );
    let posts: Vec<_> = world
        .server
        .received()
        .into_iter()
        .filter(|r| r.path.starts_with("/dl/"))
        .collect();
    assert_eq!(posts.len(), 1, "one attempt");
    assert_eq!(posts[0].method, "POST");
    assert_eq!(
        String::from_utf8_lossy(&posts[0].body),
        "emuid=1&file=Super+Mario+World+%28USA%29.zip&id=42"
    );
    assert_eq!(
        names(&world.cache().join(sha_hex(ROM))),
        ["Super Mario World (USA).gba"]
    );
    // N1: with no filename, the token is the name. It has no extension, so
    // the download runs and the zip bytes decide (decision U2).
    let token = world.get("token").await;
    imported(&token);
    assert_eq!(token.file_name.as_deref(), Some("7bf4e0908e024c5"));
}

/// ADR 0030 Change 1c: a store job reports Unpacking while korrid's reader
/// runs, stays live, and can be cancelled there: the unit stops, and the work
/// folder and the archive go. ADR 0012 artifacts keep "Unpacking cannot be
/// cancelled" (`tests::unpacking_is_observable_and_cannot_be_cancelled`).
#[tokio::test(flavor = "multi_thread")]
async fn an_unpacking_store_job_is_live_and_cancel_leaves_nothing() {
    /// Releases the hold on every exit, also a panic, so the blocking task
    /// cannot keep the runtime alive (review P2-2).
    struct Release(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let gate = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let release = Release(gate.clone());
    let world = world_gated(
        site(),
        None,
        GameAcquisition::with_engine(engine()).holding_unpack(gate.clone()),
    );
    world
        .offer("zipped", "gba", "/files/zipped.zip", zip(&[("A.gba", ROM)]))
        .await;
    let job = world.acquire("zipped").await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !matches!(
            world.api().status(&job.job_id).unwrap().state,
            AcquireJobState::Unpacking
        ) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(&world.api().status(&job.job_id).unwrap().state).unwrap(),
        serde_json::json!({"_tag": "Unpacking"})
    );
    // The worker creates the work folder after the state turns Unpacking.
    tokio::time::timeout(Duration::from_secs(10), async {
        while !names(&world.cache())
            .iter()
            .any(|name| name.starts_with(".tmp-"))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the work folder appears while the reader is held");
    // A live job answers a repeated Get.
    assert_eq!(world.acquire("zipped").await.unwrap().job_id, job.job_id);
    let cancelled = world.api().cancel(&job.job_id).await.unwrap();
    assert!(
        matches!(cancelled.state, AcquireJobState::Cancelled),
        "{cancelled:?}"
    );
    assert_eq!(names(&world.cache()), [".downloads"], "no work folder");
    assert!(
        names(&world.cache().join(".downloads")).is_empty(),
        "no archive"
    );
    assert!(world.snapshot().library.is_empty());

    // Without the hold the same job goes through Unpacking to Imported.
    drop(release);
    imported(&world.get("zipped").await);
}

/// Coordinator decision U2 (romhustler): `filename: "Zool"` has no extension
/// because the real one is only in Content-Disposition, which korrid never
/// reads (N1). A name with no extension goes to the download and the bytes
/// decide; a wrong extension still fails before the download.
#[tokio::test(flavor = "multi_thread")]
async fn a_name_without_an_extension_downloads_and_the_bytes_decide() {
    let world = world();
    let cases = [
        (
            "zipped",
            "Zool",
            "/dl/zipped/",
            seven_z(&[("Zool (Europe).gba", ROM)]),
        ),
        ("plain", "Zool", "/dl/plain/", ROM.to_vec()),
        (
            "wrong",
            "Zool.exe",
            "/dl/wrong/",
            zip(&[("Zool (Europe).gba", ROM)]),
        ),
    ];
    for (id, filename, file, bytes) in &cases {
        world.server.route(
            &format!("/game/{id}"),
            Route::ok(
                "application/json",
                format!(r#"{{"id":"{id}","title":"Zool","system":"gba","file":"{file}","filename":"{filename}"}}"#),
            ),
        );
        world
            .server
            .route(file, Route::ok("application/octet-stream", bytes.clone()));
        world
            .api()
            .details(
                &world.context.registry,
                &DetailsRequest {
                    provider_id: PROVIDER.into(),
                    id: (*id).into(),
                },
            )
            .await
            .unwrap();
    }
    let fetched = |path: &str| {
        world
            .server
            .received()
            .iter()
            .filter(|r| r.path == path)
            .count()
    };

    // An archive under an extensionless name is unpacked and identified.
    let zipped = world.get("zipped").await;
    imported(&zipped);
    assert_eq!(zipped.file_name.as_deref(), Some("Zool"));
    assert_eq!(
        names(&world.cache().join(sha_hex(ROM))),
        ["Zool (Europe).gba"]
    );

    // A plain file with no system extension fails cleanly after the download.
    failed_with(&world.get("plain").await, NOT_A_GAME);
    assert_eq!(fetched("/dl/plain/"), 1);

    // A non-system, non-archive extension still fails before the download.
    failed_with(&world.get("wrong").await, NOT_A_GAME);
    assert_eq!(fetched("/dl/wrong/"), 0);

    assert_eq!(world.snapshot().library.len(), 1);
    assert_eq!(
        names(&world.cache()),
        [".downloads".to_owned(), sha_hex(ROM)]
    );
    assert!(names(&world.cache().join(".downloads")).is_empty());
}

/// Review P1-4: discovery's `combine_discs` joins two cues under a `{disc}`
/// layout into one unit, and the route must find its primary file too.
#[tokio::test(flavor = "multi_thread")]
async fn a_disc_set_that_the_system_layout_combines_routes_to_its_first_disc() {
    let world = world();
    let files: [(&str, &[u8]); 4] = [
        ("Game (Disc 2).cue", b"FILE \"Game (Disc 2).bin\" BINARY\n"),
        ("Game (Disc 2).bin", b"second disc"),
        ("Game (Disc 1).cue", b"FILE \"Game (Disc 1).bin\" BINARY\n"),
        ("Game (Disc 1).bin", b"first disc"),
    ];
    world
        .offer("discs", "sat", "/files/discs.zip", zip(&files))
        .await;
    let game = imported(&world.get("discs").await);
    let key = world.snapshot().library[&game].releases[0]
        .identity
        .as_ref()
        .unwrap()
        .value()
        .to_owned();
    let ready = world.cache().join(key.strip_prefix("sha256:").unwrap());
    assert_eq!(world.route(&game), Some(ready.join("Game (Disc 1).cue")));
}

/// Review P1-4: a playlist that names another playlist owns it, so the
/// folder holds two `.m3u` files and one unit.
#[tokio::test(flavor = "multi_thread")]
async fn a_nested_playlist_routes_to_the_outer_playlist() {
    let world = world();
    let files: [(&str, &[u8]); 6] = [
        ("Game.m3u", b"lists/Inner.m3u\n"),
        ("lists/Inner.m3u", b"../a.cue\n../b.cue\n"),
        ("a.cue", b"FILE a.bin BINARY\n"),
        ("a.bin", b"first disc"),
        ("b.cue", b"FILE b.bin BINARY\n"),
        ("b.bin", b"second disc"),
    ];
    world
        .offer("nested-list", "psx", "/files/set.7z", seven_z(&files))
        .await;
    let game = imported(&world.get("nested-list").await);
    let key = discovered(&world, "psx", &files, "Game.m3u");
    let ready = world.cache().join(key.strip_prefix("sha256:").unwrap());
    assert_eq!(world.route(&game), Some(ready.join("Game.m3u")));
}
