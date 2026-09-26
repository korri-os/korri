use korri_inputd::input_seat::{
    validate_launch_id, GamepadState, MirrorOutcome, SeatBackend, SeatResetOutcome, SeatRuntime,
    SeatSpec, MAX_MIRROR_FRAME_BYTES,
};
use korri_inputd::input_seat_uinput::UinputSeatBackend;
use serde::Serialize;
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::Write,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicBool, Ordering},
};

const CONTROL_VERSION: u8 = 1;
const CONTROL_START: u8 = 1;
const CONTROL_STOP: u8 = 2;
const CONTROL_RESET: u8 = 3;
const CONTROL_BYTES: usize = 34;
const REPLY_BYTES: usize = 3;
const REASON_NONE: u8 = 0;
const REASON_INVALID: u8 = 1;
const REASON_PEER: u8 = 2;
const REASON_ACTIVE: u8 = 3;
const REASON_BACKEND: u8 = 4;
const REASON_STALE: u8 = 5;
const CONTROL_IO_TIMEOUT_MS: i32 = 2_000;
const ACTIVE_CONTROL_IO_TIMEOUT_MS: i32 = 50;
const MIRROR_IO_TIMEOUT_MS: i32 = 20;
static STOPPING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
struct Options {
    runtime_dir: PathBuf,
    control_uid: u32,
    control_gid: u32,
    sunshine_uid: u32,
    sunshine_gid: u32,
    event_gid: u32,
    dry_run: bool,
}

#[derive(Default)]
struct DryBackend;
impl SeatBackend for DryBackend {
    fn create(&mut self, _spec: &SeatSpec) -> Result<(), String> {
        Ok(())
    }
    fn write_state(&mut self, _slot: u8, _state: GamepadState) -> Result<(), String> {
        Ok(())
    }
    fn destroy(&mut self, _slot: u8) -> Result<(), String> {
        Ok(())
    }
}

struct Listener {
    fd: OwnedFd,
    path: PathBuf,
}
impl Listener {
    fn bind(path: &Path, mode: u32, gid: u32) -> Result<Self, String> {
        remove_runtime_object(path, true)?;
        let fd = socket_seqpacket()?;
        bind_unix(fd.as_raw_fd(), path)?;
        if unsafe { libc::listen(fd.as_raw_fd(), 8) } != 0 {
            return Err(last("listen"));
        }
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(display)?;
        chgrp(path, gid)?;
        Ok(Self {
            fd,
            path: path.to_owned(),
        })
    }
    fn accept(&self) -> Result<OwnedFd, String> {
        let fd = unsafe {
            libc::accept4(
                self.fd.as_raw_fd(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            )
        };
        if fd < 0 {
            Err(last("accept"))
        } else {
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = remove_runtime_object(&self.path, true);
    }
}

#[derive(Serialize)]
struct ActiveLaunch<'a> {
    #[serde(rename = "launchId")]
    launch_id: &'a str,
    generation: u64,
    #[serde(rename = "mirrorToken")]
    mirror_token: &'a str,
}

fn main() -> ExitCode {
    match parse_options().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("korri-input-seat-receiver: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_options() -> Result<Options, String> {
    let mut args = std::env::args_os().skip(1);
    let mut runtime_dir = None;
    let mut control_uid = None;
    let mut control_gid = None;
    let mut sunshine_uid = None;
    let mut sunshine_gid = None;
    let mut event_gid = None;
    let mut dry_run = false;
    while let Some(flag) = args.next() {
        match flag.to_str() {
            Some("--runtime-dir") => runtime_dir = args.next().map(PathBuf::from),
            Some("--control-uid") => control_uid = Some(number(args.next())?),
            Some("--control-gid") => control_gid = Some(number(args.next())?),
            Some("--sunshine-uid") => sunshine_uid = Some(number(args.next())?),
            Some("--sunshine-gid") => sunshine_gid = Some(number(args.next())?),
            Some("--event-gid") => event_gid = Some(number(args.next())?),
            Some("--dry-run") => dry_run = true,
            _ => return Err("invalid receiver option".into()),
        }
    }
    Ok(Options {
        runtime_dir: runtime_dir.ok_or("runtime directory is required")?,
        control_uid: control_uid.ok_or("control UID is required")?,
        control_gid: control_gid.ok_or("control GID is required")?,
        sunshine_uid: sunshine_uid.ok_or("Sunshine UID is required")?,
        sunshine_gid: sunshine_gid.ok_or("Sunshine GID is required")?,
        event_gid: event_gid.ok_or("event GID is required")?,
        dry_run,
    })
}

fn number(value: Option<std::ffi::OsString>) -> Result<u32, String> {
    let text = value
        .and_then(|v| v.into_string().ok())
        .ok_or("numeric option is missing")?;
    let number = text
        .parse::<u32>()
        .map_err(|_| "numeric option is invalid")?;
    if number.to_string() == text {
        Ok(number)
    } else {
        Err("numeric option is not canonical".into())
    }
}

fn run(options: Options) -> Result<(), String> {
    install_signal_handlers()?;
    validate_runtime_directory(&options.runtime_dir, options.dry_run)?;
    let control_path = options.runtime_dir.join("control.sock");
    let mirror_path = options.runtime_dir.join("sunshine-input-seat.sock");
    let sidecar_path = options.runtime_dir.join("sunshine-active-launch.json");
    remove_runtime_object(&mirror_path, true)?;
    remove_runtime_object(&sidecar_path, false)?;
    let backend: Box<dyn SeatBackend> = if options.dry_run {
        Box::new(DryBackend)
    } else {
        Box::new(UinputSeatBackend::new(options.event_gid))
    };
    // The receiver owns all four devices for its whole lifetime. A lease
    // grants launch-scoped mirror authority, not device lifetime.
    let mut runtime = SeatRuntime::boot(backend)?;
    let control = Listener::bind(&control_path, 0o660, options.control_gid)?;
    let mut generation = 0u64;
    while !STOPPING.load(Ordering::Relaxed) {
        let connection = match poll_accept(&control, 250)? {
            Some(value) => value,
            None => continue,
        };
        if peer_credentials(connection.as_raw_fd())? != (options.control_uid, options.control_gid) {
            let _ = receive_packet(
                connection.as_raw_fd(),
                CONTROL_BYTES + 1,
                ACTIVE_CONTROL_IO_TIMEOUT_MS,
            );
            let _ = send_reply(connection.as_raw_fd(), 1, REASON_PEER);
            continue;
        }
        let Some(request) = receive_control(connection.as_raw_fd(), CONTROL_IO_TIMEOUT_MS)? else {
            let _ = send_reply(connection.as_raw_fd(), 1, REASON_INVALID);
            continue;
        };
        if request.operation != CONTROL_START {
            let _ = send_reply(connection.as_raw_fd(), 1, REASON_INVALID);
            continue;
        }
        generation = generation
            .checked_add(1)
            .ok_or("input-seat generation overflow")?;
        if let Err(error) = serve_launch(
            &options,
            &control,
            connection,
            request.launch_id,
            generation,
            &mirror_path,
            &sidecar_path,
            &mut runtime,
        ) {
            // A failed cleanup must not leave held input on devices reused
            // by the next launch. Exit so the receiver drops the devices.
            return Err(format!("launch seat service failed: {error}"));
        }
    }
    remove_runtime_object(&mirror_path, true)?;
    remove_runtime_object(&sidecar_path, false)?;
    Ok(())
}

struct ControlRequest {
    operation: u8,
    launch_id: String,
}

fn serve_launch(
    options: &Options,
    control_listener: &Listener,
    lease: OwnedFd,
    launch_id: String,
    generation: u64,
    mirror_path: &Path,
    sidecar_path: &Path,
    runtime: &mut SeatRuntime<Box<dyn SeatBackend>>,
) -> Result<(), String> {
    let token = random_token()?;
    runtime.bind(&launch_id, &token)?;
    let active_result = (|| {
        let mirror = Listener::bind(mirror_path, 0o660, options.sunshine_gid)?;
        write_sidecar(
            sidecar_path,
            options.sunshine_gid,
            &launch_id,
            generation,
            &token,
        )?;
        send_reply(lease.as_raw_fd(), 0, REASON_NONE)?;
        serve_active_launch(
            options,
            control_listener,
            &lease,
            &mirror,
            &launch_id,
            runtime,
        )
    })();
    let cleanup_result = cleanup_launch(sidecar_path, runtime);
    match (active_result, cleanup_result) {
        (Ok(LaunchExit::StopRequested), Ok(())) => send_reply(lease.as_raw_fd(), 0, REASON_NONE),
        (Ok(LaunchExit::LeaseEnded), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!("{error}; input-seat cleanup failed: {cleanup}")),
    }
}

enum LaunchExit {
    StopRequested,
    LeaseEnded,
}

fn serve_active_launch(
    options: &Options,
    control_listener: &Listener,
    lease: &OwnedFd,
    mirror: &Listener,
    launch_id: &str,
    runtime: &mut SeatRuntime<Box<dyn SeatBackend>>,
) -> Result<LaunchExit, String> {
    while !STOPPING.load(Ordering::Relaxed) {
        let mut fds = [
            libc::pollfd {
                fd: lease.as_raw_fd(),
                events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
                revents: 0,
            },
            libc::pollfd {
                fd: control_listener.fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: mirror.fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 250) };
        if result < 0 {
            if errno() == libc::EINTR {
                continue;
            }
            return Err(last("poll"));
        }
        if fds[1].revents & libc::POLLIN != 0 {
            if let Ok(extra) = control_listener.accept() {
                let _ = receive_control(extra.as_raw_fd(), ACTIVE_CONTROL_IO_TIMEOUT_MS);
                let _ = send_reply(extra.as_raw_fd(), 1, REASON_ACTIVE);
            }
        }
        if fds[2].revents & libc::POLLIN != 0 {
            if let Ok(frame) = mirror.accept() {
                if peer_credentials(frame.as_raw_fd())?
                    == (options.sunshine_uid, options.sunshine_gid)
                {
                    if let Ok(packet) = receive_packet(
                        frame.as_raw_fd(),
                        MAX_MIRROR_FRAME_BYTES + 1,
                        MIRROR_IO_TIMEOUT_MS,
                    ) {
                        if packet.len() <= MAX_MIRROR_FRAME_BYTES
                            && runtime.accept(&packet, monotonic_ms())
                                == MirrorOutcome::BackendFailed
                        {
                            return Err("input-seat backend write failed".into());
                        }
                    }
                }
            }
        }
        runtime.expire_stale(monotonic_ms())?;
        if fds[0].revents & libc::POLLIN != 0 {
            let Some(request) = receive_control(lease.as_raw_fd(), ACTIVE_CONTROL_IO_TIMEOUT_MS)?
            else {
                return Ok(LaunchExit::LeaseEnded);
            };
            if request.operation == CONTROL_STOP && request.launch_id == launch_id {
                return Ok(LaunchExit::StopRequested);
            }
            if request.operation == CONTROL_RESET {
                let (status, reason) = match runtime.reset(&request.launch_id) {
                    SeatResetOutcome::Accepted => (0, REASON_NONE),
                    SeatResetOutcome::StaleLaunch => (1, REASON_STALE),
                    SeatResetOutcome::BackendFailed => (1, REASON_BACKEND),
                };
                let _ = send_reply(lease.as_raw_fd(), status, reason);
                continue;
            }
            let reason = if request.operation == CONTROL_STOP {
                REASON_STALE
            } else {
                REASON_INVALID
            };
            let _ = send_reply(lease.as_raw_fd(), 1, reason);
        }
        if fds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            return Ok(LaunchExit::LeaseEnded);
        }
    }
    Ok(LaunchExit::LeaseEnded)
}

fn cleanup_launch(
    sidecar: &Path,
    runtime: &mut SeatRuntime<Box<dyn SeatBackend>>,
) -> Result<(), String> {
    let sidecar_result = remove_runtime_object(sidecar, false);
    let runtime_result = runtime.unbind();
    sidecar_result.and(runtime_result)
}

fn receive_control(fd: RawFd, timeout_ms: i32) -> Result<Option<ControlRequest>, String> {
    let bytes = receive_packet(fd, CONTROL_BYTES + 1, timeout_ms)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() != CONTROL_BYTES || bytes[0] != CONTROL_VERSION {
        return Ok(None);
    }
    let launch_id = std::str::from_utf8(&bytes[2..])
        .map_err(display)?
        .to_owned();
    if validate_launch_id(&launch_id).is_err() {
        return Ok(None);
    }
    Ok(Some(ControlRequest {
        operation: bytes[1],
        launch_id,
    }))
}

fn receive_packet(fd: RawFd, capacity: usize, timeout_ms: i32) -> Result<Vec<u8>, String> {
    wait_ready(fd, libc::POLLIN, timeout_ms)?;
    let mut bytes = vec![0u8; capacity];
    let count = unsafe {
        libc::recv(
            fd,
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            libc::MSG_TRUNC | libc::MSG_DONTWAIT,
        )
    };
    if count < 0 {
        return Err(last("recv"));
    }
    let count = count as usize;
    if count > bytes.len() {
        return Ok(vec![0; bytes.len()]);
    }
    bytes.truncate(count);
    Ok(bytes)
}

fn send_reply(fd: RawFd, status: u8, reason: u8) -> Result<(), String> {
    wait_ready(fd, libc::POLLOUT, ACTIVE_CONTROL_IO_TIMEOUT_MS)?;
    let bytes = [CONTROL_VERSION, status, reason];
    let count = unsafe {
        libc::send(
            fd,
            bytes.as_ptr().cast(),
            REPLY_BYTES,
            libc::MSG_NOSIGNAL | libc::MSG_DONTWAIT,
        )
    };
    if count == REPLY_BYTES as isize {
        Ok(())
    } else {
        Err(last("send"))
    }
}

fn socket_seqpacket() -> Result<OwnedFd, String> {
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        Err(last("socket"))
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

fn bind_unix(fd: RawFd, path: &Path) -> Result<(), String> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.is_empty() || bytes.len() >= 108 {
        return Err("Unix socket path is invalid".into());
    }
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, source) in address.sun_path.iter_mut().zip(bytes.iter().copied()) {
        *target = source as libc::c_char;
    }
    let length = (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t;
    let result = unsafe { libc::bind(fd, (&address as *const libc::sockaddr_un).cast(), length) };
    if result == 0 {
        Ok(())
    } else {
        Err(last("bind"))
    }
}

fn poll_accept(listener: &Listener, timeout: i32) -> Result<Option<OwnedFd>, String> {
    let mut pollfd = libc::pollfd {
        fd: listener.fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut pollfd, 1, timeout) };
    if result < 0 {
        if errno() == libc::EINTR {
            return Ok(None);
        }
        return Err(last("poll"));
    }
    if result == 0 || pollfd.revents & libc::POLLIN == 0 {
        Ok(None)
    } else {
        match listener.accept() {
            Ok(connection) => Ok(Some(connection)),
            Err(_) if errno() == libc::EAGAIN || errno() == libc::EWOULDBLOCK => Ok(None),
            Err(error) => Err(error),
        }
    }
}

fn wait_ready(fd: RawFd, events: i16, timeout_ms: i32) -> Result<(), String> {
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
                return Err("socket closed before the operation completed".into());
            }
            return Err("socket reported an unexpected event".into());
        }
        if result == 0 {
            return Err("socket operation timed out".into());
        }
        if errno() != libc::EINTR {
            return Err(last("poll"));
        }
    }
}

fn peer_credentials(fd: RawFd) -> Result<(u32, u32), String> {
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

fn write_sidecar(
    path: &Path,
    gid: u32,
    launch_id: &str,
    generation: u64,
    token: &str,
) -> Result<(), String> {
    remove_runtime_object(path, false)?;
    let temporary = path.with_extension(format!("json.next.{}", std::process::id()));
    remove_runtime_object(&temporary, false)?;
    let payload = serde_json::to_vec(&ActiveLaunch {
        launch_id,
        generation,
        mirror_token: token,
    })
    .map_err(display)?;
    if payload.len() + 1 > 4096 {
        return Err("sidecar is too large".into());
    }

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(display)?;
        file.write_all(&payload)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(display)?;
        chgrp(&temporary, gid)?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o640)).map_err(display)?;
        file.sync_all().map_err(display)?;
        fs::rename(&temporary, path).map_err(display)?;
        sync_parent(path)
    })();

    if let Err(error) = result {
        let _ = remove_runtime_object(&temporary, false);
        let _ = remove_runtime_object(path, false);
        return Err(error);
    }
    Ok(())
}

fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    let mut offset = 0usize;
    while offset < bytes.len() {
        let count = unsafe {
            libc::getrandom(bytes[offset..].as_mut_ptr().cast(), bytes.len() - offset, 0)
        };
        if count > 0 {
            offset += count as usize;
        } else if count == 0 {
            return Err("getrandom returned no data".into());
        } else if errno() != libc::EINTR {
            return Err(last("getrandom"));
        }
    }
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").map_err(display)?;
    }
    Ok(output)
}

fn remove_runtime_object(path: &Path, socket: bool) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink()
                || (socket && !metadata.file_type().is_socket())
                || (!socket && !metadata.file_type().is_file())
            {
                return Err(format!("unsafe runtime object: {}", path.display()));
            }
            fs::remove_file(path).map_err(display)?;
            sync_parent(path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn sync_parent(path: &Path) -> Result<(), String> {
    File::open(path.parent().ok_or("runtime object parent is absent")?)
        .and_then(|directory| directory.sync_all())
        .map_err(display)
}

fn validate_runtime_directory(path: &Path, dry_run: bool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(display)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("runtime path is not a directory".into());
    }
    if !dry_run && (metadata.uid() != 0 || metadata.permissions().mode() & 0o777 != 0o711) {
        return Err("runtime directory must be root-owned with mode 0711".into());
    }
    Ok(())
}

fn chgrp(path: &Path, gid: u32) -> Result<(), String> {
    let value = CString::new(path.as_os_str().as_bytes()).map_err(display)?;
    if unsafe { libc::chown(value.as_ptr(), u32::MAX, gid) } == 0 {
        Ok(())
    } else {
        Err(last("chown"))
    }
}

fn monotonic_ms() -> u64 {
    let mut time: libc::timespec = unsafe { std::mem::zeroed() };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
        return 0;
    }
    time.tv_sec as u64 * 1000 + time.tv_nsec as u64 / 1_000_000
}

extern "C" fn stop_signal(_: libc::c_int) {
    STOPPING.store(true, Ordering::Relaxed);
}
fn install_signal_handlers() -> Result<(), String> {
    if unsafe {
        libc::signal(
            libc::SIGTERM,
            stop_signal as *const () as libc::sighandler_t,
        )
    } == libc::SIG_ERR
        || unsafe { libc::signal(libc::SIGINT, stop_signal as *const () as libc::sighandler_t) }
            == libc::SIG_ERR
    {
        Err(last("signal"))
    } else {
        Ok(())
    }
}
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}
fn last(operation: &str) -> String {
    format!("{operation} failed: {}", std::io::Error::last_os_error())
}
fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}
