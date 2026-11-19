//! Injected authority for one exact local Linux stream viewer.
//!
//! `installed_viewer` chooses the enabled viewer plugin at each start
//! (`selection.rs`): `@korri:viewer` through its lifecycle socket, or Moonlight.
//! Both provision their certificate through the broker and run in an
//! exact-launch systemd unit; korrid keeps compositor focus and cleanup.

mod production;
mod selection;
mod socket;
pub use selection::installed_viewer;
pub use socket::{
    FocusSocketViewer, SocketLinuxStreamViewer, SocketViewerDisplay, StartSocketViewer,
    StopSocketViewer,
};
#[cfg(test)]
#[path = "linux_viewer/socket_tests.rs"]
mod socket_tests;
pub use production::{
    render_settings, sunshine_host_uuid, CertificateProvision, LocalSeats, ViewerPlugin,
    ViewerSettings,
};

use futures::future::BoxFuture;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinuxViewerStatus {
    Running,
    Backgrounded,
    Stopped,
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{message}")]
pub struct LinuxViewerError {
    message: String,
}

impl LinuxViewerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Observations extracted from Game's current runtime and Android battery.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewerSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<ViewerShapeSnapshot>,
    /// Legacy StreamHealthSample.sampledAtMs: local monotonic sample time, not surface data.
    pub sampled_at_ms: Option<f64>,
    /// The adapter converts the producer clock to a deadline without resetting sample age.
    #[serde(skip)]
    pub sampled_at: Option<std::time::Instant>,
    pub battery_percent: Option<f64>,
    pub stream_health: Option<korri_surface_treaty::SurfaceStreamHealth>,
    pub controls: Vec<korri_surface_treaty::SurfaceGameplayControl>,
}

impl ViewerSnapshot {
    /// The producer's sample time (`sampled_at_ms`, CLOCK_MONOTONIC
    /// milliseconds: Android's `SystemClock.uptimeMillis`, and the viewer's
    /// own clock in korrid's process) as an `Instant` of korrid's, `now` and
    /// `now_ms` being the same moment in both clocks. The sample keeps its
    /// age: receiving an old sample is not a new sample. A health reading
    /// with no usable sample time is dropped.
    pub fn date_sample(&mut self, now_ms: Option<f64>, now: std::time::Instant) {
        let age = self
            .sampled_at_ms
            .zip(now_ms)
            .filter(|(at, now_ms)| at.is_finite() && *at >= 0.0 && at <= now_ms)
            .and_then(|(at, now_ms)| {
                std::time::Duration::try_from_secs_f64((now_ms - at) / 1000.0).ok()
            });
        self.sampled_at = age.and_then(|age| now.checked_sub(age));
        if self.sampled_at.is_none() {
            self.stream_health = None;
        }
    }
}

/// CLOCK_MONOTONIC now, in milliseconds; none when it cannot be read.
pub fn monotonic_ms() -> Option<f64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes only into this owned, initialized timespec.
    let read = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } == 0;
    read.then(|| time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1e6)
}

/// A treaty viewer's status (`contracts/viewer`), through its socket or in
/// korrid's own process.
impl From<korri_viewer_treaty::ViewerStatus> for LinuxViewerStatus {
    fn from(status: korri_viewer_treaty::ViewerStatus) -> Self {
        use korri_viewer_treaty::ViewerStatus as S;
        match status {
            S::Running => Self::Running,
            S::Backgrounded => Self::Backgrounded,
            S::Stopped => Self::Stopped,
        }
    }
}

/// A treaty viewer's snapshot. It carries no clock of korrid's: `sampled_at`
/// stays unset.
impl From<korri_viewer_treaty::ViewerSnapshot> for ViewerSnapshot {
    fn from(s: korri_viewer_treaty::ViewerSnapshot) -> Self {
        Self {
            connected: s.connected,
            shape: s.shape.map(|shape| ViewerShapeSnapshot {
                connection: shape.connection,
                screen: crate::launcher::plugin_launch::PluginDisplay {
                    width: shape.screen.width,
                    height: shape.screen.height,
                },
                encoded: crate::launcher::plugin_launch::PluginDisplay {
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
        }
    }
}

/// Connection identity and dimensions observed from the live viewer, not from
/// its launch preferences. These are internal adapter facts, not surface data.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewerShapeSnapshot {
    pub connection: String,
    pub screen: crate::launcher::plugin_launch::PluginDisplay,
    pub encoded: crate::launcher::plugin_launch::PluginDisplay,
    pub ready: bool,
}

/// Controls a viewer by the same exact launch ID used by the holder's korrid.
/// Implementations must never select a viewer by process name, address, or
/// "most recent" state when an exact launch ID is supplied. Every successful
/// mutation must return only after the requested runtime state is confirmed.
pub trait LinuxStreamViewer: Send + Sync {
    fn change_signal(&self) -> Option<std::sync::Arc<tokio::sync::Notify>> {
        None
    }
    /// The link loss that this launch's viewer reported before it stopped
    /// (#544, the treaty's `LinkLost` event), once: the next call has none.
    /// A viewer that cannot see one has none.
    fn take_link_lost<'a>(
        &'a self,
        _launch: &'a str,
    ) -> BoxFuture<'a, Option<korri_viewer_treaty::ViewerLinkLost>> {
        Box::pin(async { None })
    }
    fn notice<'a>(
        &'a self,
        _launch: &'a str,
        _message: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async { Err(LinuxViewerError::new("Viewer notice unavailable")) })
    }
    fn resize<'a>(
        &'a self,
        _launch: &'a str,
        _connection: &'a str,
        _display: crate::launcher::plugin_launch::PluginDisplay,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async { Err(LinuxViewerError::new("Live resolution is unavailable")) })
    }
    fn snapshot<'a>(
        &'a self,
        _launch: &'a str,
    ) -> BoxFuture<'a, Result<Option<ViewerSnapshot>, LinuxViewerError>> {
        Box::pin(async { Ok(None) })
    }
    fn apply<'a>(
        &'a self,
        _launch: &'a str,
        _control: &'a str,
        _value: Option<korri_surface_treaty::SurfaceGameplayControlValue>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async { Err(LinuxViewerError::new("Local viewer control unavailable")) })
    }

    fn display(&self) -> Option<crate::launcher::plugin_launch::PluginDisplay> {
        None
    }

    /// Android follows the holder's composition; Linux retains its local size.
    fn start<'a>(
        &'a self,
        launch_id: &'a str,
        moonlight_address: &'a str,
        display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>>;

    fn status<'a>(
        &'a self,
        launch_id: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>>;

    fn background<'a>(&'a self, launch_id: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>>;

    fn resume_and_focus<'a>(
        &'a self,
        launch_id: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>>;

    fn stop<'a>(&'a self, launch_id: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>>;
}

#[derive(Default)]
#[cfg_attr(test, allow(dead_code))]
pub(crate) struct UnavailableLinuxStreamViewer;

impl LinuxStreamViewer for UnavailableLinuxStreamViewer {
    fn start<'a>(
        &'a self,
        _launch_id: &'a str,
        _moonlight_address: &'a str,
        _display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async {
            Err(LinuxViewerError::new(
                "Linux stream viewer adapter is not configured",
            ))
        })
    }

    fn status<'a>(
        &'a self,
        _launch_id: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        Box::pin(async {
            Err(LinuxViewerError::new(
                "Linux stream viewer adapter is not configured",
            ))
        })
    }

    fn background<'a>(
        &'a self,
        _launch_id: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async {
            Err(LinuxViewerError::new(
                "Linux stream viewer adapter is not configured",
            ))
        })
    }

    fn resume_and_focus<'a>(
        &'a self,
        _launch_id: &'a str,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async {
            Err(LinuxViewerError::new(
                "Linux stream viewer adapter is not configured",
            ))
        })
    }

    fn stop<'a>(&'a self, _launch_id: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async {
            Err(LinuxViewerError::new(
                "Linux stream viewer adapter is not configured",
            ))
        })
    }
}

#[cfg(test)]
mod sample_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn sampled(at_ms: Option<f64>) -> ViewerSnapshot {
        ViewerSnapshot {
            sampled_at_ms: at_ms,
            stream_health: Some(korri_surface_treaty::SurfaceStreamHealth {
                rtt_ms: Some(8.0),
                delivered_bitrate_kbps: Some(789.0),
                frames_dropped: Some(0.0),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn a_sample_keeps_its_age_in_korrids_clock() {
        let now = Instant::now();
        let mut snapshot = sampled(Some(9_000.0));
        snapshot.date_sample(Some(10_500.0), now);
        assert_eq!(
            snapshot.sampled_at,
            now.checked_sub(Duration::from_millis(1500))
        );
        assert!(snapshot.stream_health.is_some());
    }

    #[test]
    fn health_without_a_usable_sample_time_is_dropped() {
        let now = Instant::now();
        for (at, now_ms) in [
            (None, Some(10_000.0)),
            (Some(10_001.0), Some(10_000.0)),
            (Some(-1.0), Some(10_000.0)),
            (Some(f64::NAN), Some(10_000.0)),
            (Some(9_000.0), None),
        ] {
            let mut snapshot = sampled(at);
            snapshot.date_sample(now_ms, now);
            assert!(snapshot.sampled_at.is_none(), "{at:?} {now_ms:?}");
            assert!(snapshot.stream_health.is_none(), "{at:?} {now_ms:?}");
        }
    }

    #[test]
    fn the_monotonic_clock_reads_and_rises() {
        let first = monotonic_ms().unwrap();
        std::thread::sleep(Duration::from_millis(2));
        assert!(monotonic_ms().unwrap() > first);
    }
}

#[cfg(test)]
pub(crate) mod test_backend {
    use super::*;
    use std::{
        collections::{BTreeMap, VecDeque},
        sync::{Arc, Mutex},
    };
    use tokio::sync::Semaphore;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum LinuxViewerAction {
        Resize,
        Start,
        Status,
        Background,
        ResumeAndFocus,
        Stop,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct LinuxViewerCall {
        pub action: LinuxViewerAction,
        pub launch_id: String,
        pub moonlight_address: Option<String>,
        pub display: Option<crate::launcher::plugin_launch::PluginDisplay>,
    }

    #[derive(Clone, Default)]
    pub struct TestLinuxStreamViewer {
        inner: Arc<Mutex<TestViewerState>>,
    }

    #[derive(Default)]
    struct TestViewerState {
        display: Option<crate::launcher::plugin_launch::PluginDisplay>,
        statuses: BTreeMap<String, LinuxViewerStatus>,
        calls: Vec<LinuxViewerCall>,
        failures: VecDeque<(LinuxViewerAction, String)>,
        status_block: Option<(Arc<Semaphore>, Arc<Semaphore>)>,
        snapshot: Option<ViewerSnapshot>,
        applications: Vec<(String, String)>,
        link_lost: BTreeMap<String, korri_viewer_treaty::ViewerLinkLost>,
    }

    impl TestLinuxStreamViewer {
        pub fn set_snapshot(&self, snapshot: Option<ViewerSnapshot>) {
            self.inner.lock().unwrap().snapshot = snapshot;
        }
        pub fn applications(&self) -> Vec<(String, String)> {
            self.inner.lock().unwrap().applications.clone()
        }
        pub fn set_display(&self, display: crate::launcher::plugin_launch::PluginDisplay) {
            self.inner.lock().expect("test viewer poisoned").display = Some(display);
        }

        pub fn calls(&self) -> Vec<LinuxViewerCall> {
            self.inner
                .lock()
                .expect("test viewer poisoned")
                .calls
                .clone()
        }

        pub fn fail_next(&self, action: LinuxViewerAction, message: impl Into<String>) {
            self.inner
                .lock()
                .expect("test viewer poisoned")
                .failures
                .push_back((action, message.into()));
        }

        /// The viewer of `launch_id` reports `lost` and stops, as the
        /// socket viewer's LinkLost and Stopped events (#544).
        pub fn lose_link(
            &self,
            launch_id: impl Into<String>,
            lost: korri_viewer_treaty::ViewerLinkLost,
        ) {
            let launch_id = launch_id.into();
            let mut state = self.inner.lock().expect("test viewer poisoned");
            state
                .statuses
                .insert(launch_id.clone(), LinuxViewerStatus::Stopped);
            state.link_lost.insert(launch_id, lost);
        }

        pub fn seed(&self, launch_id: impl Into<String>, status: LinuxViewerStatus) {
            self.inner
                .lock()
                .expect("test viewer poisoned")
                .statuses
                .insert(launch_id.into(), status);
        }

        pub fn block_status(&self, started: Arc<Semaphore>, release: Arc<Semaphore>) {
            self.inner
                .lock()
                .expect("test viewer poisoned")
                .status_block = Some((started, release));
        }

        fn record(
            &self,
            action: LinuxViewerAction,
            launch_id: &str,
            moonlight_address: Option<&str>,
            display: Option<crate::launcher::plugin_launch::PluginDisplay>,
        ) -> Result<(), LinuxViewerError> {
            let mut state = self.inner.lock().expect("test viewer poisoned");
            state.calls.push(LinuxViewerCall {
                action,
                launch_id: launch_id.into(),
                moonlight_address: moonlight_address.map(Into::into),
                display,
            });
            if state
                .failures
                .front()
                .is_some_and(|(expected, _)| *expected == action)
            {
                let (_, message) = state.failures.pop_front().expect("front exists");
                return Err(LinuxViewerError::new(message));
            }
            Ok(())
        }
    }

    impl LinuxStreamViewer for TestLinuxStreamViewer {
        fn take_link_lost<'a>(
            &'a self,
            launch: &'a str,
        ) -> BoxFuture<'a, Option<korri_viewer_treaty::ViewerLinkLost>> {
            Box::pin(async move {
                self.inner
                    .lock()
                    .expect("test viewer poisoned")
                    .link_lost
                    .remove(launch)
            })
        }
        fn resize<'a>(
            &'a self,
            launch: &'a str,
            connection: &'a str,
            display: crate::launcher::plugin_launch::PluginDisplay,
        ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
            Box::pin(async move {
                self.record(
                    LinuxViewerAction::Resize,
                    launch,
                    None,
                    Some(display.clone()),
                )?;
                let mut state = self.inner.lock().unwrap();
                if state.statuses.get(launch) != Some(&LinuxViewerStatus::Running) {
                    return Err(LinuxViewerError::new("Viewer is not running"));
                }
                let shape = state
                    .snapshot
                    .as_mut()
                    .and_then(|s| s.shape.as_mut())
                    .filter(|s| s.connection == connection && s.ready)
                    .ok_or_else(|| LinuxViewerError::new("Viewer connection changed"))?;
                shape.encoded = display;
                Ok(())
            })
        }
        fn snapshot<'a>(
            &'a self,
            launch: &'a str,
        ) -> BoxFuture<'a, Result<Option<ViewerSnapshot>, LinuxViewerError>> {
            Box::pin(async move {
                let state = self.inner.lock().unwrap();
                if state.statuses.get(launch) != Some(&LinuxViewerStatus::Running) {
                    return Err(LinuxViewerError::new("Viewer is not running"));
                }
                Ok(state.snapshot.clone())
            })
        }
        fn apply<'a>(
            &'a self,
            launch: &'a str,
            id: &'a str,
            value: Option<korri_surface_treaty::SurfaceGameplayControlValue>,
        ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
            Box::pin(async move {
                use korri_surface_treaty::{
                    SurfaceGameplayControlInteraction as I, SurfaceGameplayControlValue as V,
                };
                let mut state = self.inner.lock().unwrap();
                if state.statuses.get(launch) != Some(&LinuxViewerStatus::Running) {
                    return Err(LinuxViewerError::new("Viewer is not running"));
                }
                let control = state
                    .snapshot
                    .as_mut()
                    .and_then(|s| s.controls.iter_mut().find(|c| c.id == id))
                    .ok_or_else(|| LinuxViewerError::new("Unknown control"))?;
                if !crate::surface::viewer::valid(&control.interaction, &value) {
                    return Err(LinuxViewerError::new("Invalid value"));
                }
                match (&mut control.interaction, value) {
                    (I::Toggle { value: old, .. }, Some(V::Toggle { value })) => *old = value,
                    (I::Choice { value: old, .. }, Some(V::Choice { value })) => *old = value,
                    (I::Range { value: old, .. }, Some(V::Range { value })) => *old = value,
                    _ => {}
                }
                state.applications.push((launch.into(), id.into()));
                Ok(())
            })
        }
        fn display(&self) -> Option<crate::launcher::plugin_launch::PluginDisplay> {
            self.inner
                .lock()
                .expect("test viewer poisoned")
                .display
                .clone()
        }

        fn start<'a>(
            &'a self,
            launch_id: &'a str,
            moonlight_address: &'a str,
            display: Option<crate::launcher::plugin_launch::PluginDisplay>,
        ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
            Box::pin(async move {
                self.record(
                    LinuxViewerAction::Start,
                    launch_id,
                    Some(moonlight_address),
                    display,
                )?;
                self.inner
                    .lock()
                    .expect("test viewer poisoned")
                    .statuses
                    .insert(launch_id.into(), LinuxViewerStatus::Running);
                Ok(())
            })
        }

        fn status<'a>(
            &'a self,
            launch_id: &'a str,
        ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
            Box::pin(async move {
                self.record(LinuxViewerAction::Status, launch_id, None, None)?;
                let block = self
                    .inner
                    .lock()
                    .expect("test viewer poisoned")
                    .status_block
                    .clone();
                if let Some((started, release)) = block {
                    started.add_permits(1);
                    release
                        .acquire()
                        .await
                        .expect("test status release semaphore closed")
                        .forget();
                }
                Ok(self
                    .inner
                    .lock()
                    .expect("test viewer poisoned")
                    .statuses
                    .get(launch_id)
                    .copied()
                    .unwrap_or(LinuxViewerStatus::Stopped))
            })
        }

        fn background<'a>(
            &'a self,
            launch_id: &'a str,
        ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
            Box::pin(async move {
                self.record(LinuxViewerAction::Background, launch_id, None, None)?;
                self.inner
                    .lock()
                    .expect("test viewer poisoned")
                    .statuses
                    .insert(launch_id.into(), LinuxViewerStatus::Backgrounded);
                Ok(())
            })
        }

        fn resume_and_focus<'a>(
            &'a self,
            launch_id: &'a str,
        ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
            Box::pin(async move {
                self.record(LinuxViewerAction::ResumeAndFocus, launch_id, None, None)?;
                self.inner
                    .lock()
                    .expect("test viewer poisoned")
                    .statuses
                    .insert(launch_id.into(), LinuxViewerStatus::Running);
                Ok(())
            })
        }

        fn stop<'a>(&'a self, launch_id: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
            Box::pin(async move {
                self.record(LinuxViewerAction::Stop, launch_id, None, None)?;
                self.inner
                    .lock()
                    .expect("test viewer poisoned")
                    .statuses
                    .insert(launch_id.into(), LinuxViewerStatus::Stopped);
                Ok(())
            })
        }
    }
}
