//! Inactive until the real @korri:viewer plugin supplies process effects.
//! Units start/clean up processes; only the typed socket supplies observations.
use super::{LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus, ViewerSnapshot};
use crate::launcher::plugin_launch::PluginDisplay;
use futures::future::BoxFuture;
use korri_viewer_treaty::{
    ViewerCommand as C, ViewerDisplay, ViewerEvent as E, ViewerMessage as M, ViewerReply as R,
    ViewerRequest, ViewerStatus as S, MAX_MESSAGE_BYTES,
};
use std::{
    collections::HashMap,
    os::fd::AsRawFd,
    os::unix::{
        fs::PermissionsExt,
        fs::{FileTypeExt, MetadataExt},
    },
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{unix::OwnedWriteHalf, UnixStream},
    sync::{mpsc, Mutex, Notify},
    time::{timeout, Instant},
};

const DEADLINE: Duration = Duration::from_secs(30);
const ALIVE: u8 = 0;
const STOPPED: u8 = 1;
const LOST: u8 = 2;
fn error(message: impl ToString) -> LinuxViewerError {
    LinuxViewerError::new(message.to_string())
}

/// The plugin slice supplies these using the existing viewer-unit launcher,
/// broker provisioning and compositor focus. No plugin metadata is guessed here.
pub type StartSocketViewer = Arc<
    dyn Fn(
            String,
            String,
            Option<PluginDisplay>,
        ) -> BoxFuture<'static, Result<PathBuf, LinuxViewerError>>
        + Send
        + Sync,
>;
pub type FocusSocketViewer =
    Arc<dyn Fn(String, bool) -> BoxFuture<'static, Result<(), LinuxViewerError>> + Send + Sync>;
pub type StopSocketViewer =
    Arc<dyn Fn(String) -> BoxFuture<'static, Result<(), LinuxViewerError>> + Send + Sync>;

pub struct SocketLinuxStreamViewer {
    display: Option<PluginDisplay>,
    expected_viewer_uid: u32,
    start_process: StartSocketViewer,
    focus: FocusSocketViewer,
    stop_process: StopSocketViewer,
    connections: Mutex<HashMap<String, Arc<Connection>>>,
    operation: Mutex<()>,
    changed: Arc<Notify>,
}
struct Connection {
    launch: String,
    exchange: Mutex<(OwnedWriteHalf, mpsc::Receiver<Result<R, LinuxViewerError>>)>,
    terminal: Arc<AtomicU8>,
}
// Dropping an in-flight request (including caller cancellation) invalidates the
// ordered exchange. A late reply must never confirm a different mutation.
struct InFlight<'a> {
    terminal: &'a AtomicU8,
    complete: bool,
    socket: std::os::fd::RawFd,
}
impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.terminal.store(LOST, Ordering::Release);
            // The exchange owns this descriptor until after this guard drops.
            // Closing the control connection tells the viewer to stop, too.
            unsafe {
                libc::shutdown(self.socket, libc::SHUT_RDWR);
            }
        }
    }
}

impl SocketLinuxStreamViewer {
    pub fn new(
        display: Option<PluginDisplay>,
        expected_viewer_uid: u32,
        start_process: StartSocketViewer,
        focus: FocusSocketViewer,
        stop_process: StopSocketViewer,
    ) -> Self {
        Self {
            display,
            expected_viewer_uid,
            start_process,
            focus,
            stop_process,
            connections: Mutex::new(HashMap::new()),
            operation: Mutex::new(()),
            changed: Arc::new(Notify::new()),
        }
    }
    async fn connection(&self, launch: &str) -> Result<Arc<Connection>, LinuxViewerError> {
        self.connections
            .lock()
            .await
            .get(launch)
            .cloned()
            .ok_or_else(|| error("viewer launch has no lifecycle connection"))
    }
    async fn command(&self, launch: &str, command: C) -> Result<R, LinuxViewerError> {
        self.connection(launch).await?.request(command).await
    }
    async fn mutate(&self, launch: &str, command: C) -> Result<(), LinuxViewerError> {
        let _guard = self.operation.lock().await;
        let connection = self.connection(launch).await?;
        if matches!(command, C::Background | C::ResumeAndFocus) {
            if connection.terminal.load(Ordering::Acquire) != ALIVE {
                return Err(error("viewer is not running"));
            }
            timeout(
                DEADLINE,
                (self.focus)(launch.into(), matches!(command, C::Background)),
            )
            .await
            .map_err(|_| error("viewer focus deadline exceeded"))??;
        }
        ok(connection.request(command).await?)
    }
}
fn ok(reply: R) -> Result<(), LinuxViewerError> {
    match reply {
        R::Ok => Ok(()),
        R::Error { message } => Err(error(message)),
        _ => Err(error("unexpected viewer reply")),
    }
}
fn display(d: PluginDisplay) -> ViewerDisplay {
    ViewerDisplay {
        width: d.width,
        height: d.height,
    }
}
fn status(s: S) -> LinuxViewerStatus {
    match s {
        S::Running => LinuxViewerStatus::Running,
        S::Backgrounded => LinuxViewerStatus::Backgrounded,
        S::Stopped => LinuxViewerStatus::Stopped,
    }
}

impl Connection {
    async fn connect(
        path: PathBuf,
        launch: String,
        changed: Arc<Notify>,
        expected_viewer_uid: u32,
    ) -> Result<Arc<Self>, LinuxViewerError> {
        let until = Instant::now() + DEADLINE;
        let socket = loop {
            match UnixStream::connect(&path).await {
                Ok(socket) => break socket,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) && Instant::now() < until =>
                {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                Err(e) => return Err(error(e)),
            }
        };
        let metadata = std::fs::symlink_metadata(path).map_err(error)?;
        if !metadata.file_type().is_socket()
            // Named controller ACLs use the group-class mask. Other users
            // still get no access; peer identity comes from the launcher.
            || metadata.permissions().mode() & 0o007 != 0
            || metadata.uid() != expected_viewer_uid
            || socket.peer_cred().map_err(error)?.uid() != expected_viewer_uid
        {
            return Err(error(
                "viewer socket owner and peer must match the expected viewer UID",
            ));
        }
        let (read, write) = socket.into_split();
        let (send, receive) = mpsc::channel(1);
        let terminal = Arc::new(AtomicU8::new(ALIVE));
        let observed = terminal.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(read);
            loop {
                let message = async {
                    let mut bytes = Vec::new();
                    let length = (&mut reader)
                        .take((MAX_MESSAGE_BYTES + 1) as u64)
                        .read_until(b'\n', &mut bytes)
                        .await
                        .map_err(error)?;
                    if length == 0 {
                        return Err(error("viewer lifecycle socket closed"));
                    }
                    if length > MAX_MESSAGE_BYTES || bytes.last() != Some(&b'\n') {
                        return Err(error("oversized or incomplete viewer message"));
                    }
                    serde_json::from_slice::<M>(&bytes).map_err(error)
                }
                .await;
                match message {
                    Ok(M::Event(event)) => {
                        if matches!(event, E::Status(S::Stopped)) {
                            observed.store(STOPPED, Ordering::Release);
                        }
                        changed.notify_one();
                    }
                    Ok(M::Reply(reply)) => {
                        if send.send(Ok(reply)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        if observed.load(Ordering::Acquire) != STOPPED {
                            observed.store(LOST, Ordering::Release);
                        }
                        let _ = send.send(Err(e)).await;
                        changed.notify_one();
                        break;
                    }
                }
            }
        });
        Ok(Arc::new(Self {
            launch,
            exchange: Mutex::new((write, receive)),
            terminal,
        }))
    }
    async fn request(&self, command: C) -> Result<R, LinuxViewerError> {
        let result = timeout(DEADLINE, async {
            let mut exchange = self.exchange.lock().await;
            if self.terminal.load(Ordering::Acquire) == LOST {
                return Err(error("viewer lifecycle connection lost"));
            }
            let request = ViewerRequest {
                launch_id: self.launch.clone(),
                command,
            };
            let mut bytes = serde_json::to_vec(&request).map_err(error)?;
            bytes.push(b'\n');
            if bytes.len() > MAX_MESSAGE_BYTES {
                return Err(error("oversized viewer request"));
            }
            let mut flight = InFlight {
                terminal: &self.terminal,
                complete: false,
                socket: exchange.0.as_ref().as_raw_fd(),
            };
            exchange.0.write_all(&bytes).await.map_err(error)?;
            let reply = exchange
                .1
                .recv()
                .await
                .ok_or_else(|| error("viewer lifecycle reader stopped"))??;
            flight.complete = true;
            match reply {
                R::Error { message } => Err(error(message)),
                reply => Ok(reply),
            }
        })
        .await;
        match result {
            Ok(result) => result,
            Err(_) => {
                // Without request IDs, a late reply cannot be matched safely.
                // Poison rather than allow it to satisfy a subsequent command.
                self.terminal.store(LOST, Ordering::Release);
                Err(error("viewer lifecycle reply deadline exceeded"))
            }
        }
    }
}

impl LinuxStreamViewer for SocketLinuxStreamViewer {
    fn change_signal(&self) -> Option<Arc<Notify>> {
        Some(self.changed.clone())
    }
    fn display(&self) -> Option<PluginDisplay> {
        self.display.clone()
    }
    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
        _size: Option<PluginDisplay>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let _guard = self.operation.lock().await;
            super::production::validate_launch(launch)?;
            super::production::server_url(address)?;
            if self.connections.lock().await.contains_key(launch) {
                return Err(error("viewer launch already started"));
            }
            let size = self.display.clone();
            let result = async {
                let path = timeout(
                    DEADLINE,
                    (self.start_process)(launch.into(), address.into(), size.clone()),
                )
                .await
                .map_err(|_| error("viewer process start deadline exceeded"))??;
                let connection = Connection::connect(
                    path,
                    launch.into(),
                    self.changed.clone(),
                    self.expected_viewer_uid,
                )
                .await?;
                ok(connection
                    .request(C::Start {
                        address: address.into(),
                        display: size.map(display),
                    })
                    .await?)?;
                timeout(DEADLINE, (self.focus)(launch.into(), false))
                    .await
                    .map_err(|_| error("viewer focus deadline exceeded"))??;
                self.connections
                    .lock()
                    .await
                    .insert(launch.into(), connection);
                Ok(())
            }
            .await;
            if let Err(start_error) = result {
                // An unconnected or unfocused process must not survive failure.
                timeout(DEADLINE, (self.stop_process)(launch.into()))
                    .await
                    .map_err(|_| error("viewer failure cleanup deadline exceeded"))??;
                return Err(start_error);
            }
            Ok(())
        })
    }
    fn status<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(async move {
            let _guard = self.operation.lock().await;
            super::production::validate_launch(launch)?;
            let Some(connection) = self.connections.lock().await.get(launch).cloned() else {
                return Ok(LinuxViewerStatus::Stopped);
            };
            if connection.terminal.load(Ordering::Acquire) == STOPPED {
                self.connections.lock().await.remove(launch);
                return Ok(LinuxViewerStatus::Stopped);
            }
            match connection.request(C::Status).await? {
                R::Status(s) => Ok(status(s)),
                _ => Err(error("unexpected viewer status reply")),
            }
        })
    }
    fn background<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.mutate(launch, C::Background))
    }
    fn resume_and_focus<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.mutate(launch, C::ResumeAndFocus))
    }
    fn stop<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let _guard = self.operation.lock().await;
            super::production::validate_launch(launch)?;
            let Some(connection) = self.connections.lock().await.get(launch).cloned() else {
                return Ok(());
            };
            let result = if connection.terminal.load(Ordering::Acquire) == STOPPED {
                Ok(())
            } else {
                ok(connection.request(C::Stop).await?)
            };
            if result.is_ok() {
                self.connections.lock().await.remove(launch);
            }
            result
        })
    }
    fn snapshot<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<Option<ViewerSnapshot>, LinuxViewerError>> {
        Box::pin(async move {
            let _guard = self.operation.lock().await;
            super::production::validate_launch(launch)?;
            let connection = self.connections.lock().await.get(launch).cloned();
            if connection
                .as_ref()
                .is_none_or(|c| c.terminal.load(Ordering::Acquire) == STOPPED)
            {
                self.connections.lock().await.remove(launch);
                return Ok(Some(ViewerSnapshot {
                    connected: Some(false),
                    ..Default::default()
                }));
            }
            match connection
                .expect("active connection checked above")
                .request(C::Snapshot)
                .await?
            {
                R::Snapshot { snapshot } => Ok(snapshot.map(|s| ViewerSnapshot {
                    connected: s.connected,
                    shape: s.shape.map(|shape| super::ViewerShapeSnapshot {
                        connection: shape.connection,
                        screen: PluginDisplay {
                            width: shape.screen.width,
                            height: shape.screen.height,
                        },
                        encoded: PluginDisplay {
                            width: shape.encoded.width,
                            height: shape.encoded.height,
                        },
                        ready: shape.ready,
                    }),
                    sampled_at_ms: s.sampled_at_ms,
                    sampled_at: None,
                    battery_percent: s.battery_percent,
                    stream_health: s.stream_health,
                    controls: s.controls,
                })),
                _ => Err(error("unexpected viewer snapshot reply")),
            }
        })
    }
    fn resize<'a>(
        &'a self,
        launch: &'a str,
        connection: &'a str,
        size: PluginDisplay,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            ok(self
                .command(
                    launch,
                    C::Resize {
                        connection: connection.into(),
                        display: display(size),
                    },
                )
                .await?)
        })
    }
    fn apply<'a>(
        &'a self,
        launch: &'a str,
        control: &'a str,
        value: Option<korri_surface_treaty::SurfaceGameplayControlValue>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            ok(self
                .command(
                    launch,
                    C::Apply {
                        control: control.into(),
                        value,
                    },
                )
                .await?)
        })
    }
    fn notice<'a>(
        &'a self,
        launch: &'a str,
        message: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            ok(self
                .command(
                    launch,
                    C::Notice {
                        message: message.into(),
                    },
                )
                .await?)
        })
    }
}
