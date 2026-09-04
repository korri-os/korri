use crate::{identity::IdentityState, RpcRequest};
use nostr::event::{Event, Kind};

pub const PASS_EVENT_KIND: u16 = 30_079;
pub const STREAM_LAUNCH_SCOPE: &str = "stream.launch";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrincipalTier {
    Owner,
    Household,
    Guest,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Principal {
    pub device_public_key: String,
    pub tier: PrincipalTier,
    pub scopes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainAction {
    CatalogRead,
    MoonlightResolve,
    StreamLaunch,
    StreamCancel,
    CertificateAttest,
    CertificateProvision,
    CertificateRevoke,
    SessionRead,
    SessionStop,
    SessionControl,
    LocalGamesRead,
    LocalGameLaunch,
    HealthRead,
    DiscoveryRead,
    DiscoveryWrite,
    SettingsRead,
    SettingsWrite,
    SecretWrite,
}

impl DomainAction {
    pub fn is_security_mutation(self) -> bool {
        matches!(
            self,
            Self::CertificateProvision
                | Self::CertificateRevoke
                | Self::DiscoveryWrite
                | Self::SettingsWrite
                | Self::SecretWrite
        )
    }
}

pub fn action_for(request: &RpcRequest) -> DomainAction {
    match request {
        RpcRequest::CatalogSnapshot(_) => DomainAction::CatalogRead,
        RpcRequest::MoonlightResolve(_) => DomainAction::MoonlightResolve,
        RpcRequest::MoonlightLaunchPrepare(_) | RpcRequest::SessionPrepare(_) => {
            DomainAction::StreamLaunch
        }
        RpcRequest::MoonlightLaunchCancel(_) => DomainAction::StreamCancel,
        RpcRequest::MoonlightCertificateAttest(_) => DomainAction::CertificateAttest,
        RpcRequest::MoonlightCertificateProvision(_) => DomainAction::CertificateProvision,
        RpcRequest::MoonlightCertificateRevoke(_) => DomainAction::CertificateRevoke,
        RpcRequest::SessionStatus(_) | RpcRequest::SessionControls(_) => DomainAction::SessionRead,
        RpcRequest::SessionStop(_) => DomainAction::SessionStop,
        RpcRequest::SessionControlInvoke(_) => DomainAction::SessionControl,
        RpcRequest::LocalGamesList(_) => DomainAction::LocalGamesRead,
        RpcRequest::LocalGameLaunch(_) => DomainAction::LocalGameLaunch,
        RpcRequest::Health(_) => DomainAction::HealthRead,
        RpcRequest::DiscoverySnapshot(_) => DomainAction::DiscoveryRead,
        RpcRequest::DiscoveryRegisterReceipt(_)
        | RpcRequest::DiscoveryRemoveLocation(_)
        | RpcRequest::DiscoveryRescan(_) => DomainAction::DiscoveryWrite,
        RpcRequest::SettingsSnapshot(_) => DomainAction::SettingsRead,
        RpcRequest::SettingsUpdate(_) => DomainAction::SettingsWrite,
        RpcRequest::SteamGridDbCredentialSet(_) | RpcRequest::SteamGridDbCredentialClear(_) => {
            DomainAction::SecretWrite
        }
    }
}

pub fn principal_for(
    local: &IdentityState,
    sender_device_public_key: &str,
    sender_owner_event: Option<&str>,
    pass_event: Option<&str>,
    now: u64,
) -> Principal {
    let unknown = || Principal {
        device_public_key: sender_device_public_key.into(),
        tier: PrincipalTier::Unknown,
        scopes: Vec::new(),
    };
    let IdentityState::Owned {
        owner_public_key: local_owner,
        ..
    } = local
    else {
        return unknown();
    };
    if owner_of_device(sender_owner_event, sender_device_public_key, now)
        .is_some_and(|owner| owner == *local_owner)
    {
        return Principal {
            device_public_key: sender_device_public_key.into(),
            tier: PrincipalTier::Owner,
            scopes: vec!["*".into()],
        };
    }
    parse_pass(pass_event, local_owner, sender_device_public_key, now).unwrap_or_else(unknown)
}

pub fn authorize(principal: &Principal, action: DomainAction) -> bool {
    match principal.tier {
        PrincipalTier::Owner => true,
        PrincipalTier::Household => match action {
            DomainAction::CatalogRead
            | DomainAction::MoonlightResolve
            | DomainAction::StreamLaunch
            | DomainAction::StreamCancel
            | DomainAction::CertificateAttest
            | DomainAction::CertificateProvision
            | DomainAction::CertificateRevoke
            | DomainAction::SessionRead
            | DomainAction::SessionStop
            | DomainAction::SessionControl
            | DomainAction::HealthRead => has_scope(principal, action),
            DomainAction::LocalGamesRead
            | DomainAction::LocalGameLaunch
            | DomainAction::DiscoveryRead
            | DomainAction::DiscoveryWrite
            | DomainAction::SettingsRead
            | DomainAction::SettingsWrite
            | DomainAction::SecretWrite => false,
        },
        PrincipalTier::Guest => match action {
            // A host catalog contains only software that its owner installed.
            DomainAction::CatalogRead
            | DomainAction::MoonlightResolve
            | DomainAction::StreamLaunch
            | DomainAction::StreamCancel
            | DomainAction::CertificateAttest
            | DomainAction::CertificateProvision
            | DomainAction::CertificateRevoke
            | DomainAction::SessionRead
            | DomainAction::SessionStop
            | DomainAction::SessionControl
            | DomainAction::HealthRead => has_scope(principal, action),
            DomainAction::LocalGamesRead
            | DomainAction::LocalGameLaunch
            | DomainAction::DiscoveryRead
            | DomainAction::DiscoveryWrite
            | DomainAction::SettingsRead
            | DomainAction::SettingsWrite
            | DomainAction::SecretWrite => false,
        },
        PrincipalTier::Unknown => false,
    }
}

fn has_scope(principal: &Principal, action: DomainAction) -> bool {
    let required = match action {
        DomainAction::CatalogRead | DomainAction::MoonlightResolve | DomainAction::HealthRead => {
            "catalog.read"
        }
        DomainAction::StreamLaunch
        | DomainAction::StreamCancel
        | DomainAction::CertificateAttest
        | DomainAction::CertificateProvision
        | DomainAction::CertificateRevoke
        | DomainAction::SessionRead
        | DomainAction::SessionStop
        | DomainAction::SessionControl => STREAM_LAUNCH_SCOPE,
        DomainAction::LocalGamesRead
        | DomainAction::LocalGameLaunch
        | DomainAction::DiscoveryRead
        | DomainAction::DiscoveryWrite
        | DomainAction::SettingsRead
        | DomainAction::SettingsWrite
        | DomainAction::SecretWrite => return false,
    };
    principal.scopes.iter().any(|scope| scope == required)
}

fn owner_of_device(event_json: Option<&str>, device_public_key: &str, now: u64) -> Option<String> {
    let event_json = event_json?;
    if event_json.len() > 64 * 1024 {
        return None;
    }
    let event = Event::from_json(event_json).ok()?;
    event.verify().ok()?;
    if event.kind != Kind::Custom(30_078)
        || !event.content.is_empty()
        || event.created_at.as_secs() > now.saturating_add(120)
    {
        return None;
    }
    let tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice().to_vec())
        .collect();
    let expected_d = format!("org.korri.device-owner:{device_public_key}");
    if tags.len() != 3
        || tags[0].as_slice() != ["d", expected_d.as_str()]
        || tags[1].as_slice() != ["device", device_public_key]
        || tags[2].as_slice() != ["status", "owned"]
    {
        return None;
    }
    Some(event.pubkey.to_hex())
}

fn parse_pass(
    event_json: Option<&str>,
    expected_owner: &str,
    expected_device: &str,
    now: u64,
) -> Option<Principal> {
    let event_json = event_json?;
    if event_json.len() > 64 * 1024 {
        return None;
    }
    let event = Event::from_json(event_json).ok()?;
    event.verify().ok()?;
    if event.kind != Kind::Custom(PASS_EVENT_KIND)
        || event.pubkey.to_hex() != expected_owner
        || !event.content.is_empty()
        || event.created_at.as_secs() > now.saturating_add(120)
    {
        return None;
    }
    let tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice().to_vec())
        .collect();
    if tags.iter().any(|tag| {
        !matches!(
            tag.first().map(String::as_str),
            Some("device" | "tier" | "expires" | "scope")
        ) || tag.len() != 2
    }) || tags.iter().filter(|tag| tag[0] == "device").count() != 1
        || tags.iter().filter(|tag| tag[0] == "tier").count() != 1
        || tags.iter().filter(|tag| tag[0] == "expires").count() != 1
    {
        return None;
    }
    let value = |name: &str| {
        tags.iter()
            .find(|tag| tag.first().is_some_and(|value| value == name))
            .and_then(|tag| tag.get(1))
            .cloned()
    };
    if value("device").as_deref() != Some(expected_device) {
        return None;
    }
    let expires = value("expires")?.parse::<u64>().ok()?;
    if expires < now || expires <= event.created_at.as_secs() {
        return None;
    }
    let tier = match value("tier")?.as_str() {
        "household" => PrincipalTier::Household,
        "guest" => PrincipalTier::Guest,
        _ => return None,
    };
    let scopes = tags
        .iter()
        .filter(|tag| tag[0] == "scope")
        .map(|tag| tag[1].clone())
        .collect::<Vec<_>>();
    if scopes.is_empty() {
        return None;
    }
    Some(Principal {
        device_public_key: expected_device.into(),
        tier,
        scopes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CatalogSnapshotRequest, MoonlightCertificateProvisionRequest};
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        key::Keys,
        types::Timestamp,
    };

    const OWNER: &str = "0000000000000000000000000000000000000000000000000000000000000003";
    const DEVICE: &str = "0000000000000000000000000000000000000000000000000000000000000006";

    fn local(owner_public_key: String) -> IdentityState {
        IdentityState::Owned {
            device_public_key: "host".into(),
            owner_public_key,
            event_id: "event".into(),
            created_at: 1,
        }
    }

    fn pass(tier: &str, scopes: &[&str], expires: u64) -> String {
        let owner = Keys::parse(OWNER).unwrap();
        let mut tags = vec![
            Tag::parse(["device", DEVICE]).unwrap(),
            Tag::parse(["tier", tier]).unwrap(),
            Tag::parse(["expires", &expires.to_string()]).unwrap(),
        ];
        tags.extend(
            scopes
                .iter()
                .map(|scope| Tag::parse(["scope", *scope]).unwrap()),
        );
        EventBuilder::new(Kind::Custom(PASS_EVENT_KIND), "")
            .tags(tags)
            .custom_created_at(Timestamp::from(10))
            .finalize(&owner)
            .unwrap()
            .as_json()
    }

    #[test]
    fn exhaustive_request_mapping_marks_certificate_provision_as_security_mutation() {
        let request =
            RpcRequest::MoonlightCertificateProvision(MoonlightCertificateProvisionRequest {
                host_uuid: "host".into(),
                client_certificate: "certificate".into(),
            });
        assert_eq!(action_for(&request), DomainAction::CertificateProvision);
        assert!(action_for(&request).is_security_mutation());
        assert!(
            !action_for(&RpcRequest::CatalogSnapshot(CatalogSnapshotRequest {}))
                .is_security_mutation()
        );
    }

    #[test]
    fn unknown_principal_has_no_permissions() {
        let principal = Principal {
            device_public_key: "device".into(),
            tier: PrincipalTier::Unknown,
            scopes: vec!["*".into()],
        };
        assert!(!authorize(&principal, DomainAction::HealthRead));
        assert!(!authorize(&principal, DomainAction::CertificateProvision));
    }

    #[test]
    fn household_and_guest_passes_are_offline_scoped_and_expire() {
        let owner = Keys::parse(OWNER).unwrap().public_key().to_hex();
        for (tier, expected) in [
            ("household", PrincipalTier::Household),
            ("guest", PrincipalTier::Guest),
        ] {
            let event = pass(tier, &[STREAM_LAUNCH_SCOPE], 200);
            let principal = principal_for(&local(owner.clone()), DEVICE, None, Some(&event), 100);
            assert_eq!(principal.tier, expected);
            assert!(authorize(&principal, DomainAction::StreamLaunch));
            assert!(authorize(&principal, DomainAction::CertificateProvision));
            assert!(!authorize(&principal, DomainAction::SettingsWrite));
            assert_eq!(
                principal_for(&local(owner.clone()), DEVICE, None, Some(&event), 201).tier,
                PrincipalTier::Unknown
            );
        }
    }

    #[test]
    fn a_pass_from_another_owner_or_for_another_device_is_unknown() {
        let owner = Keys::parse(OWNER).unwrap().public_key().to_hex();
        let event = pass("guest", &[STREAM_LAUNCH_SCOPE], 200);
        assert_eq!(
            principal_for(&local("11".repeat(32)), DEVICE, None, Some(&event), 100).tier,
            PrincipalTier::Unknown
        );
        assert_eq!(
            principal_for(&local(owner), &"22".repeat(32), None, Some(&event), 100).tier,
            PrincipalTier::Unknown
        );
    }
}
