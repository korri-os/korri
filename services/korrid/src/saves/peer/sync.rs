use super::*;
use futures::{stream::FuturesUnordered, StreamExt};
use std::collections::{BTreeSet, VecDeque};
use tokio::time::Instant;

/// Process-local result, not a wire or persisted shape. Retain a known in-use
/// warning even when the subsequent content transfer exceeds the launch budget.
#[derive(Debug)]
pub struct PeerCheckResult {
    pub device: String,
    pub in_use: Option<SaveInUse>,
    pub transfer: Result<(), PeerSaveError>,
}

impl SavePeerService {
    fn owner_peers(&self, person: &str) -> Result<Vec<String>, PeerSaveError> {
        self.require_owner(person)?;
        Ok(self
            .directory
            .snapshot()
            .map_err(|_| PeerSaveError::Unauthorized)?
            .into_iter()
            .filter(|peer| peer.membership.owner_public_key == person)
            .map(|peer| peer.device_public_key)
            .collect())
    }

    /// Call after the snapshot is durable. There is no battery/network-type gate.
    /// Return each failure so the caller can retain NotChecked and retry on contact.
    pub async fn after_snapshot(
        &self,
        game: &str,
    ) -> Result<Vec<(String, Result<(), PeerSaveError>)>, PeerSaveError> {
        let person = self.owner()?;
        let work = self.owner_peers(&person)?.into_iter().map(|peer| {
            let person = person.clone();
            async move {
                let result = self.synchronize(&person, &peer, Some(game)).await;
                (peer, result)
            }
        });
        Ok(futures::future::join_all(work).await)
    }

    /// Call when a verified owner's peer becomes reachable or changes endpoint.
    /// The directory is checked again; callers cannot supply an arbitrary origin.
    pub async fn on_contact(&self, peer: &str) -> Result<(), PeerSaveError> {
        let person = self.owner()?;
        self.synchronize(&person, peer, None).await
    }

    /// Caller owns the 3-second budget and Skip. Dropping this future cancels
    /// network work; it never writes live save locations, only the private store.
    /// Keep per-peer failures: one offline device must not hide another's warning.
    pub async fn pull_before_launch(
        &self,
        game: &str,
        deadline: Instant,
    ) -> Result<Vec<PeerCheckResult>, PeerSaveError> {
        let person = self.owner()?;
        let work = self.owner_peers(&person)?.into_iter().map(|peer| {
            let person = person.clone();
            async move {
                let in_use = match tokio::time::timeout_at(
                    deadline,
                    self.remote_in_use(&person, &peer, game),
                )
                .await
                {
                    Ok(Ok(value)) => value,
                    Ok(Err(error)) => {
                        return PeerCheckResult {
                            device: peer,
                            in_use: None,
                            transfer: Err(error),
                        }
                    }
                    Err(_) => {
                        return PeerCheckResult {
                            device: peer,
                            in_use: None,
                            transfer: Err(PeerSaveError::Deadline),
                        }
                    }
                };
                let transfer =
                    tokio::time::timeout_at(deadline, self.synchronize(&person, &peer, Some(game)))
                        .await
                        .unwrap_or(Err(PeerSaveError::Deadline));
                PeerCheckResult {
                    device: peer,
                    in_use: Some(in_use),
                    transfer,
                }
            }
        });
        Ok(futures::future::join_all(work).await)
    }

    /// Fetch files for a snapshot already in local history into the private
    /// repository, without selecting it or writing game save folders. Restore
    /// owns that session-boundary step.
    /// Query peers concurrently so an offline peer cannot consume the deadline
    /// before a reachable device that still has the old payload is tried.
    pub async fn fetch_snapshot_payload(
        &self,
        game: &str,
        snapshot_id: &str,
        deadline: Instant,
    ) -> Result<(), PeerSaveError> {
        validate_hash(snapshot_id)?;
        let person = self.owner()?;
        tokio::time::timeout_at(deadline, async {
            let (person_key, game_key, id) =
                (person.clone(), game.to_owned(), snapshot_id.to_owned());
            let complete = self
                .storage(move |repository| repository.complete(&person_key, &game_key, &id))
                .await?;
            if complete {
                self.require_owner(&person)?;
                return Ok(());
            }
            let mut work: FuturesUnordered<_> = self
                .owner_peers(&person)?
                .into_iter()
                .map(|peer| {
                    let person = person.clone();
                    async move {
                        let response = self
                            .remote_records(
                                &person,
                                &peer,
                                SaveRecordsRequest {
                                    game_id: game.into(),
                                    ids: vec![snapshot_id.into()],
                                    records: None,
                                },
                            )
                            .await?;
                        if response.confirmed.iter().any(|id| id != snapshot_id)
                            || response.records.len() > 1
                        {
                            return Err(PeerSaveError::Protocol);
                        }
                        let record = response
                            .records
                            .into_iter()
                            .next()
                            .ok_or(PeerSaveError::PayloadUnavailable)?;
                        if !response.confirmed.iter().any(|id| id == snapshot_id) {
                            return Err(PeerSaveError::PayloadUnavailable);
                        }
                        if record.game_id != game {
                            return Err(PeerSaveError::Protocol);
                        }
                        self.import_record(&person, &peer, record.clone(), Some(snapshot_id))
                            .await?;
                        self.fetch_record_payload(&person, &peer, &record).await?;
                        self.confirm_response(
                            &person,
                            &peer,
                            game,
                            &[snapshot_id.into()],
                            response.confirmed,
                        )
                        .await
                    }
                })
                .collect();
            while let Some(result) = work.next().await {
                self.require_owner(&person)?;
                match result {
                    Ok(()) => return Ok(()),
                    Err(error @ PeerSaveError::Save(_)) => return Err(error),
                    Err(_) => {} // A different owner peer may still hold this payload.
                }
            }
            Err(PeerSaveError::PayloadUnavailable)
        })
        .await
        .map_err(|_| PeerSaveError::Deadline)?
    }

    async fn synchronize(
        &self,
        person: &str,
        peer: &str,
        only_game: Option<&str>,
    ) -> Result<(), PeerSaveError> {
        let remote = self.remote_heads(person, peer).await?;
        let person_key = person.to_owned();
        let local = self
            .storage(move |repository| repository.heads(&person_key))
            .await?;
        let mut games: BTreeSet<String> = remote
            .games
            .iter()
            .chain(&local.games)
            .map(|game| game.game_id.clone())
            .collect();
        if let Some(game) = only_game {
            games.retain(|id| id == game);
        }
        for game in games {
            let mut pending: VecDeque<String> = remote
                .games
                .iter()
                .filter(|heads| heads.game_id == game)
                .flat_map(|heads| heads.heads.clone())
                .collect();
            let advertised: BTreeSet<_> = pending.iter().cloned().collect();
            let mut visited = BTreeSet::new();
            let mut head_records = Vec::new();
            let mut confirmations = Vec::new();
            while let Some(id) = pending.pop_front() {
                validate_hash(&id)?;
                if !visited.insert(id.clone()) {
                    continue;
                }
                let response = self
                    .remote_records(
                        person,
                        peer,
                        SaveRecordsRequest {
                            game_id: game.clone(),
                            ids: vec![id.clone()],
                            records: None,
                        },
                    )
                    .await?;
                // Retention prunes payloads, never the ancestry metadata.
                if response.records.len() != 1 {
                    return Err(PeerSaveError::Protocol);
                }
                for record in response.records {
                    if record.game_id != game {
                        return Err(PeerSaveError::Protocol);
                    }
                    pending.extend(record.parents.clone());
                    self.import_record(person, peer, record.clone(), Some(&id))
                        .await?;
                    if advertised.contains(&id) {
                        head_records.push(record);
                    }
                }
                confirmations.push((id, response.confirmed));
            }
            // Import the full ancestry before a missing head payload can fail
            // this contact. Metadata remains useful even during transfer failure.
            for record in head_records {
                self.fetch_record_payload(person, peer, &record).await?;
            }
            for (id, confirmed) in confirmations {
                self.confirm_response(person, peer, &game, &[id], confirmed)
                    .await?;
            }
            let (person_key, game_key) = (person.to_owned(), game.clone());
            let records = self
                .storage(move |repository| repository.records(&person_key, &game_key))
                .await?;
            // One record per envelope keeps large histories from exceeding the
            // native peer envelope bound. Object bytes never enter RPC JSON.
            for record in records {
                let id = snapshot_id(&record)?;
                let response = self
                    .remote_records(
                        person,
                        peer,
                        SaveRecordsRequest {
                            game_id: game.clone(),
                            ids: vec![id.clone()],
                            records: Some(vec![record]),
                        },
                    )
                    .await?;
                self.confirm_response(person, peer, &game, &[id], response.confirmed)
                    .await?;
            }
        }
        Ok(())
    }
}
