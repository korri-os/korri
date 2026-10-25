use crate::{client::Client, error::CliError};
use korrid::{identity::IdentityState, *};

pub struct TargetFacts {
    pub identity: IdentityState,
    pub name: Option<String>,
    pub peers: Vec<PeerListEntry>,
}

pub fn read(client: &Client) -> Result<TargetFacts, CliError> {
    let identity = match client.request(&RpcRequest::IdentityStatus(IdentityStatusRequest {}))? {
        RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(status)) => status.identity,
        response => return Err(response_error(&response)),
    };
    let name = match client.request(&RpcRequest::SettingsSnapshot(SettingsSnapshotRequest {}))? {
        RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Ok(settings)) => {
            settings.device_name
        }
        response => return Err(response_error(&response)),
    };
    let peers = match client.request(&RpcRequest::PeerList(PeerListRequest {}))? {
        RpcResponse::PeerList(PeerListOutcome::Ok(list)) => list.peers,
        response => return Err(response_error(&response)),
    };
    Ok(TargetFacts {
        identity,
        name,
        peers,
    })
}

pub fn key(identity: &IdentityState) -> Option<&str> {
    match identity {
        IdentityState::Owned {
            device_public_key, ..
        }
        | IdentityState::Unowned { device_public_key }
        | IdentityState::Revoked {
            device_public_key, ..
        } => Some(device_public_key),
        IdentityState::Invalid { .. } => None,
    }
}

/// None selects this device. PeerList contains only verified owner devices;
/// korrid independently checks the live evidence again before forwarding.
pub fn resolve(
    facts: &TargetFacts,
    selector: &str,
    json: bool,
) -> Result<Option<String>, CliError> {
    let local_key = key(&facts.identity);
    if local_key == Some(selector) {
        return Ok(None);
    }
    if let Some(peer) = facts
        .peers
        .iter()
        .find(|peer| peer.device_public_key == selector)
    {
        return Ok(Some(peer.device_public_key.clone()));
    }
    let mut matches = Vec::new();
    if facts
        .name
        .as_ref()
        .is_some_and(|name| name.to_lowercase() == selector.to_lowercase())
    {
        if let Some(key) = local_key {
            matches.push((key, facts.name.as_deref().unwrap(), true));
        }
    }
    for peer in &facts.peers {
        if peer.label.to_lowercase() == selector.to_lowercase() {
            matches.push((peer.device_public_key.as_str(), peer.label.as_str(), false));
        }
    }
    match matches.as_slice() {
        [(key, _, local)] => Ok(if *local { None } else { Some((*key).into()) }),
        [] => Err(CliError::failure(
            "DeviceNotFound",
            "No verified owner device matches that name or key.",
            "Run korri devices list. Use SSH or local access for another owner's device.",
        )),
        _ => {
            use std::io::IsTerminal;
            if !json
                && crate::prompt::interactive(
                    std::io::stdin().is_terminal(),
                    std::io::stdout().is_terminal(),
                )
            {
                let choices = matches
                    .iter()
                    .map(|(key, name, local)| {
                        (
                            (*key).to_owned(),
                            if *local {
                                format!("{name} (current)")
                            } else {
                                (*name).to_owned()
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                let chosen = crate::prompt::select("device", &choices)?;
                return Ok(if local_key == Some(chosen.as_str()) {
                    None
                } else {
                    Some(chosen)
                });
            }
            Err(CliError::failure(
                "DeviceAmbiguous",
                format!(
                    "The device name is ambiguous: {}",
                    matches
                        .iter()
                        .map(|(key, name, _)| format!("{} ({key})", crate::output::text(name)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                "Use an exact device key from korri devices list.",
            ))
        }
    }
}

pub fn response_error(response: &RpcResponse) -> CliError {
    crate::output::failure(response)
        .map(CliError::rpc)
        .unwrap_or_else(|| {
            CliError::failure(
                "InvalidResponse",
                "Korrid answered a different operation.",
                "Check the installed Korri version.",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn facts() -> TargetFacts {
        TargetFacts {
            identity: IdentityState::Owned {
                device_public_key: "local".into(),
                owner_public_key: "owner".into(),
                event_id: "event".into(),
                created_at: 1,
            },
            name: Some("Handheld".into()),
            peers: vec![PeerListEntry {
                device_public_key: "peer".into(),
                label: "handheld".into(),
                state: PeerListState::Ready,
                updated_at: 1u32.into(),
                last_error: None,
            }],
        }
    }
    #[test]
    fn exact_keys_work_but_duplicate_labels_never_guess() {
        let facts = facts();
        assert_eq!(resolve(&facts, "local", false).unwrap(), None);
        assert_eq!(resolve(&facts, "peer", false).unwrap(), Some("peer".into()));
        assert_eq!(
            resolve(&facts, "HANDHELD", true).unwrap_err().failure.code,
            "DeviceAmbiguous"
        );
        assert_eq!(
            resolve(&facts, "unknown", false).unwrap_err().failure.code,
            "DeviceNotFound"
        );
        let mut facts = facts;
        facts.name = Some("Current".into());
        assert_eq!(
            resolve(&facts, "HANDHELD", true).unwrap(),
            Some("peer".into())
        );
        facts.peers.push(PeerListEntry {
            device_public_key: "second".into(),
            ..facts.peers[0].clone()
        });
        assert_eq!(
            resolve(&facts, "HANDHELD", true).unwrap_err().failure.code,
            "DeviceAmbiguous"
        );
    }
    #[test]
    fn invalid_identity_never_identifies_current_device() {
        let mut facts = facts();
        facts.identity = IdentityState::Invalid {
            reason: "invalid".into(),
        };
        assert_eq!(
            resolve(&facts, "local", false).unwrap_err().failure.code,
            "DeviceNotFound"
        );
    }
}
