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
    game_assets::{test_image, AssetAssignment},
    plugin_test_fixtures::{
        art_package, art_package_as, installed_with_art, installed_with_arts, ART_PLUGIN_ID,
    },
};
use image::ImageFormat;
use korri_surface_treaty::SurfaceMediaKind::{self, Icon, Logo, Picture};
use std::{
    net::IpAddr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const API: &str = "api.art.test";
const CDN: &str = "cdn.art.test";
const SECRET: &str = "artwork-pass-test-not-a-real-secret";
/// A second art plugin. Its id sorts before `ART_PLUGIN_ID`.
const SECOND: &str = "@korri:igdb";
/// The second plugin's routes on the same test site.
const SECOND_PREFIX: &str = "/two";
/// The fixture's clock starts here, in seconds since the Unix epoch.
const START: u64 = 1_794_000_000;
const DAY: u64 = 24 * 60 * 60;

/// A plugin shaped like the real one: one search turn, then one turn that
/// asks for the three lists. It checks that it is given the title, the
/// system, the stored ids and the responses and nothing else, and it never
/// sees a secret. A stored `test` id replaces the search (ADR 0090, id
/// first), and a search answer's `ids` are returned as the match's ids.
fn plugin_source(id: &str, api: &str) -> String {
    format!(
        r#"
export const name = "{name}";
export const title = "Test art";
export const providers = {{ "{id}": {{ id: "{id}", title: "Test shop" }} }};
const API = "{api}";
const KINDS = ["logo", "picture", "icon"];
export const handlers = {{
  "claims.search": () => [],
  "claims.details": () => ({{}}),
  "artifact.resolve-download": () => ({{}}),
  "game-assets.candidates": (input) => {{
    if (Object.keys(input).sort().join() !== "externalIds,responses,system,title") {{
      throw new Error("unexpected input");
    }}
    if (typeof input.system.id !== "string" || typeof input.system.externalIds !== "object"
      || (input.system.id === "gba" && input.system.title !== "Game Boy Advance")) {{
      throw new Error("unexpected system");
    }}
    if (JSON.stringify(input).toLowerCase().includes("authorization")) {{
      throw new Error("the plugin must never see a request header");
    }}
    const responses = input.responses;
    const slug = input.title.toLowerCase().replace(/[^a-z0-9]+/g, "-");
    const stored = (input.externalIds.test ?? [])[0];
    if (responses.length === 0) {{
      const url = stored === undefined ? API + "/search/" + slug : API + "/id/" + stored;
      return {{ _tag: "HttpRequests", requests: [{{ url }}] }};
    }}
    if (responses[0].status === 404) return {{ candidates: [] }};
    if (!responses[0].ok) throw new Error("search failed");
    const game = JSON.parse(responses[0].text);
    const ids = game.ids ? {{ externalIds: game.ids }} : {{}};
    if (game.candidates) return {{ candidates: game.candidates, ...ids }};
    if (responses.length === 1) {{
      return {{
        _tag: "HttpRequests",
        requests: KINDS.map((kind) => ({{ url: API + "/" + kind + "/" + game.id }})),
      }};
    }}
    const candidates = [];
    KINDS.forEach((kind, index) => {{
      const list = responses[index + 1];
      if (!list.ok) return;
      for (const item of JSON.parse(list.text).items) {{
        candidates.push({{ kind, width: item.width, height: item.height,
          source: {{ id: String(item.id), url: item.url }} }});
      }}
    }});
    return {{ candidates, ...ids }};
  }},
}};
"#,
        name = id.rsplit(':').next().unwrap(),
    )
}

struct Fixture {
    readable: tempfile::TempDir,
    private: tempfile::TempDir,
    roms: tempfile::TempDir,
    server: HttpFixture,
    lock: Arc<Mutex<()>>,
    discovery: DiscoveryCoordinator,
    /// Seconds since the Unix epoch, as every pass of this fixture reads it.
    now: Arc<AtomicU64>,
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
            now: Arc::new(AtomicU64::new(START)),
        };
        fixture.secrets().set(ART_PLUGIN_ID, SECRET).unwrap();
        fixture
    }

    /// The library lobby games of ibuki (`artwork-library-lobby.yaml`).
    fn lobby() -> Self {
        let fixture = Self::new(&[]);
        fs::write(
            fixture.library_path(),
            include_str!("../../tests/fixtures/artwork-library-lobby.yaml"),
        )
        .unwrap();
        fixture
    }

    fn library_path(&self) -> PathBuf {
        self.readable.path().join("catalog/library.yaml")
    }

    /// Change one game's record in `catalog/library.yaml`, as an explicit
    /// catalog edit can.
    fn edit_game(&self, playable_id: &str, edit: impl FnOnce(&mut serde_yaml::Value)) {
        let mut document: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(self.library_path()).unwrap()).unwrap();
        edit(&mut document["library"][playable_id]);
        fs::write(
            self.library_path(),
            serde_yaml::to_string(&document).unwrap(),
        )
        .unwrap();
    }

    fn advance(&self, seconds: u64) {
        self.now.fetch_add(seconds, Ordering::SeqCst);
    }

    /// Publish the same library record and ready-cache layout as acquisition.
    fn acquire(&self, title: &str, bytes: &[u8]) -> String {
        use crate::discovery::{identify::ContentIdentity, reconcile::AcquiredGame};
        let identity = ContentIdentity::Hash {
            sha256: format!("sha256:{}", hex::encode(Sha256::digest(bytes))),
            sha1: format!("sha1:{}", "a".repeat(40)),
        };
        let id = self
            .discovery
            .record_acquired(&AcquiredGame {
                provider: ART_PLUGIN_ID,
                claim_id: "homebrew",
                title,
                system: "gba",
                identity: &identity,
            })
            .unwrap();
        // ADR 0092: an installed release lives under its release key.
        let cache = super::super::placement::install_cache(self.readable.path())
            .join(super::super::placement::ready_name(identity.value()).unwrap());
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("game.gba"), bytes).unwrap();
        id
    }

    fn surface(&self, title: &str) -> korri_surface_treaty::SurfaceGame {
        let registry = installed_with_art(
            self.readable.path(),
            art_package(
                self.readable.path(),
                &plugin_source(ART_PLUGIN_ID, ""),
                &[],
                None,
            ),
        );
        let state =
            crate::config::snapshot::ConfigSnapshotCoordinator::new(self.readable.path()).reload();
        let game = crate::launcher::local_games_with_cover_assets(
            self.readable.path(),
            Some(self.private.path()),
            &state,
            &registry,
        )
        .games
        .into_iter()
        .find(|game| game.title == title)
        .expect("local game");
        let entry = crate::surface::launchables::state::Entry::LocalGame {
            game,
            alternatives: Vec::new(),
        };
        crate::surface::model::game_from_entry(&entry, std::slice::from_ref(&entry))
    }

    // Publish a second content release through the real writer, then remove
    // the old release from the library, as an explicit catalog edit can do.
    fn replace_acquired_release(&self, title: &str, bytes: &[u8]) {
        let id = self.acquire(title, bytes);
        let state =
            crate::config::snapshot::ConfigSnapshotCoordinator::new(self.readable.path()).reload();
        let mut library = state.snapshot.library.clone();
        assert_eq!(library.get_mut(&id).unwrap().releases.len(), 2);
        library.get_mut(&id).unwrap().releases.remove(0);
        fs::write(
            self.library_path(),
            serde_yaml::to_string(&BTreeMap::from([("library", library)])).unwrap(),
        )
        .unwrap();
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

    fn engine(&self) -> DownloadEngine {
        let loopback: IpAddr = "127.0.0.1".parse().unwrap();
        DownloadEngine::for_test(Duration::from_secs(5))
            .with_name(API, loopback)
            .with_name(CDN, loopback)
            .with_name("other.art.test", loopback)
    }

    fn pass_over(&self, registry: PluginRegistry) -> ArtworkPass {
        let now = self.now.clone();
        ArtworkPass::new(
            self.readable.path(),
            self.private.path(),
            self.lock.clone(),
            RegistrySource::Selected(Arc::new(registry)),
        )
        .with_engine(self.engine())
        .with_clock(move || UNIX_EPOCH + Duration::from_secs(now.load(Ordering::SeqCst)))
    }

    fn pass_with(&self, hosts: &[&str], credential_host: Option<&str>) -> ArtworkPass {
        self.pass_over(installed_with_art(
            self.readable.path(),
            art_package(
                self.readable.path(),
                &plugin_source(ART_PLUGIN_ID, &self.api("")),
                hosts,
                credential_host,
            ),
        ))
    }

    /// `ART_PLUGIN_ID` and `SECOND` together, each with its secret. The
    /// second plugin's routes start with `SECOND_PREFIX`.
    fn pass_of_two(&self) -> ArtworkPass {
        self.secrets().set(SECOND, SECRET).unwrap();
        let root = self.readable.path();
        self.pass_over(installed_with_arts(
            root,
            vec![
                art_package(
                    root,
                    &plugin_source(ART_PLUGIN_ID, &self.api("")),
                    &[API, CDN],
                    Some(API),
                ),
                art_package_as(
                    root,
                    SECOND,
                    &plugin_source(SECOND, &self.api(SECOND_PREFIX)),
                    &[API, CDN],
                    Some(API),
                ),
            ],
        ))
    }

    fn games(&self) -> Vec<ArtworkGame> {
        reconcile::artwork_games(
            self.readable.path(),
            self.private.path(),
            &installed_with_art(
                self.readable.path(),
                art_package(
                    self.readable.path(),
                    &plugin_source(ART_PLUGIN_ID, ""),
                    &[],
                    None,
                ),
            ),
        )
        .unwrap()
    }

    fn game(&self, title: &str) -> ArtworkGame {
        self.games()
            .into_iter()
            .find(|game| game.title == title)
            .unwrap_or_else(|| panic!("no game titled {title}"))
    }

    fn repository(&self) -> GameAssetRepository {
        GameAssetRepository::new(self.private.path())
    }

    /// Every stored record of the game as it is now.
    fn stored(&self, game: &ArtworkGame) -> Vec<AssetAssignment> {
        self.repository()
            .assignments(&owner_identity(game))
            .unwrap()
    }

    /// The stored records of one kind, by plugin and place.
    fn stored_of(&self, game: &ArtworkGame, kind: SurfaceMediaKind) -> Vec<AssetAssignment> {
        self.stored(game)
            .into_iter()
            .filter(|assignment| assignment.kind == kind)
            .collect()
    }

    fn record_of(&self, plugin: &str, game: &ArtworkGame) -> Option<AttemptRecord> {
        read_attempts(self.private.path(), plugin)
            .unwrap()
            .attempts
            .get(&attempt_key(game))
            .cloned()
    }

    fn outcome_of(&self, plugin: &str, game: &ArtworkGame) -> Option<AttemptOutcome> {
        self.record_of(plugin, game).map(|record| record.outcome)
    }

    fn outcome(&self, game: &ArtworkGame) -> Option<AttemptOutcome> {
        self.outcome_of(ART_PLUGIN_ID, game)
    }

    /// The search answers with an id, as the real site does.
    fn found(&self, slug: &str, id: u32) {
        self.found_at("", slug, id);
    }

    fn found_at(&self, prefix: &str, slug: &str, id: u32) {
        self.server.route(
            &format!("{prefix}/search/{slug}"),
            Route::ok("application/json", format!(r#"{{"id":{id}}}"#)),
        );
    }

    /// One kind's list for one site game: `(asset id, image path, width, height)`.
    fn list(&self, kind: &str, id: u32, items: &[(u32, &str, u32, u32)]) {
        self.list_at("", kind, id, items);
    }

    fn list_at(&self, prefix: &str, kind: &str, id: u32, items: &[(u32, &str, u32, u32)]) {
        let items: Vec<_> = items
            .iter()
            .map(|(asset, path, width, height)| {
                serde_json::json!({ "id": asset, "url": self.cdn(path), "width": width, "height": height })
            })
            .collect();
        self.server.route(
            &format!("{prefix}/{kind}/{id}"),
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

fn asset_id(bytes: &[u8]) -> String {
    format!("{}.png", hex::encode(Sha256::digest(bytes)))
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
fn a_pass_stores_the_first_four_candidates_of_each_kind_and_the_secret_goes_to_its_host_only() {
    assert_art_chain(false);
}

#[test]
fn an_acquired_game_gets_art_through_the_local_catalog_and_surface_model() {
    assert_art_chain(true);
}

fn assert_art_chain(acquired: bool) {
    let fixture = Fixture::new(if acquired { &[] } else { &["Wario Land 4.gba"] });
    if acquired {
        assert_eq!(
            fixture.acquire("Wario Land 4", b"acquired rom"),
            "wario-land-4"
        );
    }
    fixture.found("wario-land-4", 7);
    // Five pictures: the plugin's order is its preference, so the first four
    // are taken.
    fixture.list(
        "picture",
        7,
        &[
            (100, "/img/square.png", 512, 512),
            (101, "/img/poster.png", 600, 900),
            (102, "/img/hero.png", 1920, 620),
            (103, "/img/large.png", 1024, 1024),
            (104, "/img/fifth.png", 300, 300),
        ],
    );
    fixture.list("logo", 7, &[(400, "/img/logo.png", 400, 130)]);
    fixture.list("icon", 7, &[]);
    let images = [
        ("/img/square.png", png(512, 512)),
        ("/img/poster.png", png(600, 900)),
        ("/img/hero.png", png(1920, 620)),
        ("/img/large.png", png(1024, 1024)),
        ("/img/fifth.png", png(300, 300)),
        ("/img/logo.png", png(400, 130)),
    ];
    for (path, bytes) in &images {
        fixture.image(path, bytes.clone());
    }

    let report = fixture.pass().run();

    assert_eq!((report.attempted, report.stored), (1, 5));
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    // One search turn, then one turn with the three lists: four requests to
    // the secret's host, each with the Bearer header.
    let api: Vec<_> = fixture
        .server
        .received()
        .into_iter()
        .filter(|request| !request.path.starts_with("/img/"))
        .collect();
    assert_eq!(
        api.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
        ["/search/wario-land-4", "/logo/7", "/picture/7", "/icon/7"]
    );
    assert!(api.iter().all(carried_secret));
    // One download for each image kept, and never the secret.
    let downloads = fixture.requests_to("/img/");
    assert_eq!(
        downloads
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        [
            "/img/logo.png",
            "/img/square.png",
            "/img/poster.png",
            "/img/hero.png",
            "/img/large.png",
        ]
    );
    assert!(downloads.iter().all(|request| !request
        .headers
        .iter()
        .any(|h| h.starts_with("authorization"))));

    let game = fixture.game("Wario Land 4");
    let owner = owner_identity(&game);
    let repository = fixture.repository();
    let pictures = fixture.stored_of(&game, Picture);
    assert_eq!(
        pictures
            .iter()
            .map(|picture| (picture.source.width, picture.source.height))
            .collect::<Vec<_>>(),
        [(512, 512), (600, 900), (1920, 620), (1024, 1024)]
    );
    let hero = &pictures[2];
    assert_eq!(hero.source.provider, ART_PLUGIN_ID);
    assert_eq!(hero.source.id.as_deref(), Some("102"));
    assert_eq!(
        hero.source.url.as_deref(),
        Some(fixture.cdn("/img/hero.png").as_str())
    );
    // The stored bytes are the downloaded bytes, served by asset id.
    assert_eq!(
        repository
            .assigned_asset(&hero.asset_id)
            .unwrap()
            .unwrap()
            .bytes,
        images[2].1
    );
    // The cover is the largest square-ish picture.
    assert_eq!(
        repository.matching_cover_asset_id(&owner).unwrap(),
        Some(asset_id(&images[3].1))
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

    // The whole chain: what the plugin found is what a surface is given,
    // every image with its measured size and colour.
    let surface = fixture.surface("Wario Land 4");
    assert_eq!(
        surface.cover_art_url,
        Some(format!("/game-assets/{}", asset_id(&images[3].1)))
    );
    assert_eq!(surface.system_id.as_deref(), Some("gba"));
    let item = |kind: &str, bytes: &[u8], width: u32, height: u32| {
        serde_json::json!({ "kind": kind, "url": format!("/game-assets/{}", asset_id(bytes)),
            "width": width, "height": height, "color": "#c83c28" })
    };
    assert_eq!(
        serde_json::to_value(&surface.media).unwrap(),
        serde_json::json!([
            item("logo", &images[5].1, 400, 130),
            item("picture", &images[0].1, 512, 512),
            item("picture", &images[1].1, 600, 900),
            item("picture", &images[2].1, 1920, 620),
            item("picture", &images[3].1, 1024, 1024),
        ])
    );
    // Each of those references is an asset the object request serves.
    for media in surface.media.unwrap() {
        let id = media.url.strip_prefix("/game-assets/").unwrap();
        assert!(repository.assigned_asset(id).unwrap().is_some(), "{id}");
    }
}

/// ADR 0091: no art plugin wins by its place in plugin id order. Both
/// plugins are asked, and the game keeps the images of both.
#[test]
fn two_plugins_both_contribute_and_each_keeps_its_own_images() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    // The first plugin by id: a 16:9 picture and a logo.
    fixture.found_at(SECOND_PREFIX, "wario-land-4", 19);
    fixture.list_at(
        SECOND_PREFIX,
        "picture",
        19,
        &[(1, "/img/two-wide.png", 1920, 1080)],
    );
    fixture.list_at(
        SECOND_PREFIX,
        "logo",
        19,
        &[(2, "/img/two-logo.png", 400, 130)],
    );
    fixture.list_at(SECOND_PREFIX, "icon", 19, &[]);
    // The second: two pictures, a logo and an icon.
    fixture.found("wario-land-4", 7);
    fixture.list(
        "picture",
        7,
        &[
            (100, "/img/grid.png", 600, 900),
            (101, "/img/hero.png", 1920, 620),
        ],
    );
    fixture.list("logo", 7, &[(400, "/img/logo.png", 400, 120)]);
    fixture.list("icon", 7, &[(500, "/img/icon.png", 64, 64)]);
    for (path, bytes) in [
        ("/img/two-wide.png", png(1920, 1080)),
        ("/img/two-logo.png", png(400, 130)),
        ("/img/grid.png", png(600, 900)),
        ("/img/hero.png", png(1920, 620)),
        ("/img/logo.png", png(400, 120)),
        ("/img/icon.png", png(64, 64)),
    ] {
        fixture.image(path, bytes);
    }

    let report = fixture.pass_of_two().run();

    assert_eq!((report.attempted, report.stored), (2, 6), "{report:?}");
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let game = fixture.game("Wario Land 4");
    let by_plugin = |kind| {
        fixture
            .stored_of(&game, kind)
            .into_iter()
            .map(|stored| {
                (
                    stored.source.provider,
                    stored.source.width,
                    stored.source.height,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        by_plugin(Picture),
        [
            (SECOND.to_owned(), 1920, 1080),
            (ART_PLUGIN_ID.to_owned(), 600, 900),
            (ART_PLUGIN_ID.to_owned(), 1920, 620),
        ]
    );
    assert_eq!(
        by_plugin(Logo),
        [
            (SECOND.to_owned(), 400, 130),
            (ART_PLUGIN_ID.to_owned(), 400, 120),
        ]
    );
    assert_eq!(by_plugin(Icon), [(ART_PLUGIN_ID.to_owned(), 64, 64)]);
    // Each plugin has its own attempt, in its own folder.
    for plugin in [SECOND, ART_PLUGIN_ID] {
        assert_eq!(
            fixture.outcome_of(plugin, &game),
            Some(AttemptOutcome::Assigned),
            "{plugin}"
        );
    }
    // The surface is given every image of both.
    let media = fixture.surface("Wario Land 4").media.unwrap();
    assert_eq!(media.len(), 6);
    assert_eq!(media.iter().filter(|item| item.kind == Picture).count(), 3);

    // A later answer from one plugin replaces only that plugin's images.
    fixture.advance(30 * DAY);
    fixture.list("picture", 7, &[(102, "/img/new-grid.png", 512, 512)]);
    fixture.image("/img/new-grid.png", png(512, 512));
    fixture
        .server
        .route(&format!("{SECOND_PREFIX}/search/wario-land-4"), status(503));
    let report = fixture.pass_of_two().run();
    assert_eq!(report.attempted, 2);
    assert_eq!(
        by_plugin(Picture),
        [
            (SECOND.to_owned(), 1920, 1080),
            (ART_PLUGIN_ID.to_owned(), 512, 512),
        ]
    );
    assert_eq!(by_plugin(Logo).len(), 2);
    assert_eq!(
        fixture.outcome_of(SECOND, &game),
        Some(AttemptOutcome::Transient)
    );
}

/// ADR 0091's staleness rule: a plugin is asked about a game again only
/// when the game's title, system or public ids changed, when its answer is
/// 30 days old, or when it never answered.
#[test]
fn the_staleness_rule_asks_again_only_when_it_should() {
    let fixture = Fixture::lobby();
    fixture.found("2048gb", 7);
    fixture.list("picture", 7, &[(100, "/img/a.png", 512, 512)]);
    fixture.list("logo", 7, &[]);
    fixture.list("icon", 7, &[]);
    fixture.image("/img/a.png", png(512, 512));
    let asked = || {
        fixture.requests_to("/search/2048gb").len()
            + fixture.requests_to("/search/the-2048gb").len()
    };

    // Never answered: asked. Air Delivery has no match.
    let report = fixture.pass().run();
    assert_eq!((report.attempted, report.stored), (2, 1));
    assert_eq!(asked(), 1);
    let game = fixture.game("2048gb");
    let record = fixture.record_of(ART_PLUGIN_ID, &game).unwrap();
    assert_eq!(record.answered_at, START);
    assert_eq!(
        record.external_ids_sha256,
        external_ids_sha256(&BTreeMap::new())
    );

    // Nothing changed and the answer is new: nobody is asked.
    fixture.pass().clear_retryable_attempts();
    fixture.advance(30 * DAY - 1);
    assert_eq!(fixture.pass().run().attempted, 0);
    assert_eq!(asked(), 1);

    // 30 days old: asked again, whatever the outcome was.
    fixture.advance(1);
    assert_eq!(fixture.pass().run().attempted, 2);
    assert_eq!(asked(), 2);
    assert_eq!(
        fixture.record_of(ART_PLUGIN_ID, &game).unwrap().answered_at,
        START + 30 * DAY
    );
    assert_eq!(fixture.pass().run().attempted, 0);

    // The public ids changed, as when another plugin matched the game:
    // asked again, and only about that game.
    fixture.edit_game("2048gb", |game| {
        game["externalIds"] =
            serde_yaml::to_value(BTreeMap::from([("wikidata", vec!["Q1"])])).unwrap();
    });
    let before = attempt_key(&game);
    let game = fixture.game("2048gb");
    // The ids are not in the fingerprint, so the key is the same.
    assert_eq!(attempt_key(&game), before);
    assert_eq!(fixture.pass().run().attempted, 1);
    assert_eq!(asked(), 3);
    assert_eq!(fixture.pass().run().attempted, 0);

    // The title changed: another key, so asked again.
    fixture.edit_game("2048gb", |game| game["title"] = "The 2048gb".into());
    fixture.found("the-2048gb", 7);
    let renamed = fixture.game("The 2048gb");
    assert_ne!(attempt_key(&game), attempt_key(&renamed));
    assert_eq!(fixture.pass().run().attempted, 1);
    assert_eq!(asked(), 4);
    assert_eq!(fixture.pass().run().attempted, 0);

    // The system changed: another key, so asked again.
    fixture.edit_game("2048gb", |game| {
        game["releases"][0]["system"] = "gbc".into()
    });
    let moved = fixture.game("The 2048gb");
    assert_eq!(moved.system.as_deref(), Some("gbc"));
    assert_ne!(attempt_key(&renamed), attempt_key(&moved));
    assert_eq!(fixture.pass().run().attempted, 1);
    assert_eq!(asked(), 5);
    assert_eq!(fixture.pass().run().attempted, 0);

    // A clock that went back makes the age unknown: asked again.
    fixture.now.store(START, Ordering::SeqCst);
    assert_eq!(fixture.pass().run().attempted, 2);
}

/// The plugin's own matched ids are part of its attempt: storing them does
/// not make it ask again. Another plugin, asked before them, asks once more.
#[test]
fn a_plugins_own_ids_do_not_make_it_ask_again_but_reach_the_other_plugins() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    fixture.server.route(
        "/search/wario-land-4",
        Route::ok("application/json", r#"{"id":7,"ids":{"test":["7"]}}"#),
    );
    for kind in ["picture", "logo", "icon"] {
        fixture.list(kind, 7, &[]);
    }
    fixture
        .server
        .route(&format!("{SECOND_PREFIX}/search/wario-land-4"), status(404));

    let report = fixture.pass_of_two().run();
    assert_eq!(report.attempted, 2);
    let game = fixture.game("Wario Land 4");
    assert_eq!(
        game.external_ids,
        BTreeMap::from([("test".to_owned(), vec!["7".to_owned()])])
    );
    assert_eq!(
        fixture
            .record_of(ART_PLUGIN_ID, &game)
            .unwrap()
            .external_ids_sha256,
        external_ids_sha256(&game.external_ids)
    );
    // `SECOND` was asked before the ids arrived, so it is asked again, once.
    let report = fixture.pass_of_two().run();
    assert_eq!(report.attempted, 1);
    assert_eq!(fixture.requests_to(SECOND_PREFIX).len(), 2);
    assert_eq!(fixture.requests_to("/search/").len(), 1);
    assert_eq!(fixture.pass_of_two().run().attempted, 0);
}

#[test]
fn no_secret_no_call() {
    assert_no_secret_no_call(false);
}

#[test]
fn an_acquired_game_with_no_secret_makes_no_call() {
    assert_no_secret_no_call(true);
}

fn assert_no_secret_no_call(acquired: bool) {
    let fixture = Fixture::new(if acquired { &[] } else { &["Wario Land 4.gba"] });
    if acquired {
        fixture.acquire("Wario Land 4", b"acquired rom");
    }
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
        serde_json::json!([{ "kind": "picture", "source": { "url": fixture.cdn("/img/a.png") } }]),
    );
    fixture.image("/img/a.png", png(256, 256));
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
        assert!(fixture.requests_to("/picture/7").is_empty());
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
fn outcomes_follow_what_the_plugin_and_the_site_did() {
    let fixture = Fixture::new(&["Anguna.gba", "Broken.gba", "Square.gba", "Unknown.gba"]);
    // Anguna: a picture only. A kind with no candidate is not an error.
    fixture.candidates(
        "anguna",
        serde_json::json!([{ "kind": "picture", "source": { "url": fixture.cdn("/img/a.png") } }]),
    );
    fixture.image("/img/a.png", png(256, 256));
    // Broken: the site fails. korrid does not retry; the plugin throws.
    fixture.server.route("/search/broken", status(503));
    // Square: the only candidate is an icon that is not square.
    fixture.candidates(
        "square",
        serde_json::json!([{ "kind": "icon", "source": { "id": "9", "url": fixture.cdn("/img/s.png") } }]),
    );
    fixture.image("/img/s.png", png(64, 32));
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
    let picture = &fixture.stored_of(&fixture.game("Anguna"), Picture)[0];
    assert_eq!(picture.source.id, None);
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

/// An empty answer and an answer that korrid refuses whole both leave the
/// images an earlier answer stored: either can be a site that failed under
/// the plugin.
#[test]
fn a_later_empty_or_refused_answer_keeps_the_earlier_images() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    fixture.candidates(
        "wario-land-4",
        serde_json::json!([
            { "kind": "picture", "source": { "url": fixture.cdn("/img/a.png") } },
            { "kind": "logo", "source": { "url": fixture.cdn("/img/logo.png") } },
        ]),
    );
    fixture.image("/img/a.png", png(512, 512));
    fixture.image("/img/logo.png", png(400, 130));
    assert_eq!(fixture.pass().run().stored, 2);
    let game = fixture.game("Wario Land 4");
    let stored = fixture.stored(&game);

    // 30 days on, nothing matches: the images stay, and the game still
    // holds the plugin's images, so the attempt is `assigned`.
    fixture.advance(30 * DAY);
    fixture.candidates("wario-land-4", serde_json::json!([]));
    assert_eq!(fixture.pass().run().attempted, 1);
    assert_eq!(fixture.stored(&game), stored);
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::NoMatch));

    // 30 days on, only pictures too small and an opaque logo: nothing of
    // either kind passes, so the earlier ones stay.
    fixture.advance(30 * DAY);
    fixture.candidates(
        "wario-land-4",
        serde_json::json!([
            { "kind": "picture", "source": { "url": fixture.cdn("/img/small.png") } },
            { "kind": "logo", "source": { "url": fixture.cdn("/img/opaque.png") } },
        ]),
    );
    fixture.image("/img/small.png", png(64, 64));
    fixture.image(
        "/img/opaque.png",
        test_image(400, 130, ImageFormat::Png, false, [1, 2, 3]),
    );
    let report = fixture.pass().run();
    assert_eq!(
        codes(&report),
        ["AssetUnsupported", "AssetUnsafeDimensions"]
    );
    assert_eq!(fixture.stored(&game), stored);
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));
}

#[test]
fn a_failed_download_keeps_the_earlier_images_of_its_kind_and_a_rescan_asks_again() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    let game = fixture.game("Wario Land 4");
    fixture.candidates(
        "wario-land-4",
        serde_json::json!([
            { "kind": "picture", "source": { "url": fixture.cdn("/img/a.png") } },
            { "kind": "picture", "source": { "url": fixture.cdn("/img/b.png") } },
            { "kind": "logo", "source": { "url": fixture.cdn("/img/logo.png") } },
        ]),
    );
    let (a, b) = (png(512, 512), png(600, 900));
    fixture.image("/img/a.png", a.clone());
    fixture.server.route("/img/b.png", status(500));
    fixture.image("/img/logo.png", png(400, 130));

    let report = fixture.pass().run();

    // The game held no picture, so the one that arrived is stored.
    assert_eq!(report.stored, 2);
    assert_eq!(codes(&report), ["AssetDownloadFailed"]);
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Transient));
    let pictures = |fixture: &Fixture| {
        fixture
            .stored_of(&game, Picture)
            .into_iter()
            .map(|picture| picture.asset_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(pictures(&fixture), [asset_id(&a)]);
    // Without a rescan the game is not asked about again.
    let before = fixture.server.received().len();
    assert_eq!(fixture.pass().run().attempted, 0);
    assert_eq!(fixture.server.received().len(), before);
    // The next rescan asks again and stores both.
    fixture.image("/img/b.png", b.clone());
    fixture.pass().clear_retryable_attempts();
    let report = fixture.pass().run();
    assert_eq!((report.attempted, report.stored), (1, 3));
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));
    assert_eq!(pictures(&fixture), [asset_id(&a), asset_id(&b)]);
    // `assigned` stands: later rescans ask nobody.
    fixture.pass().clear_retryable_attempts();
    let before = fixture.server.received().len();
    assert_eq!(fixture.pass().run().attempted, 0);
    assert_eq!(fixture.server.received().len(), before);

    // 30 days on, a download fails again: the game holds pictures, so they
    // all stay, and the logo, which arrived whole, is replaced.
    fixture.advance(30 * DAY);
    fixture.server.route("/img/b.png", status(500));
    let report = fixture.pass().run();
    assert_eq!(codes(&report), ["AssetDownloadFailed"]);
    assert_eq!(report.stored, 1);
    assert_eq!(pictures(&fixture), [asset_id(&a), asset_id(&b)]);
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Transient));
}

#[test]
fn a_refused_image_alone_never_makes_a_game_transient() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    let game = fixture.game("Wario Land 4");
    // The picture is fine. The icon is not square.
    fixture.candidates(
        "wario-land-4",
        serde_json::json!([
            { "kind": "picture", "source": { "url": fixture.cdn("/img/picture.png") } },
            { "kind": "icon", "source": { "url": fixture.cdn("/img/icon.png") } },
        ]),
    );
    fixture.image("/img/picture.png", png(256, 256));
    fixture.image("/img/icon.png", png(64, 32));

    let report = fixture.pass().run();

    assert_eq!(report.stored, 1);
    assert_eq!(codes(&report), ["AssetUnsafeDimensions"]);
    // A refused kind does not block `assigned` when another kind was stored,
    // so no rescan downloads the refused image again.
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));
    fixture.pass().clear_retryable_attempts();
    let before = fixture.server.received().len();
    assert_eq!(fixture.pass().run().attempted, 0);
    assert_eq!(fixture.server.received().len(), before);
    assert_eq!(fixture.requests_to("/img/icon.png").len(), 1);
}

#[test]
fn an_image_comes_from_a_declared_host_without_a_redirect_and_within_its_size() {
    let fixture = Fixture::new(&["A.gba", "B.gba", "C.gba", "D.gba", "E.gba"]);
    let one = |url: String, size: Option<(u32, u32)>| {
        let mut candidate = serde_json::json!({ "kind": "picture", "source": { "url": url } });
        if let Some((width, height)) = size {
            candidate["width"] = width.into();
            candidate["height"] = height.into();
        }
        serde_json::json!([candidate])
    };
    // A: a host the plugin did not declare.
    fixture.candidates(
        "a",
        one(fixture.server.url("other.art.test", "/img/a.png"), None),
    );
    fixture.image("/img/a.png", png(256, 256));
    // B: the image host redirects.
    fixture.candidates("b", one(fixture.cdn("/img/b.png"), None));
    fixture
        .server
        .route("/img/b.png", Route::redirect("/img/a.png"));
    // C: more bytes than an asset may have.
    fixture.candidates("c", one(fixture.cdn("/img/c.png"), None));
    fixture.image("/img/c.png", vec![0; MAX_ASSET_BYTES + 1]);
    // D: not the size the plugin said.
    fixture.candidates("d", one(fixture.cdn("/img/d.png"), Some((512, 512))));
    fixture.image("/img/d.png", png(256, 256));
    // E: the size the plugin said.
    fixture.candidates("e", one(fixture.cdn("/img/e.png"), Some((256, 256))));
    fixture.image("/img/e.png", png(256, 256));

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
    let url = "https://cdn.art.test/a.png";
    let invalid = [
        // The fixed roles are gone, with no alias (ADR 0091).
        serde_json::json!([{ "role": "tile", "source": { "url": url } }]),
        serde_json::json!([{ "role": "logo", "source": { "url": url } }]),
        serde_json::json!([{ "kind": "picture", "role": "poster", "source": { "url": url } }]),
        serde_json::json!([{ "kind": "tile", "source": { "url": url } }]),
        serde_json::json!([{ "kind": "poster", "source": { "url": url } }]),
        serde_json::json!([{ "kind": "hero", "source": { "url": url } }]),
        serde_json::json!([{ "kind": "banner", "source": { "url": url } }]),
        serde_json::json!([{ "kind": "screenshot", "source": { "url": url } }]),
        serde_json::json!([{ "source": { "url": url } }]),
        // A plugin cannot name the provider, a title, a ratio or anything else.
        serde_json::json!([{ "kind": "picture", "source": { "provider": "@other:art", "url": url } }]),
        serde_json::json!([{ "kind": "picture", "title": "Better Title", "source": { "url": url } }]),
        serde_json::json!([{ "kind": "picture", "ratio": 1.78, "source": { "url": url } }]),
        serde_json::json!([{ "kind": "picture", "source": { "id": "1" } }]),
        serde_json::json!([{ "kind": "picture", "source": { "id": 1, "url": url } }]),
        serde_json::json!([{ "kind": "picture", "source": { "id": null, "url": url } }]),
        serde_json::json!([{ "kind": "picture", "source": { "id": "", "url": url } }]),
        serde_json::json!([{ "kind": "picture", "width": 0, "source": { "url": url } }]),
        serde_json::json!([{ "kind": "picture", "width": "512", "source": { "url": url } }]),
        // Legacy `GameAssetSource.url`: no credentials, query or fragment.
        serde_json::json!([{ "kind": "picture", "source": { "url": "https://user:pw@cdn.art.test/a.png" } }]),
        serde_json::json!([{ "kind": "picture", "source": { "url": "https://cdn.art.test/a.png?key=1" } }]),
        serde_json::json!([{ "kind": "picture", "source": { "url": "https://cdn.art.test/a.png#x" } }]),
        serde_json::json!([{ "kind": "picture", "source": { "url": "/a.png" } }]),
        // One bad candidate refuses the whole answer.
        serde_json::json!([
            { "kind": "picture", "source": { "url": url } },
            { "kind": "banner", "source": { "url": "https://cdn.art.test/b.png" } }
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
        Ok(turns::Turn::Result(GameAssetCandidates { candidates, .. })) if candidates.is_empty()
    ));
    // Each kind is its own name on the wire.
    for kind in SurfaceMediaKind::ALL {
        let candidate: GameAssetCandidate = serde_json::from_value(
            serde_json::json!({ "kind": kind.as_str(), "source": { "url": url } }),
        )
        .unwrap();
        assert_eq!(candidate.kind, kind);
    }
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
    // A record of the fixed roles, as korrid wrote it before ADR 0091.
    let old = serde_json::json!({ "wl4:tile": { "asset_id": format!("{}.png", "a".repeat(64)),
        "role": "tile",
        "owner": { "playable_id": "wl4", "release_id": "r", "release_fingerprint": "f", "rom_identity": "i" },
        "source": { "provider": ART_PLUGIN_ID, "id": "2",
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
fn attempts_from_before_the_migration_stop_the_pass_before_any_request() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    let game = fixture.game("Wario Land 4");
    let folder = fixture.private.path().join(ART_PLUGIN_ID);
    fs::create_dir_all(&folder).unwrap();
    // An attempt without an answer time or a public-ids hash.
    let old = serde_json::json!({ "attempts": { attempt_key(&game): {
        "outcome": "assigned", "playable_id": game.playable_id, "release_id": game.release_id,
        "release_fingerprint": game.release_fingerprint, "rom_identity": game.rom_identity } } })
    .to_string();
    fs::write(folder.join("attempts.json"), &old).unwrap();

    let report = fixture.pass().run();

    assert_eq!(codes(&report), ["EnrichmentStateUnavailable"]);
    assert!(fixture.server.received().is_empty());
    assert_eq!(
        fs::read_to_string(folder.join("attempts.json")).unwrap(),
        old
    );
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

#[test]
fn an_acquired_release_change_drops_the_old_art_and_requests_it_again() {
    let fixture = Fixture::new(&[]);
    fixture.acquire("Wario Land 4", b"first release");
    fixture.candidates(
        "wario-land-4",
        serde_json::json!([
            { "kind": "picture", "source": { "url": fixture.cdn("/img/poster.png") } }
        ]),
    );
    fixture.image("/img/poster.png", png(600, 900));
    assert_eq!(fixture.pass().run().stored, 1);
    let before = fixture.game("Wario Land 4");
    assert!(fixture.surface("Wario Land 4").media.is_some());

    fixture.replace_acquired_release("Wario Land 4", b"second release");
    let after = fixture.game("Wario Land 4");
    assert_eq!(before.playable_id, after.playable_id);
    assert_ne!(before.release_id, after.release_id);
    assert_ne!(before.rom_identity, after.rom_identity);
    assert_ne!(before.release_fingerprint, after.release_fingerprint);
    assert!(fixture.stored(&after).is_empty());
    assert!(fixture.surface("Wario Land 4").media.is_none());

    let report = fixture.pass().run();
    assert_eq!((report.attempted, report.stored), (1, 1));
    assert_eq!(fixture.requests_to("/search/wario-land-4").len(), 2);
    assert_eq!(fixture.outcome(&after), Some(AttemptOutcome::Assigned));
    assert!(fixture.surface("Wario Land 4").media.is_some());
    // The new release's picture replaced the old one's record.
    assert_eq!(fixture.stored(&before), []);
}

#[test]
fn an_acquired_release_that_changes_during_download_does_not_store_stale_art() {
    let fixture = Fixture::new(&[]);
    fixture.acquire("Wario Land 4", b"first release");
    fixture.candidates(
        "wario-land-4",
        serde_json::json!([
            { "kind": "picture", "source": { "url": fixture.cdn("/img/poster.png") } }
        ]),
    );
    fixture.server.route(
        "/img/poster.png",
        Route::ok("image/png", png(600, 900)).paced(256, Duration::from_millis(30)),
    );
    let before = fixture.game("Wario Land 4");
    let pass = fixture.pass();
    let running = std::thread::spawn(move || pass.run());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fixture.requests_to("/img/poster.png").is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "image download did not start"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    fixture.replace_acquired_release("Wario Land 4", b"second release");
    let report = running.join().unwrap();
    assert_eq!((report.attempted, report.stored), (1, 0));
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(fixture.outcome(&before), None);
    let after = fixture.game("Wario Land 4");
    assert!(fixture.stored(&after).is_empty());
    fixture.image("/img/poster.png", png(600, 900));
    assert_eq!(fixture.pass().run().stored, 1);
}

#[test]
fn library_lobby_games_get_art_without_downloads_or_installed_declarations() {
    let fixture = Fixture::lobby();
    for slug in ["2048gb", "air-delivery"] {
        fixture.candidates(
            slug,
            serde_json::json!([
                { "kind": "picture", "source": { "url": fixture.cdn("/img/poster.png") } }
            ]),
        );
    }
    fixture.image("/img/poster.png", png(600, 900));
    assert!(!fixture.readable.path().join("cache").exists());
    fixture.secrets().clear(ART_PLUGIN_ID).unwrap();
    let missing = fixture.pass().run();
    assert_eq!(codes(&missing), ["CredentialMissing"]);
    assert_eq!((missing.attempted, missing.stored), (0, 0));
    assert!(fixture.server.received().is_empty());
    fixture.secrets().set(ART_PLUGIN_ID, SECRET).unwrap();
    let report = fixture.pass().run();
    assert_eq!((report.attempted, report.stored), (2, 2));
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    for title in ["2048gb", "Air Delivery"] {
        let game = fixture.game(title);
        assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));
        assert_eq!(fixture.stored_of(&game, Picture).len(), 1);
    }
    assert_eq!(fixture.requests_to("/search/").len(), 2);
    assert_eq!(fixture.pass().run(), ArtworkReport::default());

    // Even a release-record edit that keeps the content hash must drop art.
    let before = fixture.game("2048gb");
    fixture.edit_game("2048gb", |game| game["releases"][0]["id"] = "gb".into());
    let after = fixture.game("2048gb");
    assert_eq!(before.release_id, after.release_id);
    assert_ne!(before.release_fingerprint, after.release_fingerprint);
    assert!(fixture.stored(&after).is_empty());
    let report = fixture.pass().run();
    assert_eq!((report.attempted, report.stored), (1, 1));
    assert_eq!(fixture.requests_to("/search/2048gb").len(), 2);
}

/// ADR 0090: the ids of a match go into the game's record, the art stored
/// with them stays visible, and the next lookup goes by the stored id.
#[test]
fn a_matched_id_is_stored_keeps_the_art_and_the_next_lookup_goes_by_id() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    fixture.server.route(
        "/search/wario-land-4",
        Route::ok("application/json", r#"{"id":7,"ids":{"test":["7"]}}"#),
    );
    fixture.list("logo", 7, &[(400, "/img/logo.png", 40, 13)]);
    fixture.image("/img/logo.png", png(40, 13));

    let report = fixture.pass().run();

    assert_eq!((report.attempted, report.stored), (1, 1), "{report:?}");
    let game = fixture.game("Wario Land 4");
    assert_eq!(
        game.external_ids,
        BTreeMap::from([("test".to_owned(), vec!["7".to_owned()])])
    );
    let library = fs::read_to_string(fixture.library_path()).unwrap();
    assert!(library.contains("externalIds"), "{library}");
    // Writing the ids changed no fingerprint: the stored logo still shows.
    assert_eq!(
        fixture
            .stored(&game)
            .iter()
            .map(|stored| stored.kind)
            .collect::<Vec<_>>(),
        [Logo]
    );
    assert_eq!(fixture.outcome(&game), Some(AttemptOutcome::Assigned));

    // Another dump of the game is asked about again, by its stored id.
    fs::write(
        fixture.roms.path().join("Wario Land 4.gba"),
        b"another dump",
    )
    .unwrap();
    fixture.discovery.rescan(&options()).unwrap();
    assert_eq!(
        fixture.game("Wario Land 4").external_ids,
        BTreeMap::from([("test".to_owned(), vec!["7".to_owned()])])
    );
    fixture
        .server
        .route("/id/7", Route::ok("application/json", r#"{"id":7}"#));
    assert_eq!(fixture.pass().run().attempted, 1);
    assert_eq!(fixture.requests_to("/id/7").len(), 1);
    assert_eq!(fixture.requests_to("/search/").len(), 1);
}

/// A plugin's ids that korrid cannot store are a provider failure.
#[test]
fn ids_korrid_cannot_store_are_a_provider_failure() {
    let fixture = Fixture::new(&["Wario Land 4.gba"]);
    fixture.server.route(
        "/search/wario-land-4",
        Route::ok("application/json", r#"{"id":7,"ids":{"test":[]}}"#),
    );
    let report = fixture.pass().run();
    assert_eq!(codes(&report), ["ProviderFailed"]);
    assert!(fixture.game("Wario Land 4").external_ids.is_empty());
}
