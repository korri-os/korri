//! Port of `clients/portal/src/surface/use-launchables.ts`: the launchables
//! brain, independent of any surface. It owns every effect the screen can
//! cause — loading sources, launching a local game, preparing a stream,
//! resuming, stopping — and publishes the result as the `LaunchablesState`
//! ADT. It does not own selection or input: a surface decides what is
//! focused, and the host calls `confirm_entry` with the entry it means.
//!
//! React's refs and effects become one mutex and a few tokio loops. The
//! sequence counters (`action_seq`, `load_seq`, `stop_poll_seq`) keep exactly
//! the portal's rules about which late reply may still write state.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use korri_surface_treaty::{
    SurfaceIdentityDisposition, SurfaceIdentityManagement, SurfaceIdentityStatus,
    SurfaceSettingsStatus,
};

use super::state::{
    is_authoritative_session_status, is_local_catalog_session, launch_subject_for_game,
    without_sessions, Content, Entry, LaunchNotice, LaunchablesState, Loaded, Phase,
};
use crate::{
    game_routes::SelectedGameLaunchOutcome,
    launcher::typed_settings::LaunchWarning,
    surface::{
        korrid::Korrid,
        settings_model::{DeviceFacts, SETUP_RESTART_ACTION_ID},
        Notify,
    },
    ActiveSession, Game, HealthOutcome, IdentityBackupOutcome, IdentityDataDisposition,
    IdentityRetiredDeleteOutcome, IdentityStatusOutcome, IdentitySwitchOutcome,
    LibraryFoldersSnapshot, LibraryFoldersSnapshotOutcome, LibraryFoldersState,
    LocalGamesListOutcome, PeerListOutcome, RpcFailure, SensitiveSettingOutcome, SessionEndOutcome,
    SessionEndPhase, SessionReserveOutcome, SessionStatus, SessionStatusOutcome,
    SettingsSnapshotOutcome, SettingsUpdateOutcome,
};

/// A slow or hung status query must not hold the whole list hostage.
const SESSION_STATUS_TIMEOUT: Duration = Duration::from_millis(3000);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(500);
const STOP_POLL_DEADLINE: Duration = Duration::from_millis(8000);
const DISCOVERY_POLL_INTERVAL: Duration = Duration::from_millis(750);
const LOCAL_SESSION_POLL_INTERVAL: Duration = Duration::from_millis(500);
const PEER_POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// updatedAt is an observation timestamp, not a catalog revision.
fn peer_catalog_key(outcome: &PeerListOutcome) -> String {
    match outcome {
        PeerListOutcome::Ok(list) => serde_json::to_string(
            &list
                .peers
                .iter()
                .map(|peer| {
                    serde_json::json!([
                        peer.device_public_key,
                        peer.label,
                        peer.state,
                        peer.last_error
                    ])
                })
                .collect::<Vec<_>>(),
        ),
        PeerListOutcome::Err(failure) => serde_json::to_string(&serde_json::json!({
            "_tag": "Err", "payload": failure
        })),
    }
    .expect("peer key serializes")
}

fn discovery_active(snapshot: Option<&LibraryFoldersSnapshot>) -> bool {
    matches!(
        snapshot.map(|snapshot| &snapshot.state),
        Some(LibraryFoldersState::Scanning {} | LibraryFoldersState::Enriching {})
    )
}

/// What the result of `start_catalog_launch` carries for the gate.
pub type SessionStartResult = SelectedGameLaunchOutcome;

#[derive(Clone, Debug)]
struct Startup {
    token: u64,
    operation: u64,
    cancelled: bool,
    launch_id: Option<String>,
    cancel_in_flight: bool,
}

type Hook = Box<dyn Fn() + Send + Sync>;
type Taker = Box<dyn Fn(&str) -> bool + Send + Sync>;
type Changer = Box<dyn Fn(&str, &str) -> bool + Send + Sync>;

/// The parts of the portal the launchables brain calls back into.
pub struct LaunchablesHooks {
    /// The one-launch values of a game for the runner its launch names. No
    /// runner: the launch names none. The ticket goes back on completion.
    pub launch_overrides: Box<
        dyn Fn(&str, Option<&str>) -> (u64, Option<crate::launch_settings::LaunchOverride>)
            + Send
            + Sync,
    >,
    /// The launch of that ticket started or failed. On a start, what it
    /// carried ends with it.
    pub launch_overrides_complete: Box<dyn Fn(u64, bool) + Send + Sync>,
    /// A prepared launch korrid started with launch warnings.
    pub launch_warnings: Box<dyn Fn(String, Vec<LaunchWarning>) + Send + Sync>,
    /// Plugin repository rows: refresh, actions and changes. Each answers
    /// whether it took the id.
    pub plugins_refresh: Hook,
    pub plugins_run: Taker,
    pub plugins_change: Changer,
    /// Setup restart and the dismissal of the network problem.
    pub restart_onboarding: Hook,
    pub dismiss_network_problem: Hook,
}

struct Inner {
    state: LaunchablesState,
    facts: DeviceFacts,
    settings_status: SurfaceSettingsStatus,
    identity_management: Option<SurfaceIdentityManagement>,
    last_entries: Vec<Entry>,
    settings_busy: bool,
    load_seq: u64,
    applied_catalog_peers_key: Option<String>,
    retry_empty_catalog: bool,
    action_seq: u64,
    stop_poll_seq: u64,
    startup: Option<Startup>,
    next_token: u64,
    dismissed_stream_end: Option<String>,
    discovery_was_active: bool,
}

pub struct Launchables {
    korrid: Korrid,
    inner: Mutex<Inner>,
    notify: Notify,
    hooks: LaunchablesHooks,
}

impl Inner {
    /// Port of the portal's `publish`: the counters move exactly as there.
    fn publish(&mut self, mut next: LaunchablesState) {
        let previous = &self.state;
        if matches!(
            previous,
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Launching {
                    return_launch_id: Some(_),
                    ..
                },
                ..
            })
        ) && !matches!(
            next,
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Launching { .. },
                ..
            })
        ) {
            self.action_seq += 1;
        }
        let next_launch_id = match &next {
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Starting { launch_id, .. },
                ..
            }) => launch_id.clone(),
            _ => None,
        };
        let previous_choosing = match &self.state {
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Choosing { choices },
                ..
            }) => Some(choices.clone()),
            _ => None,
        };
        let next_choosing = matches!(
            next,
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Choosing { .. },
                ..
            })
        );
        let surviving_choice = previous_choosing.as_ref().is_some_and(|choices| {
            next_launch_id
                .as_ref()
                .is_some_and(|id| choices.iter().any(|choice| &choice.launch_id == id))
        });
        if previous_choosing.is_some() && !next_choosing && !surviving_choice {
            self.action_seq += 1;
        }
        if let LaunchablesState::Loaded(Loaded {
            phase:
                Phase::Starting {
                    cancelling: true,
                    cancel_retry_available,
                    ..
                },
            ..
        }) = &mut next
        {
            if self
                .startup
                .as_ref()
                .is_some_and(|owned| owned.cancel_in_flight)
            {
                *cancel_retry_available = Some(false);
            }
        }
        match &next {
            LaunchablesState::Loaded(Loaded {
                phase:
                    Phase::Starting {
                        launch_id: Some(launch_id),
                        cancelling,
                        ..
                    },
                ..
            }) if self
                .startup
                .as_ref()
                .and_then(|owned| owned.launch_id.as_ref())
                != Some(launch_id) =>
            {
                let operation = if surviving_choice {
                    self.action_seq
                } else {
                    self.action_seq += 1;
                    self.action_seq
                };
                self.next_token += 1;
                self.startup = Some(Startup {
                    token: self.next_token,
                    operation,
                    launch_id: Some(launch_id.clone()),
                    cancelled: *cancelling,
                    cancel_in_flight: false,
                });
            }
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Starting { .. },
                ..
            }) => {}
            _ => {
                if self.startup.is_some() {
                    self.startup = None;
                    self.action_seq += 1;
                }
            }
        }
        if let LaunchablesState::Loaded(loaded) = &next {
            self.last_entries = loaded.content.entries.clone();
        }
        self.state = next;
    }

    fn startup_owned(&self, token: u64) -> bool {
        self.startup
            .as_ref()
            .is_some_and(|owned| owned.token == token)
    }

    fn take_state(&mut self) -> LaunchablesState {
        self.state.clone()
    }
}

fn status_failure(status: &SessionStatusOutcome) -> Option<&RpcFailure> {
    match status {
        SessionStatusOutcome::Ok(payload) => payload.observation_failure.as_ref(),
        SessionStatusOutcome::Err(failure) => Some(failure),
    }
}

fn is_stale_code(code: &str) -> bool {
    ["StaleLaunchIdentity", "NoActiveSession", "SessionCompleted"].contains(&code)
}

impl Launchables {
    pub fn new(korrid: Korrid, notify: Notify, hooks: LaunchablesHooks) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            inner: Mutex::new(Inner {
                state: LaunchablesState::Loading,
                facts: DeviceFacts::default(),
                settings_status: SurfaceSettingsStatus::Idle,
                identity_management: None,
                last_entries: Vec::new(),
                settings_busy: false,
                load_seq: 0,
                applied_catalog_peers_key: None,
                retry_empty_catalog: false,
                action_seq: 0,
                stop_poll_seq: 0,
                startup: None,
                next_token: 0,
                dismissed_stream_end: None,
                discovery_was_active: false,
            }),
            notify,
            hooks,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("launchables poisoned")
    }

    fn publish(&self, next: LaunchablesState) {
        self.lock().publish(next);
        (self.notify)();
    }

    fn changed(&self) {
        (self.notify)();
    }

    /// Input precedence reads the synchronously published owner.
    pub fn state(&self) -> LaunchablesState {
        self.lock().state.clone()
    }
    pub fn facts(&self) -> DeviceFacts {
        self.lock().facts.clone()
    }
    pub fn settings_status(&self) -> SurfaceSettingsStatus {
        self.lock().settings_status.clone()
    }
    pub fn identity_management(&self) -> Option<SurfaceIdentityManagement> {
        self.lock().identity_management.clone()
    }
    /// The shared settings lock the plugin rows take too.
    pub fn settings_busy(&self) -> bool {
        self.lock().settings_busy
    }
    pub fn set_settings_busy(&self, busy: bool) {
        self.lock().settings_busy = busy;
    }
    pub fn set_settings_status(&self, status: SurfaceSettingsStatus) {
        self.publish_settings_status(status);
    }

    pub fn set_onboarding_needed(&self, needed: Option<bool>) {
        self.lock().facts.onboarding_needed = needed;
    }
    pub fn set_plugin_groups(
        &self,
        groups: Option<Vec<korri_surface_treaty::SurfaceSettingGroup>>,
    ) {
        self.lock().facts.plugin_groups = groups;
        self.changed();
    }

    async fn session_status_with_timeout(&self) -> SessionStatusOutcome {
        let outcome = self
            .korrid
            .session_status(Some(SESSION_STATUS_TIMEOUT))
            .await;
        let dismissed = self.lock().dismissed_stream_end.clone();
        match outcome {
            SessionStatusOutcome::Ok(mut payload)
                if payload
                    .stream_play_ended
                    .as_ref()
                    .map(|ended| &ended.launch_id)
                    == dismissed.as_ref()
                    && dismissed.is_some() =>
            {
                payload.stream_play_ended = None;
                SessionStatusOutcome::Ok(payload)
            }
            other => other,
        }
    }

    fn publish_settings_status(&self, next: SurfaceSettingsStatus) {
        self.lock().settings_status = next;
        self.changed();
    }

    fn settings_problem(&self, setting_id: &str, message: &str) {
        self.publish_settings_status(SurfaceSettingsStatus::Problem {
            setting_id: setting_id.into(),
            message: message.into(),
        });
    }

    fn spawn_load(self: &Arc<Self>, preserve_action: bool) {
        let this = Arc::clone(self);
        tokio::spawn(async move { this.load(preserve_action, None).await });
    }

    /// Re-read every source. `preserve_action` keeps a command's lock.
    pub async fn load(
        self: &Arc<Self>,
        mut preserve_action: bool,
        observed_peers: Option<PeerListOutcome>,
    ) {
        let (action, seq) = {
            let mut inner = self.lock();
            let preserving_command = matches!(
                inner.state,
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Stopping { .. } | Phase::Starting { .. } | Phase::Choosing { .. },
                    ..
                })
            );
            if matches!(
                inner.state,
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Starting { .. } | Phase::Choosing { .. },
                    ..
                })
            ) {
                preserve_action = true;
            }
            if !preserve_action && !preserving_command {
                // A normal full reload supersedes pending UI work.
                inner.action_seq += 1;
                inner.stop_poll_seq += 1;
                inner.publish(LaunchablesState::Loading);
            }
            inner.load_seq += 1;
            (inner.action_seq, inner.load_seq)
        };
        self.changed();
        let fresh = |this: &Self| {
            let inner = this.lock();
            seq == inner.load_seq && action == inner.action_seq
        };
        // Observe readiness BEFORE the catalog.
        let peers = match observed_peers {
            Some(peers) => peers,
            None => self.korrid.peer_list().await,
        };
        if !fresh(self) {
            return;
        }
        (self.hooks.plugins_refresh)();
        let (games, local_games, session, health, settings, discovery, identity) = tokio::join!(
            self.korrid.catalog_snapshot(),
            self.korrid.local_games(),
            self.session_status_with_timeout(),
            self.korrid.health(),
            self.korrid.settings_snapshot(),
            self.korrid.library_folders_snapshot(),
            self.korrid.identity_status(),
        );
        if !fresh(self) {
            return;
        }
        {
            let mut inner = self.lock();
            match &identity {
                IdentityStatusOutcome::Ok(status) if status.can_switch_identity => {
                    let previous = inner
                        .identity_management
                        .as_ref()
                        .map(|current| current.status.clone());
                    inner.identity_management = Some(SurfaceIdentityManagement {
                        local_backup_available: status.local_backup_available,
                        retired_public_keys: status.retired_public_keys.clone(),
                        status: previous.unwrap_or(SurfaceIdentityStatus::Idle),
                    });
                }
                IdentityStatusOutcome::Ok(_) => inner.identity_management = None,
                IdentityStatusOutcome::Err(failure)
                    if failure.code == "OperationUnsupported"
                        || failure.code == "PermissionDenied" =>
                {
                    inner.identity_management = None
                }
                IdentityStatusOutcome::Err(_) => {}
            }
            let onboarding_needed = inner.facts.onboarding_needed;
            let plugin_groups = inner.facts.plugin_groups.clone();
            let launch_settings = inner.facts.launch_settings.clone();
            inner.facts = DeviceFacts {
                version: match &health {
                    HealthOutcome::Ok(health) => Some(health.version.clone()),
                    HealthOutcome::Err(_) => None,
                },
                settings: match &settings {
                    SettingsSnapshotOutcome::Ok(settings) => Some(settings.clone()),
                    SettingsSnapshotOutcome::Err(_) => None,
                },
                local_game_count: match &local_games {
                    LocalGamesListOutcome::Ok(list) => Some(list.games.len()),
                    LocalGamesListOutcome::Err(_) => None,
                },
                discovery: match &discovery {
                    LibraryFoldersSnapshotOutcome::Ok(snapshot) => Some(snapshot.clone()),
                    LibraryFoldersSnapshotOutcome::Err(_) => None,
                },
                onboarding_needed,
                plugin_groups,
                launch_settings,
            };
        }
        let discovery_ended = self.observe_discovery();
        let current = self.state();
        let current_phase = current.loaded().map(|loaded| &loaded.phase);
        // Recovery reads must not replace a newer launch operation's lock.
        if preserve_action
            && matches!(
                current_phase,
                Some(Phase::Starting { .. } | Phase::Choosing { .. })
            )
        {
            let observed = current.with_session_status(&session);
            let mut inner = self.lock();
            if observed.is_ready() {
                inner.startup = None;
            }
            inner.publish(observed);
            drop(inner);
            self.changed();
            return;
        }
        if preserve_action
            && matches!(
                current_phase,
                Some(Phase::Launching {
                    return_launch_id: Some(_),
                    ..
                })
            )
        {
            self.publish(current.with_session_status(&session));
            return;
        }
        if preserve_action && !matches!(current_phase, None | Some(Phase::Ready | Phase::Recovery))
        {
            return;
        }
        let previous_entries = self.lock().last_entries.clone();
        let loaded = LaunchablesState::from_sources(
            &games,
            Some(&session),
            Some(&local_games),
            &previous_entries,
            Some(&peers),
        );
        if let Some(Phase::Stopping { launch_id }) = current_phase {
            let active = match &session {
                SessionStatusOutcome::Ok(payload) => payload.active.as_ref(),
                SessionStatusOutcome::Err(_) => None,
            };
            if !is_authoritative_session_status(&session)
                || active.map(|active| &active.launch_id) == Some(launch_id)
            {
                return;
            }
            let mut inner = self.lock();
            inner.action_seq += 1;
            inner.stop_poll_seq += 1;
        }
        let keep_notice = match (&current, &loaded) {
            (
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Ready,
                    content:
                        Content {
                            notice: Some(notice),
                            ..
                        },
                }),
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Ready,
                    ..
                }),
            ) if preserve_action && !matches!(notice, LaunchNotice::Catalog { .. }) => {
                Some(notice.clone())
            }
            _ => None,
        };
        let published = match (keep_notice, loaded) {
            (Some(notice), LaunchablesState::Loaded(mut loaded)) => {
                loaded.content.notice = Some(notice);
                LaunchablesState::Loaded(loaded)
            }
            (_, loaded) => loaded,
        };
        {
            let mut inner = self.lock();
            let retry = match &published {
                LaunchablesState::Loading => false,
                LaunchablesState::Loaded(loaded) => {
                    !loaded
                        .content
                        .entries
                        .iter()
                        .any(|entry| !entry.is_now_playing())
                        && match &games {
                            crate::CatalogSnapshotOutcome::Err(_) => true,
                            crate::CatalogSnapshotOutcome::Ok(snapshot) => snapshot
                                .failures
                                .as_ref()
                                .is_some_and(|failures| !failures.is_empty()),
                        }
                }
            };
            inner.publish(published);
            inner.applied_catalog_peers_key = Some(peer_catalog_key(&peers));
            // A failed read is not an empty library. Retry it.
            inner.retry_empty_catalog = retry;
        }
        self.changed();
        if discovery_ended {
            self.spawn_load(false);
        }
    }

    /// Start the polls the portal ran from effects, then the first load.
    pub fn start(self: &Arc<Self>) {
        self.spawn_load(false);
        // PeerList is the local readiness contract: observe membership and
        // state changes even after an empty/failed initial read.
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(PEER_POLL_INTERVAL).await;
                let seq = this.lock().load_seq;
                let peers = this.korrid.peer_list().await;
                let reload = {
                    let inner = this.lock();
                    seq == inner.load_seq
                        && (inner.retry_empty_catalog
                            || Some(peer_catalog_key(&peers)) != inner.applied_catalog_peers_key)
                };
                if reload {
                    this.load(true, Some(peers)).await;
                }
            }
        });
        // Observe the local session and startup, including while a
        // reservation or command is pending.
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                this.poll_local_session().await;
                tokio::time::sleep(LOCAL_SESSION_POLL_INTERVAL).await;
            }
        });
        // Poll discovery while it scans; reload once it settles.
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(DISCOVERY_POLL_INTERVAL).await;
                let active = discovery_active(this.lock().facts.discovery.as_ref());
                if !active {
                    continue;
                }
                if let LibraryFoldersSnapshotOutcome::Ok(snapshot) =
                    this.korrid.library_folders_snapshot().await
                {
                    this.lock().facts.discovery = Some(snapshot);
                    this.changed();
                    if this.observe_discovery() {
                        this.load(false, None).await;
                    }
                }
            }
        });
    }

    /// True when discovery was active and now is not: the portal reloaded.
    fn observe_discovery(&self) -> bool {
        let mut inner = self.lock();
        let active = discovery_active(inner.facts.discovery.as_ref());
        let ended = inner.discovery_was_active && !active;
        inner.discovery_was_active = active;
        ended
    }

    fn local_launch_id(state: &LaunchablesState) -> Option<String> {
        let loaded = state.loaded()?;
        loaded.content.entries.iter().find_map(|entry| {
            entry
                .session()
                .filter(|session| {
                    is_local_catalog_session(session, &loaded.content.entries)
                        || loaded.content.stream_play_launch_id.as_ref() == Some(&session.launch_id)
                })
                .map(|session| session.launch_id.clone())
        })
    }

    async fn poll_local_session(self: &Arc<Self>) {
        let (operation, load_operation, local_launch_id, observing) = {
            let inner = self.lock();
            let observing = matches!(
                inner.state,
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Starting { .. } | Phase::Recovery | Phase::Choosing { .. },
                    ..
                })
            );
            (
                inner.action_seq,
                inner.load_seq,
                Self::local_launch_id(&inner.state),
                observing,
            )
        };
        if local_launch_id.is_none() && !observing {
            return;
        }
        let eligible = matches!(
            self.lock().state,
            LaunchablesState::Loaded(Loaded {
                phase: Phase::Ready
                    | Phase::Starting { .. }
                    | Phase::Recovery
                    | Phase::Choosing { .. }
                    | Phase::Launching {
                        return_launch_id: Some(_),
                        ..
                    },
                ..
            })
        );
        if !eligible {
            return;
        }
        let status = self.session_status_with_timeout().await;
        let reload = {
            let mut inner = self.lock();
            if operation != inner.action_seq || load_operation != inner.load_seq {
                return;
            }
            let observed = inner.take_state().with_session_status(&status);
            let observed_ready = observed.is_ready();
            if observed_ready {
                inner.startup = None;
            }
            inner.publish(observed);
            observed_ready
                && is_authoritative_session_status(&status)
                && match &status {
                    SessionStatusOutcome::Err(_) => true,
                    SessionStatusOutcome::Ok(payload) => {
                        payload.active.as_ref().map(|active| &active.launch_id)
                            != local_launch_id.as_ref()
                    }
                }
        };
        self.changed();
        if reload {
            // Refresh play facts after exit, without Loading.
            self.load(true, None).await;
        }
    }

    fn notice_on_ready(&self, operation: u64, message: &str) {
        let mut inner = self.lock();
        if operation != inner.action_seq || !inner.state.is_ready() {
            return;
        }
        let next = inner.take_state().with_notice(message);
        inner.publish(next);
        drop(inner);
        self.changed();
    }

    pub fn run_device_action(self: &Arc<Self>, action_id: &str) {
        if (self.hooks.plugins_run)(action_id) {
            return;
        }
        if action_id == SETUP_RESTART_ACTION_ID {
            (self.hooks.restart_onboarding)();
            return;
        }
        let saving = |this: &Self, setting_id: &str| matches!(&this.lock().settings_status, SurfaceSettingsStatus::Saving { setting_id: saving } if saving == setting_id);
        if action_id == "library-folder-rescan" {
            if discovery_active(self.lock().facts.discovery.as_ref()) || saving(self, action_id) {
                return;
            }
            self.publish_settings_status(SurfaceSettingsStatus::Saving {
                setting_id: action_id.into(),
            });
            let this = Arc::clone(self);
            let action_id = action_id.to_owned();
            tokio::spawn(async move {
                match this.korrid.rescan_library_folders().await {
                    LibraryFoldersSnapshotOutcome::Err(failure) => {
                        this.settings_problem(&action_id, &failure.message)
                    }
                    LibraryFoldersSnapshotOutcome::Ok(snapshot) => {
                        this.lock().facts.discovery = Some(snapshot);
                        this.observe_discovery();
                        this.publish_settings_status(SurfaceSettingsStatus::Idle);
                    }
                }
            });
            return;
        }
        if let Some(source) = action_id.strip_prefix("library-folder-remove:") {
            let setting_id = format!("library-folder:{source}");
            if saving(self, &setting_id) {
                return;
            }
            self.publish_settings_status(SurfaceSettingsStatus::Saving {
                setting_id: setting_id.clone(),
            });
            let this = Arc::clone(self);
            let source = source.to_owned();
            tokio::spawn(async move {
                match this.korrid.remove_library_folder(source).await {
                    LibraryFoldersSnapshotOutcome::Err(failure) => {
                        this.settings_problem(&setting_id, &failure.message)
                    }
                    LibraryFoldersSnapshotOutcome::Ok(snapshot) => {
                        this.lock().facts.discovery = Some(snapshot);
                        this.observe_discovery();
                        this.publish_settings_status(SurfaceSettingsStatus::Idle);
                    }
                }
            });
            return;
        }
        self.settings_problem(action_id, "This setting is not available");
    }

    pub fn change_setting(self: &Arc<Self>, setting_id: &str, value: &str) {
        // Plugin rows have their own korrid commands and the same lock.
        if (self.hooks.plugins_change)(setting_id, value) {
            return;
        }
        {
            let mut inner = self.lock();
            if inner.settings_busy {
                return;
            }
            inner.settings_busy = true;
        }
        self.publish_settings_status(SurfaceSettingsStatus::Saving {
            setting_id: setting_id.into(),
        });
        let this = Arc::clone(self);
        let setting_id = setting_id.to_owned();
        let value = value.to_owned();
        if setting_id == "steamgriddb-credential" {
            tokio::spawn(async move {
                let result = if value.trim().is_empty() {
                    this.korrid.clear_steam_grid_db_credential().await
                } else {
                    this.korrid.set_steam_grid_db_credential(value).await
                };
                this.lock().settings_busy = false;
                match result {
                    SensitiveSettingOutcome::Err(failure) => {
                        this.settings_problem(&setting_id, &failure.message)
                    }
                    SensitiveSettingOutcome::Ok(_) => {
                        this.publish_settings_status(SurfaceSettingsStatus::Idle);
                        this.load(false, None).await;
                    }
                }
            });
            return;
        }
        // A plugin's secret row: an empty value clears, as its clear action
        // sends. The value goes to korrid and is never kept here.
        if let Some(plugin_id) = crate::surface::plugins::credential_plugin_from(&setting_id) {
            tokio::spawn(async move {
                let result = if value.trim().is_empty() {
                    this.korrid.clear_plugin_credential(plugin_id).await
                } else {
                    this.korrid.set_plugin_credential(plugin_id, value).await
                };
                this.lock().settings_busy = false;
                match result {
                    SensitiveSettingOutcome::Err(failure) => {
                        this.settings_problem(&setting_id, &failure.message)
                    }
                    SensitiveSettingOutcome::Ok(_) => {
                        this.publish_settings_status(SurfaceSettingsStatus::Idle);
                        this.load(false, None).await;
                    }
                }
            });
            return;
        }
        let revision = self
            .lock()
            .facts
            .settings
            .as_ref()
            .map(|settings| settings.revision.clone())
            .filter(|revision| !revision.is_empty());
        let Some(revision) = revision else {
            self.lock().settings_busy = false;
            self.settings_problem(&setting_id, "Settings are not available");
            return;
        };
        tokio::spawn(async move {
            let result = this
                .korrid
                .update_setting(revision, setting_id.clone(), value)
                .await;
            this.lock().settings_busy = false;
            match result {
                SettingsUpdateOutcome::Err(failure) => {
                    this.settings_problem(&setting_id, &failure.message);
                    if failure.code == "SettingsConflict" {
                        this.load(false, None).await;
                    }
                }
                SettingsUpdateOutcome::Ok(settings) => {
                    this.lock().facts.settings = Some(settings);
                    this.publish_settings_status(SurfaceSettingsStatus::Idle);
                    // Plugin changes alter fulfillability.
                    this.load(false, None).await;
                }
            }
        });
    }

    pub fn dismiss_settings_problem(&self) {
        self.publish_settings_status(SurfaceSettingsStatus::Idle);
    }

    fn set_identity_status(&self, status: SurfaceIdentityStatus) {
        let mut inner = self.lock();
        if let Some(current) = &mut inner.identity_management {
            current.status = status;
        }
        drop(inner);
        self.changed();
    }

    fn identity_problem(&self, message: String) {
        self.set_identity_status(SurfaceIdentityStatus::Problem { message });
    }

    pub fn export_identity_backup(
        self: &Arc<Self>,
        password: String,
        retired_public_key: Option<String>,
    ) {
        self.set_identity_status(SurfaceIdentityStatus::Working {
            operation: "Encrypting backup".into(),
        });
        let this = Arc::clone(self);
        tokio::spawn(async move {
            match this
                .korrid
                .export_identity_backup(password, retired_public_key)
                .await
            {
                IdentityBackupOutcome::Err(failure) => this.identity_problem(failure.message),
                IdentityBackupOutcome::Ok(backup) => {
                    this.set_identity_status(SurfaceIdentityStatus::BackupReady {
                        encrypted_secret: backup.encrypted_secret,
                    })
                }
            }
        });
    }

    fn switched(&self, outcome: IdentitySwitchOutcome) {
        match outcome {
            IdentitySwitchOutcome::Err(failure) => self.identity_problem(failure.message),
            IdentitySwitchOutcome::Ok(done) => {
                self.set_identity_status(SurfaceIdentityStatus::Switched {
                    owner_public_key: done.owner_public_key,
                })
            }
        }
    }

    pub fn switch_identity_from_backup(
        self: &Arc<Self>,
        encrypted_secret: String,
        password: String,
        disposition: SurfaceIdentityDisposition,
        trust_loss_confirmed: bool,
    ) {
        self.set_identity_status(SurfaceIdentityStatus::Working {
            operation: "Switching identity".into(),
        });
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = this
                .korrid
                .switch_identity_from_backup(
                    encrypted_secret,
                    password,
                    data_disposition(disposition),
                    trust_loss_confirmed,
                )
                .await;
            this.switched(outcome);
        });
    }

    pub fn switch_identity_to_nip46(
        self: &Arc<Self>,
        bunker_uri: String,
        disposition: SurfaceIdentityDisposition,
        trust_loss_confirmed: bool,
    ) {
        self.set_identity_status(SurfaceIdentityStatus::Working {
            operation: "Connecting signer".into(),
        });
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = this
                .korrid
                .switch_identity_to_nip46(
                    bunker_uri,
                    data_disposition(disposition),
                    trust_loss_confirmed,
                )
                .await;
            this.switched(outcome);
        });
    }

    pub fn delete_retired_identity(self: &Arc<Self>, public_key: String, backup_confirmed: bool) {
        self.set_identity_status(SurfaceIdentityStatus::Working {
            operation: "Deleting retired key".into(),
        });
        let this = Arc::clone(self);
        tokio::spawn(async move {
            match this
                .korrid
                .delete_retired_identity(public_key, backup_confirmed)
                .await
            {
                IdentityRetiredDeleteOutcome::Err(failure) => {
                    this.identity_problem(failure.message)
                }
                IdentityRetiredDeleteOutcome::Ok(status) => {
                    this.lock().identity_management = Some(SurfaceIdentityManagement {
                        local_backup_available: status.local_backup_available,
                        retired_public_keys: status.retired_public_keys,
                        status: SurfaceIdentityStatus::Idle,
                    });
                    this.changed();
                }
            }
        });
    }

    pub fn dismiss_identity_status(&self) {
        let changed = {
            let mut inner = self.lock();
            match &mut inner.identity_management {
                Some(current) if current.status != SurfaceIdentityStatus::Idle => {
                    current.status = SurfaceIdentityStatus::Idle;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.changed();
        }
    }

    async fn cancel_owned_startup(self: &Arc<Self>, token: u64) {
        let launch_id = {
            let mut inner = self.lock();
            let Some(owned) = inner.startup.as_mut().filter(|owned| owned.token == token) else {
                return;
            };
            let Some(launch_id) = owned.launch_id.clone() else {
                return;
            };
            if owned.cancel_in_flight {
                return;
            }
            owned.cancel_in_flight = true;
            let next = inner.take_state().cancel_startup();
            inner.publish(next);
            launch_id
        };
        self.changed();
        let outcome = self.korrid.session_cancel(launch_id.clone()).await;
        {
            let mut inner = self.lock();
            let action_seq = inner.action_seq;
            let Some(owned) = inner.startup.as_mut().filter(|owned| owned.token == token) else {
                return;
            };
            if owned.operation != action_seq {
                return;
            }
            owned.cancel_in_flight = false;
            // A status read issued before the cancel ACK cannot resurrect it.
            inner.load_seq += 1;
        }
        match outcome {
            SessionEndOutcome::Ok(result) => {
                if matches!(result.phase, SessionEndPhase::Ended) {
                    let reload = {
                        let mut inner = self.lock();
                        match inner.take_state() {
                            LaunchablesState::Loaded(Loaded {
                                phase: Phase::Starting { .. },
                                content,
                            }) => {
                                inner.startup = None;
                                let entries = if content.entries.iter().any(|entry| {
                                    entry
                                        .session()
                                        .is_some_and(|session| session.launch_id == launch_id)
                                }) {
                                    without_sessions(&content.entries)
                                } else {
                                    content.entries
                                };
                                inner.publish(LaunchablesState::Loaded(Loaded {
                                    phase: Phase::Ready,
                                    content: Content {
                                        entries,
                                        ..Content::default()
                                    },
                                }));
                                true
                            }
                            _ => false,
                        }
                    };
                    if reload {
                        self.changed();
                        self.load(true, None).await;
                    }
                }
            }
            // A transport failure does not prove that the exact cancel failed.
            SessionEndOutcome::Err(failure) if failure.code == "BrainUnreachable" => {
                let next = self.state().startup_problem(&failure.message);
                self.publish(next);
            }
            // A stale exact cancel must only observe a replacement.
            SessionEndOutcome::Err(failure) if is_stale_code(&failure.code) => {
                self.load(true, None).await
            }
            SessionEndOutcome::Err(failure) => {
                let next = self.state().startup_problem(&failure.message);
                self.publish(next);
            }
        }
    }

    pub fn cancel_pending_launch(self: &Arc<Self>, launch_id: &str) {
        let operation = {
            let mut inner = self.lock();
            let Some(sent) = inner.state.begin_pending_cancellation(launch_id) else {
                return;
            };
            inner.load_seq += 1;
            inner.publish(sent);
            inner.action_seq
        };
        self.changed();
        let this = Arc::clone(self);
        let launch_id = launch_id.to_owned();
        tokio::spawn(async move {
            let outcome = this.korrid.session_cancel(launch_id.clone()).await;
            let reload = {
                let mut inner = this.lock();
                if operation != inner.action_seq {
                    return;
                }
                let still_owned = match &inner.state {
                    LaunchablesState::Loaded(Loaded {
                        phase: Phase::Choosing { choices },
                        ..
                    }) => choices.iter().any(|choice| {
                        choice.launch_id == launch_id
                            && choice.cancel == super::state::CancelState::Sent
                    }),
                    LaunchablesState::Loaded(Loaded {
                        phase:
                            Phase::Starting {
                                launch_id: Some(id),
                                cancelling: true,
                                ..
                            },
                        ..
                    }) => *id == launch_id,
                    _ => false,
                };
                if !still_owned {
                    return;
                }
                inner.load_seq += 1;
                let next = inner
                    .take_state()
                    .with_pending_cancellation_outcome(&launch_id, &outcome);
                inner.publish(next);
                match &outcome {
                    SessionEndOutcome::Ok(_) => true,
                    SessionEndOutcome::Err(failure) => is_stale_code(&failure.code),
                }
            };
            this.changed();
            if reload {
                this.load(true, None).await;
            }
        });
    }

    pub fn cancel_launch(self: &Arc<Self>) {
        let token = {
            let mut inner = self.lock();
            let retry_available = matches!(
                inner.state,
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Starting {
                        cancel_retry_available: Some(true),
                        ..
                    },
                    ..
                })
            );
            let starting = matches!(
                inner.state,
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Starting { .. },
                    ..
                })
            );
            let Some(owned) = inner.startup.as_mut() else {
                return;
            };
            if !starting || (owned.cancelled && !retry_available) || owned.cancel_in_flight {
                return;
            }
            owned.cancelled = true;
            let token = owned.token;
            let next = inner.take_state().cancel_startup();
            inner.publish(next);
            token
        };
        self.changed();
        // Reserve may still be in flight. Its continuation cancels the
        // returned server identity instead of ever dispatching start.
        let this = Arc::clone(self);
        tokio::spawn(async move { this.cancel_owned_startup(token).await });
    }

    /// Both local paths enter the same owned lifecycle before the first await.
    pub async fn start_catalog_launch(
        self: &Arc<Self>,
        game_id: &str,
        runner_id: Option<String>,
    ) -> Option<SessionStartResult> {
        let (current, game, token) = {
            let mut inner = self.lock();
            let current = inner.state.clone();
            let LaunchablesState::Loaded(Loaded {
                phase: Phase::Ready,
                content,
            }) = &current
            else {
                return None;
            };
            if content.unavailable_session_status.is_some() {
                return None;
            }
            // Capture the local source before any effect.
            let game = content
                .entries
                .iter()
                .flat_map(|entry| entry.catalog_games().cloned().collect::<Vec<_>>())
                .find(|game| game.id == game_id && game.source.is_local)?;
            inner.action_seq += 1;
            inner.next_token += 1;
            let token = inner.next_token;
            inner.startup = Some(Startup {
                token,
                operation: inner.action_seq,
                cancelled: false,
                launch_id: None,
                cancel_in_flight: false,
            });
            inner.load_seq += 1;
            let next = current.clone().begin_startup(&game);
            inner.publish(next);
            (current, game, token)
        };
        self.changed();
        let reservation = self.korrid.session_reserve(game_id.to_owned()).await;
        let reserved = match reservation {
            SessionReserveOutcome::Err(failure) => {
                let cancelled = {
                    let mut inner = self.lock();
                    if !inner.startup_owned(token) {
                        return None;
                    }
                    let cancelled = inner.startup.as_ref().is_some_and(|owned| owned.cancelled);
                    inner.startup = None;
                    let next = if cancelled {
                        let content = current
                            .loaded()
                            .map(|loaded| loaded.content.clone())
                            .unwrap_or_default();
                        LaunchablesState::Loaded(Loaded {
                            phase: Phase::Ready,
                            content: Content {
                                entries: content.entries,
                                notice: content.notice,
                                ..Content::default()
                            },
                        })
                    } else {
                        inner
                            .take_state()
                            .fail_startup(&format!("{}: {}", failure.code, failure.message), false)
                    };
                    inner.publish(next);
                    cancelled
                };
                self.changed();
                self.spawn_load(true);
                return if cancelled {
                    None
                } else {
                    Some(SelectedGameLaunchOutcome::Err(failure))
                };
            }
            SessionReserveOutcome::Ok(reserved) => reserved,
        };
        let cancelled = {
            let mut inner = self.lock();
            // Presentation retirement is not Cancel.
            let owned = inner
                .startup
                .as_mut()
                .filter(|owned| owned.token == token)?;
            owned.launch_id = Some(reserved.launch_id.clone());
            inner.load_seq += 1;
            let next = inner
                .take_state()
                .with_startup_reservation(&reserved, &game);
            inner.publish(next);
            // `publish` may have replaced the token for the new launch id:
            // carry the cancelled flag across.
            inner.startup.as_ref().is_some_and(|owned| owned.cancelled)
        };
        self.changed();
        let token = self
            .lock()
            .startup
            .as_ref()
            .map(|owned| owned.token)
            .unwrap_or(token);
        if cancelled {
            self.cancel_owned_startup(token).await;
            return None;
        }
        let (settings_ticket, launch) =
            (self.hooks.launch_overrides)(game_id, runner_id.as_deref());
        let outcome = self
            .korrid
            .session_start(
                game_id.to_owned(),
                reserved.launch_id.clone(),
                runner_id,
                launch,
            )
            .await;
        (self.hooks.launch_overrides_complete)(
            settings_ticket,
            matches!(&outcome, SelectedGameLaunchOutcome::Ok(_)),
        );
        let (owned_now, cancelled) = {
            let inner = self.lock();
            let owned = inner
                .startup
                .as_ref()
                .filter(|owned| owned.token == token && owned.operation == inner.action_seq);
            (owned.is_some(), owned.is_some_and(|owned| owned.cancelled))
        };
        if !owned_now {
            // Initial handoff can precede the start reply.
            let now = self.state();
            if let LaunchablesState::Loaded(Loaded {
                phase: Phase::Ready,
                content,
            }) = &now
            {
                if content.entries.iter().any(|entry| {
                    entry
                        .session()
                        .is_some_and(|session| session.launch_id == reserved.launch_id)
                }) {
                    if let SelectedGameLaunchOutcome::Err(failure) = &outcome {
                        let mut next = content.clone();
                        next.notice = Some(LaunchNotice::Launch {
                            message: format!("{}: {}", failure.code, failure.message),
                            subject: Some(launch_subject_for_game(&game, current.entries())),
                        });
                        self.publish(LaunchablesState::Loaded(Loaded {
                            phase: Phase::Ready,
                            content: next,
                        }));
                    }
                    return Some(outcome);
                }
            }
            return None;
        }
        self.lock().load_seq += 1;
        if cancelled {
            if let SelectedGameLaunchOutcome::Err(failure) = &outcome {
                if failure.code != "LaunchCancelled" && failure.code != "BrainUnreachable" {
                    let next = self.state().startup_problem(&failure.message);
                    self.publish(next);
                    self.spawn_load(true);
                }
            }
            return None;
        }
        if let SelectedGameLaunchOutcome::Err(failure) = &outcome {
            if failure.code == "BrainUnreachable" {
                // The request may have started. Never start again.
                let next = self.state().startup_problem(&failure.message);
                self.publish(next);
                self.spawn_load(true);
                return None;
            }
            {
                let mut inner = self.lock();
                inner.startup = None;
                let next = inner
                    .take_state()
                    .fail_startup(&format!("{}: {}", failure.code, failure.message), false);
                inner.publish(next);
            }
            self.changed();
            self.spawn_load(true);
            return Some(outcome);
        }
        // Do not clear Busy at ACK. Polling keeps running.
        self.spawn_load(true);
        Some(outcome)
    }

    /// Act on one entry: launch, resume, or prepare a stream.
    pub fn confirm_entry(self: &Arc<Self>, entry: Entry) {
        self.confirm(entry, None);
    }

    /// A streamed game with the runner the person chose on its holder, or
    /// none when the holder has one selected (issue #401).
    pub fn confirm_streamed(self: &Arc<Self>, entry: Entry, runner_id: Option<String>) {
        self.confirm(entry, runner_id);
    }

    fn confirm(self: &Arc<Self>, entry: Entry, runner_id: Option<String>) {
        let current = self.state();
        let LaunchablesState::Loaded(Loaded {
            phase: Phase::Ready,
            content,
        }) = &current
        else {
            return;
        };
        // A retained caller selection is not authority to launch a removed game.
        let key = entry.key();
        if !content.entries.iter().any(|candidate| {
            candidate.key() == key
                || candidate
                    .alternatives()
                    .iter()
                    .any(|copy| Entry::for_copy(copy).key() == key)
        }) {
            return;
        }
        // A retained catalog selection must resume its merged exact session,
        // including when the selected copy is remote or an alternative.
        if !entry.is_now_playing() {
            if let Some(active) = content.entries.iter().find(|candidate| {
                candidate.is_now_playing()
                    && (candidate.key() == key
                        || candidate
                            .alternatives()
                            .iter()
                            .any(|copy| Entry::for_copy(copy).key() == key))
            }) {
                self.confirm_entry(active.clone());
                return;
            }
        }
        // Last-known identity permits exact Return/End, never replacement work.
        if content.unavailable_session_status.is_some() && !entry.is_now_playing() {
            return;
        }
        if let Entry::Game { game, .. } = &entry {
            if game.source.is_local {
                let this = Arc::clone(self);
                let id = game.id.clone();
                tokio::spawn(async move {
                    this.start_catalog_launch(&id, None).await;
                });
                return;
            }
        }
        let operation = {
            let mut inner = self.lock();
            inner.action_seq += 1;
            inner.action_seq
        };
        match entry {
            Entry::NowPlaying { session, .. } => {
                // Thaw names the exact launch.
                let resuming = LaunchablesState::begin_return(content.clone(), &session);
                self.publish(resuming.clone());
                let this = Arc::clone(self);
                let notice = content.notice.clone();
                let unavailable = content.unavailable_session_status.clone();
                tokio::spawn(async move {
                    let outcome = this.korrid.session_resume(session.launch_id.clone()).await;
                    if operation != this.lock().action_seq {
                        return;
                    }
                    match outcome {
                        crate::SessionTransitionOutcome::Ok(_) => {
                            let entries = this.state().entries().to_vec();
                            this.publish(LaunchablesState::Loaded(Loaded {
                                phase: Phase::Ready,
                                content: Content {
                                    entries,
                                    notice,
                                    unavailable_session_status: unavailable,
                                    ..Content::default()
                                },
                            }));
                            this.load(true, None).await;
                        }
                        crate::SessionTransitionOutcome::Err(failure) => {
                            this.publish(resuming.with_resume_outcome(Some(&failure)));
                        }
                    }
                });
            }
            Entry::LocalGame { .. } => {
                self.notice_on_ready(operation, "Korri cannot start this entry on this device.");
            }
            Entry::Game { game, .. } => {
                let subject = launch_subject_for_game(&game, &content.entries);
                let preparing = current.clone().begin_preparing(&game.title, Some(subject));
                self.publish(preparing.clone());
                let this = Arc::clone(self);
                tokio::spawn(async move {
                    this.prepare(operation, preparing, game, runner_id).await;
                });
            }
        }
    }

    async fn prepare(
        self: &Arc<Self>,
        operation: u64,
        preparing: LaunchablesState,
        game: Game,
        runner_id: Option<String>,
    ) {
        // The changes kept for this launch travel in its request (ADR 0066).
        let holder = game.source.label.clone();
        let (settings_ticket, launch) = (self.hooks.launch_overrides)(
            &crate::surface::one_launch::game_key(&game.id, Some(&holder)),
            runner_id.as_deref(),
        );
        let outcome = match runner_id {
            Some(runner_id) => {
                self.korrid
                    .selected_game_launch(game.id.clone(), runner_id, Some(holder), launch)
                    .await
            }
            None => {
                self.korrid
                    .session_prepare(game.id.clone(), game.host.clone(), launch)
                    .await
            }
        };
        (self.hooks.launch_overrides_complete)(
            settings_ticket,
            matches!(&outcome, SelectedGameLaunchOutcome::Ok(_)),
        );
        if operation != self.lock().action_seq {
            if matches!(outcome, SelectedGameLaunchOutcome::Ok(_)) {
                self.load(true, None).await;
            }
            return;
        }
        // Reads started before the ACK must not erase the newly known launch.
        let load_operation = {
            let mut inner = self.lock();
            inner.load_seq += 1;
            inner.publish(preparing.with_local_catalog_prepare_outcome(&outcome, &game));
            inner.load_seq
        };
        self.changed();
        match outcome {
            SelectedGameLaunchOutcome::Ok(payload) => {
                if !payload.warnings.is_empty() {
                    (self.hooks.launch_warnings)(game.title.clone(), payload.warnings);
                }
            }
            SelectedGameLaunchOutcome::Err(_) => {
                // An older prepare may have succeeded while this was locked.
                let status = self.session_status_with_timeout().await;
                let mut inner = self.lock();
                if operation != inner.action_seq || load_operation != inner.load_seq {
                    return;
                }
                let next = inner.take_state().with_session_status(&status);
                inner.publish(next);
                drop(inner);
                self.changed();
            }
        }
    }

    /// Ask the host to stop the running session and wait for it to be gone.
    pub fn end_session(self: &Arc<Self>, entry: Entry) {
        let (operation, stop_requested, launch_id) = {
            let mut inner = self.lock();
            if !inner.state.is_ready() {
                return;
            }
            let Entry::NowPlaying { session, .. } = &entry else {
                return;
            };
            inner.action_seq += 1;
            let stop_requested = inner.take_state().begin_stopping(&entry);
            inner.publish(stop_requested.clone());
            (inner.action_seq, stop_requested, session.launch_id.clone())
        };
        self.changed();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = this.korrid.session_end(launch_id).await;
            if operation != this.lock().action_seq {
                return;
            }
            let stopping = stop_requested.with_stop_outcome(&outcome);
            this.publish(stopping.clone());
            if !matches!(outcome, SessionEndOutcome::Ok(_)) {
                return;
            }
            let poll_seq = {
                let mut inner = this.lock();
                inner.stop_poll_seq += 1;
                inner.stop_poll_seq
            };
            let deadline = Instant::now() + STOP_POLL_DEADLINE;
            let current = |this: &Self| {
                let inner = this.lock();
                operation == inner.action_seq && poll_seq == inner.stop_poll_seq
            };
            while current(&this) && Instant::now() < deadline {
                let status = this.session_status_with_timeout().await;
                if !current(&this) {
                    return;
                }
                if is_authoritative_session_status(&status) {
                    let after = stopping.clone().with_status_after_stop(&status);
                    if after.is_ready() {
                        this.publish(after);
                        this.load(false, None).await;
                        return;
                    }
                }
                let failure_code = status_failure(&status).map(|failure| failure.code.clone());
                if !is_authoritative_session_status(&status)
                    && failure_code.as_deref() != Some("StatusTimeout")
                {
                    let next = this.state().with_status_after_stop(&status);
                    this.publish(next);
                    return;
                }
                tokio::time::sleep(STOP_POLL_INTERVAL).await;
            }
            if !current(&this) {
                return;
            }
            let next = this.state().stop_timed_out();
            this.publish(next);
        });
    }

    /// Clear the current notice without re-reading anything.
    pub fn dismiss_notice(&self) {
        (self.hooks.dismiss_network_problem)();
        let mut inner = self.lock();
        let LaunchablesState::Loaded(Loaded {
            phase: Phase::Ready,
            content,
        }) = inner.take_state()
        else {
            return;
        };
        let mut next = content.clone();
        if let Some(ended) = &content.stream_play_ended {
            inner.dismissed_stream_end = Some(ended.launch_id.clone());
            next.stream_play_ended = None;
            next.notice = None;
        } else if content.notice.is_some() {
            next.notice = None;
        } else {
            return;
        }
        inner.publish(LaunchablesState::Loaded(Loaded {
            phase: Phase::Ready,
            content: next,
        }));
        drop(inner);
        self.changed();
    }

    /// Carry an overlay's terminal failure through the catalog's launch notice.
    pub fn session_failed(&self, message: String) {
        let mut inner = self.lock();
        let next = inner
            .take_state()
            .with_session_status(&SessionStatusOutcome::Err(RpcFailure {
                code: "HostLaunchFailed".into(),
                message,
            }));
        // A newer preparation owns its lock; do not invalidate its reply.
        if !next.is_ready() {
            return;
        }
        // Older resume or catalog replies must not erase the watch's notice.
        inner.action_seq += 1;
        inner.load_seq += 1;
        inner.publish(next);
        drop(inner);
        self.changed();
    }

    pub fn resume_catalog_session(self: &Arc<Self>, session: ActiveSession) {
        let current = self.state();
        let Some(loaded) = current.loaded().filter(|_| current.is_ready()) else {
            return;
        };
        if loaded.content.unavailable_session_status.is_some() {
            return;
        }
        let observed = current
            .clone()
            .with_session_status(&SessionStatusOutcome::Ok(SessionStatus {
                netplay: None,
                stream_play_launch_id: None,
                stream_play_ended: None,
                pending_launches: None,
                observation_failure: None,
                active: Some(session.clone()),
                overlay: None,
            }));
        let entry = observed
            .entries()
            .iter()
            .find(|entry| {
                entry
                    .session()
                    .is_some_and(|active| active.launch_id == session.launch_id)
            })
            .cloned();
        self.publish(observed);
        if let Some(entry) = entry {
            self.confirm_entry(entry);
        }
    }

    /// Re-read every source.
    pub fn reload(self: &Arc<Self>) {
        self.spawn_load(false);
    }

    /// Re-read the catalog in place, without showing it as loading.
    pub async fn refresh(self: &Arc<Self>) {
        self.load(true, None).await;
    }
}

fn data_disposition(disposition: SurfaceIdentityDisposition) -> IdentityDataDisposition {
    match disposition {
        SurfaceIdentityDisposition::Transfer => IdentityDataDisposition::Transfer,
        SurfaceIdentityDisposition::Delete => IdentityDataDisposition::Delete,
    }
}
