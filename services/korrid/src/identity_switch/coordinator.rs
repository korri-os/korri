use super::{DataDisposition, IdentitySwitchJournal, IdentitySwitchStorage, JournalPhase};
use crate::{
    authorization::Authorization,
    federation::FederationDirectory,
    host::play_log::PlayLogStore,
    identity::{DeviceIdentity, IdentityState, OwnerStatementStatus, VerifiedOwnerStatement},
    local_signer::UnixLocalSigner,
    peer_rpc::unix_time,
    relay::{CoordinatedRelays, RelayCoordinator, RelayList, WebSocketRelayTransport},
    remote_signer::{
        bunker_relay_list, persisted_relay_list, Nip46PersonSigner, PersonSigner,
        PersonSignerRequest, PersonSignerState,
    },
    FederationResources,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use zeroize::Zeroizing;

/// The person key a switch moves the device to.
#[derive(Clone)]
pub enum ReplacementIdentity {
    /// The person key that these recovery words make (ADR 0073). The local
    /// signer signs the new owner statement with it, then erases it.
    Words {
        words: Zeroizing<String>,
    },
    Nip46 {
        bunker_uri: String,
    },
}

impl std::fmt::Debug for ReplacementIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Words { .. } => formatter.write_str("Words"),
            Self::Nip46 { .. } => formatter.write_str("Nip46"),
        }
    }
}

#[derive(Clone)]
pub struct IdentitySwitchCoordinator {
    private_state_root: PathBuf,
    signer: UnixLocalSigner,
    federation: FederationResources,
    /// One identity change at a time: a switch, or a join that moves an owned
    /// device to another person key. Both use the one staging root.
    busy: Arc<tokio::sync::Mutex<()>>,
}

/// A switch that is ready for its new owner statement: the old identity,
/// its signed revocation, and the staged identity with a new device key.
/// Nothing in it is durable until `commit`. Dropping it leaves a staging
/// root without a journal, which the next switch or start removes.
pub struct PreparedSwitch {
    storage: IdentitySwitchStorage,
    old_statement: VerifiedOwnerStatement,
    old_owner_event: String,
    old_owner_was_published: bool,
    new_identity: DeviceIdentity,
    revocation: String,
    _busy: tokio::sync::OwnedMutexGuard<()>,
}

impl PreparedSwitch {
    /// The staged identity. Its new device key joins in place of the old.
    pub fn staged_identity(&self) -> &DeviceIdentity {
        &self.new_identity
    }

    pub fn old_owner_public_key(&self) -> &str {
        &self.old_statement.owner_public_key
    }
}

impl IdentitySwitchCoordinator {
    pub fn new(
        private_state_root: PathBuf,
        signer_socket: PathBuf,
        federation: FederationResources,
    ) -> Self {
        Self {
            private_state_root,
            signer: UnixLocalSigner::new(signer_socket),
            federation,
            busy: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// The local signer: it makes recovery words and signs with them.
    pub fn local_signer(&self) -> UnixLocalSigner {
        self.signer.clone()
    }

    pub fn status_snapshot(&self) -> Result<IdentityState, String> {
        let identity = self
            .federation
            .directory
            .identity_state()
            .map_err(|_| "Device identity is unavailable. Try again.".to_owned())?;
        status_identity(identity)
    }

    /// Switch to `replacement`. The current owner signs the revocation of
    /// the old statement through NIP-46: on `current_owner_bunker`, or on
    /// the connection this device keeps.
    pub async fn switch(
        &self,
        replacement: ReplacementIdentity,
        current_owner_bunker: Option<&str>,
        disposition: DataDisposition,
        trust_loss_confirmed: bool,
    ) -> Result<String, String> {
        if !trust_loss_confirmed {
            return Err("Identity switch requires explicit confirmation that pairings and stream trust will be lost".into());
        }
        let old_identity = DeviceIdentity::load_or_create(&self.private_state_root)
            .map_err(|error| error.to_string())?;
        let old_statement = owned_statement(&old_identity)?;
        let current = match current_owner_bunker {
            Some(bunker_uri) => CurrentOwner::Bunker(bunker_uri),
            None => CurrentOwner::Nip46,
        };
        let revocation = self
            .sign_revocation_with(&old_identity, &old_statement, current)
            .await?;
        let mut prepared = self.prepare(revocation).await?;
        let new_device = prepared
            .new_identity
            .device_public_key()
            .ok_or_else(|| "The staged device identity is unavailable".to_owned())?
            .to_owned();
        let created_at = unix_time().max(prepared.old_statement.created_at.saturating_add(1));
        let new_template = prepared
            .new_identity
            .owner_statement_template(OwnerStatementStatus::Owned, created_at)
            .map_err(|error| error.to_string())?;
        let request = PersonSignerRequest {
            unsigned_event_template: new_template.clone(),
        };
        let (owner_public_key, owner_event_json) = match replacement {
            ReplacementIdentity::Words { words } => approved_owner_event(
                self.signer
                    .sign_with_words(&words, &new_device, request)
                    .await?,
                &new_template,
            )?,
            ReplacementIdentity::Nip46 { bunker_uri } => {
                // NIP-46 talks to the signer on the relays its URI names.
                let relays = coordinated(
                    &old_identity,
                    bunker_relay_list(&bunker_uri).map_err(|error| error.to_string())?,
                )?;
                let staged_root = prepared.storage.staged_private_root();
                let signer =
                    Nip46PersonSigner::connect_from_bunker(&staged_root, relays, &bunker_uri)
                        .map_err(|error| error.to_string())?;
                signer
                    .establish(unix_time(), &new_device)
                    .await
                    .map_err(|error| error.to_string())?;
                // The signed owner statement names the person key.
                approved_owner_event(signer.request(request).await, &new_template)?
            }
        };
        if owner_public_key == prepared.old_statement.owner_public_key {
            return Err("The replacement identity must use a different person key".into());
        }
        prepared
            .new_identity
            .apply_signed_owner_binding(&new_template, &owner_public_key, &owner_event_json)
            .map_err(|error| error.to_string())?;
        self.commit(prepared, disposition).await
    }

    /// Begin moving this owned device to another person key by a join (ADRs
    /// 0023 and 0082). The words must be the current person key's words: it
    /// signs the old statement's revocation now, and the signer erases it.
    pub async fn begin_join(&self, current_words: &str) -> Result<PreparedSwitch, String> {
        let old_identity = DeviceIdentity::load_or_create(&self.private_state_root)
            .map_err(|error| error.to_string())?;
        let old_statement = owned_statement(&old_identity)?;
        let revocation = self
            .sign_revocation_with(
                &old_identity,
                &old_statement,
                CurrentOwner::Words(current_words),
            )
            .await?;
        self.prepare(revocation).await
    }

    /// Finish a join that moves this device: store the vouch chain for the
    /// staged device key, then switch to it, keeping this device's data.
    pub async fn finish_join(
        &self,
        mut prepared: PreparedSwitch,
        owner_statement: &str,
    ) -> Result<String, String> {
        prepared
            .new_identity
            .apply_owner_statement(owner_statement)
            .map_err(|error| error.to_string())?;
        if owned_statement(&prepared.new_identity)?.owner_public_key
            == prepared.old_statement.owner_public_key
        {
            return Err("This device already belongs to that person key".into());
        }
        self.commit(prepared, DataDisposition::Transfer).await
    }

    /// The staged identity with a new device key, ready for a new owner.
    /// The old owner's revocation is signed before anything is staged.
    async fn prepare(&self, revocation: String) -> Result<PreparedSwitch, String> {
        let busy = Arc::clone(&self.busy)
            .try_lock_owned()
            .map_err(|_| "Another identity change is in progress on this device".to_owned())?;
        let storage = IdentitySwitchStorage::open(&self.private_state_root)
            .map_err(|error| error.to_string())?;
        storage
            .cleanup_abandoned_without_journal()
            .map_err(|error| error.to_string())?;
        if storage
            .load_journal()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("An identity switch is already pending recovery".into());
        }
        let old_identity = DeviceIdentity::load_or_create(&self.private_state_root)
            .map_err(|error| error.to_string())?;
        let old_statement = owned_statement(&old_identity)?;
        let old_owner_event = old_identity
            .owner_statement_json()
            .ok_or_else(|| "The current device has no owner statement".to_owned())?;
        revokes(&revocation, &old_statement)?;
        let old_owner_was_published = owner_was_published(&old_identity, &old_owner_event).await?;
        let staged_root = storage
            .prepare_staging_root()
            .map_err(|error| error.to_string())?;
        let new_identity =
            DeviceIdentity::load_or_create(&staged_root).map_err(|error| error.to_string())?;
        Ok(PreparedSwitch {
            storage,
            old_statement,
            old_owner_event,
            old_owner_was_published,
            new_identity,
            revocation,
            _busy: busy,
        })
    }

    /// Make the switch durable and complete it (`journal.rs`).
    async fn commit(
        &self,
        prepared: PreparedSwitch,
        disposition: DataDisposition,
    ) -> Result<String, String> {
        let new_owner_event = prepared
            .new_identity
            .owner_statement_json()
            .ok_or_else(|| "The staged identity has no owner".to_owned())?;
        let new_owner = owned_statement(&prepared.new_identity)?.owner_public_key;
        let preparing = IdentitySwitchJournal::new(
            JournalPhase::Preparing,
            disposition,
            prepared.revocation.clone(),
            prepared.old_owner_was_published,
        )
        .map_err(|error| error.to_string())?;
        let storage = &prepared.storage;
        storage
            .write_preparing(&preparing)
            .map_err(|error| error.to_string())?;
        let commit = preparing.committing();
        storage
            .write_commit(&commit)
            .map_err(|error| error.to_string())?;
        storage
            .exchange_identity_directories(&prepared.old_owner_event, &new_owner_event)
            .map_err(|error| error.to_string())?;
        complete_commit(storage, &commit, Some(&self.federation)).await?;
        Ok(new_owner)
    }

    /// Sign the revocation of this device's current owner statement with
    /// the current person key.
    async fn sign_revocation_with(
        &self,
        identity: &DeviceIdentity,
        statement: &VerifiedOwnerStatement,
        current: CurrentOwner<'_>,
    ) -> Result<String, String> {
        let created_at = unix_time().max(statement.created_at.saturating_add(1));
        let template = identity
            .owner_statement_template(OwnerStatementStatus::Revoked, created_at)
            .map_err(|error| error.to_string())?;
        let request = PersonSignerRequest {
            unsigned_event_template: template.clone(),
        };
        let state = match current {
            CurrentOwner::Words(words) => {
                let state = self
                    .signer
                    .sign_with_words(words, &statement.device_public_key, request)
                    .await?;
                if matches!(&state, PersonSignerState::Approved { owner_public_key, .. }
                    if *owner_public_key != statement.owner_public_key)
                {
                    return Err(
                        "These are not the recovery words of this device's person key".into(),
                    );
                }
                state
            }
            CurrentOwner::Bunker(bunker_uri) => {
                // A connection for this one signature. Its client key and the
                // URI live in a temporary directory that is removed after it.
                let relays = coordinated(
                    identity,
                    bunker_relay_list(bunker_uri).map_err(|error| error.to_string())?,
                )?;
                let scratch = tempfile::Builder::new()
                    .prefix("korri-current-owner-")
                    .tempdir()
                    .map_err(|_| "Could not prepare the current owner's signer".to_owned())?;
                let signer =
                    Nip46PersonSigner::connect_from_bunker(scratch.path(), relays, bunker_uri)
                        .map_err(|error| error.to_string())?;
                signer
                    .establish(unix_time(), &statement.device_public_key)
                    .await
                    .map_err(|error| error.to_string())?;
                signer.request(request).await
            }
            CurrentOwner::Nip46 => {
                let relays = coordinated(
                    identity,
                    persisted_relay_list(&self.private_state_root)
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "The current owner signer is unavailable".to_owned())?,
                )?;
                let signer = Nip46PersonSigner::load(&self.private_state_root, relays)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "The current owner signer is unavailable".to_owned())?;
                signer.request(request).await
            }
        };
        let revocation = approved_event(state, &template, &statement.owner_public_key)?;
        verify_revocation(
            &revocation,
            &statement.device_public_key,
            &statement.owner_public_key,
            created_at,
        )?;
        Ok(revocation)
    }
}

/// Who signs for the current person key.
enum CurrentOwner<'a> {
    /// The owner's NIP-46 signer that this device is connected to.
    Nip46,
    /// The owner's NIP-46 signer at this bunker URI, for one signature.
    Bunker(&'a str),
    /// The current person key's recovery words, in the local signer.
    Words(&'a str),
}

pub async fn recover_pending_identity_switch(private_state_root: &Path) -> Result<(), String> {
    let storage =
        IdentitySwitchStorage::open(private_state_root).map_err(|error| error.to_string())?;
    storage
        .cleanup_abandoned_without_journal()
        .map_err(|error| error.to_string())?;
    let Some(journal) = storage.load_journal().map_err(|error| error.to_string())? else {
        return Ok(());
    };
    if journal.phase() == JournalPhase::Preparing {
        storage
            .abort_preparing()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    complete_commit(&storage, &journal, None).await
}

/// What the person chose for their data on this device: the play log, and
/// every plugin secret (ADR 0034, "Owner and devices"). `Transfer` keeps each
/// secret for the new owner; `Delete` removes each one. Every step can run
/// again, so a switch that resumes from its journal repeats it safely.
pub(super) fn apply_data_disposition(
    private_root: &Path,
    disposition: DataDisposition,
    old_owner_public_key: &str,
    new_owner_public_key: &str,
) -> Result<(), String> {
    let play_log = PlayLogStore::new(private_root);
    match disposition {
        DataDisposition::Transfer => play_log
            .transfer_person_records(old_owner_public_key, new_owner_public_key)
            .map_err(|error| error.to_string()),
        DataDisposition::Delete => {
            play_log
                .delete_person_records(old_owner_public_key)
                .map_err(|error| error.to_string())?;
            let secrets = crate::plugin_secrets::PluginSecrets::new(private_root);
            for plugin_id in secrets
                .plugins_with_a_secret()
                .map_err(|error| error.to_string())?
            {
                secrets
                    .clear(&plugin_id)
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        }
    }
}

async fn complete_commit(
    storage: &IdentitySwitchStorage,
    journal: &IdentitySwitchJournal,
    live_resources: Option<&FederationResources>,
) -> Result<(), String> {
    let old_revocation = DeviceIdentity::derive_owner_statement(journal.old_owner_revocation())
        .map_err(|error| error.to_string())?;
    let live_identity = DeviceIdentity::load_or_create(&storage.live_private_root())
        .map_err(|error| error.to_string())?;
    let staged_identity = DeviceIdentity::load_or_create(&storage.staged_private_root())
        .map_err(|error| error.to_string())?;
    let live_device = live_identity.device_public_key().unwrap_or_default();
    let (old_identity, new_identity) = if live_device == old_revocation.device_public_key {
        let old = DeviceIdentity::load_or_create(&storage.live_private_root())
            .map_err(|error| error.to_string())?;
        let new = DeviceIdentity::load_or_create(&storage.staged_private_root())
            .map_err(|error| error.to_string())?;
        let old_event = old
            .owner_statement_json()
            .ok_or_else(|| "Old owner evidence is unavailable".to_owned())?;
        let new_event = new
            .owner_statement_json()
            .ok_or_else(|| "New owner evidence is unavailable".to_owned())?;
        storage
            .exchange_identity_directories(&old_event, &new_event)
            .map_err(|error| error.to_string())?;
        (old, new)
    } else if staged_identity.device_public_key() == Some(old_revocation.device_public_key.as_str())
    {
        (staged_identity, live_identity)
    } else {
        return Err("Identity-switch storage does not match the committed journal".into());
    };
    let old_statement = owned_statement(&old_identity)?;
    let new_statement = owned_statement(&new_identity)?;
    apply_data_disposition(
        &storage.live_private_root(),
        journal.data_disposition(),
        &old_statement.owner_public_key,
        &new_statement.owner_public_key,
    )?;
    // The old identity's stream grants live in its identity directory, so
    // they moved with it to the staging root. Sunshine stops trusting each
    // of them there, before `finish_commit` removes the staging root.
    let old_authorization =
        Authorization::load(&storage.staged_private_root()).map_err(|error| error.to_string())?;
    revoke_stream_client_trust(
        old_authorization,
        crate::host::moonlight_certificate::production_adapter(),
    )
    .await?;
    // The new identity starts with empty authorization directories.
    let new_authorization =
        Authorization::load(&storage.live_private_root()).map_err(|error| error.to_string())?;
    if let Some(resources) = live_resources {
        resources
            .authorization
            .reset_after_identity_switch()
            .map_err(|error| error.to_string())?;
        resources
            .directory
            .reset_after_identity_switch(
                &new_statement.device_public_key,
                &new_statement.owner_public_key,
            )
            .map_err(|error| error.to_string())?;
    } else {
        new_authorization
            .reset_after_identity_switch()
            .map_err(|error| error.to_string())?;
        FederationDirectory::reset_persisted_after_identity_switch(&storage.live_private_root())
            .map_err(|error| error.to_string())?;
    }

    if journal.old_owner_was_published() {
        let relays = relays_for(&new_identity)?
            .ok_or_else(|| "Published owner revocation requires configured relays".to_owned())?;
        let state = relays
            .publish_signer_packet(journal.old_owner_revocation())
            .await;
        if !state.accepted() {
            return Err("The old owner revocation was not accepted by any relay".into());
        }
    }
    storage.finish_commit().map_err(|error| error.to_string())
}

/// Sunshine stops trusting each device's granted certificate, and then the
/// grant goes. Each device's revoke holds the device's certificate lock
/// (`authorization::DeviceCertificateLocks`), as its provisions and their
/// reconciliation do, so none of them can come between the revoke and the
/// grant's removal (#503). Like reconciliation, it waits for one device at a
/// time, in the order of their keys, and takes the lock before the socket
/// turn. A revoke that Sunshine does not confirm keeps its grant and stops
/// the switch.
pub(crate) async fn revoke_stream_client_trust(
    authorization: Authorization,
    adapter: Arc<dyn crate::host::moonlight_certificate::MoonlightCertificateAdapter>,
) -> Result<(), String> {
    for device in authorization
        .certificate_grant_devices()
        .map_err(|error| error.to_string())?
    {
        let held = authorization.certificate_locks().lock(&device).await;
        let authorization = authorization.clone();
        let adapter = adapter.clone();
        // One blocking step from the grant's read to its removal. It keeps
        // the device until it ends, even when this task is dropped first.
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let _device = held;
            // The device may have provisioned again while this waited for it.
            let Some((host_uuid, certificate)) = authorization
                .device_certificate_grant(&device)
                .map_err(|error| error.to_string())?
            else {
                return Ok(());
            };
            adapter
                .revoke(&host_uuid, &certificate)
                .map_err(|error| error.message)?;
            // Sunshine confirmed. The grant goes only when it names the
            // certificate that Sunshine stopped trusting.
            if authorization
                .device_certificate_grant(&device)
                .map_err(|error| error.to_string())?
                .is_some_and(|(_, granted)| granted == certificate)
            {
                authorization
                    .remove_certificate_grant(&device)
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        })
        .await
        .map_err(|_| "Stream-client trust cleanup worker failed".to_owned())??;
    }
    Ok(())
}

fn owned_statement(identity: &DeviceIdentity) -> Result<VerifiedOwnerStatement, String> {
    match identity.state() {
        IdentityState::Owned { .. } => identity
            .owner_statement_json()
            .ok_or_else(|| "Owner statement is unavailable".to_owned())
            .and_then(|event| {
                DeviceIdentity::derive_owner_statement(&event).map_err(|error| error.to_string())
            }),
        _ => Err("The device is not owned".into()),
    }
}

fn approved_owner_event(
    state: PersonSignerState,
    expected_template: &str,
) -> Result<(String, String), String> {
    let owner = match &state {
        PersonSignerState::Approved {
            owner_public_key, ..
        } => owner_public_key.clone(),
        _ => String::new(),
    };
    approved_event(state, expected_template, &owner).map(|event| (owner, event))
}

fn approved_event(
    state: PersonSignerState,
    expected_template: &str,
    expected_owner: &str,
) -> Result<String, String> {
    match state {
        PersonSignerState::Approved {
            owner_public_key,
            unsigned_event_template,
            signed_event_json,
        } if owner_public_key == expected_owner && unsigned_event_template == expected_template => {
            Ok(signed_event_json)
        }
        PersonSignerState::Denied { message }
        | PersonSignerState::Defect { message }
        | PersonSignerState::InvalidResponse { message }
        | PersonSignerState::Unavailable { message }
        | PersonSignerState::Pending { message } => Err(message),
        PersonSignerState::Approved { .. } => {
            Err("Signer approval did not match the requested owner statement".into())
        }
    }
}

fn verify_revocation(
    event_json: &str,
    device_public_key: &str,
    owner_public_key: &str,
    created_at: u64,
) -> Result<(), String> {
    let statement = DeviceIdentity::verify_owner_statement(event_json, device_public_key)
        .map_err(|error| error.to_string())?;
    if statement.status != OwnerStatementStatus::Revoked
        || statement.owner_public_key != owner_public_key
        || statement.created_at != created_at
    {
        return Err("Old owner revocation does not match the requested statement".into());
    }
    Ok(())
}

/// Whether a signed revocation revokes exactly this current statement's
/// device and person key: the identity must not change between the
/// signature and the switch.
fn revokes(revocation: &str, current: &VerifiedOwnerStatement) -> Result<(), String> {
    let statement = DeviceIdentity::verify_owner_statement(revocation, &current.device_public_key)
        .map_err(|error| error.to_string())?;
    if statement.status != OwnerStatementStatus::Revoked
        || statement.owner_public_key != current.owner_public_key
        || statement.created_at <= current.created_at
    {
        return Err("The device's owner changed during the switch. Try again.".into());
    }
    Ok(())
}

async fn owner_was_published(identity: &DeviceIdentity, event_json: &str) -> Result<bool, String> {
    let Some(relays) = relays_for(identity)? else {
        return Ok(false);
    };
    relays
        .owner_statement_was_published(event_json)
        .await
        .map_err(|error| error.to_string())
}

fn relays_for(
    identity: &DeviceIdentity,
) -> Result<Option<Arc<CoordinatedRelays<WebSocketRelayTransport>>>, String> {
    let relays = RelayList::from_linux_environment(std::env::var("KORRID_RELAYS").ok().as_deref())
        .map_err(|error| error.to_string())?;
    relays
        .map(|relays| coordinated(identity, relays))
        .transpose()
}

fn coordinated(
    identity: &DeviceIdentity,
    relays: RelayList,
) -> Result<Arc<CoordinatedRelays<WebSocketRelayTransport>>, String> {
    CoordinatedRelays::new(
        relays,
        identity.clone(),
        Arc::new(WebSocketRelayTransport::new()),
    )
    .map(Arc::new)
    .map_err(|error| error.to_string())
}

fn status_identity(identity: IdentityState) -> Result<IdentityState, String> {
    match identity {
        IdentityState::Invalid { .. } => Err("Device identity is unavailable. Try again.".into()),
        identity => Ok(identity),
    }
}

#[cfg(test)]
mod status_identity_tests {
    use super::*;
    #[test]
    fn owned_and_unowned_status_reuse_the_existing_identity_and_invalid_reason_is_private() {
        let unowned = IdentityState::Unowned {
            device_public_key: "device".into(),
        };
        let owned = IdentityState::Owned {
            device_public_key: "device".into(),
            owner_public_key: "owner".into(),
            event_id: "event".into(),
            created_at: 1,
        };
        for identity in [unowned, owned] {
            assert_eq!(status_identity(identity.clone()).unwrap(), identity);
        }
        let error = status_identity(IdentityState::Invalid {
            reason: "private storage path or sensitive diagnostic".into(),
        })
        .unwrap_err();
        assert_eq!(error, "Device identity is unavailable. Try again.");
    }
}
