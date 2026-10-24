use crate::{
    error::CliError,
    module::{Context, Module},
    output::{self, Output},
};
use clap::{ArgMatches, Command};
use korrid::{
    identity::IdentityState,
    network::{NetworkConnection, NetworkStatusOutcome, NetworkStatusRequest, Wifi},
    *,
};

pub struct Status;

impl Module for Status {
    fn command(&self) -> Command {
        Command::new("status").about("Show live Korri state")
    }
    fn run(&self, _: &ArgMatches, context: &Context) -> Result<Output, CliError> {
        let requests = [
            RpcRequest::Health(HealthRequest {}),
            RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}),
            RpcRequest::IdentityStatus(IdentityStatusRequest {}),
            RpcRequest::SessionStatus(SessionStatusRequest {}),
            RpcRequest::PeerList(PeerListRequest {}),
            RpcRequest::AttentionList(AttentionListRequest {}),
            RpcRequest::NetworkStatus(NetworkStatusRequest {}),
        ];
        let responses = context.collect(&requests)?;
        let human = responses
            .iter()
            .map(describe)
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Output::many(responses, human))
    }
}

fn describe(response: &RpcResponse) -> String {
    match response {
        RpcResponse::Health(HealthOutcome::Ok(health)) => {
            format!("Daemon: running ({})", output::text(&health.version))
        }
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings)) => format!(
            "Device: {}",
            settings
                .device_name
                .as_deref()
                .map(output::text)
                .unwrap_or_else(|| "unknown".into())
        ),
        RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(status)) => match &status.identity {
            IdentityState::Owned {
                device_public_key,
                owner_public_key,
                ..
            } => format!("Device key: {device_public_key}\nOwner: {owner_public_key}"),
            IdentityState::Unowned { device_public_key } => {
                format!("Device key: {device_public_key}\nOwner: none (unowned)")
            }
            IdentityState::Revoked {
                device_public_key, ..
            } => format!("Device key: {device_public_key}\nOwner: revoked"),
            IdentityState::Invalid { .. } => {
                "Device key: unknown\nOwner: unknown (invalid identity)".into()
            }
        },
        RpcResponse::SessionStatus(SessionStatusOutcome::Ok(status)) => {
            let mut line = match &status.active {
                Some(active) => format!(
                    "Live session: {}",
                    active
                        .title
                        .as_deref()
                        .or(active.game_id.as_deref())
                        .map(output::text)
                        .unwrap_or_else(|| active.launch_id.clone())
                ),
                None if status.observation_failure.is_some() => "Live session: unknown".into(),
                None => "Live session: none".into(),
            };
            if let Some(error) = &status.observation_failure {
                line.push_str(&format!(" ({})", output::text(&error.message)));
            }
            line
        }
        RpcResponse::PeerList(PeerListOutcome::Ok(list)) => {
            let mut lines = vec![format!(
                "Peers: {} (directory observations, not a network reachability probe)",
                list.peers.len()
            )];
            for peer in &list.peers {
                lines.push(format!(
                    "  {} {}: {:?}; reachability unknown",
                    output::text(&peer.label),
                    peer.device_public_key,
                    peer.state
                ));
            }
            lines.join("\n")
        }
        RpcResponse::AttentionList(AttentionOutcome::Ok(list)) => {
            format!("Attention: {}", list.items.len())
        }
        RpcResponse::NetworkStatus(NetworkStatusOutcome::Ok(network)) => {
            let connection = match &network.connection {
                NetworkConnection::Offline => "offline".into(),
                NetworkConnection::Wired => "wired".into(),
                NetworkConnection::Wifi { ssid, .. } => format!("Wi-Fi {}", output::text(ssid)),
            };
            let wifi = match network.wifi {
                Wifi::NoAdapter => "no adapter",
                Wifi::Available { .. } => "available",
            };
            format!(
                "Network: {connection}\nWi-Fi: {wifi}\nAddress: unknown (not supplied by korrid)"
            )
        }
        RpcResponse::Health(_) => output::unknown("Daemon", response),
        RpcResponse::SettingsSnapshot(_) => output::unknown("Device", response),
        RpcResponse::IdentityStatus(_) => output::unknown("Owner / device key", response),
        RpcResponse::SessionStatus(_) => output::unknown("Live session", response),
        RpcResponse::PeerList(_) => output::unknown("Peers / reachability", response),
        RpcResponse::AttentionList(_) => output::unknown("Attention", response),
        RpcResponse::NetworkStatus(_) => output::unknown("Network / Wi-Fi / address", response),
        _ => output::unknown("Section", response),
    }
}
