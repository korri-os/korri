//! Declared hosts, SSRF, text bounds and game payload rules on a real server.
use super::{
    download::{DownloadEngine, GameRequest, TextRequest, MAX_TEXT_BYTES, STAGING_PREFIX},
    http_fixture::{HttpFixture, Route},
    Failure,
};
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
    // ADR 0030: the copy no longer names the removed 2 GiB download cap.
    assert_eq!(
        Failure::TooLarge.rpc().message,
        "The provider sent more data than Korri accepts."
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

/// ADR 0030: no size cap. korrid counts the bytes it writes and stops before
/// the cache's file system falls below 5% free, also with no Content-Length.
#[tokio::test]
async fn the_free_space_floor_holds_for_a_body_without_a_length() {
    let server = HttpFixture::start([
        (
            "/big.gb",
            Route::ok("application/octet-stream", vec![1u8; 4096]).unsized_body(),
        ),
        (
            "/fits.gb",
            Route::ok("application/octet-stream", vec![1u8; 2048]).unsized_body(),
        ),
    ]);
    let engine = engine().with_space(super::space::fixed(2048));
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        engine
            .fetch_game(
                &GameRequest::get(server.url(HOST, "/big.gb")),
                &hosts(),
                directory.path(),
                |_| {}
            )
            .await
            .err(),
        Some(Failure::NoSpace)
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    let fits = engine
        .fetch_game(
            &GameRequest::get(server.url(HOST, "/fits.gb")),
            &hosts(),
            directory.path(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(fits.bytes, 2048);
}

/// A Content-Length larger than the space above the floor can never finish:
/// it fails before the first byte. A length over the old 2 GiB cap is no
/// longer refused when the space is there.
#[tokio::test]
async fn a_content_length_beyond_the_floor_fails_before_any_byte_and_2_gib_is_no_cap() {
    let server = HttpFixture::start([
        (
            "/declared.gb",
            Route::ok("application/octet-stream", vec![1u8; 4096]),
        ),
        (
            "/huge.gb",
            Route::ok("application/octet-stream", "").header("Content-Length", "2147483649"),
        ),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let mut progress = 0;
    assert_eq!(
        engine()
            .with_space(super::space::fixed(4095))
            .fetch_game(
                &GameRequest::get(server.url(HOST, "/declared.gb")),
                &hosts(),
                directory.path(),
                |_| progress += 1,
            )
            .await
            .err(),
        Some(Failure::NoSpace)
    );
    assert_eq!(progress, 0, "no byte was written");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    let result = engine()
        .with_space(super::space::fixed(4 * 1024 * 1024 * 1024))
        .fetch_game(
            &GameRequest::get(server.url(HOST, "/huge.gb")),
            &hosts(),
            directory.path(),
            |_| {},
        )
        .await;
    assert!(
        !matches!(result, Err(Failure::NoSpace | Failure::TooLarge) | Ok(_)),
        "{:?}",
        result.err()
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn game_payloads_refuse_web_pages_and_empty_bodies_and_leave_nothing() {
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
    ]);
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    for (path, failure) in [
        ("/typed.gb", Failure::WebPage),
        ("/lead.gb", Failure::WebPage),
        ("/lead-short.gb", Failure::WebPage),
        ("/empty.gb", Failure::Empty),
        ("/missing", Failure::Network),
    ] {
        let result = engine
            .fetch_game(
                &GameRequest::get(server.url(HOST, path)),
                &hosts(),
                directory.path(),
                |_| {},
            )
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
                &GameRequest::get(server.url(OTHER, "/rom.gb")),
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
            &GameRequest::get(server.url(HOST, "/rom.gb")),
            &hosts(),
            directory.path(),
            |p| progress.push(p.done),
        )
        .await
        .unwrap();
    assert_eq!(staged.bytes, rom.len() as u64);
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

const FORM: &str = "emuid=1&file=Super+Mario+World+%28USA%29.zip&id=42";

fn form() -> GameRequest {
    GameRequest {
        url: String::new(),
        form: Some(FORM.into()),
    }
}

/// ADR 0030 Q2a: `method: "POST"` sends the form-encoded body once, with
/// the form content type as the only added header and no credentials.
#[tokio::test]
async fn a_form_post_sends_the_encoded_body_with_only_the_form_content_type() {
    let server = HttpFixture::start([(
        "/dl/7bf4e0908e024c5",
        Route::ok("application/zip", b"PK\x03\x04 bytes".to_vec()),
    )]);
    let directory = tempfile::tempdir().unwrap();
    let request = GameRequest {
        url: server.url(HOST, "/dl/7bf4e0908e024c5"),
        ..form()
    };
    engine()
        .fetch_game(&request, &hosts(), directory.path(), |_| {})
        .await
        .unwrap();
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "POST");
    assert_eq!(received[0].body, FORM.as_bytes());
    assert!(received[0]
        .headers
        .contains(&"content-type: application/x-www-form-urlencoded".to_owned()));
    for refused in ["cookie", "authorization", "referer"] {
        assert!(
            !received[0].headers.iter().any(|h| h.starts_with(refused)),
            "{refused}"
        );
    }
}

/// ADR 0030 Q2b, browser rules (RFC 9110 section 15.4): 301, 302 and 303
/// become a GET with no body; 307 and 308 send the same POST again. Every
/// hop stays inside the declared hosts.
#[tokio::test]
async fn redirects_after_a_post_follow_browser_rules_and_declared_hosts() {
    let server = HttpFixture::start([(
        "/file",
        Route::ok("application/octet-stream", vec![1u8; 64]),
    )]);
    for status in [301u16, 302, 303, 307, 308] {
        server.route(
            &format!("/p{status}"),
            Route::redirect_with(status, "/file"),
        );
    }
    server.route(
        "/away",
        Route::redirect_with(307, &server.url(OTHER, "/file")),
    );
    let directory = tempfile::tempdir().unwrap();
    for status in [301u16, 302, 303, 307, 308] {
        let before = server.received().len();
        let request = GameRequest {
            url: server.url(HOST, &format!("/p{status}")),
            ..form()
        };
        engine()
            .fetch_game(&request, &hosts(), directory.path(), |_| {})
            .await
            .unwrap();
        let hops = &server.received()[before..];
        assert_eq!(hops.len(), 2, "{status}");
        assert_eq!(
            (hops[0].method.as_str(), hops[0].body.as_slice()),
            ("POST", FORM.as_bytes())
        );
        if status >= 307 {
            assert_eq!(hops[1].method, "POST", "{status}");
            assert_eq!(hops[1].body, FORM.as_bytes(), "{status}");
        } else {
            assert_eq!(hops[1].method, "GET", "{status}");
            assert!(hops[1].body.is_empty(), "{status}");
            assert!(
                !hops[1]
                    .headers
                    .iter()
                    .any(|h| h.starts_with("content-type")),
                "{status}"
            );
        }
    }
    let before = server.received().len();
    let away = GameRequest {
        url: server.url(HOST, "/away"),
        ..form()
    };
    assert_eq!(
        engine()
            .fetch_game(&away, &hosts(), directory.path(), |_| {})
            .await
            .err(),
        Some(Failure::UndeclaredHost)
    );
    assert_eq!(server.received().len(), before + 1, "the hop is never sent");
}

/// Coordinator decision 2026-10-23: Cloudflare-fronted sites (coolrom.com)
/// answer 403 without a User-Agent. korrid sends `Korri/<version>` on every
/// acquisition request and redirect hop. A provider turn may still set its
/// own: ADR 0016's refused headers do not include `user-agent`.
#[tokio::test]
async fn every_acquisition_request_and_hop_carries_korris_user_agent() {
    let server = HttpFixture::start([
        ("/page", Route::ok("text/plain", "page")),
        ("/hop", Route::redirect("/page")),
        (
            "/file",
            Route::ok("application/octet-stream", vec![1u8; 64]),
        ),
        ("/to-file", Route::redirect("/file")),
    ]);
    let ours = format!(
        "user-agent: {}",
        super::download::USER_AGENT.to_ascii_lowercase()
    );
    assert_eq!(
        super::download::USER_AGENT,
        format!("Korri/{}", env!("CARGO_PKG_VERSION"))
    );
    let engine = engine();
    text(&engine, &server.url(HOST, "/hop")).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    engine
        .fetch_game(
            &GameRequest::get(server.url(HOST, "/to-file")),
            &hosts(),
            directory.path(),
            |_| {},
        )
        .await
        .unwrap();
    let received = server.received();
    assert_eq!(received.len(), 4);
    for request in &received {
        let agents: Vec<_> = request
            .headers
            .iter()
            .filter(|h| h.starts_with("user-agent:"))
            .collect();
        assert_eq!(agents, [&ours], "{}", request.path);
    }

    // A provider turn's own User-Agent replaces korrid's; it is not doubled.
    let before = server.received().len();
    engine
        .fetch_text(
            TextRequest {
                url: Url::parse(&server.url(HOST, "/page")).unwrap(),
                headers: &[("user-agent".into(), "Plugin/2".into())],
                timeout: None,
            },
            &hosts(),
        )
        .await
        .unwrap();
    let agents: Vec<_> = server.received()[before]
        .headers
        .iter()
        .filter(|h| h.starts_with("user-agent:"))
        .cloned()
        .collect();
    assert_eq!(agents, ["user-agent: plugin/2"]);
}
