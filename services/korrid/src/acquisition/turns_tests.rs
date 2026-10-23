//! The request/response turn protocol against real handlers and a real server.
use super::{
    claims::DownloadResolution,
    download::DownloadEngine,
    http_fixture::{HttpFixture, Route},
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
    let post = format!("{{'claims.search': () => ({{_tag: 'HttpRequests', requests: [{{url: '{page}', method: 'POST'}}]}})}}");
    for handlers in [nine, zero, undeclared, cookie, credentials, post] {
        failed(failure(&handlers).await);
    }
    assert_eq!(
        server.received().len(),
        3,
        "refused requests never reach the network"
    );
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
