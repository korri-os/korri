//! File revisions for read projections. Native events cover external edits;
//! producers call `committed` after successful writes for read-after-write order.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex, Weak},
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::watch;

#[cfg(test)]
#[path = "file_changes_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InputState {
    Ready(u64),
    Unavailable { revision: u64, reason: String },
}

#[derive(Clone, Debug)]
pub(crate) struct InputPath {
    pub path: PathBuf,
    pub recursive: bool,
    /// Producer-owned classification of files that affect this projection.
    pub filter: Option<fn(&Path) -> bool>,
}

impl InputPath {
    fn contains(&self, changed: &Path) -> bool {
        if let Ok(relative) = changed.strip_prefix(&self.path) {
            return self.filter.is_none_or(|accept| accept(relative));
        }
        self.path.starts_with(changed)
    }
}

struct ObservedInputs {
    paths: Vec<InputPath>,
    resolved: Mutex<Vec<InputPath>>,
    changes: watch::Sender<InputState>,
}

static OBSERVERS: Mutex<Vec<Weak<ObservedInputs>>> = Mutex::new(Vec::new());

/// Call after publication or removal, with the writer's logical path. This
/// does no filesystem I/O; native watch updates refresh resolved aliases.
pub(crate) fn committed(path: &Path) {
    let live: Vec<_> = {
        let mut observers = OBSERVERS.lock().expect("file observer registry poisoned");
        observers.retain(|observer| observer.strong_count() > 0);
        observers.iter().filter_map(Weak::upgrade).collect()
    };
    for observer in live {
        let matches = observer.paths.iter().any(|input| input.contains(path))
            || observer
                .resolved
                .lock()
                .expect("file observer paths poisoned")
                .iter()
                .any(|input| input.contains(path));
        if matches {
            observer.invalidate();
        }
    }
}

impl ObservedInputs {
    fn invalidate(&self) {
        self.changes.send_modify(|state| match state {
            InputState::Ready(revision) | InputState::Unavailable { revision, .. } => {
                *revision = revision.wrapping_add(1);
            }
        });
    }

    fn publish(&self, failure: Option<String>) {
        self.changes.send_modify(|state| {
            let revision = match state {
                InputState::Ready(revision) | InputState::Unavailable { revision, .. } => {
                    revision.wrapping_add(1)
                }
            };
            *state = match failure {
                Some(reason) => InputState::Unavailable { revision, reason },
                None => InputState::Ready(revision),
            };
        });
    }
}

enum Message {
    Event(notify::Result<Event>),
    Stop,
}

pub(crate) struct FileChanges {
    changes: watch::Receiver<InputState>,
    stop: mpsc::Sender<Message>,
    worker: Option<JoinHandle<()>>,
}

impl FileChanges {
    pub(crate) fn watch(paths: Vec<InputPath>) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let (changes, current) = watch::channel(InputState::Unavailable {
            revision: 0,
            reason: "file inputs are not watched yet".into(),
        });
        let observed = Arc::new(ObservedInputs {
            resolved: Mutex::new(paths.clone()),
            paths,
            changes,
        });
        {
            let mut observers = OBSERVERS.lock().expect("file observer registry poisoned");
            observers.retain(|observer| observer.strong_count() > 0);
            observers.push(Arc::downgrade(&observed));
        }
        let events = sender.clone();
        let worker = std::thread::Builder::new()
            .name("file-inputs".into())
            .spawn(move || {
                run(observed, events, receiver);
            })?;
        Ok(Self {
            changes: current,
            stop: sender,
            worker: Some(worker),
        })
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<InputState> {
        let mut receiver = self.changes.clone();
        receiver.borrow_and_update();
        receiver
    }

    pub(crate) fn current(&self) -> InputState {
        self.changes.borrow().clone()
    }
}

impl Drop for FileChanges {
    fn drop(&mut self) {
        let _ = self.stop.send(Message::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn relevant(event: &Event, paths: &[InputPath]) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.need_rescan()
        || event.paths.is_empty()
        || event
            .paths
            .iter()
            .any(|changed| paths.iter().any(|input| input.contains(changed)))
}

fn resolved_paths(paths: &[InputPath]) -> Vec<InputPath> {
    let mut resolved = paths.to_vec();
    for input in paths {
        if let Ok(path) = input.path.canonicalize() {
            if path != input.path {
                resolved.push(InputPath {
                    path,
                    recursive: input.recursive,
                    filter: input.filter,
                });
            }
        }
    }
    resolved
}

fn same_paths(left: &[InputPath], right: &[InputPath]) -> bool {
    left.iter()
        .map(|input| (&input.path, input.recursive))
        .eq(right.iter().map(|input| (&input.path, input.recursive)))
}

fn registration(paths: &[InputPath]) -> BTreeMap<PathBuf, bool> {
    let mut result = BTreeMap::new();
    for input in paths {
        let mut directory = input.path.clone();
        while !directory.is_dir() {
            if !directory.pop() {
                break;
            }
        }
        let recursive = input.recursive && directory == input.path;
        result
            .entry(directory.clone())
            .and_modify(|mode| *mode |= recursive)
            .or_insert(recursive);
        // Watch ancestors too: inotify follows an inode, not its replacement.
        for ancestor in directory.ancestors().skip(1) {
            result.entry(ancestor.to_path_buf()).or_insert(false);
        }
    }
    result
}

fn install(
    paths: &[InputPath],
    events: mpsc::Sender<Message>,
) -> notify::Result<(RecommendedWatcher, BTreeMap<PathBuf, bool>)> {
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = events.send(Message::Event(event));
    })?;
    let registration = registration(paths);
    for (path, recursive) in &registration {
        watcher.watch(
            path,
            if *recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            },
        )?;
    }
    Ok((watcher, registration))
}

fn run(
    observed: Arc<ObservedInputs>,
    events: mpsc::Sender<Message>,
    messages: mpsc::Receiver<Message>,
) {
    loop {
        let resolved = resolved_paths(&observed.paths);
        let (watcher, watched) = match install(&resolved, events.clone()) {
            Ok(installed) => installed,
            Err(error) => {
                observed.publish(Some(error.to_string()));
                match messages.recv_timeout(Duration::from_secs(1)) {
                    Ok(Message::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    _ => continue,
                }
            }
        };
        *observed
            .resolved
            .lock()
            .expect("file observer paths poisoned") = resolved.clone();
        observed.publish(None);
        loop {
            match messages.recv() {
                Ok(Message::Stop) | Err(_) => return,
                Ok(Message::Event(Err(error))) => {
                    observed.publish(Some(error.to_string()));
                    break;
                }
                Ok(Message::Event(Ok(event))) if relevant(&event, &resolved) => {
                    let replaced = event.paths.iter().any(|path| watched.contains_key(path))
                        && matches!(
                            event.kind,
                            EventKind::Create(_)
                                | EventKind::Remove(_)
                                | EventKind::Modify(notify::event::ModifyKind::Name(_))
                        );
                    let next = resolved_paths(&observed.paths);
                    if event.need_rescan()
                        || replaced
                        || !same_paths(&resolved, &next)
                        || watched != registration(&next)
                    {
                        observed.publish(Some("file input watches changed".into()));
                        break;
                    }
                    observed.invalidate();
                }
                Ok(Message::Event(Ok(_))) => (),
            }
        }
        drop(watcher);
    }
}
