//! The unprivileged coordinator for the root-owned shared pool. No uinput access.
use super::input_seat::{connect, peer_credentials, wait_ready};
use crate::{native_input::*, portal_input::PortalInputSource};
use korri_input_contract::{
    GamepadState, RemoteEvent, SeatReply, SeatRequest, COORDINATION_VERSION, MAX_COORDINATION_BYTES,
};
use std::{
    collections::BTreeMap,
    os::fd::{AsRawFd, OwnedFd},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::broadcast;

const IO_TIMEOUT_MS: i32 = 2_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
type Command = (SeatRequest, mpsc::SyncSender<Result<SeatReply, String>>);

pub(crate) struct SeatCoordinator {
    commands: mpsc::SyncSender<Command>,
    fenced: Arc<AtomicBool>,
    pub(crate) feedback: broadcast::Sender<SeatReply>,
    pub(crate) producer: Arc<tokio::sync::Semaphore>,
    latest: Arc<Mutex<SeatReply>>,
}
impl SeatCoordinator {
    pub(crate) fn connect(
        path: &Path,
        native: Option<Arc<PortalInputSource>>,
    ) -> Result<Arc<Self>, String> {
        let fd = connect(path)?;
        verify_receiver(&fd)?;
        Self::from_connected(fd, native)
    }
    fn from_connected(
        fd: OwnedFd,
        native: Option<Arc<PortalInputSource>>,
    ) -> Result<Arc<Self>, String> {
        let hello = exchange(&fd, &SeatRequest::Hello)?;
        if hello.failure.is_some() {
            return Err("input-seat coordinator refused".into());
        }
        let latest = Arc::new(Mutex::new(hello));
        let (commands, incoming) = mpsc::sync_channel::<Command>(64);
        let (feedback, _) = broadcast::channel(256);
        let fenced = Arc::new(AtomicBool::new(false));
        let coordinator = Arc::new(Self {
            commands,
            fenced: fenced.clone(),
            feedback: feedback.clone(),
            producer: Arc::new(tokio::sync::Semaphore::new(1)),
            latest: latest.clone(),
        });
        std::thread::Builder::new()
            .name("input-coordinator".into())
            .spawn(move || {
                let mut mapper = NativeSources::new(native);
                loop {
                    if fenced.load(Ordering::SeqCst) {
                        break;
                    }
                    let command = match incoming.recv_timeout(Duration::from_millis(20)) {
                        Ok(command) => Some(command),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let request = command
                        .as_ref()
                        .map(|(request, _)| request)
                        .unwrap_or(&SeatRequest::Poll);
                    let result = exchange(&fd, request).and_then(|reply| {
                        mapper.remote(&reply)?;
                        if reply.failure.is_none() {
                            mapper.physical(request)?;
                        }
                        let mut snapshot = latest.lock().map_err(|_| "input snapshot poisoned")?;
                        *snapshot = reply.clone();
                        let _ = feedback.send(reply.clone());
                        Ok(reply)
                    });
                    let failed = result.is_err();
                    if let Some((_, response)) = command {
                        let _ = response.send(result);
                    }
                    if failed {
                        break;
                    }
                }
                fenced.store(true, Ordering::SeqCst);
                mapper.reset();
                // Dropping this authenticated connection neutralizes all sources,
                // revokes mirror authority and retains session reservations.
            })
            .map_err(|error| error.to_string())?;
        Ok(coordinator)
    }
    pub(crate) fn request(&self, request: SeatRequest) -> Result<SeatReply, String> {
        self.ready()?;
        let (send, recv) = mpsc::sync_channel(1);
        self.commands
            .try_send((request, send))
            .map_err(|_| "input coordinator is busy or unavailable")?;
        let reply = match recv.recv_timeout(REQUEST_TIMEOUT) {
            Ok(Ok(reply)) => reply,
            Ok(Err(error)) => {
                self.fence();
                return Err(error);
            }
            Err(_) => {
                self.fence();
                return Err("input coordinator acknowledgement timed out".into());
            }
        };
        if let Some(failure) = &reply.failure {
            return Err(format!("input receiver refused request: {failure:?}"));
        }
        Ok(reply)
    }
    pub(crate) fn ready(&self) -> Result<(), String> {
        if self.fenced.load(Ordering::SeqCst) {
            Err("input pool is uncertain; restart coordination before launching".into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn fence(&self) {
        self.fenced.store(true, Ordering::SeqCst);
    }
    pub(crate) fn subscribe(&self) -> Result<(SeatReply, broadcast::Receiver<SeatReply>), String> {
        self.ready()?;
        let snapshot = self.latest.lock().map_err(|_| "input snapshot poisoned")?;
        Ok((snapshot.clone(), self.feedback.subscribe()))
    }
    pub(crate) fn snapshot(&self) -> Result<SeatReply, String> {
        self.ready()?;
        self.latest
            .lock()
            .map(|reply| reply.clone())
            .map_err(|_| "input snapshot poisoned".into())
    }
}

fn verify_receiver(fd: &OwnedFd) -> Result<(), String> {
    if peer_credentials(fd.as_raw_fd())? != (0, 0) {
        return Err("input-seat receiver does not have the required root identity".into());
    }
    Ok(())
}

fn exchange(fd: &OwnedFd, request: &SeatRequest) -> Result<SeatReply, String> {
    let mut bytes = vec![COORDINATION_VERSION];
    serde_json::to_writer(&mut bytes, request).map_err(|_| "invalid seat request")?;
    if bytes.len() > MAX_COORDINATION_BYTES {
        return Err("seat request too large".into());
    }
    wait_ready(fd.as_raw_fd(), libc::POLLOUT, IO_TIMEOUT_MS)?;
    if unsafe {
        libc::send(
            fd.as_raw_fd(),
            bytes.as_ptr().cast(),
            bytes.len(),
            libc::MSG_NOSIGNAL | libc::MSG_DONTWAIT,
        )
    } != bytes.len() as isize
    {
        return Err("seat request send failed".into());
    }
    wait_ready(fd.as_raw_fd(), libc::POLLIN, IO_TIMEOUT_MS)?;
    let mut reply = vec![0; MAX_COORDINATION_BYTES];
    let size = unsafe {
        libc::recv(
            fd.as_raw_fd(),
            reply.as_mut_ptr().cast(),
            reply.len(),
            libc::MSG_TRUNC | libc::MSG_DONTWAIT,
        )
    };
    if size <= 1 || size as usize > reply.len() || reply[0] != COORDINATION_VERSION {
        return Err("invalid seat reply packet".into());
    }
    serde_json::from_slice(&reply[1..size as usize]).map_err(|_| "invalid seat reply".into())
}

/// Same evdev codes/ranges as inputd/input_seat_uinput.rs. Identity never uses slot.
const BUTTONS: &[(u32, u16)] = &[
    (0x10, 315),
    (0x20, 314),
    (0x40, 317),
    (0x80, 318),
    (0x100, 310),
    (0x200, 311),
    (0x400, 316),
    (0x1000, 304),
    (0x2000, 305),
    (0x4000, 307),
    (0x8000, 308),
];
const AXES: &[(u16, i32, i32, i32)] = &[
    (0, -32768, 32767, 4096),
    (1, -32768, 32767, 4096),
    (2, 0, 255, 0),
    (3, -32768, 32767, 4096),
    (4, -32768, 32767, 4096),
    (5, 0, 255, 0),
    (16, -1, 1, 0),
    (17, -1, 1, 0),
];
struct NativeSources {
    native: Option<Arc<PortalInputSource>>,
    states: BTreeMap<String, GamepadState>,
}
impl NativeSources {
    fn new(native: Option<Arc<PortalInputSource>>) -> Self {
        Self {
            native,
            states: BTreeMap::new(),
        }
    }
    fn emit(&self, event: NativeInputEvent) -> Result<(), String> {
        self.native.as_ref().map_or(Ok(()), |source| {
            source.publish(event).map_err(|e| e.to_string())
        })
    }
    fn connected(&mut self, id: &str, name: &str, state: GamepadState) -> Result<(), String> {
        if self.states.contains_key(id) {
            return Err("duplicate native source".into());
        }
        if let Some(native) = &self.native {
            let device = NativeInputDeviceInfo {
                device_id: id.into(),
                class: NativeInputDeviceClass::Gamepad,
                name: name.into(),
                capabilities: vec!["EV_KEY".into(), "EV_ABS".into()],
                axes: Some(
                    AXES.iter()
                        .map(|&(code, minimum, maximum, flat)| NativeInputAxisInfo {
                            code: code.into(),
                            minimum: minimum.into(),
                            maximum: maximum.into(),
                            flat: Some(flat.into()),
                        })
                        .collect(),
                ),
            };
            let baseline = evdev_values(state)
                .into_iter()
                .map(|(input_type, code, value)| NativeInputInput {
                    kind: NativeInputInputKind::Input,
                    device_id: id.into(),
                    class: NativeInputDeviceClass::Gamepad,
                    input_type: input_type.into(),
                    code: code.into(),
                    value: value.into(),
                    timestamp: 0.0,
                })
                .collect();
            native
                .publish_device(device, baseline)
                .map_err(|e| e.to_string())?;
        }
        self.states.insert(id.into(), state);
        Ok(())
    }
    fn state(&mut self, id: &str, state: GamepadState, baseline: bool) -> Result<(), String> {
        let previous = self.states.get(id).copied();
        if !baseline && previous.is_none() {
            return Err("unknown native source".into());
        }
        let before = previous.map(evdev_values);
        let mut changed = Vec::new();
        for (index, (input_type, code, value)) in evdev_values(state).into_iter().enumerate() {
            if before
                .as_ref()
                .is_some_and(|values| values[index].2 == value)
                && !baseline
            {
                continue;
            }
            changed.push(NativeInputInput {
                kind: NativeInputInputKind::Input,
                device_id: id.into(),
                class: NativeInputDeviceClass::Gamepad,
                input_type: input_type.into(),
                code: code.into(),
                value: value.into(),
                timestamp: 0.0,
            });
        }
        if !changed.is_empty() {
            if let Some(native) = &self.native {
                // A complete producer sample commits once, at SYN_REPORT.
                // Never expose temporary neutral state between button deltas.
                native
                    .publish_frame(id, changed)
                    .map_err(|error| error.to_string())?;
            }
        }
        self.states.insert(id.into(), state);
        Ok(())
    }
    fn disconnected(&mut self, id: &str) -> Result<(), String> {
        if self.states.remove(id).is_some() {
            self.emit(NativeInputEvent::DeviceRemoved(NativeInputDeviceRemoved {
                kind: NativeInputDeviceRemovedKind::DeviceRemoved,
                device_id: id.into(),
            }))?;
        }
        Ok(())
    }
    fn physical(&mut self, request: &SeatRequest) -> Result<(), String> {
        match request {
            SeatRequest::PhysicalConnected {
                device_id,
                name,
                state,
            } => self.connected(&format!("physical:{device_id}"), name, *state),
            SeatRequest::PhysicalState { device_id, state } => {
                self.state(&format!("physical:{device_id}"), *state, false)
            }
            SeatRequest::PhysicalDisconnected { device_id } => {
                self.disconnected(&format!("physical:{device_id}"))
            }
            _ => Ok(()),
        }
    }
    fn remote(&mut self, reply: &SeatReply) -> Result<(), String> {
        for event in &reply.remote_events {
            match event {
                RemoteEvent::Connected { source } => self.connected(
                    &remote_id(&source.launch_id, source.controller_number),
                    "Remote gamepad",
                    source.state,
                )?,
                RemoteEvent::State { source } => self.state(
                    &remote_id(&source.launch_id, source.controller_number),
                    source.state,
                    false,
                )?,
                RemoteEvent::Disconnected {
                    launch_id,
                    controller_number,
                } => self.disconnected(&remote_id(launch_id, *controller_number))?,
            }
        }
        Ok(())
    }
    fn reset(&mut self) {
        self.states.clear();
        if let Some(native) = &self.native {
            native.reset();
        }
    }
}
fn remote_id(launch_id: &str, controller_number: u8) -> String {
    format!("remote:{launch_id}:{controller_number}")
}
#[cfg(test)]
pub(super) mod test_support {
    use super::*;
    use std::os::fd::FromRawFd;
    pub(super) fn pair() -> (OwnedFd, OwnedFd) {
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
    pub(crate) fn reply() -> SeatReply {
        SeatReply {
            failure: None,
            count: 4,
            session: None,
            recovery_required: false,
            slot: None,
            remote_sources: vec![],
            remote_events: vec![],
        }
    }
    /// Uses the actual seqpacket client and typed wire. Only root credential
    /// verification is bypassed so these tests run as an ordinary build user.
    pub(crate) fn coordinator(
        handle: impl FnMut(SeatRequest) -> SeatReply + Send + 'static,
    ) -> (Arc<SeatCoordinator>, std::thread::JoinHandle<()>) {
        coordinator_with_native(None, handle)
    }
    pub(crate) fn coordinator_with_native(
        native: Option<Arc<PortalInputSource>>,
        mut handle: impl FnMut(SeatRequest) -> SeatReply + Send + 'static,
    ) -> (Arc<SeatCoordinator>, std::thread::JoinHandle<()>) {
        coordinator_until_disconnect(native, move |request| Some(handle(request)))
    }
    /// None closes the real receiver socket without acknowledging the request.
    pub(crate) fn coordinator_until_disconnect(
        native: Option<Arc<PortalInputSource>>,
        mut handle: impl FnMut(SeatRequest) -> Option<SeatReply> + Send + 'static,
    ) -> (Arc<SeatCoordinator>, std::thread::JoinHandle<()>) {
        let (client, server) = pair();
        let worker = std::thread::spawn(move || loop {
            let mut bytes = vec![0; MAX_COORDINATION_BYTES];
            let size = unsafe {
                libc::recv(
                    server.as_raw_fd(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                    0,
                )
            };
            if size <= 0 {
                break;
            }
            assert_eq!(bytes[0], COORDINATION_VERSION);
            let request = serde_json::from_slice(&bytes[1..size as usize]).unwrap();
            let Some(reply) = handle(request) else {
                break;
            };
            let mut response = vec![COORDINATION_VERSION];
            serde_json::to_writer(&mut response, &reply).unwrap();
            if unsafe {
                libc::send(
                    server.as_raw_fd(),
                    response.as_ptr().cast(),
                    response.len(),
                    libc::MSG_NOSIGNAL,
                )
            } < 0
            {
                break;
            }
        });
        (
            SeatCoordinator::from_connected(client, native).unwrap(),
            worker,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_seqpacket_credentials_require_both_root_uid_and_gid() {
        let (client, _server) = test_support::pair();
        let expected = unsafe { libc::geteuid() == 0 && libc::getegid() == 0 };
        assert_eq!(verify_receiver(&client).is_ok(), expected);
    }
    #[test]
    fn real_seqpacket_truncation_and_wrong_version_are_refused() {
        for response in [
            vec![1, b'{', b'}'],
            vec![2; MAX_COORDINATION_BYTES + 1],
            vec![2, b'{', b'}'],
        ] {
            let (client, server) = test_support::pair();
            let receiver = std::thread::spawn(move || {
                let mut request = [0; 128];
                assert!(
                    unsafe {
                        libc::recv(
                            server.as_raw_fd(),
                            request.as_mut_ptr().cast(),
                            request.len(),
                            0,
                        )
                    } > 0
                );
                assert_eq!(
                    unsafe {
                        libc::send(
                            server.as_raw_fd(),
                            response.as_ptr().cast(),
                            response.len(),
                            libc::MSG_NOSIGNAL,
                        )
                    },
                    response.len() as isize
                );
            });
            assert!(exchange(&client, &SeatRequest::Hello).is_err());
            receiver.join().unwrap();
        }
    }

    #[test]
    fn explicit_uncertainty_fence_refuses_further_commands() {
        let (coordinator, worker) = test_support::coordinator(|_| test_support::reply());
        coordinator.fence();
        assert!(coordinator
            .request(SeatRequest::ApplyCount { count: 6 })
            .is_err());
        drop(coordinator);
        worker.join().unwrap();
    }
    #[test]
    fn receiver_eof_fences_further_commands() {
        let (client, server) = test_support::pair();
        let receiver = std::thread::spawn(move || {
            let mut request = [0; 128];
            assert!(
                unsafe {
                    libc::recv(
                        server.as_raw_fd(),
                        request.as_mut_ptr().cast(),
                        request.len(),
                        0,
                    )
                } > 0
            );
            let mut response = vec![COORDINATION_VERSION];
            serde_json::to_writer(&mut response, &test_support::reply()).unwrap();
            assert_eq!(
                unsafe {
                    libc::send(
                        server.as_raw_fd(),
                        response.as_ptr().cast(),
                        response.len(),
                        libc::MSG_NOSIGNAL,
                    )
                },
                response.len() as isize
            );
        });
        let coordinator = SeatCoordinator::from_connected(client, None).unwrap();
        receiver.join().unwrap();
        assert!(coordinator.request(SeatRequest::Poll).is_err());
        assert!(coordinator.ready().is_err());
    }

    #[test]
    fn seqpacket_carries_acknowledged_count_then_authoritative_session() {
        let requests = Arc::new(Mutex::new(vec![]));
        let received = requests.clone();
        let mut current = test_support::reply();
        let (coordinator, worker) = test_support::coordinator(move |request| {
            received.lock().unwrap().push(request.clone());
            match request {
                SeatRequest::ApplyCount { count } => current.count = count,
                SeatRequest::BeginSession { launch_id } => current.session = Some(launch_id),
                SeatRequest::EndSession { .. } => current.session = None,
                _ => {}
            }
            current.clone()
        });
        assert_eq!(
            coordinator
                .request(SeatRequest::ApplyCount { count: 6 })
                .unwrap()
                .count,
            6
        );
        let launch_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        assert_eq!(
            coordinator
                .request(SeatRequest::BeginSession {
                    launch_id: launch_id.clone()
                })
                .unwrap()
                .session
                .as_deref(),
            Some(launch_id.as_str())
        );
        coordinator
            .request(SeatRequest::EndSession { launch_id })
            .unwrap();
        drop(coordinator);
        worker.join().unwrap();
        let requests = requests.lock().unwrap();
        let count = requests
            .iter()
            .position(|r| matches!(r, SeatRequest::ApplyCount { count: 6 }))
            .unwrap();
        let begin = requests
            .iter()
            .position(|r| matches!(r, SeatRequest::BeginSession { .. }))
            .unwrap();
        assert!(count < begin);
    }
    #[test]
    fn native_mapping_matches_actual_uinput_axes_and_all_eleven_buttons() {
        let values = evdev_values(GamepadState {
            buttons: 0xf7ff,
            left_trigger: 255,
            right_trigger: 128,
            left_stick_x: i16::MIN,
            left_stick_y: i16::MAX,
            right_stick_x: -42,
            right_stick_y: 43,
        });
        assert_eq!(values.len(), 19);
        assert_eq!(
            &values[..11],
            &[
                (1, 315, 1),
                (1, 314, 1),
                (1, 317, 1),
                (1, 318, 1),
                (1, 310, 1),
                (1, 311, 1),
                (1, 316, 1),
                (1, 304, 1),
                (1, 305, 1),
                (1, 307, 1),
                (1, 308, 1)
            ]
        );
        assert_eq!(
            &values[11..],
            &[
                (3, 0, -32768),
                (3, 1, 32767),
                (3, 2, 255),
                (3, 3, -42),
                (3, 4, 43),
                (3, 5, 128),
                (3, 16, 0),
                (3, 17, 0)
            ]
        );
    }
    #[test]
    fn overflow_remote_navigation_uses_source_identity_not_slot() {
        let mut mapper = NativeSources::new(None);
        let source = korri_input_contract::RemoteSource {
            launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            controller_number: 7,
            state: GamepadState::neutral(),
            slot: None,
        };
        let mut reply = test_support::reply();
        reply.remote_events.push(RemoteEvent::Connected {
            source: source.clone(),
        });
        mapper.remote(&reply).unwrap();
        assert!(mapper.states.contains_key(&remote_id(&source.launch_id, 7)));
        reply.remote_events = vec![RemoteEvent::Disconnected {
            launch_id: source.launch_id,
            controller_number: 7,
        }];
        mapper.remote(&reply).unwrap();
        assert!(mapper.states.is_empty());
    }
}

fn evdev_values(state: GamepadState) -> Vec<(u16, u16, i32)> {
    let mut result: Vec<_> = BUTTONS
        .iter()
        .map(|&(mask, code)| (1, code, i32::from(state.buttons & mask != 0)))
        .collect();
    let hat = |negative, positive| {
        i32::from(state.buttons & positive != 0) - i32::from(state.buttons & negative != 0)
    };
    result.extend([
        (3, 0, state.left_stick_x.into()),
        (3, 1, state.left_stick_y.into()),
        (3, 2, state.left_trigger.into()),
        (3, 3, state.right_stick_x.into()),
        (3, 4, state.right_stick_y.into()),
        (3, 5, state.right_trigger.into()),
        (3, 16, hat(4, 8)),
        (3, 17, hat(1, 2)),
    ]);
    result
}
