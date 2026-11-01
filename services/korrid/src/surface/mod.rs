//! korrid's side of the native surface platform (native surfaces contract,
//! sections 2, 3 and 6).
//!
//! korrid builds the `SurfaceModel` and performs the `SurfaceRequest`s that
//! the portal (`clients/portal/src/surface/`) used to. The shell asks for a
//! model, watches it over the authenticated WebSocket, and hands requests
//! back; it holds no host state.
//!
//! One `SurfaceHub` per portal router. It owns the ported host controllers
//! (`root`), one catalog query and one focused game per surface id, and the
//! preference store. A model is whole: the hub builds it from the
//! controllers' facts each time one changes.

pub mod acquisition_gate;
pub mod catalog;
pub mod controllers;
pub mod downloads;
pub mod korrid;
pub mod launch_model;
pub mod launch_settings;
pub mod launchables;
pub mod model;
pub mod network_onboarding;
pub mod overlay;
pub mod owner_binding;
pub mod plugins;
pub mod preferences;
pub mod root;
pub mod runner;
pub mod saves;
pub mod settings_model;
pub mod shared_play;
pub mod starter_pack;
pub mod store;
#[cfg(test)]
mod tests;
pub mod watch;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use korri_surface_treaty::{
    CatalogFrom, CatalogQuery, FocusedGame, SurfaceCatalog, SurfaceModel,
    SurfacePreferenceDeclarations, SurfacePreferenceValues, SurfaceRequest, CATALOG_DEFAULT_COUNT,
    CATALOG_MAX_COUNT,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch as tokio_watch;
use typeshare::typeshare;

use crate::RpcFailure;
use catalog::{CatalogGames, CatalogIndex};
use preferences::PreferenceStore;
use root::Root;

/// Called when a controller published new facts. It only marks the model
/// stale; it never takes a controller lock.
pub type Notify = Arc<dyn Fn() + Send + Sync>;

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceModelRequest {
    pub surface_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SurfaceModelOutcome {
    Ok(Box<SurfaceModel>),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceRequestRequest {
    pub surface_id: String,
    pub request: SurfaceRequest,
}

/// The empty Ok payload of a request korrid accepted.
#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SurfaceAccepted {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SurfaceAcceptedOutcome {
    Ok(SurfaceAccepted),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfacePreferencesRequest {
    pub surface_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SurfacePreferencesOutcome {
    Ok(SurfacePreferenceValues),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceDeclareRequest {
    pub surface_id: String,
    pub preferences: SurfacePreferenceDeclarations,
    /// Absent: `main`, as in the portal's surface registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub displays: Option<korri_surface_treaty::SurfacePackageDisplays>,
}

/// The second frame on the native-input WebSocket that selects a model watch
/// instead of a native input subscription.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum SurfaceWatchSubscription {
    #[serde(rename = "app.surface.watch")]
    Watch(SurfaceModelRequest),
}

pub fn surface_unavailable() -> RpcFailure {
    RpcFailure {
        code: "SurfaceUnavailable".into(),
        message: "Surfaces are not served on this device.".into(),
    }
}

/// `surface_id` is the package folder id (`pico`, `zone`).
pub fn check_surface_id(surface_id: &str) -> Result<(), RpcFailure> {
    let valid = (1..=64).contains(&surface_id.len())
        && surface_id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
        && !surface_id.starts_with('.');
    if valid {
        Ok(())
    } else {
        Err(RpcFailure {
            code: "InvalidSurfaceId".into(),
            message: "A surface id is its package folder name.".into(),
        })
    }
}

/// One surface's current query, window start and focus.
struct Session {
    query: CatalogQuery,
    from: CatalogFrom,
    count: u16,
    index: Option<Arc<CatalogIndex>>,
    focused: Option<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            query: CatalogQuery::default(),
            from: CatalogFrom::Start,
            count: CATALOG_DEFAULT_COUNT,
            index: None,
            focused: None,
        }
    }
}

pub struct SurfaceHub {
    korrid: korrid::Korrid,
    /// Announces every change of korrid's Home intent.
    overlay_intent: tokio_watch::Receiver<u64>,
    controllers: Arc<controllers::Controllers>,
    root: OnceLock<Arc<Root>>,
    /// Bumped by every controller publish.
    host_revision: Arc<AtomicU64>,
    /// Bumped by every change of any model: host facts or a session.
    revision: tokio_watch::Sender<u64>,
    sessions: Mutex<HashMap<String, Session>>,
    games: Mutex<Option<(u64, Arc<CatalogGames>)>>,
    preferences: PreferenceStore,
}

/// What a portal router and its native-input router share: the hub the
/// portal router makes (the watch finds it here), and the controllers the
/// native input feeds (the hub reads them).
#[derive(Default)]
pub struct SurfaceLink {
    pub hub: OnceLock<Arc<SurfaceHub>>,
    pub controllers: Arc<controllers::Controllers>,
}

pub type SurfaceSlot = Arc<SurfaceLink>;

impl SurfaceHub {
    pub fn new(
        korrid: korrid::Korrid,
        private_state_root: PathBuf,
        overlay_intent: tokio_watch::Receiver<u64>,
        controllers: Arc<controllers::Controllers>,
    ) -> Arc<Self> {
        let (revision, _) = tokio_watch::channel(0);
        Arc::new(Self {
            controllers,
            overlay_intent,
            korrid,
            root: OnceLock::new(),
            host_revision: Arc::new(AtomicU64::new(0)),
            revision,
            sessions: Mutex::new(HashMap::new()),
            games: Mutex::new(None),
            preferences: PreferenceStore::new(private_state_root),
        })
    }

    fn bump(&self) {
        self.revision.send_modify(|revision| *revision += 1);
    }

    /// The controllers start with the first surface that asks, so a korrid
    /// nobody shows a surface for polls nothing.
    fn root(self: &Arc<Self>) -> Arc<Root> {
        Arc::clone(self.root.get_or_init(|| {
            let host_revision = Arc::clone(&self.host_revision);
            let revision = self.revision.clone();
            let notify: Notify = Arc::new(move || {
                host_revision.fetch_add(1, Ordering::SeqCst);
                revision.send_modify(|revision| *revision += 1);
            });
            let root = Root::new(
                self.korrid.clone(),
                notify.clone(),
                Arc::clone(&self.controllers),
            );
            let mut controller_changes = self.controllers.subscribe();
            tokio::spawn(async move {
                while controller_changes.changed().await.is_ok() {
                    notify();
                }
            });
            root.start(self.overlay_intent.clone());
            // Effects the portal ran when its state changed.
            let reacting = Arc::downgrade(&root);
            let mut changes = self.revision.subscribe();
            tokio::spawn(async move {
                while changes.changed().await.is_ok() {
                    let Some(root) = reacting.upgrade() else {
                        return;
                    };
                    root.react();
                }
            });
            root
        }))
    }

    pub fn subscribe(&self) -> tokio_watch::Receiver<u64> {
        self.revision.subscribe()
    }

    pub fn revision(&self) -> u64 {
        *self.revision.borrow()
    }

    /// The whole game list, rebuilt only when host facts changed.
    fn games(
        &self,
        host_revision: u64,
        games: Option<Vec<korri_surface_treaty::SurfaceGame>>,
    ) -> Option<Arc<CatalogGames>> {
        let games = games?;
        let mut cached = self.games.lock().expect("games poisoned");
        if let Some((revision, list)) = cached.as_mut() {
            // Most publishes change no game: keep the list, and with it every
            // surface's sorted query.
            if *revision == host_revision || list.games() == games.as_slice() {
                *revision = host_revision;
                return Some(Arc::clone(list));
            }
        }
        let list = Arc::new(CatalogGames::new(games));
        *cached = Some((host_revision, Arc::clone(&list)));
        Some(list)
    }

    /// The current model of one surface.
    pub fn model(self: &Arc<Self>, surface_id: &str) -> Result<SurfaceModel, RpcFailure> {
        check_surface_id(surface_id)?;
        let root = self.root();
        if let Some(overlay) = root.overlay() {
            let mut model = overlay.model();
            model.displays = root.display_facts();
            return Ok(model);
        }
        let host_revision = self.host_revision.load(Ordering::SeqCst);
        let facts = root.facts();
        let games = self.games(host_revision, facts.games);
        let mut model = facts.base;
        model.displays = root.display_facts();
        let mut sessions = self.sessions.lock().expect("sessions poisoned");
        let session = sessions.entry(surface_id.to_owned()).or_default();
        model.catalog = match games {
            None => SurfaceCatalog::Loading,
            Some(games) if games.is_empty() => SurfaceCatalog::Empty,
            Some(games) => {
                let fresh = session.index.as_ref().is_some_and(|index| {
                    Arc::ptr_eq(index.games(), &games) && *index.query() == session.query
                });
                if !fresh {
                    session.index = Some(Arc::new(CatalogIndex::new(games, session.query.clone())));
                }
                session
                    .index
                    .as_ref()
                    .expect("index built")
                    .window(&session.from, session.count)
            }
        };
        let focused = session.focused.clone();
        drop(sessions);
        model.focused_game = focused
            .filter(|id| model::entry_for_id(&facts.state, id).is_some())
            .map(|game_id| FocusedGame {
                actions: root.game_actions(&game_id),
                game_id,
            });
        Ok(model)
    }

    /// Perform one request of one surface.
    pub fn request(
        self: &Arc<Self>,
        surface_id: &str,
        request: SurfaceRequest,
    ) -> Result<(), RpcFailure> {
        check_surface_id(surface_id)?;
        match request {
            SurfaceRequest::MoveCatalog { query, from, count } => {
                let mut sessions = self.sessions.lock().expect("sessions poisoned");
                let session = sessions.entry(surface_id.to_owned()).or_default();
                session.query = query;
                session.from = from;
                session.count = count.clamp(1, CATALOG_MAX_COUNT);
                drop(sessions);
                self.bump();
            }
            SurfaceRequest::FocusGame { game_id } => {
                let mut sessions = self.sessions.lock().expect("sessions poisoned");
                sessions.entry(surface_id.to_owned()).or_default().focused = game_id;
                drop(sessions);
                self.bump();
            }
            SurfaceRequest::RememberPreference { key, value } => {
                self.preferences.remember(surface_id, &key, &value)?;
            }
            other => self.root().perform(other),
        }
        Ok(())
    }

    /// The shell loaded this package: its preferences and its displays. The
    /// last package declared is the one the window shows.
    pub fn declare(
        self: &Arc<Self>,
        surface_id: &str,
        preferences: SurfacePreferenceDeclarations,
        displays: korri_surface_treaty::SurfacePackageDisplays,
    ) -> Result<(), RpcFailure> {
        check_surface_id(surface_id)?;
        self.preferences.declare(surface_id, preferences)?;
        self.root().show_package(displays);
        Ok(())
    }

    pub fn preferences(&self, surface_id: &str) -> Result<SurfacePreferenceValues, RpcFailure> {
        check_surface_id(surface_id)?;
        self.preferences.values(surface_id)
    }
}
