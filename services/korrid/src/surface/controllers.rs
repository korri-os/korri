//! Port of `clients/portal/src/input/controllers.ts`: controller presence
//! and evidence, fed by the native input korrid already publishes to the
//! portal socket (`surface_host_input.rs`). A gamepad's arrival adds it; a live
//! sample after its baseline proves it was heard; its removal drops it.

use std::{collections::HashSet, sync::Mutex};

use korri_surface_treaty::SurfaceController;
use tokio::sync::watch;

pub struct Controllers {
    inner: Mutex<(Vec<SurfaceController>, HashSet<String>)>,
    changed: watch::Sender<u64>,
}

impl Default for Controllers {
    fn default() -> Self {
        Self {
            inner: Mutex::new((Vec::new(), HashSet::new())),
            changed: watch::channel(0).0,
        }
    }
}

impl Controllers {
    fn publish(&self, change: impl FnOnce(&mut Vec<SurfaceController>, &mut HashSet<String>)) {
        let changed = {
            let mut inner = self.inner.lock().expect("controllers poisoned");
            let (controllers, heard) = &mut *inner;
            let before = controllers.clone();
            change(controllers, heard);
            *controllers != before
        };
        if changed {
            self.changed.send_modify(|revision| *revision += 1);
        }
    }

    pub fn snapshot(&self) -> Vec<SurfaceController> {
        self.inner.lock().expect("controllers poisoned").0.clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    pub fn added(&self, id: &str, name: &str) {
        self.publish(|controllers, heard| {
            controllers.retain(|controller| controller.id != id);
            controllers.push(SurfaceController {
                id: id.into(),
                name: name.into(),
                heard: heard.contains(id),
            });
        });
    }

    pub fn removed(&self, id: &str) {
        self.publish(|controllers, _| controllers.retain(|controller| controller.id != id));
    }

    /// Input from a known gamepad after its baseline.
    pub fn input(&self, id: &str) {
        self.publish(|controllers, heard| {
            heard.insert(id.into());
            for controller in controllers
                .iter_mut()
                .filter(|controller| controller.id == id)
            {
                controller.heard = true;
            }
        });
    }

    /// Setup has begun again: every pad must prove itself afresh.
    pub fn forget(&self) {
        self.publish(|controllers, heard| {
            heard.clear();
            for controller in controllers.iter_mut() {
                controller.heard = false;
            }
        });
    }

    /// The input producer was lost.
    pub fn disconnected(&self) {
        self.publish(|controllers, _| controllers.clear());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrival_input_removal_and_forgetting() {
        let controllers = Controllers::default();
        controllers.input("pad");
        controllers.added("pad", "Xbox Pad");
        // Input heard before its arrival still proves it.
        assert_eq!(
            controllers.snapshot(),
            vec![SurfaceController {
                id: "pad".into(),
                name: "Xbox Pad".into(),
                heard: true
            }]
        );
        controllers.added("other", "8BitDo");
        assert!(!controllers.snapshot()[1].heard);
        controllers.forget();
        assert!(controllers
            .snapshot()
            .iter()
            .all(|controller| !controller.heard));
        controllers.added("pad", "Xbox Pad");
        assert_eq!(controllers.snapshot().last().unwrap().id, "pad");
        assert!(!controllers.snapshot().last().unwrap().heard);
        controllers.removed("pad");
        assert_eq!(controllers.snapshot().len(), 1);
        controllers.disconnected();
        assert!(controllers.snapshot().is_empty());
    }
}
