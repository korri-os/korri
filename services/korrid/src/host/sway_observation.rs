//! Event-driven Sway observations. Cached trees describe presentation only.
//! Commands and authority checks must still read a fresh tree from Sway.

use super::sway_ipc::SwayIpc;
use std::{
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::sync::watch;

#[cfg(test)]
#[path = "sway_observation_tests.rs"]
mod tests;

type Tree = Result<Arc<str>, String>;

struct State {
    stopped: bool,
    tree: Option<Tree>,
    cancel: Option<UnixStream>,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    changed: watch::Sender<u64>,
}

pub(super) struct SwayObservation {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    timeout: Duration,
}

impl std::fmt::Debug for SwayObservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SwayObservation")
            .finish_non_exhaustive()
    }
}

impl SwayObservation {
    pub(super) fn start(socket: PathBuf, timeout: Duration) -> std::io::Result<Self> {
        // Validate before starting a thread, including for a disconnected Sway.
        SwayIpc::new(socket.clone(), timeout)?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                stopped: false,
                tree: None,
                cancel: None,
            }),
            wake: Condvar::new(),
            changed: watch::channel(0).0,
        });
        let worker_shared = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("sway-observation".into())
            .spawn(move || {
                observe(worker_shared, socket, timeout);
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
            timeout,
        })
    }

    pub(super) fn subscribe(&self) -> watch::Receiver<u64> {
        self.shared.changed.subscribe()
    }

    pub(super) fn tree(&self) -> Tree {
        let deadline = Instant::now() + self.timeout;
        let mut state = self.shared.state.lock().expect("Sway observation poisoned");
        while state.tree.is_none() && !state.stopped {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err("Sway initial observation timed out".into());
            };
            state = self
                .shared
                .wake
                .wait_timeout(state, remaining)
                .expect("Sway observation poisoned")
                .0;
        }
        state
            .tree
            .clone()
            .unwrap_or_else(|| Err("Sway observation stopped".into()))
    }
}

impl Drop for SwayObservation {
    fn drop(&mut self) {
        {
            let mut state = self.shared.state.lock().expect("Sway observation poisoned");
            state.stopped = true;
            if let Some(socket) = state.cancel.take() {
                let _ = socket.shutdown(Shutdown::Both);
            }
            self.shared.wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn publish(shared: &Shared, tree: Tree) {
    let mut state = shared.state.lock().expect("Sway observation poisoned");
    if state.stopped {
        return;
    }
    if state.tree.as_ref() != Some(&tree) {
        state.tree = Some(tree);
        shared
            .changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
    shared.wake.notify_all();
}

fn observe(shared: Arc<Shared>, socket: PathBuf, timeout: Duration) {
    let mut retry = Duration::from_millis(100);
    loop {
        if shared
            .state
            .lock()
            .expect("Sway observation poisoned")
            .stopped
        {
            return;
        }
        // Both connections belong to one feed generation. Never reuse a
        // command connection from before a subscription disconnect.
        let control = SwayIpc::new(socket.clone(), timeout).expect("validated Sway address");
        let result = observe_connection(&shared, &control);
        if let Err(error) = result {
            publish(&shared, Err(error.to_string()));
        }
        let mut state = shared.state.lock().expect("Sway observation poisoned");
        state.cancel = None;
        if state.stopped {
            return;
        }
        // Only an unavailable feed retries. A healthy feed blocks in read with
        // no timer and no periodic snapshot work.
        let (state, _) = shared
            .wake
            .wait_timeout_while(state, retry, |state| !state.stopped)
            .expect("Sway observation poisoned");
        if state.stopped {
            return;
        }
        retry = (retry * 2).min(Duration::from_secs(5));
    }
}

fn observe_connection(shared: &Shared, control: &SwayIpc) -> std::io::Result<()> {
    let mut subscription = control.subscribe()?;
    {
        let mut state = shared.state.lock().expect("Sway observation poisoned");
        if state.stopped {
            return Ok(());
        }
        state.cancel = Some(subscription.cancellation_socket()?);
    }
    loop {
        let tree = control.request(4, b"")?;
        // A complete JSON tree, not just a valid IPC frame, is required before
        // any observer sees a new generation.
        let value: serde_json::Value = serde_json::from_str(&tree)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        if !value.is_object() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Sway tree is not an object",
            ));
        }
        publish(shared, Ok(Arc::from(tree)));
        subscription.next_event()?;
        publish(shared, Err("Sway observation changed; refreshing".into()));
        if shared
            .state
            .lock()
            .expect("Sway observation poisoned")
            .stopped
        {
            return Ok(());
        }
    }
}
