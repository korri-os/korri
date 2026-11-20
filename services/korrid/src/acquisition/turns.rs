//! Request/response turns: a provider handler never performs I/O. It returns
//! its legacy result or the HTTP requests it needs; korrid fetches them and
//! calls the handler again with every response so far (ADR 0016).
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use super::claims::{
    admit_form, DownloadResolution, FinalDownloadMethod, ProviderClaim, ProviderClaimDetails,
};
use crate::plugin::deserialize_optional_non_null as non_null;

pub const CLAIMS_SEARCH: &str = "claims.search";
pub const CLAIMS_DETAILS: &str = "claims.details";
pub const ARTIFACT_RESOLVE_DOWNLOAD: &str = "artifact.resolve-download";

/// Legacy `PluginHttpRequestOptions` value type, without `undefined`.
#[typeshare(serialized_as = "PluginHttpQueryValueWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PluginHttpQueryValue {
    String(String),
    Number(serde_json::Number),
    Boolean(bool),
}

/// A POST body: legacy `PluginHttpRequestOptions.body`, which was
/// `string | Uint8Array | URLSearchParams`. A string is sent as raw UTF-8
/// text, such as an IGDB Apicalypse query (ADR 0016, IGDB amendment); an
/// object of string fields is form-encoded, as `FinalDownload`'s body is.
#[typeshare(serialized_as = "PluginHttpBodyWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PluginHttpBody {
    Text(String),
    Form(BTreeMap<String, String>),
}

/// The legacy `request(url, options)` call as a value: `url` plus the legacy
/// `PluginHttpRequestOptions` fields `query`, `headers`, `timeoutMs`, and
/// GET, the same form POST as `FinalDownload`, or a raw text POST.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginHttpRequest {
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    #[typeshare(serialized_as = "Option<HashMap<String, PluginHttpQueryValue>>")]
    pub query: Option<BTreeMap<String, PluginHttpQueryValue>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    #[typeshare(serialized_as = "Option<HashMap<String, String>>")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    #[typeshare(serialized_as = "Option<number>")]
    pub timeout_ms: Option<u64>,
    /// Absent: GET. POST sends `body`: form fields, or raw text.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub method: Option<FinalDownloadMethod>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub body: Option<PluginHttpBody>,
}

#[typeshare(serialized_as = "PluginHttpRequestsTagWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PluginHttpRequestsTag {
    HttpRequests,
}

/// A handler's "fetch these, then call me again" answer.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginHttpRequests {
    #[serde(rename = "_tag")]
    pub tag: PluginHttpRequestsTag,
    pub requests: Vec<PluginHttpRequest>,
}

/// Legacy `PluginHttpResponse` data fields; `text()` became `text`.
/// `setCookies` and `bytes` are not carried.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginHttpResponse {
    pub status: u16,
    pub ok: bool,
    pub url: String,
    #[typeshare(serialized_as = "HashMap<String, String>")]
    pub headers: BTreeMap<String, String>,
    pub text: String,
}

/// `claims.search` input: legacy `SearchRequest.query` for one provider.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginSearchInput {
    pub provider_id: String,
    pub query: String,
    pub responses: Vec<PluginHttpResponse>,
}

/// `claims.details` input: legacy `DetailsRequest`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDetailsInput {
    pub provider_id: String,
    pub id: String,
    pub responses: Vec<PluginHttpResponse>,
}

/// `artifact.resolve-download` input: legacy `ResolveDownloadRequest` without
/// `size` and `artifactFormat`. `candidateUrl` is the held claim's `url`.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginResolveDownloadInput {
    pub provider_id: String,
    pub candidate_url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub file_name: Option<String>,
    pub responses: Vec<PluginHttpResponse>,
}

#[typeshare(serialized_as = "PluginSearchOutputWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PluginSearchOutput {
    Requests(PluginHttpRequests),
    Result(Vec<ProviderClaim>),
}

#[typeshare(serialized_as = "PluginDetailsOutputWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
// One plugin turn's output, decoded once and consumed.
#[allow(clippy::large_enum_variant)]
pub enum PluginDetailsOutput {
    Requests(PluginHttpRequests),
    Result(ProviderClaimDetails),
}

#[typeshare(serialized_as = "PluginResolveDownloadOutputWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PluginResolveDownloadOutput {
    Requests(PluginHttpRequests),
    Result(DownloadResolution),
}

/// At most 4 handler calls per operation; the fourth must return a result.
pub(super) const MAX_CALLS: usize = 4;
pub(super) const MAX_REQUESTS_PER_TURN: usize = 8;
/// Serialized handler input, all responses included.
pub(super) const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

/// Request headers a provider may not set: no credentials, no host override.
const REFUSED_HEADERS: [&str; 5] = [
    "cookie",
    "set-cookie",
    "authorization",
    "proxy-authorization",
    "host",
];

/// Why a provider operation produced no admitted result. The string is a
/// diagnostic for logs at debug level only; the shell gets fixed copy.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum ProviderError {
    /// Transport: DNS, connect, timeout, stall, oversize body, refused redirect.
    Unreachable(super::Failure),
    /// The plugin: thrown error, invalid output, undeclared host, exceeded bound.
    Failed(String),
    /// The site answered 401 to a request that carried the plugin's secret
    /// (ADR 0032, section 2), or, for a client pair, the token host refused
    /// it or the site answered 401 also to a new token (ADR 0034 amendment).
    /// The operation ends: the handler is not called again with an answer
    /// that only says the secret is wrong.
    Unauthorized,
    /// A response repeated the plugin's secret: in a header, in its body, in
    /// its final address, or in the address of a redirect, which korrid then
    /// did not follow (ADR 0034: the plugin's code never receives the secret
    /// "in a response", and the secret goes to its one host only). No such
    /// response is given to the handler and the operation ends. `host` is the
    /// host that answered, never the value.
    CredentialRepeated { host: String },
}

impl ProviderError {
    pub(super) fn rpc(&self) -> crate::RpcFailure {
        let (code, message) = match self {
            Self::Unreachable(_) => (
                "ProviderUnreachable",
                "The provider is not answering. Check the connection and retry.",
            ),
            Self::Failed(_) | Self::Unauthorized => (
                "ProviderFailed",
                "The provider sent an answer Korri cannot use. Please retry later.",
            ),
            Self::CredentialRepeated { .. } => (
                "ProviderFailed",
                "The provider repeated the plugin's key in its answer. Korri did not use that answer.",
            ),
        };
        crate::RpcFailure {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// One provider: its plugin source, its declared hosts and its id.
#[derive(Clone)]
pub(super) struct ProviderContext {
    pub provider_id: String,
    pub source: std::sync::Arc<crate::script::source::SourceSnapshot>,
    pub hosts: std::sync::Arc<Vec<String>>,
    /// The plugin's secret, when it declares one (ADR 0034). The handler
    /// never receives it: korrid adds it to a request to its one host.
    pub credential: Option<std::sync::Arc<crate::plugin_secrets::PluginCredential>>,
}

pub(super) enum Turn<T> {
    Requests(Vec<PluginHttpRequest>),
    Result(T),
}

/// Run one operation to its result: call, fetch the requested responses,
/// call again with all responses so far, within the ADR 0016 bounds.
pub(super) async fn run<T>(
    engine: &super::download::DownloadEngine,
    context: &ProviderContext,
    operation: &'static str,
    input: impl Fn(Vec<PluginHttpResponse>) -> serde_json::Result<String>,
    decode: impl Fn(&str) -> serde_json::Result<Turn<T>>,
) -> Result<T, ProviderError> {
    // A failure text can hold what a site or the plugin wrote. With the
    // check below none of it can hold the secret; it is taken out all the
    // same before the text can reach a log.
    let failed = |text: String| {
        ProviderError::Failed(match &context.credential {
            Some(credential) => credential.redact(&text),
            None => text,
        })
    };
    let mut responses = Vec::new();
    for call in 1..=MAX_CALLS {
        let input = input(responses.clone()).map_err(|e| failed(e.to_string()))?;
        if input.len() > MAX_INPUT_BYTES {
            return Err(ProviderError::Failed("handler input exceeds 4 MiB".into()));
        }
        let source = context.source.clone();
        let output = tokio::task::spawn_blocking(move || {
            crate::script::call_plugin_operation_bounded(
                &source,
                operation,
                &input,
                MAX_INPUT_BYTES,
            )
        })
        .await
        .map_err(|_| ProviderError::Failed("handler call was interrupted".into()))?
        .map_err(|failure| failed(failure.to_string()))?;
        let requests = match decode(&output)
            .map_err(|e| failed(format!("invalid {operation} output: {e}")))?
        {
            Turn::Result(result) => return Ok(result),
            Turn::Requests(requests) => requests,
        };
        if call == MAX_CALLS {
            return Err(ProviderError::Failed(
                "the fourth handler call must return a result".into(),
            ));
        }
        if requests.is_empty() || requests.len() > MAX_REQUESTS_PER_TURN {
            return Err(ProviderError::Failed(
                "a turn must ask for 1 to 8 requests".into(),
            ));
        }
        let admitted = requests
            .iter()
            .map(|request| admit(request, &context.hosts))
            .collect::<Result<Vec<_>, _>>()?;
        for request in admitted {
            let mut renewed = false;
            let response = loop {
                let response = fetch(engine, context, &request).await?;
                // A client pair's token can expire or be revoked: get a new
                // one once, and ask again (ADR 0034 amendment). A Bearer
                // secret is what the person entered, so a 401 ends at once.
                if response.credential_sent && response.status == 401 {
                    match context.credential.as_deref() {
                        Some(credential) if credential.exchanges() && !renewed => {
                            credential.drop_token();
                            renewed = true;
                            continue;
                        }
                        _ => return Err(ProviderError::Unauthorized),
                    }
                }
                break response;
            };
            if let Some(credential) = &context.credential {
                if repeats_secret(credential, &response) {
                    return Err(ProviderError::CredentialRepeated {
                        host: answering_host(&response.url),
                    });
                }
            }
            responses.push(PluginHttpResponse {
                status: response.status,
                ok: (200..300).contains(&response.status),
                url: response.url,
                headers: response.headers,
                text: response.text,
            });
        }
    }
    unreachable!("the loop returns by the fourth call")
}

/// One admitted request, with the plugin's token got first when the request
/// goes to the credential host and korrid holds none.
async fn fetch(
    engine: &super::download::DownloadEngine,
    context: &ProviderContext,
    request: &Admitted,
) -> Result<super::download::TextResponse, ProviderError> {
    let credential = context.credential.as_deref();
    if let Some(credential) = credential.filter(|credential| credential.exchanges()) {
        if engine.secret_applies(credential, &request.url) {
            engine
                .ensure_token(credential, &context.hosts)
                .await
                .map_err(|failure| match failure {
                    super::download::TokenFailure::Refused => ProviderError::Unauthorized,
                    super::download::TokenFailure::Transport(failure) => {
                        ProviderError::Unreachable(failure)
                    }
                    super::download::TokenFailure::Invalid(reason) => {
                        ProviderError::Failed(credential.redact(&reason))
                    }
                })?;
        }
    }
    engine
        .fetch_text(
            super::download::TextRequest {
                url: request.url.clone(),
                headers: &request.headers,
                timeout: request.timeout,
                body: request
                    .body
                    .as_ref()
                    .map(|(content_type, text)| super::download::PostBody { content_type, text }),
                credential,
            },
            &context.hosts,
        )
        .await
        .map_err(|failure| match failure {
            super::download::ChainFailure::Transport(failure) => {
                ProviderError::Unreachable(failure)
            }
            // The site put the secret in a redirect. korrid followed it
            // nowhere; for the plugin this is the same fault as a response
            // that repeats the secret.
            super::download::ChainFailure::SecretInRedirect { host } => {
                ProviderError::CredentialRepeated { host }
            }
        })
}

/// Whether any part of a response that a handler would be given holds the
/// plugin's secret: a header name or value, the body text, or the final
/// address after redirects. Every response of the turn is checked, also one
/// from a host that was not sent the secret.
fn repeats_secret(
    credential: &crate::plugin_secrets::PluginCredential,
    response: &super::download::TextResponse,
) -> bool {
    credential.appears_in(&response.text)
        || credential.appears_in(&response.url)
        || response
            .headers
            .iter()
            .any(|(name, value)| credential.appears_in(name) || credential.appears_in(value))
}

/// The host of a response's final address, for a diagnostic. An address is
/// never copied whole: its path or query may be what repeated the secret.
fn answering_host(address: &str) -> String {
    url::Url::parse(address)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "the provider".to_owned())
}

struct Admitted {
    url: url::Url,
    headers: Vec<(String, String)>,
    timeout: Option<std::time::Duration>,
    /// A POST's Content-Type, which korrid owns, and its body.
    body: Option<(&'static str, String)>,
}

fn admit(request: &PluginHttpRequest, hosts: &[String]) -> Result<Admitted, ProviderError> {
    let failed = |reason: &str| ProviderError::Failed(reason.to_owned());
    super::claims::safe_url(&request.url).map_err(|e| failed(e.0))?;
    let mut url = url::Url::parse(&request.url).map_err(|_| failed("invalid request URL"))?;
    super::policy::declared(&url, super::policy::Hosts::Only(hosts))
        .map_err(|_| failed("request to an undeclared host"))?;
    if let Some(query) = &request.query {
        // Legacy withQuery uses searchParams.set: the first same-named
        // parameter takes the value in place, later ones go, new keys append.
        let text = |value: &PluginHttpQueryValue| match value {
            PluginHttpQueryValue::String(value) => value.clone(),
            PluginHttpQueryValue::Number(value) => value.to_string(),
            PluginHttpQueryValue::Boolean(value) => value.to_string(),
        };
        let mut set = std::collections::BTreeSet::new();
        let mut pairs = Vec::new();
        for (key, value) in url.query_pairs() {
            match query.get(key.as_ref()) {
                Some(_) if set.contains(key.as_ref()) => {}
                Some(replacement) => {
                    set.insert(key.to_string());
                    pairs.push((key.into_owned(), text(replacement)));
                }
                None => pairs.push((key.into_owned(), value.into_owned())),
            }
        }
        for (key, value) in query {
            if !set.contains(key) {
                pairs.push((key.clone(), text(value)));
            }
        }
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    let mut headers = Vec::new();
    for (name, value) in request.headers.iter().flatten() {
        let lower = name.to_ascii_lowercase();
        if REFUSED_HEADERS.contains(&lower.as_str())
            || reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err()
            || reqwest::header::HeaderValue::from_str(value).is_err()
        {
            return Err(failed("refused or invalid request header"));
        }
        if request.method.is_some() && lower == "content-type" {
            let text = matches!(request.body, Some(PluginHttpBody::Text(_)));
            let matching = if text {
                is_text_content_type(value)
            } else {
                value.eq_ignore_ascii_case(super::download::FORM_CONTENT_TYPE)
            };
            if !matching {
                return Err(failed(if text {
                    "a text POST needs the text/plain content type"
                } else {
                    "a form POST needs the form content type"
                }));
            }
            // The shared sender owns this header, as for FinalDownload.
            continue;
        }
        headers.push((lower, value.clone()));
    }
    let timeout = match request.timeout_ms {
        Some(0) => return Err(failed("timeoutMs must be positive")),
        Some(ms) => Some(std::time::Duration::from_millis(ms)),
        None => None,
    };
    let body = match &request.body {
        Some(PluginHttpBody::Text(text)) => {
            if request.method.is_none() {
                return Err(failed("download body needs method POST"));
            }
            // The same 8192-character bound as a form (ADR 0016 amendment).
            super::claims::safe_text(text).map_err(|error| failed(error.0))?;
            Some((super::download::TEXT_CONTENT_TYPE, text.clone()))
        }
        Some(PluginHttpBody::Form(fields)) => admit_form(request.method, &Some(fields.clone()))
            .map_err(|error| failed(error.0))?
            .map(|form| (super::download::FORM_CONTENT_TYPE, form)),
        None => admit_form(request.method, &None)
            .map_err(|error| failed(error.0))?
            .map(|form| (super::download::FORM_CONTENT_TYPE, form)),
    };
    Ok(Admitted {
        url,
        headers,
        timeout,
        body,
    })
}

/// `text/plain`, alone or with `charset=utf-8`: what korrid sends with a
/// text body. Any other value would describe bytes korrid does not send.
fn is_text_content_type(value: &str) -> bool {
    let mut parts = value.split(';').map(str::trim);
    let media = parts.next().unwrap_or_default();
    let rest: Vec<_> = parts.filter(|part| !part.is_empty()).collect();
    media.eq_ignore_ascii_case("text/plain")
        && match rest.as_slice() {
            [] => true,
            [charset] => charset.split_once('=').is_some_and(|(name, value)| {
                name.trim().eq_ignore_ascii_case("charset")
                    && value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
            }),
            _ => false,
        }
}

fn requests<T>(value: &serde_json::Value) -> Option<serde_json::Result<Turn<T>>> {
    (value.get("_tag").and_then(serde_json::Value::as_str) == Some("HttpRequests")).then(|| {
        serde_json::from_value::<PluginHttpRequests>(value.clone())
            .map(|requests| Turn::Requests(requests.requests))
    })
}

/// The tag decides: `_tag: "HttpRequests"` asks for data, anything else is
/// the result, decoded strictly as the legacy type.
pub(super) fn decode_turn<T: serde::de::DeserializeOwned>(
    output: &str,
) -> serde_json::Result<Turn<T>> {
    let value: serde_json::Value = serde_json::from_str(output)?;
    requests(&value).unwrap_or_else(|| serde_json::from_value(value).map(Turn::Result))
}

pub(super) async fn search(
    engine: &super::download::DownloadEngine,
    context: &ProviderContext,
    query: &str,
) -> Result<Vec<ProviderClaim>, ProviderError> {
    let claims: Vec<ProviderClaim> = run(
        engine,
        context,
        CLAIMS_SEARCH,
        |responses| {
            serde_json::to_string(&PluginSearchInput {
                provider_id: context.provider_id.clone(),
                query: query.to_owned(),
                responses,
            })
        },
        decode_turn,
    )
    .await?;
    for claim in &claims {
        claim
            .admit(&context.provider_id)
            .map_err(|e| ProviderError::Failed(e.0.into()))?;
    }
    Ok(claims)
}

pub(super) async fn details(
    engine: &super::download::DownloadEngine,
    context: &ProviderContext,
    id: &str,
) -> Result<ProviderClaimDetails, ProviderError> {
    let details: ProviderClaimDetails = run(
        engine,
        context,
        CLAIMS_DETAILS,
        |responses| {
            serde_json::to_string(&PluginDetailsInput {
                provider_id: context.provider_id.clone(),
                id: id.to_owned(),
                responses,
            })
        },
        decode_turn,
    )
    .await?;
    details
        .admit(&context.provider_id, id)
        .map_err(|e| ProviderError::Failed(e.0.into()))?;
    Ok(details)
}

pub(super) async fn resolve_download(
    engine: &super::download::DownloadEngine,
    context: &ProviderContext,
    candidate_url: &str,
    file_name: Option<&str>,
) -> Result<DownloadResolution, ProviderError> {
    let resolution: DownloadResolution = run(
        engine,
        context,
        ARTIFACT_RESOLVE_DOWNLOAD,
        |responses| {
            serde_json::to_string(&PluginResolveDownloadInput {
                provider_id: context.provider_id.clone(),
                candidate_url: candidate_url.to_owned(),
                file_name: file_name.map(str::to_owned),
                responses,
            })
        },
        decode_turn,
    )
    .await?;
    resolution
        .admit(&context.provider_id)
        .map_err(|e| ProviderError::Failed(e.0.into()))?;
    if let DownloadResolution::Final(download) = &resolution {
        let url = url::Url::parse(&download.url)
            .map_err(|_| ProviderError::Failed("invalid download URL".into()))?;
        super::policy::declared(&url, super::policy::Hosts::Only(&context.hosts))
            .map_err(|_| ProviderError::Failed("download from an undeclared host".into()))?;
    }
    Ok(resolution)
}
