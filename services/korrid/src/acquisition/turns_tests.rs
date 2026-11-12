//! The request/response turn protocol against real handlers and a real server.
use super::{
    claims::DownloadResolution,
    download::DownloadEngine,
    http_fixture::{Echo, HttpFixture, Route},
    turns::{self, ProviderContext, ProviderError},
    Failure,
};
use std::{net::IpAddr, sync::Arc, time::Duration};

const HOST: &str = "provider.test";

fn engine() -> DownloadEngine {
    let loopback: IpAddr = "127.0.0.1".parse().unwrap();
    DownloadEngine::for_test(Duration::from_secs(5))
        .with_name(HOST, loopback)
        .with_name("other.test", loopback)
}

fn context(handlers: &str) -> ProviderContext {
    let source = format!(
        "export const name = 'shop';
         export const providers = {{'@test:shop': {{id: '@test:shop', title: 'Shop'}}}};
         const fail = () => {{ throw new Error('not used'); }};
         export const handlers = {{'claims.search': fail, 'claims.details': fail, 'artifact.resolve-download': fail, ...{handlers}}};"
    );
    ProviderContext {
        provider_id: "@test:shop".into(),
        source: Arc::new(crate::script::source::SourceSnapshot::plugin(&source).unwrap()),
        hosts: Arc::new(vec![HOST.into()]),
        credential: None,
    }
}

fn claim(id: &str, title: &str) -> String {
    format!("{{_tag: 'ProviderClaim', providerId: '@test:shop', id: '{id}', title: {title}, url: 'https://{HOST}/g/{id}', artifact: {{kind: 'content', system: 'gb', format: {{id: 'gb'}}}}}}")
}

#[tokio::test]
async fn handlers_get_every_response_in_order_across_turns() {
    let server = HttpFixture::start([
        ("/a?q=old", Route::ok("text/plain", "first")),
        ("/a?q=tetris&page=2", Route::ok("text/plain", "second")),
        ("/b", Route::ok("text/plain", "third")),
    ]);
    // Turn 1 asks for two pages; turn 2 asks for one more; turn 3 answers
    // with everything it saw, in order, as claim titles.
    let handlers = format!(
        "{{'claims.search': (input) => {{
            const r = input.responses;
            if (r.length === 0) return {{_tag: 'HttpRequests', requests: [
              {{url: '{a_old}'}},
              {{url: '{a_old}', query: {{q: input.query, page: 2}}, headers: {{'Accept': 'text/plain'}}, timeoutMs: 5000}}]}};
            if (r.length === 2) return {{_tag: 'HttpRequests', requests: [{{url: '{b}'}}]}};
            return [{claim}];
        }}}}",
        a_old = server.url(HOST, "/a?q=old"),
        b = server.url(HOST, "/b"),
        claim = claim(
            "x",
            "r.map(x => x.status + ':' + x.ok + ':' + x.text + ':' + x.headers['content-type']).join('|')"
        ),
    );
    let claims = turns::search(&engine(), &context(&handlers), "tetris")
        .await
        .unwrap();
    assert_eq!(
        claims[0].title,
        "200:true:first:text/plain|200:true:second:text/plain|200:true:third:text/plain"
    );
    let paths: Vec<_> = server.received().into_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/a?q=old", "/a?q=tetris&page=2", "/b"]);
    assert!(server.received()[1]
        .headers
        .contains(&"accept: text/plain".to_owned()));
    assert!(server
        .received()
        .iter()
        .all(|r| r.method == "GET" && r.body.is_empty()));
}

async fn failure(handlers: &str) -> ProviderError {
    turns::search(&engine(), &context(handlers), "q")
        .await
        .unwrap_err()
}

fn failed(error: ProviderError) -> String {
    match error {
        ProviderError::Failed(reason) => reason,
        other => panic!("expected a provider failure, got {other:?}"),
    }
}

#[tokio::test]
async fn turn_bounds_and_requests_are_enforced_before_any_fetch() {
    let server = HttpFixture::start([("/p", Route::ok("text/plain", "page"))]);
    let page = server.url(HOST, "/p");

    // Always asking: 3 fetch turns happen, then the fourth call fails.
    let always = format!(
        "{{'claims.search': () => ({{_tag: 'HttpRequests', requests: [{{url: '{page}'}}]}})}}"
    );
    assert!(failed(failure(&always).await).contains("fourth"));
    assert_eq!(server.received().len(), 3);

    let nine = format!("{{'claims.search': () => ({{_tag: 'HttpRequests', requests: Array(9).fill({{url: '{page}'}})}})}}");
    let zero = "{'claims.search': () => ({_tag: 'HttpRequests', requests: []})}".to_owned();
    let undeclared = format!(
        "{{'claims.search': () => ({{_tag: 'HttpRequests', requests: [{{url: '{}'}}]}})}}",
        server.url("other.test", "/p")
    );
    let cookie = format!("{{'claims.search': () => ({{_tag: 'HttpRequests', requests: [{{url: '{page}', headers: {{Cookie: 'a=b'}}}}]}})}}");
    let credentials = format!(
        "{{'claims.search': () => ({{_tag: 'HttpRequests', requests: [{{url: 'http://u:p@{HOST}/p'}}]}})}}"
    );
    let put = format!("{{'claims.search': () => ({{_tag: 'HttpRequests', requests: [{{url: '{page}', method: 'PUT'}}]}})}}");
    for handlers in [nine, zero, undeclared, cookie, credentials, put] {
        failed(failure(&handlers).await);
    }
    assert_eq!(
        server.received().len(),
        3,
        "refused requests never reach the network"
    );
}

/// Public itch.io attachments resolve through a form POST that returns JSON,
/// not artifact bytes. It uses the game sender without adding a cookie session.
#[tokio::test]
async fn provider_turns_send_form_posts_without_forwarding_credentials() {
    let server = HttpFixture::start([
        (
            "/page",
            Route::ok("text/html", "page").header("Set-Cookie", "session=secret; Path=/"),
        ),
        (
            "/file",
            Route::ok(
                "application/json",
                r#"{"url":"https://provider.test/cart.bitsy"}"#,
            ),
        ),
    ]);
    let handlers = format!(
        "{{'claims.search': (i) => {{
            if (!i.responses.length) return {{_tag: 'HttpRequests', requests: [{{url: '{page}'}}]}};
            if (i.responses.length === 1) return {{_tag: 'HttpRequests', requests: [{{url: '{file}', method: 'POST', headers: {{'Content-Type': 'application/x-www-form-urlencoded'}}, body: {{csrf_token: 'a b&+'}}}}]}};
            return [{claim}];
        }}}}",
        page = server.url(HOST, "/page"),
        file = server.url(HOST, "/file"),
        claim = claim("x", "i.responses[1].text"),
    );
    let claims = turns::search(&engine(), &context(&handlers), "q")
        .await
        .unwrap();
    assert_eq!(
        claims[0].title,
        r#"{"url":"https://provider.test/cart.bitsy"}"#
    );
    let received = server.received();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[1].method, "POST");
    assert_eq!(received[1].body, b"csrf_token=a+b%26%2B");
    assert_eq!(
        received[1]
            .headers
            .iter()
            .filter(|header| header.starts_with("content-type:"))
            .collect::<Vec<_>>(),
        [&"content-type: application/x-www-form-urlencoded".to_owned()]
    );
    for request in received {
        for refused in ["cookie:", "authorization:", "proxy-authorization:"] {
            assert!(!request.headers.iter().any(|h| h.starts_with(refused)));
        }
    }
}

#[tokio::test]
async fn provider_form_posts_preserve_redirect_rules_and_exact_hosts() {
    let server = HttpFixture::start([("/answer", Route::ok("text/plain", "done"))]);
    for status in [301, 302, 303, 307, 308] {
        server.route(
            &format!("/p{status}"),
            Route::redirect_with(status, "/answer").header("Set-Cookie", "session=secret; Path=/"),
        );
    }
    server.route(
        "/away",
        Route::redirect_with(307, &server.url("other.test", "/answer")),
    );
    let handler = |path: &str| {
        format!(
        "{{'claims.search': (i) => i.responses.length ? [{claim}] : ({{_tag: 'HttpRequests', requests: [{{url: '{url}', method: 'POST', body: {{csrf_token: 'token'}}}}]}})}}",
        claim = claim("x", "i.responses[0].text"),
        url = server.url(HOST, path),
    )
    };
    for status in [301, 302, 303, 307, 308] {
        let before = server.received().len();
        let claims = turns::search(&engine(), &context(&handler(&format!("/p{status}"))), "q")
            .await
            .unwrap();
        assert_eq!(claims[0].title, "done");
        let hops = &server.received()[before..];
        assert_eq!(hops.len(), 2);
        assert_eq!(hops[0].method, "POST");
        assert_eq!(hops[0].body, b"csrf_token=token");
        if status >= 307 {
            assert_eq!(hops[1].method, "POST");
            assert_eq!(hops[1].body, b"csrf_token=token");
        } else {
            assert_eq!(hops[1].method, "GET");
            assert!(hops[1].body.is_empty());
            assert!(!hops[1]
                .headers
                .iter()
                .any(|h| h.starts_with("content-type:")));
        }
        assert!(hops
            .iter()
            .all(|r| !r.headers.iter().any(|h| h.starts_with("cookie:"))));
    }
    let before = server.received().len();
    assert_eq!(
        failure(&handler("/away")).await,
        ProviderError::Unreachable(Failure::UndeclaredHost)
    );
    assert_eq!(server.received().len(), before + 1);
}

#[tokio::test]
async fn invalid_provider_forms_are_refused_before_any_request_in_the_turn() {
    let server = HttpFixture::start([("/file", Route::ok("text/plain", "ok"))]);
    let request = |fields: &str| {
        format!(
        "{{'claims.search': (i) => i.responses.length ? [] : ({{_tag: 'HttpRequests', requests: [{{url: '{url}'}}, {{url: '{url}', {fields}}}]}})}}",
        url = server.url(HOST, "/file"),
    )
    };
    for fields in [
        "method: 'PUT'",
        "method: 'GET'",
        "method: 'post'",
        "method: null",
        "body: {id: '42'}",
        "method: 'POST', body: null",
        "method: 'POST', body: {id: 42}",
        "method: 'POST', body: {id: null}",
        "method: 'POST', body: ['id']",
        "method: 'POST', body: {file: 'x'.repeat(8188)}",
        "method: 'POST', body: {file: '&'.repeat(2730)}",
        "method: 'POST', headers: {Cookie: 'a=b'}",
        "method: 'POST', headers: {Authorization: 'Bearer secret'}",
        "method: 'POST', headers: {'Set-Cookie': 'a=b'}",
        "method: 'POST', headers: {'Proxy-Authorization': 'secret'}",
        "method: 'POST', headers: {Host: 'other.test'}",
        "method: 'POST', headers: {'Content-Type': 'application/json'}",
    ] {
        failed(failure(&request(fields)).await);
    }
    assert!(server.received().is_empty());
    // Encoded body limit is inclusive, like FinalDownload. Empty POSTs also work.
    for fields in [
        "method: 'POST', body: {file: 'x'.repeat(8187)}",
        "method: 'POST'",
        "method: 'POST', body: {}",
    ] {
        turns::search(&engine(), &context(&request(fields)), "q")
            .await
            .unwrap();
    }
    assert_eq!(server.received().len(), 6);
}

#[tokio::test]
async fn invalid_results_and_thrown_errors_are_provider_failures() {
    for result in [
        format!("[{}]", claim("x", "'t'")).replace("'@test:shop'", "'@test:other'"),
        format!("[{}]", claim("", "'t'")),
        format!("[{}]", claim("x", "'t'.repeat(8193)")),
        format!("[{}]", claim("x", "null")),
        format!("[{}]", claim("x", "'t'")).replace(
            "_tag: 'ProviderClaim'",
            "_tag: 'ProviderClaim', ref: {kind: 'url', value: 'x'}",
        ),
        format!("[{}]", claim("x", "'t'")).replace(format!("https://{HOST}").as_str(), "file://"),
        format!("[{}]", claim("x", "'t'")).replace("format: {id: 'gb'}", "format: {id: 'GB'}"),
        "{_tag: 'HttpRequestz', requests: []}".into(),
    ] {
        let handlers = format!("{{'claims.search': () => {result}}}");
        failed(failure(&handlers).await);
    }
    let thrown = failed(failure("{'claims.search': () => { throw new Error('boom'); }}").await);
    assert!(thrown.contains("boom"), "{thrown}");
    // A missing claim system is admitted: the claim is shown, not acquirable.
    let handlers = format!(
        "{{'claims.search': () => [{}]}}",
        claim("x", "'t'").replace("system: 'gb', ", "")
    );
    let claims = turns::search(&engine(), &context(&handlers), "q")
        .await
        .unwrap();
    assert!(claims[0].artifact.as_ref().unwrap().system.is_none());
}

#[tokio::test]
async fn transport_failures_are_unreachable_not_plugin_input() {
    // A declared name whose port has no listener.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    let handlers = format!(
        "{{'claims.search': (i) => i.responses.length ? [] : ({{_tag: 'HttpRequests', requests: [{{url: 'http://{HOST}:{port}/x'}}]}})}}"
    );
    assert_eq!(
        failure(&handlers).await,
        ProviderError::Unreachable(Failure::Network)
    );
}

#[tokio::test]
async fn provider_response_body_is_bounded_at_512_kib() {
    let limit = 512 * 1024;
    let server = HttpFixture::start([
        ("/limit", Route::ok("text/plain", vec![b'x'; limit])),
        ("/oversize", Route::ok("text/plain", vec![b'x'; limit + 1])),
        (
            "/unsized-limit",
            Route::ok("text/plain", vec![b'x'; limit]).unsized_body(),
        ),
        (
            "/unsized-oversize",
            Route::ok("text/plain", vec![b'x'; limit + 1]).unsized_body(),
        ),
    ]);
    let handler = |path| {
        format!(
            "{{'claims.search': (i) => i.responses.length ? [{}] : ({{_tag: 'HttpRequests', requests: [{{url: '{}'}}]}})}}",
            claim("x", "String(i.responses[0].text.length)"),
            server.url(HOST, path)
        )
    };
    for path in ["/limit", "/unsized-limit"] {
        let claims = turns::search(&engine(), &context(&handler(path)), "q")
            .await
            .unwrap();
        assert_eq!(claims[0].title, limit.to_string());
    }
    for path in ["/oversize", "/unsized-oversize"] {
        let error = failure(&handler(path)).await;
        assert_eq!(error, ProviderError::Unreachable(Failure::TooLarge));
        assert_eq!(error.rpc().code, "ProviderUnreachable");
    }
}

#[tokio::test]
async fn handler_input_is_bounded_in_total_after_json_escaping() {
    let limit = 512 * 1024;
    let server = HttpFixture::start([
        ("/page", Route::ok("text/plain", vec![b'x'; limit])),
        ("/quoted", Route::ok("text/plain", vec![b'\"'; limit])),
    ]);
    // Seven pages fit the unchanged 4 MiB input bound and 16 MiB heap.
    let seven = format!(
        "{{'claims.search': (i) => i.responses.length ? [{}] : ({{_tag: 'HttpRequests', requests: Array(7).fill({{url: '{}'}})}})}}",
        claim("x", "String(i.responses.reduce((n, r) => n + r.text.length, 0))"),
        server.url(HOST, "/page")
    );
    let claims = turns::search(&engine(), &context(&seven), "q")
        .await
        .unwrap();
    assert_eq!(claims[0].title, (7 * limit).to_string());

    // Eight plain pages exceed 4 MiB with metadata. Four quoted pages do
    // too: JSON escaping doubles the body bytes before the total is checked.
    for (path, count) in [("/page", 8), ("/quoted", 4)] {
        let handlers = format!(
            "{{'claims.search': (i) => i.responses.length ? [] : ({{_tag: 'HttpRequests', requests: Array({count}).fill({{url: '{}'}})}})}}",
            server.url(HOST, path)
        );
        assert_eq!(
            failed(failure(&handlers).await),
            "handler input exceeds 4 MiB"
        );
    }
}

#[tokio::test]
async fn a_final_download_must_name_a_declared_host() {
    let resolve = |url: &str| {
        context(&format!(
            "{{'artifact.resolve-download': (i) => ({{_tag: 'FinalDownload', providerId: i.providerId, url: '{url}', filename: i.fileName}})}}"
        ))
    };
    let engine = engine();
    let resolution = turns::resolve_download(
        &engine,
        &resolve(&format!("https://{HOST}/rom.gb")),
        &format!("https://{HOST}/g/x"),
        Some("rom.gb"),
    )
    .await
    .unwrap();
    assert!(matches!(
        resolution,
        DownloadResolution::Final(ref download) if download.filename.as_deref() == Some("rom.gb")
    ));
    failed(
        turns::resolve_download(
            &engine,
            &resolve("https://other.test/rom.gb"),
            "https://provider.test/g/x",
            None,
        )
        .await
        .unwrap_err(),
    );
}

/// ADR 0030 Q2a, Q2d and Q3a: the game download may be a form POST with a
/// string-valued body of at most 8192 encoded characters; `filename` must
/// also pass the URL-name rule (no control characters).
#[tokio::test]
async fn a_final_download_admits_a_bounded_form_post_and_a_strict_filename() {
    use super::claims::FinalDownloadMethod;
    let resolve = |fields: &str| {
        context(&format!(
            "{{'artifact.resolve-download': (i) => ({{_tag: 'FinalDownload', providerId: i.providerId, url: 'https://{HOST}/dl/7bf4e0908e024c5', {fields}}})}}"
        ))
    };
    let engine = engine();
    let admitted = |fields: &str| {
        let context = resolve(fields);
        let engine = engine.clone();
        async move {
            turns::resolve_download(&engine, &context, "https://provider.test/g/x", None).await
        }
    };
    let resolution = admitted(
        "method: 'POST', body: {emuid: '1', id: '42', file: 'Super Mario World (USA).zip'}, filename: 'Super Mario World (USA).zip'",
    )
    .await
    .unwrap();
    let DownloadResolution::Final(download) = resolution else {
        panic!("{resolution:?}");
    };
    assert_eq!(download.method, Some(FinalDownloadMethod::Post));
    assert_eq!(download.body.as_ref().unwrap().len(), 3);
    assert!(admitted("method: 'POST', body: {file: 'x'.repeat(8187)}")
        .await
        .is_ok());
    for fields in [
        "body: {id: '42'}",
        "method: 'PUT'",
        "method: 'GET'",
        "method: 'post'",
        "method: 'POST', body: {id: 42}",
        "method: 'POST', body: ['id']",
        "method: 'POST', body: {file: 'x'.repeat(8188)}",
        "filename: 'bell\\u0007.zip'",
        "filename: 'a/b.zip'",
        "filename: '..'",
    ] {
        failed(admitted(fields).await.unwrap_err());
    }
}

// ---------------------------------------------------------------------------
// A plugin's secret (ADR 0034)
// ---------------------------------------------------------------------------

const SECRET: &str = "sgdb-very-secret-value";

/// A provider whose plugin declares a secret for `HOST` and may also reach
/// `other.test`, with the secret set as a person would set it.
fn context_with_secret(handlers: &str) -> (ProviderContext, tempfile::TempDir) {
    context_with_the_secret(SECRET, handlers)
}

/// As `context_with_secret`, for a secret a person could also have set.
fn context_with_the_secret(secret: &str, handlers: &str) -> (ProviderContext, tempfile::TempDir) {
    let private = tempfile::tempdir().unwrap();
    let secrets = crate::plugin_secrets::PluginSecrets::new(private.path());
    secrets.set("@test:shop", secret).unwrap();
    let credential = secrets.credential("@test:shop", HOST).unwrap().unwrap();
    let mut context = context(handlers);
    context.hosts = Arc::new(vec![HOST.into(), "other.test".into()]);
    context.credential = Some(Arc::new(credential));
    (context, private)
}

fn carried_secret(request: &super::http_fixture::Received) -> bool {
    let bearer = format!("authorization: bearer {SECRET}");
    assert!(
        request
            .headers
            .iter()
            .filter(|header| header.starts_with("authorization:"))
            .all(|header| *header == bearer),
        "an authorization header that is not korrid's: {:?}",
        request.headers
    );
    request.headers.contains(&bearer)
}

fn host_of(request: &super::http_fixture::Received) -> String {
    request
        .headers
        .iter()
        .find_map(|header| header.strip_prefix("host: "))
        .and_then(|host| host.split(':').next())
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn the_secret_goes_as_a_bearer_header_to_its_own_host_and_to_no_other() {
    let server = HttpFixture::start([
        ("/api/search", Route::ok("application/json", "{}")),
        ("/image.png", Route::ok("text/plain", "image")),
    ]);
    // One turn asks the secret's host and the plugin's other declared host.
    // The handler answers with everything it was given, so the test can see
    // that no part of its input holds the secret.
    let handlers = format!(
        "{{'claims.search': (input) => input.responses.length
            ? [{claim}]
            : ({{_tag: 'HttpRequests', requests: [{{url: '{api}'}}, {{url: '{image}'}}]}})}}",
        claim = claim("x", "JSON.stringify(input)"),
        api = server.url(HOST, "/api/search"),
        image = server.url("other.test", "/image.png"),
    );
    let (context, _private) = context_with_secret(&handlers);
    let claims = turns::search(&engine(), &context, "q").await.unwrap();
    let received = server.received();
    assert_eq!(
        received
            .iter()
            .map(|request| (host_of(request), carried_secret(request)))
            .collect::<Vec<_>>(),
        vec![(HOST.to_owned(), true), ("other.test".to_owned(), false)]
    );
    // The plugin's code saw both responses and never the secret.
    assert!(claims[0].title.contains("\"responses\""));
    assert!(!claims[0].title.contains(SECRET));
    assert!(!claims[0].title.to_lowercase().contains("authorization"));

    // Without a secret, the same plugin's requests carry no such header.
    let mut plain = context.clone();
    plain.credential = None;
    let before = server.received().len();
    turns::search(&engine(), &plain, "q").await.unwrap();
    assert!(server.received()[before..]
        .iter()
        .all(|request| !carried_secret(request)));
}

#[tokio::test]
async fn the_secret_follows_a_redirect_only_to_the_same_host() {
    let server = HttpFixture::start([("/answer", Route::ok("text/plain", "done"))]);
    server.route("/same", Route::redirect("/answer"));
    server.route(
        "/away",
        Route::redirect(&server.url("other.test", "/answer")),
    );
    // Away and back: the redirect left the host, so the secret is gone.
    server.route(
        "/away-and-back",
        Route::redirect(&server.url("other.test", "/back")),
    );
    server.route("/back", Route::redirect(&server.url(HOST, "/answer")));
    let handler = |host: &str, path: &str| {
        format!(
            "{{'claims.search': (i) => i.responses.length ? [{claim}] : ({{_tag: 'HttpRequests', requests: [{{url: '{url}'}}]}})}}",
            claim = claim("x", "i.responses[0].text"),
            url = server.url(host, path),
        )
    };
    let hops = |handlers: String| {
        let server = &server;
        async move {
            let (context, _private) = context_with_secret(&handlers);
            let before = server.received().len();
            let claims = turns::search(&engine(), &context, "q").await.unwrap();
            assert_eq!(claims[0].title, "done");
            server.received()[before..]
                .iter()
                .map(|request| {
                    (
                        host_of(request),
                        request.path.clone(),
                        carried_secret(request),
                    )
                })
                .collect::<Vec<_>>()
        }
    };
    let own = HOST.to_owned();
    let other = "other.test".to_owned();
    assert_eq!(
        hops(handler(HOST, "/same")).await,
        vec![
            (own.clone(), "/same".to_owned(), true),
            (own.clone(), "/answer".to_owned(), true)
        ]
    );
    assert_eq!(
        hops(handler(HOST, "/away")).await,
        vec![
            (own.clone(), "/away".to_owned(), true),
            (other.clone(), "/answer".to_owned(), false)
        ]
    );
    assert_eq!(
        hops(handler(HOST, "/away-and-back")).await,
        vec![
            (own.clone(), "/away-and-back".to_owned(), true),
            (other.clone(), "/back".to_owned(), false),
            (own.clone(), "/answer".to_owned(), false)
        ]
    );
    // A request that starts on another host never gains the secret, also
    // when that host redirects to the secret's host.
    assert_eq!(
        hops(handler("other.test", "/back")).await,
        vec![
            (other, "/back".to_owned(), false),
            (own, "/answer".to_owned(), false)
        ]
    );
}

/// Turn admission lets a plugin ask for `http`. The secret still never
/// leaves in clear text: korrid adds it to an HTTPS request only.
#[tokio::test]
async fn the_secret_never_goes_to_its_host_over_plain_http() {
    let server = HttpFixture::start([
        ("/api", Route::ok("text/plain", "plain")),
        (
            "/denied",
            Route {
                status: 401,
                ..Route::ok("text/plain", "no key")
            },
        ),
    ]);
    let handler = |path: &str| {
        format!(
            "{{'claims.search': (i) => i.responses.length ? [{claim}] : ({{_tag: 'HttpRequests', requests: [{{url: '{url}'}}]}})}}",
            claim = claim("x", "i.responses[0].status + ':' + i.responses[0].text"),
            url = server.url(HOST, path),
        )
    };
    // The engine as shipped korrid has it for a secret, over the test server.
    let shipped = engine().with_secrets_over_https_only();
    let (context, _private) = context_with_secret(&handler("/api"));
    let claims = turns::search(&shipped, &context, "q").await.unwrap();
    // The request went to the secret's own host, and without the secret.
    assert_eq!(claims[0].title, "200:plain");
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(host_of(&received[0]), HOST);
    assert!(!carried_secret(&received[0]));
    assert!(!received[0]
        .headers
        .iter()
        .any(|header| header.starts_with("authorization")));
    // A 401 to a request that carried no secret is the plugin's to read: it
    // does not say the secret is wrong.
    let (context, _private) = context_with_secret(&handler("/denied"));
    let claims = turns::search(&shipped, &context, "q").await.unwrap();
    assert_eq!(claims[0].title, "401:no key");
    assert!(server
        .received()
        .iter()
        .all(|request| !carried_secret(request)));

    // The rule itself, with no network: HTTPS to the host, and nothing else.
    let secrets_folder = tempfile::tempdir().unwrap();
    let secrets = crate::plugin_secrets::PluginSecrets::new(secrets_folder.path());
    secrets.set("@test:shop", SECRET).unwrap();
    let credential = secrets.credential("@test:shop", HOST).unwrap().unwrap();
    let url = |text: &str| url::Url::parse(text).unwrap();
    for production in [DownloadEngine::default(), shipped] {
        assert!(production
            .secret_header(&credential, &url(&format!("https://{HOST}/api")))
            .is_some());
        for refused in [
            format!("http://{HOST}/api"),
            format!("http://{HOST}:443/api"),
            "https://other.test/api".to_owned(),
            "http://other.test/api".to_owned(),
        ] {
            assert!(
                production
                    .secret_header(&credential, &url(&refused))
                    .is_none(),
                "{refused}"
            );
        }
    }
}

#[tokio::test]
async fn a_plugin_with_a_secret_still_cannot_set_authorization_and_no_failure_names_the_secret() {
    let server = HttpFixture::start([("/api", Route::ok("text/plain", "ok"))]);
    let asking = |headers: &str| {
        format!(
            "{{'claims.search': (i) => i.responses.length ? [] : ({{_tag: 'HttpRequests', requests: [{{url: '{url}', headers: {headers}}}]}})}}",
            url = server.url(HOST, "/api"),
        )
    };
    for headers in [
        "{Authorization: 'Bearer mine'}",
        "{authorization: 'Bearer mine'}",
        "{'Proxy-Authorization': 'mine'}",
    ] {
        let (context, _private) = context_with_secret(&asking(headers));
        let error = turns::search(&engine(), &context, "q").await.unwrap_err();
        assert_eq!(failed(error), "refused or invalid request header");
    }
    assert!(server.received().is_empty());

    // Every way a turn with the secret can fail: the diagnostic and the
    // person's copy say nothing of the secret.
    let throwing = format!(
        "{{'claims.search': (i) => {{ if (i.responses.length) throw new Error('bad ' + JSON.stringify(i)); return {{_tag: 'HttpRequests', requests: [{{url: '{url}'}}]}}; }}}}",
        url = server.url(HOST, "/api"),
    );
    let unreachable = format!(
        "{{'claims.search': () => ({{_tag: 'HttpRequests', requests: [{{url: 'http://{HOST}:1/api'}}]}})}}"
    );
    for handlers in [throwing, unreachable] {
        let (context, _private) = context_with_secret(&handlers);
        let error = turns::search(&engine(), &context, "q").await.unwrap_err();
        let failure = error.rpc();
        for text in [format!("{error:?}"), failure.code, failure.message] {
            assert!(!text.contains(SECRET), "{text}");
        }
    }
}

/// ADR 0034: the plugin's code never receives the secret "in a response". A
/// host that repeats the request's `Authorization` header would hand it
/// over, so korrid keeps such a response from the handler.
#[tokio::test]
async fn a_response_that_repeats_the_secret_is_never_given_to_the_plugin() {
    let server = HttpFixture::start([
        ("/plain", Route::ok("application/json", r#"{"ok":true}"#)),
        (
            "/echo-header",
            Route::ok("application/json", "{}").echoing(Echo::Header),
        ),
        (
            "/echo-body",
            Route::ok("application/json", "{}").echoing(Echo::Body),
        ),
        // Only the redirect's target, on the same host, repeats it.
        ("/moved", Route::redirect("/echo-body")),
    ]);
    // The site puts the value in the address it redirects to, and another
    // declared host has a page that holds it.
    server.route(
        "/moved-with-key",
        Route::redirect(&format!("/plain?key={SECRET}")),
    );
    server.route(
        &format!("/plain?key={SECRET}"),
        Route::ok("text/plain", "fine"),
    );
    server.route(
        "/leaked",
        Route::ok("text/plain", format!("the key is {SECRET}")),
    );
    // A handler that, if it were ever called with a response, would hand
    // everything it was given back as the claim's title.
    let asking = |urls: &[String]| {
        format!(
            "{{'claims.search': (i) => i.responses.length ? [{claim}] : ({{_tag: 'HttpRequests', requests: [{requests}]}})}}",
            claim = claim("x", "JSON.stringify(i.responses)"),
            requests = urls
                .iter()
                .map(|url| format!("{{url: '{url}'}}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    };
    let own = |path: &str| server.url(HOST, path);

    // A normal response from the secret's host still passes.
    let (context, _private) = context_with_secret(&asking(&[own("/plain")]));
    let claims = turns::search(&engine(), &context, "q").await.unwrap();
    assert!(
        claims[0].title.contains(r#"{\"ok\":true}"#),
        "{}",
        claims[0].title
    );
    assert!(carried_secret(server.received().last().unwrap()));

    for (urls, answering_host, real_echo) in [
        (vec![own("/echo-header")], HOST, true),
        (vec![own("/echo-body")], HOST, true),
        (vec![own("/moved")], HOST, true),
        (vec![own("/moved-with-key")], HOST, false),
        (
            vec![server.url("other.test", "/leaked")],
            "other.test",
            false,
        ),
        // One bad response ends the turn: the good one is not passed on either.
        (vec![own("/plain"), own("/echo-header")], HOST, true),
    ] {
        let (context, _private) = context_with_secret(&asking(&urls));
        let error = turns::search(&engine(), &context, "q").await.unwrap_err();
        // The handler was not called again: its answer would have been `Ok`.
        assert_eq!(
            error,
            ProviderError::CredentialRepeated {
                host: answering_host.into()
            },
            "{urls:?}"
        );
        let failure = error.rpc();
        assert_eq!(failure.code, "ProviderFailed");
        for text in [format!("{error:?}"), failure.message] {
            assert!(!text.contains(SECRET), "{text}");
        }
        if real_echo {
            // A real echo: the response held the secret because the request did.
            assert!(
                carried_secret(server.received().last().unwrap()),
                "{urls:?}"
            );
        }
    }

    // The same echoing page is harmless to a plugin with no secret: there is
    // nothing to repeat.
    let mut plain = context_with_secret(&asking(&[own("/echo-body")])).0;
    plain.credential = None;
    let claims = turns::search(&engine(), &plain, "q").await.unwrap();
    assert!(
        claims[0].title.contains("Authorization"),
        "{}",
        claims[0].title
    );
    assert!(!claims[0].title.contains(SECRET));
}

/// A handler that asks for `urls` and, if it were ever called with their
/// responses, would hand everything it was given back as the claim's title.
fn handing_back(urls: &[String]) -> String {
    format!(
        "{{'claims.search': (i) => i.responses.length ? [{claim}] : ({{_tag: 'HttpRequests', requests: [{requests}]}})}}",
        claim = claim("x", "JSON.stringify(i.responses)"),
        requests = urls
            .iter()
            .map(|url| format!("{{url: '{url}'}}"))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// The secret can come back escaped: `JSON.parse` in the plugin would turn
/// `\"`, `\\`, `\/` and `\uXXXX` back into it, and a percent-encoded header
/// is one `decodeURIComponent` away. korrid compares those forms too.
#[tokio::test]
async fn an_escaped_echo_of_the_secret_is_never_given_to_the_plugin() {
    let server = HttpFixture::start([("/plain", Route::ok("application/json", r#"{"ok":true}"#))]);
    let echoing = |path: &str, echo: Echo| {
        server.route(path, Route::ok("application/json", "{}").echoing(echo));
        server.url(HOST, path)
    };
    let quoted = r#"sgdb"quoted"value"#;
    let backslashed = r"sgdb\back\slash";
    let slashed = "sgdb/with/slashes";
    for (name, secret, url) in [
        (
            "a quote, escaped by JSON",
            quoted,
            echoing("/body", Echo::Body),
        ),
        (
            "a backslash, escaped by JSON",
            backslashed,
            echoing("/body", Echo::Body),
        ),
        (
            "a slash written as \\/",
            slashed,
            echoing("/slashes", Echo::BodySlashEscaped),
        ),
        (
            "every character as \\uXXXX",
            SECRET,
            echoing("/unicode", Echo::BodyUnicodeEscaped),
        ),
        ("an object key", SECRET, echoing("/key", Echo::Key)),
        (
            "an object key with escapes",
            quoted,
            echoing("/key", Echo::Key),
        ),
        (
            "a percent-encoded header",
            SECRET,
            echoing("/header", Echo::HeaderPercentEncoded),
        ),
        (
            "a percent-encoded header, quotes",
            quoted,
            echoing("/header", Echo::HeaderPercentEncoded),
        ),
    ] {
        // A normal response from the same host passes with this secret.
        let (context, _private) =
            context_with_the_secret(secret, &handing_back(&[server.url(HOST, "/plain")]));
        let claims = turns::search(&engine(), &context, "q").await.unwrap();
        assert!(claims[0].title.contains("ok"), "{name}");

        let before = server.received().len();
        let (context, _private) = context_with_the_secret(secret, &handing_back(&[url]));
        let error = turns::search(&engine(), &context, "q").await.unwrap_err();
        // The handler was not called with it: its answer would have been `Ok`.
        assert_eq!(
            error,
            ProviderError::CredentialRepeated { host: HOST.into() },
            "{name}"
        );
        let failure = error.rpc();
        for text in [format!("{error:?}"), failure.message] {
            assert!(!text.contains(secret), "{name}: {text}");
        }
        // A real echo: the request carried the secret, and nothing in what
        // the server sent back holds it as it is.
        let request = &server.received()[before];
        assert!(
            request
                .headers
                .contains(&format!("authorization: bearer {}", secret.to_lowercase())),
            "{name}"
        );
    }
}

/// The comparison does not depend on korrid being able to parse the body. A
/// body nested deeper than korrid's JSON parser allows is still JSON to the
/// plugin, and JSON inside another text is one `JSON.parse` away.
#[tokio::test]
async fn an_escaped_echo_is_found_in_a_body_korrid_cannot_parse() {
    let escaped: String = SECRET
        .chars()
        .map(|character| format!("\\u{:04x}", character as u32))
        .collect();
    let deep = format!("{}\"{escaped}\"{}", "[".repeat(200), "]".repeat(200));
    assert!(serde_json::from_str::<serde_json::Value>(&deep).is_err());
    let wrapped = format!("callback({{\"auth\":\"{escaped}\"}});");
    let server = HttpFixture::start([]);
    server.route("/deep", Route::ok("application/json", deep));
    server.route("/wrapped", Route::ok("text/javascript", wrapped));
    // The premise: the plugin's own `JSON.parse` reads the deep body and
    // gets the value back.
    let digging = format!(
        "{{'claims.search': (i) => {{ if (!i.responses.length) return {{_tag: 'HttpRequests', requests: [{{url: '{url}'}}]}}; let value = JSON.parse(i.responses[0].text); while (Array.isArray(value)) value = value[0]; return [{claim}]; }}}}",
        url = server.url(HOST, "/deep"),
        claim = claim("x", "value"),
    );
    let mut without_a_secret = context_with_secret(&digging).0;
    without_a_secret.credential = None;
    let claims = turns::search(&engine(), &without_a_secret, "q")
        .await
        .unwrap();
    assert_eq!(claims[0].title, SECRET);

    for path in ["/deep", "/wrapped"] {
        let (context, _private) = context_with_secret(&handing_back(&[server.url(HOST, path)]));
        assert_eq!(
            turns::search(&engine(), &context, "q").await.unwrap_err(),
            ProviderError::CredentialRepeated { host: HOST.into() },
            "{path}"
        );
    }
}

/// The comparison is one pass for each form over a body that is at most
/// 512 KiB, so a large honest answer costs little and still passes.
#[tokio::test]
async fn a_large_json_body_with_escapes_passes_the_guard_quickly() {
    // Just under the 512 KiB bound, full of escapes and percent signs.
    let entry = r#"{"name":"Wario Land 4 \/ \u30ef\u30ea\u30aa \"EU\"","url":"https:\/\/cdn.test\/a%20b%2Fc.png","note":"100% \\ done"},"#;
    let mut body = String::from(r#"{"success":true,"data":["#);
    while body.len() + entry.len() < 500 * 1024 {
        body.push_str(entry);
    }
    body.push_str(r#"{"name":"last"}]}"#);
    assert!(body.len() > 480 * 1024 && body.len() < 512 * 1024);
    assert!(serde_json::from_str::<serde_json::Value>(&body).is_ok());
    let server = HttpFixture::start([]);
    server.route("/large", Route::ok("application/json", body.clone()));
    let handlers = format!(
        "{{'claims.search': (i) => i.responses.length ? [{claim}] : ({{_tag: 'HttpRequests', requests: [{{url: '{url}'}}]}})}}",
        claim = claim("x", "String(i.responses[0].text.length)"),
        url = server.url(HOST, "/large"),
    );
    let (context, _private) = context_with_secret(&handlers);
    let claims = turns::search(&engine(), &context, "q").await.unwrap();
    assert_eq!(claims[0].title, body.chars().count().to_string());

    // The guard alone, on the same body: every form is compared.
    let credential = context.credential.as_deref().unwrap();
    let started = std::time::Instant::now();
    assert!(!credential.appears_in(&body));
    let elapsed = started.elapsed();
    // About 35 ms in a test build. The limit only has to catch a
    // comparison that is not linear.
    assert!(
        elapsed < Duration::from_secs(2),
        "the guard took {elapsed:?} for {} bytes",
        body.len()
    );
    // And it still finds the secret at the very end of such a body.
    let escaped: String = SECRET
        .chars()
        .map(|character| format!("\\u{:04x}", character as u32))
        .collect();
    assert!(credential.appears_in(&format!("{body}\"{escaped}\"")));
}
