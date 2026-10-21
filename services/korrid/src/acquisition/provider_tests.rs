//! ADR 0014 end to end with a real loopback provider site and a fixture
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
use crate::{
    config,
    discovery::{DiscoveryLifecycleCoordinator, FolderSelectionGrantStore},
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    plugin_policy::RegistrySource,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};

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
    return { _tag: "FinalDownload", providerId: input.providerId, url: site + game.file };
  },
};
"#;

/// The system and a runner for it. `PINNED` adds an ADR 0012/0013 artifact
/// so the queue can be shared with a pinned download.
fn core_source(pinned: Option<(&str, &str)>) -> String {
    let artifacts = pinned.map_or(String::new(), |(url, sha256)| {
        format!("export const artifacts = {{ big: {{ id: '@test:core/big', version: 'v1', files: [{{ url: '{url}', sha256: '{sha256}' }}] }} }};")
    });
    format!(
        r#"
export const name = "core";
export const systems = {{
  gba: {{ id: "gba", title: "Game Boy Advance", content: {{ extensions: ["gba"] }} }},
  gb: {{ id: "gb", title: "Game Boy", content: {{ extensions: ["gb"] }} }},
}};
export const runners = {{ run: {{ id: "@test:core/run", program: "run", systems: ["gba"] }} }};
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
        ("nosys", "Unknown", "nes", "", "ok"),
        ("gone", "Gone", "gba", "", "gone"),
        ("page", "Page", "gba", "", "page"),
        ("slow", "Slow", "gba", "/files/slow.gba", "ok"),
        ("web", "Web", "gba", "/files/web.gba", "ok"),
        ("zip", "Zipped", "gba", "/files/game.zip", "ok"),
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
    server.route("/files/game.zip", Route::ok("application/zip", ROM));
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
}

fn engine() -> DownloadEngine {
    DownloadEngine::for_test(Duration::from_secs(5)).with_name(HOST, "127.0.0.1".parse().unwrap())
}

fn world_with(server: HttpFixture, pinned: Option<(&str, &str)>) -> World {
    let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let root = dirs[0].path().to_path_buf();
    let private = dirs[1].path().to_path_buf();
    let source = PROVIDER_SOURCE.replace("SITE", server.url(HOST, "").trim_end_matches('/'));
    let registry = PluginRegistry::from_installed(vec![
        package(dirs[2].path(), "shop", &source, vec![HOST.into()]),
        package(dirs[2].path(), "core", &core_source(pinned), vec![]),
    ])
    .unwrap();
    let discovery = DiscoveryLifecycleCoordinator::new_with_registry_source(
        &root,
        &private,
        Arc::new(Mutex::new(())),
        FolderSelectionGrantStore::default(),
        RegistrySource::Selected(Arc::new(registry.clone())),
    );
    discovery.wait_until_idle(Duration::from_secs(10));
    let blocks = Arc::new(sleep::InMemoryBlocks::default());
    World {
        _dirs: dirs,
        root: root.clone(),
        server,
        context: GameContext {
            root,
            registry,
            discovery,
        },
        coordinator: AcquisitionCoordinator {
            engine: DownloadEngine::for_test(Duration::from_secs(5)),
            sleep: sleep::SleepBlock::InMemory(blocks.clone()),
            games: GameAcquisition::with_engine(engine()),
            ..Default::default()
        },
        blocks,
    }
}

fn world() -> World {
    world_with(site(), None)
}

impl World {
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
    fn acquire(&self, id: &str) -> Result<AcquireJobStatus, crate::RpcFailure> {
        self.coordinator.acquire(&self.context, &request(id))
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

    let started = world.acquire("2048").unwrap();
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
    let copy = world.finish(world.acquire("copy").unwrap()).await;
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
    assert_eq!(imported(&world.acquire("2048").unwrap()), game_id);
    assert_eq!(world.downloads(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_route_reaches_the_download_by_hash_and_passes_it_as_content_path() {
    let world = world();
    world.search().await;
    let game_id = imported(&world.finish(world.acquire("2048").unwrap()).await);
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
    )
    .unwrap();
    let input: crate::launcher::plugin_launch::PluginLaunchInput =
        serde_json::from_str(&launch.command[3]).unwrap();
    assert_eq!(
        input.content_path.as_deref(),
        Some(download.display().to_string().as_str())
    );
    assert!(input.provider_ref.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_download_keeps_the_game_in_history_and_get_downloads_into_it_again() {
    let world = world();
    world.search().await;
    let game_id = imported(&world.finish(world.acquire("2048").unwrap()).await);
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
    let again = world.finish(world.acquire("2048").unwrap()).await;
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
    let slow = world.acquire("2048").unwrap();
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
    let first = world.acquire("2048").unwrap();
    let cancelled = world.acquire("copy").unwrap();
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
    let job = world.acquire("slow").unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !matches!(world.api().status(&job.job_id).unwrap().state, AcquireJobState::Downloading { bytes_done, .. } if bytes_done > 0) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // A live job answers a repeated Get.
    assert_eq!(world.acquire("slow").unwrap().job_id, job.job_id);
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
        world.acquire("2048").unwrap_err().code,
        "AcquisitionUnknownClaim"
    );
    world.search().await;
    assert_eq!(
        world.acquire("nosys").unwrap_err().code,
        "AcquisitionSystemUnknown"
    );
    let mut bad = request("2048");
    bad.file_name = Some("../x.gba".into());
    assert_eq!(
        world
            .coordinator
            .acquire(&world.context, &bad)
            .unwrap_err()
            .code,
        "AcquisitionCaller"
    );
    assert_eq!(world.downloads(), 0);

    // Not a file of the claim's system: refused by name, never downloaded.
    let zip = world.finish(world.acquire("zip").unwrap()).await;
    assert!(
        matches!(&zip.state, AcquireJobState::Failed { reason } if reason.message.contains("not a game for this system")),
        "{zip:?}"
    );
    assert_eq!(world.downloads(), 0);
    for (id, code) in [
        ("gone", "DownloadResolutionFailed"),
        ("page", "DownloadNotFinal"),
        ("web", "AcquisitionFailed"),
    ] {
        let done = world.finish(world.acquire(id).unwrap()).await;
        let AcquireJobState::Failed { reason } = &done.state else {
            panic!("{id}: {done:?}");
        };
        assert_eq!(reason.code, code, "{id}");
        assert_eq!(done.message.as_deref(), Some(reason.message.as_str()));
        for secret in ["http", HOST, "/files", "127.0.0.1", "gone at"] {
            assert!(!reason.message.contains(secret), "{id}: {}", reason.message);
        }
        let retry = world.acquire(id).unwrap();
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
    let game_id = imported(&world.finish(world.acquire("2048").unwrap()).await);
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
    let job = restarted.acquire(&world.context, &request("2048")).unwrap();
    assert_eq!(imported(&job), game_id);
    assert_eq!(world.server.received().len(), requests);
    // A claim never held and never acquired still needs search or details.
    assert_eq!(
        restarted
            .acquire(&world.context, &request("copy"))
            .unwrap_err()
            .code,
        "AcquisitionUnknownClaim"
    );
}

/// The local RPC refuses Remove download while the download's game runs, and
/// the launch reaches the download through the route (host-mode device).
#[tokio::test(flavor = "multi_thread")]
async fn remove_download_refuses_while_the_game_runs() {
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    let world = world();
    world.search().await;
    let game_id = imported(&world.finish(world.acquire("2048").unwrap()).await);
    let config = world.root.join("host.toml");
    fs::write(&config, "label = \"downloads\"\ngames = []\n").unwrap();
    let runtime = crate::host::HostRuntime::from_paths_with_backend(
        &config,
        Some(world.root.clone()),
        world.root.join("private"),
        Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
    )
    .with_route_registry(world.root.clone(), world.context.registry.clone());
    let app = crate::secure_host_routers(
        runtime,
        &world.root.join("private"),
        Some(crate::PortalAccess::new(
            "downloads",
            "http://127.0.0.1:8099",
            crate::PortalPermission::LocalSessions,
        )),
    )
    .0;
    let call = |tag: &'static str, payload: Value| {
        let app = app.clone();
        async move {
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/rpc")
                        .header("Content-Type", "application/json")
                        .header("Authorization", "Bearer downloads")
                        .header("Origin", "http://127.0.0.1:8099")
                        .body(Body::from(
                            json!({"_tag":tag, "payload":payload}).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            serde_json::from_slice::<Value>(
                &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            )
            .unwrap()
        }
    };
    let reserved = call("app.session.reserve", json!({"gameId": game_id})).await;
    let started = call(
        "app.session.start",
        json!({"gameId": game_id, "expectedLaunchId": reserved["outcome"]["payload"]["launchId"]}),
    )
    .await;
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    let refused = call("app.acquisition.remove", json!({"gameId": game_id})).await;
    assert_eq!(
        refused["outcome"]["payload"]["code"], "ActiveSessionConflict",
        "{refused}"
    );
    assert!(world.cache().join(sha_hex(ROM)).join("2048.gba").is_file());
}
