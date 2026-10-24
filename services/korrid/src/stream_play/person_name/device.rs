//! Device composition for signed person names and the existing settings RPC.
use super::*;
use crate::{
    identity::{DeviceIdentity, IdentityState},
    local_signer::{UnixLocalSignerAdmin, UnixPersonSigner},
    relay::{CoordinatedRelays, RelayList, WebSocketRelayTransport},
    remote_signer::{Nip46PersonSigner, PersonSigner, PersonSignerRequest, PersonSignerState},
    *,
};
use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Relays = CoordinatedRelays<WebSocketRelayTransport>;

struct CachedRecords {
    fetched: Instant,
    events: Vec<String>,
}

#[derive(Clone, Default)]
pub(crate) struct PersonNames {
    records: Arc<tokio::sync::Mutex<HashMap<String, CachedRecords>>>,
    writes: Arc<tokio::sync::Mutex<()>>,
}

fn identity(state: &AppState) -> Option<DeviceIdentity> {
    DeviceIdentity::load_or_create(state.private_state_root()).ok()
}

fn owner(identity: &DeviceIdentity) -> Option<&str> {
    match identity.state() {
        IdentityState::Owned {
            owner_public_key, ..
        } => Some(owner_public_key),
        _ => None,
    }
}

fn relays(identity: &DeviceIdentity) -> Option<Relays> {
    let relays = RelayList::from_linux_environment(std::env::var("KORRID_RELAYS").ok().as_deref())
        .ok()??;
    CoordinatedRelays::new(
        relays,
        identity.clone(),
        Arc::new(WebSocketRelayTransport::new()),
    )
    .ok()
}

async fn signer(state: &AppState, identity: &DeviceIdentity) -> Option<Arc<dyn PersonSigner>> {
    let owner = owner(identity)?;
    if let Some(socket) = std::env::var_os("KORRID_LOCAL_SIGNER_SOCKET").map(PathBuf::from) {
        if let Ok(Ok(status)) = tokio::time::timeout(
            Duration::from_secs(1),
            UnixLocalSignerAdmin::new(socket.clone()).status(),
        )
        .await
        {
            if status.0.as_deref() == Some(owner) {
                return Some(Arc::new(UnixPersonSigner::new(socket)));
            }
        }
    }
    let relay_list =
        crate::remote_signer::persisted_relay_list(state.private_state_root()).ok()??;
    let relays = Arc::new(
        CoordinatedRelays::new(
            relay_list,
            identity.clone(),
            Arc::new(WebSocketRelayTransport::new()),
        )
        .ok()?,
    );
    let signer = Nip46PersonSigner::load(state.private_state_root(), relays).ok()??;
    (signer.person_public_key().as_deref() == Some(owner))
        .then(|| Arc::new(signer) as Arc<dyn PersonSigner>)
}

impl PersonNames {
    pub(crate) async fn name(&self, state: &AppState, person: &str) -> Option<String> {
        let identity = identity(state)?;
        let local = if owner(&identity) == Some(person) {
            identity.person_name_event()
        } else {
            None
        };
        let mut cache = self.records.lock().await;
        let entry = cache.entry(person.into()).or_insert_with(|| CachedRecords {
            fetched: Instant::now() - Duration::from_secs(60),
            events: Vec::new(),
        });
        if entry.fetched.elapsed() >= Duration::from_secs(30) {
            if let Some(relays) = relays(&identity) {
                if let Ok(Ok(records)) = tokio::time::timeout(
                    Duration::from_millis(400),
                    relays.read_person_records(person),
                )
                .await
                {
                    entry.events = records;
                }
            }
            entry.fetched = Instant::now();
        }
        let mut records = entry.events.clone();
        records.extend(local);
        resolve(&records, person)
    }
}

fn can_write(state: &AppState, context: &authorization::AuthorizationContext) -> bool {
    matches!(context, authorization::AuthorizationContext::LocalCli)
        || (matches!(context, authorization::AuthorizationContext::LocalBrowser)
            && state.portal_access.as_ref().is_some_and(|access| {
                access.permits(&RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                    setting_id: SETTING_ID.into(),
                    expected_revision: String::new(),
                    value: String::new(),
                }))
            }))
}

pub(crate) async fn enrich(
    state: &AppState,
    context: &authorization::AuthorizationContext,
    snapshot: &mut SettingsSnapshot,
) {
    let Some(identity) = identity(state) else {
        return;
    };
    let Some(owner) = owner(&identity) else {
        return;
    };
    snapshot.person_name = state.person_names.name(state, owner).await;
    if can_write(state, context) && signer(state, &identity).await.is_some() {
        snapshot.editable_setting_ids.push(SETTING_ID.into());
    }
}

pub(crate) async fn update(
    state: &AppState,
    context: &authorization::AuthorizationContext,
    request: &SettingsUpdateRequest,
) -> SettingsUpdateOutcome {
    let result = async {
        if !can_write(state, context) {
            return Err(crate::local_settings_required());
        }
        let _write = state.person_names.writes.lock().await;
        let mut snapshot = match &state.mode {
            ServerMode::Host(host) => host.settings_snapshot().await?,
            ServerMode::Brain(brain) => {
                let readable = config::settings::read_with_registry_source(
                    &brain.local_storage_root,
                    &brain.registry_source,
                )
                .map_err(settings_failure)?;
                let sensitive = config::settings::read_sensitive(&brain.private_state_root)
                    .map_err(settings_failure)?;
                settings_snapshot(readable, sensitive)
            }
        };
        if snapshot.revision != request.expected_revision {
            return Err(settings_failure(config::settings::SettingsError::Conflict));
        }
        let identity = identity(state).ok_or_else(unavailable)?;
        let person = owner(&identity).ok_or_else(unavailable)?;
        let signer = signer(state, &identity).await.ok_or_else(unavailable)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unavailable())?
            .as_secs();
        // Make local sequential writes strictly newer even in one clock second.
        let now = identity
            .person_name_event()
            .and_then(|json| DeviceIdentity::verify_event(&json).ok())
            .map(|event| now.max(event.created_at.saturating_add(1)))
            .unwrap_or(now);
        let unsigned = template(&request.value, now).map_err(|message| RpcFailure {
            code: "SettingsInvalid".into(),
            message,
        })?;
        let approved = signer
            .request(PersonSignerRequest {
                unsigned_event_template: unsigned.clone(),
            })
            .await;
        let PersonSignerState::Approved {
            owner_public_key,
            unsigned_event_template,
            signed_event_json,
        } = approved
        else {
            return Err(unavailable());
        };
        if owner_public_key != person
            || unsigned_event_template != unsigned
            || verified_name(&signed_event_json, person).as_deref() != Some(request.value.as_str())
        {
            return Err(unavailable());
        }
        let event = DeviceIdentity::verify_event(&signed_event_json).map_err(|_| unavailable())?;
        if event.created_at != now {
            return Err(unavailable());
        }
        identity
            .store_person_name(&signed_event_json)
            .map_err(|_| RpcFailure {
                code: "SettingsStorage".into(),
                message: "Could not store the person name.".into(),
            })?;
        state.person_names.records.lock().await.remove(person);
        if let Some(relays) = relays(&identity) {
            // Like owner statements, the local signed record remains authoritative
            // offline. Retry publication from the idle watcher on the next pass.
            let _ = tokio::time::timeout(
                Duration::from_millis(400),
                relays.publish_person_name(&signed_event_json, person),
            )
            .await;
        }
        snapshot.editable_setting_ids = editable_setting_ids(state, context);
        enrich(state, context, &mut snapshot).await;
        Ok(snapshot)
    }
    .await;
    result
        .map(SettingsUpdateOutcome::Ok)
        .unwrap_or_else(SettingsUpdateOutcome::Err)
}

pub(crate) async fn publish_local(state: &AppState) {
    let Some(identity) = identity(state) else {
        return;
    };
    let (Some(person), Some(event), Some(relays)) = (
        owner(&identity),
        identity.person_name_event(),
        relays(&identity),
    ) else {
        return;
    };
    let _ = tokio::time::timeout(
        Duration::from_millis(400),
        relays.publish_person_name(&event, person),
    )
    .await;
}

fn unavailable() -> RpcFailure {
    RpcFailure {
        code: "PersonSignerUnavailable".into(),
        message: "This device cannot sign for the owner.".into(),
    }
}

pub(crate) fn spawn_publish(state: AppState) {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(Duration::from_secs(30));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticks.tick().await;
            publish_local(&state).await;
        }
    });
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
