//! Which installed plugin is this device's Linux viewer (ADR 0061).
//!
//! `@korri:viewer`, when it is enabled and built from korrid's own viewer
//! contract, runs through the lifecycle socket. Otherwise Moonlight runs as
//! before. korrid refuses a viewer of another contract; it does not adapt.
use super::production::{enabled, immutable_file, production_effects, ViewerPlugin};
use super::socket::SocketLinuxStreamViewer;
use super::{
    CertificateProvision, LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus, LocalSeats,
    ViewerSettings, ViewerSnapshot,
};
use crate::launcher::plugin_launch::PluginDisplay;
use crate::plugin_installation::EnabledPackage;
use futures::future::BoxFuture;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};

/// The Typeshare output of `contracts/viewer`, as this korrid was built with
/// it. `@korri:viewer` declares the same file from the Korri revision it was
/// built from (`files.treaty`). Equal bytes mean one wire contract. Launch
/// arguments and behaviour are not in this file, so a change to them alone
/// is not caught here: the viewer then fails at start instead.
const TREATY: &[u8] = include_bytes!("../../../../contracts/generated/viewer.ts");
/// The generated treaty is a few KiB; the budget only bounds a hostile file.
const TREATY_BUDGET: u64 = 1024 * 1024;

fn error(message: impl ToString) -> LinuxViewerError {
    LinuxViewerError::new(message.to_string())
}

/// The production read of a declared treaty file: an immutable store file,
/// bounded.
pub(super) fn read_immutable(path: &Path) -> std::io::Result<Vec<u8>> {
    if !immutable_file(path) {
        return Err(std::io::Error::other(
            "the declared treaty is not an immutable installed file",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(TREATY_BUDGET + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > TREATY_BUDGET {
        return Err(std::io::Error::other(
            "the declared treaty exceeds its budget",
        ));
    }
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Refuses an `@korri:viewer` package that was not built from this korrid's
/// viewer contract. The message names both treaty hashes.
pub(super) fn verify_treaty(
    package: &EnabledPackage,
    read: &dyn Fn(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<(), LinuxViewerError> {
    let id = ViewerPlugin::Korri.id();
    let Some(path) = package.files.get("treaty") else {
        return Err(error(format!(
            "korrid refuses {id}: it declares no treaty file, so its viewer contract is unknown (korrid's treaty is sha256 {})",
            sha256(TREATY)
        )));
    };
    let bytes = read(path).map_err(|cause| {
        error(format!(
            "korrid refuses {id}: its treaty file {} is unreadable: {cause}",
            path.display()
        ))
    })?;
    if bytes != TREATY {
        return Err(error(format!(
            "korrid refuses {id}: it was built from a different viewer contract (its treaty is sha256 {}, korrid's is sha256 {})",
            sha256(&bytes),
            sha256(TREATY)
        )));
    }
    Ok(())
}

/// The viewer for the next start. A refused `@korri:viewer` leaves Moonlight
/// the viewer when Moonlight is enabled; without Moonlight the refusal is
/// the start's error. No `@korri:viewer`: Moonlight, exactly as before.
pub(super) fn select(
    packages: &[EnabledPackage],
    read: &dyn Fn(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<ViewerPlugin, LinuxViewerError> {
    let Some(korri) = enabled(packages, ViewerPlugin::Korri)? else {
        return Ok(ViewerPlugin::Moonlight);
    };
    match verify_treaty(korri, read) {
        Ok(()) => Ok(ViewerPlugin::Korri),
        Err(refusal) => {
            eprintln!("korrid viewer: {refusal}");
            if enabled(packages, ViewerPlugin::Moonlight)?.is_some() {
                Ok(ViewerPlugin::Moonlight)
            } else {
                Err(refusal)
            }
        }
    }
}

type Choose = Arc<dyn Fn() -> Result<ViewerPlugin, LinuxViewerError> + Send + Sync>;
type Running = Arc<dyn Fn(String) -> Result<Option<ViewerPlugin>, LinuxViewerError> + Send + Sync>;

/// The daemon's Linux viewer. The certificate broker, the occupied seats and
/// the settings renderer are the caller's; everything else is production.
pub fn installed_viewer(
    provision: CertificateProvision,
    local_seats: LocalSeats,
    settings: ViewerSettings,
) -> Arc<dyn LinuxStreamViewer> {
    let effects = production_effects(provision, local_seats, settings);
    let (viewer_uid, _) = crate::host::configured_runtime_identity();
    let display = effects.clone();
    let start = effects.clone();
    let focus = effects.clone();
    let stop = effects.clone();
    let korri = SocketLinuxStreamViewer::new(
        Arc::new(move || display.display()),
        viewer_uid,
        Arc::new(move |launch, address, _display| {
            let effects = start.clone();
            Box::pin(async move { effects.start_lifecycle(&launch, &address).await })
        }),
        Arc::new(move |launch, background| {
            let effects = focus.clone();
            Box::pin(async move { effects.mutate(&launch, Some(background)).await })
        }),
        Arc::new(move |launch| {
            let effects = stop.clone();
            Box::pin(async move { effects.end_unit(&launch).await })
        }),
    );
    let running = effects.clone();
    Arc::new(InstalledLinuxStreamViewer::new(
        Arc::new(effects),
        Arc::new(korri),
        Arc::new(|| {
            let packages = crate::plugin_installation::read().map_err(error)?;
            select(&packages, &read_immutable)
        }),
        Arc::new(move |launch| running.running(&launch)),
    ))
}

/// Chooses at each start and keeps each launch with the viewer that started
/// it, so a plugin change during a stream does not move its stop elsewhere.
pub(super) struct InstalledLinuxStreamViewer {
    moonlight: Arc<dyn LinuxStreamViewer>,
    korri: Arc<dyn LinuxStreamViewer>,
    choose: Choose,
    running: Running,
    launches: Mutex<HashMap<String, ViewerPlugin>>,
}

impl InstalledLinuxStreamViewer {
    pub(super) fn new(
        moonlight: Arc<dyn LinuxStreamViewer>,
        korri: Arc<dyn LinuxStreamViewer>,
        choose: Choose,
        running: Running,
    ) -> Self {
        Self {
            moonlight,
            korri,
            choose,
            running,
            launches: Mutex::new(HashMap::new()),
        }
    }
    fn viewer(&self, plugin: ViewerPlugin) -> &Arc<dyn LinuxStreamViewer> {
        match plugin {
            ViewerPlugin::Moonlight => &self.moonlight,
            ViewerPlugin::Korri => &self.korri,
        }
    }
    /// The installed plugins are a file read; keep it off the async workers.
    async fn choose(&self) -> Result<ViewerPlugin, LinuxViewerError> {
        let choose = self.choose.clone();
        tokio::task::spawn_blocking(move || choose())
            .await
            .map_err(error)?
    }
    /// A launch that this process did not start (after a korrid restart)
    /// goes to the viewer whose unit holds it, and is kept with it. With no
    /// unit it goes where a start would go now. When systemd or the choice
    /// cannot answer it goes to Moonlight, which observes its unit itself.
    async fn route(&self, launch: &str) -> Result<Arc<dyn LinuxStreamViewer>, LinuxViewerError> {
        let recorded = self
            .launches
            .lock()
            .expect("viewer launches poisoned")
            .get(launch)
            .copied();
        if let Some(plugin) = recorded {
            return Ok(self.viewer(plugin).clone());
        }
        let running = self.running.clone();
        let asked = launch.to_owned();
        let plugin = match tokio::task::spawn_blocking(move || running(asked)).await {
            Ok(Ok(Some(plugin))) => {
                self.launches
                    .lock()
                    .expect("viewer launches poisoned")
                    .insert(launch.into(), plugin);
                plugin
            }
            Ok(Ok(None)) => self.choose().await.unwrap_or(ViewerPlugin::Moonlight),
            Ok(Err(cause)) => {
                eprintln!("korrid viewer: launch {launch} unit unreadable, Moonlight observes it: {cause}");
                ViewerPlugin::Moonlight
            }
            Err(cause) => {
                eprintln!("korrid viewer: launch {launch} unit unreadable, Moonlight observes it: {cause}");
                ViewerPlugin::Moonlight
            }
        };
        Ok(self.viewer(plugin).clone())
    }
    fn forget(&self, launch: &str) {
        self.launches
            .lock()
            .expect("viewer launches poisoned")
            .remove(launch);
    }
}

impl LinuxStreamViewer for InstalledLinuxStreamViewer {
    fn change_signal(&self) -> Option<Arc<tokio::sync::Notify>> {
        self.korri.change_signal()
    }
    fn display(&self) -> Option<PluginDisplay> {
        self.moonlight.display()
    }
    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
        display: Option<PluginDisplay>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let plugin = self.choose().await?;
            eprintln!("korrid viewer: launch {launch} uses {}", plugin.id());
            self.launches
                .lock()
                .expect("viewer launches poisoned")
                .insert(launch.into(), plugin);
            let result = self.viewer(plugin).start(launch, address, display).await;
            if result.is_err() {
                // Both adapters stop what a failed start began.
                self.forget(launch);
            }
            result
        })
    }
    fn status<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(async move { self.route(launch).await?.status(launch).await })
    }
    fn background<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move { self.route(launch).await?.background(launch).await })
    }
    fn resume_and_focus<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move { self.route(launch).await?.resume_and_focus(launch).await })
    }
    fn stop<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            self.route(launch).await?.stop(launch).await?;
            self.forget(launch);
            Ok(())
        })
    }
    fn snapshot<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<Option<ViewerSnapshot>, LinuxViewerError>> {
        Box::pin(async move { self.route(launch).await?.snapshot(launch).await })
    }
    fn resize<'a>(
        &'a self,
        launch: &'a str,
        connection: &'a str,
        display: PluginDisplay,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            self.route(launch)
                .await?
                .resize(launch, connection, display)
                .await
        })
    }
    fn apply<'a>(
        &'a self,
        launch: &'a str,
        control: &'a str,
        value: Option<korri_surface_treaty::SurfaceGameplayControlValue>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            self.route(launch)
                .await?
                .apply(launch, control, value)
                .await
        })
    }
    fn notice<'a>(
        &'a self,
        launch: &'a str,
        message: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move { self.route(launch).await?.notice(launch, message).await })
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
