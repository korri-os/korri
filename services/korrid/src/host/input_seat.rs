use std::{
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::ffi::OsStrExt,
    },
    path::{Path, PathBuf},
};

const VERSION: u8 = 1;
const START: u8 = 1;
const STOP: u8 = 2;
const RESET: u8 = 3;
const IO_TIMEOUT_MS: i32 = 2_000;

pub trait InputSeatLease: Send {
    fn alive(&self) -> bool;
    fn reset(&self, _launch_id: &str) -> Result<(), String> {
        Err("input-seat reset is unsupported".into())
    }
    fn stop(self: Box<Self>, launch_id: &str) -> Result<(), String>;
}

pub trait InputSeatManager: Send + Sync {
    fn initialize(&self, count: u8, session: Option<&str>) -> Result<(), String>;
    fn apply_count(&self, count: u8) -> Result<(), String>;
    fn begin_session(&self, launch_id: &str) -> Result<(), String>;
    fn end_session(&self, launch_id: &str) -> Result<(), String>;
    fn route(&self, launch_id: Option<&str>) -> Result<(), String>;
    fn ready(&self) -> Result<(), String>;
    /// Terminal coordination loss, not an ordinary refused route request.
    /// Permits unit-only recovery; it never grants input or launch authority.
    fn is_fenced(&self) -> bool;
    fn fence(&self);
    fn start(&self, launch_id: &str) -> Result<Box<dyn InputSeatLease>, String>;
}

#[cfg(test)]
pub struct DisabledInputSeats;
#[cfg(test)]
struct DisabledLease;
#[cfg(test)]
impl InputSeatManager for DisabledInputSeats {
    fn initialize(&self, _count: u8, _session: Option<&str>) -> Result<(), String> {
        Ok(())
    }
    fn apply_count(&self, _count: u8) -> Result<(), String> {
        Ok(())
    }
    fn begin_session(&self, _launch_id: &str) -> Result<(), String> {
        Ok(())
    }
    fn end_session(&self, _launch_id: &str) -> Result<(), String> {
        Ok(())
    }
    fn route(&self, _launch_id: Option<&str>) -> Result<(), String> {
        Ok(())
    }
    fn ready(&self) -> Result<(), String> {
        Ok(())
    }
    fn is_fenced(&self) -> bool {
        false
    }
    fn fence(&self) {}
    fn start(&self, _launch_id: &str) -> Result<Box<dyn InputSeatLease>, String> {
        Ok(Box::new(DisabledLease))
    }
}
#[cfg(test)]
impl InputSeatLease for DisabledLease {
    fn alive(&self) -> bool {
        true
    }
    fn reset(&self, _launch_id: &str) -> Result<(), String> {
        Ok(())
    }
    fn stop(self: Box<Self>, _launch_id: &str) -> Result<(), String> {
        Ok(())
    }
}

pub struct UnixInputSeatManager {
    path: PathBuf,
    coordinator: Result<std::sync::Arc<super::input_coordination::SeatCoordinator>, String>,
    initialized: std::sync::atomic::AtomicBool,
    route: std::sync::Mutex<Option<Option<String>>>,
}
impl UnixInputSeatManager {
    pub(crate) fn new(
        path: PathBuf,
        coordinator: Result<std::sync::Arc<super::input_coordination::SeatCoordinator>, String>,
    ) -> Self {
        Self {
            path,
            coordinator,
            initialized: std::sync::atomic::AtomicBool::new(false),
            route: std::sync::Mutex::new(None),
        }
    }
    fn coordinator(&self) -> Result<&super::input_coordination::SeatCoordinator, String> {
        self.coordinator.as_deref().map_err(Clone::clone)
    }
}
impl InputSeatManager for UnixInputSeatManager {
    fn initialize(&self, count: u8, session: Option<&str>) -> Result<(), String> {
        use korri_input_contract::SeatRequest;
        let coordinator = self.coordinator()?;
        let observed = coordinator.snapshot()?;
        match (session, observed.session.as_deref()) {
            (Some(_), None) => {
                self.fence();
                return Err("receiver lost live session reservations; stop the exact session before restarting input coordination".into());
            }
            (None, Some(retained)) => {
                coordinator.request(SeatRequest::EndSession {
                    launch_id: retained.into(),
                })?;
            }
            (Some(active), Some(retained)) if active != retained => {
                return Err("receiver session differs from authoritative host session".into())
            }
            _ => {}
        }
        if session.is_none() {
            self.apply_count(count)?;
        } else if observed.count != count {
            return Err("stored count differs from active pool".into());
        }
        if let Some(launch_id) = session {
            self.begin_session(launch_id)?;
        }
        self.initialized
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    fn apply_count(&self, count: u8) -> Result<(), String> {
        let reply = self
            .coordinator()?
            .request(korri_input_contract::SeatRequest::ApplyCount { count })?;
        if reply.count != count || reply.session.is_some() || reply.recovery_required {
            self.fence();
            return Err("input count acknowledgement does not match".into());
        }
        Ok(())
    }
    fn begin_session(&self, launch_id: &str) -> Result<(), String> {
        let reply =
            self.coordinator()?
                .request(korri_input_contract::SeatRequest::BeginSession {
                    launch_id: launch_id.into(),
                })?;
        if reply.session.as_deref() != Some(launch_id) || reply.recovery_required {
            self.fence();
            return Err("input session acknowledgement does not match".into());
        }
        *self.route.lock().map_err(|_| "input route poisoned")? = None;
        Ok(())
    }
    fn end_session(&self, launch_id: &str) -> Result<(), String> {
        let coordinator = self.coordinator()?;
        if coordinator.snapshot()?.session.is_none() {
            return Ok(());
        }
        let reply = coordinator.request(korri_input_contract::SeatRequest::EndSession {
            launch_id: launch_id.into(),
        })?;
        if reply.session.is_some() {
            self.fence();
            return Err("receiver did not end exact session".into());
        }
        *self.route.lock().map_err(|_| "input route poisoned")? = None;
        Ok(())
    }
    fn route(&self, launch_id: Option<&str>) -> Result<(), String> {
        self.ready()?;
        let mut route = self.route.lock().map_err(|_| "input route poisoned")?;
        let next = launch_id.map(str::to_owned);
        if route.as_ref() != Some(&next) {
            self.coordinator()?
                .request(korri_input_contract::SeatRequest::Route {
                    launch_id: next.clone(),
                })?;
            *route = Some(next);
        }
        Ok(())
    }
    fn ready(&self) -> Result<(), String> {
        if !self.initialized.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("input pool is not reconciled; resolve any exact active session, then restart korrid".into());
        }
        let observed = self.coordinator()?.snapshot()?;
        if observed.recovery_required {
            return Err("input pool requires recovery".into());
        }
        Ok(())
    }
    fn is_fenced(&self) -> bool {
        match &self.coordinator {
            Ok(coordinator) => coordinator.ready().is_err(),
            Err(_) => true,
        }
    }
    fn fence(&self) {
        self.initialized
            .store(false, std::sync::atomic::Ordering::SeqCst);
        if let Ok(coordinator) = &self.coordinator {
            coordinator.fence();
        }
    }
    fn start(&self, launch_id: &str) -> Result<Box<dyn InputSeatLease>, String> {
        self.ready()?;
        let fd = connect(&self.path)?;
        let peer = peer_credentials(fd.as_raw_fd())?;
        if peer != (0, 0) {
            return Err("input-seat receiver does not have the required root identity".into());
        }
        send_request(fd.as_raw_fd(), START, launch_id)?;
        receive_success(fd.as_raw_fd())?;
        Ok(Box::new(UnixInputSeatLease { fd }))
    }
}
struct UnixInputSeatLease {
    fd: OwnedFd,
}
impl InputSeatLease for UnixInputSeatLease {
    fn alive(&self) -> bool {
        let mut pollfd = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLHUP | libc::POLLERR,
            revents: 0,
        };
        (unsafe { libc::poll(&mut pollfd, 1, 0) }) >= 0
            && pollfd.revents & (libc::POLLHUP | libc::POLLERR) == 0
    }
    fn reset(&self, launch_id: &str) -> Result<(), String> {
        send_request(self.fd.as_raw_fd(), RESET, launch_id)?;
        receive_success(self.fd.as_raw_fd())
    }
    fn stop(self: Box<Self>, launch_id: &str) -> Result<(), String> {
        send_request(self.fd.as_raw_fd(), STOP, launch_id)?;
        receive_success(self.fd.as_raw_fd())
    }
}

pub(super) fn connect(path: &Path) -> Result<OwnedFd, String> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.is_empty() || bytes.len() >= 108 {
        return Err("input-seat control path is invalid".into());
    }
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        return Err(last("socket"));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, source) in address.sun_path.iter_mut().zip(bytes.iter().copied()) {
        *target = source as libc::c_char;
    }
    let length = (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t;
    if unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            length,
        )
    } != 0
    {
        let error = std::io::Error::last_os_error();
        if !error.raw_os_error().is_some_and(|code| {
            code == libc::EINPROGRESS || code == libc::EAGAIN || code == libc::EWOULDBLOCK
        }) {
            return Err(format!("input-seat connect failed: {error}"));
        }
        wait_ready(fd.as_raw_fd(), libc::POLLOUT, IO_TIMEOUT_MS)?;
        let pending = socket_error(fd.as_raw_fd())?;
        if pending != 0 {
            return Err(format!(
                "input-seat connect failed: {}",
                std::io::Error::from_raw_os_error(pending)
            ));
        }
    }
    Ok(fd)
}

fn send_request(fd: RawFd, operation: u8, launch_id: &str) -> Result<(), String> {
    if launch_id.len() != 32
        || !launch_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("launch ID is invalid".into());
    }
    let mut request = [0u8; 34];
    request[0] = VERSION;
    request[1] = operation;
    request[2..].copy_from_slice(launch_id.as_bytes());
    wait_ready(fd, libc::POLLOUT, IO_TIMEOUT_MS)?;
    let count = unsafe {
        libc::send(
            fd,
            request.as_ptr().cast(),
            request.len(),
            libc::MSG_NOSIGNAL | libc::MSG_DONTWAIT,
        )
    };
    if count == request.len() as isize {
        Ok(())
    } else {
        Err(last("send"))
    }
}

fn receive_success(fd: RawFd) -> Result<(), String> {
    wait_ready(fd, libc::POLLIN, IO_TIMEOUT_MS)?;
    let mut reply = [0u8; 4];
    let count = unsafe {
        libc::recv(
            fd,
            reply.as_mut_ptr().cast(),
            reply.len(),
            libc::MSG_TRUNC | libc::MSG_DONTWAIT,
        )
    };
    if count != 3 || reply[0] != VERSION || reply[1] != 0 {
        return Err("input-seat receiver rejected the request".into());
    }
    Ok(())
}

pub(super) fn wait_ready(fd: RawFd, events: i16, timeout_ms: i32) -> Result<(), String> {
    let mut descriptor = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    loop {
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if result > 0 {
            if descriptor.revents & events != 0 {
                return Ok(());
            }
            if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err("input-seat socket closed before the operation completed".into());
            }
            return Err("input-seat socket reported an unexpected event".into());
        }
        if result == 0 {
            return Err("input-seat socket operation timed out".into());
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return Err(last("poll"));
        }
    }
}

fn socket_error(fd: RawFd) -> Result<i32, String> {
    let mut error = 0i32;
    let mut length = std::mem::size_of::<i32>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            (&mut error as *mut i32).cast(),
            &mut length,
        )
    } == 0
    {
        Ok(error)
    } else {
        Err(last("getsockopt"))
    }
}

pub(super) fn peer_credentials(fd: RawFd) -> Result<(u32, u32), String> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    } != 0
    {
        return Err(last("getsockopt"));
    }
    Ok((credentials.uid, credentials.gid))
}
fn last(operation: &str) -> String {
    format!(
        "input-seat {operation} failed: {}",
        std::io::Error::last_os_error()
    )
}

#[cfg(test)]
type ApplyCountCallback = Box<dyn FnMut(u8) + Send>;
#[cfg(test)]
type RouteCallback = Box<dyn FnMut(Option<&str>) -> Result<(), String> + Send>;

#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordingInputPool {
    pub(crate) count: std::sync::Mutex<u8>,
    pub(crate) session: std::sync::Mutex<Option<String>>,
    pub(crate) calls: std::sync::Mutex<Vec<String>>,
    pub(crate) fail_counts: std::sync::Mutex<Vec<u8>>,
    pub(crate) fenced: std::sync::atomic::AtomicBool,
    pub(crate) on_apply: std::sync::Mutex<Option<ApplyCountCallback>>,
    pub(crate) on_route: std::sync::Mutex<Option<RouteCallback>>,
}
#[cfg(test)]
impl InputSeatManager for RecordingInputPool {
    fn initialize(&self, count: u8, session: Option<&str>) -> Result<(), String> {
        self.apply_count(count)?;
        if let Some(id) = session {
            self.begin_session(id)?;
        }
        Ok(())
    }
    fn apply_count(&self, count: u8) -> Result<(), String> {
        self.ready()?;
        self.calls.lock().unwrap().push(format!("count:{count}"));
        let mut failures = self.fail_counts.lock().unwrap();
        if let Some(index) = failures.iter().position(|value| *value == count) {
            failures.remove(index);
            return Err("receiver refused count".into());
        }
        *self.count.lock().unwrap() = count;
        if let Some(callback) = self.on_apply.lock().unwrap().as_mut() {
            callback(count);
        }
        Ok(())
    }
    fn begin_session(&self, id: &str) -> Result<(), String> {
        self.ready()?;
        self.calls.lock().unwrap().push(format!("begin:{id}"));
        *self.session.lock().unwrap() = Some(id.into());
        Ok(())
    }
    fn end_session(&self, id: &str) -> Result<(), String> {
        self.calls.lock().unwrap().push(format!("end:{id}"));
        let mut session = self.session.lock().unwrap();
        if session.as_deref().is_some_and(|active| active != id) {
            return Err("stale session".into());
        }
        *session = None;
        Ok(())
    }
    fn route(&self, id: Option<&str>) -> Result<(), String> {
        self.ready()?;
        self.calls
            .lock()
            .unwrap()
            .push(format!("route:{}", id.unwrap_or("portal")));
        if let Some(callback) = self.on_route.lock().unwrap().as_mut() {
            callback(id)?;
        }
        Ok(())
    }
    fn ready(&self) -> Result<(), String> {
        if self.fenced.load(std::sync::atomic::Ordering::SeqCst) {
            Err("pool fenced".into())
        } else {
            Ok(())
        }
    }
    fn is_fenced(&self) -> bool {
        self.fenced.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn fence(&self) {
        self.fenced.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    fn start(&self, id: &str) -> Result<Box<dyn InputSeatLease>, String> {
        self.calls.lock().unwrap().push(format!("mirror:{id}"));
        Ok(Box::new(DisabledLease))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    const LAUNCH_ID: &str = "0123456789abcdef0123456789abcdef";

    fn socket_pair() -> (OwnedFd, OwnedFd) {
        let mut fds = [-1; 2];
        assert_eq!(
            unsafe {
                libc::socketpair(
                    libc::AF_UNIX,
                    libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                    0,
                    fds.as_mut_ptr(),
                )
            },
            0
        );
        unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
    }

    #[test]
    fn ordinary_route_refusal_is_not_terminal_but_real_transport_eof_is() {
        use super::super::input_coordination::test_support;
        use korri_input_contract::{SeatFailure, SeatRequest};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let lost = Arc::new(AtomicBool::new(false));
        let loss = lost.clone();
        let (coordinator, receiver) =
            test_support::coordinator_until_disconnect(None, move |request| {
                if loss.load(Ordering::SeqCst) {
                    return None;
                }
                let mut reply = test_support::reply();
                if matches!(request, SeatRequest::Route { .. }) {
                    reply.failure = Some(SeatFailure::Active);
                }
                Some(reply)
            });
        let manager = UnixInputSeatManager::new("unused".into(), Ok(coordinator.clone()));
        manager.initialize(4, None).unwrap();
        assert!(manager.route(None).is_err());
        assert!(
            !manager.is_fenced(),
            "refused route still needs its acknowledgement; no emergency bypass"
        );
        assert!(manager.ready().is_ok());
        lost.store(true, Ordering::SeqCst);
        assert!(coordinator.request(SeatRequest::Poll).is_err());
        receiver.join().unwrap();
        assert!(manager.is_fenced());
        assert!(manager.ready().is_err());
        assert!(manager.route(Some(LAUNCH_ID)).is_err());
    }

    #[test]
    fn production_manager_blocks_launch_until_stored_count_is_acknowledged() {
        use super::super::input_coordination::test_support;
        let mut current = test_support::reply();
        current.recovery_required = true;
        let (coordinator, receiver) = test_support::coordinator(move |request| {
            if let korri_input_contract::SeatRequest::ApplyCount { count } = request {
                current.count = count;
                current.recovery_required = false;
            }
            current.clone()
        });
        let manager =
            UnixInputSeatManager::new("unused-lease-path".into(), Ok(coordinator.clone()));
        assert!(manager.ready().is_err());
        manager.initialize(6, None).unwrap();
        assert!(manager.ready().is_ok());
        assert_eq!(coordinator.snapshot().unwrap().count, 6);
        drop(manager);
        drop(coordinator);
        receiver.join().unwrap();
    }

    #[test]
    fn mismatched_count_acknowledgement_fences_production_manager() {
        use super::super::input_coordination::test_support;
        let (coordinator, receiver) = test_support::coordinator(|_| test_support::reply());
        let manager =
            UnixInputSeatManager::new("unused-lease-path".into(), Ok(coordinator.clone()));
        assert!(manager.initialize(6, None).is_err());
        assert!(manager.ready().is_err());
        assert!(coordinator.ready().is_err());
        drop(manager);
        drop(coordinator);
        receiver.join().unwrap();
    }

    #[test]
    fn recovered_live_session_keeps_reservations_instead_of_resizing() {
        use super::super::input_coordination::test_support;
        let mut current = test_support::reply();
        current.count = 6;
        current.session = Some(LAUNCH_ID.into());
        current.recovery_required = true;
        let (coordinator, receiver) = test_support::coordinator(move |request| {
            assert!(!matches!(
                request,
                korri_input_contract::SeatRequest::ApplyCount { .. }
            ));
            if let korri_input_contract::SeatRequest::BeginSession { launch_id } = request {
                assert_eq!(launch_id, LAUNCH_ID);
                current.recovery_required = false;
            }
            current.clone()
        });
        let manager =
            UnixInputSeatManager::new("unused-lease-path".into(), Ok(coordinator.clone()));
        manager.initialize(6, Some(LAUNCH_ID)).unwrap();
        assert!(manager.ready().is_ok());
        assert_eq!(
            coordinator.snapshot().unwrap().session.as_deref(),
            Some(LAUNCH_ID)
        );
        drop(manager);
        drop(coordinator);
        receiver.join().unwrap();
    }

    #[test]
    fn reset_uses_the_version_one_exact_launch_request_on_the_live_lease_socket() {
        let (client, server) = socket_pair();
        let receiver = thread::spawn(move || {
            let mut descriptor = libc::pollfd {
                fd: server.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            assert_eq!(unsafe { libc::poll(&mut descriptor, 1, 500) }, 1);
            let mut request = [0u8; 34];
            assert_eq!(
                unsafe {
                    libc::recv(
                        server.as_raw_fd(),
                        request.as_mut_ptr().cast(),
                        request.len(),
                        0,
                    )
                },
                request.len() as isize
            );
            assert_eq!(&request[..2], &[VERSION, RESET]);
            assert_eq!(&request[2..], LAUNCH_ID.as_bytes());
            let reply = [VERSION, 0, 0];
            assert_eq!(
                unsafe {
                    libc::send(
                        server.as_raw_fd(),
                        reply.as_ptr().cast(),
                        reply.len(),
                        libc::MSG_NOSIGNAL,
                    )
                },
                reply.len() as isize
            );
        });
        let lease = UnixInputSeatLease { fd: client };

        lease.reset(LAUNCH_ID).unwrap();
        receiver.join().unwrap();
    }

    #[test]
    fn reset_receiver_refusal_is_visible() {
        let (client, server) = socket_pair();
        let receiver = thread::spawn(move || {
            let mut descriptor = libc::pollfd {
                fd: server.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            assert_eq!(unsafe { libc::poll(&mut descriptor, 1, 500) }, 1);
            let mut request = [0u8; 34];
            assert_eq!(
                unsafe {
                    libc::recv(
                        server.as_raw_fd(),
                        request.as_mut_ptr().cast(),
                        request.len(),
                        0,
                    )
                },
                request.len() as isize
            );
            let reply = [VERSION, 1, 5];
            assert_eq!(
                unsafe {
                    libc::send(
                        server.as_raw_fd(),
                        reply.as_ptr().cast(),
                        reply.len(),
                        libc::MSG_NOSIGNAL,
                    )
                },
                reply.len() as isize
            );
        });
        let lease = UnixInputSeatLease { fd: client };

        assert_eq!(
            lease.reset(LAUNCH_ID).unwrap_err(),
            "input-seat receiver rejected the request"
        );
        receiver.join().unwrap();
    }
}
