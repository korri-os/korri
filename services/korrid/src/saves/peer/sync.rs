use super::*;
use futures::{stream::FuturesUnordered, StreamExt};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tokio::time::Instant;

/// Process-local result, not a wire or persisted shape. Retain a known in-use
/// warning even when the subsequent content transfer exceeds the launch budget.
#[derive(Debug)]
pub struct PeerCheckResult {
    pub device: String,
    pub in_use: Option<SaveInUse>,
    pub transfer: Result<(), PeerSaveError>,
    /// Signed heads arrived, but their validated head records did not arrive
    /// during this check. This is process-local, not a wire or persisted field.
    pub heads_without_records: Vec<String>,
}

/// A contact whose heads on both sides match the last complete contact with
/// that peer finds nothing new, so it stops after the one SaveHeads call.
/// Restart doubles and Ready-after-Failed flaps then stay cheap. After this long
/// a full contact runs anyway, so a head payload the peer lost is still offered
/// again even when no heads changed.
const CONTACT_MEMORY_TTL: std::time::Duration = std::time::Duration::from_secs(30 * 60);

type HeadSets = BTreeMap<String, BTreeSet<String>>;

pub(super) struct ContactMark {
    remote: HeadSets,
    local: HeadSets,
    at: Instant,
}

/// Process-local, keyed by (person, peer). Not a wire or persisted shape.
#[derive(Default)]
pub(super) struct ContactMemory {
    pub(super) marks: BTreeMap<(String, String), ContactMark>,
    /// Records the peer holds as metadata but reported without their payload.
    /// Only a restore fetch on the peer changes that, so they are not asked
    /// about again in this process.
    pub(super) incomplete: BTreeMap<(String, String), BTreeSet<String>>,
}

fn head_sets(heads: &SaveHeads) -> HeadSets {
    heads
        .games
        .iter()
        .filter(|game| !game.heads.is_empty())
        .map(|game| (game.game_id.clone(), game.heads.iter().cloned().collect()))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum HeldRecord {
    /// Push the record, so the peer fetches the payload and both sides
    /// exchange receipts.
    Push,
    /// Ask for the record by id only. The answer carries the peer's receipt
    /// when it holds the payload. The peer imports nothing and calls nothing.
    Read,
    Skip,
}

/// What a record the peer already holds as metadata still needs. A record is
/// visited when it is in the ancestry of the peer's heads, and a peer imports
/// the full ancestry of its heads. A receiver fetches payloads only for the
/// sender's heads, so an older record whose payload it never fetched stays
/// unconfirmed for good (ADR 0014: older snapshots stay on the device that
/// made them). Pushing such a record on every contact cost three signed round
/// trips each and changed nothing.
fn held_record(
    purpose: SyncPurpose,
    id: &str,
    advertised: &BTreeSet<String>,
    local_heads: &BTreeSet<String>,
    confirmed: &BTreeSet<String>,
    remote_complete: &BTreeSet<String>,
    known_incomplete: &BTreeSet<String>,
) -> HeldRecord {
    match purpose {
        // Prelaunch needs no renewed historical receipts: only a shared head
        // without a current receipt from a complete peer is offered again.
        SyncPurpose::CheckLaunch => {
            if advertised.contains(id)
                && !(confirmed.contains(id) && remote_complete.contains(id))
            {
                HeldRecord::Push
            } else {
                HeldRecord::Skip
            }
        }
        SyncPurpose::Replicate | SyncPurpose::Contact => {
            // A peer head it lacks the payload for is offered so the peer can
            // repair it. Our own head without the peer's receipt is offered so
            // the peer fetches its payload and confirms it.
            if (advertised.contains(id) && !remote_complete.contains(id))
                || (local_heads.contains(id) && !confirmed.contains(id))
            {
                HeldRecord::Push
            } else if confirmed.contains(id)
                || remote_complete.contains(id)
                || known_incomplete.contains(id)
            {
                HeldRecord::Skip
            } else {
                // Retention still wants a receipt for an older record that the
                // peer may hold in full.
                HeldRecord::Read
            }
        }
    }
}

// Launch checks need current-head receipts, not fresh receipts for every
// historical snapshot. Contact/publication still gather retention receipts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncPurpose {
    Replicate,
    Contact,
    CheckLaunch,
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
                let result = self
                    .synchronize(
                        &person,
                        &peer,
                        Some(game),
                        &mut BTreeSet::new(),
                        SyncPurpose::Replicate,
                    )
                    .await;
                (peer, result)
            }
        });
        Ok(futures::future::join_all(work).await)
    }

    /// Call when a verified owner's peer becomes reachable or changes endpoint.
    /// The directory is checked again; callers cannot supply an arbitrary origin.
    pub async fn on_contact(&self, peer: &str) -> Result<(), PeerSaveError> {
        let person = self.owner()?;
        self.synchronize(
            &person,
            peer,
            None,
            &mut BTreeSet::new(),
            SyncPurpose::Contact,
        )
        .await
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
                            heads_without_records: Vec::new(),
                        }
                    }
                    Err(_) => {
                        return PeerCheckResult {
                            device: peer,
                            in_use: None,
                            transfer: Err(PeerSaveError::Deadline),
                            heads_without_records: Vec::new(),
                        }
                    }
                };
                // This state outlives the timed-out future, but only this check.
                let mut pending_head_records = BTreeSet::new();
                let transfer = tokio::time::timeout_at(
                    deadline,
                    self.synchronize(
                        &person,
                        &peer,
                        Some(game),
                        &mut pending_head_records,
                        SyncPurpose::CheckLaunch,
                    ),
                )
                .await
                .unwrap_or(Err(PeerSaveError::Deadline));
                let heads_without_records = if pending_head_records.is_empty() {
                    Vec::new()
                } else {
                    vec![peer.clone()]
                };
                PeerCheckResult {
                    device: peer,
                    in_use: Some(in_use),
                    transfer,
                    heads_without_records,
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
                                RequestProvenance::Independent,
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
        pending_head_records: &mut BTreeSet<String>,
        purpose: SyncPurpose,
    ) -> Result<(), PeerSaveError> {
        let provenance = match purpose {
            SyncPurpose::Contact => RequestProvenance::Contact,
            _ => RequestProvenance::Independent,
        };
        let remote = self.remote_heads(person, peer, provenance).await?;
        pending_head_records.extend(
            remote
                .games
                .iter()
                .filter(|game| only_game.is_none_or(|id| game.game_id == id))
                .flat_map(|game| game.heads.iter().cloned()),
        );
        let person_key = person.to_owned();
        let local = self
            .storage(move |repository| repository.heads(&person_key))
            .await?;
        let remote_mark = (purpose == SyncPurpose::Contact).then(|| head_sets(&remote));
        if let Some(remote_mark) = &remote_mark {
            if self.contact_unchanged(person, peer, remote_mark, &head_sets(&local)) {
                return Ok(());
            }
        }
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
            let (person_key, game_key, device) = (person.to_owned(), game.clone(), peer.to_owned());
            let (mut known, confirmed) = self
                .storage(move |repository| {
                    let records = repository
                        .records(&person_key, &game_key)?
                        .into_iter()
                        .map(|record| Ok((snapshot_id(&record)?, record)))
                        .collect::<Result<BTreeMap<_, _>, SaveError>>()?;
                    let confirmed =
                        repository.confirmed_snapshots(&person_key, &game_key, &device)?;
                    Ok((records, confirmed))
                })
                .await?;
            // An advertised head whose immutable record is already local does
            // not become a missing-record warning while its receipt is queried.
            pending_head_records.retain(|id| !known.contains_key(id));
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
            let mut remote_complete = BTreeSet::new();
            while let Some(id) = pending.pop_front() {
                validate_hash(&id)?;
                if !visited.insert(id.clone()) {
                    continue;
                }
                // Always ask for current head receipts: matching metadata is
                // not proof that the peer still holds the head's payload. Walk
                // ancestors from validated local records; fetch only gaps.
                if !advertised.contains(&id) {
                    if let Some(record) = known.get(&id) {
                        pending.extend(record.parents.clone());
                        continue;
                    }
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
                        provenance,
                    )
                    .await?;
                remote_complete.extend(response.confirmed.iter().cloned());
                // Retention prunes payloads, never the ancestry metadata.
                if response.records.len() != 1 {
                    return Err(PeerSaveError::Protocol);
                }
                for record in response.records {
                    if record.game_id != game || snapshot_id(&record)? != id {
                        return Err(PeerSaveError::Protocol);
                    }
                    // The signed record is now validated, independently of
                    // subsequent storage, ancestor or payload transfer work.
                    pending_head_records.remove(&id);
                    pending.extend(record.parents.clone());
                    if !known.contains_key(&id) {
                        self.import_record(person, peer, record.clone(), Some(&id))
                            .await?;
                        known.insert(id.clone(), record.clone());
                    }
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
            let local_heads: BTreeSet<String> =
                crate::saves::store::head_ids(&records)?.into_iter().collect();
            let known_incomplete = self.known_incomplete(person, peer);
            // One record per envelope keeps large histories from exceeding the
            // native peer envelope bound. Object bytes never enter RPC JSON.
            for record in records {
                let id = snapshot_id(&record)?;
                // A record the peer lacks is always pushed, with its receipt
                // exchange. A record it already holds is pushed only to move a
                // head payload (see held_record).
                if visited.contains(&id) {
                    match held_record(
                        purpose,
                        &id,
                        &advertised,
                        &local_heads,
                        &confirmed,
                        &remote_complete,
                        &known_incomplete,
                    ) {
                        HeldRecord::Push => {}
                        HeldRecord::Skip => continue,
                        HeldRecord::Read => {
                            let response = self
                                .remote_records(
                                    person,
                                    peer,
                                    SaveRecordsRequest {
                                        game_id: game.clone(),
                                        ids: vec![id.clone()],
                                        records: None,
                                    },
                                    provenance,
                                )
                                .await?;
                            if !response.confirmed.contains(&id) {
                                self.remember_incomplete(person, peer, &id);
                            }
                            self.confirm_response(person, peer, &game, &[id], response.confirmed)
                                .await?;
                            continue;
                        }
                    }
                }
                let response = self
                    .remote_records(
                        person,
                        peer,
                        SaveRecordsRequest {
                            game_id: game.clone(),
                            ids: vec![id.clone()],
                            records: Some(vec![record]),
                        },
                        provenance,
                    )
                    .await?;
                self.confirm_response(person, peer, &game, &[id], response.confirmed)
                    .await?;
            }
        }
        if let Some(remote_mark) = remote_mark {
            // Our heads can change while the contact imports the peer's
            // records. A push can change the peer's heads too; the next contact
            // then runs once more in full and records the settled heads.
            let person_key = person.to_owned();
            let local = self
                .storage(move |repository| repository.heads(&person_key))
                .await?;
            self.remember_contact(person, peer, remote_mark, head_sets(&local));
        }
        Ok(())
    }

    fn contact_unchanged(
        &self,
        person: &str,
        peer: &str,
        remote: &HeadSets,
        local: &HeadSets,
    ) -> bool {
        self.contacts.lock().is_ok_and(|memory| {
            memory
                .marks
                .get(&(person.to_owned(), peer.to_owned()))
                .is_some_and(|mark| {
                    mark.at.elapsed() < CONTACT_MEMORY_TTL
                        && &mark.remote == remote
                        && &mark.local == local
                })
        })
    }

    fn remember_contact(&self, person: &str, peer: &str, remote: HeadSets, local: HeadSets) {
        if let Ok(mut memory) = self.contacts.lock() {
            memory.marks.insert(
                (person.to_owned(), peer.to_owned()),
                ContactMark {
                    remote,
                    local,
                    at: Instant::now(),
                },
            );
        }
    }

    fn known_incomplete(&self, person: &str, peer: &str) -> BTreeSet<String> {
        self.contacts
            .lock()
            .ok()
            .and_then(|memory| {
                memory
                    .incomplete
                    .get(&(person.to_owned(), peer.to_owned()))
                    .cloned()
            })
            .unwrap_or_default()
    }

    fn remember_incomplete(&self, person: &str, peer: &str, id: &str) {
        if let Ok(mut memory) = self.contacts.lock() {
            memory
                .incomplete
                .entry((person.to_owned(), peer.to_owned()))
                .or_default()
                .insert(id.to_owned());
        }
    }
}
