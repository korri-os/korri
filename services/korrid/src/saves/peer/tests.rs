use super::*;
use crate::{
    authorization::Authorization,
    federation::FederationDirectory,
    peer_rpc::{self, unix_time, PeerRpcServer},
    relay::{EndpointRecord, ENDPOINT_EVENT_KIND},
    RpcRequest,
};
use axum::{
    body::Body,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::Response,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    sync::Mutex,
    time::Duration,
};

use crate::saves::store::NativeSaveStore;
use std::os::unix::fs::PermissionsExt;

fn digest(bytes: &[u8]) -> String {
    crate::saves::record::hash(bytes)
}
fn id(record: &SnapshotRecord) -> String {
    snapshot_id(record).unwrap()
}

#[derive(Clone, Default)]
struct TransferBehaviour {
    ranges: Arc<Mutex<Vec<Option<String>>>>,
    fail_first: bool,
    corrupt: bool,
    delay: Duration,
}
async fn transfer_behaviour(
    axum::extract::State(behaviour): axum::extract::State<TransferBehaviour>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if request.uri().path() != "/peer-saves-object" {
        tokio::time::sleep(behaviour.delay).await;
        return next.run(request).await;
    }
    let first = {
        let mut ranges = behaviour.ranges.lock().unwrap();
        ranges.push(
            request
                .headers()
                .get(header::RANGE)
                .map(|value| value.to_str().unwrap().to_owned()),
        );
        ranges.len() == 1
    };
    let mut response = next.run(request).await;
    if response.status() == StatusCode::OK && (behaviour.corrupt || (behaviour.fail_first && first))
    {
        let body = std::mem::replace(response.body_mut(), Body::empty());
        let bytes = axum::body::to_bytes(body, 1024 * 1024).await.unwrap();
        if behaviour.corrupt {
            *response.body_mut() = Body::from(vec![0; bytes.len()]);
        } else {
            let first = bytes.slice(..4);
            use futures::StreamExt;
            let stream = futures::stream::once(async move { Ok::<_, std::io::Error>(first) })
                .chain(futures::stream::once(async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Err(std::io::Error::other("connection interrupted"))
                }));
            *response.body_mut() = Body::from_stream(stream);
        }
    }
    response
}

struct Node {
    _root: tempfile::TempDir,
    service: Arc<SavePeerService>,
    repository: Arc<NativeSaveStore>,
    base: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Node {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Node {
    async fn new(device: u8, owner: u8, behaviour: TransferBehaviour) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let identity = peer_rpc::test_owned_identity(
            root.path(),
            &format!("{device:064x}"),
            &format!("{owner:064x}"),
        );
        let credentials = PeerCredentials::from_identity(identity);
        let authorization = Authorization::load(root.path()).unwrap();
        let directory = FederationDirectory::open(
            root.path(),
            credentials.clone(),
            authorization.clone(),
            Arc::new(unix_time),
        )
        .unwrap();
        let repository = Arc::new(NativeSaveStore::new(root.path()));
        let service = Arc::new(
            SavePeerService::new(
                repository.clone(),
                credentials.clone(),
                directory,
                root.path().to_owned(),
                Arc::new(|_, _| Ok(Some(123))),
            )
            .unwrap(),
        );
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"save-test\"\n").unwrap();
        let runtime = crate::host::HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().to_owned(),
            Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
        );
        let (mut state, _) = crate::app_states(runtime);
        state.saves = Some(service.clone());
        let server =
            PeerRpcServer::with_shared_authority(state, root.path(), credentials, authorization)
                .unwrap()
                .router(None)
                .layer(middleware::from_fn_with_state(
                    behaviour,
                    transfer_behaviour,
                ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, server).await.unwrap();
        });
        Self {
            _root: root,
            service,
            repository,
            base,
            task,
        }
    }
    fn confirmations(&self) -> BTreeMap<String, BTreeSet<String>> {
        let path = self
            ._root
            .path()
            .join("saves")
            .join(self.owner())
            .join("wario-land-4/confirmations.json");
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => panic!("{error}"),
        }
    }
    fn device(&self) -> String {
        self.service.credentials.public_key().unwrap()
    }
    fn owner(&self) -> String {
        self.service.owner().unwrap()
    }
    fn remember(&self, peer: &Node) {
        let identity = peer.service.credentials.identity_snapshot().unwrap();
        let token = self.service.directory.begin_work().unwrap();
        self.service
            .directory
            .apply_membership(&token, &[identity.owner_statement_json().unwrap()])
            .unwrap();
        let now = unix_time();
        let endpoint = EndpointRecord {
            device_public_key: peer.device(),
            owner_public_key: peer.owner(),
            generation: 1,
            candidates: vec![peer.base.clone()],
            issued_at: now,
            expires_at: now + 3600,
            label: None,
            moonlight_address: None,
        };
        let event = identity
            .encrypt_tagged_event(
                &self.device(),
                ENDPOINT_EVENT_KIND,
                vec![
                    vec!["d".into(), format!("org.korri.endpoint:{}", self.device())],
                    vec!["p".into(), self.device()],
                    vec!["expiration".into(), endpoint.expires_at.to_string()],
                ],
                &serde_json::to_string(&endpoint).unwrap(),
                now,
            )
            .unwrap();
        self.service
            .directory
            .apply_endpoint_event(&self.service.directory.begin_work().unwrap(), &event.json)
            .unwrap();
    }
    fn seed(&self, bytes: &[u8], parents: Vec<String>) -> SnapshotRecord {
        let record = SnapshotRecord {
            parents,
            game_id: "wario-land-4".into(),
            release_id: Some("usa".into()),
            runner_id: "@korri:mgba/mgba".into(),
            device: self.device(),
            ended_at: "2026-10-21T00:00:00.000Z".into(),
            duration_seconds: 60,
            interrupted: false,
            locations: vec![SnapshotLocation {
                key: "saves".into(),
                kind: SaveKind::Save,
                files: vec![SaveFile {
                    path: "Wario.srm".into(),
                    sha256: digest(bytes),
                    bytes: bytes.len() as u64,
                }],
            }],
        };
        self.repository.put_record(&self.owner(), &record).unwrap();
        self.repository
            .put_object(&self.owner(), &digest(bytes), &mut &bytes[..])
            .unwrap();
        record
    }
    async fn object_request(
        &self,
        receiver: &Node,
        hash: &str,
        range: Option<&str>,
        route: &str,
    ) -> reqwest::Response {
        let encoded = self
            .service
            .credentials
            .encode_request(
                &receiver.device(),
                RpcRequest::SaveObject(SaveObjectRequest {
                    sha256: hash.into(),
                }),
                unix_time(),
            )
            .unwrap();
        let request = reqwest::Client::new()
            .post(format!("{}{route}", receiver.base))
            .body(encoded.event_json);
        let request = if let Some(range) = range {
            request.header(header::RANGE, range)
        } else {
            request
        };
        request.send().await.unwrap()
    }
}

#[tokio::test]
async fn two_authenticated_korrids_exchange_both_children_objects_and_confirmations() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let base = a.seed(b"base progress", vec![]);
    b.repository.put_record(&b.owner(), &base).unwrap();
    b.repository
        .put_object(
            &b.owner(),
            &digest(b"base progress"),
            &mut &b"base progress"[..],
        )
        .unwrap();
    let left = a.seed(b"left progress", vec![id(&base)]);
    let right = b.seed(b"right progress", vec![id(&base)]);
    a.service.on_contact(&b.device()).await.unwrap();
    for (node, peer) in [(&a, &b), (&b, &a)] {
        let heads = node.repository.heads(&node.owner()).unwrap();
        assert_eq!(heads.games.len(), 1);
        let expected: BTreeSet<_> = [id(&left), id(&right)].into_iter().collect();
        assert_eq!(
            heads.games[0]
                .heads
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected
        );
        // Both heads name the same parent and different save hashes: a split,
        // not last-writer-wins. All three immutable versions remain readable.
        for record in [&base, &left, &right] {
            assert!(node
                .repository
                .complete(&node.owner(), "wario-land-4", &id(record))
                .unwrap());
            assert!(node
                .confirmations()
                .get(&id(record))
                .is_some_and(|devices| devices.contains(&peer.device())));
        }
        let warnings = node
            .service
            .pull_before_launch(
                "wario-land-4",
                tokio::time::Instant::now() + Duration::from_secs(3),
            )
            .await
            .unwrap();
        assert!(
            matches!(&warnings[0].in_use, Some(SaveInUse::Active(value)) if value.since == 123)
        );
    }
}

#[tokio::test]
async fn object_route_authenticates_scopes_ranges_and_rejects_rpc_route_reuse() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    let stranger = Node::new(7, 4, TransferBehaviour::default()).await;
    let record = a.seed(b"0123456789", vec![]);
    let hash = &record.locations[0].files[0].sha256;
    let range = b
        .object_request(&a, hash, Some("bytes=2-4"), "/peer-saves-object")
        .await;
    assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(range.headers()[header::CONTENT_RANGE], "bytes 2-4/10");
    assert_eq!(&range.bytes().await.unwrap()[..], b"234");
    assert_eq!(
        b.object_request(&a, hash, Some("bytes=10-"), "/peer-saves-object")
            .await
            .status(),
        StatusCode::RANGE_NOT_SATISFIABLE
    );
    assert_eq!(
        b.object_request(&a, hash, None, "/peer-rpc").await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        stranger
            .object_request(&a, hash, None, "/peer-saves-object")
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert!(a
        .service
        .heads(&AuthorizationContext::LocalBrowser)
        .await
        .is_err());
    assert!(a
        .service
        .heads(&AuthorizationContext::Peer(Principal::Unknown {
            device_public_key: b.device()
        }))
        .await
        .is_err());
    let event = b
        .service
        .credentials
        .encode_request(
            &a.device(),
            RpcRequest::SaveHeads(SaveHeadsRequest {}),
            unix_time(),
        )
        .unwrap();
    assert_eq!(
        reqwest::Client::new()
            .post(format!("{}/peer-saves-object", a.base))
            .body(event.event_json)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn declarations_without_verified_objects_are_never_confirmed() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let record = a.seed(b"missing", vec![]);
    std::fs::remove_file(
        a._root
            .path()
            .join("saves/objects")
            .join(digest(b"missing").strip_prefix("sha256:").unwrap()),
    )
    .unwrap();
    let response = b
        .service
        .remote_records(
            &b.owner(),
            &a.device(),
            SaveRecordsRequest {
                game_id: record.game_id.clone(),
                ids: vec![id(&record)],
                records: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(response.records, vec![record]);
    assert!(response.confirmed.is_empty());
}

#[tokio::test]
async fn interrupted_object_stream_resumes_with_a_new_signed_range_request() {
    let behaviour = TransferBehaviour {
        fail_first: true,
        ..Default::default()
    };
    let a = Node::new(5, 3, behaviour.clone()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let record = a.seed(b"recover all this progress", vec![]);
    b.service.on_contact(&a.device()).await.unwrap();
    assert!(behaviour
        .ranges
        .lock()
        .unwrap()
        .iter()
        .any(|range| range.as_deref() == Some("bytes=4-")));
    assert!(b
        .repository
        .complete(&b.owner(), &record.game_id, &id(&record))
        .unwrap());
}

#[tokio::test]
async fn corrupted_object_bytes_never_enter_repository_and_deadline_cancels_pull() {
    let a = Node::new(
        5,
        3,
        TransferBehaviour {
            corrupt: true,
            ..Default::default()
        },
    )
    .await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let record = a.seed(b"valuable progress", vec![]);
    assert!(b.service.on_contact(&a.device()).await.is_err());
    assert!(!b
        .repository
        .complete(&b.owner(), &record.game_id, &id(&record))
        .unwrap());
    assert!(b
        .repository
        .object(&b.owner(), &record.locations[0].files[0].sha256)
        .is_err());
    assert!(b.confirmations().is_empty());
    let result = b
        .service
        .pull_before_launch("wario-land-4", tokio::time::Instant::now())
        .await
        .unwrap();
    assert!(matches!(result[0].transfer, Err(PeerSaveError::Deadline)));
}

#[tokio::test]
async fn large_state_bytes_use_raw_transfer_not_the_rpc_envelope() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let bytes = vec![42; peer_rpc::MAX_PEER_EVENT_BYTES + 1];
    let record = a.seed(&bytes, vec![]);
    b.service.on_contact(&a.device()).await.unwrap();
    let mut object = b.repository.object(&b.owner(), &digest(&bytes)).unwrap();
    let mut received = Vec::new();
    object.read_to_end(&mut received).unwrap();
    assert_eq!(received, bytes);
    assert!(b
        .repository
        .complete(&b.owner(), &record.game_id, &id(&record))
        .unwrap());
}

#[tokio::test]
async fn object_nonces_share_replay_guard_and_known_warning_survives_transfer_deadline() {
    let a = Node::new(
        5,
        3,
        TransferBehaviour {
            corrupt: true,
            ..Default::default()
        },
    )
    .await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let record = a.seed(b"save bytes", vec![]);
    let encoded = b
        .service
        .credentials
        .encode_request(
            &a.device(),
            RpcRequest::SaveObject(SaveObjectRequest {
                sha256: record.locations[0].files[0].sha256.clone(),
            }),
            unix_time(),
        )
        .unwrap();
    let http = reqwest::Client::new();
    let url = format!("{}/peer-saves-object", a.base);
    assert_eq!(
        http.post(&url)
            .body(encoded.event_json.clone())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        http.post(&url)
            .body(encoded.event_json)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let result = b
        .service
        .pull_before_launch(
            "wario-land-4",
            tokio::time::Instant::now() + Duration::from_millis(200),
        )
        .await
        .unwrap();
    assert!(matches!(result[0].in_use, Some(SaveInUse::Active(_))));
    assert!(matches!(result[0].transfer, Err(PeerSaveError::Deadline)));
}

#[tokio::test]
async fn snapshot_hook_pushes_to_each_verified_owner_peer() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let record = a.seed(b"just exited", vec![]);
    let results = a.service.after_snapshot("wario-land-4").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, b.device());
    assert!(results[0].1.is_ok());
    assert!(b
        .repository
        .complete(&b.owner(), &record.game_id, &id(&record))
        .unwrap());
    assert!(a.confirmations()[&id(&record)].contains(&b.device()));
}

fn prune_payload(node: &Node, record: &SnapshotRecord) {
    for file in record.locations.iter().flat_map(|location| &location.files) {
        let path = node
            ._root
            .path()
            .join("saves/objects")
            .join(hash_hex(&file.sha256).unwrap());
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }
}

#[tokio::test]
async fn meet_keeps_ancestor_metadata_when_historical_payloads_are_pruned() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let base = a.seed(b"old shared save", vec![]);
    b.repository.put_record(&b.owner(), &base).unwrap();
    let left_history = a.seed(b"old left save", vec![id(&base)]);
    let right_history = b.seed(b"old right save", vec![id(&base)]);
    let left = a.seed(b"current left save", vec![id(&left_history)]);
    let right = b.seed(b"current right save", vec![id(&right_history)]);
    prune_payload(&a, &base);
    prune_payload(&a, &left_history);
    prune_payload(&b, &right_history);

    a.service.on_contact(&b.device()).await.unwrap();
    for node in [&a, &b] {
        let records = node
            .repository
            .records(&node.owner(), &base.game_id)
            .unwrap();
        for ancestor in [&base, &left_history, &right_history] {
            assert!(records.contains(ancestor));
            assert!(!node
                .repository
                .complete(&node.owner(), &base.game_id, &id(ancestor))
                .unwrap());
        }
        let heads = node
            .repository
            .heads(&node.owner())
            .unwrap()
            .games
            .remove(0)
            .heads;
        assert_eq!(
            heads.into_iter().collect::<BTreeSet<_>>(),
            [id(&left), id(&right)].into_iter().collect()
        );
        for head in [&left, &right] {
            assert!(node
                .repository
                .complete(&node.owner(), &head.game_id, &id(head))
                .unwrap());
        }
    }
    let checks = b
        .service
        .pull_before_launch(
            &base.game_id,
            tokio::time::Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap();
    assert!(checks.iter().all(|check| check.transfer.is_ok()));
}

#[tokio::test]
async fn snapshot_push_does_not_fetch_historical_payload_even_when_available() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let old = a.seed(b"retain only at the making device", vec![]);
    let head = a.seed(b"latest save", vec![id(&old)]);
    let results = a.service.after_snapshot(&head.game_id).await.unwrap();
    assert!(results.into_iter().all(|(_, result)| result.is_ok()));
    assert!(b
        .repository
        .records(&b.owner(), &old.game_id)
        .unwrap()
        .contains(&old));
    assert!(!b
        .repository
        .complete(&b.owner(), &old.game_id, &id(&old))
        .unwrap());
    assert!(b
        .repository
        .complete(&b.owner(), &head.game_id, &id(&head))
        .unwrap());
    assert!(!a
        .confirmations()
        .get(&id(&old))
        .is_some_and(|devices| devices.contains(&b.device())));
}

#[tokio::test]
async fn restore_fetch_gets_one_old_payload_without_changing_heads() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let old = a.seed(b"restorable old save", vec![]);
    let head = a.seed(b"current save", vec![id(&old)]);
    b.service.on_contact(&a.device()).await.unwrap();
    assert!(!b
        .repository
        .complete(&b.owner(), &old.game_id, &id(&old))
        .unwrap());
    let before = b.repository.heads(&b.owner()).unwrap();

    b.service
        .fetch_snapshot_payload(
            &old.game_id,
            &id(&old),
            tokio::time::Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap();

    assert!(b
        .repository
        .complete(&b.owner(), &old.game_id, &id(&old))
        .unwrap());
    assert!(b
        .repository
        .complete(&b.owner(), &head.game_id, &id(&head))
        .unwrap());
    assert_eq!(b.repository.heads(&b.owner()).unwrap(), before);
    assert!(b.confirmations()[&id(&old)].contains(&a.device()));
}

#[tokio::test]
async fn restore_fetch_reports_when_no_reachable_peer_has_the_payload() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let old = a.seed(b"pruned everywhere", vec![]);
    a.seed(b"retained head", vec![id(&old)]);
    prune_payload(&a, &old);
    b.service.on_contact(&a.device()).await.unwrap();

    let error = b
        .service
        .fetch_snapshot_payload(
            &old.game_id,
            &id(&old),
            tokio::time::Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, PeerSaveError::PayloadUnavailable));
    assert_eq!(error.failure().code, "SavePayloadUnavailable");
    assert!(error.failure().message.contains("reachable device"));
    assert!(!b
        .repository
        .complete(&b.owner(), &old.game_id, &id(&old))
        .unwrap());
}

#[tokio::test]
async fn restore_fetch_uses_local_content_offline_and_honors_the_deadline() {
    let a = Node::new(
        5,
        3,
        TransferBehaviour {
            delay: Duration::from_millis(200),
            ..Default::default()
        },
    )
    .await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    let old = a.seed(b"restore on demand", vec![]);
    a.service
        .fetch_snapshot_payload(
            &old.game_id,
            &id(&old),
            tokio::time::Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap();
    b.remember(&a);
    b.repository.put_record(&b.owner(), &old).unwrap();
    let result = b
        .service
        .fetch_snapshot_payload(
            &old.game_id,
            &id(&old),
            tokio::time::Instant::now() + Duration::from_millis(10),
        )
        .await;
    assert!(matches!(result, Err(PeerSaveError::Deadline)));
    assert!(!b
        .repository
        .complete(&b.owner(), &old.game_id, &id(&old))
        .unwrap());
}

#[tokio::test]
async fn missing_head_payload_still_imports_its_ancestor_metadata() {
    let a = Node::new(5, 3, TransferBehaviour::default()).await;
    let b = Node::new(6, 3, TransferBehaviour::default()).await;
    a.remember(&b);
    b.remember(&a);
    let old = a.seed(b"historical payload must not be fetched", vec![]);
    let head = a.seed(b"missing head", vec![id(&old)]);
    prune_payload(&a, &head);
    assert!(b.service.on_contact(&a.device()).await.is_err());
    let records = b.repository.records(&b.owner(), &head.game_id).unwrap();
    assert!(records.contains(&head));
    assert!(records.contains(&old));
    assert!(!b
        .repository
        .complete(&b.owner(), &old.game_id, &id(&old))
        .unwrap());
}

#[tokio::test]
async fn restore_fetch_tries_a_reachable_replica_without_waiting_for_a_slow_peer() {
    let slow = Node::new(
        5,
        3,
        TransferBehaviour {
            delay: Duration::from_secs(60),
            ..Default::default()
        },
    )
    .await;
    let local = Node::new(6, 3, TransferBehaviour::default()).await;
    let replica = Node::new(7, 3, TransferBehaviour::default()).await;
    let bytes = b"available on a different owner device";
    let old = slow.seed(bytes, vec![]);
    local.repository.put_record(&local.owner(), &old).unwrap();
    replica
        .repository
        .put_record(&replica.owner(), &old)
        .unwrap();
    replica
        .repository
        .put_object(&replica.owner(), &digest(bytes), &mut &bytes[..])
        .unwrap();
    local.remember(&slow);
    local.remember(&replica);

    local
        .service
        .fetch_snapshot_payload(
            &old.game_id,
            &id(&old),
            tokio::time::Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap();
    assert!(local
        .repository
        .complete(&local.owner(), &old.game_id, &id(&old))
        .unwrap());
    assert!(local.confirmations()[&id(&old)].contains(&replica.device()));
}
