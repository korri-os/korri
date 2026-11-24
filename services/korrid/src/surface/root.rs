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
    launch_settings::{Closing, LaunchSettings, LAUNCH_SETTINGS_ACTION_ID},
    launchables::driver::SessionStartResult,
    launchables::{
        driver::{Launchables, LaunchablesHooks},
        state::{Entry, LaunchablesState, Loaded, Phase},
    },
    model::{
        catalog_games, entry_for_id, entry_for_launch_location, game_actions_for_entry,
        launch_locations_for_entry, ordered_copies_for_entry, status_from, surface_download_from,
        GameDownloadContext, DOWNLOAD_ACTION_IDS,
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

/// A running game's card wraps its catalog entry; read the catalog facts
/// through it so the one card keeps its download, save and profile badges.
fn catalog_view(entry: &Entry) -> &Entry {
    entry.catalog_entry().unwrap_or(entry)
}

/// korrid ids of every game this device can launch itself, sorted.
fn local_game_ids_of(state: &LaunchablesState) -> Vec<String> {
    let mut ids: Vec<String> = state
        .entries()
        .iter()
        .filter_map(|entry| match catalog_view(entry) {
            Entry::Game { game, .. } if game.source.is_local => Some(game.id.clone()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids
}

/// Surface id -> korrid id for the badges: the local games, plus a running
/// local game whose card wraps its catalog entry. Actions keep using
/// `local_game_ids_by_surface_id`; a running card offers only its session's.
fn decorated_game_ids_by_surface_id(state: &LaunchablesState) -> BTreeMap<String, String> {
    state
        .entries()
        .iter()
        .filter_map(|entry| match catalog_view(entry) {
            Entry::Game { game, .. } if game.source.is_local => {
                Some((entry.key(), game.id.clone()))
            }
            _ => None,
        })
        .collect()
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

/// The copy of a game that another device of the federation runs, when the
/// entry presents that copy. Its launch settings and runners are that
/// device's (ADR 0066); only a native peer answers for them.
fn streamed_game(entry: Option<&Entry>) -> Option<&crate::Game> {
    match entry {
        Some(Entry::Game { game, .. })
            if !game.source.is_local && game.source.device_public_key.is_some() =>
        {
            Some(game)
        }
        _ => None,
    }
}

/// The streamed copy a holder's label and korrid id name, as an entry.
fn streamed_entry(state: &LaunchablesState, id: &str, holder: &str) -> Option<Entry> {
    let is_copy =
        |game: &crate::Game| !game.source.is_local && game.id == id && game.source.label == holder;
    state.entries().iter().find_map(|entry| {
        if entry.is_now_playing() {
            return None;
        }
        match entry {
            Entry::Game { game, .. } if is_copy(game) => Some(entry.clone()),
            _ => entry
                .alternatives()
                .iter()
                .find(|copy| copy.remote().is_some_and(is_copy))
                .map(Entry::for_copy),
        }
    })
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

/// How often play here reads the library for a release that just landed,
/// and how long it waits between reads: discovery records the release and
/// the catalog publishes it a moment after the install ends.
const PLAY_INSTALLED_READS: u32 = 20;
const PLAY_INSTALLED_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_millis(500)
};

/// The presented entry and the catalog copy on this device of a release,
/// by its release key: the copy a launch here takes.
fn local_copy_of_release(state: &LaunchablesState, key: &str) -> Option<(String, crate::Game)> {
    let is_release = |game: &crate::Game| {
        game.source.is_local
            && matches!(&game.identity, Some(crate::GameIdentity::Hash(hash)) if hash == key)
    };
    state.entries().iter().find_map(|entry| {
        if entry.is_now_playing() {
            return None;
        }
        ordered_copies_for_entry(entry)
            .into_iter()
            .find_map(|copy| match copy {
                super::launchables::fold::GameCopy::Remote(game) if is_release(&game) => Some(game),
                _ => None,
            })
            .map(|game| (entry.key(), game))
    })
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

/// `SurfaceModel.publishedAt` for a model built at `now`: UTC epoch
/// milliseconds. A clock that reads before the Unix epoch gives no time.
pub fn published_at(now: std::time::SystemTime) -> Option<f64> {
    now.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|since| since.as_millis() as f64)
}

/// The present as a surface is told it. Both facts are read at one moment
/// and change together, so the time adds no publish of its own.
#[derive(Clone)]
struct ClockSample {
    label: String,
    published_at: Option<f64>,
}

impl ClockSample {
    fn now() -> Self {
        Self {
            label: clock_label(),
            published_at: published_at(std::time::SystemTime::now()),
        }
    }
}

/// Port of `surface-displays.ts` and SurfaceRoot's catalog restore: the
/// window's placement for the package the shell shows.
#[derive(Default)]
struct Displays {
    /// The panes of the package declared last (ADR 0086). The outer None:
    /// no package declared yet; the inner None: a package with one pane.
    scope: Option<Option<korri_surface_treaty::SurfacePackagePanes>>,
    /// Which placement succeeded, which is in flight, which waits.
    placement: super::placement::Placement,
    /// The windows the last placement gave the panes; published as
    /// `SurfaceModel.windows`. An effect-free check that finds other windows
    /// places the surface again (ADR 0086).
    windows: Option<Vec<korri_surface_treaty::SurfaceWindow>>,
    /// The watch that checks the windows while a package with panes shows.
    watching: bool,
    /// A game held the displays; restore the catalog's after it is gone.
    restore: bool,
}

/// How often korrid checks, without effects, whether a package's panes would
/// get other windows: a display under the main one appeared or went (ADR 0086).
const PANE_WINDOWS_CHECK: std::time::Duration = std::time::Duration::from_secs(1);

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
    pub steam: Arc<super::steam_account::SteamAccount>,
    /// ADR 0083: a Steam engine launch that waits for the person.
    pub steam_cloud: Arc<super::steam_cloud::SteamCloudQuestion>,
    /// ADR 0089: every other plugin's account sign-in.
    pub accounts: Arc<super::account_sign_ins::PluginAccounts>,
    /// The last launch through the gate: a Steam Cloud conflict refused on
    /// its sync goes back through the gate on that same attempt.
    last_gated: std::sync::Mutex<Option<AcquisitionGameRef>>,
    pub downloads: Arc<GameDownloads>,
    pub acquisition: Arc<AcquisitionGate>,
    pub runner: Arc<RunnerChooser>,
    pub saves: Arc<SaveSync>,
    pub launch: Arc<LaunchSettings>,
    pub store: Arc<StoreAcquisition>,
    pub plugins: Arc<PluginsController>,
    pub shared: Arc<SharedPlayController>,
    /// ADR 0092 (#553): play here and the list of installs.
    pub installs: Arc<super::installs::Installs>,
    plugin_facts: Arc<std::sync::Mutex<PluginFacts>>,
    last_library: std::sync::Mutex<Option<String>>,
    last_live_ids: std::sync::Mutex<Option<Vec<String>>>,
    runner_was_closed: std::sync::Mutex<bool>,
    controllers: Arc<super::controllers::Controllers>,
    displays: std::sync::Mutex<Displays>,
    onboarding_was_needed: std::sync::Mutex<Option<bool>>,
    clock: std::sync::Mutex<ClockSample>,
    last_local_ids: std::sync::Mutex<Option<Vec<String>>>,
    /// This device's battery, where korrid reads one (ADR 0094).
    battery: Option<tokio::sync::watch::Receiver<Option<korri_surface_treaty::SurfaceBattery>>>,
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
                        start_launch: Box::new(move |id, host, runner_id| {
                            let root = start.upgrade();
                            Box::pin(async move {
                                let root = root?;
                                root.start_from_chooser(id, host, runner_id).await
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
                        launch_overrides: {
                            let root = this.clone();
                            Box::new(move |game, runner| {
                                root.upgrade().map_or((0, None), |root| {
                                    root.launch.submit_one_launch(game, runner)
                                })
                            })
                        },
                        launch_overrides_complete: {
                            let root = this.clone();
                            Box::new(move |ticket, success| {
                                if let Some(root) = root.upgrade() {
                                    root.launch.complete_one_launch(ticket, success);
                                }
                            })
                        },
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
                                // ADR 0083: the Steam engine runner may have come or gone.
                                root.steam.refresh();
                                // ADR 0089: a plugin's account may have come or gone.
                                tokio::spawn(Arc::clone(&root.accounts).refresh());
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
            let installs = {
                let (state, play, stream, changed, busy) = (
                    this.clone(),
                    this.clone(),
                    this.clone(),
                    this.clone(),
                    this.clone(),
                );
                let facts = Arc::clone(&plugin_facts);
                super::installs::Installs::new(
                    korrid.clone(),
                    notify.clone(),
                    super::installs::InstallsHost {
                        state: Box::new(move || {
                            state
                                .upgrade()
                                .map_or(LaunchablesState::Loading, |root| root.launchables.state())
                        }),
                        plugin_facts: Box::new(move || {
                            facts.lock().expect("plugin facts poisoned").clone()
                        }),
                        play: Box::new(move |installed| {
                            if let Some(root) = play.upgrade() {
                                tokio::spawn(async move { root.play_installed(installed).await });
                            }
                        }),
                        stream: Box::new(move |game_id, launch_location_id| {
                            if let Some(root) = stream.upgrade() {
                                root.perform(SurfaceRequest::LaunchGame {
                                    game_id,
                                    launch_location_id,
                                });
                            }
                        }),
                        library_changed: Box::new(move || {
                            if let Some(root) = changed.upgrade() {
                                tokio::spawn(async move { root.launchables.refresh().await });
                            }
                        }),
                        busy: Box::new(move || busy.upgrade().is_none_or(|root| root.starting())),
                    },
                )
            };
            // Launch settings: Device settings' group and one game's editor.
            // Back is not taken here: the surface closes its own questions,
            // then the editor.
            let launch = LaunchSettings::new(korrid.clone(), notify.clone());
            Self {
                shared,
                installs,
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
                steam: super::steam_account::SteamAccount::new(korrid.clone(), notify.clone()),
                steam_cloud: super::steam_cloud::SteamCloudQuestion::new(&korrid, notify.clone()),
                accounts: super::account_sign_ins::PluginAccounts::new(
                    korrid.clone(),
                    notify.clone(),
                ),
                last_gated: std::sync::Mutex::new(None),
                downloads,
                acquisition,
                controllers,
                displays: std::sync::Mutex::new(Displays::default()),
                onboarding_was_needed: std::sync::Mutex::new(None),
                clock: std::sync::Mutex::new(ClockSample::now()),
                last_local_ids: std::sync::Mutex::new(None),
                battery: korrid.battery.as_ref().map(|battery| battery()),
                notify,
            }
        })
    }

    /// This device's battery now; absent where korrid reads none.
    pub fn battery(&self) -> Option<korri_surface_treaty::SurfaceBattery> {
        self.battery
            .as_ref()
            .and_then(|battery| battery.borrow().clone())
    }

    /// The chooser's start: the presented entry's id and title, then the gate.
    async fn start_from_chooser(
        self: Arc<Self>,
        id: String,
        host: Option<String>,
        runner_id: Option<String>,
    ) -> Option<SessionStartResult> {
        // A launch from the chooser leaves any launch settings it was opened
        // from. Its changes are kept first, as when the editor closes. The
        // editor is not read again for a chooser that closed to start a game:
        // it closes, or it stays open to say what was not kept.
        *self.runner_was_closed.lock().expect("runner poisoned") = true;
        if self.launch.close().await == Closing::NotKept {
            // Nothing starts with a change that was not kept.
            return None;
        }
        let current = self.launchables.state();
        if let Some(host) = host {
            // A streamed game starts through the stream lifecycle, which
            // publishes its own progress and warnings.
            if let Some(entry) = streamed_entry(&current, &id, &host) {
                *self.launch_plan.lock().expect("launch plan poisoned") =
                    Some(LaunchPlan::new(&id, false, false));
                self.launchables.confirm_streamed(entry, runner_id);
            }
            return None;
        }
        let runner_id = runner_id?;
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

    /// The shell shows a package with these panes.
    pub fn show_package(
        self: &Arc<Self>,
        panes: Option<korri_surface_treaty::SurfacePackagePanes>,
    ) {
        let watch = panes.is_some();
        {
            let mut displays = self.displays.lock().expect("displays poisoned");
            if displays.scope.as_ref().and_then(Option::as_ref) != panes.as_ref() {
                displays.windows = None;
            }
            displays.scope = Some(panes);
            // The shell declares once its new window has mapped: a
            // placement from before, failed or in flight, was for the old one.
            displays.placement.redeclare();
        }
        self.place();
        if watch {
            self.watch_pane_windows();
        }
        (self.notify)();
    }

    /// `model.windows`: published only for a package that declared panes.
    pub fn windows(&self) -> Option<Vec<korri_surface_treaty::SurfaceWindow>> {
        self.displays
            .lock()
            .expect("displays poisoned")
            .windows
            .clone()
    }

    /// While a package with panes shows, check each `PANE_WINDOWS_CHECK`
    /// whether its panes would get other windows, and place it again when
    /// they would. The check has no effect; one watch runs at a time.
    fn watch_pane_windows(self: &Arc<Self>) {
        {
            let mut displays = self.displays.lock().expect("displays poisoned");
            if displays.watching {
                return;
            }
            displays.watching = true;
        }
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(PANE_WINDOWS_CHECK).await;
                let Some(root) = weak.upgrade() else { return };
                let panes = {
                    let mut displays = root.displays.lock().expect("displays poisoned");
                    match displays.scope.clone().flatten() {
                        Some(panes) => panes,
                        None => {
                            displays.watching = false;
                            return;
                        }
                    }
                };
                // While the gameplay overlay shows, the panes share its window.
                if root.overlay().is_some() {
                    continue;
                }
                let outcome = root
                    .korrid
                    .surface_displays(Some(panes.clone()), false, None, true)
                    .await;
                let crate::host::display_placement::SurfaceDisplaysOutcome::Ok(windows) = outcome
                else {
                    continue;
                };
                let changed = {
                    let mut displays = root.displays.lock().expect("displays poisoned");
                    let same_package =
                        displays.scope.as_ref().and_then(Option::as_ref) == Some(&panes);
                    let changed = same_package && displays.windows != windows;
                    if changed {
                        // What was placed was for the windows before.
                        displays.placement.redeclare();
                    }
                    changed
                };
                if changed {
                    root.place();
                }
            }
        });
    }

    /// Place the window for the current package and presentation. It counts
    /// as placed only once the effect succeeded; a failure is tried again on
    /// the next presentation change or window event, after its backoff.
    pub(super) fn place(self: &Arc<Self>) {
        let overlay_launch = self.overlay().map(|overlay| overlay.launch_id().to_owned());
        let (key, attempt) = {
            let mut displays = self.displays.lock().expect("displays poisoned");
            let Some(scope) = displays.scope.clone() else {
                return;
            };
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
                super::placement::Next::Attempt(attempt) => (key, attempt),
            }
        };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = this
                .korrid
                .surface_displays(key.0.clone(), key.1, key.2.clone(), false)
                .await;
            let windows = match outcome {
                // Only a package with panes gets windows published.
                crate::host::display_placement::SurfaceDisplaysOutcome::Ok(windows) => {
                    windows.filter(|_| key.0.is_some())
                }
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
                let changed = displays.windows != windows;
                displays.windows = windows;
                drop(displays);
                if changed {
                    (this.notify)();
                }
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
            displays.scope.clone().flatten()
        };
        let korrid = self.korrid.clone();
        tokio::spawn(async move {
            korrid.surface_displays(scope, false, None, false).await;
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

    pub(crate) fn session_failed(&self, message: String) {
        self.launchables.session_failed(message);
        self.show_catalog(None);
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
                    OverlayExit::SessionEnded | OverlayExit::Resumed => root.show_catalog(None),
                    OverlayExit::SessionFailed(message) => root.session_failed(message),
                    OverlayExit::OpenKorri => root.show_catalog(Some(launch_id.clone())),
                }
            }),
        );
        *self.presentation.lock().expect("presentation poisoned") =
            Presentation::Overlay(Arc::clone(&controller));
        (self.notify)();
        let local = Arc::clone(&controller);
        tokio::spawn(async move {
            loop {
                if !local.refresh_viewer().await {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
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
        let found = match found {
            Ok(found) => found,
            Err(failure) => {
                self.session_failed(failure.message);
                return;
            }
        };
        let found = found.filter(|active| {
            // Android's new Home intent can reopen the same running launch.
            // Linux still reopens an Open Korri launch only on focus failure.
            cfg!(target_os = "android")
                || !from_intent
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
        self.steam.start();
        self.accounts.start();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                this.steam_conflict_again();
                this.steam_cloud.poll();
            }
        });
        if let Some(changes) = &self.korrid.session_changes {
            let mut changes = changes();
            let this = Arc::clone(self);
            tokio::spawn(async move {
                while changes.changed().await.is_ok() {
                    this.launchables.session_changed().await;
                    this.inspect_overlay(false).await;
                }
            });
        }
        self.shared.start();
        self.installs.start();
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
        if let Some(mut battery) = self.battery.clone() {
            let notify = self.notify.clone();
            tokio::spawn(async move {
                while battery.changed().await.is_ok() {
                    notify();
                }
            });
        }
        self.owner_binding.start();
        self.launchables.start();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let sample = ClockSample::now();
                let changed = {
                    let mut clock = this.clock.lock().expect("clock poisoned");
                    // The time moves only with the label, each minute.
                    let changed = clock.label != sample.label;
                    if changed {
                        *clock = sample;
                    }
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

    /// The same local clock used by the catalog, also available over gameplay.
    pub fn clock(&self) -> String {
        self.clock.lock().expect("clock poisoned").label.clone()
    }

    /// The time read with that clock label: `SurfaceModel.publishedAt`.
    pub fn published_at(&self) -> Option<f64> {
        self.clock.lock().expect("clock poisoned").published_at
    }

    /// A test gives the model's time; no assertion reads the system clock.
    #[cfg(test)]
    pub fn set_published_at(&self, published_at: Option<f64>) {
        self.clock.lock().expect("clock poisoned").published_at = published_at;
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
            facts
                .settings
                .as_ref()
                .map_or(&[], |settings| settings.editable_setting_ids.as_slice()),
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
            .or_else(|| self.installs.status())
        {
            status = problem;
        }
        let status = self.with_launch_steps(status, &state);
        let status = self.steam_cloud.over(status);
        let onboarding = (onboarding_needed == Some(true) && owner.is_unsupported()).then(|| {
            SurfaceOnboarding {
                controllers: self.controllers.snapshot(),
                starter_pack: self.starter_pack.snapshot(),
            }
        });
        let games = catalog_games(&state).map(|games| self.decorate(games, &state));
        let clock = self.clock.lock().expect("clock poisoned").clone();
        let base = SurfaceModel {
            windows: None,
            presentation: SurfacePresentation::catalog(),
            catalog: korri_surface_treaty::SurfaceCatalog::Loading,
            status,
            save_history: self.saves.history(),
            runner_choice: Some(self.runner.snapshot()),
            launch_settings: Some(self.launch.editor()),
            clock_label: Some(clock.label),
            actions: Vec::new(),
            settings,
            settings_status: self
                .launch
                .status()
                .unwrap_or_else(|| self.launchables.settings_status()),
            identity_management: self.launchables.identity_management(),
            build_label: None,
            network: network.network,
            battery: self.battery(),
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
            systems: None,
            published_at: clock.published_at,
            account_sign_ins: self
                .steam
                .snapshot()
                .into_iter()
                .chain(self.accounts.snapshot())
                .collect(),
            installs: self.installs.list(&state),
        };
        Facts { base, games, state }
    }

    /// Download facts on each local game, as the portal decorated them.
    fn decorate(&self, games: Vec<SurfaceGame>, state: &LaunchablesState) -> Vec<SurfaceGame> {
        let facts = self.downloads.facts();
        let marks = self.saves.marks();
        let profiles = self.launch.active_profiles();
        // ADR 0092: a game that only other devices hold can be installed.
        let releases: BTreeMap<String, super::model::PeerRelease> = if self.installs.available() {
            state
                .entries()
                .iter()
                .filter_map(super::model::peer_release)
                .map(|release| (release.game_id.clone(), release))
                .collect()
        } else {
            BTreeMap::new()
        };
        if facts.is_empty() && marks.is_empty() && profiles.is_empty() && releases.is_empty() {
            return games;
        }
        let ids = decorated_game_ids_by_surface_id(state);
        games
            .into_iter()
            .map(|mut game| {
                if let Some(release) = releases.get(&game.id) {
                    game.install_here = self.installs.here(release);
                    game.download = self.installs.download(release);
                }
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
        // A streamed game has the same two actions (issue #401): its runners
        // and its launch settings are those of the device that runs it.
        let streamed = streamed_game(entry);
        if local_runner_entry(entry).is_some() {
            actions.push(SurfaceAction::new(
                "runners",
                "Runners on this device",
                healthy,
            ));
        } else if let Some(game) = streamed.filter(|game| game.supports_runner_selection) {
            actions.push(SurfaceAction::new(
                "runners",
                format!("Runners on {}", game.source.label),
                healthy,
            ));
        }
        if korrid_id.is_some() || streamed.is_some() {
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

    /// A launch is under way or a game runs: the person is busy with
    /// something, so a play-here install that ends now does not start.
    fn starting(&self) -> bool {
        let state = self.launchables.state();
        self.planning.load(std::sync::atomic::Ordering::SeqCst)
            || self
                .launch_plan
                .lock()
                .expect("launch plan poisoned")
                .is_some()
            || self.acquisition.status().is_some()
            || self.saves.status().is_some()
            || !matches!(
                &state,
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Ready,
                    ..
                })
            )
            || state.entries().iter().any(Entry::is_now_playing)
    }

    /// A play-here install is done (ADR 0092), or the person asked to play
    /// a release this device already holds: read the library until it holds
    /// the release's local copy, then start that copy here, as a press of
    /// Play on it would. The local copy is the catalog game with
    /// `source.isLocal` and this release key: the only one with a launch
    /// path on this device (`launchable_on_this_device`), never the stream.
    async fn play_installed(self: Arc<Self>, installed: super::installs::PlayInstalled) {
        for attempt in 0..PLAY_INSTALLED_READS {
            if attempt > 0 {
                tokio::time::sleep(PLAY_INSTALLED_WAIT).await;
            }
            self.launchables.refresh().await;
            // The person started something else meanwhile.
            if self.starting() {
                return;
            }
            let current = self.launchables.state();
            if let Some((id, local)) = local_copy_of_release(&current, &installed.key) {
                self.confirm_launch(
                    &id,
                    Entry::Game {
                        game: local,
                        alternatives: Vec::new(),
                    },
                );
                return;
            }
        }
        self.installs.play_failed(installed);
    }

    /// The person looks at a game: when only other devices hold it, learn
    /// whether this device has a runner for it.
    pub fn learn_install(&self, id: &str) {
        let current = self.launchables.state();
        if let Some(release) = entry_for_id(&current, id).and_then(super::model::peer_release) {
            self.installs.learn(release);
        }
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
            // Several runners on the holder: its chooser first (issue #401).
            // A live session of this game is returned to by the chooser.
            if streamed_game(Some(&chosen)).is_some_and(|game| game.supports_runner_selection) {
                if content.unavailable_session_status.is_none() {
                    self.runner.open(
                        game.id.clone(),
                        game.title.clone(),
                        Some(game.source.label.clone()),
                        Intent::Launch,
                    );
                }
                return;
            }
            *self.launch_plan.lock().expect("launch plan poisoned") =
                Some(LaunchPlan::new(&game.id, false, false));
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
                .open(game.id.clone(), game.title.clone(), None, Intent::Launch);
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
        let runner_steps = self
            .korrid
            .launch_steps(game.id.clone(), game.runner_id.clone())
            .await;
        LaunchPlan::with_runner_steps(
            &game.id,
            peer_in_reach,
            needs_download,
            runner_steps.as_deref(),
        )
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
    fn with_launch_steps(&self, status: SurfaceStatus, state: &LaunchablesState) -> SurfaceStatus {
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
            if plan.as_ref().is_some_and(|plan| {
                plan.current() == LaunchStep::Start
                    || state
                        .entries()
                        .iter()
                        .filter_map(Entry::session)
                        .any(|session| {
                            session.game_id.as_deref() == Some(plan.game_id())
                                && matches!(
                                    session.initial_handoff,
                                    Some(
                                        crate::InitialHandoff::Observed
                                            | crate::InitialHandoff::Recovered
                                    )
                                )
                        })
            }) {
                *plan = None;
            }
            return status;
        };
        if let Some(plan) = plan.as_mut() {
            if let Some(active) = state
                .entries()
                .iter()
                .filter_map(Entry::session)
                .find(|session| session.game_id.as_deref() == Some(plan.game_id()))
            {
                if active.steps.is_some()
                    || active.initial_handoff == Some(crate::InitialHandoff::Waiting)
                {
                    plan.reach(LaunchStep::Start);
                    if let Some(steps) = &active.steps {
                        plan.follow_runner(steps);
                    }
                }
            }
        }
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
        // ADR 0083: after the downloads, a launch on the Steam engine runner
        // with nobody signed in waits for the sign-in.
        *self.last_gated.lock().expect("last gated poisoned") = Some(game.clone());
        let steam = Arc::clone(&self.steam);
        let (steam_game, steam_abandon, start) = (game.clone(), abandon.clone(), proceed);
        let proceed: super::acquisition_gate::Step = Arc::new(move || {
            steam.gate(
                steam_game.id.clone(),
                steam_game.title.clone(),
                steam_game.runner_id.clone(),
                start.clone(),
                steam_abandon.clone(),
            )
        });
        self.acquisition.launch(game, proceed, abandon);
    }

    /// ADR 0083: the launch sync of the last launch found a Steam Cloud
    /// conflict, the shim refused to start the game, and korrid wrote the
    /// split. That same attempt goes back through the gate, whose save check
    /// finds the split and asks for the choice, as korrid's `SavesSplit`
    /// refusal does. The refused unit's failure is no news to the person.
    fn steam_conflict_again(self: &Arc<Self>) {
        let refused = matches!(
            self.launchables.state().loaded().and_then(|loaded| loaded.content.notice.as_ref()),
            Some(super::launchables::state::LaunchNotice::Launch { message, .. })
                if message.contains(crate::steam_engine::CONFLICT_REFUSAL)
        );
        if refused {
            self.launchables.dismiss_notice();
        }
        let game = self.last_gated.lock().expect("last gated poisoned").clone();
        let Some(game) = game else {
            return;
        };
        if !self.steam.take_conflict(&game.id) {
            return;
        }
        self.last_gated.lock().expect("last gated poisoned").take();
        self.end_plan();
        drop(self.gated_start(game));
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
            SurfaceRequest::ReportActivity => {}
            SurfaceRequest::LaunchGame {
                game_id,
                launch_location_id,
            } => {
                // A launch now: a play-here install no longer starts its game.
                self.installs.started_something_else();
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
                    || self.steam.act(&action_id)
                    || self.steam_cloud.act(&action_id)
                    || self.accounts.act(&action_id)
                    || self.store.act(&action_id)
                    || self.installs.act(&action_id)
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
                    // A launch that waits on a Steam Cloud question ends unplayed.
                    self.steam_cloud.cancelled();
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
            SurfaceRequest::SwitchIdentityToNip46 {
                bunker_uri,
                disposition,
                trust_loss_confirmed,
            } => {
                launchables.switch_identity_to_nip46(bunker_uri, disposition, trust_loss_confirmed)
            }
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
                        self.runner
                            .open(local.id, local.title, None, Intent::Inspect);
                    } else if let Some(game) = streamed_game(Some(&entry))
                        .filter(|game| healthy && game.supports_runner_selection)
                    {
                        self.runner.open(
                            game.id.clone(),
                            game.title.clone(),
                            Some(game.source.label.clone()),
                            Intent::Inspect,
                        );
                    }
                    return;
                }
                if action_id == LAUNCH_SETTINGS_ACTION_ID {
                    let Entry::Game { game, .. } = &entry else {
                        return;
                    };
                    let healthy = matches!(&current, LaunchablesState::Loaded(Loaded { content, .. }) if content.unavailable_session_status.is_none());
                    // The runner chooser, when this page offers it, fills in
                    // the settings that wait on a runner choice.
                    let chooser = |id: String, title: String, host: Option<String>| {
                        let runner = Arc::clone(&self.runner);
                        Arc::new(move || {
                            runner.open(id.clone(), title.clone(), host.clone(), Intent::Inspect)
                        }) as super::launch_settings::OpenRunners
                    };
                    if let Some(korrid_id) = local_game_ids_by_surface_id(&current)
                        .get(&game_id)
                        .cloned()
                    {
                        let choose = local_runner_entry(Some(&entry))
                            .filter(|_| healthy)
                            .map(|local| chooser(local.id, local.title, None));
                        self.launch
                            .open(korrid_id, game.title.clone(), None, choose);
                    } else if let Some(game) = streamed_game(Some(&entry)) {
                        // The list is read from and written to the holder.
                        let holder = game.source.label.clone();
                        let choose = (healthy && game.supports_runner_selection).then(|| {
                            chooser(game.id.clone(), game.title.clone(), Some(holder.clone()))
                        });
                        self.launch
                            .open(game.id.clone(), game.title.clone(), Some(holder), choose);
                    }
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
                if !self.saves.retry() && !self.acquisition.retry() && !self.installs.retry() {
                    launchables.reload()
                }
            }
            SurfaceRequest::Dismiss => {
                if !self.shared.dismiss()
                    && !self.saves.dismiss()
                    && !self.acquisition.dismiss()
                    && !self.downloads.dismiss()
                    && !self.installs.dismiss()
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
            SurfaceRequest::InstallGame { game_id, play } => {
                let current = launchables.state();
                if !current.is_ready() {
                    return;
                }
                if let Some(release) =
                    entry_for_id(&current, &game_id).and_then(super::model::peer_release)
                {
                    self.installs.install(release, play);
                }
            }
            SurfaceRequest::MoveInstall { install_id, index } => {
                self.installs.move_install(&install_id, index)
            }
            SurfaceRequest::CancelInstall { install_id } => self.installs.cancel(&install_id),
            SurfaceRequest::AnswerStreamMeanwhile {
                install_id,
                stream,
                launch_location_id,
            } => self
                .installs
                .answer(&install_id, stream, launch_location_id),
        }
    }
}

#[cfg(test)]
mod badge_ids_tests {
    use serde_json::json;

    use super::{decorated_game_ids_by_surface_id, local_game_ids_by_surface_id};
    use crate::{
        surface::launchables::state::LaunchablesState, CatalogSnapshotOutcome, SessionStatusOutcome,
    };

    #[test]
    fn a_running_local_game_keeps_its_badges_but_not_its_catalog_actions() {
        let catalog: CatalogSnapshotOutcome = serde_json::from_value(json!({ "_tag": "Ok", "payload": { "games": [
            { "id": "local", "title": "Local", "source": { "label": "this", "isLocal": true }, "host": "this", "supportsRunnerSelection": false },
        ] } }))
        .unwrap();
        let before = LaunchablesState::from_sources(&catalog, None, None, &[], None);
        let key = before.entries()[0].key();
        let active: SessionStatusOutcome = serde_json::from_value(
            json!({ "_tag": "Ok", "payload": { "active": { "launchId": "l1", "gameId": "local" } } }),
        )
        .unwrap();
        let running = before.with_session_status(&active);
        assert!(running.entries()[0].is_now_playing());
        assert_eq!(
            decorated_game_ids_by_surface_id(&running)
                .get(&key)
                .map(String::as_str),
            Some("local")
        );
        // Catalog actions (launch settings, save history) stay off the running card.
        assert!(!local_game_ids_by_surface_id(&running).contains_key(&key));
    }
}
