//! A raw text POST body (ADR 0016, IGDB amendment) and a client pair that
//! korrid exchanges for a token (ADR 0034, IGDB amendment), against real
//! handlers and a real loopback server.
use super::{
    download::DownloadEngine,
    http_fixture::{Echo, HttpFixture, Route},
    turns::{self, ProviderContext, ProviderError},
    Failure,
};
use std::{net::IpAddr, sync::Arc, time::Duration};

/// The host that receives the client ID and the token.
const API: &str = "api.test";
/// The host of the token address.
const TOKEN: &str = "token.test";
/// A declared host that receives neither.
const OTHER: &str = "images.test";
const CLIENT_ID: &str = "client-id-one";
const CLIENT_SECRET: &str = "client-secret-two";

fn engine() -> DownloadEngine {
    let loopback: IpAddr = "127.0.0.1".parse().unwrap();
    DownloadEngine::for_test(Duration::from_secs(5))
        .with_name(API, loopback)
        .with_name(TOKEN, loopback)
        .with_name(OTHER, loopback)
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
        hosts: Arc::new(vec![API.into(), TOKEN.into(), OTHER.into()]),
        credential: None,
    }
}

/// A claim whose title is `title`, a JS expression over the input `i`.
fn claim(title: &str) -> String {
    format!("{{_tag: 'ProviderClaim', providerId: '@test:shop', id: 'x', title: {title}, url: 'https://{API}/g/x', artifact: {{kind: 'content', system: 'gb', format: {{id: 'gb'}}}}}}")
}

/// A handler that asks for `requests` (JS array text) once and then answers
/// with its whole input as the claim title.
fn asking(requests: &str) -> String {
    format!(
        "{{'claims.search': (i) => i.responses.length ? [{claim}] : ({{_tag: 'HttpRequests', requests: {requests}}})}}",
        claim = claim("JSON.stringify(i)"),
    )
}

/// `context` with the client pair set as a person sets it, for a plugin that
/// declares `credentialHost = API` and the token address on `server`.
fn context_with_pair(server: &HttpFixture, handlers: &str) -> (ProviderContext, tempfile::TempDir) {
    let private = tempfile::tempdir().unwrap();
    let secrets = crate::plugin_secrets::PluginSecrets::new(private.path());
    secrets
        .set_client_pair("@test:shop", &format!("{CLIENT_ID}:{CLIENT_SECRET}"))
        .unwrap();
    let token_url = server.url(TOKEN, "/oauth2/token");
    let credential = secrets
        .credential_declared("@test:shop", API, Some(&token_url))
        .unwrap()
        .unwrap();
    let mut context = context(handlers);
    context.credential = Some(Arc::new(credential));
    (context, private)
}

fn token_answer(token: &str) -> Route {
    Route::ok(
        "application/json",
        format!(r#"{{"access_token":"{token}","expires_in":5011271,"token_type":"bearer"}}"#),
    )
}

fn unauthorized() -> Route {
    Route {
        status: 401,
        ..Route::ok("application/json", r#"{"message":"Authorization Failure"}"#)
    }
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

/// The value of a header the fixture received (names and values are lower
/// case there).
fn header<'a>(request: &'a super::http_fixture::Received, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}: ");
    request
        .headers
        .iter()
        .find_map(|header| header.strip_prefix(prefix.as_str()))
}

fn sent_to<'a>(
    received: &'a [super::http_fixture::Received],
    host: &str,
) -> Vec<&'a super::http_fixture::Received> {
    received
        .iter()
        .filter(|request| host_of(request) == host)
        .collect()
}

// ---------------------------------------------------------------------------
// Raw text body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_text_body_is_posted_as_it_is_with_korrids_content_type() {
    let server = HttpFixture::start([("/v4/games", Route::ok("application/json", "[]"))]);
    let query = r#"fields name, platforms; search "Ys \"I\" & II"; where platforms = (19,58);"#;
    let handlers = asking(&format!(
        "[{{url: '{url}', method: 'POST', headers: {{'content-type': 'text/plain'}}, body: {body}}}]",
        url = server.url(API, "/v4/games"),
        body = serde_json::to_string(query).unwrap(),
    ));
    turns::search(&engine(), &context(&handlers), "q")
        .await
        .unwrap();
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "POST");
    assert_eq!(received[0].body, query.as_bytes());
    assert_eq!(
        received[0]
            .headers
            .iter()
            .filter(|header| header.starts_with("content-type:"))
            .collect::<Vec<_>>(),
        [&"content-type: text/plain; charset=utf-8".to_owned()]
    );
}

#[tokio::test]
async fn a_text_body_has_the_form_bound_and_rules_and_is_refused_before_any_request() {
    let server = HttpFixture::start([("/q", Route::ok("text/plain", "ok"))]);
    let request = |fields: &str| {
        asking(&format!(
            "[{{url: '{url}'}}, {{url: '{url}', {fields}}}]",
            url = server.url(API, "/q"),
        ))
    };
    for fields in [
        // A body needs POST.
        "body: 'fields name;'",
        // The same 8192-character bound as a form.
        "method: 'POST', body: 'x'.repeat(8193)",
        "method: 'POST', body: 'é'.repeat(8193)",
        // korrid owns the Content-Type of a text body.
        "method: 'POST', body: 'x', headers: {'Content-Type': 'application/x-www-form-urlencoded'}",
        "method: 'POST', body: 'x', headers: {'Content-Type': 'application/json'}",
        "method: 'POST', body: 'x', headers: {'Content-Type': 'text/plain; charset=latin1'}",
        // A form keeps its own Content-Type rule.
        "method: 'POST', body: {a: 'b'}, headers: {'Content-Type': 'text/plain'}",
        "method: 'POST', body: 42",
    ] {
        let error = turns::search(&engine(), &context(&request(fields)), "q")
            .await
            .err()
            .unwrap();
        assert!(
            matches!(error, ProviderError::Failed(_)),
            "{fields}: {error:?}"
        );
    }
    assert!(server.received().is_empty());
    // The bound is inclusive and counts characters, not bytes.
    for fields in [
        "method: 'POST', body: 'x'.repeat(8192)",
        "method: 'POST', body: 'é'.repeat(8192)",
        "method: 'POST', body: ''",
        "method: 'POST', body: 'x', headers: {'Content-Type': 'Text/Plain; Charset=\"UTF-8\"'}",
    ] {
        turns::search(&engine(), &context(&request(fields)), "q")
            .await
            .unwrap();
    }
    assert_eq!(server.received().len(), 8);
}

#[tokio::test]
async fn a_text_post_keeps_the_redirect_rules_of_a_form_post() {
    let server = HttpFixture::start([("/answer", Route::ok("text/plain", "done"))]);
    for status in [303, 307] {
        server.route(
            &format!("/p{status}"),
            Route::redirect_with(status, "/answer"),
        );
    }
    server.route(
        "/away",
        Route::redirect_with(307, &server.url("undeclared.test", "/answer")),
    );
    let handler = |path: &str| {
        asking(&format!(
            "[{{url: '{url}', method: 'POST', body: 'fields name;'}}]",
            url = server.url(API, path),
        ))
    };
    turns::search(&engine(), &context(&handler("/p307")), "q")
        .await
        .unwrap();
    turns::search(&engine(), &context(&handler("/p303")), "q")
        .await
        .unwrap();
    let received = server.received();
    assert_eq!(
        received
            .iter()
            .map(|r| (
                r.method.as_str(),
                String::from_utf8_lossy(&r.body).into_owned()
            ))
            .collect::<Vec<_>>(),
        [
            ("POST", "fields name;".to_owned()),
            ("POST", "fields name;".to_owned()),
            ("POST", "fields name;".to_owned()),
            ("GET", String::new()),
        ]
    );
    assert_eq!(
        turns::search(&engine(), &context(&handler("/away")), "q")
            .await
            .err(),
        Some(ProviderError::Unreachable(Failure::UndeclaredHost))
    );
}

// ---------------------------------------------------------------------------
// Client pair and token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_client_pair_is_exchanged_once_and_goes_as_client_id_and_bearer_to_its_host_only() {
    let server = HttpFixture::start([
        ("/oauth2/token", token_answer("token-aaaa-1111")),
        ("/v4/games", Route::ok("application/json", "[]")),
        ("/image.png", Route::ok("text/plain", "image")),
    ]);
    let handlers = asking(&format!(
        "[{{url: '{api}', method: 'POST', body: 'fields name; where id = 1;'}}, {{url: '{other}'}}]",
        api = server.url(API, "/v4/games"),
        other = server.url(OTHER, "/image.png"),
    ));
    let (context, _private) = context_with_pair(&server, &handlers);
    // Two operations: the token is got once and kept.
    for _ in 0..2 {
        let claims = turns::search(&engine(), &context, "q").await.unwrap();
        for value in [CLIENT_ID, CLIENT_SECRET, "token-aaaa-1111"] {
            assert!(!claims[0].title.contains(value), "{value}");
        }
        assert!(claims[0].title.contains("\"responses\""));
    }
    let received = server.received();
    let token = sent_to(&received, TOKEN);
    assert_eq!(token.len(), 1, "one exchange for both operations");
    assert_eq!(token[0].method, "POST");
    assert_eq!(token[0].path, "/oauth2/token");
    assert_eq!(
        String::from_utf8_lossy(&token[0].body),
        format!(
            "grant_type=client_credentials&client_id={CLIENT_ID}&client_secret={CLIENT_SECRET}"
        )
    );
    assert_eq!(
        header(token[0], "content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(header(token[0], "authorization"), None);
    assert_eq!(header(token[0], "client-id"), None);

    let api = sent_to(&received, API);
    assert_eq!(api.len(), 2);
    for request in api {
        assert_eq!(header(request, "client-id"), Some(CLIENT_ID));
        assert_eq!(
            header(request, "authorization"),
            Some("bearer token-aaaa-1111")
        );
        assert_eq!(request.body, b"fields name; where id = 1;");
    }
    for request in sent_to(&received, OTHER) {
        assert_eq!(header(request, "client-id"), None);
        assert_eq!(header(request, "authorization"), None);
    }
}

#[tokio::test]
async fn a_401_gets_one_new_token_and_a_second_401_is_unauthorized() {
    let server = HttpFixture::start([]);
    server.queue(
        "/oauth2/token",
        vec![
            token_answer("token-old-0001"),
            token_answer("token-new-0002"),
            token_answer("token-new-0003"),
        ],
    );
    server.queue(
        "/v4/games",
        vec![unauthorized(), Route::ok("application/json", "[]")],
    );
    server.route("/v4/covers", unauthorized());
    let handler = |path: &str| {
        asking(&format!(
            "[{{url: '{url}', method: 'POST', body: 'fields url;'}}]",
            url = server.url(API, path),
        ))
    };
    // The site refuses the first token; korrid gets a new one and asks again.
    let (context, _private) = context_with_pair(&server, &handler("/v4/games"));
    let claims = turns::search(&engine(), &context, "q").await.unwrap();
    assert!(
        claims[0].title.contains("\"status\":200"),
        "{}",
        claims[0].title
    );
    let received = server.received();
    assert_eq!(sent_to(&received, TOKEN).len(), 2);
    let api = sent_to(&received, API);
    assert_eq!(
        api.iter()
            .map(|request| header(request, "authorization").unwrap())
            .collect::<Vec<_>>(),
        ["bearer token-old-0001", "bearer token-new-0002"]
    );

    // A site that refuses the new token too: the operation ends, once.
    let before = server.received().len();
    let (context, _private) = context_with_pair(&server, &handler("/v4/covers"));
    assert_eq!(
        turns::search(&engine(), &context, "q").await.err(),
        Some(ProviderError::Unauthorized)
    );
    let received = &server.received()[before..];
    assert_eq!(sent_to(received, TOKEN).len(), 2);
    assert_eq!(sent_to(received, API).len(), 2);
}

#[tokio::test]
async fn a_token_host_that_refuses_or_answers_badly_ends_the_operation_before_the_site() {
    let cases: Vec<(Route, ProviderError)> = vec![
        (unauthorized(), ProviderError::Unauthorized),
        (
            Route {
                status: 400,
                ..Route::ok("application/json", r#"{"message":"invalid client secret"}"#)
            },
            ProviderError::Unauthorized,
        ),
        (
            Route::redirect_with(307, "/elsewhere"),
            ProviderError::Unreachable(Failure::Redirect),
        ),
        (
            Route {
                status: 500,
                ..Route::ok("text/plain", "down")
            },
            ProviderError::Failed("the token host answered HTTP 500".into()),
        ),
        (
            Route::ok("application/json", r#"{"token_type":"bearer"}"#),
            ProviderError::Failed("the token answer holds no Bearer token".into()),
        ),
        (
            Route::ok(
                "application/json",
                r#"{"access_token":"abc","token_type":"mac"}"#,
            ),
            ProviderError::Failed("the token answer holds no Bearer token".into()),
        ),
        (
            Route::ok(
                "application/json",
                r#"{"access_token":"a\nb","token_type":"bearer"}"#,
            ),
            ProviderError::Failed("the token answer holds no Bearer token".into()),
        ),
    ];
    for (answer, expected) in cases {
        let server = HttpFixture::start([
            ("/oauth2/token", answer),
            ("/v4/games", Route::ok("application/json", "[]")),
        ]);
        let handlers = asking(&format!(
            "[{{url: '{url}', method: 'POST', body: 'fields name;'}}]",
            url = server.url(API, "/v4/games"),
        ));
        let (context, _private) = context_with_pair(&server, &handlers);
        assert_eq!(
            turns::search(&engine(), &context, "q").await.err(),
            Some(expected)
        );
        let received = server.received();
        assert_eq!(sent_to(&received, TOKEN).len(), 1);
        assert!(sent_to(&received, API).is_empty());
    }
}

#[tokio::test]
async fn no_exchange_happens_for_a_request_that_does_not_go_to_the_credential_host() {
    let server = HttpFixture::start([
        ("/oauth2/token", token_answer("token-aaaa-1111")),
        ("/image.png", Route::ok("text/plain", "image")),
        ("/v4/games", Route::ok("application/json", "[]")),
    ]);
    let (context, _private) = context_with_pair(
        &server,
        &asking(&format!("[{{url: '{}'}}]", server.url(OTHER, "/image.png"))),
    );
    turns::search(&engine(), &context, "q").await.unwrap();
    assert!(sent_to(&server.received(), TOKEN).is_empty());

    // As shipped, the credential goes over HTTPS only: a plain http request
    // to the credential host gets no token, and korrid asks for none.
    let (context, _private) = context_with_pair(
        &server,
        &asking(&format!("[{{url: '{}'}}]", server.url(API, "/v4/games"))),
    );
    let shipped = engine().with_secrets_over_https_only();
    turns::search(&shipped, &context, "q").await.unwrap();
    let received = server.received();
    assert!(sent_to(&received, TOKEN).is_empty());
    let api = sent_to(&received, API);
    assert_eq!(api.len(), 1);
    assert_eq!(header(api[0], "authorization"), None);
    assert_eq!(header(api[0], "client-id"), None);
}

#[tokio::test]
async fn a_response_that_repeats_the_token_the_client_id_or_the_secret_is_never_given_to_the_plugin(
) {
    let repeating = [
        // The request's Authorization header: the token.
        Route::ok("application/json", "[]").echoing(Echo::Body),
        Route::ok("application/json", "[]").echoing(Echo::HeaderPercentEncoded),
        Route::ok(
            "application/json",
            format!(r#"[{{"client":"{CLIENT_ID}"}}]"#),
        ),
        Route::ok("application/json", "[]").header("X-Client", CLIENT_ID),
        Route::ok(
            "application/json",
            format!(
                r#"[{{"seen":"{}"}}]"#,
                CLIENT_SECRET.replace('-', "\\u002d")
            ),
        ),
    ];
    for route in repeating {
        let server = HttpFixture::start([
            ("/oauth2/token", token_answer("token-aaaa-1111")),
            ("/v4/games", route),
        ]);
        let handlers = asking(&format!(
            "[{{url: '{url}', method: 'POST', body: 'fields name;'}}]",
            url = server.url(API, "/v4/games"),
        ));
        let (context, _private) = context_with_pair(&server, &handlers);
        assert_eq!(
            turns::search(&engine(), &context, "q").await.err(),
            Some(ProviderError::CredentialRepeated { host: API.into() })
        );
    }
}

#[test]
fn a_client_pair_must_be_an_id_and_a_secret_and_a_new_value_drops_the_token() {
    let private = tempfile::tempdir().unwrap();
    let secrets = crate::plugin_secrets::PluginSecrets::new(private.path());
    for invalid in ["no-colon", ":secret", "id:", ":"] {
        assert_eq!(
            secrets.set_client_pair("@test:shop", invalid),
            Err(crate::plugin_secrets::PluginSecretError::NotAClientPair),
            "{invalid}"
        );
    }
    assert_eq!(secrets.read("@test:shop"), Ok(None));
    secrets
        .set_client_pair("@test:shop", " id:sec:ret ")
        .unwrap();
    let token_url = "https://token.test/oauth2/token";
    let credential = secrets
        .credential_declared("@test:shop", API, Some(token_url))
        .unwrap()
        .unwrap();
    // The ID ends at the first `:`; the rest is the secret.
    let request = credential.token_request().unwrap();
    assert_eq!(
        request.form,
        "grant_type=client_credentials&client_id=id&client_secret=sec%3Aret"
    );
    credential.keep_token("token-kept-0001", Some(60)).unwrap();
    assert!(credential.token_request().is_none());
    let api = url::Url::parse(&format!("https://{API}/v4/games")).unwrap();
    assert_eq!(
        credential.authorization(&api).unwrap().to_str().unwrap(),
        "Bearer token-kept-0001"
    );
    assert!(credential.appears_in("x token-kept-0001 y"));
    assert_eq!(
        credential.redact("id sec:ret token-kept-0001"),
        "<redacted> <redacted> <redacted>"
    );
    // An expired token is not used.
    credential.keep_token("token-gone-0002", Some(0)).unwrap();
    assert!(credential.authorization(&api).is_none());
    assert!(credential.token_request().is_some());
    // A set drops the token: it was got with the old value.
    credential.keep_token("token-kept-0003", None).unwrap();
    secrets.set_client_pair("@test:shop", "id:other").unwrap();
    assert!(credential.authorization(&api).is_none());
    let again = secrets
        .credential_declared("@test:shop", API, Some(token_url))
        .unwrap()
        .unwrap();
    assert!(!again.same_secret(&credential));
    again.keep_token("token-kept-0004", None).unwrap();
    secrets.clear("@test:shop").unwrap();
    assert!(again.authorization(&api).is_none());
    // A Bearer secret is not a client pair and has no token request.
    secrets.set("@test:shop", "plain-key").unwrap();
    let bearer = secrets.credential("@test:shop", API).unwrap().unwrap();
    assert!(bearer.token_request().is_none());
    assert!(bearer.companion_headers().is_empty());
}

#[test]
fn a_token_address_is_validated_where_the_manifest_is_read() {
    use crate::plugin_installation::validate_credential_token_url as validate;
    let hosts = vec!["api.igdb.com".to_owned(), "id.twitch.tv".to_owned()];
    assert_eq!(validate(None, None, &hosts), Ok(()));
    assert_eq!(
        validate(
            Some("https://id.twitch.tv/oauth2/token"),
            Some("api.igdb.com"),
            &hosts
        ),
        Ok(())
    );
    for invalid in [
        "http://id.twitch.tv/oauth2/token",
        "https://other.example/oauth2/token",
        "https://id.twitch.tv:443/oauth2/token",
        "https://id.twitch.tv/oauth2/token?x=1",
        "https://id.twitch.tv/oauth2/token#x",
        "https://u:p@id.twitch.tv/oauth2/token",
        "https://ID.twitch.tv/oauth2/token",
        "https://1.2.3.4/oauth2/token",
        "id.twitch.tv",
    ] {
        assert!(
            validate(Some(invalid), Some("api.igdb.com"), &hosts).is_err(),
            "{invalid}"
        );
    }
    assert!(validate(Some("https://id.twitch.tv/oauth2/token"), None, &hosts).is_err());
}
