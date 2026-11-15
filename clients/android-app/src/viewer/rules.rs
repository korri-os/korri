// SPDX-License-Identifier: GPL-3.0-or-later
//! The stream's rules that need no Android: the launches queued for an
//! activity when it ends, the end of the runtime's thread, the keys that go
//! on to Android, the files of the pinned server certificates, and the
//! blocking calls from korrid's async code. The
//! library builds for Android only, so the build machine runs these tests
//! through `tests/host.rs` (`cargo test --test host`).
use std::{
    io,
    path::Path,
    sync::{mpsc, Mutex, PoisonError},
    thread,
};

/// Where the adapter hands launches to the activity in front: that
/// activity's number and the sender of its stream's queue.
pub(crate) type Slot<T> = Mutex<Option<(u64, mpsc::Sender<T>)>>;

/// The activity `id` ends. No launch reaches its queue any more, and each
/// launch still in it goes to `refuse`, so that its lifecycle fails and
/// korrid's start hears why. A launch that is only dropped leaves its
/// lifecycle unfinished, and the adapter then refuses every later start.
///
/// The adapter sends while it holds the slot's lock, so after the slot is
/// emptied under that lock nothing more arrives.
pub(crate) fn close<T>(
    slot: &Slot<T>,
    id: u64,
    queue: &mpsc::Receiver<T>,
    mut refuse: impl FnMut(T),
) {
    let mut shows = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if shows.as_ref().is_some_and(|(owner, _)| *owner == id) {
        *shows = None;
    }
    drop(shows);
    while let Ok(launch) = queue.try_recv() {
        refuse(launch);
    }
}

/// What the runtime's lifecycle must still hear once its thread has ended.
/// A runtime that returns has reported its own end, a failure too. One whose
/// thread panicked reported nothing: its lifecycle would never finish, and
/// the adapter would refuse every later start.
pub(crate) fn unreported<T>(joined: &thread::Result<T>) -> Option<&'static str> {
    joined
        .as_ref()
        .err()
        .map(|_| "the viewer's thread panicked")
}

/// Whether the stream keeps a key from the shell under it. `android`: one of
/// Android's own keys (volume, power, `overlay::is_system_key`). `taken`:
/// the stream or its overlay took it.
///
/// While the stream stops nothing takes a key, and Android's own keys go on
/// to Android. Any other key stays the stream's, taken or not: one that
/// reached Slint under the video once raised the soft keyboard over it.
pub(crate) fn keeps_key(android: bool, stopping: bool, taken: bool) -> bool {
    if stopping {
        !android
    } else {
        taken || !android
    }
}

/// The file name of a pinned server certificate: one launch's, at one of
/// its starts (korrid's recovery starts a launch again).
pub(crate) fn pin_name(launch: &str, start: u64) -> String {
    format!("viewer-pin-{launch}-{start}.pem")
}

/// Whether `name` is a pinned server certificate's (`pin_name`).
fn is_pin(name: &str) -> bool {
    name.starts_with("viewer-pin-") && name.ends_with(".pem")
}

/// Remove the pinned server certificates that an earlier process left in
/// `directory`: a process that Android ends drops no `Pin`. When the adapter
/// is made no launch plays yet, so every one is stale. Only regular files
/// go; a link or a folder of that name stays. Returns how many it removed.
pub(crate) fn remove_stale_pins(directory: &Path) -> io::Result<usize> {
    let mut removed = 0;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let pin = entry.file_name().to_str().is_some_and(is_pin);
        // The entry's own type: a link is not followed.
        if pin && entry.file_type()?.is_file() {
            std::fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// A treaty request, or any call that waits or goes into Android, from
/// korrid's async code: on a thread of the runtime's blocking pool, so that
/// korrid's async worker stays free. `None` when it panicked.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

/// Start `work` with `spawn`; its value goes to `answer`. When no thread
/// starts, `refused` goes to `answer` here instead, so that whoever waits for
/// the answer is not left waiting. The error says why no thread started.
pub(crate) fn spawn_answering<T: Send + 'static>(
    spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> io::Result<()>,
    answer: mpsc::Sender<T>,
    work: impl FnOnce() -> T + Send + 'static,
    refused: T,
) -> io::Result<()> {
    let sender = answer.clone();
    spawn(Box::new(move || {
        let _ = sender.send(work());
    }))
    .inspect_err(|_| {
        let _ = answer.send(refused);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::symlink,
        path::PathBuf,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    // D11 review: Snapshot ran the lifecycle's request, with the viewer's
    // battery read through JNI, on korrid's async worker. On a runtime with
    // one worker, work that waits must leave that worker free.
    #[test]
    fn blocking_work_leaves_korrids_async_worker_free() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let freed = runtime.block_on(async {
            let (ran, heard) = mpsc::channel();
            let other = tokio::spawn(async move { ran.send(()).unwrap() });
            let freed = blocking(move || heard.recv_timeout(Duration::from_secs(2)).is_ok()).await;
            other.await.unwrap();
            freed
        });
        assert_eq!(freed, Some(true));
    }

    // Review of D5b: when the leave's thread does not start, the opening
    // must still hear an answer, or it stays lent with the input held.
    #[test]
    fn a_thread_that_does_not_start_still_answers_with_the_refusal() {
        let (answer, answers) = mpsc::channel();
        let failed = spawn_answering(
            |_| Err(io::Error::other("no thread")),
            answer,
            || (7, true),
            (7, false),
        );
        assert!(failed.is_err());
        assert_eq!(answers.recv_timeout(Duration::from_secs(1)), Ok((7, false)));
    }

    #[test]
    fn a_thread_that_starts_answers_with_its_work() {
        let (answer, answers) = mpsc::channel();
        let started = spawn_answering(
            |work| thread::Builder::new().spawn(work).map(drop),
            answer,
            || (8, true),
            (8, false),
        );
        assert!(started.is_ok());
        assert_eq!(answers.recv_timeout(Duration::from_secs(5)), Ok((8, true)));
    }

    #[test]
    fn an_activity_that_ends_refuses_every_launch_still_queued() {
        let (sender, queue) = mpsc::channel();
        let slot: Slot<&str> = Mutex::new(Some((1, sender.clone())));
        sender.send("first").unwrap();
        sender.send("second").unwrap();
        drop(sender);
        let mut refused = Vec::new();
        close(&slot, 1, &queue, |launch| refused.push(launch));
        assert_eq!(refused, ["first", "second"]);
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn a_newer_activity_keeps_its_slot_and_the_old_queue_is_still_refused() {
        let (old, queue) = mpsc::channel();
        old.send("queued before the newer activity came").unwrap();
        drop(old);
        let (newer, _newer_queue) = mpsc::channel::<&str>();
        let slot: Slot<&str> = Mutex::new(Some((2, newer)));
        let mut refused = Vec::new();
        close(&slot, 1, &queue, |launch| refused.push(launch));
        assert_eq!(refused, ["queued before the newer activity came"]);
        assert_eq!(slot.lock().unwrap().as_ref().map(|(id, _)| *id), Some(2));
    }

    #[test]
    fn a_runtime_thread_that_panicked_still_fails_its_lifecycle() {
        let panicked =
            thread::spawn(|| -> Result<(), String> { panic!("the runtime panicked") }).join();
        assert_eq!(unreported(&panicked), Some("the viewer's thread panicked"));
        // A runtime that returns reports its own end, also a failure.
        let ended = thread::spawn(|| -> Result<(), String> { Ok(()) }).join();
        assert_eq!(unreported(&ended), None);
        let failed = thread::spawn(|| -> Result<(), String> { Err("refused".into()) }).join();
        assert_eq!(unreported(&failed), None);
    }

    #[test]
    fn android_keys_go_on_to_android_also_while_the_stream_stops() {
        assert!(
            !keeps_key(true, true, false),
            "volume while the stream stops"
        );
        assert!(
            keeps_key(false, true, false),
            "a letter while the stream stops"
        );
        assert!(
            !keeps_key(true, false, false),
            "volume that the stream leaves"
        );
        assert!(
            keeps_key(false, false, false),
            "a letter that the stream leaves"
        );
        assert!(keeps_key(false, false, true), "a key that the stream takes");
    }

    fn scratch() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "korri-android-app-pins-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn stale_pins_go_and_nothing_else_does() {
        let directory = scratch();
        let stale = [pin_name("0123", 1), pin_name("4567", 2)];
        for name in &stale {
            fs::write(directory.join(name), "certificate").unwrap();
        }
        fs::write(directory.join("other.pem"), "kept").unwrap();
        fs::write(directory.join("viewer-pin-note.txt"), "kept").unwrap();
        symlink("other.pem", directory.join(pin_name("link", 3))).unwrap();
        fs::create_dir(directory.join(pin_name("folder", 4))).unwrap();

        assert_eq!(remove_stale_pins(&directory).unwrap(), 2);
        let mut left: Vec<String> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "other.pem",
                "viewer-pin-folder-4.pem",
                "viewer-pin-link-3.pem",
                "viewer-pin-note.txt",
            ]
        );
        fs::remove_dir_all(&directory).unwrap();
    }
}
