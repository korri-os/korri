//! Upstream client: speaks the legacy korrid Effect-RPC wire over HTTP.
//!
//! Envelope (verified against the working hand-rolled client in
//! KorriShellActivity.korriRpc and legacy rpc schemas):
//!   request  {"_tag":"Request","id":"<bigint-string>","tag":<op>,
//!             "payload":{...},"headers":[]}
//!   response frames array; the frame carrying "exit" resolves the call:
//!   exit._tag == "Success" -> exit.value, otherwise a failure exit.
//!
//! This wire format is scaffolding: it exists so the Android-local brain
//! can orchestrate today's TS daemon on the host. When the host daemon is
//! rewritten, this module is replaced by the Rust-owned treaty.

use crate::{upstreams::UpstreamError, GameIdentity};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct UpstreamConfig {
    pub base_url: String,
}

impl UpstreamConfig {
    pub fn from_env() -> Self {
        Self {
            // Scaffolding default: aka's LAN control URL. Replaced by
            // pairing-derived discovery when the shell owns upstream wiring.
            base_url: std::env::var("KORRID_UPSTREAM_URL")
                // Every device is on Tailscale, so peers are named rather
                // than addressed: a LAN IP only works on one network and
                // breaks the moment a device leaves the house.
                .unwrap_or_else(|_| "http://aka:3001".into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpstreamCatalogEntry {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub launchable: bool,
    #[serde(default)]
    pub releases: Vec<UpstreamCatalogRelease>,
}

impl UpstreamCatalogEntry {
    pub fn single_identity(&self) -> Option<GameIdentity> {
        let mut identities = self
            .releases
            .iter()
            .filter_map(|release| release.identity.clone());
        let first = identities.next()?;
        identities
            .all(|identity| identity == first)
            .then_some(first)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpstreamCatalogRelease {
    #[serde(default)]
    pub identity: Option<GameIdentity>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpstreamCatalog {
    pub entries: Vec<UpstreamCatalogEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamPrepared {
    pub game_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamActiveSession {
    #[serde(default)]
    pub netplay: Option<Box<crate::netplay::NetplaySession>>,
    #[serde(default)]
    pub person_public_key: Option<String>,
    #[serde(default)]
    pub stream_players: Vec<crate::stream_play::StreamPlayer>,
    pub launch_id: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub game_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,
}

/// Legacy `app.session.status` union. Lenient: only the fields korrid
/// forwards are decoded; everything else is tolerated and dropped.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "_tag")]
pub enum UpstreamSessionStatus {
    SessionStatus {
        #[serde(default)]
        active: Option<UpstreamActiveSession>,
    },
    SessiondNotConfigured {},
    HostUnavailable {},
}

/// Aka's external Effect-RPC `app.session.stop` union. This is not the native
/// korrid product wire; its producer has not switched protocols.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "_tag")]
pub enum UpstreamSessionStop {
    Stopped {
        #[serde(rename = "launchId", default)]
        launch_id: Option<String>,
    },
    StopPending {
        #[serde(rename = "launchId", default)]
        launch_id: Option<String>,
    },
    NothingToStop {},
    ConfirmationRequired {
        #[serde(default)]
        action: Option<String>,
    },
    SessiondNotConfigured {},
    HostUnavailable {},
}

#[derive(Clone)]
pub struct UpstreamClient {
    config: UpstreamConfig,
    http: reqwest::Client,
}

impl UpstreamClient {
    pub fn new(config: UpstreamConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("construct reqwest client");
        Self { config, http }
    }

    pub async fn catalog_snapshot(&self) -> Result<UpstreamCatalog, UpstreamError> {
        let value = self
            .call("app.catalog.snapshot", json!({"scope": "self"}))
            .await?;
        serde_json::from_value(value).map_err(|error| UpstreamError::Wire(error.to_string()))
    }

    pub async fn prepare_stream(&self, game_id: &str) -> Result<UpstreamPrepared, UpstreamError> {
        let value = self
            .call("app.server.stream.prepare", json!({"id": game_id}))
            .await?;
        serde_json::from_value(value).map_err(|error| UpstreamError::Wire(error.to_string()))
    }

    pub async fn session_status(&self) -> Result<UpstreamSessionStatus, UpstreamError> {
        let value = self.call("app.session.status", json!({})).await?;
        serde_json::from_value(value).map_err(|error| UpstreamError::Wire(error.to_string()))
    }

    pub async fn session_stop(&self, force: bool) -> Result<UpstreamSessionStop, UpstreamError> {
        // The portal's explicit stop action is the user confirmation. The
        // legacy host refuses mutation without this bit and returns
        // ConfirmationRequired.
        let value = self
            .call("app.session.stop", session_stop_payload(force))
            .await?;
        serde_json::from_value(value).map_err(|error| UpstreamError::Wire(error.to_string()))
    }

    async fn call(&self, tag: &str, payload: Value) -> Result<Value, UpstreamError> {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_micros()
            .to_string();
        let body = json!({
            "_tag": "Request",
            "id": id,
            "tag": tag,
            "payload": payload,
            "headers": [],
        });
        let response = self
            .http
            .post(format!("{}/api/rpc", self.config.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|error| UpstreamError::Unreachable(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(UpstreamError::Http(status.as_u16()));
        }
        let text = response
            .text()
            .await
            .map_err(|error| UpstreamError::Wire(error.to_string()))?;
        decode_exit_value(&text)
    }
}

fn session_stop_payload(force: bool) -> Value {
    if force {
        json!({"force": true, "confirmed": true})
    } else {
        json!({"confirmed": true})
    }
}

/// Extracts the Success exit value from an Effect-RPC response body.
fn decode_exit_value(body: &str) -> Result<Value, UpstreamError> {
    let parsed: Value =
        serde_json::from_str(body).map_err(|error| UpstreamError::Wire(error.to_string()))?;
    let frames: Vec<Value> = match parsed {
        Value::Array(frames) => frames,
        other => vec![other],
    };
    for frame in frames {
        let Some(exit) = frame.get("exit") else {
            continue;
        };
        if exit.get("_tag").and_then(Value::as_str) == Some("Success") {
            return Ok(exit.get("value").cloned().unwrap_or(Value::Null));
        }
        return Err(UpstreamError::Failure(exit.to_string()));
    }
    Err(UpstreamError::Wire("no Exit frame in RPC response".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_success_exit_from_a_frame_array() {
        let body = r#"[{"_tag":"Exit","requestId":"1","exit":{"_tag":"Success","value":{"entries":[{"id":"g1","title":"Skate 3","launchable":true}]}}}]"#;
        let value = decode_exit_value(body).expect("success exit");
        let catalog: UpstreamCatalog = serde_json::from_value(value).expect("catalog shape");
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].title.as_deref(), Some("Skate 3"));
        assert!(catalog.entries[0].launchable);
    }

    #[test]
    fn preserves_one_unambiguous_legacy_release_identity() {
        let hash: UpstreamCatalogEntry = serde_json::from_value(json!({
            "id": "wl4",
            "launchable": true,
            "releases": [{
                "identity": {
                    "kind": "hash",
                    "value": "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414"
                }
            }]
        }))
        .unwrap();
        assert_eq!(
            hash.single_identity(),
            Some(GameIdentity::Hash(
                "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414".into()
            ))
        );

        let ambiguous: UpstreamCatalogEntry = serde_json::from_value(json!({
            "id": "collection",
            "releases": [
                {"identity": {"kind": "provider", "value": {"provider": "steam", "ref": "1"}}},
                {"identity": {"kind": "provider", "value": {"provider": "steam", "ref": "2"}}}
            ]
        }))
        .unwrap();
        assert_eq!(ambiguous.single_identity(), None);
    }

    #[test]
    fn surfaces_a_failure_exit_as_an_error() {
        let body = r#"[{"exit":{"_tag":"Failure","cause":{"_tag":"Fail"}}}]"#;
        let error = decode_exit_value(body).expect_err("failure exit");
        assert!(matches!(error, UpstreamError::Failure(_)));
    }

    #[test]
    fn rejects_bodies_without_an_exit_frame() {
        let error = decode_exit_value(r#"[{"_tag":"Pong"}]"#).expect_err("no exit frame");
        assert!(matches!(error, UpstreamError::Wire(_)));
    }

    #[test]
    fn decodes_an_active_session_from_a_status_exit() {
        let body = r#"[{"exit":{"_tag":"Success","value":{"_tag":"SessionStatus","configured":true,"mode":"stream","active":{"launchId":"l1","mode":"stream","phase":"running","gameId":"g1","title":"Skate 3"}}}}]"#;
        let value = decode_exit_value(body).expect("success exit");
        let status: UpstreamSessionStatus = serde_json::from_value(value).expect("status shape");
        let UpstreamSessionStatus::SessionStatus { active } = status else {
            panic!("expected SessionStatus");
        };
        let active = active.expect("active session");
        assert_eq!(active.launch_id, "l1");
        assert_eq!(active.game_id.as_deref(), Some("g1"));
        assert_eq!(active.title.as_deref(), Some("Skate 3"));
        assert_eq!(active.phase.as_deref(), Some("running"));
    }

    #[test]
    fn decodes_nothing_playing_when_active_is_absent() {
        let value = serde_json::json!({"_tag":"SessionStatus","configured":true,"mode":"idle"});
        let status: UpstreamSessionStatus = serde_json::from_value(value).expect("status shape");
        assert!(matches!(
            status,
            UpstreamSessionStatus::SessionStatus { active: None }
        ));
    }

    #[test]
    fn decodes_not_configured_and_host_unavailable_status_variants() {
        let not_configured: UpstreamSessionStatus =
            serde_json::from_value(serde_json::json!({"_tag":"SessiondNotConfigured"}))
                .expect("variant");
        assert!(matches!(
            not_configured,
            UpstreamSessionStatus::SessiondNotConfigured {}
        ));
        let unavailable: UpstreamSessionStatus =
            serde_json::from_value(serde_json::json!({"_tag":"HostUnavailable","reason":"down"}))
                .expect("variant");
        assert!(matches!(
            unavailable,
            UpstreamSessionStatus::HostUnavailable {}
        ));
    }

    #[test]
    fn stop_payload_carries_explicit_user_confirmation() {
        assert_eq!(
            session_stop_payload(false),
            serde_json::json!({"confirmed": true})
        );
        assert_eq!(
            session_stop_payload(true),
            serde_json::json!({"confirmed": true, "force": true})
        );
    }

    #[test]
    fn decodes_stopped_and_pending_stop_variants() {
        let stopped: UpstreamSessionStop =
            serde_json::from_value(serde_json::json!({"_tag":"Stopped","launchId":"l1"}))
                .expect("stopped");
        assert!(matches!(
            stopped,
            UpstreamSessionStop::Stopped { launch_id: Some(_) }
        ));
        let pending: UpstreamSessionStop = serde_json::from_value(
            serde_json::json!({"_tag":"StopPending","launchId":"l1","deadlineMs":5000}),
        )
        .expect("pending");
        assert!(matches!(pending, UpstreamSessionStop::StopPending { .. }));

        let nothing: UpstreamSessionStop =
            serde_json::from_value(serde_json::json!({"_tag":"NothingToStop"}))
                .expect("nothing to stop");
        assert!(matches!(nothing, UpstreamSessionStop::NothingToStop {}));

        let confirmation: UpstreamSessionStop = serde_json::from_value(
            serde_json::json!({"_tag":"ConfirmationRequired","action":"stop-session"}),
        )
        .expect("confirmation required");
        assert!(matches!(
            confirmation,
            UpstreamSessionStop::ConfirmationRequired { .. }
        ));
    }

    #[test]
    fn tolerates_unknown_fields_in_session_responses() {
        let value = serde_json::json!({
            "_tag":"SessionStatus",
            "configured":true,
            "mode":"stream",
            "generation": 9,
            "active":{"launchId":"l2","mode":"stream","extra":{"nested":true}},
        });
        let status: UpstreamSessionStatus = serde_json::from_value(value).expect("lenient decode");
        let UpstreamSessionStatus::SessionStatus { active } = status else {
            panic!("expected SessionStatus");
        };
        let active = active.expect("active session");
        assert_eq!(active.launch_id, "l2");
        assert_eq!(active.game_id, None);
    }

    #[test]
    fn tolerates_unknown_upstream_entry_fields() {
        let value = serde_json::json!({
            "entries": [{"id":"g2","itemId":"i2","launchable":false,"releases":[]}],
            "generation": 4,
        });
        let catalog: UpstreamCatalog = serde_json::from_value(value).expect("lenient decode");
        assert_eq!(catalog.entries[0].title, None);
        assert!(!catalog.entries[0].launchable);
    }
}
