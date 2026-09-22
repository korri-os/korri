use super::{
    DataDisposition, IdentitySwitchJournal, IdentitySwitchStorage, JournalPhase, ReplacementSigner,
};
use crate::{
    authorization::Authorization,
    federation::FederationDirectory,
    host::play_log::PlayLogStore,
    identity::{DeviceIdentity, IdentityState, OwnerStatementStatus, VerifiedOwnerStatement},
    local_signer::{UnixLocalSignerAdmin, UnixPersonSigner},
    peer_rpc::unix_time,
    relay::{CoordinatedRelays, RelayCoordinator, RelayList, WebSocketRelayTransport},
    remote_signer::{Nip46PersonSigner, PersonSigner, PersonSignerRequest, PersonSignerState},
    FederationResources,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub enum ReplacementIdentity {
    LocalBackup {
        encrypted_secret: String,
        password: String,
    },
    Nip46 {
        bunker_uri: String,
    },
}

#[derive(Clone)]
pub struct IdentitySwitchCoordinator {
    private_state_root: PathBuf,
    signer: UnixLocalSignerAdmin,
    federation: FederationResources,
}

impl IdentitySwitchCoordinator {
    pub fn new(
        private_state_root: PathBuf,
        signer_socket: PathBuf,
        federation: FederationResources,
    ) -> Self {
        Self {
            private_state_root,
            signer: UnixLocalSignerAdmin::new(signer_socket),
            federation,
        }
    }

    pub async fn export_active_backup(&self, password: String) -> Result<String, String> {
        let (available, _) = self.signer_status().await?;
        if !available {
            return Err("The current identity is not held by the local signer".into());
        }
        self.signer.export_active(password).await
    }

    pub async fn export_retired_backup(
        &self,
        public_key: String,
        password: String,
    ) -> Result<String, String> {
        self.signer.export_retired(public_key, password).await
    }

    pub async fn delete_retired_key(&self, public_key: String) -> Result<(), String> {
        self.signer.delete_retired(public_key).await
    }

    pub async fn signer_status(&self) -> Result<(bool, Vec<String>), String> {
        let (active, retired) = self.signer.status().await?;
        let identity = DeviceIdentity::load_or_create(&self.private_state_root)
            .map_err(|error| error.to_string())?;
        let owner = owned_statement(&identity)?.owner_public_key;
        Ok((active.as_deref() == Some(owner.as_str()), retired))
    }

    pub async fn switch(
        &self,
        replacement: ReplacementIdentity,
        disposition: DataDisposition,
        trust_loss_confirmed: bool,
    ) -> Result<String, String> {
        if !trust_loss_confirmed {
            return Err("Identity switch requires explicit confirmation that pairings and stream trust will be lost".into());
        }
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
        let old_owner_was_published = owner_was_published(&old_identity, &old_owner_event).await?;
        let staged_root = storage
            .prepare_staging_root()
            .map_err(|error| error.to_string())?;
        let mut new_identity =
            DeviceIdentity::load_or_create(&staged_root).map_err(|error| error.to_string())?;
        let new_device = new_identity
            .device_public_key()
            .ok_or_else(|| "The staged device identity is unavailable".to_owned())?
            .to_owned();
        let created_at = unix_time().max(old_statement.created_at.saturating_add(1));
        let new_template = new_identity
            .owner_statement_template(OwnerStatementStatus::Owned, created_at)
            .map_err(|error| error.to_string())?;

        let prepared = match replacement {
            ReplacementIdentity::LocalBackup {
                encrypted_secret,
                password,
            } => {
                let public_key = self
                    .signer
                    .stage_import(encrypted_secret, password, new_device.clone())
                    .await?;
                let state = self
                    .signer
                    .sign_inactive(
                        public_key.clone(),
                        new_device.clone(),
                        PersonSignerRequest {
                            unsigned_event_template: new_template.clone(),
                        },
                    )
                    .await?;
                let event = approved_event(state, &new_template, &public_key)?;
                PreparedReplacement {
                    kind: ReplacementSigner::Local,
                    owner_public_key: public_key,
                    owner_event_json: event,
                }
            }
            ReplacementIdentity::Nip46 { bunker_uri } => {
                let relays = relays_for(&old_identity)?
                    .ok_or_else(|| "NIP-46 requires at least one configured relay".to_owned())?;
                let signer =
                    Nip46PersonSigner::connect_from_bunker(&staged_root, relays, &bunker_uri)
                        .map_err(|error| error.to_string())?;
                let owner_public_key = signer
                    .establish(unix_time())
                    .await
                    .map_err(|error| error.to_string())?;
                let event = approved_event(
                    signer
                        .request(PersonSignerRequest {
                            unsigned_event_template: new_template.clone(),
                        })
                        .await,
                    &new_template,
                    &owner_public_key,
                )?;
                PreparedReplacement {
                    kind: ReplacementSigner::Nip46,
                    owner_public_key,
                    owner_event_json: event,
                }
            }
        };
        if prepared.owner_public_key == old_statement.owner_public_key {
            if prepared.kind == ReplacementSigner::Local {
                let _ = self
                    .signer
                    .rollback_inactive(prepared.owner_public_key.clone())
                    .await;
            }
            let _ = storage.cleanup_abandoned_without_journal();
            return Err("The replacement identity must use a different person key".into());
        }
        new_identity
            .apply_signed_owner_binding(
                &new_template,
                &prepared.owner_public_key,
                &prepared.owner_event_json,
            )
            .map_err(|error| error.to_string())?;

        let revocation_created_at = unix_time().max(old_statement.created_at.saturating_add(1));
        let revocation_template = old_identity
            .owner_statement_template(OwnerStatementStatus::Revoked, revocation_created_at)
            .map_err(|error| error.to_string())?;
        let revocation = self
            .sign_current_owner(&old_identity, &old_statement, revocation_template.clone())
            .await?;
        verify_revocation(
            &revocation,
            &old_statement.device_public_key,
            &old_statement.owner_public_key,
            revocation_created_at,
        )?;

        let preparing = IdentitySwitchJournal::new(
            JournalPhase::Preparing,
            disposition,
            prepared.kind,
            revocation,
            old_owner_was_published,
        )
        .map_err(|error| error.to_string())?;
        storage
            .write_preparing(&preparing)
            .map_err(|error| error.to_string())?;
        let commit = preparing.committing();
        storage
            .write_commit(&commit)
            .map_err(|error| error.to_string())?;
        storage
            .exchange_identity_directories(&old_owner_event, &prepared.owner_event_json)
            .map_err(|error| error.to_string())?;

        complete_commit(&storage, &commit, &self.signer, Some(&self.federation)).await?;
        Ok(prepared.owner_public_key)
    }

    async fn sign_current_owner(
        &self,
        identity: &DeviceIdentity,
        statement: &VerifiedOwnerStatement,
        template: String,
    ) -> Result<String, String> {
        let (active, _) = self.signer.status().await?;
        if active.as_deref() == Some(statement.owner_public_key.as_str()) {
            return approved_event(
                UnixPersonSigner::new(signer_socket_from_environment()?)
                    .request(PersonSignerRequest {
                        unsigned_event_template: template.clone(),
                    })
                    .await,
                &template,
                &statement.owner_public_key,
            );
        }
        let relays = relays_for(identity)?
            .ok_or_else(|| "The current NIP-46 owner requires configured relays".to_owned())?;
        let signer = Nip46PersonSigner::load(&self.private_state_root, relays)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "The current owner signer is unavailable".to_owned())?;
        approved_event(
            signer
                .request(PersonSignerRequest {
                    unsigned_event_template: template.clone(),
                })
                .await,
            &template,
            &statement.owner_public_key,
        )
    }
}

struct PreparedReplacement {
    kind: ReplacementSigner,
    owner_public_key: String,
    owner_event_json: String,
}

pub async fn recover_pending_identity_switch(
    private_state_root: &Path,
    signer_socket: PathBuf,
) -> Result<(), String> {
    let storage =
        IdentitySwitchStorage::open(private_state_root).map_err(|error| error.to_string())?;
    storage
        .cleanup_abandoned_without_journal()
        .map_err(|error| error.to_string())?;
    let Some(journal) = storage.load_journal().map_err(|error| error.to_string())? else {
        return Ok(());
    };
    let signer = UnixLocalSignerAdmin::new(signer_socket);
    if journal.phase() == JournalPhase::Preparing {
        if journal.replacement_signer() == ReplacementSigner::Local {
            if let Ok(identity) = DeviceIdentity::load_or_create(&storage.staged_private_root()) {
                if let Ok(statement) = owned_statement(&identity) {
                    signer.rollback_inactive(statement.owner_public_key).await?;
                }
            }
        }
        storage
            .abort_preparing()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    complete_commit(&storage, &journal, &signer, None).await
}

async fn complete_commit(
    storage: &IdentitySwitchStorage,
    journal: &IdentitySwitchJournal,
    signer: &UnixLocalSignerAdmin,
    live_resources: Option<&FederationResources>,
) -> Result<(), String> {
    let old_revocation = DeviceIdentity::derive_owner_statement(journal.old_owner_revocation())
        .map_err(|error| error.to_string())?;
    let live_identity = DeviceIdentity::load_or_create(&storage.live_private_root())
        .map_err(|error| error.to_string())?;
    let staged_identity = DeviceIdentity::load_or_create(&storage.staged_private_root())
        .map_err(|error| error.to_string())?;
    let live_device = live_identity.device_public_key().unwrap_or_default();
    let (old_identity, new_identity, new_event) = if live_device == old_revocation.device_public_key
    {
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
        (old, new, new_event)
    } else if staged_identity.device_public_key() == Some(old_revocation.device_public_key.as_str())
    {
        let new_event = live_identity
            .owner_statement_json()
            .ok_or_else(|| "New owner evidence is unavailable".to_owned())?;
        (staged_identity, live_identity, new_event)
    } else {
        return Err("Identity-switch storage does not match the committed journal".into());
    };
    let old_statement = owned_statement(&old_identity)?;
    let new_statement = owned_statement(&new_identity)?;
    let play_log = PlayLogStore::new(&storage.live_private_root());
    match journal.data_disposition() {
        DataDisposition::Transfer => play_log
            .transfer_person_records(
                &old_statement.owner_public_key,
                &new_statement.owner_public_key,
            )
            .map_err(|error| error.to_string())?,
        DataDisposition::Delete => play_log
            .delete_person_records(&old_statement.owner_public_key)
            .map_err(|error| error.to_string())?,
    }
    let (active_local_key, _) = signer.status().await?;
    signer
        .activate(
            active_local_key.filter(|key| key == &old_statement.owner_public_key),
            (journal.replacement_signer() == ReplacementSigner::Local)
                .then(|| new_statement.owner_public_key.clone()),
            new_statement.device_public_key.clone(),
            new_event.clone(),
        )
        .await?;

    if let Some(resources) = live_resources {
        revoke_stream_client_trust(
            resources
                .authorization
                .stream_client_grants()
                .map_err(|error| error.to_string())?,
        )
        .await?;
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
        let authorization =
            Authorization::load(&storage.live_private_root()).map_err(|error| error.to_string())?;
        revoke_stream_client_trust(
            authorization
                .stream_client_grants()
                .map_err(|error| error.to_string())?,
        )
        .await?;
        authorization
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

async fn revoke_stream_client_trust(grants: Vec<(String, String)>) -> Result<(), String> {
    if grants.is_empty() {
        return Ok(());
    }
    tokio::task::spawn_blocking(move || {
        let adapter = crate::host::moonlight_certificate::production_adapter();
        for (host_uuid, client_certificate) in grants {
            adapter
                .revoke(&host_uuid, &client_certificate)
                .map_err(|error| error.message)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| "Stream-client trust cleanup worker failed".to_owned())?
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

async fn owner_was_published(
    identity: &DeviceIdentity,
    event_json: &str,
) -> Result<bool, String> {
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
        .map(|relays| {
            CoordinatedRelays::new(
                relays,
                identity.clone(),
                Arc::new(WebSocketRelayTransport::default()),
            )
            .map(Arc::new)
            .map_err(|error| error.to_string())
        })
        .transpose()
}

fn signer_socket_from_environment() -> Result<PathBuf, String> {
    std::env::var_os("KORRID_LOCAL_SIGNER_SOCKET")
        .map(PathBuf::from)
        .ok_or_else(|| "KORRID_LOCAL_SIGNER_SOCKET must be set".to_owned())
}
