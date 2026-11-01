//! Port of `clients/portal/src/surface/SurfaceRoot.tsx`: where Korri meets a
//! surface. It composes the host controllers, builds one `SurfaceModel`
//! from their facts, and performs each `SurfaceRequest` with the same rules
//! the portal's `SurfaceHost` methods had.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use korri_surface_treaty::{
    SurfaceAction, SurfaceGame, SurfaceModel, SurfaceOnboarding, SurfacePresentation,
    SurfaceRequest, SurfaceStatus,
};

use super::{
    downloads::{DownloadGameRef, GameDownloads, ACQUISITION_POLL_INTERVAL},
    korrid::Korrid,
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
    settings_model::settings_from,
    starter_pack::StarterPack,
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

fn local_runner_entry(entry: Option<&Entry>) -> bool {
    match entry {
        Some(Entry::Game { game, .. })
            if game.source.is_local && game.supports_runner_selection =>
        {
            true
        }
        Some(entry @ (Entry::Game { .. } | Entry::LocalGame { .. })) => entry
            .alternatives()
            .iter()
            .filter_map(|copy| copy.remote())
            .any(|game| game.source.is_local && game.supports_runner_selection),
        _ => false,
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
    presentation: std::sync::Mutex<Presentation>,
    pub launchables: Arc<Launchables>,
    pub network: Arc<NetworkOnboarding>,
    pub owner_binding: Arc<OwnerBinding>,
    pub starter_pack: Arc<StarterPack>,
    pub downloads: Arc<GameDownloads>,
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
    pub fn new(korrid: Korrid, notify: Notify) -> Arc<Self> {
        let launchables_korrid = korrid.clone();
        let network = NetworkOnboarding::new(korrid.clone(), notify.clone());
        let downloads =
            GameDownloads::new(korrid.clone(), notify.clone(), ACQUISITION_POLL_INTERVAL);
        let launchables = {
            let restart = Arc::clone(&network);
            let dismiss = Arc::clone(&network);
            Launchables::new(
                korrid.clone(),
                notify.clone(),
                LaunchablesHooks {
                    launch_warnings: Box::new(|_, _| {}),
                    plugins_refresh: Box::new(|| {}),
                    plugins_run: Box::new(|_| false),
                    plugins_change: Box::new(|_, _| false),
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
                korrid,
                notify.clone(),
                Arc::new(move || {
                    if let Some(launchables) = reload.upgrade() {
                        launchables.reload();
                    }
                }),
            )
        };
        Arc::new(Self {
            korrid: launchables_korrid,
            presentation: std::sync::Mutex::new(Presentation::Catalog {
                retained_failure: None,
            }),
            launchables,
            network,
            owner_binding,
            starter_pack,
            downloads,
            clock: std::sync::Mutex::new(clock_label()),
            last_local_ids: std::sync::Mutex::new(None),
            notify,
        })
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
    async fn inspect_overlay(self: &Arc<Self>) {
        let retained = match &*self.presentation.lock().expect("presentation poisoned") {
            Presentation::Catalog { retained_failure } => retained_failure.clone(),
            Presentation::Overlay(_) => return,
        };
        let found = match &retained {
            // While the catalog stays in front after Open Korri, only the
            // exact launch's focus-failed phase reopens it.
            Some(launch_id) => {
                focused_overlay_session(&self.korrid, false, Some(launch_id), 1).await
            }
            None => focused_overlay_session(&self.korrid, true, None, 4).await,
        };
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
    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick: u64 = 0;
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                tick += 1;
                match this.overlay() {
                    // The overlay re-reads its launch every second.
                    Some(controller) if tick % 4 == 0 => controller.refresh().await,
                    Some(_) => {}
                    None => {
                        let retained = matches!(
                            &*this.presentation.lock().expect("presentation poisoned"),
                            Presentation::Catalog {
                                retained_failure: Some(_)
                            }
                        );
                        if retained || tick % 4 == 0 {
                            this.inspect_overlay().await;
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
        let onboarding = self.network.snapshot().onboarding_needed;
        self.launchables.set_onboarding_needed(onboarding);
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
        let facts = self.launchables.facts();
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
        if let Some(problem) = self.downloads.status() {
            status = problem;
        }
        let onboarding = (onboarding_needed == Some(true) && owner.is_unsupported()).then(|| {
            SurfaceOnboarding {
                // Controllers come from the native input stream, which the
                // shell owns; korrid does not see them.
                controllers: Vec::new(),
                starter_pack: self.starter_pack.snapshot(),
            }
        });
        let games = catalog_games(&state).map(|games| self.decorate(games, &state));
        let base = SurfaceModel {
            displays: None,
            presentation: SurfacePresentation::catalog(),
            catalog: korri_surface_treaty::SurfaceCatalog::Loading,
            status,
            save_history: None,
            runner_choice: None,
            launch_settings: None,
            clock_label: Some(self.clock.lock().expect("clock poisoned").clone()),
            actions: Vec::new(),
            settings,
            settings_status: self.launchables.settings_status(),
            identity_management: self.launchables.identity_management(),
            build_label: None,
            network: network.network,
            onboarding,
            store: None,
            invites: None,
            focused_game: None,
        };
        Facts { base, games, state }
    }

    /// Download facts on each local game, as the portal decorated them.
    fn decorate(&self, games: Vec<SurfaceGame>, state: &LaunchablesState) -> Vec<SurfaceGame> {
        let facts = self.downloads.facts();
        if facts.is_empty() {
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
        if local_runner_entry(entry) {
            actions.push(SurfaceAction::new(
                "runners",
                "Runners on this device",
                healthy,
            ));
        }
        if korrid_id.is_some() {
            actions.push(SurfaceAction::new(
                "launch-settings",
                "Launch settings",
                ready_now,
            ));
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
        let _ = id;
        let launchables = Arc::clone(&self.launchables);
        let game_id = game.id.clone();
        tokio::spawn(async move {
            launchables.start_catalog_launch(&game_id, None).await;
        });
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
                if !current.is_ready() {
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
                if self.owner_binding.act(&action_id) || self.starter_pack.act(&action_id) {
                    return;
                }
                if let Some(launch_id) = action_id.strip_prefix("cancel-pending:") {
                    launchables.cancel_pending_launch(launch_id);
                } else if action_id == "cancel-launch" {
                    launchables.cancel_launch();
                } else if action_id.starts_with("runner:") {
                    // The runner chooser publishes these; it is not ported.
                } else {
                    launchables.run_device_action(&action_id);
                }
            }
            SurfaceRequest::ChangeSetting { setting_id, value } => {
                launchables.change_setting(&setting_id, &value)
            }
            SurfaceRequest::DismissSettingsProblem => launchables.dismiss_settings_problem(),
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
                if !current.is_ready() {
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
                if action_id == "runners" || action_id == "launch-settings" {
                    // The runner chooser and the launch settings editor are not ported.
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
            SurfaceRequest::Retry => launchables.reload(),
            SurfaceRequest::Dismiss => {
                if !self.downloads.dismiss() {
                    launchables.dismiss_notice();
                }
            }
            SurfaceRequest::Reload => {
                launchables.reload();
                let network = Arc::clone(&self.network);
                network.dismiss_problem();
                tokio::spawn(async move { network.refresh().await });
                let downloads = Arc::clone(&self.downloads);
                tokio::spawn(async move { downloads.refresh(None).await });
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
            // The store is not ported.
            SurfaceRequest::OpenStore
            | SurfaceRequest::SearchStore { .. }
            | SurfaceRequest::OpenStoreEntry { .. } => {}
            SurfaceRequest::FocusGame { .. }
            | SurfaceRequest::RememberPreference { .. }
            | SurfaceRequest::MoveCatalog { .. } => {}
        }
    }
}
