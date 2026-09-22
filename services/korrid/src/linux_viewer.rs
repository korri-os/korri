//! Injected authority for one exact local Linux stream viewer.
//!
//! This module deliberately does not choose or package a viewer executable.
//! The production systemd/Moonlight adapter is a separate deployment slice.

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

/// Controls a viewer by the same exact launch ID used by the far korrid.
/// Implementations must never select a viewer by process name, address, or
/// "most recent" state when an exact launch ID is supplied. Every successful
/// mutation must return only after the requested runtime state is confirmed.
pub trait LinuxStreamViewer: Send + Sync {
    fn start<'a>(
        &'a self,
        launch_id: &'a str,
        moonlight_address: &'a str,
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
pub(crate) mod test_backend {
    use super::*;
    use std::{
        collections::{BTreeMap, VecDeque},
        sync::{Arc, Mutex},
    };
    use tokio::sync::Semaphore;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum LinuxViewerAction {
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
    }

    #[derive(Clone, Default)]
    pub struct TestLinuxStreamViewer {
        inner: Arc<Mutex<TestViewerState>>,
    }

    #[derive(Default)]
    struct TestViewerState {
        statuses: BTreeMap<String, LinuxViewerStatus>,
        calls: Vec<LinuxViewerCall>,
        failures: VecDeque<(LinuxViewerAction, String)>,
        status_block: Option<(Arc<Semaphore>, Arc<Semaphore>)>,
    }

    impl TestLinuxStreamViewer {
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
        ) -> Result<(), LinuxViewerError> {
            let mut state = self.inner.lock().expect("test viewer poisoned");
            state.calls.push(LinuxViewerCall {
                action,
                launch_id: launch_id.into(),
                moonlight_address: moonlight_address.map(Into::into),
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
        fn start<'a>(
            &'a self,
            launch_id: &'a str,
            moonlight_address: &'a str,
        ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
            Box::pin(async move {
                self.record(LinuxViewerAction::Start, launch_id, Some(moonlight_address))?;
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
                self.record(LinuxViewerAction::Status, launch_id, None)?;
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
                self.record(LinuxViewerAction::Background, launch_id, None)?;
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
                self.record(LinuxViewerAction::ResumeAndFocus, launch_id, None)?;
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
                self.record(LinuxViewerAction::Stop, launch_id, None)?;
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
