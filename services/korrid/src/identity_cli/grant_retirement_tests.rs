//! The real CLI/method, producer grant writer, and Unix seqpacket transport.
use super::{tests::bind_seqpacket, SocketCertificateAdapter, MAX_FRAME_BYTES};
use crate::{
    authorization::{Authorization, AuthorizationContext},
    identity::{DeviceIdentity, IdentityState},
    identity_cli,
};
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::{Keys, SecretKey},
    types::Timestamp,
};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::Cursor,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::{symlink, MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    thread,
};

const NOW: u64 = 300;
const HOST: &str = "sunshine-host";
const CERTIFICATE: &str =
    "-----BEGIN CERTIFICATE-----\nclient-private-fixture\n-----END CERTIFICATE-----\n";

fn keys(number: u8) -> Keys {
    Keys::new(SecretKey::from_hex(&format!("{number:064x}")).unwrap())
}

fn statement(owner: u8, device: &str, status: &str) -> String {
    EventBuilder::new(Kind::Custom(30_078), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
            Tag::parse(["device", device]).unwrap(),
            Tag::parse(["status", status]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(100))
        .finalize(&keys(owner))
        .unwrap()
        .as_json()
}

struct Fixture {
    root: tempfile::TempDir,
    args: Vec<OsString>,
    preserved: Vec<(PathBuf, Vec<u8>, fs::Metadata)>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let mut identity = DeviceIdentity::load_or_create(root.path()).unwrap();
        let device = identity.device_public_key().unwrap().to_owned();
        let event = statement(4, &device, "owned");
        identity.apply_owner_statement(&event).unwrap();
        let state = identity.state().clone();
        let authorization = Authorization::load(root.path()).unwrap();
        let revocation = statement(3, &keys(9).public_key().to_hex(), "revoked");
        let attempt = authorization
            .attempt(&state, &device, Some(&event), None, &[revocation], NOW)
            .unwrap();
        authorization.commit_revocations(&attempt).unwrap();
        // This unrelated fixture stands in an existing replay/evidence path. The
        // command must not enumerate or change any other identity files.
        let evidence = root.path().join("identity/unrelated-evidence");
        fs::write(&evidence, b"preserve replay and recovery evidence").unwrap();
        fs::set_permissions(&evidence, fs::Permissions::from_mode(0o600)).unwrap();
        // Existing ReplayGuard::consume writes an empty file named by the
        // SHA-256 of sender:nonce. Preserve that real on-disk replay marker too.
        use sha2::{Digest, Sha256};
        let replay_directory = root.path().join("identity/security-replay");
        fs::create_dir(&replay_directory).unwrap();
        fs::set_permissions(&replay_directory, fs::Permissions::from_mode(0o700)).unwrap();
        let replay = replay_directory.join(hex::encode(Sha256::digest(
            format!("{}:{}", keys(6).public_key().to_hex(), "11".repeat(32)).as_bytes(),
        )));
        fs::write(&replay, []).unwrap();
        fs::set_permissions(&replay, fs::Permissions::from_mode(0o600)).unwrap();
        let mut paths = vec![
            root.path().join("identity/device.key"),
            root.path().join("identity/owner.event.json"),
            evidence,
            replay,
        ];
        paths.extend(
            fs::read_dir(root.path().join("identity/authorization-revocations"))
                .unwrap()
                .map(|entry| entry.unwrap().path()),
        );
        let preserved = paths
            .into_iter()
            .map(|path| {
                let bytes = fs::read(&path).unwrap();
                let metadata = fs::metadata(&path).unwrap();
                (path, bytes, metadata)
            })
            .collect();
        Self {
            args: vec![
                "reconcile-stale-grants-offline".into(),
                "--expected-device".into(),
                device.into(),
                "--expected-owner".into(),
                keys(4).public_key().to_hex().into(),
                "--expected-event".into(),
                DeviceIdentity::verify_event(&event).unwrap().id.into(),
            ],
            root,
            preserved,
        }
    }

    fn grant(&self, device: u8, owner: u8, certificate: &str) -> PathBuf {
        let authorization = Authorization::load(self.root.path()).unwrap();
        let device = keys(device).public_key().to_hex();
        let owner_statement = statement(owner, &device, "owned");
        let local = IdentityState::Owned {
            device_public_key: keys(5).public_key().to_hex(),
            owner_public_key: keys(owner).public_key().to_hex(),
            event_id: "00".repeat(32),
            created_at: 100,
        };
        let attempt = authorization
            .attempt(&local, &device, Some(&owner_statement), None, &[], NOW)
            .unwrap();
        let AuthorizationContext::Peer(principal) = attempt.context() else {
            panic!("peer principal")
        };
        authorization
            .record_certificate(principal, HOST, certificate)
            .unwrap();
        self.root
            .path()
            .join("identity/peer-certificates")
            .join(device)
    }

    fn pass_grant(&self, owner: u8, expires_at: u64) -> PathBuf {
        use crate::authorization::{PERSON_PASS_EVENT_KIND, STREAM_LAUNCH_SCOPE};
        let authorization = Authorization::load(self.root.path()).unwrap();
        let device = keys(6).public_key().to_hex();
        let statement = statement(8, &device, "owned");
        let pass = EventBuilder::new(Kind::Custom(PERSON_PASS_EVENT_KIND), "")
            .tags([
                Tag::parse([
                    "d",
                    &format!("org.korri.person-pass:{}", keys(8).public_key().to_hex()),
                ])
                .unwrap(),
                Tag::parse(["device", &device]).unwrap(),
                Tag::parse(["tier", "guest"]).unwrap(),
                Tag::parse(["expires", &expires_at.to_string()]).unwrap(),
                Tag::parse(["scope", STREAM_LAUNCH_SCOPE]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(100))
            .finalize(&keys(owner))
            .unwrap()
            .as_json();
        let local = IdentityState::Owned {
            device_public_key: keys(5).public_key().to_hex(),
            owner_public_key: keys(owner).public_key().to_hex(),
            event_id: "00".repeat(32),
            created_at: 100,
        };
        let attempt = authorization
            .attempt(&local, &device, Some(&statement), Some(&pass), &[], 150)
            .unwrap();
        let AuthorizationContext::Peer(principal) = attempt.context() else {
            panic!("peer principal")
        };
        authorization
            .record_certificate(principal, HOST, CERTIFICATE)
            .unwrap();
        self.root
            .path()
            .join("identity/peer-certificates")
            .join(device)
    }

    fn run(&self, adapter: &SocketCertificateAdapter) -> Result<String, String> {
        identity_cli::grant_retirement::reconcile(
            self.root.path(),
            self.args[2].to_str().unwrap(),
            self.args[4].to_str().unwrap(),
            self.args[6].to_str().unwrap(),
            NOW,
            adapter,
        )
    }

    fn assert_preserved(&self) {
        for (path, bytes, before) in &self.preserved {
            assert!(
                fs::read(path).unwrap() == *bytes,
                "preserve {}",
                path.display()
            );
            let after = fs::metadata(path).unwrap();
            let fields = |m: &fs::Metadata| {
                (
                    m.dev(),
                    m.ino(),
                    m.uid(),
                    m.gid(),
                    m.mode(),
                    m.nlink(),
                    m.mtime(),
                    m.mtime_nsec(),
                    m.ctime(),
                    m.ctime_nsec(),
                )
            };
            assert_eq!(
                fields(&after),
                fields(before),
                "preserve metadata {}",
                path.display()
            );
        }
    }
}

fn adapter(path: &Path) -> SocketCertificateAdapter {
    SocketCertificateAdapter::for_test(
        path.into(),
        unsafe { libc::geteuid() },
        unsafe { libc::getegid() },
        unsafe { libc::geteuid() },
        unsafe { libc::getegid() },
        0o660,
    )
}

// Configured replies use the existing socket response contract. Poll bounds a
// missing call; accepting and recording actual frames proves the revoke order.
enum Reply {
    Changed(bool),
    PersistenceFailed,
    BlockGrantDeletion,
    Close,
}

fn serve(
    path: &Path,
    outcomes: Vec<Reply>,
    grants: Vec<PathBuf>,
) -> thread::JoinHandle<Vec<serde_json::Value>> {
    let listener = bind_seqpacket(path);
    fs::set_permissions(path, fs::Permissions::from_mode(0o660)).unwrap();
    thread::spawn(move || {
        let mut requests = vec![];
        for (outcome, grant) in outcomes.into_iter().zip(grants) {
            let mut descriptor = libc::pollfd {
                fd: listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut descriptor, 1, 2000) };
            assert!(ready >= 0, "poll certificate requests");
            if ready == 0 {
                break;
            }
            let fd = unsafe {
                libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                )
            };
            assert!(fd >= 0);
            let accepted = unsafe { OwnedFd::from_raw_fd(fd) };
            let mut bytes = [0_u8; MAX_FRAME_BYTES];
            let size = unsafe {
                libc::recv(
                    accepted.as_raw_fd(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                    0,
                )
            };
            assert!(size > 0);
            assert!(
                grant.exists(),
                "never delete a grant before the producer acknowledges durable revoke"
            );
            requests.push(serde_json::from_slice(&bytes[..size as usize]).unwrap());
            let response = match outcome {
                Reply::Changed(changed) => serde_json::json!({"status":"ok", "changed":changed, "serverCertificate":CERTIFICATE}),
                Reply::PersistenceFailed => serde_json::json!({"status":"error", "code":"PersistenceFailed"}),
                Reply::BlockGrantDeletion => {
                    fs::set_permissions(grant.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
                    serde_json::json!({"status":"ok", "changed":false, "serverCertificate":CERTIFICATE})
                }
                Reply::Close => continue,
            }.to_string();
            assert_eq!(
                unsafe {
                    libc::send(
                        accepted.as_raw_fd(),
                        response.as_ptr().cast(),
                        response.len(),
                        libc::MSG_NOSIGNAL,
                    )
                },
                response.len() as isize
            );
        }
        requests
    })
}

#[test]
fn grant_retirement_cli_recognizes_exact_evidence_and_empty_retry_without_a_socket() {
    let fixture = Fixture::new();
    let output = identity_cli::run(
        &fixture.args,
        fixture.root.path(),
        NOW,
        &mut Cursor::new([]),
    )
    .unwrap();
    assert_eq!(
        output,
        "Reconciled 0 stale client grants; remaining grants: 0."
    );
    fixture.assert_preserved();
}

#[test]
fn grant_retirement_reconciles_exact_certificate_after_bulk_erase_and_retries_empty() {
    for changed in [true, false] {
        let fixture = Fixture::new();
        let grant = fixture.grant(6, 3, CERTIFICATE);
        let path = fixture.root.path().join("certificate.sock");
        let server = serve(&path, vec![Reply::Changed(changed)], vec![grant.clone()]);
        let output = fixture.run(&adapter(&path)).unwrap();
        let requests = server.join().unwrap();
        assert_eq!(
            requests,
            vec![
                serde_json::json!({"operation":"revoke", "hostUuid":HOST, "certificate":CERTIFICATE})
            ]
        );
        assert_eq!(
            output,
            "Reconciled 1 stale client grants; remaining grants: 0."
        );
        assert!(!grant.exists());
        assert_eq!(
            fixture.run(&adapter(&path)).unwrap(),
            "Reconciled 0 stale client grants; remaining grants: 0."
        );
        fixture.assert_preserved();
    }
}

#[test]
fn grant_retirement_uses_the_existing_pass_parser_for_expired_and_old_owner_grants() {
    for (owner, expires_at) in [(3, 400), (3, 200), (4, 200)] {
        let fixture = Fixture::new();
        let grant = fixture.pass_grant(owner, expires_at);
        let path = fixture.root.path().join("pass.sock");
        let server = serve(&path, vec![Reply::Changed(false)], vec![grant.clone()]);
        fixture.run(&adapter(&path)).unwrap();
        assert_eq!(server.join().unwrap().len(), 1);
        assert!(!grant.exists());
        fixture.assert_preserved();
    }
    let fixture = Fixture::new();
    let grant = fixture.pass_grant(4, 400);
    let error = fixture
        .run(&adapter(&fixture.root.path().join("absent.sock")))
        .unwrap_err();
    assert!(error.contains("authorization evidence is invalid"));
    assert!(
        grant.exists(),
        "a still-authorized new-owner pass blocks acceptance"
    );
}

#[test]
fn grant_retirement_partial_failure_keeps_failed_and_unattempted_grants_for_retry() {
    let fixture = Fixture::new();
    let mut grants: Vec<_> = [6, 7, 8]
        .into_iter()
        .map(|device| fixture.grant(device, 3, CERTIFICATE))
        .collect();
    grants.sort();
    let path = fixture.root.path().join("first.sock");
    let server = serve(
        &path,
        vec![Reply::Changed(false), Reply::PersistenceFailed],
        grants[..2].to_vec(),
    );
    let error = fixture.run(&adapter(&path)).unwrap_err();
    assert!(error.contains("StreamCertificateControlPersistenceFailed"));
    assert!(!error.contains(CERTIFICATE));
    assert_eq!(server.join().unwrap().len(), 2);
    assert!(!grants[0].exists());
    assert!(grants[1..].iter().all(|grant| grant.exists()));
    let retry_path = fixture.root.path().join("retry.sock");
    let retry = serve(
        &retry_path,
        vec![Reply::Changed(false), Reply::Changed(false)],
        grants[1..].to_vec(),
    );
    assert_eq!(
        fixture.run(&adapter(&retry_path)).unwrap(),
        "Reconciled 2 stale client grants; remaining grants: 0."
    );
    assert_eq!(retry.join().unwrap().len(), 2);
    assert!(grants.iter().all(|grant| !grant.exists()));
    fixture.assert_preserved();
}

#[test]
fn grant_retirement_retains_grant_after_lost_reply_or_failed_delete_then_retries_absence() {
    for reply in [Reply::Close, Reply::BlockGrantDeletion] {
        let fixture = Fixture::new();
        let grant = fixture.grant(6, 3, CERTIFICATE);
        let path = fixture.root.path().join("interrupted.sock");
        let server = serve(&path, vec![reply], vec![grant.clone()]);
        assert!(fixture.run(&adapter(&path)).is_err());
        assert_eq!(server.join().unwrap().len(), 1);
        assert!(grant.exists());
        fs::set_permissions(grant.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = fixture.root.path().join("recovered.sock");
        let server = serve(&path, vec![Reply::Changed(false)], vec![grant.clone()]);
        fixture.run(&adapter(&path)).unwrap();
        assert_eq!(server.join().unwrap().len(), 1);
        assert!(!grant.exists());
        fixture.assert_preserved();
    }
}

#[test]
fn grant_retirement_rejects_wrong_socket_peer_and_sends_no_frame() {
    let fixture = Fixture::new();
    let grant = fixture.grant(6, 3, CERTIFICATE);
    let path = fixture.root.path().join("wrong-peer.sock");
    let listener = bind_seqpacket(&path);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
    let mut adapter = adapter(&path);
    adapter.expected_peer_uid = unsafe { libc::geteuid() }.wrapping_add(1);
    let error = fixture.run(&adapter).unwrap_err();
    assert!(error.contains("StreamCertificateControlInvalid"));
    assert!(grant.exists());
    let accepted = unsafe {
        libc::accept4(
            listener.as_raw_fd(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
        )
    };
    assert!(accepted >= 0);
    let accepted = unsafe { OwnedFd::from_raw_fd(accepted) };
    let mut byte = [0_u8; 1];
    assert_eq!(
        unsafe { libc::recv(accepted.as_raw_fd(), byte.as_mut_ptr().cast(), 1, 0) },
        0
    );
    fixture.assert_preserved();
}

#[test]
fn grant_retirement_refuses_entire_inventory_before_any_revoke() {
    for problem in [
        "authorized",
        "malformed-json",
        "signature",
        "bad-pem",
        "symlink",
        "hardlink",
        "public",
        "temporary",
        "oversized",
        "revocation",
    ] {
        let fixture = Fixture::new();
        let first = fixture.grant(7, 3, CERTIFICATE);
        let second = fixture.grant(
            6,
            if problem == "authorized" { 4 } else { 3 },
            if problem == "bad-pem" {
                "not a certificate"
            } else {
                CERTIFICATE
            },
        );
        assert!(
            first < second,
            "a valid stale grant must precede malformed state"
        );
        match problem {
            "malformed-json" => fs::write(&second, b"{").unwrap(),
            "signature" => {
                let bytes = fs::read_to_string(&second).unwrap();
                let signature = DeviceIdentity::verify_event(&statement(
                    3,
                    &keys(6).public_key().to_hex(),
                    "owned",
                ))
                .unwrap();
                // Corrupt the producer's signed evidence without inventing a grant shape.
                fs::write(&second, bytes.replace(&signature.id, &"00".repeat(32))).unwrap();
            }
            "symlink" => {
                fs::remove_file(&second).unwrap();
                symlink(&first, &second).unwrap();
            }
            "hardlink" => fs::hard_link(&second, fixture.root.path().join("extra-link")).unwrap(),
            "public" => fs::set_permissions(&second, fs::Permissions::from_mode(0o644)).unwrap(),
            "temporary" => {
                fs::rename(&second, second.with_file_name(".unfinished.tmp")).unwrap();
            }
            "oversized" => fs::write(&second, vec![b' '; 128 * 1024 + 1]).unwrap(),
            "revocation" => fs::write(fixture.preserved.last().unwrap().0.clone(), b"{").unwrap(),
            "authorized" | "bad-pem" => (),
            _ => unreachable!(),
        }
        let path = fixture.root.path().join("no-revokes.sock");
        let listener = bind_seqpacket(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
        let error = fixture.run(&adapter(&path)).expect_err(problem);
        assert!(!error.contains(CERTIFICATE));
        assert!(first.exists());
        let mut descriptor = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(
            unsafe { libc::poll(&mut descriptor, 1, 0) },
            0,
            "{problem} must fail before connecting"
        );
    }
}

#[test]
fn grant_retirement_rejects_encoded_frame_expansion_before_any_mutation() {
    let mut fixture = Fixture::new();
    let expanded_certificate = format!("{CERTIFICATE}{}", "\n".repeat(9000));
    super::validate_single_pem(&expanded_certificate).unwrap();
    let encoded = serde_json::to_vec(&super::SocketRequest {
        operation: super::SocketOperation::Revoke,
        host_uuid: HOST,
        certificate: Some(&expanded_certificate),
    })
    .unwrap();
    assert!(encoded.len() > MAX_FRAME_BYTES);
    let first = fixture.grant(7, 3, CERTIFICATE);
    let second = fixture.grant(6, 3, &expanded_certificate);
    assert!(first < second, "the oversized revoke must sort second");
    for grant in [&first, &second] {
        fixture.preserved.push((
            grant.clone(),
            fs::read(grant).unwrap(),
            fs::metadata(grant).unwrap(),
        ));
    }
    let path = fixture.root.path().join("frame-expansion.sock");
    // A reachable producer acknowledges any first revoke so the regression
    // exposes avoidable deletion, not just a connection or timeout failure.
    let server = serve(&path, vec![Reply::Changed(false)], vec![first.clone()]);
    let error = fixture.run(&adapter(&path)).unwrap_err();
    let connections = server.join().unwrap().len();
    assert!(error.contains("StreamCertificateControlInvalid"));
    assert!(!error.contains(CERTIFICATE));
    assert_eq!(
        (connections, first.exists(), second.exists()),
        (0, true, true),
        "preflight must reject the entire plan before any connection or grant deletion"
    );
    fixture.assert_preserved();
}

#[test]
fn grant_retirement_rejects_test_owner_revoked_or_unsafe_identity_before_the_adapter() {
    for (owner, status) in [(3, "owned"), (4, "revoked")] {
        let mut fixture = Fixture::new();
        let grant = fixture.grant(6, 3, CERTIFICATE);
        let event = statement(owner, fixture.args[2].to_str().unwrap(), status);
        fs::write(
            fixture.root.path().join("identity/owner.event.json"),
            &event,
        )
        .unwrap();
        fixture.args[4] = keys(owner).public_key().to_hex().into();
        fixture.args[6] = DeviceIdentity::verify_event(&event).unwrap().id.into();
        let error = fixture
            .run(&adapter(&fixture.root.path().join("absent.sock")))
            .unwrap_err();
        assert!(
            !error.contains("Sunshine"),
            "identity must fail before transport"
        );
        assert!(grant.exists());
    }
    for problem in [
        "public-root",
        "public-identity",
        "public-key",
        "invalid-key",
        "key-link",
        "owner-hardlink",
        "invalid-owner",
    ] {
        let fixture = Fixture::new();
        let grant = fixture.grant(6, 3, CERTIFICATE);
        let key = fixture.root.path().join("identity/device.key");
        let owner = fixture.root.path().join("identity/owner.event.json");
        match problem {
            "public-root" => {
                fs::set_permissions(fixture.root.path(), fs::Permissions::from_mode(0o755)).unwrap()
            }
            "public-identity" => {
                fs::set_permissions(key.parent().unwrap(), fs::Permissions::from_mode(0o755))
                    .unwrap()
            }
            "public-key" => fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap(),
            "invalid-key" => fs::write(&key, b"not a device key").unwrap(),
            "key-link" => {
                let target = fixture.root.path().join("key-target");
                fs::rename(&key, &target).unwrap();
                symlink(target, &key).unwrap();
            }
            "owner-hardlink" => {
                fs::hard_link(&owner, fixture.root.path().join("owner-link")).unwrap()
            }
            "invalid-owner" => fs::write(&owner, b"{").unwrap(),
            _ => unreachable!(),
        }
        let error = fixture
            .run(&adapter(&fixture.root.path().join("absent.sock")))
            .unwrap_err();
        assert!(
            !error.contains("Sunshine"),
            "{problem} must fail before transport"
        );
        assert!(grant.exists());
    }
}

#[test]
fn grant_retirement_rejects_wrong_evidence_missing_key_and_competing_lock_without_creating_state() {
    for index in [2, 4, 6] {
        let mut fixture = Fixture::new();
        fixture.args[index] = if index == 6 {
            "00".repeat(32)
        } else {
            keys(8).public_key().to_hex()
        }
        .into();
        let error = identity_cli::run(
            &fixture.args,
            fixture.root.path(),
            NOW,
            &mut Cursor::new([]),
        )
        .unwrap_err();
        assert!(!error.starts_with("usage:"));
        fixture.assert_preserved();
    }
    for missing in [
        "identity/device.key",
        "identity/owner.event.json",
        "identity/peer-certificates",
        "identity/authorization-revocations",
    ] {
        let fixture = Fixture::new();
        let path = fixture.root.path().join(missing);
        if path.is_dir() {
            fs::remove_dir_all(&path).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        assert!(fixture
            .run(&adapter(&fixture.root.path().join("absent.sock")))
            .is_err());
        assert!(!path.exists());
    }
    let fixture = Fixture::new();
    let missing = fixture.root.path().join("missing-root");
    assert!(identity_cli::run(&fixture.args, &missing, NOW, &mut Cursor::new([])).is_err());
    assert!(!missing.exists());
    let directory = File::open(fixture.root.path().join("identity")).unwrap();
    assert_eq!(
        unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    assert!(fixture
        .run(&adapter(&fixture.root.path().join("absent.sock")))
        .is_err());
    fixture.assert_preserved();
}
