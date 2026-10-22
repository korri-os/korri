//! Owner-only save replication. Transfer never materializes game save folders.
mod client;
mod sync;
pub use sync::PeerCheckResult;

#[cfg(test)]
mod tests;

use super::{
    record::{hash_hex, snapshot_id},
    types::*,
    SaveError, SaveRepository,
};
use crate::{
    authorization::{AuthorizationContext, Principal},
    federation::FederationDirectory,
    identity::IdentityState,
    peer_rpc::PeerCredentials,
    RpcFailure,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SaveHeadsOutcome {
    Ok(SaveHeads),
    Err(RpcFailure),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SaveRecordsOutcome {
    Ok(SaveRecords),
    Err(RpcFailure),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SaveInUseOutcome {
    Ok(SaveInUse),
    Err(RpcFailure),
}

#[derive(Debug, thiserror::Error)]
pub enum PeerSaveError {
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error("save peer is not an authenticated device of this owner")]
    Unauthorized,
    #[error("save peer transport is unavailable")]
    Network,
    #[error("save peer returned invalid data")]
    Protocol,
    #[error("save peer check reached its deadline")]
    Deadline,
    #[error("this save's files are not available on any reachable device")]
    PayloadUnavailable,
}

impl PeerSaveError {
    pub fn failure(&self) -> RpcFailure {
        RpcFailure {
            code: match self {
                Self::Unauthorized => "SavePeerUnauthorized",
                Self::Deadline => "SavePeerDeadline",
                Self::Protocol => "SavePeerInvalid",
                Self::Network => "SavePeerUnavailable",
                Self::Save(_) => "SaveStorageUnavailable",
                Self::PayloadUnavailable => "SavePayloadUnavailable",
            }
            .into(),
            // Storage diagnostics can contain private paths. Never serialize them.
            message: match self {
                Self::PayloadUnavailable => "This save's files are not available on any reachable device. Connect a device that has this save and retry.",
                Self::Deadline => "The save transfer timed out. Check that the other device is reachable and retry.",
                _ => "Saves could not be checked with this device.",
            }.into(),
        }
    }
}

/// The host supplies pending/live (frozen included) session knowledge. None is
/// idle; a timestamp is display metadata, not a lease or an expiry.
pub type InUse = Arc<dyn Fn(&str, &str) -> Result<Option<u64>, SaveError> + Send + Sync>;

#[derive(Clone)]
pub struct SavePeerService {
    repository: Arc<dyn SaveRepository>,
    credentials: PeerCredentials,
    directory: FederationDirectory,
    private_root: PathBuf,
    http: reqwest::Client,
    in_use: InUse,
}

impl SavePeerService {
    pub fn new(
        repository: Arc<dyn SaveRepository>,
        credentials: PeerCredentials,
        directory: FederationDirectory,
        private_root: PathBuf,
        in_use: InUse,
    ) -> Result<Self, PeerSaveError> {
        let http = reqwest::Client::builder()
            .connect_timeout(crate::federation::connect::CONNECT_TIMEOUT)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| PeerSaveError::Network)?;
        Ok(Self {
            repository,
            credentials,
            directory,
            private_root,
            http,
            in_use,
        })
    }

    fn owner(&self) -> Result<String, PeerSaveError> {
        match self
            .credentials
            .identity_snapshot()
            .map_err(|_| PeerSaveError::Unauthorized)?
            .state()
        {
            IdentityState::Owned {
                owner_public_key, ..
            } => Ok(owner_public_key.clone()),
            _ => Err(PeerSaveError::Unauthorized),
        }
    }

    fn require_owner(&self, person: &str) -> Result<(), PeerSaveError> {
        if self.owner()? == person {
            Ok(())
        } else {
            Err(PeerSaveError::Unauthorized)
        }
    }

    fn principal(&self, context: &AuthorizationContext) -> Result<(String, String), PeerSaveError> {
        let AuthorizationContext::Peer(Principal::OwnerDevice {
            owner_public_key,
            device_public_key,
            ..
        }) = context
        else {
            return Err(PeerSaveError::Unauthorized);
        };
        self.require_owner(owner_public_key)?;
        Ok((owner_public_key.clone(), device_public_key.clone()))
    }

    async fn storage<T: Send + 'static>(
        &self,
        operation: impl FnOnce(Arc<dyn SaveRepository>) -> Result<T, SaveError> + Send + 'static,
    ) -> Result<T, PeerSaveError> {
        let repository = self.repository.clone();
        tokio::task::spawn_blocking(move || operation(repository))
            .await
            .map_err(|_| PeerSaveError::Save(SaveError::Storage("save worker stopped".into())))?
            .map_err(Into::into)
    }

    pub async fn heads(&self, context: &AuthorizationContext) -> Result<SaveHeads, PeerSaveError> {
        let (person, _) = self.principal(context)?;
        self.storage(move |repository| repository.heads(&person))
            .await
    }

    pub fn in_use(
        &self,
        context: &AuthorizationContext,
        game: &str,
    ) -> Result<SaveInUse, PeerSaveError> {
        let (person, _) = self.principal(context)?;
        let device = self
            .credentials
            .public_key()
            .map_err(|_| PeerSaveError::Unauthorized)?;
        Ok(match (self.in_use)(&person, game)? {
            Some(since) => SaveInUse::Active(SaveInUseActive {
                device,
                in_use: true,
                since,
            }),
            None => SaveInUse::Idle(SaveInUseIdle {
                device,
                in_use: false,
            }),
        })
    }

    pub async fn records(
        &self,
        context: &AuthorizationContext,
        request: SaveRecordsRequest,
    ) -> Result<SaveRecords, PeerSaveError> {
        let (person, peer) = self.principal(context)?;
        for id in &request.ids {
            validate_hash(id)?;
        }
        if let Some(records) = &request.records {
            for record in records {
                if record.game_id != request.game_id {
                    return Err(PeerSaveError::Protocol);
                }
                self.import_record(&person, &peer, record.clone(), None)
                    .await?;
            }
            // The sender proves receipt through a signed confirmation too. A
            // record declaration by itself never counts as holding its objects.
            let ids = records
                .iter()
                .map(snapshot_id)
                .collect::<Result<Vec<_>, _>>()?;
            if !ids.is_empty() {
                // The sender's advertised heads, not this receiver's temporarily
                // incomplete graph, decide which pushed records need payloads.
                let heads = self.remote_heads(&person, &peer).await?;
                for record in records {
                    let id = snapshot_id(record)?;
                    if heads
                        .games
                        .iter()
                        .any(|game| game.game_id == request.game_id && game.heads.contains(&id))
                    {
                        self.fetch_record_payload(&person, &peer, record).await?;
                    }
                }
                let response = self
                    .remote_records(
                        &person,
                        &peer,
                        SaveRecordsRequest {
                            game_id: request.game_id.clone(),
                            ids: ids.clone(),
                            records: None,
                        },
                    )
                    .await?;
                self.confirm_response(&person, &peer, &request.game_id, &ids, response.confirmed)
                    .await?;
            }
        }
        self.require_owner(&person)?;
        let game = request.game_id;
        self.storage(move |repository| {
            let mut response = SaveRecords {
                records: Vec::new(),
                confirmed: Vec::new(),
            };
            for record in repository.records(&person, &game)? {
                let id = snapshot_id(&record)?;
                if request.ids.contains(&id) {
                    if repository.complete(&person, &game, &id)? {
                        response.confirmed.push(id);
                    }
                    response.records.push(record);
                }
            }
            Ok(response)
        })
        .await
    }

    pub async fn object(
        &self,
        context: &AuthorizationContext,
        hash: String,
    ) -> Result<std::fs::File, PeerSaveError> {
        validate_hash(&hash)?;
        let (person, _) = self.principal(context)?;
        self.storage(move |repository| repository.object(&person, &hash))
            .await
    }

    async fn confirm_response(
        &self,
        person: &str,
        peer: &str,
        game: &str,
        requested: &[String],
        confirmed: Vec<String>,
    ) -> Result<(), PeerSaveError> {
        if confirmed.iter().any(|id| !requested.contains(id)) {
            return Err(PeerSaveError::Protocol);
        }
        self.require_owner(person)?;
        let (person, peer, game) = (person.to_owned(), peer.to_owned(), game.to_owned());
        self.storage(move |repository| {
            for id in confirmed {
                repository.confirm(&person, &game, &id, &peer)?;
            }
            Ok(())
        })
        .await
    }
}

fn validate_hash(hash: &str) -> Result<(), PeerSaveError> {
    hash_hex(hash)
        .map(|_| ())
        .map_err(|_| PeerSaveError::Protocol)
}
