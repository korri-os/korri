//! korrid's client for the root plugin host's admin socket.
//!
//! korrid holds no plugin authority. It forwards one typed request to
//! `korri-plugin serve-admin`, which runs the host's own checks, and returns
//! the host's answer or its refusal text unchanged.
use crate::plugin_admin::{
    PluginAdminFailure, PluginAdminRequest, PluginAdminResponse, CODE_REFUSED, CODE_UNAVAILABLE,
    MAX_MESSAGE_BYTES,
};
use crate::RpcFailure;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// These plugin requests await the root admin socket. Its transfer/stall
/// bounds, not an RPC total deadline, determine when the operation finishes.
/// Shared by the person client and native peer transport.
pub fn is_admin_request(request: &crate::RpcRequest) -> bool {
    matches!(
        request,
        crate::RpcRequest::PluginRepositories(_)
            | crate::RpcRequest::PluginRepositoryAdd(_)
            | crate::RpcRequest::PluginRepositoryRemove(_)
            | crate::RpcRequest::PluginCatalog(_)
            | crate::RpcRequest::PluginInspect(_)
            | crate::RpcRequest::PluginInstall(_)
            | crate::RpcRequest::PluginPublishers(_)
            | crate::RpcRequest::PluginTrustPublisher(_)
            | crate::RpcRequest::PluginRevokePublisher(_)
            | crate::RpcRequest::PluginArchiveBegin(_)
            | crate::RpcRequest::PluginArchiveChunk(_)
            | crate::RpcRequest::PluginArchiveFinish(_)
            | crate::RpcRequest::PluginArchiveInspect(_)
            | crate::RpcRequest::PluginArchiveInstall(_)
            | crate::RpcRequest::PluginSetEnabled(_)
    )
}

/// Set by korrid's NixOS module only when the device runs the plugin host.
pub const SOCKET_ENV: &str = "KORRID_PLUGIN_ADMIN_SOCKET";

#[derive(Clone, Debug)]
pub struct PluginAdminSocket {
    path: PathBuf,
    uploads: Arc<tokio::sync::Mutex<Uploads>>,
}

impl PluginAdminSocket {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            uploads: Arc::default(),
        }
    }

    pub fn from_env() -> Option<Self> {
        std::env::var_os(SOCKET_ENV).map(|path| Self::new(PathBuf::from(path)))
    }

    /// No deadline here: the host bounds catalog transfers (30 s, 4 MiB)
    /// and fails a stalled archive transfer, so it always answers.
    pub async fn request(
        &self,
        request: &PluginAdminRequest,
    ) -> Result<PluginAdminResponse, RpcFailure> {
        let unavailable = |message: &str| RpcFailure {
            code: CODE_UNAVAILABLE.into(),
            message: message.into(),
        };
        let bytes = serde_json::to_vec(request)
            .map_err(|_| unavailable("the plugin host request could not be encoded"))?;
        let mut stream = tokio::net::UnixStream::connect(&self.path)
            .await
            .map_err(|_| unavailable("the plugin host is unavailable"))?;
        stream
            .write_all(&bytes)
            .await
            .map_err(|_| unavailable("the plugin host request failed"))?;
        stream
            .shutdown()
            .await
            .map_err(|_| unavailable("the plugin host request failed"))?;
        let mut response = Vec::new();
        (&mut stream)
            .take(MAX_MESSAGE_BYTES as u64 + 1)
            .read_to_end(&mut response)
            .await
            .map_err(|_| unavailable("the plugin host did not answer"))?;
        if response.is_empty() {
            return Err(unavailable("the plugin host refused this connection"));
        }
        if response.len() > MAX_MESSAGE_BYTES {
            return Err(unavailable("the plugin host answer is too large"));
        }
        let response = serde_json::from_slice::<PluginAdminResponse>(&response)
            .map_err(|_| unavailable("the plugin host answer is invalid"))?;
        match response {
            PluginAdminResponse::Refused(PluginAdminFailure { code, message }) => {
                Err(RpcFailure { code, message })
            }
            other => Ok(other),
        }
    }
}

/// The plugin host is not part of this device.
pub fn not_configured() -> RpcFailure {
    RpcFailure {
        code: CODE_UNAVAILABLE.into(),
        message: "plugins are not managed on this device".into(),
    }
}

/// The host answered with the wrong response kind for the request.
pub fn wrong_response() -> RpcFailure {
    RpcFailure {
        code: CODE_REFUSED.into(),
        message: "the plugin host returned the wrong response".into(),
    }
}

use crate::plugin_admin::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const UPLOAD_IDLE: Duration = Duration::from_secs(10 * 60);
const CHUNK_BYTES: usize = 1024 * 1024;
#[derive(Default, Debug)]
struct Uploads {
    active: Option<Upload>,
}
#[derive(Debug)]
struct Upload {
    id: String,
    path: PathBuf,
    file: File,
    expected: String,
    size: u64,
    received: u64,
    digest: Sha256,
    finished: bool,
    touched: Instant,
}
impl Drop for Upload {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
impl Uploads {
    fn expire(&mut self, now: Instant) {
        if self
            .active
            .as_ref()
            .is_some_and(|u| now.duration_since(u.touched) >= UPLOAD_IDLE)
        {
            self.active = None;
        }
    }
    fn selected(&mut self, id: &str) -> Result<&mut Upload, PluginRefusal> {
        self.expire(Instant::now());
        if !self.active.as_ref().is_some_and(|u| u.id == id) {
            // A failed upload operation releases this device's sole staging slot.
            self.active = None;
            return Err(PluginRefusal::new(
                PluginRefusalCategory::UploadNotFound,
                "archive upload is missing or expired",
            ));
        }
        let upload = self.active.as_mut().unwrap();
        upload.touched = Instant::now();
        Ok(upload)
    }
}
fn upload_refusal(error: PluginRefusal) -> RpcFailure {
    RpcFailure {
        code: error.category.code().into(),
        message: error.message,
    }
}

fn upload_failure(message: &str) -> RpcFailure {
    RpcFailure {
        code: CODE_REFUSED.into(),
        message: message.into(),
    }
}

impl PluginAdminSocket {
    pub async fn begin_archive(
        &self,
        root: &Path,
        request: PluginArchiveBeginRequest,
    ) -> Result<PluginArchiveUpload, RpcFailure> {
        let mut uploads = self.uploads.lock().await;
        uploads.expire(Instant::now());
        if uploads.active.is_some() {
            return Err(upload_failure("another archive upload is in progress"));
        }
        if request.sha256.len() != 64
            || !request
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || request.size == 0
        {
            return Err(upload_failure(
                "archive needs a lowercase SHA-256 and a positive size",
            ));
        }
        let directory = root.join("plugin-archive");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)
            .map_err(|_| upload_failure("cannot create archive staging directory"))?;
        let metadata = fs::symlink_metadata(&directory)
            .map_err(|_| upload_failure("cannot inspect archive staging directory"))?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(upload_failure("archive staging directory must be private"));
        }
        let path = directory.join("archive.tar");
        // A previous korrid process cannot retain an upload. Remove only this
        // fixed private file; no caller supplies a filename or filesystem path.
        match fs::remove_file(&path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(upload_failure("cannot clear archive staging file")),
        }
        let directory_file = File::open(&directory)
            .map_err(|_| upload_failure("cannot open archive staging directory"))?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstatvfs(directory_file.as_raw_fd(), &mut stat) } != 0 {
            return Err(upload_failure("cannot measure archive free space"));
        }
        if (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64) < request.size {
            return Err(upload_failure("not enough space for this archive"));
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|_| upload_failure("cannot create archive staging file"))?;
        let id = hex::encode(rand::random::<[u8; 32]>());
        uploads.active = Some(Upload {
            id: id.clone(),
            path,
            file,
            expected: request.sha256,
            size: request.size,
            received: 0,
            digest: Sha256::new(),
            finished: false,
            touched: Instant::now(),
        });
        let shared = self.uploads.clone();
        let watched = id.clone();
        tokio::spawn(async move {
            let mut remaining = UPLOAD_IDLE;
            loop {
                tokio::time::sleep(remaining).await;
                let mut uploads = shared.lock().await;
                let now = Instant::now();
                uploads.expire(now);
                let Some(upload) = uploads.active.as_ref().filter(|u| u.id == watched) else {
                    break;
                };
                remaining = UPLOAD_IDLE.saturating_sub(now.duration_since(upload.touched));
            }
        });
        Ok(PluginArchiveUpload { upload_id: id })
    }

    pub async fn chunk_archive(
        &self,
        request: PluginArchiveChunkRequest,
    ) -> Result<PluginChangeApplied, RpcFailure> {
        let mut uploads = self.uploads.lock().await;
        let result = (|| {
            let upload = uploads
                .selected(&request.upload_id)
                .map_err(upload_refusal)?;
            if upload.finished || request.offset != upload.received {
                return Err(upload_failure("archive chunk has the wrong offset"));
            }
            let bytes = decode_chunk(&request.data)?;
            if bytes.is_empty() || bytes.len() as u64 > upload.size - upload.received {
                return Err(upload_failure("archive chunk exceeds declared size"));
            }
            upload
                .file
                .write_all(&bytes)
                .map_err(|_| upload_failure("cannot write archive chunk"))?;
            upload.digest.update(&bytes);
            upload.received += bytes.len() as u64;
            Ok(PluginChangeApplied {})
        })();
        if result.is_err() {
            uploads.active = None;
        }
        result
    }

    pub async fn finish_archive(
        &self,
        request: PluginArchiveUploadRequest,
    ) -> Result<PluginChangeApplied, RpcFailure> {
        let mut uploads = self.uploads.lock().await;
        let result = (|| {
            let upload = uploads
                .selected(&request.upload_id)
                .map_err(upload_refusal)?;
            if upload.received != upload.size
                || hex::encode(upload.digest.clone().finalize()) != upload.expected
            {
                return Err(upload_failure("archive size or SHA-256 does not match"));
            }
            upload
                .file
                .sync_all()
                .map_err(|_| upload_failure("cannot persist archive"))?;
            // Root may read this immutable staging file; korrid never modifies it
            // after finish. Reinspection and install hash the same bytes.
            upload
                .file
                .set_permissions(fs::Permissions::from_mode(0o400))
                .map_err(|_| upload_failure("cannot protect archive"))?;
            upload.finished = true;
            Ok(PluginChangeApplied {})
        })();
        if result.is_err() {
            uploads.active = None;
        }
        result
    }

    pub async fn use_archive(
        &self,
        id: &str,
        approval: Option<String>,
    ) -> Result<PluginAdminResponse, RpcFailure> {
        let mut uploads = self.uploads.lock().await;
        let result = async {
            let upload = uploads.selected(id).map_err(upload_refusal)?;
            if !upload.finished {
                return Err(upload_failure("archive upload is not finished"));
            }
            let path = upload
                .path
                .to_str()
                .ok_or_else(|| upload_failure("invalid archive staging path"))?
                .to_owned();
            let request = match &approval {
                Some(approval) => PluginAdminRequest::InstallArchive(PluginArchiveInstallRequest {
                    path,
                    approval: approval.clone(),
                }),
                None => PluginAdminRequest::InspectArchive(PluginArchiveRequest { path }),
            };
            let response = self.request(&request).await?;
            if matches!(
                (&approval, &response),
                (None, PluginAdminResponse::Inspection(_))
                    | (Some(_), PluginAdminResponse::Applied(_))
            ) {
                Ok(response)
            } else {
                Err(wrong_response())
            }
        }
        .await;
        let trust_review = approval.is_none()
            && result.as_ref().is_err_and(|error| {
                error.code == PluginRefusalCategory::PublisherNotTrusted.code()
                    || error.code == PluginRefusalCategory::PublisherKeyChanged.code()
            });
        if (result.is_err() && !trust_review) || approval.is_some() {
            uploads.active = None;
        }
        result
    }
}

/// Strict RFC 4648 decoding with a bound checked before allocation. Keeping
/// this at the transport boundary avoids an unbounded decoded JSON payload.
fn decode_chunk(encoded: &str) -> Result<Vec<u8>, RpcFailure> {
    if encoded.len() > CHUNK_BYTES.div_ceil(3) * 4 || !encoded.len().is_multiple_of(4) {
        return Err(upload_failure(
            "archive chunk exceeds 1 MiB or is invalid base64",
        ));
    }
    let mut decoded = Vec::with_capacity(encoded.len() / 4 * 3);
    let value = |b| match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    for (index, chunk) in encoded.as_bytes().chunks_exact(4).enumerate() {
        let last = (index + 1) * 4 == encoded.len();
        let a = value(chunk[0]).ok_or_else(|| upload_failure("invalid archive base64"))?;
        let b = value(chunk[1]).ok_or_else(|| upload_failure("invalid archive base64"))?;
        decoded.push((a << 2) | (b >> 4));
        if chunk[2] == b'=' && chunk[3] == b'=' && last && b & 15 == 0 {
            continue;
        }
        let c = value(chunk[2]).ok_or_else(|| upload_failure("invalid archive base64"))?;
        decoded.push((b << 4) | (c >> 2));
        if chunk[3] == b'=' && last && c & 3 == 0 {
            continue;
        }
        let d = value(chunk[3]).ok_or_else(|| upload_failure("invalid archive base64"))?;
        decoded.push((c << 6) | d);
    }
    if decoded.len() > CHUNK_BYTES {
        return Err(upload_failure("archive chunk exceeds 1 MiB"));
    }
    Ok(decoded)
}

#[cfg(test)]
mod archive_tests {
    use super::*;
    fn begin(bytes: &[u8]) -> PluginArchiveBeginRequest {
        PluginArchiveBeginRequest {
            sha256: hex::encode(Sha256::digest(bytes)),
            size: bytes.len() as u64,
        }
    }
    #[tokio::test]
    async fn begin_clears_crash_left_staging_before_a_free_space_refusal() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("plugin-archive");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let archive = directory.join("archive.tar");
        fs::write(&archive, b"stale").unwrap();
        let socket = PluginAdminSocket::new(root.path().join("unused"));
        let mut request = begin(b"abc");
        request.size = u64::MAX;
        assert!(socket.begin_archive(root.path(), request).await.is_err());
        assert!(!archive.exists());
        assert!(socket
            .begin_archive(root.path(), begin(b"abc"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn wrong_offset_and_hash_mismatch_delete_staging_and_release_slot() {
        let root = tempfile::tempdir().unwrap();
        let socket = PluginAdminSocket::new(root.path().join("unused"));
        let upload = socket
            .begin_archive(root.path(), begin(b"abc"))
            .await
            .unwrap();
        assert!(socket
            .chunk_archive(PluginArchiveChunkRequest {
                upload_id: upload.upload_id,
                offset: 1,
                data: "YWJj".into()
            })
            .await
            .is_err());
        assert!(!root.path().join("plugin-archive/archive.tar").exists());
        let upload = socket
            .begin_archive(root.path(), begin(b"abc"))
            .await
            .unwrap();
        assert!(socket
            .begin_archive(root.path(), begin(b"abc"))
            .await
            .is_err());
        socket
            .chunk_archive(PluginArchiveChunkRequest {
                upload_id: upload.upload_id.clone(),
                offset: 0,
                data: "YWJk".into(),
            })
            .await
            .unwrap();
        assert!(socket
            .finish_archive(PluginArchiveUploadRequest {
                upload_id: upload.upload_id
            })
            .await
            .is_err());
        assert!(!root.path().join("plugin-archive/archive.tar").exists());
        assert!(socket
            .begin_archive(root.path(), begin(b"abc"))
            .await
            .is_ok());
    }
    #[tokio::test]
    async fn expiry_cleans_finished_and_partial_uploads_and_failures_clean_finished_files() {
        let root = tempfile::tempdir().unwrap();
        let socket = PluginAdminSocket::new(root.path().join("missing.sock"));
        let upload = socket
            .begin_archive(root.path(), begin(b"abc"))
            .await
            .unwrap();
        socket.uploads.lock().await.active.as_mut().unwrap().touched -= UPLOAD_IDLE;
        assert_eq!(
            socket
                .finish_archive(PluginArchiveUploadRequest {
                    upload_id: upload.upload_id
                })
                .await
                .unwrap_err()
                .code,
            CODE_UPLOAD_NOT_FOUND
        );
        assert!(!root.path().join("plugin-archive/archive.tar").exists());
        let upload = socket
            .begin_archive(root.path(), begin(b"abc"))
            .await
            .unwrap();
        socket
            .chunk_archive(PluginArchiveChunkRequest {
                upload_id: upload.upload_id.clone(),
                offset: 0,
                data: "YWJj".into(),
            })
            .await
            .unwrap();
        socket
            .finish_archive(PluginArchiveUploadRequest {
                upload_id: upload.upload_id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(
            fs::read(root.path().join("plugin-archive/archive.tar")).unwrap(),
            b"abc"
        );
        assert!(socket.use_archive(&upload.upload_id, None).await.is_err());
        assert!(!root.path().join("plugin-archive/archive.tar").exists());
    }
    #[tokio::test]
    async fn trust_refusal_keeps_the_upload_for_trust_reinspect_and_install() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("admin.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let mut trusted = false;
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                stream.read_to_end(&mut bytes).await.unwrap();
                let request: PluginAdminRequest = serde_json::from_slice(&bytes).unwrap();
                let response = match request {
                    PluginAdminRequest::InspectArchive(request) if !trusted => {
                        assert_eq!(fs::read(request.path).unwrap(), b"abc");
                        PluginAdminResponse::Refused(PluginAdminFailure {
                            code: CODE_PUBLISHER_NOT_TRUSTED.into(),
                            message: "publisher @test is not trusted".into(),
                        })
                    }
                    PluginAdminRequest::TrustPublisher(_) => {
                        trusted = true;
                        PluginAdminResponse::Applied(PluginChangeApplied {})
                    }
                    PluginAdminRequest::InspectArchive(request) => {
                        assert_eq!(fs::read(request.path).unwrap(), b"abc");
                        PluginAdminResponse::Inspection(PluginInspection {
                            publisher: PluginPublisher {
                                namespace: "@test".into(),
                                state: PluginPublisherState::Trusted,
                                fingerprint: None,
                                previous_fingerprint: None,
                                disclosure: None,
                                public_key: None,
                            },
                            id: "@test:a".into(),
                            title: None,
                            approval: "digest".into(),
                            warning: String::new(),
                            ports: PluginPorts {
                                allowed_tcp_ports: vec![],
                                allowed_udp_ports: vec![],
                            },
                            native_units: vec![],
                            brings: vec![],
                        })
                    }
                    PluginAdminRequest::InstallArchive(request) => {
                        assert_eq!(request.approval, "digest");
                        assert_eq!(fs::read(request.path).unwrap(), b"abc");
                        PluginAdminResponse::Applied(PluginChangeApplied {})
                    }
                    _ => panic!("unexpected request"),
                };
                stream
                    .write_all(&serde_json::to_vec(&response).unwrap())
                    .await
                    .unwrap();
            }
        });
        let socket = PluginAdminSocket::new(path);
        let upload = socket
            .begin_archive(root.path(), begin(b"abc"))
            .await
            .unwrap();
        socket
            .chunk_archive(PluginArchiveChunkRequest {
                upload_id: upload.upload_id.clone(),
                offset: 0,
                data: "YWJj".into(),
            })
            .await
            .unwrap();
        socket
            .finish_archive(PluginArchiveUploadRequest {
                upload_id: upload.upload_id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(
            socket
                .use_archive(&upload.upload_id, None)
                .await
                .unwrap_err()
                .code,
            CODE_PUBLISHER_NOT_TRUSTED
        );
        assert!(root.path().join("plugin-archive/archive.tar").exists());
        socket
            .request(&PluginAdminRequest::TrustPublisher(
                PluginTrustPublisherRequest {
                    namespace: "@test".into(),
                    public_key: "label:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
                },
            ))
            .await
            .unwrap();
        assert!(matches!(
            socket.use_archive(&upload.upload_id, None).await.unwrap(),
            PluginAdminResponse::Inspection(_)
        ));
        socket
            .use_archive(&upload.upload_id, Some("digest".into()))
            .await
            .unwrap();
        assert!(!root.path().join("plugin-archive/archive.tar").exists());
        assert_eq!(
            socket
                .use_archive(&upload.upload_id, None)
                .await
                .unwrap_err()
                .code,
            CODE_UPLOAD_NOT_FOUND
        );
        server.await.unwrap();
    }

    #[test]
    fn missing_upload_has_a_typed_category_before_wire_encoding() {
        let mut uploads = Uploads::default();
        let error = uploads.selected("unknown").unwrap_err();
        assert_eq!(error.category, PluginRefusalCategory::UploadNotFound);
        let wire = upload_refusal(PluginRefusal::new(error.category, "different diagnostic"));
        assert_eq!(wire.code, CODE_UPLOAD_NOT_FOUND);
        assert_eq!(wire.message, "different diagnostic");
    }

    #[test]
    fn chunks_are_strict_base64_and_bounded_before_decode() {
        for (encoded, plain) in [("YQ==", "a"), ("YWI=", "ab"), ("YWJj", "abc")] {
            assert_eq!(decode_chunk(encoded).unwrap(), plain.as_bytes());
        }
        for bad in ["YQ=", "YQ==YQ==", "YR==", "YWJ=", "!!!!"] {
            assert!(decode_chunk(bad).is_err(), "{bad}");
        }
        assert!(decode_chunk(&"A".repeat(CHUNK_BYTES.div_ceil(3) * 4 + 4)).is_err());
    }
}
