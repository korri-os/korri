//! ADR 0040: same-owner netplay. Wire facts never contain the session secret.
pub(crate) mod holder;
pub(crate) mod password;
pub(crate) mod receiver;
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunnerNetplay {
    pub library: String,
    pub version: String,
}

#[typeshare(serialized_as = "PluginLaunchNetplayWire")]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase", deny_unknown_fields)]
pub enum PluginLaunchNetplay {
    Host {
        #[serde(rename = "passwordConfig")]
        password_config: String,
    },
    Client {
        address: String,
        port: u16,
        player: u8,
        #[serde(rename = "passwordConfig")]
        password_config: String,
    },
}
impl PluginLaunchNetplay {
    pub(crate) fn is_client(&self) -> bool {
        matches!(self, Self::Client { .. })
    }
}

#[typeshare(serialized_as = "InviteNetplayWire")]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "_tag")]
pub enum InviteNetplay {
    Ready,
    NeedsCopy {
        bytes: f64,
    },
    NeedsPlugin {
        #[serde(rename = "pluginId")]
        plugin_id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bytes: Option<f64>,
    },
    Unavailable {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn netplay_launch_input_exposes_a_config_path_not_a_password() {
        let input: crate::launcher::plugin_launch::PluginLaunchInput = serde_json::from_value(serde_json::json!({
            "runnerId":"core", "program":"retroarch", "accountRoot":"/users/person", "saveRoot":"/users/person/saves/game", "cacheRoot":"/cache", "files":{}, "artifacts":{},
            "netplay":{"role":"client", "address":"127.0.0.1", "port":1234,"player":2,"passwordConfig":"/config"}
        })).unwrap();
        let wire = serde_json::to_value(input).unwrap();
        assert_eq!(wire["netplay"]["passwordConfig"], "/config");
        assert!(wire["netplay"].get("password").is_none());
        assert!(serde_json::from_value::<PluginLaunchNetplay>(serde_json::json!({"role":"client","address":"127.0.0.1","port":65536,"player":2,"passwordConfig":"/config"})).is_err());
    }
    #[test]
    fn runner_netplay_has_exactly_library_and_version() {
        assert!(serde_json::from_value::<RunnerNetplay>(
            serde_json::json!({"library":"Snes9x 2010","version":"1.52.4fe690dd","other":true})
        )
        .is_err());
        let source = "export const name='core'; export const runners={core:{id:'@korri:core/core',program:'program',systems:['snes'],netplay:{library:'Snes9x 2010',version:'1.52.4fe690dd'}}}; export const handlers={'launch.prepare': input=>({command:input.program,args:[]})};";
        let registry = crate::plugin::load_plugin_source("@korri", source);
        assert!(registry.is_ok(), "{registry:?}");
    }
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplayRunner {
    pub plugin_id: String,
    pub runner_id: String,
    pub netplay: RunnerNetplay,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplaySession {
    pub release_key: String,
    #[typeshare(serialized_as = "number")]
    pub release_bytes: u64,
    pub runner: NetplayRunner,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionNetplayRequest {
    pub launch_id: String,
    pub device_label: String,
}
#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNetplayPrepared {
    pub seat: u8,
    pub port: u16,
    pub password: String,
    pub release_key: String,
    #[typeshare(serialized_as = "number")]
    pub release_bytes: u64,
    pub runner: NetplayRunner,
}
impl std::fmt::Debug for SessionNetplayPrepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionNetplayPrepared")
            .field("seat", &self.seat)
            .field("password", &"[redacted]")
            .finish_non_exhaustive()
    }
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionNetplayOutcome {
    Ok(SessionNetplayPrepared),
    Err(crate::RpcFailure),
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseObjectRequest {
    pub release_key: String,
}

#[typeshare(serialized_as = "NetplayPhaseWire")]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub enum NetplayPhase {
    Copying {
        #[typeshare(serialized_as = "number")]
        done: u64,
        #[typeshare(serialized_as = "number")]
        total: u64,
    },
    Connecting,
    Playing,
    HolderLeft,
    Reconnecting,
}
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplayStatus {
    pub launch_id: String,
    pub device_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_title: Option<String>,
    pub phase: NetplayPhase,
}

pub(crate) fn failure(code: &str, message: &str) -> crate::RpcFailure {
    crate::RpcFailure {
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod wire_tests {
    use crate::{
        authorization::{policy_for, PeerPolicy},
        RpcRequest,
    };
    #[test]
    fn netplay_peer_rpcs_are_owner_only_and_the_invite_action_is_local_only() {
        for (tag, payload, policy) in [
            (
                "app.session.netplay",
                serde_json::json!({"launchId":"exact","deviceLabel":"Device"}),
                PeerPolicy::OwnerDeviceOnly,
            ),
            (
                "app.releases.object",
                serde_json::json!({"releaseKey":format!("sha256:{}", "1".repeat(64))}),
                PeerPolicy::OwnerDeviceOnly,
            ),
            (
                "app.invites.netplay",
                serde_json::json!({"inviteId":"opaque"}),
                PeerPolicy::LocalOnly,
            ),
        ] {
            let wire = serde_json::json!({"_tag":tag,"payload":payload});
            let request: RpcRequest = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(policy_for(&request), policy);
            assert_eq!(serde_json::to_value(request).unwrap(), wire);
        }
    }
    #[test]
    fn netplay_password_is_redacted_from_all_containing_debug_values() {
        let prepared: super::SessionNetplayPrepared = serde_json::from_value(serde_json::json!({"seat":2,"port":55435,"password":"private-session-value","releaseKey":"release","releaseBytes":1,"runner":{"pluginId":"plugin","runnerId":"runner","netplay":{"library":"Snes9x 2010","version":"1.52.4fe690dd"}}})).unwrap();
        let debug = format!(
            "{:?}",
            crate::RpcResponse::SessionNetplay(super::SessionNetplayOutcome::Ok(prepared))
        );
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("private-session-value"));
    }
}
