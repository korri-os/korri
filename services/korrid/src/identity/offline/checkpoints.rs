//! Test-build-only observation of the offline replacement's real persistence.
//! The subprocess test installs a thread-local observer; other tests do not stop.
//! No operation is replaced, and no runtime option enables these checkpoints.
use std::{cell::Cell, fs::File};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Checkpoint {
    Staged,
    BeforeRename,
    Renamed,
}

type Observer = fn(Checkpoint, &File);

thread_local! {
    static OBSERVER: Cell<Option<Observer>> = const { Cell::new(None) };
}

pub(crate) fn install(observer: Observer) {
    OBSERVER.with(|slot| assert!(slot.replace(Some(observer)).is_none()));
}

pub(super) fn observe(checkpoint: Checkpoint, directory: &File) {
    OBSERVER.with(|slot| {
        if let Some(observer) = slot.get() {
            observer(checkpoint, directory);
        }
    });
}
