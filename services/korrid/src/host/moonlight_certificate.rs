use crate::{MoonlightCertificateProvisioned, RpcFailure};
use serde::{Deserialize, Serialize};
use std::{
    ffi::CString,
    fs, io, mem,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, fs::FileTypeExt, fs::MetadataExt, fs::PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const MAX_FRAME_BYTES: usize = 16_384;
const MAX_CERTIFICATE_BYTES: usize = 12_288;
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1_500);
const SOCKET_ENV: &str = "KORRID_SUNSHINE_CERTIFICATE_CONTROL_SOCKET";
const SOCKET_GID_ENV: &str = "KORRID_SUNSHINE_CERTIFICATE_CONTROL_GID";
const SOCKET_PEER_UID_ENV: &str = "KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_UID";
const SOCKET_PEER_GID_ENV: &str = "KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_GID";
const SOCKET_MODE: u32 = 0o660;

pub trait MoonlightCertificateAdapter: Send + Sync {
    /// Bounded readiness probe. Returns true only when the protected control
    /// channel passes every path, credential, mode, connect, and peer check.
    /// It never sends a certificate operation and never mutates state.
    fn available(&self) -> bool;
    fn attest(&self, host_uuid: &str) -> Result<bool, RpcFailure>;
    fn provision(
        &self,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<MoonlightCertificateProvisioned, RpcFailure>;
    fn revoke(&self, host_uuid: &str, client_certificate: &str) -> Result<bool, RpcFailure>;
}

pub fn production_adapter() -> Arc<dyn MoonlightCertificateAdapter> {
    match SocketCertificateAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter),
        Err(failure) => Arc::new(UnavailableCertificateAdapter { failure }),
    }
}

#[derive(Clone)]
struct UnavailableCertificateAdapter {
    failure: RpcFailure,
}

impl MoonlightCertificateAdapter for UnavailableCertificateAdapter {
    fn available(&self) -> bool {
        false
    }

    fn attest(&self, _host_uuid: &str) -> Result<bool, RpcFailure> {
        Err(self.failure.clone())
    }

    fn provision(
        &self,
        _host_uuid: &str,
        _client_certificate: &str,
    ) -> Result<MoonlightCertificateProvisioned, RpcFailure> {
        Err(self.failure.clone())
    }

    fn revoke(&self, _host_uuid: &str, _client_certificate: &str) -> Result<bool, RpcFailure> {
        Err(self.failure.clone())
    }
}

#[derive(Clone)]
struct SocketCertificateAdapter {
    path: PathBuf,
    expected_path_uid: u32,
    expected_path_gid: u32,
    expected_peer_uid: u32,
    expected_peer_gid: u32,
    expected_mode: u32,
    timeout: Duration,
}

impl SocketCertificateAdapter {
    fn from_env() -> Result<Self, RpcFailure> {
        let path = std::env::var_os(SOCKET_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                failure(
                    "SunshineCertificateControlUnavailable",
                    "Sunshine certificate control socket is not configured",
                )
            })?;
        let expected_path_gid = std::env::var(SOCKET_GID_ENV)
            .map_err(|_| {
                failure(
                    "SunshineCertificateControlInvalid",
                    "Sunshine certificate control socket group is not configured",
                )
            })?
            .parse::<u32>()
            .map_err(|_| {
                failure(
                    "SunshineCertificateControlInvalid",
                    "Sunshine certificate control socket group is invalid",
                )
            })?;
        let expected_peer_uid = required_id_env(SOCKET_PEER_UID_ENV, "peer user")?;
        let expected_peer_gid = required_id_env(SOCKET_PEER_GID_ENV, "peer group")?;
        Ok(Self {
            path,
            expected_path_uid: 0,
            expected_path_gid,
            expected_peer_uid,
            expected_peer_gid,
            expected_mode: SOCKET_MODE,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    #[cfg(test)]
    fn for_test(
        path: PathBuf,
        expected_path_uid: u32,
        expected_path_gid: u32,
        expected_peer_uid: u32,
        expected_peer_gid: u32,
        expected_mode: u32,
    ) -> Self {
        Self {
            path,
            expected_path_uid,
            expected_path_gid,
            expected_peer_uid,
            expected_peer_gid,
            expected_mode,
            timeout: Duration::from_millis(500),
        }
    }

    fn request(&self, request: SocketRequest<'_>) -> Result<SocketSuccess, RpcFailure> {
        let socket_identity = self.validate_path()?;
        let encoded = serde_json::to_vec(&request).map_err(|_| {
            failure(
                "SunshineCertificateControlInvalid",
                "Sunshine certificate control request could not be encoded",
            )
        })?;
        if encoded.len() > MAX_FRAME_BYTES {
            return Err(failure(
                "SunshineCertificateControlInvalid",
                "Sunshine certificate control request is too large",
            ));
        }
        let response = send_seqpacket(
            &self.path,
            socket_identity,
            self.expected_peer_uid,
            self.expected_peer_gid,
            &encoded,
            self.timeout,
        )
        .map_err(socket_failure)?;
        let decoded: SocketResponse = serde_json::from_slice(&response).map_err(|_| {
            failure(
                "SunshineCertificateControlProtocol",
                "Sunshine certificate control returned an invalid response",
            )
        })?;
        decoded.into_result(request.operation)
    }

    fn validate_path(&self) -> Result<SocketIdentity, RpcFailure> {
        if !self.path.is_absolute() {
            return Err(path_failure());
        }
        let parent = self.path.parent().ok_or_else(path_failure)?;
        let parent_metadata = fs::symlink_metadata(parent).map_err(|_| path_failure())?;
        if !parent_metadata.file_type().is_dir()
            || parent_metadata.file_type().is_symlink()
            || parent_metadata.uid() != self.expected_path_uid
            || parent_metadata.permissions().mode() & 0o022 != 0
        {
            return Err(path_failure());
        }
        let metadata = fs::symlink_metadata(&self.path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                failure(
                    "SunshineCertificateControlUnavailable",
                    "Sunshine certificate control socket is unavailable",
                )
            } else {
                path_failure()
            }
        })?;
        if !metadata.file_type().is_socket()
            || metadata.file_type().is_symlink()
            || metadata.uid() != self.expected_path_uid
            || metadata.gid() != self.expected_path_gid
            || metadata.permissions().mode() & 0o7777 != self.expected_mode
        {
            return Err(path_failure());
        }
        Ok(SocketIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SocketIdentity {
    device: u64,
    inode: u64,
}

fn required_id_env(name: &str, label: &str) -> Result<u32, RpcFailure> {
    std::env::var(name)
        .map_err(|_| {
            failure(
                "SunshineCertificateControlInvalid",
                &format!("Sunshine certificate control {label} is not configured"),
            )
        })?
        .parse::<u32>()
        .map_err(|_| {
            failure(
                "SunshineCertificateControlInvalid",
                &format!("Sunshine certificate control {label} is invalid"),
            )
        })
}

impl MoonlightCertificateAdapter for SocketCertificateAdapter {
    fn available(&self) -> bool {
        let Ok(socket_identity) = self.validate_path() else {
            return false;
        };
        probe_seqpacket(
            &self.path,
            socket_identity,
            self.expected_peer_uid,
            self.expected_peer_gid,
            self.timeout,
        )
        .is_ok()
    }

    fn attest(&self, host_uuid: &str) -> Result<bool, RpcFailure> {
        validate_host_uuid(host_uuid)?;
        match self.request(SocketRequest {
            operation: SocketOperation::Attest,
            host_uuid,
            certificate: None,
        })? {
            SocketSuccess::Attested(matched) => Ok(matched),
            _ => Err(protocol_failure()),
        }
    }

    fn provision(
        &self,
        host_uuid: &str,
        client_certificate: &str,
    ) -> Result<MoonlightCertificateProvisioned, RpcFailure> {
        validate_host_uuid(host_uuid)?;
        validate_single_pem(client_certificate)?;
        match self.request(SocketRequest {
            operation: SocketOperation::Provision,
            host_uuid,
            certificate: Some(client_certificate),
        })? {
            SocketSuccess::Changed {
                changed: _,
                server_certificate,
            } => {
                validate_single_pem(&server_certificate)?;
                Ok(MoonlightCertificateProvisioned { server_certificate })
            }
            _ => Err(protocol_failure()),
        }
    }

    fn revoke(&self, host_uuid: &str, client_certificate: &str) -> Result<bool, RpcFailure> {
        validate_host_uuid(host_uuid)?;
        validate_single_pem(client_certificate)?;
        match self.request(SocketRequest {
            operation: SocketOperation::Revoke,
            host_uuid,
            certificate: Some(client_certificate),
        })? {
            SocketSuccess::Changed {
                changed,
                server_certificate,
            } => {
                validate_single_pem(&server_certificate)?;
                Ok(changed)
            }
            _ => Err(protocol_failure()),
        }
    }
}

pub fn validate_single_pem(value: &str) -> Result<(), RpcFailure> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    if value.is_empty() || value.len() > MAX_CERTIFICATE_BYTES || value.contains('\0') {
        return Err(invalid_certificate());
    }
    let begin = value.find(BEGIN).ok_or_else(invalid_certificate)?;
    if !value[..begin].chars().all(char::is_whitespace)
        || value[begin + BEGIN.len()..].contains(BEGIN)
    {
        return Err(invalid_certificate());
    }
    let end_start = value.find(END).ok_or_else(invalid_certificate)?;
    if end_start <= begin + BEGIN.len() || value[end_start + END.len()..].contains(END) {
        return Err(invalid_certificate());
    }
    if !value[end_start + END.len()..]
        .chars()
        .all(char::is_whitespace)
    {
        return Err(invalid_certificate());
    }
    Ok(())
}

pub fn validate_host_uuid(host_uuid: &str) -> Result<(), RpcFailure> {
    if host_uuid.is_empty() || host_uuid.len() > 256 || host_uuid.contains('\0') {
        Err(failure(
            "InvalidMoonlightHostUuid",
            "Moonlight host UUID is invalid",
        ))
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum SocketOperation {
    Attest,
    Provision,
    Revoke,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SocketRequest<'a> {
    operation: SocketOperation,
    host_uuid: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    certificate: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SocketResponse {
    status: String,
    #[serde(default)]
    matched: Option<bool>,
    #[serde(default)]
    changed: Option<bool>,
    #[serde(default, rename = "serverCertificate")]
    server_certificate: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

enum SocketSuccess {
    Attested(bool),
    Changed {
        changed: bool,
        server_certificate: String,
    },
}

impl SocketResponse {
    fn into_result(self, operation: SocketOperation) -> Result<SocketSuccess, RpcFailure> {
        match self.status.as_str() {
            "error"
                if self.matched.is_none()
                    && self.changed.is_none()
                    && self.server_certificate.is_none() =>
            {
                let code = self.code.ok_or_else(protocol_failure)?;
                Err(map_sunshine_error(&code))
            }
            "ok" if self.code.is_none() => match operation {
                SocketOperation::Attest => {
                    match (self.matched, self.changed, self.server_certificate) {
                        (Some(matched), None, None) => Ok(SocketSuccess::Attested(matched)),
                        _ => Err(protocol_failure()),
                    }
                }
                SocketOperation::Provision | SocketOperation::Revoke => {
                    match (self.matched, self.changed, self.server_certificate) {
                        (None, Some(changed), Some(server_certificate)) => {
                            Ok(SocketSuccess::Changed {
                                changed,
                                server_certificate,
                            })
                        }
                        _ => Err(protocol_failure()),
                    }
                }
            },
            _ => Err(protocol_failure()),
        }
    }
}

fn map_sunshine_error(code: &str) -> RpcFailure {
    match code {
        "HostMismatch" => failure("HostMismatch", "Sunshine host UUID does not match"),
        "InvalidCertificate" => invalid_certificate(),
        "PersistenceFailed" => failure(
            "SunshineCertificateControlPersistenceFailed",
            "Sunshine could not persist certificate control state",
        ),
        "StateIntegrityFailed" => failure(
            "SunshineCertificateControlStateIntegrityFailed",
            "Sunshine certificate control state integrity failed",
        ),
        "InvalidFrame" | "UnknownOperation" | "ResponseTooLarge" => protocol_failure(),
        "InvalidState" | "InternalError" => failure(
            "SunshineCertificateControlFailed",
            "Sunshine certificate control request failed",
        ),
        _ => protocol_failure(),
    }
}

/// Connects to the protected control socket and performs every identity and
/// peer-credential check, without sending a frame. The returned descriptor
/// is dropped by the caller; the peer sees one connect and one close.
fn probe_seqpacket(
    path: &Path,
    expected_identity: SocketIdentity,
    expected_peer_uid: u32,
    expected_peer_gid: u32,
    timeout: Duration,
) -> io::Result<()> {
    connect_seqpacket(
        path,
        expected_identity,
        expected_peer_uid,
        expected_peer_gid,
        timeout,
    )
    .map(drop)
}

fn connect_seqpacket(
    path: &Path,
    expected_identity: SocketIdentity,
    expected_peer_uid: u32,
    expected_peer_gid: u32,
    timeout: Duration,
) -> io::Result<OwnedFd> {
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let path_bytes = path.as_os_str().as_bytes();
    let sun_path_capacity = unsafe { mem::zeroed::<libc::sockaddr_un>() }.sun_path.len();
    if path_bytes.is_empty() || path_bytes.len() >= sun_path_capacity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid socket path",
        ));
    }
    let mut address: libc::sockaddr_un = unsafe { mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let c_path = CString::new(path_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid socket path"))?;
    unsafe {
        std::ptr::copy_nonoverlapping(
            c_path.as_ptr(),
            address.sun_path.as_mut_ptr(),
            c_path.as_bytes_with_nul().len(),
        );
    }
    let address_len =
        (mem::size_of::<libc::sa_family_t>() + c_path.as_bytes_with_nul().len()) as libc::socklen_t;
    let connected = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&raw const address).cast::<libc::sockaddr>(),
            address_len,
        )
    };
    if connected != 0 {
        let error = io::Error::last_os_error();
        let raw = error.raw_os_error();
        if raw != Some(libc::EINPROGRESS) && raw != Some(libc::EAGAIN) {
            return Err(error);
        }
        wait_for_connect(fd.as_raw_fd(), timeout)?;
    }
    let connected_metadata = fs::symlink_metadata(path)?;
    if !connected_metadata.file_type().is_socket()
        || connected_metadata.dev() != expected_identity.device
        || connected_metadata.ino() != expected_identity.inode
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "connected socket path changed",
        ));
    }
    validate_peer_credentials(fd.as_raw_fd(), expected_peer_uid, expected_peer_gid)?;
    Ok(fd)
}

fn send_seqpacket(
    path: &Path,
    expected_identity: SocketIdentity,
    expected_peer_uid: u32,
    expected_peer_gid: u32,
    request: &[u8],
    timeout: Duration,
) -> io::Result<Vec<u8>> {
    let fd = connect_seqpacket(
        path,
        expected_identity,
        expected_peer_uid,
        expected_peer_gid,
        timeout,
    )?;
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    set_socket_timeouts(fd.as_raw_fd(), timeout)?;
    let sent = unsafe {
        libc::send(
            fd.as_raw_fd(),
            request.as_ptr().cast(),
            request.len(),
            libc::MSG_NOSIGNAL,
        )
    };
    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    if sent as usize != request.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "partial socket frame",
        ));
    }
    let mut response = vec![0_u8; MAX_FRAME_BYTES];
    let received = unsafe {
        libc::recv(
            fd.as_raw_fd(),
            response.as_mut_ptr().cast(),
            response.len(),
            libc::MSG_TRUNC,
        )
    };
    if received < 0 {
        return Err(io::Error::last_os_error());
    }
    let received = received as usize;
    if received == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "socket closed",
        ));
    }
    if received > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "socket frame is too large",
        ));
    }
    response.truncate(received);
    Ok(response)
}

fn validate_peer_credentials(
    fd: libc::c_int,
    expected_uid: u32,
    expected_gid: u32,
) -> io::Result<()> {
    let mut credentials: libc::ucred = unsafe { mem::zeroed() };
    let mut length = mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut credentials).cast(),
            &mut length,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    if length as usize != mem::size_of::<libc::ucred>()
        || credentials.uid != expected_uid
        || credentials.gid != expected_gid
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unexpected certificate control peer",
        ));
    }
    Ok(())
}

fn wait_for_connect(fd: libc::c_int, timeout: Duration) -> io::Result<()> {
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLOUT,
        revents: 0,
    };
    let ready = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
    if ready == 0 {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "socket connect timed out",
        ));
    }
    if ready < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut socket_error: libc::c_int = 0;
    let mut length = mem::size_of::<libc::c_int>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            (&raw mut socket_error).cast(),
            &mut length,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    if socket_error != 0 {
        return Err(io::Error::from_raw_os_error(socket_error));
    }
    Ok(())
}

fn set_socket_timeouts(fd: libc::c_int, timeout: Duration) -> io::Result<()> {
    let value = libc::timeval {
        tv_sec: timeout.as_secs() as libc::time_t,
        tv_usec: timeout.subsec_micros() as libc::suseconds_t,
    };
    for option in [libc::SO_SNDTIMEO, libc::SO_RCVTIMEO] {
        let result = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                option,
                (&raw const value).cast(),
                mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn socket_failure(error: io::Error) -> RpcFailure {
    let code = match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => "SunshineCertificateControlTimeout",
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => {
            "SunshineCertificateControlUnavailable"
        }
        io::ErrorKind::PermissionDenied => "SunshineCertificateControlInvalid",
        _ => "SunshineCertificateControlFailed",
    };
    failure(code, "Sunshine certificate control request failed")
}

fn path_failure() -> RpcFailure {
    failure(
        "SunshineCertificateControlInvalid",
        "Sunshine certificate control socket failed validation",
    )
}

fn protocol_failure() -> RpcFailure {
    failure(
        "SunshineCertificateControlProtocol",
        "Sunshine certificate control returned an invalid response",
    )
}

fn invalid_certificate() -> RpcFailure {
    failure(
        "InvalidMoonlightClientCertificate",
        "Moonlight client certificate must be one bounded PEM certificate",
    )
}

fn failure(code: &str, message: &str) -> RpcFailure {
    RpcFailure {
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_validation_is_exact_and_redacted() {
        let pem = "-----BEGIN CERTIFICATE-----\nbody\n-----END CERTIFICATE-----\n";
        assert!(validate_single_pem(pem).is_ok());
        for invalid in [
            "",
            "not pem",
            "x-----BEGIN CERTIFICATE-----\nbody\n-----END CERTIFICATE-----",
            "-----BEGIN CERTIFICATE-----\nbody\n-----END CERTIFICATE-----x",
            "-----BEGIN CERTIFICATE-----\nbody\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nbody\n-----END CERTIFICATE-----",
        ] {
            let error = validate_single_pem(invalid).unwrap_err();
            assert_eq!(error.code, "InvalidMoonlightClientCertificate");
            if !invalid.is_empty() {
                assert!(!error.message.contains(invalid));
            }
        }
    }

    fn bind_seqpacket(path: &Path) -> OwnedFd {
        let fd =
            unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
        assert!(fd >= 0, "{}", io::Error::last_os_error());
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let bytes = path.as_os_str().as_bytes();
        let mut address: libc::sockaddr_un = unsafe { mem::zeroed() };
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr().cast(),
                address.sun_path.as_mut_ptr(),
                bytes.len(),
            );
        }
        let len = (mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t;
        assert_eq!(
            unsafe {
                libc::bind(
                    fd.as_raw_fd(),
                    (&raw const address).cast::<libc::sockaddr>(),
                    len,
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
        assert_eq!(unsafe { libc::listen(fd.as_raw_fd(), 4) }, 0);
        fd
    }

    #[test]
    fn socket_adapter_round_trips_one_bounded_seqpacket() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.path().join("certificate.sock");
        let listener = bind_seqpacket(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
        let server = std::thread::spawn(move || {
            let accepted = unsafe {
                OwnedFd::from_raw_fd(libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                ))
            };
            let mut request = [0_u8; MAX_FRAME_BYTES];
            let size = unsafe {
                libc::recv(
                    accepted.as_raw_fd(),
                    request.as_mut_ptr().cast(),
                    request.len(),
                    0,
                )
            };
            assert!(size > 0);
            let request: serde_json::Value =
                serde_json::from_slice(&request[..size as usize]).unwrap();
            assert_eq!(request["operation"], "provision");
            assert_eq!(request["hostUuid"], "sunshine-host");
            let response = serde_json::json!({
                "status": "ok",
                "changed": true,
                "serverCertificate": "-----BEGIN CERTIFICATE-----\nserver\n-----END CERTIFICATE-----\n"
            })
            .to_string();
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
        });
        let adapter = SocketCertificateAdapter::for_test(
            path,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            0o660,
        );
        let result = adapter
            .provision(
                "sunshine-host",
                "-----BEGIN CERTIFICATE-----\nclient\n-----END CERTIFICATE-----\n",
            )
            .unwrap();
        assert!(result.server_certificate.contains("server"));
        server.join().unwrap();
    }

    #[test]
    fn socket_adapter_availability_probe_connects_without_sending_a_frame() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.path().join("probe.sock");
        let listener = bind_seqpacket(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
        let server = std::thread::spawn(move || {
            let accepted = unsafe {
                OwnedFd::from_raw_fd(libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                ))
            };
            // The probe must close without writing. A zero-length read is
            // the peer-closed signal on a SOCK_SEQPACKET socket.
            let mut byte = [0_u8; 1];
            let received =
                unsafe { libc::recv(accepted.as_raw_fd(), byte.as_mut_ptr().cast(), 1, 0) };
            assert_eq!(received, 0, "probe must not send any frame");
        });
        let adapter = SocketCertificateAdapter::for_test(
            path,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            0o660,
        );
        assert!(adapter.available());
        server.join().unwrap();
    }

    #[test]
    fn socket_adapter_availability_is_false_for_missing_invalid_and_wrong_peer() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();

        let missing = SocketCertificateAdapter::for_test(
            root.path().join("missing.sock"),
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            0o660,
        );
        assert!(!missing.available());

        let wrong_mode = root.path().join("wrong-mode.sock");
        let _mode_listener = bind_seqpacket(&wrong_mode);
        fs::set_permissions(&wrong_mode, fs::Permissions::from_mode(0o666)).unwrap();
        let invalid = SocketCertificateAdapter::for_test(
            wrong_mode,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            0o660,
        );
        assert!(!invalid.available());

        let wrong_peer = root.path().join("wrong-peer.sock");
        let peer_listener = bind_seqpacket(&wrong_peer);
        fs::set_permissions(&wrong_peer, fs::Permissions::from_mode(0o660)).unwrap();
        let server = std::thread::spawn(move || {
            let accepted = unsafe {
                OwnedFd::from_raw_fd(libc::accept4(
                    peer_listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                ))
            };
            let mut byte = [0_u8; 1];
            assert_eq!(
                unsafe { libc::recv(accepted.as_raw_fd(), byte.as_mut_ptr().cast(), 1, 0) },
                0
            );
        });
        let mismatched = SocketCertificateAdapter::for_test(
            wrong_peer,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() }.wrapping_add(1),
            unsafe { libc::getegid() },
            0o660,
        );
        assert!(!mismatched.available());
        server.join().unwrap();

        // A bound path with no listener accepting is still "connected" at
        // the kernel level for AF_UNIX, so a never-accepting server cannot
        // be distinguished from a slow one here. That case is bounded by
        // the connect timeout and covered by the timeout test above.
        let unavailable = UnavailableCertificateAdapter {
            failure: failure("SunshineCertificateControlUnavailable", "not configured"),
        };
        assert!(!unavailable.available());
    }

    #[test]
    fn socket_adapter_rejects_a_connected_peer_with_wrong_credentials() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.path().join("wrong-peer.sock");
        let listener = bind_seqpacket(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
        let server = std::thread::spawn(move || {
            let accepted = unsafe {
                OwnedFd::from_raw_fd(libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                ))
            };
            let mut byte = [0_u8; 1];
            assert_eq!(
                unsafe { libc::recv(accepted.as_raw_fd(), byte.as_mut_ptr().cast(), 1, 0) },
                0
            );
        });
        let adapter = SocketCertificateAdapter::for_test(
            path,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() }.wrapping_add(1),
            unsafe { libc::getegid() },
            0o660,
        );
        let error = adapter.attest("sunshine-host").unwrap_err();
        assert_eq!(error.code, "SunshineCertificateControlInvalid");
        server.join().unwrap();
    }

    #[test]
    fn socket_adapter_distinguishes_missing_and_invalid_paths() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let missing = SocketCertificateAdapter::for_test(
            root.path().join("missing.sock"),
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            0o660,
        );
        assert_eq!(
            missing.attest("sunshine-host").unwrap_err().code,
            "SunshineCertificateControlUnavailable"
        );

        let path = root.path().join("wrong-mode.sock");
        let _listener = bind_seqpacket(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        let invalid = SocketCertificateAdapter::for_test(
            path,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            0o660,
        );
        assert_eq!(
            invalid.attest("sunshine-host").unwrap_err().code,
            "SunshineCertificateControlInvalid"
        );
    }

    #[test]
    fn socket_adapter_maps_timeout_without_exposing_request_material() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.path().join("timeout.sock");
        let listener = bind_seqpacket(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
        let server = std::thread::spawn(move || {
            let accepted = unsafe {
                OwnedFd::from_raw_fd(libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                ))
            };
            let mut request = [0_u8; MAX_FRAME_BYTES];
            assert!(
                unsafe {
                    libc::recv(
                        accepted.as_raw_fd(),
                        request.as_mut_ptr().cast(),
                        request.len(),
                        0,
                    )
                } > 0
            );
            std::thread::sleep(Duration::from_millis(100));
        });
        let mut adapter = SocketCertificateAdapter::for_test(
            path,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            0o660,
        );
        adapter.timeout = Duration::from_millis(20);
        let error = adapter.attest("sunshine-host").unwrap_err();
        assert_eq!(error.code, "SunshineCertificateControlTimeout");
        assert!(!error.message.contains("sunshine-host"));
        server.join().unwrap();
    }

    enum InvalidSocketReply {
        Bytes(Vec<u8>),
        Close,
    }

    fn socket_adapter_error_for_reply(reply: InvalidSocketReply) -> RpcFailure {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.path().join("invalid-reply.sock");
        let listener = bind_seqpacket(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
        let server = std::thread::spawn(move || {
            let accepted = unsafe {
                OwnedFd::from_raw_fd(libc::accept4(
                    listener.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                ))
            };
            let mut request = [0_u8; MAX_FRAME_BYTES];
            assert!(
                unsafe {
                    libc::recv(
                        accepted.as_raw_fd(),
                        request.as_mut_ptr().cast(),
                        request.len(),
                        0,
                    )
                } > 0
            );
            if let InvalidSocketReply::Bytes(response) = reply {
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
        });
        let adapter = SocketCertificateAdapter::for_test(
            path,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
            0o660,
        );
        let error = adapter.attest("sunshine-host").unwrap_err();
        server.join().unwrap();
        error
    }

    #[test]
    fn socket_adapter_rejects_malformed_json_with_a_redacted_protocol_failure() {
        let secret = "not-json-with-client-certificate-body";
        let error =
            socket_adapter_error_for_reply(InvalidSocketReply::Bytes(secret.as_bytes().to_vec()));
        assert_eq!(error.code, "SunshineCertificateControlProtocol");
        assert_eq!(
            error.message,
            "Sunshine certificate control returned an invalid response"
        );
        assert!(!error.message.contains(secret));
        assert!(!error.message.contains("sunshine-host"));
    }

    #[test]
    fn socket_adapter_rejects_peer_close_without_exposing_request_material() {
        let error = socket_adapter_error_for_reply(InvalidSocketReply::Close);
        assert_eq!(error.code, "SunshineCertificateControlFailed");
        assert_eq!(error.message, "Sunshine certificate control request failed");
        assert!(!error.message.contains("sunshine-host"));
    }

    #[test]
    fn socket_adapter_rejects_an_oversized_seqpacket_via_msg_trunc() {
        let oversized = vec![b'x'; MAX_FRAME_BYTES + 1];
        let error = socket_adapter_error_for_reply(InvalidSocketReply::Bytes(oversized));
        assert_eq!(error.code, "SunshineCertificateControlFailed");
        assert_eq!(error.message, "Sunshine certificate control request failed");
        assert!(!error.message.contains("sunshine-host"));
    }

    #[test]
    fn socket_response_requires_the_exact_operation_shape() {
        let attest: SocketResponse =
            serde_json::from_str(r#"{"status":"ok","matched":true}"#).unwrap();
        assert!(matches!(
            attest.into_result(SocketOperation::Attest),
            Ok(SocketSuccess::Attested(true))
        ));
        let wrong: SocketResponse =
            serde_json::from_str(r#"{"status":"ok","changed":true,"serverCertificate":"secret"}"#)
                .unwrap();
        assert!(wrong.into_result(SocketOperation::Attest).is_err());
        let error: SocketResponse =
            serde_json::from_str(r#"{"status":"error","code":"HostMismatch"}"#).unwrap();
        let error = match error.into_result(SocketOperation::Provision) {
            Err(error) => error,
            Ok(_) => panic!("error response must fail"),
        };
        assert_eq!(error.code, "HostMismatch");
        assert!(!error.message.contains("secret"));

        let peer_controlled: SocketResponse =
            serde_json::from_str(r#"{"status":"error","code":"PeerControlledCode"}"#).unwrap();
        let error = match peer_controlled.into_result(SocketOperation::Provision) {
            Err(error) => error,
            Ok(_) => panic!("unknown peer error must fail"),
        };
        assert_eq!(error.code, "SunshineCertificateControlProtocol");
        assert!(!error.message.contains("PeerControlledCode"));
    }
}
