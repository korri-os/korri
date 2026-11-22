use super::*;
use crate::{
    federation::{PeerState, WorkToken},
    identity::OwnerStatementStatus,
    peer_rpc::{unix_time, MAX_PEER_EVENT_BYTES},
    RpcRequest, RpcResponse,
};
use reqwest::{header, StatusCode};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    time::Duration,
};

const TRANSFER_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const STALL_TIMEOUT: Duration = Duration::from_secs(60);

impl SavePeerService {
    fn require_peer(&self, person: &str, device: &str) -> Result<(), PeerSaveError> {
        self.require_owner(person)?;
        self.directory
            .snapshot()
            .map_err(|_| PeerSaveError::Unauthorized)?
            .into_iter()
            .find(|peer| {
                peer.device_public_key == device
                    && peer.membership.owner_public_key == person
                    && peer.membership.status == OwnerStatementStatus::Owned
            })
            .ok_or(PeerSaveError::Unauthorized)?;
        Ok(())
    }

    async fn request<T>(
        &self,
        person: &str,
        peer: &str,
        request: RpcRequest,
        provenance: RequestProvenance,
        checked: impl FnOnce(RpcResponse) -> Result<T, PeerSaveError>,
    ) -> Result<T, PeerSaveError> {
        let mut token = None;
        let result = self
            .request_candidates(person, peer, request, provenance, &mut token)
            .await
            .and_then(checked);
        if let Some(token) = token {
            let state = match &result {
                Ok(_) => PeerState::Ready,
                Err(_) => PeerState::Failed {
                    error: "Authenticated peer operation failed".into(),
                },
            };
            // Superseded, revoked or configuration-changed work cannot overwrite
            // newer state. Contact provenance also prevents scheduling itself.
            let _ = self.directory.set_peer_state(&token, peer, state);
        }
        result
    }

    async fn request_candidates(
        &self,
        person: &str,
        peer: &str,
        request: RpcRequest,
        provenance: RequestProvenance,
        token: &mut Option<WorkToken>,
    ) -> Result<RpcResponse, PeerSaveError> {
        self.require_peer(person, peer)?;
        if !self.directory.has_peer_connection(peer)
            && !self
                .directory
                .snapshot()
                .map_err(|_| PeerSaveError::Unauthorized)?
                .iter()
                .any(|entry| entry.device_public_key == peer && entry.remembered_endpoint.is_some())
        {
            return Err(PeerSaveError::Network);
        }
        *token = Some(
            match provenance {
                RequestProvenance::Independent => self.directory.begin_peer_work(peer),
                RequestProvenance::Contact => self.directory.begin_peer_work_from_contact(peer),
            }
            .map_err(|_| PeerSaveError::Unauthorized)?,
        );
        let deadline = tokio::time::Instant::now() + TRANSFER_TIMEOUT;
        let connection = self
            .directory
            .reach_connection(peer)
            .await
            .map_err(|_| PeerSaveError::Network)?;
        let encoded = self
            .credentials
            .encode_request(peer, request.clone(), unix_time())
            .map_err(|_| PeerSaveError::Unauthorized)?;
        let result = tokio::time::timeout_at(deadline, async {
            let mut response = crate::federation::reach::http::post(
                &connection,
                "/peer-rpc",
                encoded.event_json.clone(),
                None,
            )
            .await
            .map_err(|_| PeerSaveError::Network)?;
            if !response.status().is_success() {
                return Err(PeerSaveError::Network);
            }
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| PeerSaveError::Network)? {
                if body.len().saturating_add(chunk.len()) > MAX_PEER_EVENT_BYTES {
                    return Err(PeerSaveError::Protocol);
                }
                body.extend_from_slice(&chunk);
            }
            // Recheck the shared identity and roster after asynchronous work.
            self.require_peer(person, peer)?;
            self.credentials
                .decode_response(
                    peer,
                    &encoded,
                    std::str::from_utf8(&body).map_err(|_| PeerSaveError::Protocol)?,
                    unix_time(),
                )
                .map_err(|_| PeerSaveError::Protocol)
        })
        .await
        .map_err(|_| PeerSaveError::Deadline);
        result?
    }

    pub(super) async fn remote_heads(
        &self,
        person: &str,
        peer: &str,
        provenance: RequestProvenance,
    ) -> Result<SaveHeads, PeerSaveError> {
        self.request(
            person,
            peer,
            RpcRequest::SaveHeads(SaveHeadsRequest {}),
            provenance,
            |response| match response {
                RpcResponse::SaveHeads(SaveHeadsOutcome::Ok(value)) => Ok(value),
                RpcResponse::SaveHeads(SaveHeadsOutcome::Err(_)) => Err(PeerSaveError::Network),
                _ => Err(PeerSaveError::Protocol),
            },
        )
        .await
    }

    pub(super) async fn remote_records(
        &self,
        person: &str,
        peer: &str,
        request: SaveRecordsRequest,
        provenance: RequestProvenance,
    ) -> Result<SaveRecords, PeerSaveError> {
        self.request(
            person,
            peer,
            RpcRequest::SaveRecords(request),
            provenance,
            |response| match response {
                RpcResponse::SaveRecords(SaveRecordsOutcome::Ok(value)) => Ok(value),
                RpcResponse::SaveRecords(SaveRecordsOutcome::Err(_)) => Err(PeerSaveError::Network),
                _ => Err(PeerSaveError::Protocol),
            },
        )
        .await
    }

    pub(super) async fn remote_in_use(
        &self,
        person: &str,
        peer: &str,
        game: &str,
    ) -> Result<SaveInUse, PeerSaveError> {
        self.request(
            person,
            peer,
            RpcRequest::SaveInUse(SaveGameRequest {
                game_id: game.into(),
            }),
            RequestProvenance::Independent,
            |response| match response {
                RpcResponse::SaveInUse(SaveInUseOutcome::Ok(value)) => {
                    match &value {
                        SaveInUse::Active(active) if active.device == peer && active.in_use => {}
                        SaveInUse::Idle(idle) if idle.device == peer && !idle.in_use => {}
                        _ => return Err(PeerSaveError::Protocol),
                    }
                    Ok(value)
                }
                RpcResponse::SaveInUse(SaveInUseOutcome::Err(_)) => Err(PeerSaveError::Network),
                _ => Err(PeerSaveError::Protocol),
            },
        )
        .await
    }

    pub(super) async fn import_record(
        &self,
        person: &str,
        peer: &str,
        record: SnapshotRecord,
        expected_id: Option<&str>,
    ) -> Result<String, PeerSaveError> {
        self.require_peer(person, peer)?;
        let id = snapshot_id(&record)?;
        if expected_id.is_some_and(|expected| expected != id) {
            return Err(PeerSaveError::Protocol);
        }
        let person_key = person.to_owned();
        let incoming = record.clone();
        self.storage(move |repository| repository.put_record(&person_key, &incoming))
            .await?;
        Ok(id)
    }

    // Payload transfer is separate from ancestry import: historical records
    // remain useful after retention has removed their objects.
    pub(super) async fn fetch_record_payload(
        &self,
        person: &str,
        peer: &str,
        record: &SnapshotRecord,
    ) -> Result<(), PeerSaveError> {
        // One read of this record answers for all its files. Asking the store
        // for each object made it read every record of every game per file.
        let (person_key, game, id) = (
            person.to_owned(),
            record.game_id.clone(),
            snapshot_id(record)?,
        );
        let mut held = self
            .storage(move |repository| repository.payload_sizes(&person_key, &game, &id))
            .await?;
        for file in record.locations.iter().flat_map(|l| &l.files) {
            match held.get(&file.sha256) {
                Some(bytes) if *bytes == file.bytes => {}
                Some(_) => return Err(PeerSaveError::Protocol),
                None => {
                    self.download_object(person, peer, file).await?;
                    held.insert(file.sha256.clone(), file.bytes);
                }
            }
        }
        Ok(())
    }

    async fn download_object(
        &self,
        person: &str,
        peer: &str,
        file: &SaveFile,
    ) -> Result<(), PeerSaveError> {
        validate_hash(&file.sha256)?;
        let metadata = std::fs::symlink_metadata(&self.private_root)
            .map_err(|e| SaveError::Storage(e.to_string()))?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(PeerSaveError::Unauthorized);
        }
        // An anonymous private file avoids inventing a persisted partial-file
        // schema. Retries resume this descriptor; cancellation leaves no file.
        let mut output = OpenOptions::new()
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_TMPFILE | libc::O_NOFOLLOW)
            .open(&self.private_root)
            .map_err(|e| SaveError::Storage(e.to_string()))?;
        let mut done = 0;
        let mut hash = Sha256::new();
        let mut attempt = 0;
        let connection = self
            .directory
            .reach_connection(peer)
            .await
            .map_err(|_| PeerSaveError::Network)?;
        loop {
            self.require_peer(person, peer)?;
            if connection.close_reason().is_some() {
                return Err(PeerSaveError::Network);
            }
            if attempt >= 6 {
                break;
            }
            attempt += 1;
            let encoded = self
                .credentials
                .encode_request(
                    peer,
                    RpcRequest::SaveObject(SaveObjectRequest {
                        sha256: file.sha256.clone(),
                    }),
                    unix_time(),
                )
                .map_err(|_| PeerSaveError::Unauthorized)?;
            let range = (done > 0).then(|| format!("bytes={done}-"));
            let transfer = crate::federation::reach::http::post(
                &connection,
                "/peer-saves-object",
                encoded.event_json,
                range,
            );
            let mut response = match tokio::time::timeout(STALL_TIMEOUT, transfer).await {
                Ok(Ok(response)) => response,
                _ => continue,
            };
            match response.status() {
                StatusCode::RANGE_NOT_SATISFIABLE => {
                    reset(&mut output)?;
                    done = 0;
                    hash = Sha256::new();
                    continue;
                }
                StatusCode::OK => {
                    // A peer may ignore Range. Never append its complete body.
                    reset(&mut output)?;
                    done = 0;
                    hash = Sha256::new();
                }
                StatusCode::PARTIAL_CONTENT => {
                    let range = response
                        .headers()
                        .get(header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok())
                        .ok_or(PeerSaveError::Protocol)?;
                    validate_content_range(range, done, file.bytes)?;
                }
                _ => continue,
            }
            if response
                .content_length()
                .is_some_and(|n| n != file.bytes.saturating_sub(done))
            {
                return Err(PeerSaveError::Protocol);
            }
            let mut finished = false;
            let mut window_start = tokio::time::Instant::now();
            let mut window_bytes = done;
            loop {
                let chunk = match tokio::time::timeout(STALL_TIMEOUT, response.chunk()).await {
                    Ok(Ok(Some(chunk))) => chunk,
                    Ok(Ok(None)) => {
                        finished = true;
                        break;
                    }
                    _ => break,
                };
                done = done
                    .checked_add(chunk.len() as u64)
                    .ok_or(PeerSaveError::Protocol)?;
                if done > file.bytes {
                    return Err(PeerSaveError::Protocol);
                }
                let elapsed = window_start.elapsed();
                if elapsed >= STALL_TIMEOUT {
                    if (done - window_bytes) as f64 / elapsed.as_secs_f64() < 1024.0 {
                        // Do not count or hash a chunk we did not write.
                        done -= chunk.len() as u64;
                        break;
                    }
                    window_start = tokio::time::Instant::now();
                    window_bytes = done;
                }
                hash.update(&chunk);
                output = tokio::task::spawn_blocking(move || {
                    output.write_all(&chunk)?;
                    Ok::<_, std::io::Error>(output)
                })
                .await
                .map_err(|_| PeerSaveError::Network)?
                .map_err(|e| SaveError::Storage(e.to_string()))?;
            }
            if finished && done == file.bytes {
                if format!("sha256:{}", hex::encode(hash.clone().finalize())) != file.sha256 {
                    reset(&mut output)?;
                    done = 0;
                    hash = Sha256::new();
                    continue;
                }
                self.require_peer(person, peer)?;
                let (person, hash) = (person.to_owned(), file.sha256.clone());
                return self
                    .storage(move |repository| {
                        output
                            .rewind()
                            .map_err(|e| SaveError::Storage(e.to_string()))?;
                        repository.put_object(&person, &hash, &mut output)
                    })
                    .await;
            }
        }
        Err(PeerSaveError::Network)
    }
}

fn reset(output: &mut File) -> Result<(), PeerSaveError> {
    output
        .set_len(0)
        .and_then(|_| output.seek(SeekFrom::Start(0)))
        .map_err(|e| SaveError::Storage(e.to_string()))?;
    Ok(())
}

fn validate_content_range(value: &str, offset: u64, bytes: u64) -> Result<(), PeerSaveError> {
    let (range, total) = value
        .strip_prefix("bytes ")
        .and_then(|v| v.split_once('/'))
        .ok_or(PeerSaveError::Protocol)?;
    let (start, end) = range.split_once('-').ok_or(PeerSaveError::Protocol)?;
    let parse = |s: &str| s.parse::<u64>().map_err(|_| PeerSaveError::Protocol);
    if parse(start)? != offset
        || parse(end)?.checked_add(1) != Some(bytes)
        || parse(total)? != bytes
        || offset >= bytes
    {
        return Err(PeerSaveError::Protocol);
    }
    Ok(())
}
