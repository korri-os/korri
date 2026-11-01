//! Port of `clients/portal/src/surface/starter-pack-onboarding.ts`: the
//! Starter pack offered once per setup session. No persisted state beyond the
//! plugin host's receipt.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use korri_surface_treaty::{SurfaceAction, SurfaceStarterPack};

use super::{korrid::Korrid, owner_binding::Hook, Notify};
use crate::{
    LibraryFoldersSnapshotOutcome, LibraryFoldersState, PluginChangeOutcome,
    PluginRepositoriesOutcome,
};

const PACK_ID: &str = "@korri:starter-pack";
pub const INSTALL_ACTION: &str = "starter-pack-install";
const FAILURE: &str = "Could not install the Starter pack. Try again.";

#[derive(Default)]
struct Inner {
    state: Option<SurfaceStarterPack>,
    needed: bool,
    busy: bool,
    pack_enabled: bool,
    generation: u64,
}

pub struct StarterPack {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    on_installed: Hook,
    poll_interval: Duration,
    discovery_timeout: Duration,
}

fn failed() -> SurfaceStarterPack {
    SurfaceStarterPack::Failed {
        message: FAILURE.into(),
        actions: vec![SurfaceAction::new(INSTALL_ACTION, "Retry", true)],
    }
}

fn pack_enabled(outcome: &PluginRepositoriesOutcome) -> Option<bool> {
    match outcome {
        PluginRepositoriesOutcome::Ok(repositories) => repositories
            .other_installs
            .iter()
            .find(|install| install.id == PACK_ID)
            .map(|install| install.enabled),
        PluginRepositoriesOutcome::Err(_) => None,
    }
}

impl StarterPack {
    pub fn new(korrid: Korrid, notify: Notify, on_installed: Hook) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner::default()),
            notify,
            on_installed,
            poll_interval: Duration::from_millis(250),
            discovery_timeout: Duration::from_secs(60),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("starter pack poisoned")
    }

    pub fn snapshot(&self) -> Option<SurfaceStarterPack> {
        self.lock().state.clone()
    }

    fn publish(&self, next: Option<SurfaceStarterPack>) {
        self.lock().state = next;
        (self.notify)();
    }

    async fn wait_for_discovery(&self, initial: LibraryFoldersSnapshotOutcome) -> Result<(), ()> {
        let wait = async {
            let mut outcome = initial;
            loop {
                match &outcome {
                    LibraryFoldersSnapshotOutcome::Err(_) => return Err(()),
                    LibraryFoldersSnapshotOutcome::Ok(snapshot) => match snapshot.state {
                        LibraryFoldersState::Idle {} => return Ok(()),
                        LibraryFoldersState::Problem {} => return Err(()),
                        LibraryFoldersState::Scanning {} | LibraryFoldersState::Enriching {} => {}
                    },
                }
                tokio::time::sleep(self.poll_interval).await;
                outcome = self.korrid.library_folders_snapshot().await;
            }
        };
        tokio::time::timeout(self.discovery_timeout, wait)
            .await
            .unwrap_or(Err(()))
    }

    async fn install(self: Arc<Self>, retry: bool) {
        let current = {
            let mut inner = self.lock();
            if inner.busy {
                return;
            }
            inner.busy = true;
            inner.generation
        };
        let visible = |this: &Self| {
            let inner = this.lock();
            inner.needed && inner.generation == current
        };
        self.publish(Some(SurfaceStarterPack::Installing));
        let result: Result<(), ()> = async {
            // A lost enable reply may still have committed. Retry reads the
            // receipt before repeating that effect.
            if retry && !self.lock().pack_enabled {
                let repositories = self.korrid.plugin_repositories().await;
                if matches!(repositories, PluginRepositoriesOutcome::Err(_)) {
                    return Err(());
                }
                self.lock().pack_enabled = pack_enabled(&repositories) == Some(true);
            }
            if !self.lock().pack_enabled {
                let enabled = self.korrid.set_plugin_enabled(PACK_ID.into(), true).await;
                if matches!(enabled, PluginChangeOutcome::Err(_)) {
                    return Err(());
                }
                self.lock().pack_enabled = true;
            }
            // Finishing setup hides this operation, but never cancels its effects.
            let rescanned = self.korrid.rescan_library_folders().await;
            self.wait_for_discovery(rescanned).await?;
            (self.on_installed)();
            Ok(())
        }
        .await;
        if visible(&self) {
            self.publish(Some(match result {
                Ok(()) => SurfaceStarterPack::Installed,
                Err(()) => failed(),
            }));
        }
        self.lock().busy = false;
    }

    async fn read(self: Arc<Self>, current: u64) {
        let outcome = self.korrid.plugin_repositories().await;
        {
            let inner = self.lock();
            if !inner.needed || current != inner.generation {
                return;
            }
        }
        if pack_enabled(&outcome) == Some(false) {
            self.lock().pack_enabled = false;
            self.publish(Some(SurfaceStarterPack::Offered {
                actions: vec![SurfaceAction::new(INSTALL_ACTION, "Install", true)],
            }));
        }
    }

    pub fn set_needed(self: &Arc<Self>, next: bool) {
        let current = {
            let mut inner = self.lock();
            if inner.needed == next {
                return;
            }
            inner.needed = next;
            inner.generation += 1;
            inner.generation
        };
        self.publish(None);
        if next {
            tokio::spawn(Arc::clone(self).read(current));
        }
    }

    pub fn act(self: &Arc<Self>, id: &str) -> bool {
        if id != INSTALL_ACTION {
            return false;
        }
        let retry = {
            let inner = self.lock();
            match (&inner.state, inner.needed) {
                (Some(SurfaceStarterPack::Offered { .. }), true) => Some(false),
                (Some(SurfaceStarterPack::Failed { .. }), true) => Some(true),
                _ => None,
            }
        };
        if let Some(retry) = retry {
            tokio::spawn(Arc::clone(self).install(retry));
        }
        true
    }
}
