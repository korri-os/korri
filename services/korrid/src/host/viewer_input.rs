//! A viewer uses this device's seat scope. The registry remains the sole
//! selection authority; the holder's session and input are never touched.
use super::*;
use crate::host::{input_coordination::SeatCoordinator, HostRuntime};
use crate::linux_viewer::{LinuxStreamViewer, LinuxViewerError, LinuxViewerStatus};
use futures::future::BoxFuture;
use std::sync::OnceLock;

type SelectedPeer = Arc<dyn Fn() -> bool + Send + Sync>;
#[derive(Clone, Default)]
pub(super) struct PeerSelection(Arc<OnceLock<SelectedPeer>>);
impl PeerSelection {
    pub(super) fn active(&self) -> bool {
        self.0.get().is_some_and(|selected| selected())
    }
}

impl HostRuntime {
    pub(crate) fn viewer_with_local_input(
        &self,
        viewer: Arc<dyn LinuxStreamViewer>,
        selected: SelectedPeer,
    ) -> Arc<dyn LinuxStreamViewer> {
        let control = self.control().cloned();
        if let Ok(control) = &control {
            let _ = control.peer_selection.0.set(selected);
        }
        Arc::new(LocalInputViewer {
            viewer,
            control,
            coordinator: self.input_coordinator.clone(),
            operation: tokio::sync::Mutex::new(()),
        })
    }
}

struct LocalInputViewer {
    viewer: Arc<dyn LinuxStreamViewer>,
    control: Result<HostSessionControl, RpcFailure>,
    coordinator: Option<Arc<SeatCoordinator>>,
    operation: tokio::sync::Mutex<()>,
}
#[derive(Clone, Copy)]
enum Input {
    Begin,
    Focus,
    Background,
    End,
}
impl LocalInputViewer {
    async fn input(&self, launch: &str, action: Input) -> Result<(), LinuxViewerError> {
        let control = self
            .control
            .clone()
            .map_err(|e| LinuxViewerError::new(e.message))?;
        let coordinator = self
            .coordinator
            .clone()
            .ok_or_else(|| LinuxViewerError::new("Local input-seat coordination is unavailable"))?;
        let launch = launch.to_owned();
        tokio::task::spawn_blocking(move || {
            let session = coordinator.snapshot()?.session;
            if session.as_deref().is_some_and(|id| id != launch) {
                return Err("Local input belongs to another exact launch".into());
            }
            match action {
                Input::Begin => {
                    control.input_seats.ready()?;
                    if session.is_none() {
                        control
                            .input_seats
                            .begin_session(&launch, korri_input_contract::SeatOrder::Attached)?;
                    }
                    control.thaw_portal()
                }
                Input::Focus => {
                    if session.is_none() {
                        return Err("Viewer input session is absent".into());
                    }
                    control.reconcile_local_portal(&ActiveState::Running {
                        launch_id: launch,
                        game_id: None,
                    })
                }
                Input::Background => control.thaw_portal(),
                Input::End => {
                    control.thaw_portal()?;
                    if session.is_some() {
                        control.input_seats.end_session(&launch)?;
                    }
                    Ok(())
                }
            }
        })
        .await
        .map_err(|e| LinuxViewerError::new(e.to_string()))?
        .map_err(LinuxViewerError::new)
    }
}
impl LinuxStreamViewer for LocalInputViewer {
    fn start<'a>(
        &'a self,
        launch: &'a str,
        address: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let _operation = self.operation.lock().await;
            self.input(launch, Input::Begin).await?;
            let result = match self.viewer.start(launch, address).await {
                Ok(()) => self.input(launch, Input::Focus).await,
                Err(error) => Err(error),
            };
            if result.is_err() {
                self.input(launch, Input::Background).await?;
                self.viewer.stop(launch).await?;
                self.input(launch, Input::End).await?;
            }
            result
        })
    }
    fn status<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(async move {
            let _operation = self.operation.lock().await;
            let status = self.viewer.status(launch).await?;
            self.input(
                launch,
                if status == LinuxViewerStatus::Running {
                    Input::Focus
                } else {
                    Input::Background
                },
            )
            .await?;
            Ok(status)
        })
    }
    fn background<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let _operation = self.operation.lock().await;
            self.input(launch, Input::Background).await?;
            self.viewer.background(launch).await
        })
    }
    fn resume_and_focus<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let _operation = self.operation.lock().await;
            self.input(launch, Input::Background).await?;
            self.viewer.resume_and_focus(launch).await?;
            self.input(launch, Input::Focus).await
        })
    }
    fn stop<'a>(&'a self, launch: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            let _operation = self.operation.lock().await;
            self.input(launch, Input::Background).await?;
            self.viewer.stop(launch).await?;
            self.input(launch, Input::End).await
        })
    }
}

#[cfg(test)]
#[path = "viewer_input_tests.rs"]
mod tests;
