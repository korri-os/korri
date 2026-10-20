//! Request/response turns: a provider handler never performs I/O. It returns
//! its legacy result or the HTTP requests it needs; korrid fetches them and
//! calls the handler again with every response so far (ADR 0013).
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use super::claims::{DownloadResolution, ProviderClaim, ProviderClaimDetails};
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

/// The legacy `request(url, options)` call as a value: `url` plus the legacy
/// `PluginHttpRequestOptions` fields `query`, `headers`, `timeoutMs`. GET only.
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
