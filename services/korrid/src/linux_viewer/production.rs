//! Viewer effects stay at this edge. Admission comes from the plugin host;
//! process lifetime and focus come from the existing exact-launch authorities.
use super::{LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus};
use crate::host::compositor_focus::{
    focused_ownership, select_focus_target, CompositorControl, FocusTarget, SwayCompositorControl,
};
use crate::host::{LaunchSandbox, LaunchUnitBackend, LaunchUnitState, SystemdLaunchUnitBackend};
use crate::plugin_installation::EnabledPackage;
use futures::future::BoxFuture;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
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

/// The two Linux viewer plugins (ADR 0061). Both take Moonlight's arguments,
/// keydir and broker certificate; only `Korri` reports over a lifecycle socket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewerPlugin {
    Moonlight,
    Korri,
}

impl ViewerPlugin {
    pub fn id(self) -> &'static str {
        match self {
            Self::Moonlight => "@korri:moonlight",
            Self::Korri => "@korri:viewer",
        }
    }
    /// The runner whose settings korrid renders for this viewer.
    pub fn runner(self) -> &'static str {
        match self {
            Self::Moonlight => "@korri:moonlight/moonlight",
            Self::Korri => "@korri:viewer/viewer",
        }
    }
    /// The name in korrid's messages; Moonlight's are unchanged.
    fn title(self) -> &'static str {
        match self {
            Self::Moonlight => "Moonlight",
            Self::Korri => "Korri viewer",
        }
    }
    /// The declared file that names the viewer program.
    fn program(self) -> &'static str {
        match self {
            Self::Moonlight => "moonlight",
            Self::Korri => "viewer",
        }
    }
}

/// The callbacks use the registry's authenticated certificate broker and inputd's
/// occupied local seats. No peer credentials or controller state are duplicated.
pub(super) fn production_effects(
    provision: CertificateProvision,
    local_seats: LocalSeats,
    settings: ViewerSettings,
) -> ProductionLinuxStreamViewer {
    ProductionLinuxStreamViewer {
        provision,
        local_seats,
        settings,
        resolve_files: Arc::new(installed_files),
        units: Arc::new(SystemdLaunchUnitBackend::default().for_stream_viewer()),
        compositor: std::env::var_os("KORRID_COMPOSITOR_CONTROL_SOCKET")
            .and_then(|socket| SwayCompositorControl::new(socket.into(), Duration::from_secs(3))),
        shell: crate::host::configured_shell(),
        keydir: PathBuf::from("/home/korri/.config/moonlight"),
        operation: Arc::new(tokio::sync::Mutex::new(())),
    }
}

/// Moonlight's adapter observes systemd and compositor state. The Korri
/// viewer's adapter (`socket.rs`) borrows only its effects: unit start, focus
/// and confirmed stop.
#[derive(Clone)]
pub(super) struct ProductionLinuxStreamViewer {
    provision: CertificateProvision,
    local_seats: LocalSeats,
    settings: ViewerSettings,
    resolve_files: ResolveFiles,
    units: Arc<dyn LaunchUnitBackend>,
    compositor: Option<SwayCompositorControl>,
    shell: Option<Arc<dyn crate::host::ShellUnit>>,
    keydir: PathBuf,
    operation: Arc<tokio::sync::Mutex<()>>,
}

type ResolveFiles =
    Arc<dyn Fn(ViewerPlugin) -> Result<ViewerFiles, LinuxViewerError> + Send + Sync>;

fn unit_error(value: crate::host::LaunchUnitError) -> LinuxViewerError {
    error(value.message)
}

fn error(message: impl ToString) -> LinuxViewerError {
    LinuxViewerError::new(message.to_string())
}

#[derive(Clone)]
struct ViewerFiles {
    executable: PathBuf,
    mapping: PathBuf,
    bootstrap: PathBuf,
}

/// The one enabled package with this id, if any. Two is an error.
pub(super) fn enabled(
    packages: &[EnabledPackage],
    plugin: ViewerPlugin,
) -> Result<Option<&EnabledPackage>, LinuxViewerError> {
    let mut selected = packages.iter().filter(|p| p.id == plugin.id());
    let package = selected.next();
    if selected.next().is_some() {
        return Err(error(format!(
            "{} plugin selection is ambiguous",
            plugin.title()
        )));
    }
    Ok(package)
}

/// A declared file korrid reads or runs must be a file inside the immutable store.
pub(super) fn immutable_file(path: &Path) -> bool {
    path.starts_with("/nix/store") && path.is_file()
}

fn installed_files(plugin: ViewerPlugin) -> Result<ViewerFiles, LinuxViewerError> {
    let packages = crate::plugin_installation::read().map_err(error)?;
    let package = enabled(&packages, plugin)?
        .ok_or_else(|| error(format!("{} plugin is not enabled", plugin.title())))?;
    if plugin == ViewerPlugin::Korri {
        // Selection checked this too; the package can change between the two reads.
        super::selection::verify_treaty(package, &super::selection::read_immutable)?;
    }
    let file = |name: &str| -> Result<PathBuf, LinuxViewerError> {
        let path = package
            .files
            .get(name)
            .ok_or_else(|| error(format!("{} plugin has no {name} file", plugin.title())))?;
        if !immutable_file(path) {
            return Err(error(format!(
                "{} declared file is not an immutable installed file",
                plugin.title()
            )));
        }
        Ok(path.clone())
    };
    Ok(ViewerFiles {
        executable: file(plugin.program())?,
        mapping: file("mapping")?,
        bootstrap: file("identity-bootstrap")?,
    })
}

pub(super) fn validate_launch(launch: &str) -> Result<(), LinuxViewerError> {
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

pub(super) fn server_url(address: &str) -> Result<url::Url, LinuxViewerError> {
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

fn platform() -> &'static str {
    platform_from_path(
        Path::new("/sys/firmware/devicetree/base/compatible"),
        std::env::consts::ARCH,
        Path::new("/dev/dri"),
        |error| {
            log::warn!("Moonlight cannot read device-tree compatibility; using x11: {error}");
        },
    )
}

fn platform_from_path(
    path: &Path,
    architecture: &str,
    dri: &Path,
    report: impl FnOnce(&std::io::Error),
) -> &'static str {
    let compatible = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A render node makes VA-API worth trying, not guaranteed. Moonlight
            // falls back to SDL software decoding if VA-API cannot initialize.
            let render_node = architecture == "x86_64"
                && std::fs::read_dir(dri).is_ok_and(|mut entries| {
                    entries.any(|entry| {
                        entry.is_ok_and(|entry| {
                            entry.file_name().to_str().is_some_and(|name| {
                                name.strip_prefix("renderD").is_some_and(|suffix| {
                                    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                                })
                            }) && entry.path().exists()
                        })
                    })
                });
            return if render_node { "x11_vaapi" } else { "x11" };
        }
        Err(error) => {
            report(&error);
            return "x11";
        }
    };
    let mut values = compatible.split(|byte| *byte == 0);
    if values
        .clone()
        .any(|value| value == b"qcom,sm8550" || value == b"qcom,sm8250")
    {
        // Qualcomm Iris/Venus: stateful V4L2 M2M decoders.
        "v4l2m2m"
    } else if values.any(|value| value == b"rockchip,rk3566" || value == b"allwinner,sun50i-h700") {
        // RK3566 Hantro G1 and H700 Cedrus: stateless decoders driven
        // through FFmpeg's V4L2 Request API hwaccel.
        "v4l2request"
    } else {
        "x11"
    }
}

/// Renders the launch settings of the chosen viewer's runner for a display.
pub type ViewerSettings = Arc<
    dyn Fn(
            ViewerPlugin,
            crate::launcher::plugin_launch::PluginDisplay,
        ) -> Result<Vec<String>, LinuxViewerError>
        + Send
        + Sync,
>;

/// Each viewer plugin has its own settings under its own runner (ADR 0061):
/// settings stored for `@korri:moonlight` do not carry over.
pub fn render_settings(
    root: &Path,
    registry: &crate::plugin::PluginRegistry,
    runner: &str,
    game_id: Option<&str>,
    display: crate::launcher::plugin_launch::PluginDisplay,
) -> Result<Vec<String>, LinuxViewerError> {
    let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root).map_err(error)?;
    let route = crate::launch_settings::resolution::runner_settings_context(
        &snapshot, registry, runner, game_id,
    );
    let resolved =
        crate::launch_settings::resolve_launch(&snapshot, &route, None, None).map_err(error)?;
    for notice in &resolved.notices {
        log::warn!("{runner} settings: {notice:?}");
    }
    let package = registry.installed_package(runner).map_err(error)?;
    let source = crate::script::source::SourceSnapshot::package_plugin(
        &package.package,
        &package.entry,
        &package.sources,
    )
    .map_err(error)?;
    let (args, warnings) = crate::launcher::typed_settings::render(
        &source,
        runner,
        &resolved.runner.settings,
        Some(display),
        &package.package.display().to_string(),
    )
    .map_err(error)?;
    for warning in warnings {
        log::warn!("{runner} setting {}: {}", warning.setting, warning.message);
    }
    Ok(args)
}

/// Plugins can add presentation options, never change Korri's route or input.
fn plugin_args(args: &[String]) -> (Vec<String>, Vec<String>) {
    let mut accepted = Vec::new();
    let mut warnings = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let flag = arg.split('=').next().unwrap_or(arg);
        let owned = matches!(
            flag,
            "stream"
                | "-app"
                | "-keydir"
                | "-mapping"
                | "-platform"
                | "-input"
                | "-viewonly"
                | "-port"
                | "-windowed"
                | "-autowindowresize"
                | "-noautowindowresize"
                | "--"
        );
        let takes_value = matches!(
            flag,
            "-app" | "-keydir" | "-mapping" | "-platform" | "-input" | "-port"
        );
        if owned || !arg.starts_with('-') {
            warnings.push(format!(
                "dropped plugin argument {flag}: Korri owns the stream route and input"
            ));
            index += 1;
            if takes_value
                && !arg.contains('=')
                && args.get(index).is_some_and(|v| !v.starts_with('-'))
            {
                index += 1;
            }
            if flag == "--" {
                break;
            }
            continue;
        }
        accepted.push(arg.clone());
        index += 1;
        if !arg.contains('=')
            && args
                .get(index)
                .is_some_and(|v| !v.starts_with('-') || v.parse::<f64>().is_ok())
        {
            accepted.push(args[index].clone());
            index += 1;
        }
    }
    (accepted, warnings)
}

fn stream_args(
    files: &ViewerFiles,
    keydir: &Path,
    address: &str,
    rendered: &[String],
    platform: &str,
    seats: &[PathBuf],
) -> Result<Vec<String>, LinuxViewerError> {
    let mut args = vec![
        files.executable.display().to_string(),
        "stream".into(),
        "-app".into(),
        "Korri Stream".into(),
        "-platform".into(),
        platform.into(),
        "-mapping".into(),
        files.mapping.display().to_string(),
        "-keydir".into(),
        keydir.display().to_string(),
        "-windowed".into(),
        "-autowindowresize".into(),
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
        // x11 resolves to SDL. x11_vaapi uses evdev with udev auto-discovery
        // when no -input is given; failed VA-API init also resolves to SDL.
        // SDL exits on any -input, so both X11 choices must omit it. ARM
        // platforms keep explicit seats. See docs/research/2026-11-11-x86-viewer-vaapi.md.
        if !matches!(platform, "x11" | "x11_vaapi") {
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
    let (rendered, warnings) = plugin_args(rendered);
    for warning in warnings {
        log::warn!("Moonlight {warning}");
    }
    args.extend(rendered);
    Ok(args)
}

/// The viewer's own arguments precede Moonlight's (clients/viewer/README.md).
fn lifecycle_args(socket: &Path, launch: &str, controller: u32) -> [String; 6] {
    [
        "--lifecycle-socket".into(),
        socket.display().to_string(),
        "--launch-id".into(),
        launch.into(),
        "--controller-uid".into(),
        controller.to_string(),
    ]
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
    fn bootstrap(&self, files: &ViewerFiles, launch: &str) -> Result<String, LinuxViewerError> {
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

    fn control(&self) -> Result<SwayCompositorControl, LinuxViewerError> {
        self.compositor
            .clone()
            .ok_or_else(|| error("invalid compositor configuration"))
    }
    fn dimensions(&self) -> Result<(u64, u64), LinuxViewerError> {
        let output = self.control()?.outputs().map_err(error)?;
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
        let shell_pids = self
            .shell
            .as_ref()
            .map(|unit| unit.window_pids())
            .transpose()
            .map_err(unit_error)?
            .unwrap_or_default();
        match focused_ownership(&tree, &pids, &shell_pids).map_err(error)? {
            crate::FocusOwnership::Launch => Ok(LinuxViewerStatus::Running),
            _ => Ok(LinuxViewerStatus::Backgrounded),
        }
    }
    fn focus(&self, launch: &str, shell: bool) -> Result<(), LinuxViewerError> {
        let control = self.control()?;
        let shell_pids = self
            .shell
            .as_ref()
            .map(|unit| unit.window_pids())
            .transpose()
            .map_err(unit_error)?
            .unwrap_or_default();
        if shell && shell_pids.is_empty() {
            return Err(error("shell compositor identity is not configured"));
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
            let target = if shell {
                Some(crate::host::compositor_focus::shell_node(&tree, &shell_pids).map_err(error)?)
            } else {
                match select_focus_target(&tree, &pids, &shell_pids) {
                    FocusTarget::Window(window) => Some(window.node_id),
                    FocusTarget::NoWindow => None,
                    _ => return Err(error("viewer window is ambiguous or unreadable")),
                }
            };
            if let Some(node) = target {
                if shell {
                    control.focus(node).map_err(error)?;
                } else {
                    control.focus_fullscreen(node).map_err(error)?;
                }
                let tree = control.tree().map_err(error)?;
                let actual = focused_ownership(&tree, &pids, &shell_pids).map_err(error)?;
                let expected = if shell {
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
    /// Stops the unit and waits until systemd reports it gone. A unit that
    /// failed is reset by the stop. The bound covers the unit's own 30 s
    /// stop timeout (systemd_unit.rs); the socket adapter waits longer.
    fn ended(&self, launch: &str) -> Result<(), LinuxViewerError> {
        let deadline = Instant::now() + super::socket::UNIT_END;
        let mut stop = self.units.stop(launch);
        loop {
            match self.units.state(launch) {
                Ok(LaunchUnitState::Completed) => return Ok(()),
                Ok(_) => {}
                Err(failed) if failed.kind == crate::host::LaunchUnitErrorKind::UnitFailed => {
                    stop = self.units.stop(launch);
                }
                Err(other) => return Err(unit_error(other)),
            }
            if Instant::now() >= deadline {
                stop.map_err(unit_error)?;
                return Err(error("viewer unit did not end"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    /// Whose viewer unit holds this launch now, from systemd: None when no
    /// unit does. Only the Korri viewer's unit names its runner (start_unit),
    /// so a unit without that name is Moonlight's, including one started by
    /// a korrid before this marker existed.
    pub(super) fn running(&self, launch: &str) -> Result<Option<ViewerPlugin>, LinuxViewerError> {
        validate_launch(launch)?;
        match self.units.state(launch) {
            Ok(LaunchUnitState::Completed) => return Ok(None),
            Ok(_) => {}
            // A failed unit still holds the name until a stop resets it.
            Err(failed) if failed.kind == crate::host::LaunchUnitErrorKind::UnitFailed => {}
            Err(other) => return Err(unit_error(other)),
        }
        let runner = self.units.runner_id(launch).map_err(unit_error)?;
        Ok(Some(
            if runner.as_deref() == Some(ViewerPlugin::Korri.runner()) {
                ViewerPlugin::Korri
            } else {
                ViewerPlugin::Moonlight
            },
        ))
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

impl LinuxStreamViewer for ProductionLinuxStreamViewer {
    fn snapshot<'a>(
        &'a self,
        launch: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<Option<super::ViewerSnapshot>, LinuxViewerError>>
    {
        let this = self.clone();
        let launch = launch.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                validate_launch(&launch)?;
                let (phase, text) = this.units.state_with_status(&launch).map_err(unit_error)?;
                if phase == crate::host::LaunchUnitState::Completed {
                    return Ok(Some(super::ViewerSnapshot {
                        connected: Some(false),
                        ..Default::default()
                    }));
                }
                if phase != crate::host::LaunchUnitState::Running {
                    return Ok(None);
                }
                let Some(text) = text else {
                    return Ok(None);
                };
                if text == "korri-viewer-disconnected" {
                    return Ok(Some(super::ViewerSnapshot {
                        connected: Some(false),
                        ..Default::default()
                    }));
                }
                let Some(encoded) = connected_stream_size(&text) else {
                    return Ok(None);
                };
                let screen = this
                    .display()
                    .ok_or_else(|| error("Viewer display unavailable"))?;
                Ok(Some(super::ViewerSnapshot {
                    connected: Some(true),
                    shape: Some(super::ViewerShapeSnapshot {
                        connection: text
                            .rsplitn(3, ':')
                            .last()
                            .expect("validated presence")
                            .to_owned(),
                        screen,
                        encoded,
                        ready: false,
                    }),
                    ..Default::default()
                }))
            })
            .await
            .map_err(error)?
        })
    }

    fn display(&self) -> Option<crate::launcher::plugin_launch::PluginDisplay> {
        let (width, height) = self.dimensions().ok()?;
        Some(crate::launcher::plugin_launch::PluginDisplay {
            width: width as u32,
            height: height as u32,
        })
    }

    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
        _display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(self.start_unit(ViewerPlugin::Moonlight, launch, address))
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
    /// Starts the Korri viewer's unit and returns where its lifecycle socket
    /// will be. The socket adapter connects, sends Start and asks for focus.
    pub(super) async fn start_lifecycle(
        &self,
        launch: &str,
        address: &str,
    ) -> Result<PathBuf, LinuxViewerError> {
        self.start_unit(ViewerPlugin::Korri, launch, address)
            .await?;
        crate::host::viewer_lifecycle_socket(launch).map_err(unit_error)
    }

    /// One viewer unit for one exact launch: identity bootstrap, broker
    /// certificate, occupied seats, the chosen plugin's settings. Moonlight is
    /// focused here; the Korri viewer is focused after its socket confirms Start.
    async fn start_unit(
        &self,
        plugin: ViewerPlugin,
        launch: &str,
        address: &str,
    ) -> Result<(), LinuxViewerError> {
        validate_launch(launch)?;
        let url = server_url(address)?;
        let guard = self.operation.clone().lock_owned().await;
        let this = self.clone();
        let launch_owned = launch.to_owned();
        let (files, certificate, size) = tokio::task::spawn_blocking(move || {
            let files = (this.resolve_files)(plugin)?;
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
            log::info!(
                "{} starts watch-only: no occupied local controller seats",
                plugin.title()
            );
        }
        let this = self.clone();
        let launch = launch.to_owned();
        let address = address.to_owned();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let rendered = (this.settings)(
                plugin,
                crate::launcher::plugin_launch::PluginDisplay {
                    width: size.0 as u32,
                    height: size.1 as u32,
                },
            )?;
            let mut command = stream_args(
                &files,
                &this.keydir,
                &address,
                &rendered,
                platform(),
                &seats,
            )?;
            let controller = match plugin {
                ViewerPlugin::Moonlight => None,
                ViewerPlugin::Korri => {
                    // korrid's own UID: the only peer the viewer accepts.
                    let controller = unsafe { libc::geteuid() };
                    let socket =
                        crate::host::viewer_lifecycle_socket(&launch).map_err(unit_error)?;
                    command.splice(1..1, lifecycle_args(&socket, &launch, controller));
                    Some(controller)
                }
            };
            let mut environment = BTreeMap::from([
                ("KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT".into(), "1".into()),
                // SDL drops gamepad events while its window lacks X11
                // focus. Under Xwayland the viewer can hold compositor
                // focus without it, which left joiners with no controls.
                ("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS".into(), "1".into()),
                ("DISPLAY".into(), ":0".into()),
                ("HOME".into(), "/home/korri".into()),
                (
                    "PULSE_SERVER".into(),
                    "unix:/run/korri-game-audio/native".into(),
                ),
            ]);
            if controller.is_some() {
                // The live-unit runner marker game units carry: a restarted
                // korrid reads it to route this launch (`running`).
                environment.insert(crate::host::RUNNER_ID_ENV.into(), plugin.runner().into());
            }
            let sandbox = LaunchSandbox::new(&command, &environment);
            this.units
                .launch_viewer(
                    &launch,
                    &command,
                    &environment,
                    &sandbox,
                    &server_certificate,
                    controller,
                )
                .map_err(unit_error)?;
            if plugin == ViewerPlugin::Moonlight {
                if let Err(focus_error) = this.focus(&launch, false) {
                    this.confirmed_stop(&launch)?;
                    return Err(focus_error);
                }
            }
            Ok(())
        })
        .await
        .map_err(error)?
    }

    /// The Korri viewer's cleanup: after Stop the process ends by itself,
    /// so the unit is usually already deactivating.
    pub(super) async fn end_unit(&self, launch: &str) -> Result<(), LinuxViewerError> {
        validate_launch(launch)?;
        let guard = self.operation.clone().lock_owned().await;
        let this = self.clone();
        let launch = launch.to_owned();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            this.ended(&launch)
        })
        .await
        .map_err(error)?
    }

    pub(super) fn mutate<'a>(
        &'a self,
        launch: &'a str,
        shell: Option<bool>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            validate_launch(launch)?;
            let guard = self.operation.clone().lock_owned().await;
            let this = self.clone();
            let launch = launch.to_owned();
            tokio::task::spawn_blocking(move || {
                let _guard = guard;
                match shell {
                    Some(shell) => this.focus(&launch, shell),
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

// Only the native connectionStarted callback emits this StatusText. A live
// process or a visible Moonlight window alone does not prove a connection.
fn connected_stream_size(text: &str) -> Option<crate::launcher::plugin_launch::PluginDisplay> {
    let fields: Vec<_> = text
        .strip_prefix("korri-viewer-connected:")?
        .split(':')
        .collect();
    if fields.len() != 5 {
        return None;
    }
    let pid = fields[0].parse::<u32>().ok()?;
    let seconds = fields[1].parse::<u64>().ok()?;
    let nanos = fields[2].parse::<u32>().ok()?;
    let width = fields[3].parse::<u32>().ok()?;
    let height = fields[4].parse::<u32>().ok()?;
    if pid == 0
        || seconds == 0
        || nanos >= 1_000_000_000
        || width == 0
        || height == 0
        || width > 16384
        || height > 16384
    {
        return None;
    }
    Some(crate::launcher::plugin_launch::PluginDisplay { width, height })
}
