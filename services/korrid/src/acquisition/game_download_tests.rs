//! Declared hosts, SSRF, text bounds and game payload rules on a real server.
use super::{
    download::{DownloadEngine, TextRequest, MAX_TEXT_BYTES, STAGING_PREFIX},
    http_fixture::{HttpFixture, Route},
    Failure,
};
use sha2::{Digest, Sha256};
use std::{net::IpAddr, time::Duration};
use url::Url;

const HOST: &str = "provider.test";
const OTHER: &str = "elsewhere.test";

fn engine() -> DownloadEngine {
    let loopback: IpAddr = "127.0.0.1".parse().unwrap();
    DownloadEngine::for_test(Duration::from_secs(5))
        .with_name(HOST, loopback)
        .with_name(OTHER, loopback)
        .with_name("private.test", "10.0.0.1".parse().unwrap())
}

fn hosts() -> Vec<String> {
    vec![HOST.into()]
}

async fn text(
    engine: &DownloadEngine,
    url: &str,
) -> Result<super::download::TextResponse, Failure> {
    engine
        .fetch_text(
            TextRequest {
                url: Url::parse(url).unwrap(),
                headers: &[("accept".into(), "text/html".into())],
                timeout: None,
            },
            &hosts(),
        )
        .await
}

#[tokio::test]
async fn provider_requests_reach_only_declared_hosts_on_every_hop() {
    let server = HttpFixture::start([
        (
            "/page",
            Route::ok("text/html", "<p>hi</p>")
                .header("X-Twice", "a")
                .header("X-Twice", "b"),
        ),
        ("/hop", Route::redirect("/page")),
    ]);
    server.route("/away", Route::redirect(&server.url(OTHER, "/page")));
    let engine = engine();

    let response = text(&engine, &server.url(HOST, "/hop")).await.unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.url, server.url(HOST, "/page"));
    assert_eq!(response.text, "<p>hi</p>");
    assert_eq!(response.headers["x-twice"], "a, b");
    assert_eq!(response.headers["content-type"], "text/html");
    assert!(server.received()[0]
        .headers
        .contains(&"accept: text/html".to_owned()));

    // The undeclared host is refused before any connection.
    let before = server.received().len();
    assert_eq!(
        text(&engine, &server.url(OTHER, "/page")).await.err(),
        Some(Failure::UndeclaredHost)
    );
    assert_eq!(server.received().len(), before);
    // A redirect hop to an undeclared host is refused before it is followed.
    assert_eq!(
        text(&engine, &server.url(HOST, "/away")).await.err(),
        Some(Failure::UndeclaredHost)
    );
    assert_eq!(server.received().len(), before + 1);

    // SSRF still applies to a declared name that resolves to a private address.
    let private = vec!["private.test".to_owned()];
    let refused = engine
        .fetch_text(
            TextRequest {
                url: Url::parse("http://private.test/").unwrap(),
                headers: &[],
                timeout: None,
            },
            &private,
        )
        .await;
    assert_eq!(refused.err(), Some(Failure::UnsafeUrl));
    // An empty list means no network.
    let none = engine
        .fetch_text(
            TextRequest {
                url: Url::parse(&server.url(HOST, "/page")).unwrap(),
                headers: &[],
                timeout: None,
            },
            &[],
        )
        .await;
    assert_eq!(none.err(), Some(Failure::UndeclaredHost));
}

#[tokio::test]
async fn provider_text_is_capped_and_error_statuses_are_responses() {
    let server = HttpFixture::start([
        (
            "/big",
            Route::ok("text/plain", vec![b'x'; MAX_TEXT_BYTES as usize + 1]),
        ),
        (
            "/limit",
            Route::ok("text/plain", vec![b'x'; MAX_TEXT_BYTES as usize]),
        ),
        ("/bytes", Route::ok("text/plain", vec![b'a', 0xff, b'b'])),
    ]);
    let engine = engine();
    assert_eq!(
        text(&engine, &server.url(HOST, "/big")).await.err(),
        Some(Failure::TooLarge)
    );
    assert_eq!(
        text(&engine, &server.url(HOST, "/limit"))
            .await
            .unwrap()
            .text
            .len(),
        MAX_TEXT_BYTES as usize
    );
    assert_eq!(
        text(&engine, &server.url(HOST, "/bytes"))
            .await
            .unwrap()
            .text,
        "a\u{fffd}b"
    );
    let missing = text(&engine, &server.url(HOST, "/missing")).await.unwrap();
    assert_eq!((missing.status, missing.text.as_str()), (404, "missing"));
}

/// The cap also holds for a body with no Content-Length, counted as it arrives.
#[tokio::test]
async fn the_game_cap_holds_for_a_body_without_a_length() {
    let server = HttpFixture::start([
        (
            "/big.gb",
            Route::ok("application/octet-stream", vec![1u8; 4096]).unsized_body(),
        ),
        (
            "/fits.gb",
            Route::ok("application/octet-stream", vec![1u8; 1024]).unsized_body(),
        ),
    ]);
    let engine = engine().with_game_cap(1024);
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        engine
            .fetch_game(
                &server.url(HOST, "/big.gb"),
                &hosts(),
                directory.path(),
                |_| {}
            )
            .await
            .err(),
        Some(Failure::TooLarge)
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    let fits = engine
        .fetch_game(
            &server.url(HOST, "/fits.gb"),
            &hosts(),
            directory.path(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(fits.bytes, 1024);
}

#[tokio::test]
async fn game_payloads_refuse_web_pages_empty_bodies_and_the_cap_and_leave_nothing() {
    let rom = b"\x00\x01game bytes".repeat(100);
    let server = HttpFixture::start([
        (
            "/rom.gb",
            Route::ok("application/octet-stream", rom.clone()),
        ),
        (
            "/typed.gb",
            Route::ok("text/html; charset=utf-8", rom.clone()),
        ),
        (
            "/lead.gb",
            Route::ok(
                "application/octet-stream",
                "\u{feff}  <!DOCTYPE html><html></html>",
            ),
        ),
        (
            "/lead-short.gb",
            Route::ok("application/octet-stream", "<html>"),
        ),
        ("/empty.gb", Route::ok("application/octet-stream", "")),
        (
            "/huge.gb",
            Route::ok("application/octet-stream", "").header("Content-Length", "2147483649"),
        ),
    ]);
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    for (path, failure) in [
        ("/typed.gb", Failure::WebPage),
        ("/lead.gb", Failure::WebPage),
        ("/lead-short.gb", Failure::WebPage),
        ("/empty.gb", Failure::Empty),
        ("/huge.gb", Failure::TooLarge),
        ("/missing", Failure::Network),
    ] {
        let result = engine
            .fetch_game(&server.url(HOST, path), &hosts(), directory.path(), |_| {})
            .await;
        assert_eq!(result.err(), Some(failure), "{path}");
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            0,
            "{path}"
        );
    }
    assert_eq!(
        engine
            .fetch_game(
                &server.url(OTHER, "/rom.gb"),
                &hosts(),
                directory.path(),
                |_| {}
            )
            .await
            .err(),
        Some(Failure::UndeclaredHost)
    );

    let mut progress = Vec::new();
    let staged = engine
        .fetch_game(
            &server.url(HOST, "/rom.gb"),
            &hosts(),
            directory.path(),
            |p| progress.push(p.done),
        )
        .await
        .unwrap();
    assert_eq!(staged.bytes, rom.len() as u64);
    assert_eq!(staged.sha256, hex::encode(Sha256::digest(&rom)));
    assert_eq!(std::fs::read(&staged.path).unwrap(), rom);
    assert!(staged
        .path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with(STAGING_PREFIX));
    assert_eq!(progress.last(), Some(&(rom.len() as u64)));
    let path = staged.path.clone();
    drop(staged);
    assert!(!path.exists());
}
