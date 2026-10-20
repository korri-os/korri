//! End to end with a real loopback provider site, a fixture provider plugin,
//! a temp storage root and a paired source: search -> details -> acquire ->
//! placed -> rescan -> Imported, and the refusals around it.
use super::{
    download::{DownloadEngine, STAGING_PREFIX},
    game::{GameAcquisition, GameContext},
    http_fixture::{HttpFixture, Route},
    types::{
        AcquireArtifactRequest, AcquireJobState, AcquireJobStatus, DetailsRequest, SearchRequest,
    },
};
use crate::{
    discovery::{DiscoveryLifecycleCoordinator, FolderSelectionGrantStore},
    plugin::PluginRegistry,
    plugin_installation::EnabledPackage,
    plugin_policy::RegistrySource,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
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

const CORE_SOURCE: &str = r#"
export const name = "core";
export const systems = {
  gba: { id: "gba", title: "Game Boy Advance", content: { extensions: ["gba"] } },
  gb: { id: "gb", title: "Game Boy", content: { extensions: ["gb"] } },
};
"#;

struct World {
    _dirs: Vec<tempfile::TempDir>,
    root: std::path::PathBuf,
    games: std::path::PathBuf,
    server: HttpFixture,
    context: GameContext,
    games_api: GameAcquisition,
    grants: FolderSelectionGrantStore,
}

fn package(root: &Path, name: &str, source: &str, hosts: Vec<String>) -> EnabledPackage {
    let package = root.join(format!("package-{name}"));
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.ts"), source).unwrap();
    EnabledPackage {
        id: format!("@test:{name}"),
        package,
        files: BTreeMap::new(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: vec![],
        hosts,
    }
}

const ROM: &[u8] = b"\x00\x01 a lawful homebrew rom image";

fn world() -> World {
    let server = HttpFixture::start([]);
    let site = server.url(HOST, "");
    let games_json = |games: &[(&str, &str, &str)]| {
        format!(
            "[{}]",
            games
                .iter()
                .map(|(id, title, system)| format!(
                    r#"{{"id":"{id}","title":"{title}","system":"{system}"}}"#
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    server.route(
        "/search?q=2048",
        Route::ok(
            "application/json",
            games_json(&[
                ("2048", "2048", "gba"),
                ("copy", "2048 again", "gba"),
                ("tetris", "Tetris", "gb"),
                ("nosys", "Unknown", "nes"),
                ("gone", "Gone", "gba"),
                ("page", "Page", "gba"),
                ("slow", "Slow", "gba"),
                ("web", "Web", "gba"),
            ]),
        ),
    );
    for (id, file, state) in [
        ("2048", "/files/2048.gba", "ok"),
        ("copy", "/files/copy.gba", "ok"),
        ("tetris", "/files/tetris.gb", "ok"),
        ("gone", "", "gone"),
        ("page", "", "page"),
        ("slow", "/files/slow.gba", "ok"),
        ("web", "/files/web.gba", "ok"),
    ] {
        let system = if id == "tetris" { "gb" } else { "gba" };
        server.route(
            &format!("/game/{id}"),
            Route::ok(
                "application/json",
                format!(
                    r#"{{"id":"{id}","title":"{id}","system":"{system}","file":"{file}","state":"{state}"}}"#
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
        "/files/slow.gba",
        Route::ok("application/octet-stream", vec![1u8; 64 * 1024])
            .paced(1024, Duration::from_millis(50)),
    );
    let _ = site;

    let dirs: Vec<_> = (0..4).map(|_| tempfile::tempdir().unwrap()).collect();
    let root = dirs[0].path().to_path_buf();
    let private = dirs[1].path().to_path_buf();
    let games = dirs[2].path().to_path_buf();
    let packages_root = dirs[3].path();
    let source = PROVIDER_SOURCE.replace(
        "SITE",
        &server.url(HOST, "").trim_end_matches('/').to_owned(),
    );
    let registry = PluginRegistry::from_installed(vec![
        package(packages_root, "shop", &source, vec![HOST.into()]),
        package(packages_root, "core", CORE_SOURCE, vec![]),
    ])
    .unwrap();
    let grants = FolderSelectionGrantStore::default();
    let discovery = DiscoveryLifecycleCoordinator::new_with_registry_source(
        &root,
        &private,
        Arc::new(Mutex::new(())),
        grants.clone(),
        RegistrySource::Selected(Arc::new(registry.clone())),
    );
    discovery.wait_until_idle(Duration::from_secs(10));
    pair(&discovery, &grants, &games, "gba");
    let engine = DownloadEngine::for_test(Duration::from_secs(5))
        .with_name(HOST, "127.0.0.1".parse().unwrap());
    World {
        _dirs: dirs,
        root: root.clone(),
        games,
        server,
        context: GameContext {
            root,
            registry,
            discovery,
        },
        games_api: GameAcquisition::with_engine(engine),
        grants,
    }
}

fn pair(
    discovery: &DiscoveryLifecycleCoordinator,
    grants: &FolderSelectionGrantStore,
    folder: &Path,
    system: &str,
) {
    let grant = grants.issue_approved_path(folder).unwrap();
    discovery.pair(&grant.token, system).unwrap();
    discovery.wait_until_idle(Duration::from_secs(10));
}

fn acquire(id: &str) -> AcquireArtifactRequest {
    AcquireArtifactRequest {
        provider_id: PROVIDER.into(),
        id: id.into(),
        file_name: None,
        source_id: None,
    }
}

async fn finish(world: &World, job: AcquireJobStatus) -> AcquireJobStatus {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let status = world.games_api.status(&job.job_id).unwrap();
            if !matches!(
                status.state,
                AcquireJobState::Resolving
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

fn files_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn downloads(world: &World) -> usize {
    world
        .server
        .received()
        .iter()
        .filter(|r| r.path.starts_with("/files/"))
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn search_details_acquire_place_rescan_imported() {
    let world = world();
    let api = &world.games_api;
    let providers = api.providers(&world.context.registry);
    assert_eq!(providers.providers.len(), 1);
    assert_eq!(
        (
            providers.providers[0].id.as_str(),
            providers.providers[0].title.as_str(),
            providers.providers[0].plugin_id.as_str()
        ),
        (PROVIDER, "Test Shop", "@test:shop")
    );

    let search = SearchRequest {
        query: "2048".into(),
        provider_ids: vec![PROVIDER.into()],
    };
    let claims = api
        .search(&world.context.registry, &search)
        .await
        .unwrap()
        .claims;
    assert_eq!(claims.len(), 8);
    assert_eq!(claims[0].url, world.server.url(HOST, "/game/2048"));
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

    let started = api.acquire(&world.context, &acquire("2048")).unwrap();
    assert!(matches!(started.state, AcquireJobState::Resolving));
    assert_eq!(started.system.as_deref(), Some("gba"));
    let done = finish(&world, started.clone()).await;
    let AcquireJobState::Imported { game_id } = &done.state else {
        panic!("{done:?}");
    };
    assert_eq!(done.file_name.as_deref(), Some("2048.gba"));
    assert_eq!(done.message, None);
    assert_eq!(fs::read(world.games.join("2048.gba")).unwrap(), ROM);
    let snapshot = world.context.discovery.published().unwrap();
    let key = format!("sha256:{}", hex::encode(Sha256::digest(ROM)));
    assert!(snapshot.library[game_id]
        .releases
        .iter()
        .any(|r| r.identity.as_ref().is_some_and(|i| i.value() == key)));
    // No staging bytes remain.
    let staging = world.root.join("cache/shop/shop/.downloads");
    assert!(files_in(&staging).is_empty());

    // Already in the library: Get answers the same job, no second download.
    assert_eq!(downloads(&world), 1);
    let again = api.acquire(&world.context, &acquire("2048")).unwrap();
    assert_eq!(again.job_id, started.job_id);
    assert_eq!(downloads(&world), 1);

    // The same bytes from another claim are not copied a second time.
    let copy = finish(
        &world,
        api.acquire(&world.context, &acquire("copy")).unwrap(),
    )
    .await;
    assert!(
        matches!(&copy.state, AcquireJobState::Imported { game_id: other } if other == game_id),
        "{copy:?}"
    );
    assert_eq!(files_in(&world.games), ["2048.gba"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn refusals_happen_before_any_download_and_failures_are_plain() {
    let world = world();
    let api = &world.games_api;
    let registry = &world.context.registry;
    // Exactly one provider id.
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
    // A claim korrid never returned cannot be acquired.
    assert_eq!(
        api.acquire(&world.context, &acquire("2048"))
            .unwrap_err()
            .code,
        "AcquisitionUnknownClaim"
    );
    api.search(
        registry,
        &SearchRequest {
            query: "2048".into(),
            provider_ids: vec![PROVIDER.into()],
        },
    )
    .await
    .unwrap();

    // Unknown system: shown, not acquirable.
    assert_eq!(
        api.acquire(&world.context, &acquire("nosys"))
            .unwrap_err()
            .code,
        "AcquisitionSystemUnknown"
    );
    // No folder for gb on this device, refused before any download, naming the system.
    let no_source = api.acquire(&world.context, &acquire("tetris")).unwrap_err();
    assert_eq!(no_source.code, "AcquisitionNoSource");
    assert!(no_source.message.contains("Game Boy"), "{no_source:?}");
    assert_eq!(downloads(&world), 0);
    // An unusable file name or source id is a caller error.
    let mut bad = acquire("2048");
    bad.file_name = Some("../x.gba".into());
    assert_eq!(
        api.acquire(&world.context, &bad).unwrap_err().code,
        "AcquisitionCaller"
    );
    let mut bad = acquire("2048");
    bad.source_id = Some("not-a-source".into());
    assert_eq!(
        api.acquire(&world.context, &bad).unwrap_err().code,
        "AcquisitionCaller"
    );

    for (id, code) in [
        ("gone", "DownloadResolutionFailed"),
        ("page", "DownloadNotFinal"),
        ("web", "AcquisitionFailed"),
    ] {
        let done = finish(&world, api.acquire(&world.context, &acquire(id)).unwrap()).await;
        let AcquireJobState::Failed { reason } = &done.state else {
            panic!("{id}: {done:?}");
        };
        assert_eq!(reason.code, code, "{id}");
        assert_eq!(done.message.as_deref(), Some(reason.message.as_str()));
        for secret in ["http", HOST, "/files", "127.0.0.1", "gone at"] {
            assert!(!reason.message.contains(secret), "{id}: {}", reason.message);
        }
        // A failed claim starts a new job on Retry.
        let retry = api.acquire(&world.context, &acquire(id)).unwrap();
        assert_ne!(retry.job_id, done.job_id);
        finish(&world, retry).await;
    }
    assert!(files_in(&world.games).is_empty());
    assert_eq!(
        api.status("no-such-job").unwrap_err().code,
        "AcquisitionUnknownJob"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn several_sources_ask_the_person_and_cancel_leaves_nothing() {
    let world = world();
    let api = &world.games_api;
    let second = tempfile::tempdir().unwrap();
    pair(
        &world.context.discovery,
        &world.grants,
        second.path(),
        "gba",
    );
    api.search(
        &world.context.registry,
        &SearchRequest {
            query: "2048".into(),
            provider_ids: vec![PROVIDER.into()],
        },
    )
    .await
    .unwrap();

    let choose = api.acquire(&world.context, &acquire("slow")).unwrap();
    let AcquireJobState::ChooseSource { sources } = &choose.state else {
        panic!("{choose:?}");
    };
    assert_eq!(sources.len(), 2);
    assert!(sources.iter().all(|s| s.label == "Game Boy Advance"));
    assert_eq!(downloads(&world), 0);
    // Terminal: status stays; a follow-up with sourceId is a new job.
    assert!(matches!(
        api.status(&choose.job_id).unwrap().state,
        AcquireJobState::ChooseSource { .. }
    ));
    let mut chosen = acquire("slow");
    chosen.source_id = Some(sources[1].id.clone());
    let job = api.acquire(&world.context, &chosen).unwrap();
    assert_ne!(job.job_id, choose.job_id);

    // Wait for real progress, then cancel.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let AcquireJobState::Downloading { bytes_done, .. } =
                api.status(&job.job_id).unwrap().state
            {
                if bytes_done > 0 {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // The same claim while live answers the live job.
    assert_eq!(
        api.acquire(&world.context, &chosen).unwrap().job_id,
        job.job_id
    );
    let cancelled = api.cancel(&job.job_id).await.unwrap();
    assert!(
        matches!(cancelled.state, AcquireJobState::Cancelled),
        "{cancelled:?}"
    );
    let staging = world.root.join("cache/shop/shop/.downloads");
    assert!(files_in(&staging).is_empty());
    assert!(files_in(&world.games).is_empty());
    assert!(files_in(second.path()).is_empty());
    // Cancel of a finished job answers it unchanged.
    assert!(matches!(
        api.cancel(&job.job_id).await.unwrap().state,
        AcquireJobState::Cancelled
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_forgets_jobs_and_removes_staging_files() {
    let world = world();
    let staging = super::placement::game_staging(&world.root, "@test:shop", PROVIDER).unwrap();
    fs::write(
        staging.join(format!("{STAGING_PREFIX}interrupted")),
        b"partial",
    )
    .unwrap();
    // A new process: fresh state, startup recovery.
    let restarted = GameAcquisition::default();
    restarted
        .recover(&world.root, &world.context.registry)
        .unwrap();
    assert!(files_in(&staging).is_empty());
    assert_eq!(
        restarted.status("any").unwrap_err().code,
        "AcquisitionUnknownJob"
    );
    assert!(files_in(&world.games).is_empty());
}
