//! The artwork pass against a real plugin handler, a real HTTP server, a
//! real discovery library and the real stores.
use super::{
    super::{
        download::DownloadEngine,
        http_fixture::{Echo, HttpFixture, Received, Route},
    },
    *,
};
use crate::{
    discovery::{reconcile::DiscoveryCoordinator, DiscoveryOptions},
    game_assets::test_image,
    plugin_test_fixtures::{art_package, installed_with_art, ART_PLUGIN_ID},
};
use image::ImageFormat;
use std::{net::IpAddr, time::Duration};

const API: &str = "api.art.test";
const CDN: &str = "cdn.art.test";
const SECRET: &str = "artwork-pass-test-not-a-real-secret";

/// A plugin shaped like the real one: one search turn, then one turn that
/// asks for the four lists. It checks that it is given the title and the
/// responses and nothing else, and it never sees a secret.
fn plugin_source(api: &str) -> String {
    format!(
        r#"
export const name = "steamgriddb";
export const title = "SteamGridDB";
const API = "{api}";
const ROLES = ["tile", "poster", "hero", "logo"];
export const handlers = {{
  "game-assets.candidates": (input) => {{
    if (Object.keys(input).sort().join() !== "responses,title") {{
      throw new Error("unexpected input");
    }}
    if (JSON.stringify(input).toLowerCase().includes("authorization")) {{
      throw new Error("the plugin must never see a request header");
    }}
    const responses = input.responses;
    const slug = input.title.toLowerCase().replace(/[^a-z0-9]+/g, "-");
    if (responses.length === 0) {{
      return {{ _tag: "HttpRequests", requests: [{{ url: API + "/search/" + slug }}] }};
    }}
    if (responses[0].status === 404) return {{ candidates: [] }};
    if (!responses[0].ok) throw new Error("search failed");
    const game = JSON.parse(responses[0].text);
    if (game.candidates) return {{ candidates: game.candidates }};
    if (responses.length === 1) {{
      return {{
        _tag: "HttpRequests",
        requests: ROLES.map((role) => ({{ url: API + "/" + role + "/" + game.id }})),
      }};
    }}
    const candidates = [];
    ROLES.forEach((role, index) => {{
      const list = responses[index + 1];
      if (!list.ok) return;
      for (const item of JSON.parse(list.text).items) {{
        candidates.push({{ role, width: item.width, height: item.height,
          source: {{ id: String(item.id), url: item.url }} }});
      }}
    }});
    return {{ candidates }};
  }},
}};
"#
    )
}

struct Fixture {
    readable: tempfile::TempDir,
    private: tempfile::TempDir,
    roms: tempfile::TempDir,
    server: HttpFixture,
    lock: Arc<Mutex<()>>,
    discovery: DiscoveryCoordinator,
}

fn options() -> DiscoveryOptions {
    DiscoveryOptions {
        first_seen_at: "2026-11-11T00:00:00Z".into(),
        max_diagnostics: 100,
        max_candidates: 100,
        ..DiscoveryOptions::default()
    }
}

impl Fixture {
    /// A library with one game for each ROM file name, and the secret set.
    fn new(roms: &[&str]) -> Self {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let rom_folder = tempfile::tempdir().unwrap();
        for (index, rom) in roms.iter().enumerate() {
            fs::write(rom_folder.path().join(rom), format!("rom-{index}")).unwrap();
        }
        let lock = Arc::new(Mutex::new(()));
        let discovery = DiscoveryCoordinator::for_tests_with_write_lock(
            readable.path(),
            private.path(),
            lock.clone(),
        );
        discovery
            .pair(rom_folder.path().to_path_buf(), "gba", &options())
            .unwrap();
        let fixture = Self {
            readable,
            private,
            roms: rom_folder,
            server: HttpFixture::start([]),
            lock,
            discovery,
        };
        fixture.secrets().set(ART_PLUGIN_ID, SECRET).unwrap();
        fixture
    }

    fn secrets(&self) -> PluginSecrets {
        PluginSecrets::new(self.private.path())
    }

    fn api(&self, path: &str) -> String {
        self.server.url(API, path)
    }

    fn cdn(&self, path: &str) -> String {
        self.server.url(CDN, path)
    }

    fn pass(&self) -> ArtworkPass {
        self.pass_with(&[API, CDN], Some(API))
    }

    fn pass_with(&self, hosts: &[&str], credential_host: Option<&str>) -> ArtworkPass {
        let loopback: IpAddr = "127.0.0.1".parse().unwrap();
        let registry = installed_with_art(
            self.readable.path(),
            art_package(
                self.readable.path(),
                &plugin_source(&self.api("")),
                hosts,
                credential_host,
            ),
        );
        ArtworkPass::new(
            self.readable.path(),
            self.private.path(),
            self.lock.clone(),
            RegistrySource::Selected(Arc::new(registry)),
        )
        .with_engine(
            DownloadEngine::for_test(Duration::from_secs(5))
                .with_name(API, loopback)
                .with_name(CDN, loopback)
                .with_name("other.art.test", loopback),
        )
    }

    fn games(&self) -> Vec<DiscoveryOwnedGame> {
        reconcile::owned_discovery_games(self.readable.path(), self.private.path()).unwrap()
    }

    fn game(&self, title: &str) -> DiscoveryOwnedGame {
        self.games()
            .into_iter()
            .find(|game| game.title == title)
            .unwrap_or_else(|| panic!("no game titled {title}"))
    }

    fn repository(&self) -> GameAssetRepository {
        GameAssetRepository::new(self.private.path())
    }

    fn outcome(&self, game: &DiscoveryOwnedGame) -> Option<AttemptOutcome> {
        read_attempts(self.private.path(), ART_PLUGIN_ID)
            .unwrap()
            .attempts
            .get(&attempt_key(game))
            .map(|record| record.outcome)
    }

    /// The search answers with an id, as the real site does.
    fn found(&self, slug: &str, id: u32) {
        self.server.route(
            &format!("/search/{slug}"),
            Route::ok("application/json", format!(r#"{{"id":{id}}}"#)),
        );
    }

    /// One role's list for one site game: `(asset id, image path, width, height)`.
    fn list(&self, role: &str, id: u32, items: &[(u32, &str, u32, u32)]) {
        let items: Vec<_> = items
            .iter()
            .map(|(asset, path, width, height)| {
                serde_json::json!({ "id": asset, "url": self.cdn(path), "width": width, "height": height })
            })
            .collect();
        self.server.route(
            &format!("/{role}/{id}"),
            Route::ok(
                "application/json",
                serde_json::json!({ "items": items }).to_string(),
            ),
        );
    }

    /// The search answers with the candidates themselves.
    fn candidates(&self, slug: &str, candidates: serde_json::Value) {
        self.server.route(
            &format!("/search/{slug}"),
            Route::ok(
                "application/json",
                serde_json::json!({ "candidates": candidates }).to_string(),
            ),
        );
    }

    fn image(&self, path: &str, bytes: Vec<u8>) {
        self.server.route(path, Route::ok("image/png", bytes));
    }

    fn requests_to(&self, prefix: &str) -> Vec<Received> {
        self.server
            .received()
            .into_iter()
            .filter(|request| request.path.starts_with(prefix))
            .collect()
    }
}

fn status(status: u16) -> Route {
    Route {
        status,
        ..Route::ok("application/json", "{}")
    }
}

fn png(width: u32, height: u32) -> Vec<u8> {
    test_image(width, height, ImageFormat::Png, true, [200, 60, 40])
}

fn carried_secret(request: &Received) -> bool {
    request
        .headers
        .contains(&format!("authorization: bearer {SECRET}"))
}

fn codes(report: &ArtworkReport) -> Vec<&'static str> {
    report.diagnostics.iter().map(|d| d.code).collect()
}

/// Nothing a person or a log can read from a report holds the secret, a URL
/// or a host.
fn assert_plain(report: &ArtworkReport) {
    for diagnostic in &report.diagnostics {
        let text = format!("{diagnostic:?}");
        for forbidden in [SECRET, "http://", "https://", API, CDN, "Bearer"] {
            assert!(!text.contains(forbidden), "{forbidden} in {text}");
        }
    }
}

#[test]
fn a_pass_stores_the_first_candidate_of_each_role_and_the_secret_goes_to_its_host_only() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    fixture.found("wario-land-4", 7);
    // Two tiles: the plugin's order is its preference, so the first is taken.
    fixture.list(
        "tile",
        7,
        &[
            (100, "/img/tile-a.png", 8, 8),
            (101, "/img/tile-b.png", 8, 8),
        ],
    );
    fixture.list("poster", 7, &[(200, "/img/poster.png", 60, 90)]);
    fixture.list("hero", 7, &[(300, "/img/hero.png", 192, 62)]);
    fixture.list("logo", 7, &[(400, "/img/logo.png", 40, 13)]);
    let images = [
        ("/img/tile-a.png", png(8, 8)),
        ("/img/tile-b.png", png(8, 8)),
        ("/img/poster.png", png(60, 90)),
        ("/img/hero.png", png(192, 62)),
        ("/img/logo.png", png(40, 13)),
    ];
    for (path, bytes) in &images {
        fixture.image(path, bytes.clone());
    }

    let report = fixture.pass().run();

    assert_eq!((report.attempted, report.stored), (1, 4));
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    // One search turn, then one turn with the four lists: five requests to
    // the secret's host, each with the Bearer header.
    let api: Vec<_> = fixture
        .server
        .received()
        .into_iter()
        .filter(|request| !request.path.starts_with("/img/"))
        .collect();
    assert_eq!(
        api.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
        [
            "/search/wario-land-4",
            "/tile/7",
            "/poster/7",
            "/hero/7",
            "/logo/7"
        ]
    );
    assert!(api.iter().all(carried_secret));
    // One download for each role, the first candidate, and never the secret.
    let downloads = fixture.requests_to("/img/");
    assert_eq!(
        downloads
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        [
            "/img/tile-a.png",
            "/img/poster.png",
            "/img/hero.png",
            "/img/logo.png"
        ]
    );
    assert!(downloads.iter().all(|request| !request
        .headers
        .iter()
        .any(|h| h.starts_with("authorization"))));

    let game = fixture.game("Wario Land 4");
    let owner = owner_identity(&game);
    let repository = fixture.repository();
    assert_eq!(
        repository.assigned_roles(&owner).unwrap(),
        GameAssetRole::ALL.into_iter().collect()
    );
    let hero = repository
        .matching_assignment(&owner, GameAssetRole::Hero)
        .unwrap()
        .unwrap();
    assert_eq!(hero.source.provider, ART_PLUGIN_ID);
    assert_eq!(hero.source.id.as_deref(), Some("300"));
    assert_eq!(
        hero.source.url.as_deref(),
        Some(fixture.cdn("/img/hero.png").as_str())
    );
    assert_eq!((hero.source.width, hero.source.height), (192, 62));
    // The stored bytes are the downloaded bytes, served by asset id.
    assert_eq!(
        repository
            .assigned_asset(&hero.asset_id)
            .unwrap()
            .unwrap()
            .bytes,
        images[3].1
    );
    // The tile is the cover, as before.
    assert_eq!(
        repository.matching_tile_asset_id(&owner).unwrap(),
        Some(format!(
            "{}.png",
            hex::encode(sha2::Sha256::digest(&images[0].1))
        ))
    );
    // The attempt is in the plugin's own folder.
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));
    let attempts = fixture
        .private
        .path()
        .join(ART_PLUGIN_ID)
        .join("attempts.json");
    assert!(attempts.is_file());
    let text = fs::read_to_string(attempts).unwrap();
    assert!(text.contains("\"assigned\"") && !text.contains(SECRET));

    // A second pass asks nobody: the attempt stands.
    let before = fixture.server.received().len();
    assert_eq!(fixture.pass().run(), ArtworkReport::default());
    assert_eq!(fixture.server.received().len(), before);

    // The whole chain: what the plugin found is what a surface is given.
    let registry = installed_with_art(
        fixture.readable.path(),
        art_package(fixture.readable.path(), &plugin_source(""), &[], None),
    );
    let state =
        crate::config::snapshot::ConfigSnapshotCoordinator::new(fixture.readable.path()).reload();
    let listed = crate::launcher::local_games_with_cover_assets(
        fixture.readable.path(),
        Some(fixture.private.path()),
        &state,
        &registry,
    )
    .games
    .remove(0);
    let entry = crate::surface::launchables::state::Entry::LocalGame {
        game: listed,
        alternatives: Vec::new(),
    };
    let surface = crate::surface::model::game_from_entry(&entry, std::slice::from_ref(&entry));
    let asset_id = |bytes: &[u8]| format!("{}.png", hex::encode(sha2::Sha256::digest(bytes)));
    assert_eq!(
        surface.cover_art_url,
        Some(format!("/game-assets/{}", asset_id(&images[0].1)))
    );
    assert_eq!(surface.system_id.as_deref(), Some("gba"));
    assert_eq!(
        serde_json::to_value(&surface.media).unwrap(),
        serde_json::json!([
            { "role": "poster", "url": format!("/game-assets/{}", asset_id(&images[2].1)),
              "width": 60, "height": 90, "color": "#c83c28" },
            { "role": "hero", "url": format!("/game-assets/{}", asset_id(&images[3].1)),
              "width": 192, "height": 62, "color": "#c83c28" },
            { "role": "logo", "url": format!("/game-assets/{}", asset_id(&images[4].1)),
              "width": 40, "height": 13, "color": "#c83c28" },
        ])
    );
    // Each of those references is an asset the object request serves.
    for media in surface.media.unwrap() {
        let id = media.url.strip_prefix("/game-assets/").unwrap();
        assert!(repository.assigned_asset(id).unwrap().is_some(), "{id}");
    }
}

#[test]
fn no_secret_no_call() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    fixture.secrets().clear(ART_PLUGIN_ID).unwrap();
    fixture.found("wario-land-4", 7);

    let report = fixture.pass().run();

    assert_eq!(codes(&report), ["CredentialMissing"]);
    assert_eq!((report.attempted, report.stored), (0, 0));
    assert!(fixture.server.received().is_empty());
    assert_eq!(fixture.outcome(&fixture.game("Wario Land 4")), None);
    assert_plain(&report);

    // A plugin that declares no secret is asked without one.
    fixture.server.route("/search/wario-land-4", status(404));
    let report = fixture.pass_with(&[API, CDN], None).run();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let received = fixture.server.received();
    assert_eq!(received.len(), 1);
    assert!(!received[0]
        .headers
        .iter()
        .any(|h| h.starts_with("authorization")));
    assert_eq!(
        fixture.outcome(&fixture.game("Wario Land 4")),
        Some(AttemptOutcome::NoMatch)
    );
}

/// A site that repeats the request's `Authorization` header in its answer
/// would hand the secret to the plugin. korrid keeps that answer from the
/// handler, says which host did it, and does not ask again at every rescan.
#[test]
fn a_site_that_repeats_the_secret_is_kept_from_the_plugin_and_is_not_asked_at_every_rescan() {
    let fixture = Fixture::new(&["Anguna.gba", "Wario Land 4.gba"]);
    fixture.candidates(
        "anguna",
        serde_json::json!([{ "role": "tile", "source": { "url": fixture.cdn("/img/a.png") } }]),
    );
    fixture.image("/img/a.png", png(8, 8));
    for (echo, title) in [(Echo::Body, "Wario Land 4"), (Echo::Header, "Wario Land 4")] {
        fixture.server.route(
            "/search/wario-land-4",
            Route::ok("application/json", r#"{"id":7}"#).echoing(echo),
        );
        let report = fixture.pass().run();

        // The echo was real: the request carried the secret.
        let search = fixture.requests_to("/search/wario-land-4");
        assert!(carried_secret(search.last().unwrap()));
        // One search and no second turn: the handler never got that answer,
        // so it never asked for the lists of game 7.
        assert!(fixture.requests_to("/tile/7").is_empty());
        assert_eq!(codes(&report), ["ProviderFailed"]);
        let diagnostic = &report.diagnostics[0];
        assert_eq!(
            diagnostic.message,
            format!(
                "{API} repeated the key of {ART_PLUGIN_ID} in a response. Korri did not give that response to the plugin."
            )
        );
        assert!(!format!("{diagnostic:?}").contains(SECRET));
        // On record until the secret changes: a rescan does not ask again.
        let game = fixture.game(title);
        assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::NoMatch));
        fixture.pass().clear_retryable_attempts();
        let before = fixture.server.received().len();
        assert_eq!(fixture.pass().run().attempted, 0);
        assert_eq!(fixture.server.received().len(), before);
        // A normal answer from the same host in the same pass still worked.
        assert_eq!(
            fixture.outcome(&fixture.game("Anguna")),
            Some(AttemptOutcome::Assigned)
        );
        // Nothing korrid wrote holds the secret.
        let attempts = fs::read_to_string(
            fixture
                .private
                .path()
                .join(ART_PLUGIN_ID)
                .join("attempts.json"),
        )
        .unwrap();
        assert!(!attempts.contains(SECRET));
        // A set or a clear of the secret lets the next pass ask again.
        fixture.pass().secret_changed(ART_PLUGIN_ID).unwrap();
        assert_eq!(fixture.outcome(&game), None);
    }
}

#[test]
fn a_refused_secret_stops_the_pass_and_records_nothing() {
    let fixture = Fixture::new(&["Anguna.gba", "Wario Land 4.gba"]);
    fixture.server.route("/search/anguna", status(401));
    fixture.server.route("/search/wario-land-4", status(401));

    let report = fixture.pass().run();

    // One request, not one for each game.
    assert_eq!(fixture.server.received().len(), 1);
    assert_eq!(codes(&report), ["CredentialUnauthorized"]);
    assert!(fixture
        .games()
        .iter()
        .all(|game| fixture.outcome(game).is_none()));
    assert_plain(&report);
}

#[test]
fn outcomes_follow_what_the_plugin_and_the_site_did() {
    let fixture = Fixture::new(&["Anguna.gba", "Broken.gba", "Square.gba", "Unknown.gba"]);
    // Anguna: a tile only. A role with no candidate is not an error.
    fixture.candidates(
        "anguna",
        serde_json::json!([{ "role": "tile", "source": { "url": fixture.cdn("/img/a.png") } }]),
    );
    fixture.image("/img/a.png", png(8, 8));
    // Broken: the site fails. korrid does not retry; the plugin throws.
    fixture.server.route("/search/broken", status(503));
    // Square: the only candidate is a hero that is not a hero's shape.
    fixture.candidates(
        "square",
        serde_json::json!([{ "role": "hero", "source": { "id": "9", "url": fixture.cdn("/img/s.png") } }]),
    );
    fixture.image("/img/s.png", png(64, 64));
    // Unknown: no route, so the search answers 404 and the plugin finds nothing.

    let report = fixture.pass().run();

    assert_eq!((report.attempted, report.stored), (4, 1));
    let outcome = |title: &str| fixture.outcome(&fixture.game(title));
    assert_eq!(outcome("Anguna"), Some(AttemptOutcome::Assigned));
    assert_eq!(outcome("Broken"), Some(AttemptOutcome::Transient));
    assert_eq!(outcome("Square"), Some(AttemptOutcome::AssetRejected));
    assert_eq!(outcome("Unknown"), Some(AttemptOutcome::NoMatch));
    assert_eq!(codes(&report), ["ProviderFailed", "AssetUnsafeDimensions"]);
    assert_plain(&report);
    // No source without an id keeps one.
    let tile = fixture
        .repository()
        .matching_assignment(
            &owner_identity(&fixture.game("Anguna")),
            GameAssetRole::Tile,
        )
        .unwrap()
        .unwrap();
    assert_eq!(tile.source.id, None);
    // A 503 is one request: korrid does not wait and ask again.
    assert_eq!(fixture.requests_to("/search/broken").len(), 1);

    // Nothing is asked again while the attempts stand.
    let before = fixture.server.received().len();
    assert_eq!(fixture.pass().run().attempted, 0);
    assert_eq!(fixture.server.received().len(), before);

    // A rescan clears `transient` only.
    fixture.pass().clear_retryable_attempts();
    assert_eq!(outcome("Broken"), None);
    assert_eq!(outcome("Square"), Some(AttemptOutcome::AssetRejected));
    assert_eq!(outcome("Unknown"), Some(AttemptOutcome::NoMatch));
    assert_eq!(fixture.pass().run().attempted, 1);
    assert_eq!(outcome("Broken"), Some(AttemptOutcome::Transient));

    // A secret change clears everything except `assigned`.
    fixture.pass().secret_changed(ART_PLUGIN_ID).unwrap();
    assert_eq!(outcome("Anguna"), Some(AttemptOutcome::Assigned));
    for title in ["Broken", "Square", "Unknown"] {
        assert_eq!(outcome(title), None, "{title}");
    }
    // Another plugin's attempts are not this plugin's.
    assert!(clear_attempts_after_secret_change(fixture.private.path(), "@other:art").is_ok());
    assert!(!fixture.private.path().join("@other:art").exists());
}

#[test]
fn a_held_role_is_not_downloaded_again_and_a_complete_game_gets_no_call() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    let game = fixture.game("Wario Land 4");
    let repository = fixture.repository();
    // The tile the device had before the roles widened (after its migration).
    let held = |role, bytes: Vec<u8>| {
        repository
            .assign(
                owner_identity(&game),
                role,
                AssetCandidate {
                    bytes,
                    declared_width: None,
                    declared_height: None,
                    provider: ART_PLUGIN_ID.into(),
                    source_id: Some("1".into()),
                    source_url: None,
                },
            )
            .unwrap()
    };
    let tile = held(GameAssetRole::Tile, png(8, 8));
    fixture.found("wario-land-4", 7);
    fixture.list("tile", 7, &[(100, "/img/new-tile.png", 8, 8)]);
    fixture.list("poster", 7, &[(200, "/img/poster.png", 60, 90)]);
    fixture.list("hero", 7, &[]);
    fixture.list("logo", 7, &[]);
    fixture.image("/img/new-tile.png", png(16, 16));
    fixture.image("/img/poster.png", png(60, 90));

    let report = fixture.pass().run();

    assert_eq!((report.attempted, report.stored), (1, 1));
    assert_eq!(
        fixture
            .requests_to("/img/")
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        ["/img/poster.png"]
    );
    assert_eq!(
        repository
            .matching_assignment(&owner_identity(&game), GameAssetRole::Tile)
            .unwrap()
            .unwrap(),
        tile
    );
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));

    // With every role held and no attempt on record, korrid records the
    // attempt without a call.
    held(GameAssetRole::Hero, png(192, 62));
    held(GameAssetRole::Logo, png(40, 13));
    fs::remove_file(
        fixture
            .private
            .path()
            .join(ART_PLUGIN_ID)
            .join("attempts.json"),
    )
    .unwrap();
    let before = fixture.server.received().len();
    let report = fixture.pass().run();
    assert_eq!((report.attempted, report.stored), (0, 0));
    assert_eq!(fixture.server.received().len(), before);
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));
}

#[test]
fn a_failed_download_keeps_the_roles_that_arrived_and_a_rescan_asks_again() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    let game = fixture.game("Wario Land 4");
    fixture.candidates(
        "wario-land-4",
        serde_json::json!([
            { "role": "tile", "source": { "url": fixture.cdn("/img/tile.png") } },
            { "role": "hero", "source": { "url": fixture.cdn("/img/hero.png") } },
        ]),
    );
    fixture.image("/img/tile.png", png(8, 8));
    fixture.server.route("/img/hero.png", status(500));

    let report = fixture.pass().run();

    assert_eq!(report.stored, 1);
    assert_eq!(codes(&report), ["AssetDownloadFailed"]);
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Transient));
    let tile = fixture
        .repository()
        .matching_assignment(&owner_identity(&game), GameAssetRole::Tile)
        .unwrap()
        .unwrap();
    // Without a rescan the game is not asked about again.
    let before = fixture.server.received().len();
    assert_eq!(fixture.pass().run().attempted, 0);
    assert_eq!(fixture.server.received().len(), before);
    // The next rescan asks again and downloads only what is missing.
    fixture.image("/img/hero.png", png(192, 62));
    fixture.pass().clear_retryable_attempts();
    let before = fixture.server.received().len();
    let report = fixture.pass().run();
    assert_eq!((report.attempted, report.stored), (1, 1));
    assert_eq!(
        fixture.server.received()[before..]
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        ["/search/wario-land-4", "/img/hero.png"]
    );
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));
    assert_eq!(
        fixture
            .repository()
            .assigned_roles(&owner_identity(&game))
            .unwrap(),
        [GameAssetRole::Tile, GameAssetRole::Hero]
            .into_iter()
            .collect()
    );
    // The tile was downloaded once in all, and its record is untouched.
    assert_eq!(fixture.requests_to("/img/tile.png").len(), 1);
    assert_eq!(
        fixture
            .repository()
            .matching_assignment(&owner_identity(&game), GameAssetRole::Tile)
            .unwrap()
            .unwrap(),
        tile
    );
    // `assigned` stands: later rescans ask nobody.
    fixture.pass().clear_retryable_attempts();
    let before = fixture.server.received().len();
    assert_eq!(fixture.pass().run().attempted, 0);
    assert_eq!(fixture.server.received().len(), before);
}

#[test]
fn a_refused_image_alone_never_makes_a_game_transient() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    let game = fixture.game("Wario Land 4");
    // The tile is fine. The hero has a shape korrid refuses.
    fixture.candidates(
        "wario-land-4",
        serde_json::json!([
            { "role": "tile", "source": { "url": fixture.cdn("/img/tile.png") } },
            { "role": "hero", "source": { "url": fixture.cdn("/img/hero.png") } },
        ]),
    );
    fixture.image("/img/tile.png", png(8, 8));
    fixture.image("/img/hero.png", png(64, 64));

    let report = fixture.pass().run();

    assert_eq!(report.stored, 1);
    assert_eq!(codes(&report), ["AssetUnsafeDimensions"]);
    // A refused role does not block `assigned` when another role was stored,
    // so no rescan downloads the refused image again.
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));
    fixture.pass().clear_retryable_attempts();
    let before = fixture.server.received().len();
    assert_eq!(fixture.pass().run().attempted, 0);
    assert_eq!(fixture.server.received().len(), before);
    assert_eq!(fixture.requests_to("/img/hero.png").len(), 1);
}

#[test]
fn an_image_comes_from_a_declared_host_without_a_redirect_and_within_its_size() {
    let fixture = Fixture::new(&["A.gba", "B.gba", "C.gba", "D.gba", "E.gba"]);
    let one = |role: &str, url: String, size: Option<(u32, u32)>| {
        let mut candidate = serde_json::json!({ "role": role, "source": { "url": url } });
        if let Some((width, height)) = size {
            candidate["width"] = width.into();
            candidate["height"] = height.into();
        }
        serde_json::json!([candidate])
    };
    // A: a host the plugin did not declare.
    fixture.candidates(
        "a",
        one(
            "tile",
            fixture.server.url("other.art.test", "/img/a.png"),
            None,
        ),
    );
    fixture.image("/img/a.png", png(8, 8));
    // B: the image host redirects.
    fixture.candidates("b", one("tile", fixture.cdn("/img/b.png"), None));
    fixture
        .server
        .route("/img/b.png", Route::redirect("/img/a.png"));
    // C: more bytes than an asset may have.
    fixture.candidates("c", one("tile", fixture.cdn("/img/c.png"), None));
    fixture.image("/img/c.png", vec![0; MAX_ASSET_BYTES + 1]);
    // D: not the size the plugin said.
    fixture.candidates(
        "d",
        one("tile", fixture.cdn("/img/d.png"), Some((512, 512))),
    );
    fixture.image("/img/d.png", png(8, 8));
    // E: the size the plugin said.
    fixture.candidates("e", one("tile", fixture.cdn("/img/e.png"), Some((8, 8))));
    fixture.image("/img/e.png", png(8, 8));

    let report = fixture.pass().run();

    assert_eq!(
        codes(&report),
        [
            "AssetUrlRejected",
            "AssetRedirectRejected",
            "AssetTooLarge",
            "AssetInconsistentDimensions"
        ]
    );
    assert_plain(&report);
    let outcome = |title: &str| fixture.outcome(&fixture.game(title));
    assert_eq!(outcome("A"), Some(AttemptOutcome::Transient));
    assert_eq!(outcome("B"), Some(AttemptOutcome::Transient));
    assert_eq!(outcome("C"), Some(AttemptOutcome::Transient));
    assert_eq!(outcome("D"), Some(AttemptOutcome::AssetRejected));
    assert_eq!(outcome("E"), Some(AttemptOutcome::Assigned));
    assert_eq!(report.stored, 1);
    // The undeclared host got no request, and the redirect was not followed.
    assert_eq!(fixture.requests_to("/img/a.png").len(), 0);
    assert_eq!(fixture.requests_to("/img/b.png").len(), 1);
}

#[test]
fn an_image_comes_over_https_only() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let hosts = vec![CDN.to_owned()];
    let fetch = |url: &str| {
        runtime.block_on(DownloadEngine::default().fetch_image(
            &url::Url::parse(url).unwrap(),
            &hosts,
            MAX_ASSET_BYTES,
        ))
    };
    // Refused before any lookup or request.
    assert_eq!(
        fetch(&format!("http://{CDN}/grid/1.png")),
        Err(Failure::UnsafeUrl)
    );
    assert_eq!(
        fetch("https://not-declared.example/grid/1.png"),
        Err(Failure::UndeclaredHost)
    );
    assert_eq!(
        fetch("https://127.0.0.1/grid/1.png"),
        Err(Failure::UndeclaredHost)
    );
}

#[test]
fn a_result_korrid_cannot_admit_is_a_provider_failure_and_downloads_nothing() {
    let invalid = [
        // Roles with no consumer stay refused.
        serde_json::json!([{ "role": "banner", "source": { "url": "https://cdn.art.test/a.png" } }]),
        serde_json::json!([{ "role": "screenshot", "source": { "url": "https://cdn.art.test/a.png" } }]),
        // A plugin cannot name the provider, a title or anything else.
        serde_json::json!([{ "role": "tile", "source": { "provider": "@other:art", "url": "https://cdn.art.test/a.png" } }]),
        serde_json::json!([{ "role": "tile", "title": "Better Title", "source": { "url": "https://cdn.art.test/a.png" } }]),
        serde_json::json!([{ "role": "tile", "source": { "id": "1" } }]),
        serde_json::json!([{ "role": "tile", "source": { "id": 1, "url": "https://cdn.art.test/a.png" } }]),
        serde_json::json!([{ "role": "tile", "source": { "id": null, "url": "https://cdn.art.test/a.png" } }]),
        serde_json::json!([{ "role": "tile", "source": { "id": "", "url": "https://cdn.art.test/a.png" } }]),
        serde_json::json!([{ "role": "tile", "width": 0, "source": { "url": "https://cdn.art.test/a.png" } }]),
        serde_json::json!([{ "role": "tile", "width": "512", "source": { "url": "https://cdn.art.test/a.png" } }]),
        // Legacy `GameAssetSource.url`: no credentials, query or fragment.
        serde_json::json!([{ "role": "tile", "source": { "url": "https://user:pw@cdn.art.test/a.png" } }]),
        serde_json::json!([{ "role": "tile", "source": { "url": "https://cdn.art.test/a.png?key=1" } }]),
        serde_json::json!([{ "role": "tile", "source": { "url": "https://cdn.art.test/a.png#x" } }]),
        serde_json::json!([{ "role": "tile", "source": { "url": "/a.png" } }]),
        // One bad candidate refuses the whole answer.
        serde_json::json!([
            { "role": "tile", "source": { "url": "https://cdn.art.test/a.png" } },
            { "role": "banner", "source": { "url": "https://cdn.art.test/b.png" } }
        ]),
    ];
    for candidates in invalid {
        let fixture = Fixture::new(&["Wario Land 4.gba"]);
        fixture.candidates("wario-land-4", candidates.clone());
        let report = fixture.pass().run();
        assert_eq!(codes(&report), ["ProviderFailed"], "{candidates}");
        assert_eq!(report.stored, 0);
        assert_eq!(fixture.server.received().len(), 1, "{candidates}");
        assert_eq!(
            fixture.outcome(&fixture.game("Wario Land 4")),
            Some(AttemptOutcome::Transient)
        );
        assert_plain(&report);
    }
    // The result is `{ candidates }` and nothing else.
    for output in [
        r#"{"candidates": null}"#,
        r#"{"candidates": [], "title": "Better Title"}"#,
        r#"[]"#,
        r#"{}"#,
    ] {
        assert!(
            turns::decode_turn::<GameAssetCandidates>(output).is_err(),
            "{output}"
        );
    }
    assert!(matches!(
        turns::decode_turn::<GameAssetCandidates>(r#"{"candidates": []}"#),
        Ok(turns::Turn::Result(GameAssetCandidates { candidates })) if candidates.is_empty()
    ));
}

#[test]
fn a_pass_asks_one_plugin_about_eight_games_and_the_next_pass_goes_on() {
    let roms: Vec<String> = (0..10)
        .map(|index| format!("Game {index:02}.gba"))
        .collect();
    let fixture = Fixture::new(&roms.iter().map(String::as_str).collect::<Vec<_>>());

    let first = fixture.pass().run();
    assert_eq!(first.attempted, 8);
    assert_eq!(fixture.server.received().len(), 8);
    let second = fixture.pass().run();
    assert_eq!(second.attempted, 2);
    assert_eq!(fixture.server.received().len(), 10);
    assert_eq!(fixture.pass().run().attempted, 0);
    assert!(fixture
        .games()
        .iter()
        .all(|game| fixture.outcome(game) == Some(AttemptOutcome::NoMatch)));
}

#[test]
fn a_changed_rom_is_another_game_and_is_asked_about_again() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    assert_eq!(fixture.pass().run().attempted, 1);
    let before = fixture.game("Wario Land 4");
    fs::write(
        fixture.roms.path().join("Wario Land 4.gba"),
        b"another dump",
    )
    .unwrap();
    fixture.discovery.rescan(&options()).unwrap();
    let after = fixture.game("Wario Land 4");
    assert_ne!(attempt_key(&before), attempt_key(&after));
    assert_eq!(fixture.pass().run().attempted, 1);
    assert_eq!(fixture.outcome(&after), Some(AttemptOutcome::NoMatch));
}

#[test]
fn a_secret_that_changes_during_the_pass_keeps_its_attempt_from_being_recorded() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    fixture.server.route(
        "/search/wario-land-4",
        Route::ok("application/json", vec![b' '; 4096]).paced(256, Duration::from_millis(40)),
    );
    let secrets = fixture.secrets();
    let changing = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        secrets.set(ART_PLUGIN_ID, "another-secret").unwrap();
    });

    let report = fixture.pass().run();
    changing.join().unwrap();

    // The answer was about the old secret: nothing is on record, so the next
    // pass asks again with the new one.
    assert_eq!(report.attempted, 1);
    assert_eq!(fixture.outcome(&fixture.game("Wario Land 4")), None);
    assert!(!fixture
        .private
        .path()
        .join(ART_PLUGIN_ID)
        .join("attempts.json")
        .exists());
}

#[test]
fn records_from_before_the_migration_stop_the_pass_before_any_request() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    let assets = fixture.private.path().join("game-assets");
    fs::create_dir_all(&assets).unwrap();
    let old = serde_json::json!({ "wl4": { "asset_id": format!("{}.png", "a".repeat(64)),
        "role": "tile",
        "owner": { "playable_id": "wl4", "release_id": "r", "release_fingerprint": "f", "rom_identity": "i" },
        "source": { "provider": "steamgriddb", "game_id": 1, "grid_id": 2,
            "sha256": "sha256:aa", "width": 512, "height": 512, "format": "png" } } })
    .to_string();
    fs::write(assets.join("assignments.json"), &old).unwrap();

    let report = fixture.pass().run();

    assert_eq!(codes(&report), ["AssetStorageUnavailable"]);
    assert!(fixture.server.received().is_empty());
    assert_eq!(
        fs::read_to_string(assets.join("assignments.json")).unwrap(),
        old
    );
    assert_eq!(fixture.outcome(&fixture.game("Wario Land 4")), None);
}

#[test]
fn a_device_with_no_artwork_plugin_runs_no_pass() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    let pass = ArtworkPass::new(
        fixture.readable.path(),
        fixture.private.path(),
        fixture.lock.clone(),
        RegistrySource::Selected(Arc::new(crate::plugin_test_fixtures::installed(
            fixture.readable.path(),
        ))),
    );
    assert_eq!(pass.run(), ArtworkReport::default());
    pass.clear_retryable_attempts();
    assert!(!fixture
        .private
        .path()
        .join(ART_PLUGIN_ID)
        .join("attempts.json")
        .exists());
}
