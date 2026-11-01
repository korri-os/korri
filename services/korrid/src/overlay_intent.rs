//! The process-local Home intent: the exact launch a Home freeze handed to
//! the gameplay overlay. Every change is announced, so the surface host can
//! show the overlay the moment the intent lands instead of polling for it.

use std::{
    ops::{Deref, DerefMut},
    sync::{Mutex, MutexGuard},
};

use tokio::sync::watch;

pub struct OverlayIntent {
    value: Mutex<Option<String>>,
    changed: watch::Sender<u64>,
}

impl Default for OverlayIntent {
    fn default() -> Self {
        Self {
            value: Mutex::new(None),
            changed: watch::channel(0).0,
        }
    }
}

/// Holds the intent; announces a change when dropped.
pub struct IntentGuard<'a> {
    guard: MutexGuard<'a, Option<String>>,
    before: Option<String>,
    changed: &'a watch::Sender<u64>,
}

impl Deref for IntentGuard<'_> {
    type Target = Option<String>;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}
impl DerefMut for IntentGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}
impl Drop for IntentGuard<'_> {
    fn drop(&mut self) {
        if *self.guard != self.before {
            self.changed.send_modify(|revision| *revision += 1);
        }
    }
}

#[derive(Debug)]
pub struct Poisoned;

impl OverlayIntent {
    pub fn lock(&self) -> Result<IntentGuard<'_>, Poisoned> {
        let guard = self.value.lock().map_err(|_| Poisoned)?;
        Ok(IntentGuard {
            before: guard.clone(),
            guard,
            changed: &self.changed,
        })
    }
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_change_is_announced() {
        let intent = OverlayIntent::default();
        let mut changes = intent.subscribe();
        *intent.lock().unwrap() = Some("a".into());
        assert!(changes.has_changed().unwrap());
        changes.borrow_and_update();
        *intent.lock().unwrap() = Some("a".into());
        let _ = intent.lock().unwrap().is_some();
        assert!(!changes.has_changed().unwrap());
        intent.lock().unwrap().take();
        assert!(changes.has_changed().unwrap());
    }
}
