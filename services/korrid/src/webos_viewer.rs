//! Owned multi-call viewer child. Trust and game lifetime remain in korrid.
use crate::linux_viewer::{
    CertificateProvision, LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus,
};
use futures::future::BoxFuture;
use std::{
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

fn error(message: impl ToString) -> LinuxViewerError {
    LinuxViewerError::new(message.to_string())
}

/// Attachment to the single existing native input producer.
pub type InputSource =
    Arc<dyn Fn() -> Option<Arc<crate::surface_host_input::SurfaceHostInputSource>> + Send + Sync>;

/// The same executable path as appinfo.main is mandatory for NDL's LS2 identity.
pub fn viewer(
    executable: PathBuf,
    identity: PathBuf,
    provision: CertificateProvision,
    presenting: Arc<dyn Fn(bool) + Send + Sync>,
    input: InputSource,
) -> Arc<dyn LinuxStreamViewer> {
    Arc::new(WebosViewer {
        executable,
        identity,
        provision,
        presenting,
        input,
        readiness_timeout: Duration::from_secs(45),
        session: Arc::new(Mutex::new(None)),
    })
}

#[derive(Clone)]
struct WebosViewer {
    executable: PathBuf,
    identity: PathBuf,
    provision: CertificateProvision,
    session: Arc<Mutex<Option<Session>>>,
    presenting: Arc<dyn Fn(bool) + Send + Sync>,
    input: InputSource,
    readiness_timeout: Duration,
}
struct Session {
    launch: String,
    address: String,
    child: Option<OwnedChild>,
    backgrounded: bool,
}
struct OwnedChild {
    child: Child,
    _channel: UnixStream,
    presenting: Arc<dyn Fn(bool) + Send + Sync>,
    _parent_thread: Option<ParentThread>,
    input: Option<(
        Arc<std::sync::atomic::AtomicBool>,
        std::thread::JoinHandle<()>,
    )>,
}
// Linux PDEATHSIG follows the forking THREAD, not just the process. Keep a
// dedicated parent thread alive for this child's whole lifetime. A transient
// Tokio blocking worker would kill a healthy stream when its idle thread exits.
struct ParentThread {
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for ParentThread {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        (self.presenting)(false);
        if let Some((stop, thread)) = self.input.take() {
            stop.store(true, std::sync::atomic::Ordering::Release);
            let _ = thread.join();
        }
        let _ = self._channel.shutdown(std::net::Shutdown::Write);
        // Let the owning C thread neutralize input and release media first.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // Child::kill targets an unreaped owned handle, never a name or saved PID.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl WebosViewer {
    async fn release(child: Option<OwnedChild>) -> Result<(), LinuxViewerError> {
        tokio::task::spawn_blocking(move || drop(child))
            .await
            .map_err(error)
    }
    fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .arg("viewer")
            .env_clear()
            .env("APPID", "org.korri.shell");
        if let Some(root) = self.executable.parent().and_then(|p| p.parent()) {
            command.env("LD_LIBRARY_PATH", root.join("lib"));
            command.env("SS4S_MODULE_PATH", root.join("lib"));
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let parent = std::process::id();
        // SAFETY: only async-signal-safe syscalls run after fork. The parent
        // check closes the death-before-prctl race. No capability is inherited.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
                    || libc::getppid() as u32 != parent
                {
                    return Err(std::io::Error::other("viewer parent disappeared"));
                }
                Ok(())
            });
        }
        command
    }
    fn output(&self, arguments: &[String], limit: usize) -> Result<String, LinuxViewerError> {
        let mut child = self
            .command()
            .args(arguments)
            .stdout(Stdio::piped())
            .spawn()
            .map_err(error)?;
        let mut pipe = child
            .stdout
            .take()
            .ok_or_else(|| error("missing viewer output"))?;
        // Polling a bounded pipe avoids wait_with_output's unlimited allocation.
        let fd = pipe.as_raw_fd();
        // SAFETY: fd belongs to pipe; fcntl retains no pointers.
        unsafe {
            if libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) < 0 {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error("viewer output setup failed"));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut output = Vec::new();
        let result = loop {
            let mut bytes = [0; 1024];
            match pipe.read(&mut bytes) {
                Ok(n) => {
                    output.extend_from_slice(&bytes[..n]);
                    if output.len() > limit {
                        break Err(error("viewer output exceeds budget"));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => break Err(error(e)),
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Drain bytes written immediately before exit.
                    match pipe
                        .take((limit + 1 - output.len()) as u64)
                        .read_to_end(&mut output)
                    {
                        Ok(_) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => break Err(error(e)),
                    }
                    break if status.success() && output.len() <= limit {
                        String::from_utf8(output).map_err(error)
                    } else {
                        Err(error("viewer preparation failed"))
                    };
                }
                Ok(None) => {}
                Err(e) => break Err(error(e)),
            }
            if Instant::now() >= deadline {
                break Err(error("viewer preparation timed out"));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let _ = child.kill();
        let _ = child.wait();
        result
    }
    fn prepare(&self, address: &str) -> Result<(String, u16, String, String), LinuxViewerError> {
        // Numeric route only. Use the existing Linux URL validation shape,
        // without accepting DNS/proxies or an authenticated HTTP fallback.
        let url = url::Url::parse(&format!("http://{address}/serverinfo")).map_err(error)?;
        let host = match url.host() {
            Some(url::Host::Ipv4(h)) => h.to_string(),
            Some(url::Host::Ipv6(h)) => h.to_string(),
            _ => return Err(error("Moonlight route must be numeric")),
        };
        if !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/serverinfo"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(error("invalid Moonlight route"));
        }
        let certificate = self.output(
            &["identity".into(), self.identity.display().to_string()],
            8192,
        )?;
        crate::host::moonlight_certificate::validate_single_pem(&certificate)
            .map_err(|_| error("invalid viewer public certificate"))?;
        let discovery = self.output(
            &[
                "discover".into(),
                host.clone(),
                url.port().unwrap_or(47989).to_string(),
            ],
            256,
        )?;
        let mut lines = discovery.lines();
        let uuid = lines.next().ok_or_else(|| error("missing Sunshine UUID"))?;
        crate::host::moonlight_certificate::validate_host_uuid(uuid)
            .map_err(|_| error("invalid Sunshine UUID"))?;
        let port: u16 = lines
            .next()
            .ok_or_else(|| error("missing HTTPS port"))?
            .parse()
            .map_err(error)?;
        if port == 0 || lines.next().is_some() {
            return Err(error("invalid discovery result"));
        }
        Ok((host, port, uuid.to_owned(), certificate))
    }
    async fn launch(&self, address: &str) -> Result<OwnedChild, LinuxViewerError> {
        let this = self.clone();
        let address = address.to_owned();
        let (host, port, uuid, certificate) =
            tokio::task::spawn_blocking(move || this.prepare(&address))
                .await
                .map_err(error)??;
        let pin = (self.provision)(uuid, certificate)
            .await?
            .server_certificate;
        let this = self.clone();
        let (ready, result) = tokio::sync::oneshot::channel();
        let (stop, stopped) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("webos-viewer-parent".into())
            .spawn(move || {
                let child = this.launch_child(&host, port, &pin);
                // Do not retain session -> child -> parent-thread -> session.
                // Dropping the last viewer must still reap its owned child.
                drop(this);
                let live = child.is_ok();
                if ready.send(child).is_ok() && live {
                    let _ = stopped.recv();
                }
            })
            .map_err(error)?;
        let parent = ParentThread {
            stop: Some(stop),
            thread: Some(thread),
        };
        let mut child = result.await.map_err(error)??;
        child._parent_thread = Some(parent);
        Ok(child)
    }
    fn launch_child(
        &self,
        host: &str,
        port: u16,
        pin: &str,
    ) -> Result<OwnedChild, LinuxViewerError> {
        crate::host::moonlight_certificate::validate_single_pem(pin)
            .map_err(|_| error("invalid broker server certificate"))?;
        let (mut parent, child_channel) = UnixStream::pair().map_err(error)?;
        parent
            .set_read_timeout(Some(self.readiness_timeout))
            .map_err(error)?;
        let fd = child_channel.as_raw_fd();
        let mut command = self.command();
        command
            .args([
                "stream",
                self.identity
                    .to_str()
                    .ok_or_else(|| error("identity path must be UTF-8"))?,
                host,
                &port.to_string(),
            ])
            .stdin(Stdio::piped());
        // SAFETY: fd is held until spawn returns. dup2 reserves descriptor 3 for
        // the private socket and clears CLOEXEC; all other descriptors stay closed.
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(fd, 3) < 0 || libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().map_err(error)?;
        drop(child_channel);
        let callback = self.presenting.clone();
        let visible = std::sync::atomic::AtomicBool::new(false);
        let presenting: Arc<dyn Fn(bool) + Send + Sync> = Arc::new(move |now| {
            if visible.swap(now, std::sync::atomic::Ordering::AcqRel) != now {
                callback(now);
            }
        });
        let mut owned = OwnedChild {
            child,
            _channel: parent.try_clone().map_err(error)?,
            presenting: presenting.clone(),
            input: None,
            _parent_thread: None,
        };
        let mut stdin = owned
            .child
            .stdin
            .take()
            .ok_or_else(|| error("missing pin pipe"))?;
        stdin.write_all(pin.as_bytes()).map_err(error)?;
        drop(stdin);
        let mut ready = [0];
        parent
            .read_exact(&mut ready)
            .map_err(|_| error("viewer exited or readiness timed out"))?;
        if ready != *b"R" || owned.child.try_wait().map_err(error)?.is_some() {
            return Err(error("viewer readiness was not confirmed"));
        }
        // Host-only presentation fact; never a surface treaty field. The
        // embedded composition supplies the one setter owned by the shell edge.
        presenting(true);
        {
            let (baseline, mut receiver) = match (self.input)() {
                Some(source) => {
                    let (baseline, receiver) = source.subscribe_local_gamepads();
                    (baseline, Some(receiver))
                }
                None => (Vec::new(), None),
            };
            let mut socket = parent;
            socket
                .set_write_timeout(Some(Duration::from_millis(100)))
                .map_err(error)?;
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stopped = stop.clone();
            let peer_fd = socket.as_raw_fd();
            let thread = std::thread::Builder::new()
                .name("webos-viewer-input".into())
                .spawn(move || {
                    use crate::native_input::*;
                    let mut devices = std::collections::BTreeSet::new();
                    let mut send = |text: &str| -> std::io::Result<()> {
                        if let Ok(event) = serde_json::from_str::<NativeInputEvent>(text) {
                            match event {
                                NativeInputEvent::DeviceAdded(added) => {
                                    devices.insert(added.device.device_id);
                                }
                                NativeInputEvent::DeviceRemoved(removed) => {
                                    devices.remove(&removed.device_id);
                                }
                                _ => {}
                            }
                        }
                        socket.write_all(text.as_bytes())?;
                        socket.write_all(b"\n")
                    };
                    let mut failed = baseline.iter().any(|text| send(text).is_err());
                    while !failed && !stopped.load(std::sync::atomic::Ordering::Acquire) {
                        // Observe idle child exit independently of far RPC status.
                        // The shared recovery policy still owns every retry.
                        let mut byte = 0u8;
                        // SAFETY: peer_fd is held by socket; byte is valid output.
                        let observed = unsafe {
                            libc::recv(
                                peer_fd,
                                (&mut byte as *mut u8).cast(),
                                1,
                                libc::MSG_PEEK | libc::MSG_DONTWAIT,
                            )
                        };
                        if observed >= 0 {
                            break;
                        } // EOF or unapproved child payload
                        let error = std::io::Error::last_os_error();
                        if error.kind() != std::io::ErrorKind::WouldBlock
                            && error.kind() != std::io::ErrorKind::Interrupted
                        {
                            break;
                        }
                        match receiver
                            .as_mut()
                            .map(|queue| queue.try_recv())
                            .unwrap_or(Err(tokio::sync::mpsc::error::TryRecvError::Empty))
                        {
                            Ok(text) => failed = send(&text).is_err(),
                            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                                std::thread::sleep(Duration::from_millis(2))
                            }
                            Err(_) => break,
                        }
                    }
                    for device_id in devices {
                        let event = NativeInputEvent::DeviceRemoved(NativeInputDeviceRemoved {
                            kind: NativeInputDeviceRemovedKind::DeviceRemoved,
                            device_id,
                        });
                        if let Ok(text) = serde_json::to_vec(&event) {
                            let _ = socket.write_all(&text);
                            let _ = socket.write_all(b"\n");
                        }
                    }
                    let _ = socket.shutdown(std::net::Shutdown::Write);
                    presenting(false);
                })
                .map_err(error)?;
            owned.input = Some((stop, thread));
        }
        Ok(owned)
    }
}
impl LinuxStreamViewer for WebosViewer {
    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            if launch.len() != 32
                || !launch
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(error("invalid exact launch"));
            }
            let mut session = self.session.lock().await;
            if session.as_ref().is_some_and(|s| s.child.is_some()) {
                return Err(error("viewer already active"));
            }
            let child = self.launch(address).await?;
            log::info!("webOS viewer ready for exact launch {launch}");
            *session = Some(Session {
                launch: launch.into(),
                address: address.into(),
                child: Some(child),
                backgrounded: false,
            });
            Ok(())
        })
    }
    fn status<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(async move {
            let mut guard = self.session.lock().await;
            let Some(s) = guard.as_mut().filter(|s| s.launch == launch) else {
                return Ok(LinuxViewerStatus::Stopped);
            };
            if s.backgrounded {
                return Ok(LinuxViewerStatus::Backgrounded);
            }
            if let Some(child) = &mut s.child {
                if child.child.try_wait().map_err(error)?.is_none() {
                    return Ok(LinuxViewerStatus::Running);
                }
            }
            Self::release(s.child.take()).await?;
            // Shared upstreams::recover_viewer decides reconnection for every
            // platform. No TV-specific retry/give-up/end policy (#390).
            Ok(LinuxViewerStatus::Stopped)
        })
    }
    fn background<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let mut guard = self.session.lock().await;
            let s = guard
                .as_mut()
                .filter(|s| s.launch == launch)
                .ok_or_else(|| error("stale viewer launch"))?;
            // Reap/release media before confirming Backgrounded. This retained
            // exact state prevents recover_viewer from restarting after Leave.
            Self::release(s.child.take()).await?;
            s.backgrounded = true;
            Ok(())
        })
    }
    fn resume_and_focus<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let mut guard = self.session.lock().await;
            let s = guard
                .as_mut()
                .filter(|s| s.launch == launch)
                .ok_or_else(|| error("stale viewer launch"))?;
            if let Some(child) = &mut s.child {
                if child.child.try_wait().map_err(error)?.is_some() {
                    Self::release(s.child.take()).await?;
                }
            }
            if s.child.is_none() {
                s.child = Some(self.launch(&s.address).await?);
            }
            s.backgrounded = false;
            Ok(())
        })
    }
    fn stop<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let mut guard = self.session.lock().await;
            if guard.as_ref().is_some_and(|s| s.launch == launch) {
                let child = guard.as_mut().and_then(|s| s.child.take());
                Self::release(child).await?;
                guard.take();
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "webos_viewer_tests.rs"]
mod tests;
