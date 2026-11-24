//! The install manager (ADR 0092): one line for every install on this device.
//! Pinned artifacts (ADR 0012, ADR 0013), provider games (ADR 0016) and
//! installs from devices, netplay's among them (ADR 0040), wait in it. One
//! install runs at a time. The person can reorder the line, and play-here and
//! netplay go to its front, which pauses a background download until they are
//! done.
//!
//! The cache stays the truth of what is in the line: an artifact's `.part`
//! marker, a provider job's record (`game.rs`). The order file is the
//! person's order and only sorts them: a member it does not name sorts after
//! the named ones, by when it was queued (ADR 0013). It lives in the private
//! state root, because the order is the person's intent and the cache can be
//! lost and fetched again.
use super::Job;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, watch};

/// What the order file names: an artifact by its declared id
/// (`AcquisitionStartRequest.artifactIds`) or a provider job by its
/// `AcquireJobStatus.jobId`. An install from devices, netplay's too, is
/// shown under its job id but never kept across a restart (ADR 0092).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Member {
    Artifact(String),
    Job(String),
}

/// How an install that waits through a ticket shows in the line.
pub(crate) enum Listed<'a> {
    /// Not in `line()`: a test's stand-in for an install that runs.
    #[cfg(test)]
    Hidden,
    /// In `line()` and in the order file a restart reads.
    Kept(&'a Path, Member),
    /// In `line()` only: a restart forgets it.
    Shown(Member),
}

/// Where in the line a new install goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Place {
    /// Behind every install that already waits.
    Back,
    /// Play-here and netplay: before every install that waits. A background
    /// download that runs pauses and goes next.
    Front,
}

pub(super) fn order_path(private_root: &Path) -> PathBuf {
    private_root.join("acquisition").join("queue.json")
}

/// The person's order, front first. A missing or unreadable file is no order.
pub(super) fn read_order(private_root: &Path) -> Vec<Member> {
    let path = order_path(private_root);
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            eprintln!("korrid install manager: the queue order is unreadable: {error}");
            Vec::new()
        }),
        Err(_) => Vec::new(),
    }
}

fn write_order(private_root: &Path, order: &[Member]) -> std::io::Result<()> {
    let path = order_path(private_root);
    let directory = path.parent().expect("order file has a folder");
    if !directory.is_dir() {
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    let temporary = directory.join(".queue.json.tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(order).expect("order serializes"))?;
    file.sync_all()?;
    fs::rename(&temporary, &path)?;
    fs::File::open(directory)?.sync_all()
}

#[derive(Default)]
pub(super) struct Queue {
    pub(super) jobs: BTreeMap<PathBuf, Job>,
    pub(super) waiting: VecDeque<PathBuf>,
    pub(super) running: Option<Running>,
    pub(super) worker: bool,
    /// Installs that wait through a ticket (provider games, netplay). The
    /// worker sends each of its turns here; a ticket can take several turns
    /// when a launch pauses it.
    pub(super) tickets: BTreeMap<PathBuf, mpsc::UnboundedSender<Grant>>,
    /// The order file's name of each kept install.
    pub(super) members: BTreeMap<PathBuf, Member>,
    /// Where the order file is kept; none until a caller names the root.
    pub(super) private_root: Option<PathBuf>,
    /// Startup recovery is still filling the line: no worker starts yet.
    pub(super) holding: bool,
    /// Installs the line shows but a restart forgets: an install from
    /// devices, whose record schema is not decided (ADR 0092, #552).
    pub(super) transient: std::collections::BTreeSet<PathBuf>,
    pub(super) written: Option<Vec<Member>>,
}

pub(super) struct Running {
    pub(super) base: PathBuf,
    pub(super) stop: watch::Sender<Stop>,
    pub(super) done: watch::Receiver<bool>,
    /// A ticket's install says when a pause would keep its bytes.
    pub(super) pausable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Stop {
    Run,
    Pause,
    Cancel,
}

/// One turn for a ticket: report done, and hear pause and cancel.
pub(super) struct Grant {
    done: watch::Sender<bool>,
    stop: watch::Receiver<Stop>,
}

impl Queue {
    /// Write the order when it changed: the running install, then the line.
    /// Only `end` and explicit changes to the line call this; dropping a
    /// place never does, so a korrid that stops keeps the order it had. A
    /// kept install that is not in the line keeps its place from the saved
    /// order. An install that a restart forgets (`transient`) is not saved.
    pub(super) fn persist(&mut self) {
        // Startup recovery refills the line in cache order; the saved order
        // is applied after it, and only then written.
        if self.holding {
            return;
        }
        let Some(private_root) = self.private_root.clone() else {
            return;
        };
        let mut order: Vec<Member> = self
            .running
            .iter()
            .map(|running| &running.base)
            .chain(self.waiting.iter())
            .filter(|key| !self.transient.contains(*key))
            .filter_map(|key| self.members.get(key).cloned())
            .collect();
        let saved = self.written.clone().unwrap_or_default();
        for (index, member) in saved.iter().enumerate() {
            let remembered = self
                .members
                .iter()
                .any(|(key, m)| m == member && !self.transient.contains(key));
            if remembered && !order.contains(member) {
                order.insert(index.min(order.len()), member.clone());
            }
        }
        if self.written.as_ref() == Some(&order) {
            return;
        }
        match write_order(&private_root, &order) {
            Ok(()) => self.written = Some(order),
            Err(error) => {
                eprintln!("korrid install manager: the queue order was not saved: {error}")
            }
        }
    }

    pub(super) fn remember(&mut self, private_root: &Path, key: &Path, member: Member) {
        if self.private_root.is_none() {
            self.private_root = Some(private_root.to_path_buf());
        }
        self.members.insert(key.to_path_buf(), member);
    }

    /// Put these installs first, in this order. A background download that
    /// runs and can keep its bytes pauses and goes right after them.
    pub(super) fn place_front(&mut self, first: Vec<PathBuf>) {
        if first.is_empty() {
            return;
        }
        let mut paused = None;
        if let Some(running) = &self.running {
            let pausable = running.pausable
                || self.jobs.get(&running.base).is_some_and(|job| {
                    matches!(job.state, super::ArtifactState::Downloading { .. })
                });
            if !first.contains(&running.base) && pausable {
                let _ = running.stop.send(Stop::Pause);
                paused = Some(running.base.clone());
            }
        }
        let running = self.running.as_ref().map(|run| run.base.clone());
        let mut order = VecDeque::new();
        let candidates = first
            .into_iter()
            .filter(|base| Some(base) != running.as_ref())
            .chain(paused)
            .chain(self.waiting.drain(..));
        for base in candidates {
            if !order.contains(&base) {
                order.push_back(base);
            }
        }
        self.waiting = order;
        self.persist();
    }

    /// Move a waiting install to `index` in the line (0 is next). The running
    /// install keeps running. False when the install does not wait.
    #[cfg_attr(not(test), allow(dead_code))] // #553: Velvet's list of installs.
    pub(super) fn reorder(&mut self, member: &Member, index: usize) -> bool {
        let Some(key) = self
            .members
            .iter()
            .find(|(_, named)| *named == member)
            .map(|(key, _)| key.clone())
        else {
            return false;
        };
        let Some(position) = self.waiting.iter().position(|base| *base == key) else {
            return false;
        };
        self.waiting.remove(position);
        let index = index.min(self.waiting.len());
        self.waiting.insert(index, key);
        self.persist();
        true
    }

    /// Sort the line by the person's order. Unnamed installs keep their
    /// relative order after the named ones.
    pub(super) fn sort_by_order(&mut self, order: &[Member]) {
        let rank = |key: &PathBuf| {
            self.members
                .get(key)
                .and_then(|member| order.iter().position(|named| named == member))
                .unwrap_or(usize::MAX)
        };
        let mut waiting: Vec<_> = self.waiting.drain(..).collect();
        waiting.sort_by_key(rank);
        self.waiting = waiting.into();
        self.persist();
    }

    /// The running install is a ticket's: give it its turn. False when the
    /// ticket is gone.
    pub(super) fn grant(&mut self, base: PathBuf) -> Option<watch::Receiver<bool>> {
        let sender = self.tickets.get(&base)?;
        let (done, finished) = watch::channel(false);
        let (stop, stop_rx) = watch::channel(Stop::Run);
        if sender
            .send(Grant {
                done,
                stop: stop_rx,
            })
            .is_err()
        {
            self.tickets.remove(&base);
            return None;
        }
        self.running = Some(Running {
            base,
            stop,
            done: finished.clone(),
            pausable: false,
        });
        self.persist();
        Some(finished)
    }
}

/// An install's place in the line. Dropping it leaves the line.
pub(crate) struct TurnWait {
    pub(super) key: PathBuf,
    pub(super) grants: mpsc::UnboundedReceiver<Grant>,
    pub(super) queue: Arc<Mutex<Queue>>,
}

/// The running turn. Dropping it lets the line continue.
pub(crate) struct Turn {
    done: watch::Sender<bool>,
    pub(super) stop: watch::Receiver<Stop>,
    key: PathBuf,
    queue: Arc<Mutex<Queue>>,
}

impl Drop for Turn {
    fn drop(&mut self) {
        let _ = self.done.send(true);
    }
}

impl Turn {
    /// From here a pause keeps this install's bytes.
    pub(super) fn allow_pause(&self) {
        let mut queue = self.queue.lock().expect("acquisition queue mutex poisoned");
        if let Some(running) = queue.running.as_mut().filter(|run| run.base == self.key) {
            running.pausable = true;
        }
    }

    /// From here a pause would lose work. A pause that the line sent first
    /// lets this install finish before the line goes on; its stale place in
    /// the line goes when its ticket does.
    pub(super) fn forbid_pause(&self) {
        let mut queue = self.queue.lock().expect("acquisition queue mutex poisoned");
        if let Some(running) = queue.running.as_mut().filter(|run| run.base == self.key) {
            running.pausable = false;
        }
    }

    /// Resolves when the line asks this install to pause.
    pub(super) async fn paused(&mut self) {
        while *self.stop.borrow() != Stop::Pause {
            if self.stop.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
}

impl TurnWait {
    pub(super) async fn turn(&mut self) -> Turn {
        match self.grants.recv().await {
            Some(grant) => Turn {
                done: grant.done,
                stop: grant.stop,
                key: self.key.clone(),
                queue: self.queue.clone(),
            },
            // The line is gone; nothing else can run, so proceed alone.
            None => Turn {
                done: watch::channel(false).0,
                stop: watch::channel(Stop::Run).1,
                key: self.key.clone(),
                queue: self.queue.clone(),
            },
        }
    }
}

impl TurnWait {
    /// The install ended: it leaves the person's order too.
    pub(super) fn end(&self) {
        let mut queue = self.queue.lock().expect("acquisition queue mutex poisoned");
        queue.members.remove(&self.key);
        queue.transient.remove(&self.key);
        queue.persist();
    }
}

/// Dropping a place leaves the line in memory only. It never writes the
/// order file: on a stop, tokio drops tasks in any order, and each write
/// would see a different line. Only `end` removes a kept install from it.
impl Drop for TurnWait {
    fn drop(&mut self) {
        let mut queue = self.queue.lock().expect("acquisition queue mutex poisoned");
        queue.tickets.remove(&self.key);
        queue.waiting.retain(|item| *item != self.key);
    }
}
