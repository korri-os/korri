//! Port of `clients/portal/src/surface/SurfaceRoot.tsx`: where Korri meets a
//! surface. It composes the host controllers, builds one `SurfaceModel`
//! from their facts, and performs each `SurfaceRequest` with the same rules
//! the portal's `SurfaceHost` methods had.

use super::launch_steps::{LaunchPlan, LaunchStep};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

use korri_surface_treaty::{
    SurfaceAction, SurfaceGame, SurfaceModel, SurfaceOnboarding, SurfacePresentation,
    SurfaceRequest, SurfaceStatus,
};

use super::{
    acquisition_gate::{AcquisitionGameRef, AcquisitionGate},
    downloads::{DownloadGameRef, GameDownloads, ACQUISITION_POLL_INTERVAL},
    korrid::Korrid,
    launch_model::launch_id,
    launch_settings::{LaunchSettings, LAUNCH_SETTINGS_ACTION_ID},
    launchables::driver::SessionStartResult,
    launchables::{
        driver::{Launchables, LaunchablesHooks},
        state::{Entry, LaunchablesState, Loaded, Phase},
    },
    model::{
        catalog_games, entry_for_id, entry_for_launch_location, game_actions_for_entry,
        launch_locations_for_entry, status_from, surface_download_from, GameDownloadContext,
        DOWNLOAD_ACTION_IDS,
    },
    network_onboarding::NetworkOnboarding,
    overlay::{focused_overlay_session, OverlayController, OverlayExit},
    owner_binding::{owner_binding_status, OwnerBinding},
    plugins::{plugin_setting_groups, PluginFacts, PluginsController, PluginsHost},
    runner::{Intent, RunnerChooser, RunnerLaunchIntegration},
    saves::{
        surface_saves_from, SaveGameRef, SaveSync, SAVE_CHECK_LIMIT, SAVE_CHOOSE_ACTION_ID,
        SAVE_HISTORY_ACTION_ID,
    },
    settings_model::settings_from,
    shared_play::{surface_invites_from, SharedPlayContext, SharedPlayController},
    starter_pack::StarterPack,
    store::StoreAcquisition,
    Notify,
};

/// korrid ids of every game this device can launch itself, sorted.
fn local_game_ids_of(state: &LaunchablesState) -> Vec<String> {
    let mut ids: Vec<String> = state
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Game { game, .. } if game.source.is_local => Some(game.id.clone()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids
}

/// Surface id -> korrid id, for the games the download side can speak about.
fn local_game_ids_by_surface_id(state: &LaunchablesState) -> BTreeMap<String, String> {
    state
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Game { game, .. } if game.source.is_local => {
                Some((entry.key(), game.id.clone()))
            }
            _ => None,
        })
        .collect()
}

/// The launchable form of a game on this device: the catalog copy with
/// `source.isLocal`, which alone has a launch path.
fn launchable_on_this_device(entry: Entry) -> Entry {
    if let Entry::LocalGame { alternatives, .. } = &entry {
        if let Some(game) = alternatives
            .iter()
            .filter_map(|copy| copy.remote())
            .find(|game| game.source.is_local)
        {
            return Entry::Game {
                game: game.clone(),
                alternatives: Vec::new(),
            };
        }
    }
    entry
}

/// The local catalog copy with installed-runner selection, if any.
fn local_runner_entry(entry: Option<&Entry>) -> Option<crate::Game> {
    match entry {
        Some(Entry::Game { game, .. })
            if game.source.is_local && game.supports_runner_selection =>
        {
            Some(game.clone())
        }
        Some(entry @ (Entry::Game { .. } | Entry::LocalGame { .. })) => entry
            .alternatives()
            .iter()
            .filter_map(|copy| copy.remote())
            .find(|game| game.source.is_local && game.supports_runner_selection)
            .cloned(),
        _ => None,
    }
}

/// The korrid id of a game this device can play that korrid linked to a
/// provider's claim (ADR 0016). Only local copies count.
fn korrid_game_for_claim(
    state: &LaunchablesState,
    provider_id: &str,
    claim_id: &str,
) -> Option<String> {
    for entry in state.entries() {
        if entry.is_now_playing() {
            continue;
        }
        let own = match entry {
            Entry::LocalGame { game, .. } => Some((true, game.identity.as_ref(), game.id.clone())),
            Entry::Game { game, .. } => Some((
                game.source.is_local,
                game.identity.as_ref(),
                game.id.clone(),
            )),
            Entry::NowPlaying { .. } => None,
        };
        let copies = own
            .into_iter()
            .chain(entry.alternatives().iter().map(|copy| match copy {
                super::launchables::fold::GameCopy::Local(game) => {
                    (true, game.identity.as_ref(), game.id.clone())
                }
                super::launchables::fold::GameCopy::Remote(game) => (
                    game.source.is_local,
                    game.identity.as_ref(),
                    game.id.clone(),
                ),
            }));
        for (local, identity, id) in copies {
            if let (true, Some(crate::GameIdentity::Provider(provider))) = (local, identity) {
                if provider.provider == provider_id && provider.provider_ref == claim_id {
                    return Some(id);
                }
            }
        }
    }
    None
}

/// The surface's id for a korrid game, once the catalog holds it.
fn surface_id_for_korrid_game(state: &LaunchablesState, game_id: &str) -> Option<String> {
    state
        .entries()
        .iter()
        .find(|entry| match entry {
            Entry::NowPlaying { .. } => false,
            Entry::LocalGame { game, .. } => {
                game.id == game_id || entry.alternatives().iter().any(|copy| copy.id() == game_id)
            }
            Entry::Game { game, .. } => {
                game.id == game_id || entry.alternatives().iter().any(|copy| copy.id() == game_id)
            }
        })
        .map(Entry::key)
}

/// The store as the surface sees it: an imported game by the surface's id.
fn store_for_surface(
    mut store: korri_surface_treaty::SurfaceStore,
    state: &LaunchablesState,
) -> korri_surface_treaty::SurfaceStore {
    if let korri_surface_treaty::SurfaceStoreDetails::Ready {
        get: korri_surface_treaty::SurfaceStoreGet::InLibrary { game_id, .. },
        ..
    } = &mut store.details
    {
        if let Some(korrid_id) = game_id.take() {
            *game_id = surface_id_for_korrid_game(state, &korrid_id);
        }
    }
    store
}

fn save_ref(game: &AcquisitionGameRef) -> SaveGameRef {
    SaveGameRef {
        id: game.id.clone(),
        surface_id: game.surface_id.clone(),
        title: game.title.clone(),
        runner_id: game.runner_id.clone(),
    }
}

/// This device's own key, as a local catalog game's source states it.
fn local_device_keys_of(state: &LaunchablesState) -> std::collections::HashSet<String> {
    state
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Game { game, .. } if game.source.is_local => {
                game.source.device_public_key.clone()
            }
            _ => None,
        })
        .collect()
}

/// korrid ids of games with a live session, sorted.
fn live_game_ids_of(state: &LaunchablesState) -> Vec<String> {
    let mut ids: Vec<String> = state
        .entries()
        .iter()
        .filter_map(|entry| {
            entry
                .session()
                .map(|session| session.game_id.clone().unwrap_or_default())
        })
        .collect();
    ids.sort();
    ids
}

type StartDone =
    Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<Option<SessionStartResult>>>>>;

fn finish(done: &StartDone, outcome: Option<SessionStartResult>) {
    if let Some(sender) = done.lock().expect("start poisoned").take() {
        let _ = sender.send(outcome);
    }
}

/// Local time as the surface should print it.
pub fn clock_label() -> String {
    // SAFETY: localtime_r writes only into the provided struct.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut local: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut local).is_null() {
            return String::new();
        }
        let hour = local.tm_hour % 12;
        format!(
            "{}:{:02} {}",
            if hour == 0 { 12 } else { hour },
            local.tm_min,
            if local.tm_hour < 12 { "AM" } else { "PM" }
        )
    }
}

/// Port of `surface-displays.ts` and SurfaceRoot's catalog restore: the
/// window's placement for the package the shell shows.
#[derive(Default)]
struct Displays {
    /// The scope of the package declared last; None before any.
    scope: Option<korri_surface_treaty::SurfacePackageDisplays>,
    /// Which placement succeeded, which is in flight, which waits.
    placement: super::placement::Placement,
    facts: Option<Vec<korri_surface_treaty::SurfaceDisplay>>,
    /// A game held the displays; restore the catalog's after it is gone.
    restore: bool,
}

fn host_scope(
    scope: korri_surface_treaty::SurfacePackageDisplays,
) -> crate::host::display_placement::SurfaceDisplayScope {
    match scope {
        korri_surface_treaty::SurfacePackageDisplays::Main => {
            crate::host::display_placement::SurfaceDisplayScope::Main
        }
        korri_surface_treaty::SurfacePackageDisplays::All => {
            crate::host::display_placement::SurfaceDisplayScope::All
        }
    }
}

/// Which presentation the surface shows: the catalog, or the gameplay
/// overlay of one exact launch (`main.tsx`'s switch).
enum Presentation {
    Catalog {
        /// After Open Korri: only this launch's focus-failed phase reopens.
        retained_failure: Option<String>,
    },
    Overlay(Arc<OverlayController>),
}

pub struct Root {
    korrid: Korrid,
    /// The launch under way from this surface and its steps (amendment A4).
    launch_plan: std::sync::Mutex<Option<LaunchPlan>>,
    /// The press's plan is being read: a second press waits for it.
    planning: std::sync::atomic::AtomicBool,
    presentation: std::sync::Mutex<Presentation>,
    pub launchables: Arc<Launchables>,
    pub network: Arc<NetworkOnboarding>,
    pub owner_binding: Arc<OwnerBinding>,
    pub starter_pack: Arc<StarterPack>,
    pub downloads: Arc<GameDownloads>,
    pub acquisition: Arc<AcquisitionGate>,
    pub runner: Arc<RunnerChooser>,
    pub saves: Arc<SaveSync>,
    pub launch: Arc<LaunchSettings>,
    pub store: Arc<StoreAcquisition>,
    pub plugins: Arc<PluginsController>,
    pub shared: Arc<SharedPlayController>,
    plugin_facts: Arc<std::sync::Mutex<PluginFacts>>,
    last_library: std::sync::Mutex<Option<String>>,
    last_live_ids: std::sync::Mutex<Option<Vec<String>>>,
    runner_was_closed: std::sync::Mutex<bool>,
    controllers: Arc<super::controllers::Controllers>,
    displays: std::sync::Mutex<Displays>,
    onboarding_was_needed: std::sync::Mutex<Option<bool>>,
    clock: std::sync::Mutex<String>,
    last_local_ids: std::sync::Mutex<Option<Vec<String>>>,
    notify: Notify,
}

/// The facts the model is built from, read once per publish.
pub struct Facts {
    pub base: SurfaceModel,
    pub games: Option<Vec<SurfaceGame>>,
    pub state: LaunchablesState,
}

impl Root {
    pub fn new(
        korrid: Korrid,
        notify: Notify,
        controllers: Arc<super::controllers::Controllers>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|this: &std::sync::Weak<Root>| {
            let launchables_korrid = korrid.clone();
            // The chooser speaks korrid ids and starts through the shared gate.
            let runner = {
                let start = this.clone();
                let back = this.clone();
                let reload = this.clone();
                RunnerChooser::new(
                    korrid.clone(),
                    notify.clone(),
                    RunnerLaunchIntegration {
                        start_launch: Box::new(move |id, runner_id| {
                            let root = start.upgrade();
                            Box::pin(async move {
                                let root = root?;
                                root.start_from_chooser(id, runner_id).await
                            })
                        }),
                        return_session: Box::new(move |session| {
                            if let Some(root) = back.upgrade() {
                                root.launchables.resume_catalog_session(session);
                            }
                        }),
                        reload: Box::new(move || {
                            if let Some(root) = reload.upgrade() {
                                root.launchables.reload();
                            }
                        }),
                    },
                )
            };
            let network = NetworkOnboarding::new(korrid.clone(), notify.clone());
            let downloads =
                GameDownloads::new(korrid.clone(), notify.clone(), ACQUISITION_POLL_INTERVAL);
            // Downloads a launch needs come before reserve/start; the gate tells
            // the downloads store when a launch's download changed what is on disk.
            let acquisition = {
                let settled = Arc::downgrade(&downloads);
                AcquisitionGate::new(
                    korrid.clone(),
                    notify.clone(),
                    Arc::new(move || {
                        if let Some(downloads) = settled.upgrade() {
                            tokio::spawn(async move { downloads.refresh(None).await });
                        }
                    }),
                    ACQUISITION_POLL_INTERVAL,
                )
            };
            {
                // The gate's open launch counts as a pending download.
                let gate = Arc::downgrade(&acquisition);
                downloads.set_held_games(Arc::new(move || {
                    gate.upgrade()
                        .and_then(|gate| gate.game_id())
                        .into_iter()
                        .collect()
                }));
            }
            let launchables = {
                let restart = Arc::clone(&network);
                let dismiss = Arc::clone(&network);
                Launchables::new(
                    korrid.clone(),
                    notify.clone(),
                    LaunchablesHooks {
                        launch_warnings: {
                            // Prepare-path warnings show in the chooser's Warnings panel.
                            let runner = Arc::clone(&runner);
                            Box::new(move |title, warnings| runner.show_warnings(title, warnings))
                        },
                        // Plugin facts are read beside the rest and publish on their own.
                        plugins_refresh: {
                            let root = this.clone();
                            Box::new(move || {
                                if let Some(root) = root.upgrade() {
                                    tokio::spawn(async move { root.plugins.refresh().await });
                                }
                            })
                        },
                        plugins_run: {
                            let root = this.clone();
                            Box::new(move |id| {
                                root.upgrade().is_some_and(|root| root.plugins.run(id))
                            })
                        },
                        plugins_change: {
                            let root = this.clone();
                            Box::new(move |id, value| {
                                root.upgrade()
                                    .is_some_and(|root| root.plugins.change(id, value))
                            })
                        },
                        restart_onboarding: Box::new(move || restart.restart_onboarding()),
                        dismiss_network_problem: Box::new(move || dismiss.dismiss_problem()),
                    },
                )
            };
            let owner_binding = {
                let changed = Arc::downgrade(&launchables);
                let finish = Arc::clone(&network);
                OwnerBinding::new(
                    korrid.clone(),
                    notify.clone(),
                    Arc::new(move || {
                        if let Some(launchables) = changed.upgrade() {
                            tokio::spawn(async move { launchables.refresh().await });
                        }
                    }),
                    Arc::new(move || finish.finish_onboarding()),
                )
            };
            let starter_pack = {
                let reload = Arc::downgrade(&launchables);
                StarterPack::new(
                    korrid.clone(),
                    notify.clone(),
                    Arc::new(move || {
                        if let Some(launchables) = reload.upgrade() {
                            launchables.reload();
                        }
                    }),
                )
            };
            let saves = {
                let keys = this.clone();
                SaveSync::new(
                    korrid.clone(),
                    notify.clone(),
                    Arc::new(move || {
                        keys.upgrade()
                            .map(|root| local_device_keys_of(&root.launchables.state()))
                            .unwrap_or_default()
                    }),
                    SAVE_CHECK_LIMIT,
                )
            };
            let store = {
                let owned = this.clone();
                let imported = this.clone();
                StoreAcquisition::new(
                    korrid.clone(),
                    notify.clone(),
                    Arc::new(move |provider_id, claim_id| {
                        owned.upgrade().and_then(|root| {
                            korrid_game_for_claim(&root.launchables.state(), provider_id, claim_id)
                        })
                    }),
                    // An imported game joins the catalog by re-reading it in place.
                    Arc::new(move |_| {
                        if let Some(root) = imported.upgrade() {
                            tokio::spawn(async move { root.launchables.refresh().await });
                        }
                    }),
                    ACQUISITION_POLL_INTERVAL,
                )
            };
            let plugin_facts = Arc::new(std::sync::Mutex::new(PluginFacts::default()));
            let plugins = {
                let read = Arc::clone(&plugin_facts);
                let write = Arc::clone(&plugin_facts);
                let published = notify.clone();
                let status = this.clone();
                let busy_get = this.clone();
                let busy_set = this.clone();
                let changed = this.clone();
                PluginsController::new(
                    korrid.clone(),
                    PluginsHost {
                        facts: Box::new(move || {
                            read.lock().expect("plugin facts poisoned").clone()
                        }),
                        publish: Box::new(move |next| {
                            *write.lock().expect("plugin facts poisoned") = next;
                            published();
                        }),
                        status: Box::new(move |next| {
                            if let Some(root) = status.upgrade() {
                                root.launchables.set_settings_status(next);
                            }
                        }),
                        busy_get: Box::new(move || {
                            busy_get
                                .upgrade()
                                .is_some_and(|root| root.launchables.settings_busy())
                        }),
                        busy_set: Box::new(move |value| {
                            if let Some(root) = busy_set.upgrade() {
                                root.launchables.set_settings_busy(value);
                            }
                        }),
                        // Installing or switching a plugin changes what can be played.
                        changed: Box::new(move || {
                            if let Some(root) = changed.upgrade() {
                                tokio::spawn(async move { root.launchables.refresh().await });
                            }
                        }),
                    },
                )
            };
            let shared = {
                let state = this.clone();
                let session = this.clone();
                let plugins = this.clone();
                SharedPlayController::new(
                    korrid.clone(),
                    notify.clone(),
                    SharedPlayContext {
                        state: Box::new(move || {
                            state
                                .upgrade()
                                .map_or(LaunchablesState::Loading, |root| root.launchables.state())
                        }),
                        refresh_session: Box::new(move || {
                            let root = session.upgrade();
                            Box::pin(async move {
                                if let Some(root) = root {
                                    root.launchables.refresh().await;
                                }
                            })
                        }),
                        // Installing a plugin for netplay changes what can be played.
                        plugins_changed: Box::new(move || {
                            if let Some(root) = plugins.upgrade() {
                                root.launchables.reload();
                            }
                        }),
                    },
                )
            };
            // Launch settings: Device settings' group and one game's editor.
            // Back is not taken here: the surface closes its own questions,
            // then the editor.
            let launch = LaunchSettings::new(korrid.clone(), notify.clone());
            Self {
                shared,
                launch,
                runner_was_closed: std::sync::Mutex::new(true),
                plugins,
                plugin_facts,
                store,
                last_library: std::sync::Mutex::new(None),
                saves,
                last_live_ids: std::sync::Mutex::new(None),
                runner,
                korrid: launchables_korrid,
                launch_plan: std::sync::Mutex::new(None),
                planning: std::sync::atomic::AtomicBool::new(false),
                presentation: std::sync::Mutex::new(Presentation::Catalog {
                    retained_failure: None,
                }),
                launchables,
                network,
                owner_binding,
                starter_pack,
                downloads,
                acquisition,
                controllers,
                displays: std::sync::Mutex::new(Displays::default()),
                onboarding_was_needed: std::sync::Mutex::new(None),
                clock: std::sync::Mutex::new(clock_label()),
                last_local_ids: std::sync::Mutex::new(None),
                notify,
            }
        })
    }

    /// The chooser's start: the presented entry's id and title, then the gate.
    async fn start_from_chooser(
        self: Arc<Self>,
        id: String,
        runner_id: String,
    ) -> Option<SessionStartResult> {
        // A launch from the chooser leaves any launch settings it was opened from.
        self.launch.cancel();
        let current = self.launchables.state();
        let surface_id = local_game_ids_by_surface_id(&current)
            .into_iter()
            .find(|(_, korrid_id)| *korrid_id == id)
            .map(|(surface_id, _)| surface_id)
            .unwrap_or_else(|| id.clone());
        let title = match entry_for_id(&current, &surface_id) {
            Some(Entry::Game { game, .. }) => game.title.clone(),
            _ => id.clone(),
        };
        self.gated_start(AcquisitionGameRef {
            id,
            surface_id,
            title,
            runner_id: Some(runner_id),
        })
        .await
        .ok()
        .flatten()
    }

    /// The shell shows a package with this display scope.
    pub fn show_package(self: &Arc<Self>, scope: korri_surface_treaty::SurfacePackageDisplays) {
        {
            let mut displays = self.displays.lock().expect("displays poisoned");
            displays.scope = Some(scope);
            // The shell declares once its new window has mapped: a
            // placement from before, failed or in flight, was for the old one.
            displays.placement.redeclare();
            displays.facts = None;
        }
        self.place();
        (self.notify)();
    }

    /// `model.displays`: published only for a package that draws on all.
    pub fn display_facts(&self) -> Option<Vec<korri_surface_treaty::SurfaceDisplay>> {
        let displays = self.displays.lock().expect("displays poisoned");
        (displays.scope == Some(korri_surface_treaty::SurfacePackageDisplays::All))
            .then(|| displays.facts.clone())
            .flatten()
    }

    /// Place the window for the current package and presentation. It counts
    /// as placed only once the effect succeeded; a failure is tried again on
    /// the next presentation change or window event, after its backoff.
    pub(super) fn place(self: &Arc<Self>) {
        let overlay_launch = self.overlay().map(|overlay| overlay.launch_id().to_owned());
        let (key, attempt) = {
            let mut displays = self.displays.lock().expect("displays poisoned");
            let Some(scope) = displays.scope else { return };
            let key = (scope, overlay_launch.is_some(), overlay_launch);
            match displays.placement.want(&key, std::time::Instant::now()) {
                super::placement::Next::Nothing => return,
                super::placement::Next::RetryAt(at) => {
                    let weak = Arc::downgrade(self);
                    tokio::spawn(async move {
                        tokio::time::sleep_until(at.into()).await;
                        if let Some(root) = weak.upgrade() {
                            root.place();
                        }
                    });
                    return;
                }
                super::placement::Next::Attempt(attempt) => {
                    displays.facts = None;
                    (key, attempt)
                }
            }
        };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = this
                .korrid
                .surface_displays(host_scope(key.0), key.1, key.2.clone())
                .await;
            let facts = match outcome {
                crate::host::display_placement::SurfaceDisplaysOutcome::Ok(facts) => facts,
                crate::host::display_placement::SurfaceDisplaysOutcome::Err(_) => {
                    this.displays
                        .lock()
                        .expect("displays poisoned")
                        .placement
                        .failed(attempt, std::time::Instant::now());
                    return;
                }
            };
            {
                let mut displays = this.displays.lock().expect("displays poisoned");
                if !displays.placement.succeeded(attempt) {
                    return;
                }
                displays.facts = Some(
                    facts
                        .into_iter()
                        .map(|display| korri_surface_treaty::SurfaceDisplay {
                            id: display.id,
                            main: display.main,
                            x: f64::from(display.x),
                            y: f64::from(display.y),
                            width: f64::from(display.width),
                            height: f64::from(display.height),
                            touch: display.touch,
                        })
                        .collect(),
                );
                drop(displays);
                (this.notify)();
            }
        });
    }

    /// The catalog stays mounted while a game owns the displays. Restore its
    /// placement only after authoritative absence, never on a failed read.
    fn restore_catalog_displays(self: &Arc<Self>, state: &LaunchablesState) {
        let game_using = match state {
            LaunchablesState::Loading => false,
            LaunchablesState::Loaded(loaded) => {
                matches!(
                    loaded.phase,
                    Phase::Preparing { .. }
                        | Phase::Starting { .. }
                        | Phase::Launching { .. }
                        | Phase::Stopping { .. }
                ) || loaded.content.entries.iter().any(Entry::is_now_playing)
            }
        };
        let scope = {
            let mut displays = self.displays.lock().expect("displays poisoned");
            if game_using {
                displays.restore = true;
                return;
            }
            let healthy = matches!(state, LaunchablesState::Loaded(Loaded { phase: Phase::Ready, content }) if content.unavailable_session_status.is_none());
            if !(displays.restore && healthy) {
                return;
            }
            displays.restore = false;
            displays.scope.unwrap_or_default()
        };
        let korrid = self.korrid.clone();
        tokio::spawn(async move {
            korrid
                .surface_displays(host_scope(scope), false, None)
                .await;
        });
    }

    /// The overlay of the exact launch, while it shows.
    pub fn overlay(&self) -> Option<Arc<OverlayController>> {
        match &*self.presentation.lock().expect("presentation poisoned") {
            Presentation::Overlay(controller) => Some(Arc::clone(controller)),
            Presentation::Catalog { .. } => None,
        }
    }

    fn show_catalog(&self, retained_failure: Option<String>) {
        let previous = std::mem::replace(
            &mut *self.presentation.lock().expect("presentation poisoned"),
            Presentation::Catalog { retained_failure },
        );
        if let Presentation::Overlay(controller) = previous {
            controller.destroy();
        }
        (self.notify)();
    }

    fn show_overlay(self: &Arc<Self>, active: crate::ActiveSession) {
        let weak = Arc::downgrade(self);
        let launch_id = active.launch_id.clone();
        let controller = OverlayController::new(
            active.launch_id,
            active.title,
            self.korrid.clone(),
            self.notify.clone(),
            Box::new(move |exit| {
                let Some(root) = weak.upgrade() else { return };
                match exit {
                    OverlayExit::SessionEnded => root.show_catalog(None),
                    OverlayExit::OpenKorri => root.show_catalog(Some(launch_id.clone())),
                }
            }),
        );
        *self.presentation.lock().expect("presentation poisoned") =
            Presentation::Overlay(Arc::clone(&controller));
        (self.notify)();
        tokio::spawn(async move { controller.refresh().await });
    }

    /// One inspection of korrid's Home intent while the catalog shows.
    async fn inspect_overlay(self: &Arc<Self>, from_intent: bool) {
        let retained = match &*self.presentation.lock().expect("presentation poisoned") {
            Presentation::Catalog { retained_failure } => retained_failure.clone(),
            Presentation::Overlay(_) => return,
        };
        let found = match retained.as_ref().filter(|_| !from_intent) {
            // While the catalog stays in front after Open Korri, only the
            // exact launch's focus-failed phase reopens it.
            Some(launch_id) => {
                focused_overlay_session(&self.korrid, false, Some(launch_id), 1).await
            }
            None => focused_overlay_session(&self.korrid, true, None, 4).await,
        };
        let found = found.filter(|active| {
            // A Home intent for the launch the person left with Open Korri
            // reopens it only when it is focus-failed.
            !from_intent
                || retained.as_ref() != Some(&active.launch_id)
                || active.phase.as_deref() == Some("focus-failed")
        });
        let Some(active) = found else { return };
        let still_catalog = matches!(
            &*self.presentation.lock().expect("presentation poisoned"),
            Presentation::Catalog { retained_failure } if *retained_failure == retained
        );
        if still_catalog {
            self.show_overlay(active);
        }
    }

    /// Start every poll the portal ran while mounted.
    pub fn start(self: &Arc<Self>, mut overlay_intent: tokio::sync::watch::Receiver<u64>) {
        self.saves.start();
        self.shared.start();
        let launch = Arc::clone(&self.launch);
        tokio::spawn(async move { launch.refresh_device().await });
        // The overlay opens as soon as korrid's Home intent lands (the
        // portal waited for its window's focus), and once at start for an
        // intent recovered before any surface asked.
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                overlay_intent.borrow_and_update();
                this.inspect_overlay(true).await;
                if overlay_intent.changed().await.is_err() {
                    return;
                }
            }
        });
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick: u64 = 0;
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                tick += 1;
                match this.overlay() {
                    // The overlay re-reads its launch every second.
                    Some(controller) if tick.is_multiple_of(4) => controller.refresh().await,
                    Some(_) => {}
                    // After Open Korri the portal watched the exact launch's
                    // focus-failed phase every 250 ms.
                    None => {
                        let retained = matches!(
                            &*this.presentation.lock().expect("presentation poisoned"),
                            Presentation::Catalog {
                                retained_failure: Some(_)
                            }
                        );
                        if retained {
                            this.inspect_overlay(false).await;
                        }
                    }
                }
            }
        });
        self.network.start();
        self.owner_binding.start();
        self.launchables.start();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let label = clock_label();
                let changed = {
                    let mut clock = this.clock.lock().expect("clock poisoned");
                    let changed = *clock != label;
                    *clock = label;
                    changed
                };
                if changed {
                    (this.notify)();
                }
            }
        });
    }

    /// Reactions the portal ran as effects of a new launchables state.
    pub fn react(self: &Arc<Self>) {
        let state = self.launchables.state();
        // The editor shows again once the runner chooser it opened closes,
        // read afresh: a runner chosen there brings its layers.
        let closed = self.runner.is_closed();
        let was_closed = std::mem::replace(
            &mut *self.runner_was_closed.lock().expect("runner poisoned"),
            closed,
        );
        if !was_closed && closed && self.launch.is_open() {
            self.launch.act(launch_id::RELOAD);
        }
        self.place();
        // The library changed: a store entry may now be owned, or no longer be.
        let library = serde_json::to_string(
            &state
                .entries()
                .iter()
                .map(|entry| {
                    (
                        entry.key(),
                        entry
                            .alternatives()
                            .iter()
                            .map(|copy| copy.id().to_owned())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .expect("library key serializes");
        let library_changed = {
            let mut last = self.last_library.lock().expect("library poisoned");
            let changed = last.as_ref() != Some(&library);
            *last = Some(library);
            changed
        };
        if library_changed {
            self.store.library_changed();
        }
        self.restore_catalog_displays(&state);
        let onboarding = self.network.snapshot().onboarding_needed;
        self.launchables.set_onboarding_needed(onboarding);
        // Setup run again: `heard` counts input since setup began, so pads
        // heard during the first run must not pass the second one unproven.
        let previous = std::mem::replace(
            &mut *self
                .onboarding_was_needed
                .lock()
                .expect("onboarding poisoned"),
            onboarding,
        );
        if previous == Some(false) && onboarding == Some(true) {
            self.controllers.forget();
        }
        self.starter_pack
            .set_needed(onboarding == Some(true) && self.owner_binding.snapshot().is_unsupported());
        // Which local games still need a download is re-read when the set
        // of local games changes.
        let ids = local_game_ids_of(&state);
        let changed = {
            let mut last = self.last_local_ids.lock().expect("ids poisoned");
            let changed = last.as_ref() != Some(&ids);
            *last = Some(ids.clone());
            changed
        };
        // Marks change when a session ends, and the set of local games
        // decides which marks there are to read.
        let live = live_game_ids_of(&state);
        let live_changed = {
            let mut last = self.last_live_ids.lock().expect("ids poisoned");
            let changed = last.as_ref() != Some(&live);
            *last = Some(live);
            changed
        };
        if (changed || live_changed) && !ids.is_empty() {
            let saves = Arc::clone(&self.saves);
            tokio::spawn(async move { saves.refresh().await });
        }
        if changed && !ids.is_empty() {
            let downloads = Arc::clone(&self.downloads);
            tokio::spawn(async move { downloads.refresh(Some(ids)).await });
        }
    }

    /// The model without the catalog window and focused game, and the whole
    /// game list in Korri's order.
    pub fn facts(&self) -> Facts {
        let state = self.launchables.state();
        let network = self.network.snapshot();
        let owner = self.owner_binding.snapshot();
        let onboarding_needed = network.onboarding_needed;
        let mut facts = self.launchables.facts();
        facts.plugin_groups = Some(plugin_setting_groups(
            &self.plugin_facts.lock().expect("plugin facts poisoned"),
            facts
                .settings
                .as_ref()
                .map(|settings| settings.plugins.as_slice()),
        ));
        facts.launch_settings = self.launch.device_groups();
        let settings = settings_from(&facts);
        let mut status = match &network.problem {
            Some(problem) => SurfaceStatus::Problem {
                kicker: "Setup needs attention".into(),
                reason: problem.clone(),
                can_retry: true,
                game_id: None,
                game_title: None,
            },
            None => status_from(&state),
        };
        if network.problem.is_none() {
            if let Some(setup) = owner_binding_status(&owner, onboarding_needed == Some(true)) {
                status = setup;
            }
        }
        let shared_status = self.shared.status();
        if let Some(problem) = self
            .saves
            .status()
            .or_else(|| self.acquisition.status())
            .or_else(|| shared_status.clone())
            .or_else(|| self.downloads.status())
        {
            status = problem;
        }
        let status = self.with_launch_steps(status);
        let onboarding = (onboarding_needed == Some(true) && owner.is_unsupported()).then(|| {
            SurfaceOnboarding {
                controllers: self.controllers.snapshot(),
                starter_pack: self.starter_pack.snapshot(),
            }
        });
        let games = catalog_games(&state).map(|games| self.decorate(games, &state));
        let base = SurfaceModel {
            displays: None,
            presentation: SurfacePresentation::catalog(),
            catalog: korri_surface_treaty::SurfaceCatalog::Loading,
            status,
            save_history: self.saves.history(),
            runner_choice: Some(self.runner.snapshot()),
            launch_settings: Some(self.launch.editor()),
            clock_label: Some(self.clock.lock().expect("clock poisoned").clone()),
            actions: Vec::new(),
            settings,
            settings_status: self
                .launch
                .status()
                .unwrap_or_else(|| self.launchables.settings_status()),
            identity_management: self.launchables.identity_management(),
            build_label: None,
            network: network.network,
            onboarding,
            store: Some(store_for_surface(self.store.snapshot(), &state)),
            invites: Some(
                if shared_status.is_none()
                    && state.is_ready()
                    && !state.entries().iter().any(Entry::is_now_playing)
                {
                    surface_invites_from(&self.shared.invites(), &state)
                } else {
                    Vec::new()
                },
            ),
            focused_game: None,
        };
        Facts { base, games, state }
    }

    /// Download facts on each local game, as the portal decorated them.
    fn decorate(&self, games: Vec<SurfaceGame>, state: &LaunchablesState) -> Vec<SurfaceGame> {
        let facts = self.downloads.facts();
        let marks = self.saves.marks();
        let profiles = self.launch.active_profiles();
        if facts.is_empty() && marks.is_empty() && profiles.is_empty() {
            return games;
        }
        let ids = local_game_ids_by_surface_id(state);
        games
            .into_iter()
            .map(|mut game| {
                if let Some(korrid_id) = ids.get(&game.id) {
                    if let Some(download) = surface_download_from(facts.get(korrid_id)) {
                        game.download = Some(download);
                    }
                    if let Some(saves) = surface_saves_from(marks.get(korrid_id)) {
                        game.saves = Some(saves);
                    }
                    if let Some(Some(profile)) = profiles.get(korrid_id) {
                        game.active_profile = Some(profile.clone());
                    }
                }
                game
            })
            .collect()
    }

    /// The download side of one presented game, when this device fetches for
    /// it. Facts korrid has not reported yet are asked for here, once.
    fn download_subject(&self, id: &str) -> Option<(DownloadGameRef, Option<GameDownloadContext>)> {
        let current = self.launchables.state();
        let korrid_id = local_game_ids_by_surface_id(&current).get(id)?.clone();
        let Some(Entry::Game { game, .. }) = entry_for_id(&current, id) else {
            return None;
        };
        let reference = DownloadGameRef {
            id: korrid_id.clone(),
            surface_id: id.into(),
            title: game.title.clone(),
        };
        let Some(facts) = self.downloads.facts().get(&korrid_id).cloned() else {
            self.downloads.learn(&korrid_id);
            return Some((reference, None));
        };
        let live = current.entries().iter().any(|candidate| {
            candidate.session().is_some_and(|session| {
                session.game_id.as_ref() == Some(&korrid_id)
                    && (session.host.is_none() || session.host == game.host)
            })
        });
        let enabled = matches!(&current, LaunchablesState::Loaded(Loaded { phase: Phase::Ready, content }) if content.unavailable_session_status.is_none())
            && !self.downloads.is_pending(&korrid_id);
        Some((
            reference,
            Some(GameDownloadContext {
                facts,
                live,
                enabled,
            }),
        ))
    }

    /// `gameActions(gameId)` of the portal, now `focusedGame.actions`.
    pub fn game_actions(&self, id: &str) -> Vec<SurfaceAction> {
        let current = self.launchables.state();
        let entry = entry_for_id(&current, id);
        let korrid_id = local_game_ids_by_surface_id(&current).get(id).cloned();
        let context = self.download_subject(id).and_then(|(_, context)| context);
        let mut actions = game_actions_for_entry(entry, context.as_ref());
        let ready_now = matches!(
            &current,
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Ready,
                ..
            })
        );
        let healthy = matches!(&current, LaunchablesState::Loaded(Loaded { phase: Phase::Ready, content }) if content.unavailable_session_status.is_none());
        // A game's page shows the profile its next launch uses: read it once.
        if let Some(korrid_id) = korrid_id.as_ref().filter(|_| ready_now) {
            self.launch.learn(korrid_id);
        }
        if local_runner_entry(entry).is_some() {
            actions.push(SurfaceAction::new(
                "runners",
                "Runners on this device",
                healthy,
            ));
        }
        if korrid_id.is_some() {
            actions.push(SurfaceAction::new(
                LAUNCH_SETTINGS_ACTION_ID,
                "Launch settings",
                ready_now,
            ));
        }
        if korrid_id
            .as_ref()
            .is_some_and(|id| self.saves.has_history(id))
        {
            actions.push(SurfaceAction::new(SAVE_HISTORY_ACTION_ID, "Saves", true));
        }
        actions
    }

    /// Location choices and game actions share the same local lifecycle.
    fn confirm_launch(self: &Arc<Self>, id: &str, picked: Entry) {
        let current = self.launchables.state();
        let LaunchablesState::Loaded(Loaded {
            phase: Phase::Ready,
            content,
        }) = &current
        else {
            return;
        };
        let chosen = launchable_on_this_device(picked);
        let Entry::Game { game, .. } = &chosen else {
            self.launchables.confirm_entry(chosen);
            return;
        };
        if !game.source.is_local {
            self.launchables.confirm_entry(chosen);
            return;
        }
        // A game already live on this device is returned to.
        if let Some(active) = content.entries.iter().find(|entry| {
            entry.session().is_some_and(|session| {
                session.game_id.as_ref() == Some(&game.id)
                    && (session.host.is_none() || session.host == game.host)
            })
        }) {
            self.launchables.confirm_entry(active.clone());
            return;
        }
        if content.unavailable_session_status.is_some() {
            return;
        }
        // Several runners: the chooser first, then the gate with its choice,
        // because korrid resolves artifacts per runner.
        if game.supports_runner_selection {
            self.runner
                .open(game.id.clone(), game.title.clone(), Intent::Launch);
            return;
        }
        // The gate starts at once; nothing here waits for its outcome.
        drop(self.gated_start(AcquisitionGameRef {
            id: game.id.clone(),
            surface_id: id.into(),
            title: game.title.clone(),
            runner_id: None,
        }));
    }

    /// The startup behind the gate: once every artifact the game's runner
    /// needs is Ready, the reserve/start lifecycle. korrid refusing a start
    /// with ArtifactsNotReady means the world moved: ask again.
    fn gated_start(
        self: &Arc<Self>,
        game: AcquisitionGameRef,
    ) -> tokio::sync::oneshot::Receiver<Option<SessionStartResult>> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.gate(game, Arc::new(std::sync::Mutex::new(Some(sender))));
        receiver
    }

    /// The download gate, then the start. Resolves None when the gate
    /// closes without launching.
    /// The save check and its questions first, then the download gate.
    fn gate(self: &Arc<Self>, game: AcquisitionGameRef, done: StartDone) {
        crate::launch_timeline::begin(&game.id);
        self.planning
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let root = Arc::clone(self);
        tokio::spawn(async move {
            let plan = root.plan_launch(&game).await;
            *root.launch_plan.lock().expect("launch plan poisoned") = Some(plan);
            root.planning
                .store(false, std::sync::atomic::Ordering::SeqCst);
            (root.notify)();
            let weak = Arc::downgrade(&root);
            let gate_game = game.clone();
            let gate_done = Arc::clone(&done);
            let proceed: super::acquisition_gate::Step = Arc::new(move || {
                if let Some(root) = weak.upgrade() {
                    root.reach_step(LaunchStep::Download);
                    root.download_gate(gate_game.clone(), Arc::clone(&gate_done));
                }
            });
            let abandon_root = Arc::downgrade(&root);
            let abandon: super::acquisition_gate::Step = Arc::new(move || {
                if let Some(root) = abandon_root.upgrade() {
                    root.end_plan();
                }
                finish(&done, None)
            });
            root.saves.launch(save_ref(&game), proceed, abandon);
        });
    }

    /// The launch's steps, fixed before it begins: the save check when a
    /// peer is in reach, the download when the game needs one.
    async fn plan_launch(&self, game: &AcquisitionGameRef) -> LaunchPlan {
        let peers = self.korrid.peer_list();
        let known = match &game.runner_id {
            None => self
                .downloads
                .facts()
                .get(&game.id)
                .map(|facts| facts.needs),
            // The download facts are for the default runner.
            Some(_) => None,
        };
        let needs = async {
            match known {
                Some(needs) => needs,
                None => match self
                    .korrid
                    .acquisition_status(Some(game.id.clone()), game.runner_id.clone())
                    .await
                {
                    crate::acquisition::AcquisitionOutcome::Ok(snapshot) => {
                        snapshot.artifacts.iter().any(|artifact| {
                            !matches!(
                                artifact.state,
                                crate::acquisition::ArtifactState::Ready { .. }
                            )
                        })
                    }
                    // The gate reads it again and says what went wrong.
                    crate::acquisition::AcquisitionOutcome::Err(_) => false,
                },
            }
        };
        let (peers, needs_download) = tokio::join!(peers, needs);
        // A peer whose last contact failed is not in reach; the save check
        // spends at most its reach budget on it.
        let peer_in_reach = matches!(&peers, crate::PeerListOutcome::Ok(list)
            if list.peers.iter().any(|peer| !matches!(peer.state, crate::PeerListState::Failed)));
        LaunchPlan::new(&game.id, peer_in_reach, needs_download)
    }

    #[cfg(test)]
    pub(crate) fn launch_plan_for_tests(&self) -> Option<LaunchPlan> {
        self.launch_plan
            .lock()
            .expect("launch plan poisoned")
            .clone()
    }

    fn reach_step(&self, step: LaunchStep) {
        if let Some(plan) = self
            .launch_plan
            .lock()
            .expect("launch plan poisoned")
            .as_mut()
        {
            plan.reach(step);
        }
        (self.notify)();
    }

    fn end_plan(&self) {
        *self.launch_plan.lock().expect("launch plan poisoned") = None;
        (self.notify)();
    }

    /// A Busy of the launch under way names its steps.
    fn with_launch_steps(&self, status: SurfaceStatus) -> SurfaceStatus {
        let mut plan = self.launch_plan.lock().expect("launch plan poisoned");
        let SurfaceStatus::Busy {
            kicker,
            detail,
            progress,
            actions,
            decline_action_id,
            game_id,
            ..
        } = status
        else {
            // The game has taken over, or the start failed: the launch is over.
            if plan
                .as_ref()
                .is_some_and(|plan| plan.current() == LaunchStep::Start)
            {
                *plan = None;
            }
            return status;
        };
        SurfaceStatus::Busy {
            kicker,
            detail,
            progress,
            steps: plan.as_ref().map(LaunchPlan::surface_steps),
            actions,
            decline_action_id,
            game_id,
        }
    }

    /// The download gate, then the start. korrid refusing the start over
    /// artifacts goes back through the gate; over a split it asks for the
    /// choice; with no owner it says so.
    fn download_gate(self: &Arc<Self>, game: AcquisitionGameRef, done: StartDone) {
        let weak = Arc::downgrade(self);
        let retry_game = game.clone();
        let proceed_done = Arc::clone(&done);
        let gate_started = std::time::Instant::now();
        let proceed = Arc::new(move || {
            let Some(root) = weak.upgrade() else { return };
            let game = retry_game.clone();
            crate::launch_timeline::record(
                &game.id,
                crate::launch_timeline::LaunchStage::Downloads,
                gate_started.elapsed(),
            );
            let done = Arc::clone(&proceed_done);
            tokio::spawn(async move {
                root.reach_step(LaunchStep::Prepare);
                let outcome = root
                    .launchables
                    .start_catalog_launch(&game.id, game.runner_id.clone())
                    .await;
                match &outcome {
                    Some(crate::game_routes::SelectedGameLaunchOutcome::Ok(_)) => {
                        root.reach_step(LaunchStep::Start)
                    }
                    // A refusal that goes back through a gate keeps its plan.
                    Some(crate::game_routes::SelectedGameLaunchOutcome::Err(failure))
                        if matches!(
                            failure.code.as_str(),
                            "ArtifactsNotReady" | "SavesSplit" | "DeviceNeedsOwner"
                        ) => {}
                    _ => root.end_plan(),
                }
                if let Some(crate::game_routes::SelectedGameLaunchOutcome::Err(failure)) = &outcome
                {
                    if failure.code == "ArtifactsNotReady" {
                        root.launchables.dismiss_notice();
                        root.download_gate(game, done);
                        return;
                    }
                    if failure.code == "SavesSplit" {
                        root.launchables.dismiss_notice();
                        let again = Arc::downgrade(&root);
                        let again_game = game.clone();
                        let again_done = Arc::clone(&done);
                        let abandon_done = Arc::clone(&done);
                        root.saves.resolve_split(
                            save_ref(&game),
                            Arc::new(move || {
                                if let Some(root) = again.upgrade() {
                                    root.download_gate(again_game.clone(), Arc::clone(&again_done));
                                }
                            }),
                            Arc::new(move || finish(&abandon_done, None)),
                        );
                        return;
                    }
                    if failure.code == "DeviceNeedsOwner" {
                        root.launchables.dismiss_notice();
                        let abandon_done = Arc::clone(&done);
                        root.saves.refuse_start(
                            save_ref(&game),
                            failure,
                            Arc::new(move || finish(&abandon_done, None)),
                        );
                        return;
                    }
                }
                finish(&done, outcome);
            });
        });
        let abandon_root = Arc::downgrade(self);
        let abandon: super::acquisition_gate::Step = Arc::new(move || {
            if let Some(root) = abandon_root.upgrade() {
                root.end_plan();
            }
            finish(&done, None)
        });
        self.acquisition.launch(game, proceed, abandon);
    }

    /// Perform one request with the portal host's rules. Requests about the
    /// catalog window, focus and preferences belong to the hub.
    pub fn perform(self: &Arc<Self>, request: SurfaceRequest) {
        if let Some(controller) = self.overlay() {
            // The overlay host answered only its own requests.
            tokio::spawn(async move {
                match request {
                    SurfaceRequest::InvokeGameplayControl { control_id, value } => {
                        controller.invoke(control_id, value).await
                    }
                    SurfaceRequest::DismissGameplayOverlay | SurfaceRequest::Dismiss => {
                        controller.resume_game().await
                    }
                    SurfaceRequest::Retry => controller.retry().await,
                    SurfaceRequest::Reload => controller.refresh().await,
                    _ => {}
                }
            });
            return;
        }
        let launchables = &self.launchables;
        match request {
            SurfaceRequest::LaunchGame {
                game_id,
                launch_location_id,
            } => {
                let current = launchables.state();
                if !current.is_ready()
                    || self.planning.load(std::sync::atomic::Ordering::SeqCst)
                    || self.acquisition.status().is_some()
                    || self.saves.status().is_some()
                    || !self.runner.is_closed()
                    || self.launch.is_open()
                {
                    return;
                }
                let Some(entry) = entry_for_id(&current, &game_id).cloned() else {
                    return;
                };
                if launch_locations_for_entry(&entry).len() > 1 {
                    let Some(location) = launch_location_id else {
                        return;
                    };
                    if let Some(chosen) = entry_for_launch_location(&entry, &location) {
                        self.confirm_launch(&game_id, chosen);
                    }
                    return;
                }
                self.confirm_launch(&game_id, entry);
            }
            SurfaceRequest::RunAction { action_id } => {
                if self.owner_binding.act(&action_id)
                    || self.shared.act(&action_id)
                    || self.launch.act(&action_id)
                    || self.saves.act(&action_id)
                    || self.starter_pack.act(&action_id)
                    || self.store.act(&action_id)
                    || self.acquisition.act(&action_id)
                {
                    return;
                }
                if let Some(launch_id) = action_id.strip_prefix("cancel-pending:") {
                    launchables.cancel_pending_launch(launch_id);
                } else if action_id == "cancel-launch" {
                    // Before the startup, the same intent leaves the gate.
                    if self.acquisition.back() {
                        return;
                    }
                    self.runner.cancel();
                    launchables.cancel_launch();
                } else if let Some(runner_action) = action_id.strip_prefix("runner:") {
                    self.runner.act(runner_action.to_owned());
                } else if self.runner.is_closed() {
                    launchables.run_device_action(&action_id);
                }
            }
            SurfaceRequest::ChangeSetting { setting_id, value } => {
                if !self.launch.change(&setting_id, &value) {
                    launchables.change_setting(&setting_id, &value)
                }
            }
            SurfaceRequest::DismissSettingsProblem => {
                if !self.launch.dismiss_problem() {
                    launchables.dismiss_settings_problem()
                }
            }
            SurfaceRequest::ExportIdentityBackup {
                password,
                retired_public_key,
            } => launchables.export_identity_backup(password, retired_public_key),
            SurfaceRequest::SwitchIdentityFromBackup {
                encrypted_secret,
                password,
                disposition,
                trust_loss_confirmed,
            } => launchables.switch_identity_from_backup(
                encrypted_secret,
                password,
                disposition,
                trust_loss_confirmed,
            ),
            SurfaceRequest::SwitchIdentityToNip46 {
                bunker_uri,
                disposition,
                trust_loss_confirmed,
            } => {
                launchables.switch_identity_to_nip46(bunker_uri, disposition, trust_loss_confirmed)
            }
            SurfaceRequest::DeleteRetiredIdentity {
                public_key,
                backup_confirmed,
            } => launchables.delete_retired_identity(public_key, backup_confirmed),
            SurfaceRequest::DismissIdentityStatus => launchables.dismiss_identity_status(),
            SurfaceRequest::RunGameAction { game_id, action_id } => {
                let current = launchables.state();
                if !current.is_ready()
                    || self.planning.load(std::sync::atomic::Ordering::SeqCst)
                    || self.acquisition.status().is_some()
                    || self.saves.status().is_some()
                    || !self.runner.is_closed()
                    || self.launch.is_open()
                {
                    return;
                }
                let Some(entry) = entry_for_id(&current, &game_id).cloned() else {
                    return;
                };
                // Only a download action on offer now, enabled, is authority.
                if DOWNLOAD_ACTION_IDS.contains(&action_id.as_str()) {
                    let Some((reference, context)) = self.download_subject(&game_id) else {
                        return;
                    };
                    let offered = game_actions_for_entry(Some(&entry), context.as_ref())
                        .into_iter()
                        .any(|action| action.id == action_id && action.enabled);
                    if !offered {
                        return;
                    }
                    match action_id.as_str() {
                        "download" => self.downloads.download(reference),
                        "cancel-download" => self.downloads.cancel(reference),
                        _ => self.downloads.remove(reference),
                    }
                    return;
                }
                if action_id == SAVE_HISTORY_ACTION_ID || action_id == SAVE_CHOOSE_ACTION_ID {
                    let Some(korrid_id) = local_game_ids_by_surface_id(&current)
                        .get(&game_id)
                        .cloned()
                    else {
                        return;
                    };
                    let Entry::Game { game, .. } = &entry else {
                        return;
                    };
                    let reference = SaveGameRef {
                        id: korrid_id.clone(),
                        surface_id: game_id.clone(),
                        title: game.title.clone(),
                        runner_id: None,
                    };
                    let split = matches!(
                        surface_saves_from(self.saves.marks().get(&korrid_id)),
                        Some(korri_surface_treaty::SurfaceSaves::Split { .. })
                    );
                    if action_id == SAVE_HISTORY_ACTION_ID && self.saves.has_history(&korrid_id) {
                        self.saves.open_history(reference);
                    } else if action_id == SAVE_CHOOSE_ACTION_ID && split {
                        self.saves.choose(reference);
                    }
                    return;
                }
                if action_id == "runners" {
                    let healthy = matches!(&current, LaunchablesState::Loaded(Loaded { content, .. }) if content.unavailable_session_status.is_none());
                    if let Some(local) = local_runner_entry(Some(&entry)).filter(|_| healthy) {
                        self.runner.open(local.id, local.title, Intent::Inspect);
                    }
                    return;
                }
                if action_id == LAUNCH_SETTINGS_ACTION_ID {
                    let Some(korrid_id) = local_game_ids_by_surface_id(&current)
                        .get(&game_id)
                        .cloned()
                    else {
                        return;
                    };
                    let Entry::Game { game, .. } = &entry else {
                        return;
                    };
                    // The runner chooser, when this page offers it, fills in
                    // the layers that wait on a runner choice.
                    let healthy = matches!(&current, LaunchablesState::Loaded(Loaded { content, .. }) if content.unavailable_session_status.is_none());
                    let choose =
                        local_runner_entry(Some(&entry))
                            .filter(|_| healthy)
                            .map(|local| {
                                let runner = Arc::clone(&self.runner);
                                Arc::new(move || {
                                    runner.open(
                                        local.id.clone(),
                                        local.title.clone(),
                                        Intent::Inspect,
                                    )
                                })
                                    as super::launch_settings::OpenRunners
                            });
                    self.launch.open(korrid_id, game.title.clone(), choose);
                    return;
                }
                if action_id == "end" {
                    launchables.end_session(entry);
                } else {
                    self.confirm_launch(&game_id, entry);
                }
            }
            // The browsing root never publishes gameplay-overlay controls.
            SurfaceRequest::InvokeGameplayControl { .. }
            | SurfaceRequest::DismissGameplayOverlay => {}
            // Korri has nothing new to try after a failed launch, so retrying
            // means re-reading the world.
            SurfaceRequest::Retry => {
                if !self.saves.retry() && !self.acquisition.retry() {
                    launchables.reload()
                }
            }
            SurfaceRequest::Dismiss => {
                if !self.shared.dismiss()
                    && !self.saves.dismiss()
                    && !self.acquisition.dismiss()
                    && !self.downloads.dismiss()
                {
                    launchables.dismiss_notice();
                }
            }
            SurfaceRequest::Reload => {
                launchables.reload();
                let launch = Arc::clone(&self.launch);
                tokio::spawn(async move { launch.refresh_device().await });
                let network = Arc::clone(&self.network);
                network.dismiss_problem();
                tokio::spawn(async move { network.refresh().await });
                let downloads = Arc::clone(&self.downloads);
                tokio::spawn(async move { downloads.refresh(None).await });
                let saves = Arc::clone(&self.saves);
                tokio::spawn(async move { saves.refresh().await });
            }
            SurfaceRequest::ScanWifi => self.network.scan_wifi(),
            SurfaceRequest::ConnectWifi {
                network_id,
                password,
            } => self.network.connect_wifi(network_id, password),
            SurfaceRequest::ConnectHiddenWifi { ssid, password } => {
                self.network.connect_hidden_wifi(ssid, password)
            }
            SurfaceRequest::FinishOnboarding => self.network.finish_onboarding(),
            SurfaceRequest::OpenStore => self.store.open_store(),
            SurfaceRequest::SearchStore { provider_id, query } => {
                self.store.search(provider_id, query)
            }
            SurfaceRequest::OpenStoreEntry { entry_id } => self.store.open_entry(entry_id),
            SurfaceRequest::FocusGame { .. }
            | SurfaceRequest::RememberPreference { .. }
            | SurfaceRequest::MoveCatalog { .. } => {}
        }
    }
}
