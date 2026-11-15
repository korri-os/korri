// SPDX-License-Identifier: GPL-3.0-or-later
//! The stream's rules that need no Android: the launches queued for an
//! activity when it ends, the end of the runtime's thread, and the keys
//! that go on to Android. The library builds for Android only, so the
//! build machine runs these tests through `tests/host.rs`
//! (`cargo test --test host`).
use std::{
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
