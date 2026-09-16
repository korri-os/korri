use super::asset_download::{download_image, download_image_with_policy, is_public_ip};
use super::*;
use crate::{config::settings, discovery::reconcile::DiscoveryCoordinator};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Condvar, Mutex},
    thread,
};

const PNG_1X1: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4, 0,
    0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 250, 207, 0, 0, 2, 7, 1, 2,
    154, 28, 49, 113, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn discovery_options() -> discovery::DiscoveryOptions {
    discovery::DiscoveryOptions {
        first_seen_at: "2026-08-05T00:00:00Z".into(),
        max_diagnostics: 100,
        max_candidates: 100,
        ..discovery::DiscoveryOptions::default()
    }
}

fn api_server(
    responses: Vec<&'static str>,
) -> (Url, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!(
        "http://{}/api/v2/",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let responses = Arc::new(Mutex::new(responses));
    let handle = thread::spawn(move || {
        while !responses.lock().unwrap().is_empty() {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let read = stream.read(&mut buffer).unwrap();
            captured
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buffer[..read]).into_owned());
            let response = responses.lock().unwrap().remove(0);
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (base, requests, handle)
}

fn delayed_api_server(
    responses: Vec<&'static str>,
    delay: Duration,
) -> (Url, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!(
        "http://{}/api/v2/",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let responses = Arc::new(Mutex::new(responses));
    let handle = thread::spawn(move || {
        while !responses.lock().unwrap().is_empty() {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let read = stream.read(&mut buffer).unwrap();
            captured
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buffer[..read]).into_owned());
            thread::sleep(delay);
            let response = responses.lock().unwrap().remove(0);
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (base, requests, handle)
}

fn byte_server(responses: Vec<Vec<u8>>) -> (Url, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let responses = Arc::new(Mutex::new(responses));
    let handle = thread::spawn(move || {
        while !responses.lock().unwrap().is_empty() {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let read = stream.read(&mut buffer).unwrap();
            captured
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buffer[..read]).into_owned());
            let response = responses.lock().unwrap().remove(0);
            stream.write_all(&response).unwrap();
        }
    });
    (base, requests, handle)
}

fn http_json(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    )
}

fn http_bytes(content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

fn http_redirect(location: &str) -> Vec<u8> {
    format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n").into_bytes()
}

fn options_with_base(base: Url, batch_limit: usize) -> EnrichmentOptions {
    EnrichmentOptions {
        api_base_url: base,
        batch_limit,
        retry_limit: 0,
        max_image_bytes: 1024,
        retry_after_delay: Arc::new(|_| {}),
        asset_download_policy: AssetDownloadPolicy::default(),
    }
}

fn options_with_local_assets(base: Url, batch_limit: usize) -> EnrichmentOptions {
    EnrichmentOptions {
        asset_download_policy: AssetDownloadPolicy {
            allow_http_loopback: true,
            resolver: Arc::new(|host, port| {
                format!("{host}:{port}")
                    .parse()
                    .map_err(|_| EnrichmentDiagnostic {
                        code: "AssetUrlRejected",
                        message: "asset host could not be resolved".into(),
                        playable_id: None,
                    })
            }),
            resolver_timeout: asset_download::ASSET_RESOLUTION_TIMEOUT,
        },
        ..options_with_base(base, batch_limit)
    }
}

#[test]
fn exact_match_requires_one_verified_normalized_result() {
    let query = title::normalized_match_name("Wario Land 4");
    assert!(matches!(
        exact_verified_match(
            &query,
            &[SearchGame {
                id: 1,
                name: "Wario Land 4".into(),
                verified: true
            }]
        ),
        MatchDecision::Accepted(_)
    ));
    assert_eq!(
        exact_verified_match(
            &query,
            &[SearchGame {
                id: 1,
                name: "Wario Land 4".into(),
                verified: false
            }]
        ),
        MatchDecision::Unverified
    );
    assert_eq!(
        exact_verified_match(
            &query,
            &[
                SearchGame {
                    id: 1,
                    name: "Wario Land 4".into(),
                    verified: true
                },
                SearchGame {
                    id: 2,
                    name: "Wario-Land 4".into(),
                    verified: true
                },
            ]
        ),
        MatchDecision::Ambiguous
    );
    assert_eq!(
        exact_verified_match(
            &query,
            &[SearchGame {
                id: 1,
                name: "Wario Land".into(),
                verified: true
            }]
        ),
        MatchDecision::NoMatch
    );
}

#[test]
fn chooses_highest_ranked_static_safe_square_grid() {
    let grids = vec![
        Grid {
            id: 1,
            url: Url::parse("https://example.com/1.png").unwrap(),
            score: Some(100),
            tags: vec!["humor".into()],
            style: None,
        },
        Grid {
            id: 2,
            url: Url::parse("https://example.com/2.png").unwrap(),
            score: Some(10),
            tags: vec!["nsfw".into()],
            style: None,
        },
        Grid {
            id: 3,
            url: Url::parse("https://example.com/3.png").unwrap(),
            score: Some(50),
            tags: Vec::new(),
            style: None,
        },
    ];
    assert_eq!(choose_grid(&grids).unwrap().id, 3);
}

#[test]
fn decodes_spec_grid_response_fields_without_provider_dimensions() {
    let response: GridResponse = serde_json::from_str(
        r#"{"data":[{"id":10,"score":20,"style":"alternate","url":"https://example.com/grid.png","thumb":"https://example.com/thumb.png","tags":[]}]}"#,
    )
    .unwrap();

    assert_eq!(choose_grid(&response.data).unwrap().id, 10);
}

#[test]
fn chooses_spec_response_grid_without_declared_dimensions() {
    let grids = vec![Grid {
        id: 10,
        url: Url::parse("https://example.com/grid.png").unwrap(),
        score: Some(20),
        tags: Vec::new(),
        style: Some("alternate".into()),
    }];
    assert_eq!(choose_grid(&grids).unwrap().id, 10);
}

#[test]
fn grids_request_uses_provider_supported_square_dimensions_and_styles() {
    let (base, requests, handle) = api_server(vec![
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"data\":[]}",
    ]);
    let client = api_client(&base).unwrap();

    let grids = grids_for_game(&client, &base, "secret-token-123", 42).unwrap();
    handle.join().unwrap();

    assert!(grids.is_empty());
    let captured = requests.lock().unwrap().join("\n");
    assert!(captured.contains("dimensions=512x512"), "{captured}");
    let request_target = captured
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .expect("request target");
    let request_url =
        Url::parse(&format!("http://localhost{request_target}")).expect("captured request URL");
    let styles = request_url
        .query_pairs()
        .find_map(|(key, value)| (key == "styles").then_some(value))
        .expect("styles query");
    assert_eq!(
        styles.split(',').collect::<BTreeSet<_>>(),
        BTreeSet::from(["alternate", "blurred", "material", "no_logo"])
    );
    assert!(!captured.contains("dimensions=1x1"), "{captured}");
}

#[test]
fn rejects_private_and_ipv4_mapped_asset_destinations_before_request() {
    let url = Url::parse("https://127.0.0.1/asset.png").unwrap();
    let error = download_image(&url, 1024).unwrap_err();
    assert_eq!(error.code, "AssetUrlRejected");
    assert!(!is_public_ip("::ffff:127.0.0.1".parse().unwrap()));
    assert!(!is_public_ip("::ffff:10.0.0.1".parse().unwrap()));
}

#[test]
fn asset_resolver_timeout_returns_sanitized_transient_download_diagnostic() {
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let resolver_release = release.clone();
    let policy = AssetDownloadPolicy {
        allow_http_loopback: false,
        resolver: Arc::new(move |_, _| {
            let (lock, condition) = &*resolver_release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = condition.wait(released).unwrap();
            }
            Ok("93.184.216.34:443".parse().unwrap())
        }),
        resolver_timeout: Duration::from_millis(10),
    };
    let started = std::time::Instant::now();

    let error = download_image_with_policy(
        &Url::parse("https://slow-resolver.invalid/asset.png").unwrap(),
        1024,
        &policy,
    )
    .unwrap_err();

    {
        let (lock, condition) = &*release;
        let mut released = lock.lock().unwrap();
        *released = true;
        condition.notify_all();
    }
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "asset resolution did not return at the resolver deadline"
    );
    assert_eq!(error.code, "AssetDownloadFailed");
    assert_eq!(error.message, "asset host resolution timed out");
    assert_eq!(error.playable_id, None);
}

#[test]
fn unauthorized_credential_stops_after_one_redacted_provider_error() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("one.gba"), b"one").unwrap();
    std::fs::write(root.path().join("two.gba"), b"two").unwrap();
    DiscoveryCoordinator::for_tests(readable.path(), private.path())
        .add_location(root.path(), &discovery_options())
        .unwrap();
    settings::set_steamgriddb_credential(private.path(), "secret-token-123").unwrap();
    let (base, requests, handle) = api_server(vec![
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n",
    ]);

    let report = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options_with_base(base, 10),
    )
    .run();
    handle.join().unwrap();

    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(report.diagnostics.len(), 1);
    let rendered = format!("{:?}", report.diagnostics);
    assert!(!rendered.contains("secret-token-123"));
    assert!(!rendered.contains("Bearer"));
    assert_eq!(
        report.diagnostics[0].code,
        "SteamGridDbCredentialUnauthorized"
    );
}

#[test]
fn matching_assignment_repairs_missing_attempt_without_provider_call() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    DiscoveryCoordinator::for_tests(readable.path(), private.path())
        .add_location(root.path(), &discovery_options())
        .unwrap();
    let game = discovery::reconcile::owned_discovery_games(readable.path(), private.path())
        .unwrap()
        .pop()
        .unwrap();
    GameAssetRepository::new(private.path())
        .assign_tile(
            owner_identity(&game),
            AssetCandidate {
                bytes: PNG_1X1.to_vec(),
                declared_width: Some(1),
                declared_height: Some(1),
                game_id: 1,
                grid_id: 2,
            },
        )
        .unwrap();
    settings::set_steamgriddb_credential(private.path(), "secret-token-123").unwrap();

    let report = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options_with_base(Url::parse("http://127.0.0.1:9/api/v2/").unwrap(), 1),
    )
    .run();

    assert_eq!(report.attempted, 1);
    assert_eq!(report.assigned, 1);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn unreadable_assignment_state_stops_before_provider_request() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    DiscoveryCoordinator::for_tests(readable.path(), private.path())
        .add_location(root.path(), &discovery_options())
        .unwrap();
    let assignment_dir = private.path().join("game-assets");
    std::fs::create_dir_all(&assignment_dir).unwrap();
    std::fs::write(assignment_dir.join("assignments.json"), b"not-json").unwrap();
    settings::set_steamgriddb_credential(private.path(), "secret-token-123").unwrap();

    let report = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options_with_base(Url::parse("http://127.0.0.1:9/api/v2/").unwrap(), 1),
    )
    .run();

    assert_eq!(report.attempted, 1);
    assert_eq!(report.diagnostics[0].code, "AssetStorageUnavailable");
}

#[test]
fn permanent_provider_failure_is_not_retried_until_provider_configuration_changes() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    DiscoveryCoordinator::for_tests(readable.path(), private.path())
        .add_location(root.path(), &discovery_options())
        .unwrap();
    settings::set_steamgriddb_credential(private.path(), "secret-token-123").unwrap();
    let (base, requests, handle) = api_server(vec![
        "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n",
    ]);
    let options = options_with_base(base, 1);

    let first = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options.clone(),
    )
    .run();
    handle.join().unwrap();
    let second = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options,
    )
    .run();
    SteamGridDbEnricher::clear_non_assigned_attempts(private.path()).unwrap();

    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(first.diagnostics[0].code, "SteamGridDbPermanentFailure");
    assert_eq!(second.attempted, 0);
}

#[test]
fn changed_same_id_rom_ignores_stale_attempt_and_enriches_current_identity() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let rom = root.path().join("wl4.gba");
    std::fs::write(&rom, b"rom-one").unwrap();
    let discovery = DiscoveryCoordinator::for_tests(readable.path(), private.path());
    discovery
        .add_location(root.path(), &discovery_options())
        .unwrap();
    settings::set_steamgriddb_credential(private.path(), "secret-token-123").unwrap();
    let no_match = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"data\":[]}";
    let (base, requests, handle) = api_server(vec![no_match, no_match]);
    let options = options_with_base(base, 1);

    let first = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options.clone(),
    )
    .run();
    std::fs::write(&rom, b"rom-two").unwrap();
    discovery.rescan(&discovery_options()).unwrap();
    let second = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options,
    )
    .run();
    handle.join().unwrap();

    assert_eq!(first.attempted, 1);
    assert_eq!(second.attempted, 1);
    assert_eq!(requests.lock().unwrap().len(), 2);
}

#[test]
fn provider_success_updates_title_assigns_identity_bound_tile_and_records_attempt() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Wario_Land_4.gba"), b"rom").unwrap();
    DiscoveryCoordinator::for_tests(readable.path(), private.path())
        .add_location(root.path(), &discovery_options())
        .unwrap();
    settings::set_steamgriddb_credential(private.path(), "secret-token-123").unwrap();
    let asset_response = http_bytes("image/png", PNG_1X1);
    let (asset_base, asset_requests, asset_handle) = byte_server(vec![asset_response]);
    let asset_url = asset_base.join("grid.png").unwrap();
    let search = http_json(r#"{"data":[{"id":123,"name":"Wario Land 4","verified":true}]}"#);
    let grids = http_json(&format!(
        r#"{{"data":[{{"id":456,"score":99,"style":"alternate","url":"{asset_url}","thumb":"{asset_url}","tags":[]}}]}}"#
    ));
    let (base, api_requests, api_handle) = api_server(vec![
        Box::leak(search.into_boxed_str()),
        Box::leak(grids.into_boxed_str()),
    ]);

    let report = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options_with_local_assets(base, 1),
    )
    .run();
    api_handle.join().unwrap();
    asset_handle.join().unwrap();

    assert_eq!(report.attempted, 1);
    assert_eq!(report.assigned, 1);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let captured_api = api_requests.lock().unwrap().join("\n");
    assert!(
        captured_api.contains("GET /api/v2/search/autocomplete/wario%20land%204?types=game"),
        "{captured_api}"
    );
    assert!(captured_api.contains("GET /api/v2/grids/game/123?dimensions=512x512"));
    assert_eq!(asset_requests.lock().unwrap().len(), 1);
    let updated = discovery::reconcile::owned_discovery_games(readable.path(), private.path())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(updated.title, "Wario Land 4");
    let repo = GameAssetRepository::new(private.path());
    let assignment = repo
        .matching_assignment(&owner_identity(&updated))
        .unwrap()
        .unwrap();
    assert_eq!(assignment.source.game_id, 123);
    assert_eq!(assignment.source.grid_id, 456);
    assert_eq!(assignment.source.width, 1);
    assert_eq!(assignment.source.height, 1);
    assert!(private
        .path()
        .join("game-assets")
        .join("blobs")
        .join(&assignment.asset_id)
        .is_file());
    let attempts = read_attempt_state(private.path()).unwrap();
    assert!(attempts
        .attempts
        .values()
        .any(|record| matches!(record.outcome, AttemptOutcome::Assigned)));
}

#[test]
fn asset_redirect_is_rejected_after_exactly_one_asset_request() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Wario_Land_4.gba"), b"rom").unwrap();
    DiscoveryCoordinator::for_tests(readable.path(), private.path())
        .add_location(root.path(), &discovery_options())
        .unwrap();
    settings::set_steamgriddb_credential(private.path(), "secret-token-123").unwrap();
    let (asset_base, asset_requests, asset_handle) =
        byte_server(vec![http_redirect("http://127.0.0.1/other.png")]);
    let asset_url = asset_base.join("grid.png").unwrap();
    let search = http_json(r#"{"data":[{"id":123,"name":"Wario Land 4","verified":true}]}"#);
    let grids = http_json(&format!(
        r#"{{"data":[{{"id":456,"score":99,"style":"alternate","url":"{asset_url}","tags":[]}}]}}"#
    ));
    let (base, _api_requests, api_handle) = api_server(vec![
        Box::leak(search.into_boxed_str()),
        Box::leak(grids.into_boxed_str()),
    ]);

    let report = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options_with_local_assets(base, 1),
    )
    .run();
    api_handle.join().unwrap();
    asset_handle.join().unwrap();

    assert_eq!(asset_requests.lock().unwrap().len(), 1);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "AssetRedirectRejected");
}

#[test]
fn concurrent_credential_change_prevents_stale_attempt_from_returning() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("wl4.gba"), b"rom").unwrap();
    DiscoveryCoordinator::for_tests(readable.path(), private.path())
        .add_location(root.path(), &discovery_options())
        .unwrap();
    settings::set_steamgriddb_credential(private.path(), "old-token").unwrap();
    let no_match = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"data\":[]}";
    let (base, requests, handle) = delayed_api_server(vec![no_match], Duration::from_millis(100));
    let lock = Arc::new(Mutex::new(()));
    let enricher = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        lock.clone(),
        options_with_base(base, 1),
    );
    let worker = thread::spawn(move || enricher.run());
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while requests.lock().unwrap().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "provider was not called"
        );
        thread::sleep(Duration::from_millis(5));
    }
    {
        let _guard = lock.lock().unwrap();
        settings::set_steamgriddb_credential(private.path(), "new-token").unwrap();
        SteamGridDbEnricher::clear_non_assigned_attempts(private.path()).unwrap();
    }

    let report = worker.join().unwrap();
    handle.join().unwrap();

    assert_eq!(report.attempted, 1);
    assert!(read_attempt_state(private.path())
        .unwrap()
        .attempts
        .is_empty());
}

#[test]
fn finite_batch_attempts_no_matches_so_later_restart_resumes() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("one.gba"), b"one").unwrap();
    std::fs::write(root.path().join("two.gba"), b"two").unwrap();
    DiscoveryCoordinator::for_tests(readable.path(), private.path())
        .add_location(root.path(), &discovery_options())
        .unwrap();
    settings::set_steamgriddb_credential(private.path(), "secret-token-123").unwrap();
    let no_match = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"data\":[]}";
    let (base, requests, handle) = api_server(vec![no_match, no_match]);
    let options = options_with_base(base, 1);

    let first = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options.clone(),
    )
    .run();
    let second = SteamGridDbEnricher::with_options(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        options,
    )
    .run();
    handle.join().unwrap();

    assert_eq!(first.attempted, 1);
    assert_eq!(second.attempted, 1);
    assert_eq!(requests.lock().unwrap().len(), 2);
}
