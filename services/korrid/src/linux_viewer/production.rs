//! Moonlight effects stay at this edge. Admission comes from the plugin host;
//! process lifetime and focus come from the existing exact-launch authorities.
use super::{LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus};
use crate::host::compositor_focus::{
    focused_ownership, select_focus_target, CompositorControl, FocusTarget,
    SwaymsgCompositorControl,
};
use crate::host::{LaunchSandbox, LaunchUnitBackend, LaunchUnitState, SystemdLaunchUnitBackend};
use futures::future::BoxFuture;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

pub type CertificateProvision = Arc<
    dyn Fn(
            String,
            String,
        )
            -> BoxFuture<'static, Result<crate::MoonlightCertificateProvisioned, LinuxViewerError>>
        + Send
        + Sync,
>;
pub type LocalSeats =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Vec<PathBuf>, LinuxViewerError>> + Send + Sync>;

/// The callbacks use the registry's authenticated certificate broker and inputd's
/// occupied local seats. No peer credentials or controller state are duplicated.
pub fn production_viewer(
    provision: CertificateProvision,
    local_seats: LocalSeats,
) -> Arc<dyn LinuxStreamViewer> {
    let viewer = ProductionLinuxStreamViewer {
        provision,
        local_seats,
        resolve_files: Arc::new(installed_files),
        units: Arc::new(SystemdLaunchUnitBackend::default().for_stream_viewer()),
        swaymsg: std::env::var_os("KORRID_SWAYMSG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/run/current-system/sw/bin/swaymsg")),
        sway_socket: std::env::var_os("KORRID_COMPOSITOR_CONTROL_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_default(),
        portal_ids: std::env::var("KORRID_NEVER_FOCUS_APP_IDS")
            .unwrap_or_default()
            .split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect(),
        keydir: PathBuf::from("/home/korri/.config/moonlight"),
        operation: Arc::new(tokio::sync::Mutex::new(())),
    };
    Arc::new(viewer)
}

#[derive(Clone)]
struct ProductionLinuxStreamViewer {
    provision: CertificateProvision,
    local_seats: LocalSeats,
    resolve_files: Arc<dyn Fn() -> Result<MoonlightFiles, LinuxViewerError> + Send + Sync>,
    units: Arc<dyn LaunchUnitBackend>,
    swaymsg: PathBuf,
    sway_socket: PathBuf,
    portal_ids: Vec<String>,
    keydir: PathBuf,
    operation: Arc<tokio::sync::Mutex<()>>,
}

fn unit_error(value: crate::host::LaunchUnitError) -> LinuxViewerError {
    error(value.message)
}

fn error(message: impl ToString) -> LinuxViewerError {
    LinuxViewerError::new(message.to_string())
}

#[derive(Clone)]
struct MoonlightFiles {
    executable: PathBuf,
    mapping: PathBuf,
    bootstrap: PathBuf,
}

fn installed_files() -> Result<MoonlightFiles, LinuxViewerError> {
    let packages = crate::plugin_installation::read().map_err(error)?;
    let mut selected = packages.iter().filter(|p| p.id == "@korri:moonlight");
    let package = selected
        .next()
        .ok_or_else(|| error("Moonlight plugin is not enabled"))?;
    if selected.next().is_some() {
        return Err(error("Moonlight plugin selection is ambiguous"));
    }
    let file = |name: &str| -> Result<PathBuf, LinuxViewerError> {
        let path = package
            .files
            .get(name)
            .ok_or_else(|| error(format!("Moonlight plugin has no {name} file")))?;
        if !path.starts_with("/nix/store") || !path.is_file() {
            return Err(error(
                "Moonlight declared file is not an immutable installed file",
            ));
        }
        Ok(path.clone())
    };
    Ok(MoonlightFiles {
        executable: file("moonlight")?,
        mapping: file("mapping")?,
        bootstrap: file("identity-bootstrap")?,
    })
}

fn validate_launch(launch: &str) -> Result<(), LinuxViewerError> {
    if launch.len() == 32
        && launch
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        Ok(())
    } else {
        Err(error("invalid exact viewer launch identity"))
    }
}

fn server_url(address: &str) -> Result<url::Url, LinuxViewerError> {
    let mut url = url::Url::parse(&format!("http://{address}/serverinfo")).map_err(error)?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/serverinfo"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error("invalid Moonlight address"));
    }
    let explicit_port = address
        .rsplit_once(':')
        .is_some_and(|(_, port)| port.parse::<u16>().is_ok());
    if !explicit_port {
        url.set_port(Some(47989))
            .map_err(|_| error("invalid Moonlight port"))?;
    }
    Ok(url)
}

/// Bounded real helper execution. Never log helper output: Moonlight bootstrap
/// output can contain identity details. A deadline also applies to failed hosts.
fn run(program: &Path, args: &[String]) -> Result<(bool, String), LinuxViewerError> {
    use std::os::fd::AsRawFd;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(error)?;
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| error("helper has no stdout"))?;
        let fd = stdout.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(error("cannot make helper output nonblocking"));
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut bytes = Vec::new();
        let mut exited = None;
        loop {
            let mut buffer = [0; 8192];
            loop {
                match stdout.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if bytes.len() + count > 4 * 1024 * 1024 {
                            return Err(error("helper output exceeded its budget"));
                        }
                        bytes.extend_from_slice(&buffer[..count]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(error(e)),
                }
            }
            if let Some(status) = exited {
                return Ok((status, String::from_utf8(bytes).map_err(error)?));
            }
            exited = child.try_wait().map_err(error)?.map(|s| s.success());
            if Instant::now() >= deadline {
                return Err(error("Moonlight/compositor helper timed out"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn platform() -> &'static str {
    platform_from_path(
        Path::new("/sys/firmware/devicetree/base/compatible"),
        |error| {
            log::warn!("Moonlight cannot read device-tree compatibility; using x11: {error}");
        },
    )
}

fn platform_from_path(path: &Path, report: impl FnOnce(&std::io::Error)) -> &'static str {
    let compatible = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return "x11",
        Err(error) => {
            report(&error);
            return "x11";
        }
    };
    if compatible
        .split(|byte| *byte == 0)
        .any(|value| value == b"qcom,sm8550" || value == b"qcom,sm8250")
    {
        "v4l2m2m"
    } else {
        "x11"
    }
}

/// Frame rate every Korri stream requests.
const STREAM_FPS: u64 = 60;

/// Multiple of Moonlight's default bitrate that a Korri stream requests.
/// Moonlight's table assumes desktop-class encoders. At 3 Mbit/s the RG35XX
/// SP's cedrus H.264 encoder looked visibly soft at 640x480 (2026-10-25).
const HANDHELD_ENCODER_BITRATE_FACTOR: u64 = 2;

fn stream_bitrate_kbps(width: u64, height: u64, fps: u64) -> u64 {
    default_bitrate_kbps(width, height, fps) * HANDHELD_ENCODER_BITRATE_FACTOR
}

/// Moonlight's own default bitrate for a resolution and frame rate, in kbps.
/// A port of `StreamingPreferences::getDefaultBitrate` (moonlight-qt,
/// app/settings/streamingpreferences.cpp, 4:2:0): linear interpolation over
/// the Shield resolution table, scaled by fps / 30, with sub-linear growth
/// above 60 fps. 640x480 at 60 fps gives 3000 kbps.
fn default_bitrate_kbps(width: u64, height: u64, fps: u64) -> u64 {
    const TABLE: [(f64, f64); 6] = [
        (640.0 * 360.0, 1.0),
        (854.0 * 480.0, 2.0),
        (1280.0 * 720.0, 5.0),
        (1920.0 * 1080.0, 10.0),
        (2560.0 * 1440.0, 20.0),
        (3840.0 * 2160.0, 40.0),
    ];
    let fps = fps as f64;
    let frame_rate_factor = if fps <= 60.0 {
        fps
    } else {
        (fps / 60.0).sqrt() * 60.0
    } / 30.0;
    let pixels = (width * height) as f64;
    let resolution_factor = match TABLE.iter().position(|(limit, _)| pixels <= *limit) {
        Some(0) => TABLE[0].1,
        Some(index) => {
            let (low_pixels, low_factor) = TABLE[index - 1];
            let (high_pixels, high_factor) = TABLE[index];
            (pixels - low_pixels) / (high_pixels - low_pixels) * (high_factor - low_factor)
                + low_factor
        }
        None => TABLE[TABLE.len() - 1].1,
    };
    (resolution_factor * frame_rate_factor).round() as u64 * 1000
}

fn stream_args(
    files: &MoonlightFiles,
    keydir: &Path,
    address: &str,
    size: (u64, u64),
    platform: &str,
    seats: &[PathBuf],
) -> Result<Vec<String>, LinuxViewerError> {
    let mut args = vec![
        files.executable.display().to_string(),
        "stream".into(),
        "-app".into(),
        "Korri Stream".into(),
        "-width".into(),
        size.0.to_string(),
        "-height".into(),
        size.1.to_string(),
        "-fps".into(),
        STREAM_FPS.to_string(),
        "-bitrate".into(),
        stream_bitrate_kbps(size.0, size.1, STREAM_FPS).to_string(),
        "-codec".into(),
        "h264".into(),
        "-platform".into(),
        platform.into(),
        "-mapping".into(),
        files.mapping.display().to_string(),
        "-keydir".into(),
        keydir.display().to_string(),
        "-windowed".into(),
        "-autowindowresize".into(),
        // The holder keeps playing; its own speaker must keep its sound.
        "-localaudio".into(),
    ];
    if seats.is_empty() {
        args.push("-viewonly".into());
    }
    let mut unique = std::collections::BTreeSet::new();
    for seat in seats {
        let name = seat
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if seat.parent() != Some(Path::new("/dev/input"))
            || !name
                .strip_prefix("event")
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            || !unique.insert(seat)
        {
            return Err(error(
                "occupied seat callback returned an invalid or duplicate event path",
            ));
        }
        // Moonlight resolves `x11` to its SDL system when SDL is built in, and
        // its SDL system exits on any `-input`: SDL reads every controller
        // itself. Only the evdev systems (such as v4l2m2m) take explicit seats.
        if platform != "x11" {
            args.extend(["-input".into(), seat.display().to_string()]);
        }
    }
    let peer = server_url(address)?;
    args.extend([
        "-port".into(),
        peer.port_or_known_default().unwrap_or(47989).to_string(),
    ]);
    // moonlight-common-c passes this host to getaddrinfo, not a URL parser.
    args.push(match peer.host() {
        Some(url::Host::Ipv6(address)) => address.to_string(),
        Some(host) => host.to_string(),
        None => return Err(error("missing Moonlight host")),
    });
    Ok(args)
}

fn fresh_bootstrap_id(live: &[String], selected: &str, mut next: impl FnMut() -> String) -> String {
    loop {
        let id = next();
        if id != selected && !live.contains(&id) {
            return id;
        }
    }
}

impl ProductionLinuxStreamViewer {
    fn bootstrap(&self, files: &MoonlightFiles, launch: &str) -> Result<String, LinuxViewerError> {
        let live = self.units.live_launch_ids().map_err(unit_error)?;
        let id = fresh_bootstrap_id(&live, launch, crate::generate_launch_id);
        let command = vec![
            files.bootstrap.display().to_string(),
            files.executable.display().to_string(),
            self.keydir.display().to_string(),
        ];
        let environment = BTreeMap::from([("HOME".into(), "/home/korri".into())]);
        let sandbox = LaunchSandbox::new(&command, &environment);
        let certificate = self
            .units
            .bootstrap_viewer(&id, &command, &environment, &sandbox)
            .map_err(unit_error)?;
        crate::host::moonlight_certificate::validate_single_pem(&certificate)
            .map_err(|_| error("Moonlight bootstrap returned an invalid client certificate"))?;
        Ok(certificate)
    }

    fn control(&self) -> Result<SwaymsgCompositorControl, LinuxViewerError> {
        SwaymsgCompositorControl::new(
            self.swaymsg.clone(),
            self.sway_socket.clone(),
            Duration::from_secs(3),
        )
        .ok_or_else(|| error("invalid compositor configuration"))
    }
    fn dimensions(&self) -> Result<(u64, u64), LinuxViewerError> {
        let (ok, output) = run(
            &self.swaymsg,
            &[
                "-s".into(),
                self.sway_socket.display().to_string(),
                "-t".into(),
                "get_outputs".into(),
                "-r".into(),
            ],
        )?;
        if !ok {
            return Err(error("cannot read active compositor output"));
        }
        let outputs: Vec<serde_json::Value> = serde_json::from_str(&output).map_err(error)?;
        let output = outputs
            .iter()
            .find(|o| o["active"] == true)
            .ok_or_else(|| error("no active compositor output"))?;
        let width = output["current_mode"]["width"]
            .as_u64()
            .filter(|w| *w > 0 && *w <= 16384);
        let height = output["current_mode"]["height"]
            .as_u64()
            .filter(|h| *h > 0 && *h <= 16384);
        width
            .zip(height)
            .ok_or_else(|| error("active output has no valid dimensions"))
    }
    fn observe(&self, launch: &str) -> Result<LinuxViewerStatus, LinuxViewerError> {
        match self.units.state(launch).map_err(unit_error)? {
            LaunchUnitState::Completed => return Ok(LinuxViewerStatus::Stopped),
            LaunchUnitState::Running => (),
            _ => return Err(error("viewer unit is not settled running or stopped")),
        }
        let pids = self.units.window_pids(launch).map_err(unit_error)?;
        let tree = self.control()?.tree().map_err(error)?;
        let portal_ids: Vec<_> = self.portal_ids.iter().map(String::as_str).collect();
        match focused_ownership(&tree, &pids, &portal_ids).map_err(error)? {
            crate::FocusOwnership::Launch => Ok(LinuxViewerStatus::Running),
            _ => Ok(LinuxViewerStatus::Backgrounded),
        }
    }
    fn focus(&self, launch: &str, portal: bool) -> Result<(), LinuxViewerError> {
        let control = self.control()?;
        let portal_ids: Vec<_> = self.portal_ids.iter().map(String::as_str).collect();
        if portal && portal_ids.is_empty() {
            return Err(error("portal compositor identity is not configured"));
        }
        // Moonlight draws its window only after the stream connects. On an
        // RG35XX Pro over Wi-Fi that took about 10 s from unit start.
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if self.units.state(launch).map_err(unit_error)? != LaunchUnitState::Running {
                return Err(error("cannot focus a viewer that is not running"));
            }
            let pids = self.units.window_pids(launch).map_err(unit_error)?;
            let tree = control.tree().map_err(error)?;
            let target = if portal {
                portal_node(&tree, &portal_ids)?
            } else {
                match select_focus_target(&tree, &pids, &portal_ids) {
                    FocusTarget::Window(window) => Some(window.node_id),
                    FocusTarget::NoWindow => None,
                    _ => return Err(error("viewer window is ambiguous or unreadable")),
                }
            };
            if let Some(node) = target {
                control.focus(node).map_err(error)?;
                let tree = control.tree().map_err(error)?;
                let actual = focused_ownership(&tree, &pids, &portal_ids).map_err(error)?;
                let expected = if portal {
                    crate::FocusOwnership::Excluded
                } else {
                    crate::FocusOwnership::Launch
                };
                if actual == expected {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(error("compositor did not confirm viewer focus"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn confirmed_stop(&self, launch: &str) -> Result<(), LinuxViewerError> {
        let stop = self.units.stop(launch);
        if self.units.state(launch).map_err(unit_error)? == LaunchUnitState::Completed {
            Ok(())
        } else {
            stop.map_err(unit_error)?;
            Err(error("viewer stop was not confirmed"))
        }
    }
}

fn portal_node(tree: &str, identities: &[&str]) -> Result<Option<i64>, LinuxViewerError> {
    fn collect(node: &serde_json::Value, identities: &[&str], found: &mut Vec<i64>) {
        if identities.iter().any(|id| {
            node["app_id"].as_str() == Some(id)
                || node["window_properties"]["class"].as_str() == Some(id)
        }) {
            if let Some(id) = node["id"].as_i64() {
                found.push(id);
            }
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = node[key].as_array() {
                for child in children {
                    collect(child, identities, found);
                }
            }
        }
    }
    let root = serde_json::from_str(tree).map_err(error)?;
    let mut found = Vec::new();
    collect(&root, identities, &mut found);
    match found.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        _ => Err(error("portal window is ambiguous")),
    }
}

impl LinuxStreamViewer for ProductionLinuxStreamViewer {
    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            validate_launch(launch)?;
            let url = server_url(address)?;
            let guard = self.operation.clone().lock_owned().await;
            let this = self.clone();
            let launch_owned = launch.to_owned();
            let (files, certificate, size) = tokio::task::spawn_blocking(move || {
                let files = (this.resolve_files)()?;
                let cert = this.bootstrap(&files, &launch_owned)?;
                Ok::<_, LinuxViewerError>((files, cert, this.dimensions()?))
            })
            .await
            .map_err(error)??;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(error)?;
            let mut response = client
                .get(url)
                .send()
                .await
                .map_err(|_| error("Sunshine serverinfo unavailable"))?
                .error_for_status()
                .map_err(|_| error("Sunshine serverinfo rejected"))?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| error("Sunshine serverinfo unreadable"))?
            {
                if bytes.len() + chunk.len() > 64 * 1024 {
                    return Err(error("Sunshine serverinfo exceeds budget"));
                }
                bytes.extend_from_slice(&chunk);
            }
            let body = String::from_utf8(bytes).map_err(error)?;
            let uuid = regex::Regex::new(r"<uniqueid>([^<]+)</uniqueid>")
                .expect("literal regex")
                .captures(&body)
                .and_then(|c| c.get(1))
                .map(|v| v.as_str().to_owned())
                .ok_or_else(|| error("Sunshine serverinfo has no host UUID"))?;
            crate::host::moonlight_certificate::validate_host_uuid(&uuid)
                .map_err(|_| error("Sunshine host UUID is invalid"))?;
            let server_certificate = (self.provision)(uuid, certificate)
                .await?
                .server_certificate;
            let seats = (self.local_seats)().await?;
            if seats.is_empty() {
                log::info!("Moonlight starts watch-only: no occupied local controller seats");
            }
            let command = stream_args(&files, &self.keydir, address, size, platform(), &seats)?;
            let this = self.clone();
            let launch = launch.to_owned();
            tokio::task::spawn_blocking(move || {
                let _guard = guard;
                let environment = BTreeMap::from([
                    ("KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT".into(), "1".into()),
                    ("DISPLAY".into(), ":0".into()),
                    ("HOME".into(), "/home/korri".into()),
                    (
                        "PULSE_SERVER".into(),
                        "unix:/run/korri-game-audio/native".into(),
                    ),
                ]);
                let sandbox = LaunchSandbox::new(&command, &environment);
                this.units
                    .launch_viewer(
                        &launch,
                        &command,
                        &environment,
                        &sandbox,
                        &server_certificate,
                    )
                    .map_err(unit_error)?;
                if let Err(focus_error) = this.focus(&launch, false) {
                    this.confirmed_stop(&launch)?;
                    return Err(focus_error);
                }
                Ok(())
            })
            .await
            .map_err(error)?
        })
    }
    fn status<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(async move {
            validate_launch(launch)?;
            let this = self.clone();
            let launch = launch.to_owned();
            tokio::task::spawn_blocking(move || this.observe(&launch))
                .await
                .map_err(error)?
        })
    }
    fn background<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        self.mutate(launch, Some(true))
    }
    fn resume_and_focus<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        self.mutate(launch, Some(false))
    }
    fn stop<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        self.mutate(launch, None)
    }
}
impl ProductionLinuxStreamViewer {
    fn mutate<'a>(
        &'a self,
        launch: &'a str,
        portal: Option<bool>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            validate_launch(launch)?;
            let guard = self.operation.clone().lock_owned().await;
            let this = self.clone();
            let launch = launch.to_owned();
            tokio::task::spawn_blocking(move || {
                let _guard = guard;
                match portal {
                    Some(portal) => this.focus(&launch, portal),
                    None => this.confirmed_stop(&launch),
                }
            })
            .await
            .map_err(error)?
        })
    }
}

#[cfg(test)]
#[path = "production_tests.rs"]
mod tests;
